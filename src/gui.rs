//! Графический интерфейс приложения на базе `egui`/`eframe`.

mod controller;
mod logs;
mod rendering;
mod state;
mod views;

use crate::app::{ModuleUpdateProgress, SelectionFilter, UpdateCancellation};
use crate::localization::Language;
use crate::model::ModuleSnapshot;
use crate::repaint::{self, RepaintSender};
use crate::system;
use eframe::{App, Frame, egui};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;

#[derive(Debug)]
/// События, которыми фоновые задачи уведомляют GUI.
pub enum GuiEvent {
    /// Строка лога от процесса сканирования/обновления.
    Log(String),
    /// Обновление фазы конкретного модуля.
    ModuleProgress(ModuleUpdateProgress),
    /// Завершение сканирования с итоговым списком модулей.
    ScanFinished(Vec<ModuleSnapshot>),
    /// Прогресс сканирования модулей.
    ScanProgress { completed: usize, total: usize },
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
    language: Language,
    events_tx: RepaintSender<GuiEvent>,
    events_rx: Receiver<GuiEvent>,
    modules: Vec<ModuleSnapshot>,
    logs: BTreeMap<String, Vec<String>>,
    busy: bool,
    auto_yes: bool,
    status_line: String,
    started_scan: bool,
    persisted_selection: Option<BTreeSet<String>>,
    persisted_update_selection: BTreeMap<String, BTreeSet<String>>,
    active_tab: GuiTab,
    show_not_found: bool,
    show_up_to_date: bool,
    module_progress: BTreeMap<String, ModuleUpdateProgress>,
    update_cancellation: Option<UpdateCancellation>,
    elevation_pending: bool,
    close_after_elevation: bool,
}

impl App for GuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut Frame) {
        let language = self.language;
        crate::localization::with_language(language, || self.show_ui(ui, _frame));
    }
}

impl GuiApp {
    fn show_ui(&mut self, ui: &mut egui::Ui, _frame: &mut Frame) {
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
    language: Option<Language>,
    elevation_ready_file: Option<PathBuf>,
    elevation_state_file: Option<PathBuf>,
) -> anyhow::Result<()> {
    system::hide_windows_console_if_needed(true);
    if let Some(path) = &elevation_state_file {
        system::validate_elevation_state_file(path)?;
    }
    if let Some(path) = &elevation_ready_file {
        system::validate_elevation_ready_file(path)?;
    }
    let native_options = repaint::stable_native_options();
    eframe::run_native(
        "UpdateAllModules",
        native_options,
        Box::new(move |creation_context| {
            let previous_modules = elevation_state_file
                .as_deref()
                .and_then(state::take_elevation_modules);
            let app = GuiApp::new(
                filter,
                auto_yes,
                language,
                &creation_context.egui_ctx,
                previous_modules,
            );
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
