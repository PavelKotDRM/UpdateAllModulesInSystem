use crate::model::{ModuleKind, ModuleSnapshot, ModuleStatus};
use crate::system;
use crate::updater::{Updater, UpdaterError};
use crate::updaters::{registry, UpdaterDescriptor};
use comfy_table::{presets::UTF8_FULL, ContentArrangement, Table};
use std::collections::BTreeSet;
use std::sync::mpsc::Sender;

#[derive(Debug, Clone, Default)]
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
    let handlers = registry()
        .into_iter()
        .map(|descriptor| (descriptor.updater.name().to_owned(), descriptor.updater))
        .collect::<std::collections::BTreeMap<_, _>>();

    for module in modules {
        let selected_updates: Vec<_> = module
            .updates
            .iter()
            .filter(|update| update.selected)
            .cloned()
            .collect();

        if let Some(reason) = skip_update_reason(module, &selected_updates) {
            let _ = log_sender.send(format!("Пропуск {}: {}", module.name, reason));
            continue;
        }

        let _ = log_sender.send(format!("Запуск обновления: {}", module.name));

        let outcome = match handlers.get(&module.name) {
            Some(updater) => updater.apply_updates(force_yes, &selected_updates, log_sender),
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

fn skip_update_reason(
    module: &ModuleSnapshot,
    selected_updates: &[crate::model::PackageUpdate],
) -> Option<&'static str> {
    if !module.selected {
        return Some("модуль не выбран");
    }

    if !module.installed {
        return Some("инструмент не установлен");
    }

    if !module.status.has_updates() {
        return Some("обновления не требуются");
    }

    if !module.updates.is_empty() && selected_updates.is_empty() {
        return Some("все обновления внутри модуля сняты");
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PackageUpdate;

    struct FakeUpdater {
        name: &'static str,
        installed: bool,
        updates: Vec<PackageUpdate>,
        fail_message: Option<String>,
    }

    impl Updater for FakeUpdater {
        fn name(&self) -> &'static str {
            self.name
        }

        fn is_installed(&self) -> bool {
            self.installed
        }

        fn check_updates(&self) -> Result<Vec<PackageUpdate>, UpdaterError> {
            if let Some(message) = &self.fail_message {
                return Err(UpdaterError::Message(message.clone()));
            }
            Ok(self.updates.clone())
        }

        fn apply_updates(
            &self,
            _force_yes: bool,
            _selected_updates: &[PackageUpdate],
            _log_sender: &Sender<String>,
        ) -> Result<(), UpdaterError> {
            Ok(())
        }
    }

    fn make_descriptor(name: &'static str, kind: ModuleKind) -> UpdaterDescriptor {
        UpdaterDescriptor {
            updater: Box::new(FakeUpdater {
                name,
                installed: true,
                updates: Vec::new(),
                fail_message: None,
            }),
            kind,
            requires_elevation: false,
        }
    }

    #[test]
    fn includes_descriptor_respects_skip_and_only_flags() {
        let system = make_descriptor("winget", ModuleKind::System);
        let tool = make_descriptor("rustup", ModuleKind::Tool);
        let python = make_descriptor("pip", ModuleKind::Python);

        let mut filter = SelectionFilter::default();
        assert!(filter.includes_descriptor(&system));
        assert!(filter.includes_descriptor(&tool));
        assert!(filter.includes_descriptor(&python));

        filter.skip_system = true;
        assert!(!filter.includes_descriptor(&system));
        assert!(filter.includes_descriptor(&tool));

        filter.skip_tools = true;
        assert!(!filter.includes_descriptor(&tool));

        filter.skip_pip = true;
        assert!(!filter.includes_descriptor(&python));

        filter = SelectionFilter::default();
        filter.only_tools = true;
        assert!(filter.includes_descriptor(&tool));
        assert!(!filter.includes_descriptor(&system));

        filter.only_tools = false;
        filter.only.insert("pip".to_owned());
        assert!(!filter.includes_descriptor(&tool));
        assert!(filter.includes_descriptor(&python));
    }

    #[test]
    fn scan_updater_sets_not_found_when_not_installed() {
        let updater = FakeUpdater {
            name: "fake",
            installed: false,
            updates: Vec::new(),
            fail_message: None,
        };

        let snapshot = scan_updater(&updater, ModuleKind::Tool, false);
        assert!(!snapshot.installed);
        assert!(matches!(snapshot.status, ModuleStatus::NotFound));
    }

    #[test]
    fn scan_updater_sets_up_to_date_and_updates_available() {
        let up_to_date = FakeUpdater {
            name: "fake-up-to-date",
            installed: true,
            updates: Vec::new(),
            fail_message: None,
        };

        let snapshot = scan_updater(&up_to_date, ModuleKind::Tool, false);
        assert!(snapshot.installed);
        assert!(matches!(snapshot.status, ModuleStatus::UpToDate));

        let with_updates = FakeUpdater {
            name: "fake-with-updates",
            installed: true,
            updates: vec![PackageUpdate::new("pkg", "1.0", "1.1")],
            fail_message: None,
        };

        let snapshot = scan_updater(&with_updates, ModuleKind::Tool, false);
        assert!(matches!(snapshot.status, ModuleStatus::UpdatesAvailable(1)));
        assert_eq!(snapshot.updates.len(), 1);
    }

    #[test]
    fn scan_updater_sets_error_status_on_failure() {
        let updater = FakeUpdater {
            name: "fake-error",
            installed: true,
            updates: Vec::new(),
            fail_message: Some("boom".to_owned()),
        };

        let snapshot = scan_updater(&updater, ModuleKind::Tool, false);
        assert!(matches!(snapshot.status, ModuleStatus::Error(_)));
        assert!(snapshot.status_label().contains("boom"));
    }

    #[test]
    fn run_updates_skips_ineligible_modules_and_reports_missing_handler() {
        let mut skipped_not_selected = ModuleSnapshot::new("m1", ModuleKind::Tool, false);
        skipped_not_selected.selected = false;
        skipped_not_selected.installed = true;
        skipped_not_selected.status = ModuleStatus::UpdatesAvailable(1);

        let mut skipped_not_installed = ModuleSnapshot::new("m2", ModuleKind::Tool, false);
        skipped_not_installed.selected = true;
        skipped_not_installed.installed = false;
        skipped_not_installed.status = ModuleStatus::UpdatesAvailable(1);

        let mut skipped_no_updates = ModuleSnapshot::new("m3", ModuleKind::Tool, false);
        skipped_no_updates.selected = true;
        skipped_no_updates.installed = true;
        skipped_no_updates.status = ModuleStatus::UpToDate;

        let mut missing_handler = ModuleSnapshot::new("missing-updater", ModuleKind::Tool, false);
        missing_handler.selected = true;
        missing_handler.installed = true;
        missing_handler.status = ModuleStatus::UpdatesAvailable(1);

        let modules = vec![
            skipped_not_selected,
            skipped_not_installed,
            skipped_no_updates,
            missing_handler,
        ];

        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let results = run_updates(&modules, true, &tx);
        drop(tx);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "missing-updater");
        assert!(results[0].1.is_err());

        let logs: Vec<String> = rx.iter().collect();
        assert!(logs.iter().any(|line| line.contains("модуль не выбран")));
        assert!(logs.iter().any(|line| line.contains("инструмент не установлен")));
        assert!(logs.iter().any(|line| line.contains("обновления не требуются")));
        assert!(logs.iter().any(|line| line.contains("обработчик не найден")));
    }
}
