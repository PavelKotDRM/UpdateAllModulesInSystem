//! Оркестрация обнаружения модулей, отображения статусов и запуска обновлений.

mod discovery;

pub use discovery::{SelectionFilter, discover_modules};

use crate::model::{ModuleKind, ModuleSnapshot};
use crate::system;
use crate::updater::{Updater, UpdaterError};
use crate::updaters::registry;
use comfy_table::{ContentArrangement, Table, presets::UTF8_FULL};
use std::collections::BTreeMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Sender},
};
use std::thread;

const MAX_PARALLEL_WORKERS: usize = 8;

/// Фаза выполнения обновления отдельного модуля.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdatePhase {
    /// Модуль поставлен в очередь на выполнение.
    Queued,
    /// Обновление модуля выполняется в данный момент.
    Running,
    /// Обновление завершено успешно.
    Completed,
    /// Обновление завершено с ошибкой.
    Failed,
    /// Обновление отменено до запуска модуля.
    Cancelled,
}

impl UpdatePhase {
    /// Возвращает локализованную подпись для CLI и GUI.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Queued => "В очереди",
            Self::Running => "Выполняется",
            Self::Completed => "Завершен",
            Self::Failed => "Ошибка",
            Self::Cancelled => "Отменено",
        }
    }
}

/// Потокобезопасный сигнал кооперативной отмены обновления.
///
/// Отмена останавливает выдачу новых заданий, но не прерывает внешние процессы,
/// которые уже выполняются.
#[derive(Clone, Default)]
pub struct UpdateCancellation {
    cancelled: Arc<AtomicBool>,
}

impl UpdateCancellation {
    /// Запрашивает остановку очереди после завершения уже запущенных модулей.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Возвращает `true`, если пользователь запросил отмену.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// Событие прогресса по конкретному модулю.
#[derive(Debug, Clone)]
pub struct ModuleUpdateProgress {
    /// Имя модуля.
    pub module_name: String,
    /// Текущая фаза выполнения.
    pub phase: UpdatePhase,
    /// Последняя строка потокового вывода обновлятора.
    pub detail: Option<String>,
}

/// Рендерит снимки модулей в UTF-8 таблицу для CLI.
pub fn render_cli_table(modules: &[ModuleSnapshot]) -> String {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec!["Инструмент", "Установлен", "Статус", "Обновления"]);

    for module in modules {
        table.add_row(vec![
            module.name.clone(),
            if module.installed {
                "Да".to_owned()
            } else {
                "Нет".to_owned()
            },
            module.status_label(),
            if module.updates.is_empty() {
                "-".to_owned()
            } else {
                module
                    .updates
                    .iter()
                    .map(|update| format!("{}→{}", update.display_name(), update.available_version))
                    .collect::<Vec<_>>()
                    .join(", ")
            },
        ]);
    }

    table.to_string()
}

/// Формирует предупреждение, если обнаруженному менеджеру нужны повышенные
/// права, а текущий процесс ими не обладает.
pub fn summarize_elevation_warning(modules: &[ModuleSnapshot]) -> Option<String> {
    if system::is_admin() {
        return None;
    }

    let needs_privileges = modules
        .iter()
        .any(|module| module.requires_elevation && module.installed);
    if !needs_privileges {
        return None;
    }

    Some("Запуск без прав администратора: системные менеджеры могут быть пропущены или завершиться с ошибкой".to_owned())
}

/// Запускает обновление модулей без явного канала прогресса.
///
/// Возвращает результаты только для фактически запущенных заданий; пропущенные
/// модули отражаются в логе.
///
/// # Panics
/// Паникует при аварийном завершении worker-потока.
pub fn run_updates(
    modules: &[ModuleSnapshot],
    force_yes: bool,
    log_sender: &Sender<String>,
) -> Vec<(String, Result<(), UpdaterError>)> {
    run_updates_with_progress(modules, force_yes, log_sender, None)
}

/// Запускает обновление модулей с передачей лога и событий прогресса.
///
/// Результаты фактически запущенных заданий возвращаются в порядке входного
/// списка. Пропущенные модули в результат не включаются, а ошибки отдельных
/// заданий представлены значениями [`UpdaterError`].
///
/// # Panics
/// Может паниковать при аварийном завершении worker-потоков.
///
pub fn run_updates_with_progress(
    modules: &[ModuleSnapshot],
    force_yes: bool,
    log_sender: &Sender<String>,
    progress_sender: Option<Sender<ModuleUpdateProgress>>,
) -> Vec<(String, Result<(), UpdaterError>)> {
    run_updates_with_progress_cancellable(
        modules,
        force_yes,
        log_sender,
        progress_sender,
        UpdateCancellation::default(),
    )
}

