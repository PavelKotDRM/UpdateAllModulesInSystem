//! Базовые абстракции обновляторов и утилиты запуска внешних команд.

mod parsing;

pub use parsing::heuristic_parse_updates;

use crate::model::PackageUpdate;
use std::cell::RefCell;
use std::collections::HashSet;
use std::fmt::{Display, Formatter};
use std::io::{self, Read};
use std::process::{Command, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
};
use std::thread;
use thiserror::Error;

#[cfg(target_os = "windows")]
use encoding_rs::{IBM866, WINDOWS_1251};

thread_local! {
    static UPDATE_CANCELLATION: RefCell<Option<UpdateCancellation>> = const { RefCell::new(None) };
    static COMMAND_EXECUTOR: RefCell<Option<Arc<dyn CommandExecutor>>> = const { RefCell::new(None) };
}

#[derive(Default)]
struct CancellationState {
    cancelled: AtomicBool,
    active_processes: Mutex<HashSet<u32>>,
}

/// Потокобезопасный сигнал отмены очереди и активных дочерних процессов.
#[derive(Clone, Default)]
pub struct UpdateCancellation {
    state: Arc<CancellationState>,
}

impl UpdateCancellation {
    /// Запрашивает остановку очереди и завершает зарегистрированные процессы.
    pub fn cancel(&self) {
        self.state.cancelled.store(true, Ordering::Release);
        let active_processes = self
            .state
            .active_processes
            .lock()
            .expect("active processes mutex poisoned");
        for &process_id in active_processes.iter() {
            terminate_process(process_id);
        }
    }

    /// Возвращает `true`, если пользователь запросил отмену.
    pub fn is_cancelled(&self) -> bool {
        self.state.cancelled.load(Ordering::Acquire)
    }

    fn register_process(&self, process_id: u32) -> ProcessRegistration {
        let mut active_processes = self
            .state
            .active_processes
            .lock()
            .expect("active processes mutex poisoned");
        if self.is_cancelled() {
            terminate_process(process_id);
        } else {
            active_processes.insert(process_id);
        }
        ProcessRegistration {
            cancellation: self.clone(),
            process_id,
        }
    }
}

struct ProcessRegistration {
    cancellation: UpdateCancellation,
    process_id: u32,
}

impl Drop for ProcessRegistration {
    fn drop(&mut self) {
        self.cancellation
            .state
            .active_processes
            .lock()
            .expect("active processes mutex poisoned")
            .remove(&self.process_id);
    }
}

/// Выполняет операцию, связывая запускаемые ею процессы с сигналом отмены.
pub fn with_update_cancellation<T>(
    cancellation: UpdateCancellation,
    operation: impl FnOnce() -> T,
) -> T {
    UPDATE_CANCELLATION.with(|active| {
        let previous = active.replace(Some(cancellation));
        let result = operation();
        active.replace(previous);
        result
    })
}

fn register_active_process(process_id: u32) -> Option<ProcessRegistration> {
    UPDATE_CANCELLATION.with(|active| {
        active
            .borrow()
            .as_ref()
            .map(|cancellation| cancellation.register_process(process_id))
    })
}

#[cfg(target_os = "windows")]
fn terminate_process(process_id: u32) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};

    let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, process_id) };
    if !handle.is_null() {
        unsafe {
            TerminateProcess(handle, 1);
            CloseHandle(handle);
        }
    }
}

#[cfg(unix)]
fn terminate_process(process_id: u32) {
    unsafe extern "C" {
        fn kill(process_id: i32, signal: i32) -> i32;
    }

    const SIGTERM: i32 = 15;
    let _ = unsafe { kill(process_id as i32, SIGTERM) };
}

#[cfg(not(any(target_os = "windows", unix)))]
fn terminate_process(_process_id: u32) {}

