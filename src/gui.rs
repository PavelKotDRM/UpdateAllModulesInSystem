//! Графический интерфейс приложения на базе `egui`/`eframe`.

mod logs;
mod rendering;
mod state;

use crate::app::{
    ModuleUpdateProgress, SelectionFilter, UpdateCancellation, discover_modules,
    run_updates_with_progress_cancellable,
};
use crate::model::ModuleSnapshot;
use crate::repaint::{self, RepaintSender};
use crate::system;
use eframe::{App, Frame, egui};
use state::{load_gui_state, save_gui_state};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;

#[derive(Debug)]
/// События, которыми фоновые задачи уведомляют GUI.
pub enum GuiEvent {
    /// Строка лога от процесса сканирования/обновления.
    Log(String),
    /// Обновление фазы конкретного модуля.
    ModuleProgress(ModuleUpdateProgress),
    /// Завершение сканирования с итоговым списком модулей.
    ScanFinished(Vec<ModuleSnapshot>),
    /// Завершение процесса обновления с итоговым сообщением.
    UpdateFinished(String),
    /// Результат попытки перезапуска с повышенными правами.
    ElevationFinished(Result<(), String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum GuiTab {
    #[default]
    Overview,
    Modules,
    Logs,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModuleSelectionState {
    None,
    Partial,
    All,
}

fn module_selection_state(selected_count: usize, total_count: usize) -> ModuleSelectionState {
    match (selected_count, total_count) {
        (0, _) => ModuleSelectionState::None,
        (selected, total) if selected == total => ModuleSelectionState::All,
        _ => ModuleSelectionState::Partial,
    }
}

/// Корневое состояние и контроллер GUI-приложения.
pub struct GuiApp {
    filter: SelectionFilter,
    events_tx: RepaintSender<GuiEvent>,
    events_rx: Receiver<GuiEvent>,
    modules: Vec<ModuleSnapshot>,
    logs: BTreeMap<String, Vec<String>>,
    busy: bool,
    auto_yes: bool,
    status_line: String,
    started_scan: bool,
    persisted_selection: BTreeSet<String>,
    persisted_update_selection: BTreeMap<String, BTreeSet<String>>,
    active_tab: GuiTab,
    show_not_found: bool,
    module_progress: BTreeMap<String, ModuleUpdateProgress>,
    update_cancellation: Option<UpdateCancellation>,
    elevation_pending: bool,
    close_after_elevation: bool,
}

impl GuiApp {
    /// Создаёт GUI-приложение, восстанавливает сохранённое состояние и запускает
    /// первичное сканирование модулей.
    pub fn new(filter: SelectionFilter, auto_yes: bool, context: &egui::Context) -> Self {
        let (events_tx, events_rx) = repaint::channel(context);
        let persisted_state = load_gui_state();
        let persisted_selection: BTreeSet<String> =
            persisted_state.selected_modules.iter().cloned().collect();
        let persisted_update_selection: BTreeMap<String, BTreeSet<String>> = persisted_state
            .selected_updates
            .into_iter()
            .map(|(module, updates)| (module, updates.into_iter().collect()))
            .collect();
        let auto_yes = persisted_state.auto_yes.unwrap_or(auto_yes);
        let show_not_found = persisted_state.show_not_found.unwrap_or(false);
        let mut app = Self {
            filter,
            events_tx,
            events_rx,
            modules: Vec::new(),
            logs: BTreeMap::new(),
            busy: false,
            auto_yes,
            status_line: String::from("Ожидание запуска"),
            started_scan: false,
            persisted_selection,
            persisted_update_selection,
            active_tab: GuiTab::Overview,
            show_not_found,
            module_progress: BTreeMap::new(),
            update_cancellation: None,
            elevation_pending: false,
            close_after_elevation: false,
        };
        app.start_scan();
        app
    }

    fn start_scan(&mut self) {
        if self.started_scan {
            return;
        }
        self.started_scan = true;
        self.busy = true;
        self.status_line = String::from("Сканирование...");
        let filter = self.filter.clone();
        let sender = self.events_tx.clone();
        thread::spawn(move || {
            let modules = discover_modules(&filter);
            let _ = sender.send(GuiEvent::ScanFinished(modules));
        });
    }

    fn start_update(&mut self) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.status_line = String::from("Обновление...");
        let modules = self.modules.clone();
        self.module_progress.clear();
        let sender = self.events_tx.clone();
        let force_yes = self.auto_yes;
        let cancellation = UpdateCancellation::default();
        self.update_cancellation = Some(cancellation.clone());
        thread::spawn(move || {
            let (log_tx, log_rx) = mpsc::channel::<String>();
            let (progress_tx, progress_rx) = mpsc::channel::<ModuleUpdateProgress>();
            let log_sender = sender.clone();
            thread::spawn(move || {
                while let Ok(message) = log_rx.recv() {
                    let _ = log_sender.send(GuiEvent::Log(message));
                }
            });

            let progress_sender = sender.clone();
            thread::spawn(move || {
                while let Ok(progress) = progress_rx.recv() {
                    let _ = progress_sender.send(GuiEvent::ModuleProgress(progress));
                }
            });

            let results = run_updates_with_progress_cancellable(
                &modules,
                force_yes,
                &log_tx,
                Some(progress_tx),
                cancellation.clone(),
            );
            let summary = if cancellation.is_cancelled() {
                String::from("Обновление отменено")
            } else if results.is_empty() {
                String::from("Нет выбранных модулей с доступными обновлениями")
            } else if results.iter().all(|(_, result)| result.is_ok()) {
                String::from("Обновление успешно завершено")
            } else {
                String::from("Обновление завершено с ошибками")
            };
            let _ = sender.send(GuiEvent::UpdateFinished(summary));
        });
    }

    fn cancel_update(&mut self) {
        let Some(cancellation) = &self.update_cancellation else {
            return;
        };
        if cancellation.is_cancelled() {
            return;
        }

        cancellation.cancel();
        self.status_line =
            String::from("Отмена запрошена: ожидается завершение активных модулей...");
        self.append_log(String::from(
            "[system] Отмена запрошена; новые модули запускаться не будут",
        ));
    }

    fn start_update_all(&mut self) {
        if self.busy {
            return;
        }

        for module in &mut self.modules {
            module.selected = true;
            for update in &mut module.updates {
                update.selected = true;
            }
        }
        self.persist_state();
        self.start_update();
    }

    fn start_elevated(&mut self) {
        if self.elevation_pending {
            return;
        }

        self.elevation_pending = true;
        self.status_line = String::from("Запрос повышенных прав...");
        let sender = self.events_tx.clone();
        thread::spawn(move || {
            let result = system::restart_elevated().map_err(|error| error.to_string());
            let _ = sender.send(GuiEvent::ElevationFinished(result));
        });
    }

    fn select_only_updates(&mut self) {
        for module in &mut self.modules {
            module.selected = module.installed && module.status.has_updates();
        }
        self.persist_state();
    }

    fn invert_selection(&mut self) {
        for module in &mut self.modules {
            module.selected = !module.selected;
        }
        self.persist_state();
    }

    fn selected_count(&self) -> usize {
        self.modules
            .iter()
            .filter(|module| self.is_module_visible(module) && module.selected)
            .count()
    }

    fn updates_count(&self) -> usize {
        self.modules
            .iter()
            .filter(|module| {
                self.is_module_visible(module) && module.installed && module.status.has_updates()
            })
            .count()
    }

    fn apply_persisted_selection(&mut self) {
        if self.persisted_selection.is_empty() {
            for module in &mut self.modules {
                if let Some(saved_updates) = self.persisted_update_selection.get(&module.name) {
                    for update in &mut module.updates {
                        update.selected = saved_updates.contains(&update.selection_key());
                    }
                }
            }
            return;
        }

        for module in &mut self.modules {
            module.selected = self.persisted_selection.contains(&module.name);
            if let Some(saved_updates) = self.persisted_update_selection.get(&module.name) {
                for update in &mut module.updates {
                    update.selected = saved_updates.contains(&update.selection_key());
                }
            }
        }
    }

    fn is_module_visible(&self, module: &ModuleSnapshot) -> bool {
        self.show_not_found || module.installed
    }

    fn visible_modules_count(&self) -> usize {
        self.modules
            .iter()
            .filter(|module| self.is_module_visible(module))
            .count()
    }

    fn show_selection_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Выбор модулей", |ui| {
            if ui.button("Отметить все видимые").clicked() {
                for module in &mut self.modules {
                    if self.show_not_found || module.installed {
                        module.selected = true;
                        for update in &mut module.updates {
                            update.selected = true;
                        }
                    }
                }
                self.persist_state();
                ui.close();
            }

            if ui.button("Снять выбор с видимых").clicked() {
                for module in &mut self.modules {
                    if self.show_not_found || module.installed {
                        module.selected = false;
                        for update in &mut module.updates {
                            update.selected = false;
                        }
                    }
                }
                self.persist_state();
                ui.close();
            }

            if ui.button("Инвертировать выбор").clicked() {
                self.invert_selection();
                ui.close();
            }

            if ui.button("Выбрать только с обновлениями").clicked() {
                self.select_only_updates();
                ui.close();
            }

            ui.separator();

            let mut selection_changed = false;
            for module in &mut self.modules {
                if !self.show_not_found && !module.installed {
                    continue;
                }
                let label = format!("{} ({})", module.name, module.status_label());
                if ui.checkbox(&mut module.selected, label).changed() {
                    selection_changed = true;
                }
            }

            if selection_changed {
                self.persist_state();
            }
        });
    }

    fn persist_state(&mut self) {
        self.persisted_selection = self
            .modules
            .iter()
            .filter(|module| module.selected)
            .map(|module| module.name.clone())
            .collect();

        self.persisted_update_selection = self
            .modules
            .iter()
            .map(|module| {
                let selected_updates = module
                    .updates
                    .iter()
                    .filter(|update| update.selected)
                    .map(|update| update.selection_key())
                    .collect::<BTreeSet<_>>();
                (module.name.clone(), selected_updates)
            })
            .collect();

        if let Err(error) = save_gui_state(
            &self.persisted_selection,
            &self.persisted_update_selection,
            self.auto_yes,
            self.show_not_found,
        ) {
            self.append_log(format!(
                "[system] Не удалось сохранить состояние GUI: {error}"
            ));
        }
    }

    fn process_events(&mut self) {
        while let Ok(event) = self.events_rx.try_recv() {
            match event {
                GuiEvent::Log(message) => self.append_log(message),
                GuiEvent::ModuleProgress(progress) => {
                    self.module_progress
                        .insert(progress.module_name.clone(), progress);
                }
                GuiEvent::ScanFinished(modules) => {
                    self.modules = modules;
                    self.module_progress.clear();
                    self.apply_persisted_selection();
                    self.busy = false;
                    self.status_line = String::from("Сканирование завершено");
                }
                GuiEvent::UpdateFinished(message) => {
                    self.busy = false;
                    self.update_cancellation = None;
                    self.append_log(format!("[summary] {message}"));
                    self.status_line = message;
                    self.started_scan = false;
                    self.start_scan();
                }
                GuiEvent::ElevationFinished(Ok(())) => {
                    self.elevation_pending = false;
                    self.close_after_elevation = true;
                    self.status_line = String::from("Запущено с повышенными правами");
                }
                GuiEvent::ElevationFinished(Err(error)) => {
                    self.elevation_pending = false;
                    let message = format!("Не удалось повысить права: {error}");
                    self.status_line = message.clone();
                    self.append_log(format!("[system] Ошибка: {message}"));
                }
            }
        }
    }

    fn show_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!self.busy, egui::Button::new("Проверить обновления"))
                .clicked()
            {
                self.started_scan = false;
                self.start_scan();
            }

            if ui
                .add_enabled(!self.busy, egui::Button::new("Обновить выбранное"))
                .clicked()
            {
                self.start_update();
            }

            if ui
                .add_enabled(!self.busy, egui::Button::new("Обновить все"))
                .clicked()
            {
                self.start_update_all();
            }

            let can_cancel = self
                .update_cancellation
                .as_ref()
                .is_some_and(|cancellation| !cancellation.is_cancelled());
            if can_cancel && ui.button("Отменить").clicked() {
                self.cancel_update();
            }

            if !system::is_admin() {
                let elevate_response = ui
                    .add_enabled(!self.elevation_pending, egui::Button::new("Повысить права"))
                    .on_hover_text("Перезапустить приложение с правами администратора");
                if elevate_response.clicked() {
                    self.start_elevated();
                }
            }

            ui.add_enabled_ui(!self.busy, |ui| {
                self.show_selection_menu(ui);
            });

            let auto_yes_response = ui.checkbox(&mut self.auto_yes, "Автоматическое согласие (-y)");
            if auto_yes_response.changed() {
                self.persist_state();
            }
        });

        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.active_tab, GuiTab::Overview, "Обзор");
            ui.selectable_value(&mut self.active_tab, GuiTab::Modules, "Модули");
            ui.selectable_value(&mut self.active_tab, GuiTab::Logs, "Логи");
            ui.selectable_value(&mut self.active_tab, GuiTab::Settings, "Настройки");
        });

        ui.separator();
        ui.label(&self.status_line);
        ui.horizontal_wrapped(|ui| {
            ui.label(format!(
                "Выбрано модулей: {}/{}",
                self.selected_count(),
                self.visible_modules_count()
            ));
            ui.separator();
            ui.label(format!(
                "С обновлениями: {}/{}",
                self.updates_count(),
                self.visible_modules_count()
            ));
        });
    }

    fn show_overview_tab(&self, ui: &mut egui::Ui) {
        ui.heading("Сводка");
        ui.add_space(4.0);
        if let Some(warning) = self.elevation_warning_text() {
            ui.colored_label(egui::Color32::YELLOW, warning);
            ui.add_space(4.0);
        }

        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Найдено модулей: {}", self.visible_modules_count()));
            ui.separator();
            ui.label(format!("Выбрано: {}", self.selected_count()));
            ui.separator();
            ui.label(format!("Требуют обновления: {}", self.updates_count()));
            ui.separator();
            ui.label(format!("Логов накоплено: {}", self.log_count()));
        });

        ui.add_space(8.0);
        ui.label("Используйте вкладку 'Модули' для выбора пакетов, 'Логи' для просмотра прогресса и ошибок.");
    }

    fn show_modules_tab(&mut self, ui: &mut egui::Ui) {
        if let Some(warning) = self.elevation_warning_text() {
            ui.colored_label(egui::Color32::YELLOW, warning);
            ui.add_space(6.0);
        }

        egui::ScrollArea::vertical()
            .id_salt("modules_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.modules.is_empty() {
                    ui.label("Список модулей пуст. Нажмите 'Проверить обновления'.");
                    return;
                }

                let available_width = ui.available_width();
                let columns = ((available_width / 420.0).floor() as usize).clamp(1, 3);
                let mut selection_changed = false;
                let visible_count = self.visible_modules_count();
                let module_progress = &self.module_progress;

                if visible_count == 0 {
                    ui.label("Нет отображаемых модулей. Включите показ не найденных в настройках или выполните сканирование.");
                    return;
                }

                if columns == 1 {
                    for module in &mut self.modules {
                        if !self.show_not_found && !module.installed {
                            continue;
                        }
                        let progress = module_progress.get(&module.name);
                        Self::render_module_card(ui, module, progress, &mut selection_changed);
                    }
                } else {
                    ui.columns(columns, |columns_ui| {
                        let mut visible_index = 0usize;
                        for module in &mut self.modules {
                            if !self.show_not_found && !module.installed {
                                continue;
                            }
                            let column = &mut columns_ui[visible_index % columns];
                            let progress = module_progress.get(&module.name);
                            Self::render_module_card(column, module, progress, &mut selection_changed);
                            visible_index += 1;
                        }
                    });
                }

                if selection_changed {
                    self.persist_state();
                }
            });
    }

    fn show_logs_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Console Log");
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Экспорт логов").clicked() {
                self.export_logs();
            }
            ui.label(format!("Модулей: {}", self.logs.len()));
            ui.separator();
            ui.label(format!("Записей: {}", self.log_count()));
        });
        ui.add_space(4.0);

        egui::ScrollArea::vertical()
            .id_salt("logs_scroll")
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if self.logs.is_empty() {
                    ui.label("Логи пока пусты");
                } else {
                    for (module, lines) in &self.logs {
                        egui::CollapsingHeader::new(format!("{module} ({})", lines.len()))
                            .default_open(true)
                            .show(ui, |ui| {
                                for line in lines {
                                    ui.monospace(line);
                                }
                            });
                    }
                }
            });
    }

    fn show_settings_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Настройки");
        ui.add_space(4.0);

        let auto_yes_response = ui.checkbox(
            &mut self.auto_yes,
            "Автоматическое согласие на обновление (-y)",
        );
        if auto_yes_response.changed() {
            self.persist_state();
        }

        let show_not_found_response =
            ui.checkbox(&mut self.show_not_found, "Показывать не найденные модули");
        if show_not_found_response.changed() {
            self.persist_state();
        }

        ui.add_space(8.0);
        ui.label(
            "Состояние выбранных модулей и настройки отображения сохраняются между запусками.",
        );

        ui.add_space(8.0);
        ui.separator();
        ui.small(format!("Версия: {}", crate::build_info::VERSION));
        ui.small(format!("Коммит: {}", crate::build_info::GIT_HASH));
        ui.small(format!(
            "Описание сборки: {}",
            crate::build_info::DESCRIPTION
        ));

        egui::CollapsingHeader::new("Подробная информация о сборке")
            .default_open(false)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .show(ui, |ui| {
                        for (section, entries) in crate::build_info::DETAILS {
                            ui.strong(*section);
                            for (label, value) in *entries {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(format!("{label}:"));
                                    ui.monospace(*value);
                                });
                            }
                            ui.add_space(6.0);
                        }
                    });
            });
    }

    fn elevation_warning_text(&self) -> Option<&'static str> {
        if system::should_warn_about_elevation() {
            Some("Запуск без прав администратора: системные менеджеры могут быть недоступны")
        } else {
            None
        }
    }
}

