//! Управление жизненным циклом GUI и фоновыми задачами.
//!
//! Контроллер запускает сканирование и обновление в рабочих потоках, принимает
//! [`GuiEvent`] и синхронизирует результаты с состоянием UI.

use super::state::{load_gui_state, save_gui_state};
use super::{GuiApp, GuiEvent, GuiTab};
use crate::app::{
    ModuleUpdateProgress, SelectionFilter, UpdateCancellation, discover_modules,
    run_updates_with_progress_cancellable,
};
use crate::model::ModuleSnapshot;
use crate::{repaint, system};
use eframe::egui;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc;
use std::thread;

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

    pub(super) fn start_scan(&mut self) {
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

    pub(super) fn start_update(&mut self) {
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

    pub(super) fn cancel_update(&mut self) {
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

    pub(super) fn start_update_all(&mut self) {
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

    pub(super) fn start_elevated(&mut self) {
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

    pub(super) fn selected_count(&self) -> usize {
        self.modules
            .iter()
            .filter(|module| self.is_module_visible(module) && module.selected)
            .count()
    }

    pub(super) fn updates_count(&self) -> usize {
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

    pub(super) fn visible_modules_count(&self) -> usize {
        self.modules
            .iter()
            .filter(|module| self.is_module_visible(module))
            .count()
    }

    pub(super) fn show_selection_menu(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn persist_state(&mut self) {
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

    pub(super) fn process_events(&mut self) {
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
}