/// Трейт, описывающий жизненный цикл обновлятора конкретного менеджера пакетов.
pub trait Updater: Send + Sync {
    /// Возвращает стабильное имя обновлятора.
    fn name(&self) -> &'static str;
    /// Проверяет, доступен ли соответствующий менеджер пакетов в системе.
    fn is_installed(&self) -> bool;
    /// Собирает список доступных обновлений.
    ///
    /// # Errors
    /// Возвращает [`UpdaterError`], если вызов менеджера завершился неуспешно
    /// или вывод не удалось разобрать.
    fn check_updates(&self) -> Result<Vec<PackageUpdate>, UpdaterError>;
    /// Применяет обновления для выбранных пакетов.
    ///
    /// Пустой `selected_updates` означает использование стратегии обновлятора
    /// по умолчанию; `force_yes` разрешает автоматическое подтверждение.
    ///
    /// # Errors
    /// Возвращает [`UpdaterError`], если команда обновления завершилась ошибкой.
    fn apply_updates(
        &self,
        force_yes: bool,
        selected_updates: &[PackageUpdate],
        log_sender: &Sender<String>,
    ) -> Result<(), UpdaterError>;
}

/// Типизированные ошибки взаимодействия с внешними менеджерами пакетов.
#[derive(Debug, Error)]
pub enum UpdaterError {
    /// Процесс не удалось создать.
    #[error("ошибка запуска команды `{program}`: {source}")]
    SpawnError {
        program: String,
        #[source]
        source: io::Error,
    },
    /// Не удалось прочитать вывод или дождаться завершения процесса.
    #[error("ошибка чтения вывода команды `{program}`: {source}")]
    StreamError {
        program: String,
        #[source]
        source: io::Error,
    },
    /// Команда завершилась ненулевым кодом там, где требуется успех.
    #[error("команда `{program}` завершилась с кодом {code:?}: {stderr}")]
    CommandFailed {
        program: String,
        code: Option<i32>,
        stderr: String,
    },
    /// Менеджер вернул некорректный JSON.
    #[error("json error: {0}")]
    JsonError(#[from] serde_json::Error),
    /// HTTP-запрос завершился ошибкой.
    #[error("http error: {0}")]
    HttpError(#[from] reqwest::Error),
    /// Ошибка, не имеющая более специализированного варианта.
    #[error("{0}")]
    Message(String),
}

/// Результат выполнения внешней команды.
#[derive(Debug, Clone)]
pub struct CommandOutput {
    /// Захваченный `stdout`.
    pub stdout: String,
    /// Захваченный `stderr`.
    pub stderr: String,
    /// Код выхода процесса.
    pub exit_code: Option<i32>,
    /// Признак успешного завершения процесса.
    pub success: bool,
}

/// Исполнитель внешних команд, подменяемый в тестах адаптеров.
pub(crate) trait CommandExecutor: Send + Sync {
    /// Выполняет команду с полным захватом вывода.
    fn capture(&self, program: &str, args: &[String]) -> Result<CommandOutput, UpdaterError>;

    /// Выполняет команду с потоковой передачей вывода.
    fn stream(
        &self,
        program: &str,
        args: &[String],
        log_sender: &Sender<String>,
    ) -> Result<CommandOutput, UpdaterError>;
}

#[cfg(test)]
pub(crate) fn with_command_executor<T>(
    executor: Arc<dyn CommandExecutor>,
    operation: impl FnOnce() -> T,
) -> T {
    COMMAND_EXECUTOR.with(|active| {
        let previous = active.replace(Some(executor));
        let result = operation();
        active.replace(previous);
        result
    })
}

fn command_executor() -> Option<Arc<dyn CommandExecutor>> {
    COMMAND_EXECUTOR.with(|active| active.borrow().clone())
}

impl CommandOutput {
    /// Объединяет непустые `stdout` и `stderr`, сохраняя их порядок.
    pub fn merged_text(&self) -> String {
        if self.stderr.trim().is_empty() {
            return self.stdout.clone();
        }

        if self.stdout.trim().is_empty() {
            return self.stderr.clone();
        }

        format!("{}\n{}", self.stdout, self.stderr)
    }
}

impl Display for CommandOutput {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.merged_text())
    }
}

