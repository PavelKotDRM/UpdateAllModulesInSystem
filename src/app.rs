use crate::model::{ModuleKind, ModuleSnapshot, ModuleStatus};
use crate::system;
use crate::updater::{Updater, UpdaterError};
use crate::updaters::{lookup_updater, registry, UpdaterDescriptor};
use comfy_table::{presets::UTF8_FULL, ContentArrangement, Table};
use std::collections::BTreeSet;
use std::sync::mpsc::Sender;

#[derive(Debug, Clone)]
pub struct SelectionFilter {
    pub skip_system: bool,
    pub skip_pip: bool,
    pub skip_tools: bool,
    pub only_tools: bool,
    pub only: BTreeSet<String>,
}

impl SelectionFilter {
    pub fn includes_descriptor(&self, descriptor: &UpdaterDescriptor) -> bool {
        if self.only_tools && descriptor.kind != ModuleKind::Tool {
            return false;
        }

        if self.skip_system && descriptor.kind == ModuleKind::System {
            return false;
        }

        if self.skip_pip && descriptor.kind == ModuleKind::Python {
            return false;
        }

        if self.skip_tools && descriptor.kind == ModuleKind::Tool {
            return false;
        }

        if !self.only.is_empty() && !self.only.contains(descriptor.updater.name()) {
            return false;
        }

        true
    }
}

impl Default for SelectionFilter {
    fn default() -> Self {
        Self {
            skip_system: false,
            skip_pip: false,
            skip_tools: false,
            only_tools: false,
            only: BTreeSet::new(),
        }
    }
}

pub fn discover_modules(filter: &SelectionFilter) -> Vec<ModuleSnapshot> {
    let mut modules = Vec::new();

    for descriptor in registry() {
        if !filter.includes_descriptor(&descriptor) {
            continue;
        }

        modules.push(scan_updater(&*descriptor.updater, descriptor.kind, descriptor.requires_elevation));
    }

    modules
}

pub fn scan_updater(updater: &dyn Updater, kind: ModuleKind, requires_elevation: bool) -> ModuleSnapshot {
    let mut snapshot = ModuleSnapshot::new(updater.name(), kind, requires_elevation);

    if !updater.is_installed() {
        snapshot.status = ModuleStatus::NotFound;
        snapshot.installed = false;
        return snapshot;
    }

    snapshot.installed = true;
    match updater.check_updates() {
        Ok(updates) if updates.is_empty() => {
            snapshot.status = ModuleStatus::UpToDate;
        }
        Ok(updates) => {
            snapshot.status = ModuleStatus::UpdatesAvailable(updates.len());
            snapshot.updates = updates;
        }
        Err(error) => {
            snapshot.status = ModuleStatus::Error(error.to_string());
        }
    }

    snapshot
}

pub fn render_cli_table(modules: &[ModuleSnapshot]) -> String {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec!["Инструмент", "Установлен", "Статус", "Обновления"]);

    for module in modules {
        table.add_row(vec![
            module.name.clone(),
            if module.installed { "Да".to_owned() } else { "Нет".to_owned() },
            module.status_label(),
            if module.updates.is_empty() {
                "-".to_owned()
            } else {
                module
                    .updates
                    .iter()
                    .map(|update| format!("{}→{}", update.name, update.available_version))
                    .collect::<Vec<_>>()
                    .join(", ")
            },
        ]);
    }

    table.to_string()
}

pub fn summarize_elevation_warning(modules: &[ModuleSnapshot]) -> Option<String> {
    if system::is_admin() {
        return None;
    }

    let needs_privileges = modules.iter().any(|module| module.requires_elevation && module.installed);
    if !needs_privileges {
        return None;
    }

    Some("Запуск без прав администратора: системные менеджеры могут быть пропущены или завершиться с ошибкой".to_owned())
}

pub fn run_updates(modules: &[ModuleSnapshot], force_yes: bool, log_sender: &Sender<String>) -> Vec<(String, Result<(), UpdaterError>)> {
    let mut results = Vec::new();

    for module in modules {
        if !module.selected {
            let _ = log_sender.send(format!("Пропуск {}: модуль не выбран", module.name));
            continue;
        }

        if !module.installed {
            let _ = log_sender.send(format!("Пропуск {}: инструмент не установлен", module.name));
            continue;
        }

        if !module.status.has_updates() {
            let _ = log_sender.send(format!("Пропуск {}: обновления не требуются", module.name));
            continue;
        }

        let _ = log_sender.send(format!("Запуск обновления: {}", module.name));

        let outcome = match lookup_updater(&module.name) {
            Some(descriptor) => descriptor.updater.apply_updates(force_yes, log_sender),
            None => Err(UpdaterError::Message(format!("{}: обработчик не найден", module.name))),
        };

        if let Err(error) = &outcome {
            let _ = log_sender.send(format!("Ошибка в {}: {}", module.name, error));
        } else {
            let _ = log_sender.send(format!("Завершено: {}", module.name));
        }

        results.push((module.name.clone(), outcome));
    }

    results
}
