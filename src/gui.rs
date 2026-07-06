use crate::app::{discover_modules, run_updates, SelectionFilter};
use crate::model::ModuleSnapshot;
use crate::system;
use eframe::{egui, App, Frame, NativeOptions};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

const GUI_STATE_FILE: &str = ".update_all_modules_gui_state.json";

#[derive(Debug, Default, Serialize, Deserialize)]
struct GuiState {
    selected_modules: Vec<String>,
    auto_yes: Option<bool>,
    show_not_found: Option<bool>,
    selected_updates: BTreeMap<String, Vec<String>>,
}

#[derive(Debug)]
pub enum GuiEvent {
    Log(String),
    ScanFinished(Vec<ModuleSnapshot>),
    UpdateFinished(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum GuiTab {
    #[default]
    Overview,
    Modules,
    Logs,
    Settings,
}

pub struct GuiApp {
    filter: SelectionFilter,
    events_tx: Sender<GuiEvent>,
    events_rx: Receiver<GuiEvent>,
    modules: Vec<ModuleSnapshot>,
    logs: Vec<String>,
    busy: bool,
    auto_yes: bool,
    status_line: String,
    started_scan: bool,
    persisted_selection: BTreeSet<String>,
    persisted_update_selection: BTreeMap<String, BTreeSet<String>>,
    active_tab: GuiTab,
    show_not_found: bool,
}

impl GuiApp {
    pub fn new(filter: SelectionFilter, auto_yes: bool) -> Self {
        let (events_tx, events_rx) = mpsc::channel();
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
            logs: Vec::new(),
            busy: false,
            auto_yes,
            status_line: String::from("Ожидание запуска"),
            started_scan: false,
            persisted_selection,
            persisted_update_selection,
            active_tab: GuiTab::Overview,
            show_not_found,
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
        let sender = self.events_tx.clone();
        let force_yes = self.auto_yes;
        thread::spawn(move || {
            let (log_tx, log_rx) = mpsc::channel::<String>();
            let log_sender = sender.clone();
            thread::spawn(move || {
                while let Ok(message) = log_rx.recv() {
                    let _ = log_sender.send(GuiEvent::Log(message));
                }
            });

            let results = run_updates(&modules, force_yes, &log_tx);
            let summary = if results.is_empty() {
                String::from("Нет выбранных модулей с доступными обновлениями")
            } else if results.iter().all(|(_, result)| result.is_ok()) {
                String::from("Обновление успешно завершено")
            } else {
                String::from("Обновление завершено с ошибками")
            };
            let _ = sender.send(GuiEvent::UpdateFinished(summary));
        });
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
                        update.selected = saved_updates.contains(&update.name);
                    }
                }
            }
            return;
        }

        for module in &mut self.modules {
            module.selected = self.persisted_selection.contains(&module.name);
            if let Some(saved_updates) = self.persisted_update_selection.get(&module.name) {
                for update in &mut module.updates {
                    update.selected = saved_updates.contains(&update.name);
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
                    .map(|update| update.name.clone())
                    .collect::<BTreeSet<_>>();
                (module.name.clone(), selected_updates)
            })
            .collect();

        if let Err(error) =
            save_gui_state(
                &self.persisted_selection,
                &self.persisted_update_selection,
                self.auto_yes,
                self.show_not_found,
            )
        {
            self.logs
                .push(format!("Не удалось сохранить состояние GUI: {error}"));
        }
    }

    fn process_events(&mut self) {
        while let Ok(event) = self.events_rx.try_recv() {
            match event {
                GuiEvent::Log(message) => self.logs.push(message),
                GuiEvent::ScanFinished(modules) => {
                    self.modules = modules;
                    self.apply_persisted_selection();
                    self.busy = false;
                    self.status_line = String::from("Сканирование завершено");
                }
                GuiEvent::UpdateFinished(message) => {
                    self.busy = false;
                    self.logs.push(format!("[summary] {message}"));
                    self.status_line = message;
                    self.started_scan = false;
                    self.start_scan();
                }
            }
        }
    }

    fn export_logs(&mut self) {
        match export_logs_to_file(&self.logs) {
            Ok(path) => {
                let message = format!("Логи экспортированы: {}", path.display());
                self.status_line = message.clone();
                self.logs.push(format!("[info] {message}"));
            }
            Err(error) => {
                let message = format!("Не удалось экспортировать логи: {error}");
                self.status_line = message.clone();
                self.logs.push(format!("[error] {message}"));
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

            ui.add_enabled_ui(!self.busy, |ui| {
                self.show_selection_menu(ui);
            });

            let auto_yes_response =
                ui.checkbox(&mut self.auto_yes, "Автоматическое согласие (-y)");
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
        if let Some(warning) = if system::should_warn_about_elevation() {
            Some("Запуск без прав администратора: системные менеджеры могут быть недоступны")
        } else {
            None
        } {
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
            ui.label(format!("Логов накоплено: {}", self.logs.len()));
        });

        ui.add_space(8.0);
        ui.label("Используйте вкладку 'Модули' для выбора пакетов, 'Логи' для просмотра прогресса и ошибок.");
    }

    fn show_modules_tab(&mut self, ui: &mut egui::Ui) {
        if let Some(warning) = if system::should_warn_about_elevation() {
            Some("Запуск без прав администратора: системные менеджеры могут быть недоступны")
        } else {
            None
        } {
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

                if visible_count == 0 {
                    ui.label("Нет отображаемых модулей. Включите показ не найденных в настройках или выполните сканирование.");
                    return;
                }

                if columns == 1 {
                    for module in &mut self.modules {
                        if !self.show_not_found && !module.installed {
                            continue;
                        }
                        Self::render_module_card(ui, module, &mut selection_changed);
                    }
                } else {
                    ui.columns(columns, |columns_ui| {
                        let mut visible_index = 0usize;
                        for module in &mut self.modules {
                            if !self.show_not_found && !module.installed {
                                continue;
                            }
                            let column = &mut columns_ui[visible_index % columns];
                            Self::render_module_card(column, module, &mut selection_changed);
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
            ui.label(format!("Записей: {}", self.logs.len()));
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
                    for line in &self.logs {
                        ui.monospace(line);
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

        let show_not_found_response = ui.checkbox(
            &mut self.show_not_found,
            "Показывать не найденные модули",
        );
        if show_not_found_response.changed() {
            self.persist_state();
        }

        ui.add_space(8.0);
        ui.label("Состояние выбранных модулей и настройки отображения сохраняются между запусками.");
    }

    fn render_module_card(
        ui: &mut egui::Ui,
        module: &mut ModuleSnapshot,
        selection_changed: &mut bool,
    ) {
        ui.group(|ui| {
            ui.set_min_width(320.0);
            ui.horizontal_wrapped(|ui| {
                let response = ui.checkbox(&mut module.selected, "");
                if response.changed() {
                    *selection_changed = true;
                }
                ui.heading(format!("{} ({:?})", module.name, module.kind));
            });

            ui.label(module.status_label());

            if !module.updates.is_empty() {
                let selected_updates_count = module
                    .updates
                    .iter()
                    .filter(|update| update.selected)
                    .count();
                let detail_lines = module.detail_lines();
                ui.small(format!(
                    "Выбрано приложений: {}/{}",
                    selected_updates_count,
                    module.updates.len()
                ));

                egui::CollapsingHeader::new(format!("Детали ({})", module.updates.len()))
                    .default_open(false)
                    .show(ui, |ui| {
                        for (update, detail_line) in module.updates.iter_mut().zip(detail_lines.into_iter()) {
                            ui.horizontal_wrapped(|ui| {
                                let response = ui.checkbox(&mut update.selected, "");
                                if response.changed() {
                                    *selection_changed = true;
                                }
                                ui.label(detail_line);
                            });
                        }
                    });
            }
        });
    }
}

impl App for GuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut Frame) {
        self.process_events();
        let ctx = ui.ctx().clone();
        ctx.request_repaint_after(std::time::Duration::from_millis(100));

        egui::Panel::top("toolbar").show(ui, |ui| {
            self.show_toolbar(ui);
        });

        egui::CentralPanel::default().show(ui, |ui| {
            match self.active_tab {
                GuiTab::Overview => self.show_overview_tab(ui),
                GuiTab::Modules => self.show_modules_tab(ui),
                GuiTab::Logs => self.show_logs_tab(ui),
                GuiTab::Settings => self.show_settings_tab(ui),
            }
        });
    }
}

fn gui_state_path() -> Option<PathBuf> {
    std::env::current_dir().ok().map(|cwd| cwd.join(GUI_STATE_FILE))
}

fn load_gui_state() -> GuiState {
    let Some(path) = gui_state_path() else {
        return GuiState::default();
    };

    let Ok(text) = fs::read_to_string(path) else {
        return GuiState::default();
    };

    serde_json::from_str::<GuiState>(&text).unwrap_or_default()
}

fn save_gui_state(
    selection: &BTreeSet<String>,
    selected_updates: &BTreeMap<String, BTreeSet<String>>,
    auto_yes: bool,
    show_not_found: bool,
) -> anyhow::Result<()> {
    let path = gui_state_path().ok_or_else(|| anyhow::anyhow!("не удалось определить рабочую директорию"))?;
    let state = GuiState {
        selected_modules: selection.iter().cloned().collect(),
        auto_yes: Some(auto_yes),
        show_not_found: Some(show_not_found),
        selected_updates: selected_updates
            .iter()
            .map(|(module, updates)| (module.clone(), updates.iter().cloned().collect()))
            .collect(),
    };
    let text = serde_json::to_string_pretty(&state)?;
    fs::write(path, text)?;
    Ok(())
}

fn export_logs_to_file(logs: &[String]) -> anyhow::Result<PathBuf> {
    let cwd = std::env::current_dir()
        .map_err(|error| anyhow::anyhow!("не удалось определить рабочую директорию: {error}"))?;

    let logs_dir = cwd.join("logs");
    fs::create_dir_all(&logs_dir)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| anyhow::anyhow!("ошибка времени системы: {error}"))?
        .as_secs();

    let path = logs_dir.join(format!("update_all_modules_logs_{now}.txt"));
    let content = if logs.is_empty() {
        "Логи отсутствуют\n".to_owned()
    } else {
        let mut text = logs.join("\n");
        text.push('\n');
        text
    };

    fs::write(&path, content)?;
    Ok(path)
}

pub fn launch_gui(filter: SelectionFilter, auto_yes: bool) -> anyhow::Result<()> {
    system::hide_windows_console_if_needed(true);
    let native_options = NativeOptions::default();
    eframe::run_native(
        "UpdateAllModules",
        native_options,
        Box::new(move |_creation_context| Ok(Box::new(GuiApp::new(filter, auto_yes)))),
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    Ok(())
}
