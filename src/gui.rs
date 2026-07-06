use crate::app::{discover_modules, run_updates, SelectionFilter};
use crate::model::{ModuleKind, ModuleSnapshot};
use crate::system;
use eframe::{egui, App, Frame, NativeOptions};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

const GUI_STATE_FILE: &str = ".update_all_modules_gui_state.json";

#[derive(Debug, Default, Serialize, Deserialize)]
struct GuiState {
    selected_modules: Vec<String>,
    auto_yes: Option<bool>,
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
    active_tab: GuiTab,
}

impl GuiApp {
    pub fn new(filter: SelectionFilter, auto_yes: bool) -> Self {
        let (events_tx, events_rx) = mpsc::channel();
        let persisted_state = load_gui_state();
        let persisted_selection: BTreeSet<String> =
            persisted_state.selected_modules.iter().cloned().collect();
        let auto_yes = persisted_state.auto_yes.unwrap_or(auto_yes);
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
            active_tab: GuiTab::Overview,
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

    fn select_only_updates(&mut self) {
        for module in &mut self.modules {
            module.selected = module.installed && module.status.has_updates();
        }
        self.persist_state();
    }

    fn select_all(&mut self) {
        for module in &mut self.modules {
            module.selected = true;
        }
        self.persist_state();
    }

    fn select_by_kind(&mut self, kind: ModuleKind) {
        for module in &mut self.modules {
            module.selected = module.kind == kind;
        }
        self.persist_state();
    }

    fn invert_selection(&mut self) {
        for module in &mut self.modules {
            module.selected = !module.selected;
        }
        self.persist_state();
    }

    fn clear_selection(&mut self) {
        for module in &mut self.modules {
            module.selected = false;
        }
        self.persist_state();
    }

    fn selected_count(&self) -> usize {
        self.modules.iter().filter(|module| module.selected).count()
    }

    fn updates_count(&self) -> usize {
        self.modules
            .iter()
            .filter(|module| module.installed && module.status.has_updates())
            .count()
    }

    fn apply_persisted_selection(&mut self) {
        if self.persisted_selection.is_empty() {
            return;
        }

        for module in &mut self.modules {
            module.selected = self.persisted_selection.contains(&module.name);
        }
    }

    fn persist_state(&mut self) {
        self.persisted_selection = self
            .modules
            .iter()
            .filter(|module| module.selected)
            .map(|module| module.name.clone())
            .collect();

        if let Err(error) = save_gui_state(&self.persisted_selection, self.auto_yes) {
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
                    self.status_line = message;
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
                .add_enabled(!self.busy, egui::Button::new("Выбрать все"))
                .clicked()
            {
                self.select_all();
            }

            if ui
                .add_enabled(!self.busy, egui::Button::new("Только системные"))
                .clicked()
            {
                self.select_by_kind(ModuleKind::System);
            }

            if ui
                .add_enabled(!self.busy, egui::Button::new("Только инструменты"))
                .clicked()
            {
                self.select_by_kind(ModuleKind::Tool);
            }

            if ui
                .add_enabled(!self.busy, egui::Button::new("Только Python"))
                .clicked()
            {
                self.select_by_kind(ModuleKind::Python);
            }

            if ui
                .add_enabled(!self.busy, egui::Button::new("Инвертировать выбор"))
                .clicked()
            {
                self.invert_selection();
            }

            if ui
                .add_enabled(!self.busy, egui::Button::new("Выбрать с обновлениями"))
                .clicked()
            {
                self.select_only_updates();
            }

            if ui
                .add_enabled(!self.busy, egui::Button::new("Снять выбор"))
                .clicked()
            {
                self.clear_selection();
            }

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
                self.modules.len()
            ));
            ui.separator();
            ui.label(format!(
                "С обновлениями: {}/{}",
                self.updates_count(),
                self.modules.len()
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
            ui.label(format!("Найдено модулей: {}", self.modules.len()));
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

                if columns == 1 {
                    for module in &mut self.modules {
                        Self::render_module_card(ui, module, &mut selection_changed);
                    }
                } else {
                    ui.columns(columns, |columns_ui| {
                        for (index, module) in self.modules.iter_mut().enumerate() {
                            let column = &mut columns_ui[index % columns];
                            Self::render_module_card(column, module, &mut selection_changed);
                        }
                    });
                }

                if selection_changed {
                    self.persist_state();
                }
            });
    }

    fn show_logs_tab(&self, ui: &mut egui::Ui) {
        ui.heading("Console Log");
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

        ui.add_space(8.0);
        ui.label("Состояние выбранных модулей и настройка автосогласия сохраняются между запусками.");
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
                egui::CollapsingHeader::new(format!("Детали ({})", module.updates.len()))
                    .default_open(false)
                    .show(ui, |ui| {
                        for line in module.detail_lines() {
                            ui.label(line);
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

fn save_gui_state(selection: &BTreeSet<String>, auto_yes: bool) -> anyhow::Result<()> {
    let path = gui_state_path().ok_or_else(|| anyhow::anyhow!("не удалось определить рабочую директорию"))?;
    let state = GuiState {
        selected_modules: selection.iter().cloned().collect(),
        auto_yes: Some(auto_yes),
    };
    let text = serde_json::to_string_pretty(&state)?;
    fs::write(path, text)?;
    Ok(())
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