/// Запускает обновление с возможностью кооперативно остановить очередь заданий.
///
/// После [`UpdateCancellation::cancel`] задания, ещё не взятые рабочими потоками,
/// получают фазу [`UpdatePhase::Cancelled`]. Уже запущенные задания завершаются
/// штатно. Результаты возвращаются в порядке исходного списка модулей.
///
/// # Panics
///
/// Может паниковать при poisoned mutex или аварийном завершении рабочего потока.
pub fn run_updates_with_progress_cancellable(
    modules: &[ModuleSnapshot],
    force_yes: bool,
    log_sender: &Sender<String>,
    progress_sender: Option<Sender<ModuleUpdateProgress>>,
    cancellation: UpdateCancellation,
) -> Vec<(String, Result<(), UpdaterError>)> {
    let mut results = Vec::new();
    let mut handlers = registry()
        .into_iter()
        .map(|descriptor| (descriptor.updater.name().to_owned(), descriptor.updater))
        .collect::<BTreeMap<_, _>>();
    let mut serial_jobs = Vec::new();
    let mut parallel_jobs = Vec::new();

    for (index, module) in modules.iter().enumerate() {
        let selected_updates: Vec<_> = module
            .updates
            .iter()
            .filter(|update| update.selected)
            .cloned()
            .collect();

        if let Some(reason) = skip_update_reason(module, &selected_updates) {
            let _ = log_sender.send(format_module_log(
                &module.name,
                format!("Пропуск: {reason}"),
            ));
            continue;
        }

        let Some(updater) = handlers.remove(&module.name) else {
            let outcome = Err(UpdaterError::Message(format!(
                "{}: обработчик не найден",
                module.name
            )));
            send_progress(&progress_sender, &module.name, UpdatePhase::Failed);
            let _ = log_sender.send(format_module_log(
                &module.name,
                format!("Этап: ошибка: {}", outcome.as_ref().err().unwrap()),
            ));
            results.push((module.name.clone(), outcome));
            continue;
        };

        let job = UpdateJob {
            index,
            module_name: module.name.clone(),
            updater,
            selected_updates,
        };

        send_progress(&progress_sender, &module.name, UpdatePhase::Queued);

        // Системные менеджеры часто конфликтуют из-за блокировок пакетной базы,
        // поэтому их оставляем в одной последовательной очереди.
        if should_run_update_in_parallel(module) {
            parallel_jobs.push(job);
        } else {
            serial_jobs.push(job);
        }
    }

    let parallel_handle = (!parallel_jobs.is_empty()).then(|| {
        let log_sender = log_sender.clone();
        let progress_sender = progress_sender.clone();
        let cancellation = cancellation.clone();
        thread::spawn(move || {
            run_parallel_update_jobs(
                parallel_jobs,
                force_yes,
                log_sender,
                progress_sender,
                cancellation,
            )
        })
    });

    let serial_sender = log_sender.clone();
    let serial_progress_sender = progress_sender.clone();
    let serial_cancellation = cancellation.clone();
    let serial_handle = (!serial_jobs.is_empty()).then(|| {
        thread::spawn(move || {
            let mut completed = Vec::new();
            let mut jobs = serial_jobs.into_iter();
            while let Some(job) = jobs.next() {
                if serial_cancellation.is_cancelled() {
                    cancel_queued_job(job, &serial_sender, &serial_progress_sender);
                    for queued_job in jobs {
                        cancel_queued_job(queued_job, &serial_sender, &serial_progress_sender);
                    }
                    break;
                }
                completed.push(run_update_job(
                    job,
                    force_yes,
                    &serial_sender,
                    serial_progress_sender.clone(),
                ));
            }
            completed
        })
    });

    let mut completed = parallel_handle
        .map(|handle| handle.join().expect("parallel update worker panicked"))
        .unwrap_or_default();

    if let Some(handle) = serial_handle {
        completed.extend(handle.join().expect("serial update worker panicked"));
    }

    completed.sort_by_key(|(index, _, _)| *index);
    results.extend(
        completed
            .into_iter()
            .map(|(_, module_name, outcome)| (module_name, outcome)),
    );

    results
}