/// Проверяет наличие исполняемого файла в `PATH`.
pub fn command_exists(program: &str) -> bool {
    find_command(program).is_some()
}

/// Ищет полный путь к исполняемому файлу в `PATH`.
///
/// На Windows учитывает расширения из `PATHEXT`.
pub fn find_command(program: &str) -> Option<String> {
    let command_path = std::env::var_os("PATH")?;
    let path_entries = std::env::split_paths(&command_path);

    #[cfg(target_os = "windows")]
    let extensions: Vec<String> = std::env::var_os("PATHEXT")
        .map(|value| {
            value
                .to_string_lossy()
                .split(';')
                .map(|ext| ext.trim().to_ascii_lowercase())
                .filter(|ext| !ext.is_empty())
                .collect()
        })
        .unwrap_or_else(|| vec![".exe".to_owned(), ".cmd".to_owned(), ".bat".to_owned()]);

    #[cfg(not(target_os = "windows"))]
    let extensions: Vec<String> = vec![String::new()];

    for entry in path_entries {
        for extension in &extensions {
            let candidate = if extension.is_empty() {
                entry.join(program)
            } else {
                entry.join(format!("{program}{extension}"))
            };

            if candidate.is_file() {
                return Some(candidate.to_string_lossy().into_owned());
            }
        }
    }

    None
}

/// Выполняет команду и полностью захватывает вывод потоков.
///
/// Ненулевой код завершения не преобразуется в ошибку: вызывающий код должен
/// проверить [`CommandOutput::success`]. Это необходимо для команд проверки,
/// которые сообщают о доступных обновлениях специальным кодом выхода.
///
/// # Errors
/// Возвращает [`UpdaterError`], если процесс не удалось запустить или дождаться
/// его завершения.
pub fn capture_command(program: &str, args: &[String]) -> Result<CommandOutput, UpdaterError> {
    if let Some(executor) = command_executor() {
        return executor.capture(program, args);
    }

    capture_system_command(program, args)
}

fn capture_system_command(program: &str, args: &[String]) -> Result<CommandOutput, UpdaterError> {
    let executable = find_command(program).unwrap_or_else(|| program.to_owned());
    let child = Command::new(executable)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| UpdaterError::SpawnError {
            program: program.to_owned(),
            source,
        })?;
    let _registration = register_active_process(child.id());
    let output = child
        .wait_with_output()
        .map_err(|source| UpdaterError::StreamError {
            program: program.to_owned(),
            source,
        })?;

    Ok(CommandOutput {
        stdout: decode_bytes(&output.stdout),
        stderr: decode_bytes(&output.stderr),
        exit_code: output.status.code(),
        success: output.status.success(),
    })
}

/// Выполняет команду и потоково пересылает строки вывода в канал логов.
///
/// Как и [`capture_command`], возвращает [`CommandOutput`] при любом коде
/// завершения. `stdout` и `stderr` читаются параллельно, чтобы дочерний процесс
/// не заблокировался при заполнении одного из каналов.
///
/// # Errors
/// Возвращает [`UpdaterError`], если запуск, ожидание процесса или чтение
/// потоков завершились неуспешно.
pub fn stream_command(
    program: &str,
    args: &[String],
    log_sender: &Sender<String>,
) -> Result<CommandOutput, UpdaterError> {
    if let Some(executor) = command_executor() {
        return executor.stream(program, args, log_sender);
    }

    stream_system_command(program, args, log_sender)
}