impl App for GuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut Frame) {
        self.process_events();

        if self.close_after_elevation {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }

        egui::Panel::top("toolbar").show(ui, |ui| {
            self.show_toolbar(ui);
        });

        egui::CentralPanel::default().show(ui, |ui| match self.active_tab {
            GuiTab::Overview => self.show_overview_tab(ui),
            GuiTab::Modules => self.show_modules_tab(ui),
            GuiTab::Logs => self.show_logs_tab(ui),
            GuiTab::Settings => self.show_settings_tab(ui),
        });
    }
}

/// Запускает нативное GUI-приложение.
///
/// # Errors
/// Возвращает ошибку, если запуск `eframe` не удался.
pub fn launch_gui(
    filter: SelectionFilter,
    auto_yes: bool,
    elevation_ready_file: Option<PathBuf>,
) -> anyhow::Result<()> {
    system::hide_windows_console_if_needed(true);
    let native_options = repaint::stable_native_options();
    eframe::run_native(
        "UpdateAllModules",
        native_options,
        Box::new(move |creation_context| {
            let app = GuiApp::new(filter, auto_yes, &creation_context.egui_ctx);
            if let Some(path) = &elevation_ready_file {
                fs::write(path, "ready")?;
            }
            Ok(Box::new(app))
        }),
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_selection_state_distinguishes_none_partial_and_all() {
        assert_eq!(module_selection_state(0, 3), ModuleSelectionState::None);
        assert_eq!(module_selection_state(1, 3), ModuleSelectionState::Partial);
        assert_eq!(module_selection_state(3, 3), ModuleSelectionState::All);
    }

    #[test]
    fn split_module_log_extracts_module_and_uses_system_fallback() {
        assert_eq!(
            logs::split_module_log("[npm] [stdout] updated 2 packages"),
            ("npm".to_owned(), "[stdout] updated 2 packages".to_owned())
        );
        assert_eq!(
            logs::split_module_log("Обновление завершено"),
            ("system".to_owned(), "Обновление завершено".to_owned())
        );
    }
}