struct UpdateJob {
    index: usize,
    module_name: String,
    updater: Box<dyn Updater>,
    selected_updates: Vec<crate::model::PackageUpdate>,
}

fn should_run_update_in_parallel(module: &ModuleSnapshot) -> bool {
    module.kind != ModuleKind::System
}

fn run_update_job(
    job: UpdateJob,
    force_yes: bool,
    log_sender: &Sender<String>,
    progress_sender: Option<Sender<ModuleUpdateProgress>>,
) -> (usize, String, Result<(), UpdaterError>) {
    let UpdateJob {
        index,
        module_name,
        updater,
        selected_updates,
    } = job;

    send_progress(&progress_sender, &module_name, UpdatePhase::Running);
    let _ = log_sender.send(format_module_log(&module_name, "Этап: запуск обновления"));

    let (module_log_tx, module_log_rx) = mpsc::channel::<String>();
    let forward_sender = log_sender.clone();
    let forward_module_name = module_name.clone();
    let forward_progress_sender = progress_sender.clone();
    // Каждый обновлятор пишет в свой канал, а здесь мы добавляем префикс модуля
    // и объединяем поток логов в общий канал UI/CLI.
    let forward_handle = thread::spawn(move || {
        while let Ok(message) = module_log_rx.recv() {
            let _ = forward_sender.send(format_module_log(&forward_module_name, &message));
            if let Some(sender) = &forward_progress_sender {
                let _ = sender.send(ModuleUpdateProgress {
                    module_name: forward_module_name.clone(),
                    phase: UpdatePhase::Running,
                    detail: Some(message),
                });
            }
        }
    });

    let outcome = updater.apply_updates(force_yes, &selected_updates, &module_log_tx);
    drop(module_log_tx);
    let _ = forward_handle.join();

    match &outcome {
        Ok(()) => {
            send_progress(&progress_sender, &module_name, UpdatePhase::Completed);
            let _ = log_sender.send(format_module_log(&module_name, "Этап: завершено"));
        }
        Err(error) => {
            send_progress(&progress_sender, &module_name, UpdatePhase::Failed);
            let _ = log_sender.send(format_module_log(
                &module_name,
                format!("Этап: ошибка: {error}"),
            ));
        }
    }

    (index, module_name, outcome)
}

fn format_module_log(module_name: &str, message: impl AsRef<str>) -> String {
    format!("[{module_name}] {}", message.as_ref())
}

fn run_parallel_update_jobs(
    jobs: Vec<UpdateJob>,
    force_yes: bool,
    log_sender: Sender<String>,
    progress_sender: Option<Sender<ModuleUpdateProgress>>,
    cancellation: UpdateCancellation,
) -> Vec<(usize, String, Result<(), UpdaterError>)> {
    if jobs.is_empty() {
        return Vec::new();
    }

    let worker_count = worker_count_for(jobs.len());
    let jobs = Arc::new(Mutex::new(jobs));
    let (result_tx, result_rx) = mpsc::channel();
    let mut handles = Vec::with_capacity(worker_count);

    for _ in 0..worker_count {
        let jobs = Arc::clone(&jobs);
        let result_sender = result_tx.clone();
        let worker_log_sender = log_sender.clone();
        let worker_progress_sender = progress_sender.clone();
        let worker_cancellation = cancellation.clone();
        handles.push(thread::spawn(move || {
            loop {
                let next_job = {
                    let mut jobs = jobs.lock().expect("update jobs mutex poisoned");
                    if worker_cancellation.is_cancelled() {
                        for queued_job in jobs.drain(..) {
                            cancel_queued_job(
                                queued_job,
                                &worker_log_sender,
                                &worker_progress_sender,
                            );
                        }
                        return;
                    }
                    jobs.pop()
                };

                let Some(job) = next_job else {
                    break;
                };

                let result = run_update_job(
                    job,
                    force_yes,
                    &worker_log_sender,
                    worker_progress_sender.clone(),
                );
                let _ = result_sender.send(result);
            }
        }));
    }
    drop(result_tx);

    let mut results = result_rx.iter().collect::<Vec<_>>();
    for handle in handles {
        handle.join().expect("update worker panicked");
    }
    results.sort_by_key(|(index, _, _)| *index);
    results
}