fn stream_system_command(
    program: &str,
    args: &[String],
    log_sender: &Sender<String>,
) -> Result<CommandOutput, UpdaterError> {
    let executable = find_command(program).unwrap_or_else(|| program.to_owned());
    let mut child = Command::new(executable)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| UpdaterError::SpawnError {
            program: program.to_owned(),
            source,
        })?;
    let _registration = register_active_process(child.id());

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| UpdaterError::Message(format!("{}: не удалось получить stdout", program)))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| UpdaterError::Message(format!("{}: не удалось получить stderr", program)))?;

    let stdout_sender = log_sender.clone();
    let stderr_sender = log_sender.clone();
    let stdout_handle = thread::spawn(move || pump_stream(stdout, stdout_sender, "stdout"));
    let stderr_handle = thread::spawn(move || pump_stream(stderr, stderr_sender, "stderr"));

    let status = child.wait().map_err(|source| UpdaterError::StreamError {
        program: program.to_owned(),
        source,
    })?;

    let stdout_text = stdout_handle
        .join()
        .map_err(|_| UpdaterError::Message(format!("{}: stdout thread panicked", program)))??;
    let stderr_text = stderr_handle
        .join()
        .map_err(|_| UpdaterError::Message(format!("{}: stderr thread panicked", program)))??;

    Ok(CommandOutput {
        stdout: stdout_text,
        stderr: stderr_text,
        exit_code: status.code(),
        success: status.success(),
    })
}

fn pump_stream<R: Read + Send + 'static>(
    mut reader: R,
    sender: Sender<String>,
    label: &'static str,
) -> Result<String, UpdaterError> {
    let mut chunk = [0_u8; 4096];
    let mut pending = Vec::new();
    let mut collected = String::new();

    loop {
        let read = reader
            .read(&mut chunk)
            .map_err(|source| UpdaterError::StreamError {
                program: label.to_owned(),
                source,
            })?;

        if read == 0 {
            break;
        }

        for &byte in &chunk[..read] {
            if byte == b'\n' || byte == b'\r' {
                emit_stream_message(&mut pending, &sender, label, &mut collected);
            } else {
                pending.push(byte);
            }
        }
    }

    emit_stream_message(&mut pending, &sender, label, &mut collected);
    Ok(collected)
}

fn emit_stream_message(
    pending: &mut Vec<u8>,
    sender: &Sender<String>,
    label: &str,
    collected: &mut String,
) {
    if pending.is_empty() {
        return;
    }

    let text = decode_bytes(pending);
    pending.clear();
    let trimmed = text.trim_end();
    if !trimmed.is_empty() {
        collected.push_str(trimmed);
        collected.push('\n');
        let _ = sender.send(format!("[{label}] {trimmed}"));
    }
}

