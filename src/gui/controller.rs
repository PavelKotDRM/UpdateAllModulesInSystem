//! Управление жизненным циклом GUI и фоновыми задачами.
//!
//! Контроллер запускает сканирование и обновление в рабочих потоках, принимает
//! [`GuiEvent`] и синхронизирует результаты с состоянием UI.

use super::state::{load_gui_state, save_elevation_modules, save_gui_state};
use super::{GuiApp, GuiEvent, GuiTab};
use crate::app::{
    ModuleUpdateProgress, SelectionFilter, UpdateCancellation, discover_modules_with_progress,
    run_updates_with_progress_cancellable,
};
use crate::localization::Language;
use crate::model::{ModuleSnapshot, ModuleStatus};
use crate::{repaint, system};
use eframe::egui;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::sync::mpsc;
use std::thread;

impl GuiApp {
    /// Создаёт GUI-приложение, восстанавливает сохранённое состояние и запускает
    /// первичное сканирование модулей.
    pub fn new(
        filter: SelectionFilter,
        auto_yes: bool,
        language_override: Option<Language>,
        context: &egui::Context,
        previous_modules: Option<Vec<ModuleSnapshot>>,
    ) -> Self {
        let (events_tx, events_rx) = repaint::channel(context);
        let persisted_state = load_gui_state();
        let language = language_override
            .or(persisted_state.language)
            .unwrap_or_default();
        let persisted_selection = persisted_state
            .selected_modules
            .map(|selected| selected.into_iter().collect());
        let persisted_update_selection: BTreeMap<String, BTreeSet<String>> = persisted_state
            .selected_updates
            .into_iter()
            .map(|(module, updates)| (module, updates.into_iter().collect()))
            .collect();
        let auto_yes = persisted_state.auto_yes.unwrap_or(auto_yes);
        let show_not_found = persisted_state.show_not_found.unwrap_or(false);
        let show_up_to_date = persisted_state.show_up_to_date.unwrap_or(false);
        let mut app = Self {
            filter,
            language,
            events_tx,
            events_rx,
            modules: previous_modules.unwrap_or_default(),
            logs: BTreeMap::new(),
            busy: false,
            auto_yes,
            status_line: crate::tr!(language, gui, status_waiting).to_owned(),
            started_scan: false,
            persisted_selection,
            persisted_update_selection,
            active_tab: GuiTab::Overview,
            show_not_found,
            show_up_to_date,
            module_progress: BTreeMap::new(),
            update_cancellation: None,
            elevation_pending: false,
            close_after_elevation: false,
        };
        crate::localization::with_language(language, || app.start_scan());
        app
    }

    pub(super) fn start_scan(&mut self) {
        if self.started_scan {
            return;
        }
        self.started_scan = true;
        self.busy = true;
        self.status_line = crate::tr!(self.language, gui, status_scanning).to_owned();
        self.append_log(crate::tr!(self.language, gui, log_scan_started).to_owned());
        let filter = self.filter.clone();
        let sender = self.events_tx.clone();
        let language = self.language;
        thread::spawn(move || {
            crate::localization::with_language(language, || {
                let modules = discover_modules_with_progress(&filter, |completed, total| {
                    let _ = sender.send(GuiEvent::ScanProgress { completed, total });
                });
                let _ = sender.send(GuiEvent::ScanFinished(modules));
            });
        });
    }