fn cancel_queued_job(
    job: UpdateJob,
    log_sender: &Sender<String>,
    progress_sender: &Option<Sender<ModuleUpdateProgress>>,
) {
    send_progress(progress_sender, &job.module_name, UpdatePhase::Cancelled);
    let _ = log_sender.send(format_module_log(
        &job.module_name,
        "Этап: отменено до запуска",
    ));
}

fn send_progress(
    progress_sender: &Option<Sender<ModuleUpdateProgress>>,
    module_name: &str,
    phase: UpdatePhase,
) {
    if let Some(sender) = progress_sender {
        let _ = sender.send(ModuleUpdateProgress {
            module_name: module_name.to_owned(),
            phase,
            detail: None,
        });
    }
}

fn worker_count_for(job_count: usize) -> usize {
    if job_count == 0 {
        return 0;
    }

    let available = thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(4);

    job_count.min(available).clamp(1, MAX_PARALLEL_WORKERS)
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
    use crate::model::{ModuleStatus, PackageUpdate};
    use crate::updaters::UpdaterDescriptor;

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

        let snapshot = discovery::scan_updater(&updater, ModuleKind::Tool, false);
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

        let snapshot = discovery::scan_updater(&up_to_date, ModuleKind::Tool, false);
        assert!(snapshot.installed);
        assert!(matches!(snapshot.status, ModuleStatus::UpToDate));

        let with_updates = FakeUpdater {
            name: "fake-with-updates",
            installed: true,
            updates: vec![PackageUpdate::new("pkg", "1.0", "1.1")],
            fail_message: None,
        };

        let snapshot = discovery::scan_updater(&with_updates, ModuleKind::Tool, false);
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

        let snapshot = discovery::scan_updater(&updater, ModuleKind::Tool, false);
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
        assert!(
            logs.iter()
                .any(|line| line.contains("[m1]") && line.contains("модуль не выбран"))
        );
        assert!(
            logs.iter()
                .any(|line| line.contains("[m2]") && line.contains("инструмент не установлен"))
        );
        assert!(
            logs.iter()
                .any(|line| line.contains("[m3]") && line.contains("обновления не требуются"))
        );
        assert!(logs.iter().any(
            |line| line.contains("[missing-updater]") && line.contains("обработчик не найден")
        ));
    }

    #[test]
    fn run_update_job_reports_running_and_completed_progress() {
        let updater: Box<dyn Updater> = Box::new(FakeUpdater {
            name: "fake",
            installed: true,
            updates: vec![PackageUpdate::new("pkg", "1.0", "1.1")],
            fail_message: None,
        });
        let job = UpdateJob {
            index: 0,
            module_name: "fake".to_owned(),
            updater,
            selected_updates: vec![PackageUpdate::new("pkg", "1.0", "1.1")],
        };

        let (log_tx, _log_rx) = std::sync::mpsc::channel::<String>();
        let (progress_tx, progress_rx) = std::sync::mpsc::channel::<ModuleUpdateProgress>();

        let (_, module_name, result) = run_update_job(job, true, &log_tx, Some(progress_tx));

        assert_eq!(module_name, "fake");
        assert!(result.is_ok());

        let phases = progress_rx
            .iter()
            .map(|progress| progress.phase)
            .collect::<Vec<_>>();
        assert_eq!(phases, vec![UpdatePhase::Running, UpdatePhase::Completed]);
    }

    #[test]
    fn cancelled_queued_job_reports_cancelled_progress() {
        let updater: Box<dyn Updater> = Box::new(FakeUpdater {
            name: "fake",
            installed: true,
            updates: Vec::new(),
            fail_message: None,
        });
        let job = UpdateJob {
            index: 0,
            module_name: "fake".to_owned(),
            updater,
            selected_updates: Vec::new(),
        };
        let (log_tx, log_rx) = std::sync::mpsc::channel();
        let (progress_tx, progress_rx) = std::sync::mpsc::channel();

        cancel_queued_job(job, &log_tx, &Some(progress_tx));
        drop(log_tx);

        assert_eq!(progress_rx.recv().unwrap().phase, UpdatePhase::Cancelled);
        assert!(log_rx.recv().unwrap().contains("отменено до запуска"));
    }

    #[test]
    fn worker_count_is_limited() {
        assert_eq!(worker_count_for(0), 0);
        assert!(worker_count_for(1) >= 1);
        assert!(worker_count_for(128) <= MAX_PARALLEL_WORKERS);
    }
}