/// Декодирует вывод как UTF-8, а на Windows дополнительно пробует CP1251 и
/// CP866 перед заменой некорректных последовательностей.
pub fn decode_bytes(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }

    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_owned();
    }

    #[cfg(target_os = "windows")]
    {
        let (text, _, had_errors) = WINDOWS_1251.decode(bytes);
        if !had_errors {
            return text.into_owned();
        }

        let (text, _, had_errors) = IBM866.decode(bytes);
        if !had_errors {
            return text.into_owned();
        }
    }

    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    type CommandCalls = Arc<Mutex<Vec<(String, Vec<String>)>>>;

    struct FakeCommandExecutor {
        output: CommandOutput,
        calls: CommandCalls,
    }

    impl CommandExecutor for FakeCommandExecutor {
        fn capture(&self, program: &str, args: &[String]) -> Result<CommandOutput, UpdaterError> {
            self.calls
                .lock()
                .unwrap()
                .push((program.to_owned(), args.to_vec()));
            Ok(self.output.clone())
        }

        fn stream(
            &self,
            program: &str,
            args: &[String],
            log_sender: &Sender<String>,
        ) -> Result<CommandOutput, UpdaterError> {
            let _ = log_sender.send("fake output".to_owned());
            self.capture(program, args)
        }
    }

    #[test]
    fn command_output_merges_streams_correctly() {
        let output = CommandOutput {
            stdout: "line1".to_owned(),
            stderr: "line2".to_owned(),
            exit_code: Some(0),
            success: true,
        };
        assert_eq!(output.merged_text(), "line1\nline2");

        let only_stdout = CommandOutput {
            stdout: "ok".to_owned(),
            stderr: String::new(),
            exit_code: Some(0),
            success: true,
        };
        assert_eq!(only_stdout.merged_text(), "ok");

        let only_stderr = CommandOutput {
            stdout: String::new(),
            stderr: "err".to_owned(),
            exit_code: Some(1),
            success: false,
        };
        assert_eq!(only_stderr.merged_text(), "err");
    }

    #[test]
    fn command_helpers_use_substituted_executor() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let executor = FakeCommandExecutor {
            output: CommandOutput {
                stdout: "captured".to_owned(),
                stderr: String::new(),
                exit_code: Some(0),
                success: true,
            },
            calls: Arc::clone(&calls),
        };
        let (log_sender, log_receiver) = std::sync::mpsc::channel();

        with_command_executor(Arc::new(executor), || {
            let captured = capture_command("tool", &["check".to_owned()]).unwrap();
            let streamed = stream_command("tool", &["update".to_owned()], &log_sender).unwrap();
            assert_eq!(captured.stdout, "captured");
            assert!(streamed.success);
        });

        assert_eq!(log_receiver.recv().unwrap(), "fake output");
        assert_eq!(
            *calls.lock().unwrap(),
            vec![
                ("tool".to_owned(), vec!["check".to_owned()]),
                ("tool".to_owned(), vec!["update".to_owned()]),
            ]
        );
    }

    #[test]
    fn decode_bytes_reads_utf8_text() {
        let bytes = "Привет".as_bytes();
        assert_eq!(decode_bytes(bytes), "Привет");
    }

    #[test]
    fn pump_stream_emits_carriage_return_progress() {
        let output = b"package 10%\rpackage 55%\rpackage 100%\ninstalled\n".to_vec();
        let (sender, receiver) = std::sync::mpsc::channel();

        let collected = pump_stream(std::io::Cursor::new(output), sender, "stdout").unwrap();
        let messages = receiver.iter().collect::<Vec<_>>();

        assert_eq!(
            messages,
            vec![
                "[stdout] package 10%",
                "[stdout] package 55%",
                "[stdout] package 100%",
                "[stdout] installed",
            ]
        );
        assert_eq!(
            collected,
            "package 10%\npackage 55%\npackage 100%\ninstalled\n"
        );
    }

    #[test]
    fn heuristic_parse_updates_skips_noise_and_parses_packages() {
        let text = "\
warning: mirror\n\
package old new\n\
foo 1.0 1.2\n\
bar 2.0 3.0\n\
";

        let updates = heuristic_parse_updates("apt", text);
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].name, "apt:foo");
        assert_eq!(updates[0].current_version, "1.0");
        assert_eq!(updates[0].available_version, "1.2");
        assert_eq!(updates[1].name, "apt:bar");
    }

    #[test]
    fn cancellation_terminates_active_process() {
        let cancellation = UpdateCancellation::default();
        let worker_cancellation = cancellation.clone();
        let handle = thread::spawn(move || {
            with_update_cancellation(worker_cancellation, || {
                #[cfg(target_os = "windows")]
                let (program, args) = (
                    if find_command("powershell").is_some() {
                        "powershell"
                    } else {
                        "pwsh"
                    },
                    vec![
                        "-NoProfile".to_owned(),
                        "-Command".to_owned(),
                        "Start-Sleep -Seconds 30".to_owned(),
                    ],
                );
                #[cfg(unix)]
                let (program, args) = ("sleep", vec!["30".to_owned()]);
                #[cfg(not(any(target_os = "windows", unix)))]
                let (program, args) = ("echo", vec!["unsupported".to_owned()]);

                capture_command(program, &args)
            })
        });

        let deadline = Instant::now() + Duration::from_secs(5);
        while cancellation
            .state
            .active_processes
            .lock()
            .expect("active processes mutex poisoned")
            .is_empty()
        {
            assert!(Instant::now() < deadline, "process was not registered");
            thread::yield_now();
        }

        cancellation.cancel();
        let output = handle
            .join()
            .expect("command thread should not panic")
            .expect("terminated command should still return its output");
        assert!(!output.success);
        assert!(cancellation.is_cancelled());
    }
}