    pub(super) fn start_update(&mut self) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.status_line = crate::tr!(self.language, gui, status_updating).to_owned();
        let modules = self.modules.clone();
        self.module_progress.clear();
        let sender = self.events_tx.clone();
        let force_yes = self.auto_yes;
        let language = self.language;
        let cancellation = UpdateCancellation::default();
        self.update_cancellation = Some(cancellation.clone());
        thread::spawn(move || {
            crate::localization::with_language(language, || {
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
                    crate::tr!(
                        crate::localization::current_language(),
                        gui,
                        summary_update_cancelled
                    )
                    .to_owned()
                } else if results.is_empty() {
                    crate::tr!(
                        crate::localization::current_language(),
                        gui,
                        summary_no_selected_updates
                    )
                    .to_owned()
                } else if results.iter().all(|(_, result)| result.is_ok()) {
                    crate::tr!(
                        crate::localization::current_language(),
                        gui,
                        summary_update_success
                    )
                    .to_owned()
                } else {
                    crate::tr!(
                        crate::localization::current_language(),
                        gui,
                        summary_update_errors
                    )
                    .to_owned()
                };
                let _ = sender.send(GuiEvent::UpdateFinished(summary));
            });
        });
    }

    pub(super) fn cancel_update(&mut self) {
        let Some(cancellation) = &self.update_cancellation else {
            return;
        };
        if cancellation.is_cancelled() {
            return;
        }

        let termination_errors = cancellation.cancel();
        if termination_errors.is_empty() {
            self.status_line = crate::tr!(self.language, gui, status_cancel_requested).to_owned();
            self.append_log(crate::tr!(self.language, gui, log_cancel_requested).to_owned());
        } else {
            self.status_line = crate::tr!(self.language, gui, status_cancel_incomplete).to_owned();
            for error in termination_errors {
                self.append_log(crate::tr!(
                    self.language,
                    gui,
                    log_cancel_error,
                    error = error
                ));
            }
        }
    }

    pub(super) fn start_update_all(&mut self) {
        if self.busy {
            return;
        }

        for module in &mut self.modules {
            Self::set_module_selected(module, true);
        }
        self.persist_state();
        self.start_update();
    }

    pub(super) fn start_elevated(&mut self) {
        if self.busy || self.elevation_pending {
            return;
        }

        let elevated_module_names = Self::elevated_module_names(&self.modules);
        if elevated_module_names.is_empty() {
            self.status_line =
                crate::tr!(self.language, gui, status_no_elevated_modules).to_owned();
            return;
        }

        let elevation_state_file = match save_elevation_modules(&self.modules) {
            Ok(path) => path,
            Err(error) => {
                let message =
                    crate::tr!(self.language, gui, error_save_scan_results, error = error);
                self.status_line = message.clone();
                self.append_log(crate::tr!(
                    self.language,
                    gui,
                    error_prefix,
                    error = message
                ));
                return;
            }
        };

        self.elevation_pending = true;
        self.status_line = crate::tr!(self.language, gui, status_request_elevation).to_owned();
        let sender = self.events_tx.clone();
        let language = self.language;
        thread::spawn(move || {
            crate::localization::with_language(language, || {
                let result =
                    system::restart_elevated(&elevated_module_names, &elevation_state_file)
                        .map_err(|error| error.to_string());
                if result.is_err() {
                    let _ = fs::remove_file(elevation_state_file);
                }
                let _ = sender.send(GuiEvent::ElevationFinished(result));
            });
        });
    }

    fn select_only_updates(&mut self) {
        for module in &mut self.modules {
            let selected = module.installed && module.status.has_updates();
            Self::set_module_selected(module, selected);
        }
        self.persist_state();
    }

    fn invert_selection(&mut self) {
        for module in &mut self.modules {
            Self::set_module_selected(module, !module.selected);
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

    fn elevated_module_names(modules: &[ModuleSnapshot]) -> Vec<String> {
        modules
            .iter()
            .filter(|module| module.requires_elevation)
            .map(|module| module.name.clone())
            .collect()
    }

    fn replace_scanned_modules(&mut self, scanned_modules: Vec<ModuleSnapshot>) {
        self.modules = scanned_modules;
    }

    fn apply_persisted_selection(&mut self) {
        for module in &mut self.modules {
            if let Some(selection) = &self.persisted_selection {
                module.selected = selection.contains(&module.name);
            }
            if let Some(saved_updates) = self.persisted_update_selection.get(&module.name) {
                for update in &mut module.updates {
                    update.selected = saved_updates.contains(&update.selection_key());
                }
            }

            if !module.selected {
                for update in &mut module.updates {
                    update.selected = false;
                }
            } else if module.supports_package_selection
                && !module.updates.is_empty()
                && !module.updates.iter().any(|update| update.selected)
            {
                module.selected = false;
            } else if !module.supports_package_selection {
                for update in &mut module.updates {
                    update.selected = true;
                }
            }
        }
    }

    pub(super) fn is_module_visible(&self, module: &ModuleSnapshot) -> bool {
        Self::module_is_visible(module, self.show_not_found, self.show_up_to_date)
    }

    pub(super) fn visible_modules_count(&self) -> usize {
        self.modules
            .iter()
            .filter(|module| self.is_module_visible(module))
            .count()
    }

    pub(super) fn show_selection_menu(&mut self, ui: &mut egui::Ui) {
        let show_not_found = self.show_not_found;
        let show_up_to_date = self.show_up_to_date;
        ui.menu_button(crate::tr!(self.language, gui, selection_menu), |ui| {
            if ui
                .button(crate::tr!(self.language, gui, select_visible))
                .clicked()
            {
                for module in &mut self.modules {
                    if Self::module_is_visible(module, show_not_found, show_up_to_date) {
                        Self::set_module_selected(module, true);
                    }
                }
                self.persist_state();
                ui.close();
            }

            if ui
                .button(crate::tr!(self.language, gui, deselect_visible))
                .clicked()
            {
                for module in &mut self.modules {
                    if Self::module_is_visible(module, show_not_found, show_up_to_date) {
                        Self::set_module_selected(module, false);
                    }
                }
                self.persist_state();
                ui.close();
            }

            if ui
                .button(crate::tr!(self.language, gui, invert_selection))
                .clicked()
            {
                self.invert_selection();
                ui.close();
            }

            if ui
                .button(crate::tr!(self.language, gui, select_only_updates))
                .clicked()
            {
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
                    let selected = module.selected;
                    Self::set_module_selected(module, selected);
                    selection_changed = true;
                }
            }

            if selection_changed {
                self.persist_state();
            }
        });
    }

    pub(super) fn persist_state(&mut self) {
        let selection = if self.started_scan && self.modules.is_empty() {
            self.persisted_selection.clone()
        } else {
            let selection = self
                .modules
                .iter()
                .filter(|module| module.selected)
                .map(|module| module.name.clone())
                .collect::<BTreeSet<_>>();
            self.persisted_selection = Some(selection.clone());

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
            Some(selection)
        };

        if let Err(error) = save_gui_state(
            self.language,
            selection.as_ref(),
            &self.persisted_update_selection,
            self.auto_yes,
            self.show_not_found,
            self.show_up_to_date,
        ) {
            self.append_log(crate::tr!(
                self.language,
                gui,
                error_save_state,
                error = error
            ));
        }
    }

    fn set_module_selected(module: &mut ModuleSnapshot, selected: bool) {
        module.selected = selected;
        for update in &mut module.updates {
            update.selected = selected;
        }
    }

    fn module_is_visible(
        module: &ModuleSnapshot,
        show_not_found: bool,
        show_up_to_date: bool,
    ) -> bool {
        if !module.installed {
            return show_not_found;
        }

        show_up_to_date || !matches!(module.status, ModuleStatus::UpToDate)
    }

    pub(super) fn process_events(&mut self) {
        while let Ok(event) = self.events_rx.try_recv() {
            match event {
                GuiEvent::Log(message) => self.append_log(message),
                GuiEvent::ModuleProgress(progress) => {
                    self.module_progress
                        .insert(progress.module_name.clone(), progress);
                }
                GuiEvent::ScanProgress { completed, total } => {
                    self.status_line = crate::tr!(
                        self.language,
                        gui,
                        status_scan_progress,
                        completed = completed,
                        total = total
                    );
                }
                GuiEvent::ScanFinished(modules) => {
                    let module_count = modules.len();
                    let update_count = modules
                        .iter()
                        .map(|module| module.updates.len())
                        .sum::<usize>();
                    self.replace_scanned_modules(modules);
                    self.module_progress
                        .retain(|_, progress| progress.phase == crate::app::UpdatePhase::Failed);
                    self.apply_persisted_selection();
                    self.busy = false;
                    self.started_scan = false;
                    let message = crate::tr!(
                        self.language,
                        gui,
                        status_scan_complete,
                        modules = module_count,
                        updates = update_count
                    );
                    self.append_log(format!("[system] {message}"));
                    self.status_line = message;
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
                    self.status_line =
                        crate::tr!(self.language, gui, status_elevated_success).to_owned();
                }
                GuiEvent::ElevationFinished(Err(error)) => {
                    self.elevation_pending = false;
                    let message = crate::tr!(self.language, gui, error_elevate, error = error);
                    self.status_line = message.clone();
                    self.append_log(crate::tr!(
                        self.language,
                        gui,
                        error_prefix,
                        error = message
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ModuleKind, ModuleStatus};

    #[test]
    fn selects_installed_and_missing_modules_that_require_elevation() {
        let mut missing_elevated = ModuleSnapshot::new("windows-update", ModuleKind::System, true);
        let mut installed_elevated = ModuleSnapshot::new("winget", ModuleKind::System, true);
        installed_elevated.installed = true;
        let missing_standard = ModuleSnapshot::new("npm", ModuleKind::Tool, false);
        missing_elevated.installed = false;

        assert_eq!(
            GuiApp::elevated_module_names(&[
                missing_elevated,
                installed_elevated,
                missing_standard,
            ]),
            vec!["windows-update", "winget"]
        );
    }

    #[test]
    fn empty_persisted_selection_keeps_every_module_unselected() {
        let mut module = ModuleSnapshot::new("winget", ModuleKind::System, true);
        module.installed = true;
        module.status = ModuleStatus::UpdatesAvailable(1);
        module.updates = vec![crate::model::PackageUpdate::new(
            "winget:package",
            "1.0",
            "2.0",
        )];
        let mut app = GuiApp {
            filter: SelectionFilter::default(),
            language: Language::English,
            events_tx: repaint::channel(&egui::Context::default()).0,
            events_rx: mpsc::channel().1,
            modules: vec![module],
            logs: BTreeMap::new(),
            busy: false,
            auto_yes: false,
            status_line: String::new(),
            started_scan: false,
            persisted_selection: Some(BTreeSet::new()),
            persisted_update_selection: BTreeMap::from([(
                "winget".to_owned(),
                BTreeSet::from(["winget:package".to_owned()]),
            )]),
            active_tab: GuiTab::Overview,
            show_not_found: false,
            show_up_to_date: false,
            module_progress: BTreeMap::new(),
            update_cancellation: None,
            elevation_pending: false,
            close_after_elevation: false,
        };

        app.apply_persisted_selection();

        assert!(!app.modules[0].selected);
        assert!(!app.modules[0].updates[0].selected);
    }

    #[test]
    fn changing_module_selection_synchronizes_package_selection() {
        let mut module = ModuleSnapshot::new("winget", ModuleKind::System, true);
        module.updates = vec![
            crate::model::PackageUpdate::new("winget:first", "1.0", "2.0"),
            crate::model::PackageUpdate::new("winget:second", "1.0", "2.0"),
        ];

        GuiApp::set_module_selected(&mut module, false);
        assert!(!module.selected);
        assert!(module.updates.iter().all(|update| !update.selected));

        GuiApp::set_module_selected(&mut module, true);
        assert!(module.selected);
        assert!(module.updates.iter().all(|update| update.selected));
    }

    #[test]
    fn replaces_previous_modules_with_full_rescan() {
        let mut scanned_module = ModuleSnapshot::new("windows-update", ModuleKind::System, true);
        scanned_module.installed = true;
        scanned_module.status = ModuleStatus::UpdatesAvailable(1);

        let mut app = GuiApp {
            filter: SelectionFilter::default(),
            language: Language::English,
            events_tx: repaint::channel(&egui::Context::default()).0,
            events_rx: mpsc::channel().1,
            modules: vec![ModuleSnapshot::new("npm", ModuleKind::Tool, false)],
            logs: BTreeMap::new(),
            busy: false,
            auto_yes: false,
            status_line: String::new(),
            started_scan: false,
            persisted_selection: None,
            persisted_update_selection: BTreeMap::new(),
            active_tab: GuiTab::Overview,
            show_not_found: false,
            show_up_to_date: false,
            module_progress: BTreeMap::new(),
            update_cancellation: None,
            elevation_pending: false,
            close_after_elevation: false,
        };
        app.replace_scanned_modules(vec![scanned_module]);

        assert_eq!(app.modules.len(), 1);
        assert_eq!(app.modules[0].name, "windows-update");
        assert!(app.modules[0].installed);
        assert!(matches!(
            app.modules[0].status,
            ModuleStatus::UpdatesAvailable(1)
        ));
    }

    #[test]
    fn hides_up_to_date_modules_unless_enabled() {
        let app = GuiApp {
            filter: SelectionFilter::default(),
            language: Language::English,
            events_tx: repaint::channel(&egui::Context::default()).0,
            events_rx: mpsc::channel().1,
            modules: Vec::new(),
            logs: BTreeMap::new(),
            busy: false,
            auto_yes: false,
            status_line: String::new(),
            started_scan: false,
            persisted_selection: None,
            persisted_update_selection: BTreeMap::new(),
            active_tab: GuiTab::Overview,
            show_not_found: false,
            show_up_to_date: false,
            module_progress: BTreeMap::new(),
            update_cancellation: None,
            elevation_pending: false,
            close_after_elevation: false,
        };
        let mut module = ModuleSnapshot::new("npm", ModuleKind::Tool, false);
        module.installed = true;
        module.status = ModuleStatus::UpToDate;

        assert!(!app.is_module_visible(&module));

        let app = GuiApp {
            show_up_to_date: true,
            ..app
        };
        assert!(app.is_module_visible(&module));
    }
}
