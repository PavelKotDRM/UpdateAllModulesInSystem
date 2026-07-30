use super::worker_count_for;
use crate::model::{ModuleKind, ModuleSnapshot, ModuleStatus};
use crate::updater::Updater;
use crate::updaters::{UpdaterDescriptor, registry};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

/// Фильтр выбора модулей для сканирования и обновления.
#[derive(Debug, Clone, Default)]
pub struct SelectionFilter {
    /// Исключить системные менеджеры.
    pub skip_system: bool,
    /// Исключить Python-пакеты.
    pub skip_pip: bool,
    /// Исключить инструменты разработки.
    pub skip_tools: bool,
    /// Включить только инструменты разработки.
    pub only_tools: bool,
    /// Явный список имен модулей для включения.
    pub only: BTreeSet<String>,
}

impl SelectionFilter {
    /// Проверяет, должен ли дескриптор участвовать в сканировании.
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

/// Выполняет параллельное сканирование модулей с учетом фильтра.
///
/// Возвращает снимки в стабильном порядке реестра, хотя проверки выполняются
/// параллельно.
///
/// # Panics
/// Может паниковать при `poisoned mutex` или аварийном завершении worker-потока.
///
pub fn discover_modules(filter: &SelectionFilter) -> Vec<ModuleSnapshot> {
    let scan_jobs = registry()
        .into_iter()
        .enumerate()
        .filter(|(_, descriptor)| filter.includes_descriptor(descriptor))
        .collect::<Vec<_>>();

    if scan_jobs.is_empty() {
        return Vec::new();
    }

    let expected_results = scan_jobs.len();
    let worker_count = worker_count_for(expected_results);
    let scan_jobs = Arc::new(Mutex::new(scan_jobs));
    let (result_tx, result_rx) = mpsc::channel::<(usize, ModuleSnapshot)>();
    let mut handles = Vec::with_capacity(worker_count);

    for _ in 0..worker_count {
        let jobs = Arc::clone(&scan_jobs);
        let result_sender = result_tx.clone();
        handles.push(thread::spawn(move || {
            loop {
                let next_job = {
                    let mut jobs = jobs.lock().expect("scan jobs mutex poisoned");
                    jobs.pop()
                };

                let Some((index, descriptor)) = next_job else {
                    break;
                };

                let snapshot = scan_updater(
                    &*descriptor.updater,
                    descriptor.kind,
                    descriptor.requires_elevation,
                );
                let _ = result_sender.send((index, snapshot));
            }
        }));
    }
    drop(result_tx);

    // Сканирование идет параллельно, но наружу возвращаем стабильный порядок реестра.
    let mut modules = result_rx.iter().take(expected_results).collect::<Vec<_>>();
    for handle in handles {
        handle.join().expect("scan worker panicked");
    }
    modules.sort_by_key(|(index, _)| *index);
    modules.into_iter().map(|(_, snapshot)| snapshot).collect()
}

/// Сканирует один обновлятор и преобразует ошибку проверки в
/// [`ModuleStatus::Error`].
pub fn scan_updater(
    updater: &dyn Updater,
    kind: ModuleKind,
    requires_elevation: bool,
) -> ModuleSnapshot {
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
