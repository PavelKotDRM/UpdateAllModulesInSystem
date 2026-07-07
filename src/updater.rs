//! Базовые абстракции обновляторов и утилиты запуска внешних команд.

use crate::model::PackageUpdate;
use std::fmt::{Display, Formatter};
use std::io::{self, BufRead, BufReader, Read};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::thread;
use thiserror::Error;

#[cfg(target_os = "windows")]
use encoding_rs::{IBM866, WINDOWS_1251};

/// Трейт, описывающий жизненный цикл обновлятора конкретного менеджера пакетов.
pub trait Updater: Send + Sync {
    /// Возвращает стабильное имя обновлятора.
    fn name(&self) -> &'static str;
    /// Проверяет, доступен ли соответствующий менеджер пакетов в системе.
    fn is_installed(&self) -> bool;
    /// Собирает список доступных обновлений.
    ///
    /// # Returns
    /// Список пакетов для обновления или ошибку проверки.
    ///
    /// # Errors
    /// Возвращает [`UpdaterError`], если вызов менеджера завершился неуспешно
    /// или вывод не удалось разобрать.
    fn check_updates(&self) -> Result<Vec<PackageUpdate>, UpdaterError>;
    /// Применяет обновления для выбранных пакетов.
    ///
    /// # Arguments
    /// * `force_yes` - Признак автоматического подтверждения.
    /// * `selected_updates` - Явно выбранные обновления; пустой список означает
    ///   использование стратегии обновлятора по умолчанию.
    /// * `log_sender` - Канал для отправки строк лога.
    ///
    /// # Returns
    /// `Ok(())` при успешном выполнении.
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

#[derive(Debug, Error)]
/// Типизированные ошибки взаимодействия с внешними менеджерами пакетов.
pub enum UpdaterError {
    #[error("ошибка запуска команды `{program}`: {source}")]
    SpawnError {
        program: String,
        #[source]
        source: io::Error,
    },
    #[error("ошибка чтения вывода команды `{program}`: {source}")]
    StreamError {
        program: String,
        #[source]
        source: io::Error,
    },
    #[error("команда `{program}` завершилась с кодом {code:?}: {stderr}")]
    CommandFailed {
        program: String,
        code: Option<i32>,
        stderr: String,
    },
    #[error("json error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("{0}")]
    Message(String),
}

#[derive(Debug, Clone)]
/// Результат выполнения внешней команды.
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

impl CommandOutput {
    /// Объединяет `stdout` и `stderr` в один текстовый блок.
    ///
    /// # Arguments
    /// Функция не принимает аргументов.
    ///
    /// # Returns
    /// Строку с приоритетом непустых потоков: `stdout`, `stderr` или оба вместе.
    ///
    /// # Panics
    /// Не паникует.
    ///
    /// # Examples
    /// ```rust,ignore
    /// let output = CommandOutput {
    ///     stdout: "ok".into(),
    ///     stderr: String::new(),
    ///     exit_code: Some(0),
    ///     success: true,
    /// };
    /// assert_eq!(output.merged_text(), "ok");
    /// ```
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

/// Проверяет наличие команды в переменной окружения `PATH`.
///
/// # Arguments
/// * `program` - Имя исполняемого файла.
///
/// # Returns
/// `true`, если команда найдена.
///
/// # Panics
/// Не паникует.
///
/// # Examples
/// ```rust,ignore
/// let _ = command_exists("cargo");
/// ```
pub fn command_exists(program: &str) -> bool {
    find_command(program).is_some()
}

/// Ищет полный путь к исполняемому файлу в `PATH`.
///
/// # Arguments
/// * `program` - Имя команды для поиска.
///
/// # Returns
/// `Some(String)` с путем до найденной команды либо `None`.
///
/// # Panics
/// Не паникует.
///
/// # Examples
/// ```rust,ignore
/// let _path = find_command("rustup");
/// ```
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
/// # Arguments
/// * `program` - Имя исполняемой команды.
/// * `args` - Аргументы запуска.
///
/// # Returns
/// Структуру [`CommandOutput`] с потоками и кодом завершения.
///
/// # Errors
/// Возвращает [`UpdaterError::SpawnError`], если процесс не удалось запустить.
///
/// # Panics
/// Не паникует.
///
/// # Examples
/// ```rust,ignore
/// let output = capture_command("rustup", &["--version".to_owned()])?;
/// println!("{}", output.stdout);
/// # Ok::<(), UpdaterError>(())
/// ```
pub fn capture_command(program: &str, args: &[String]) -> Result<CommandOutput, UpdaterError> {
    let output = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|source| UpdaterError::SpawnError {
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
/// # Arguments
/// * `program` - Имя исполняемой команды.
/// * `args` - Аргументы запуска.
/// * `log_sender` - Канал для построчной передачи лога.
///
/// # Returns
/// [`CommandOutput`] после завершения процесса.
///
/// # Errors
/// Возвращает [`UpdaterError`], если запуск, ожидание процесса или чтение
/// потоков завершились неуспешно.
///
/// # Panics
/// Не паникует.
///
/// # Examples
/// ```rust,ignore
/// let (tx, _rx) = std::sync::mpsc::channel::<String>();
/// let _ = stream_command("rustup", &["--version".to_owned()], &tx)?;
/// # Ok::<(), UpdaterError>(())
/// ```
pub fn stream_command(
    program: &str,
    args: &[String],
    log_sender: &Sender<String>,
) -> Result<CommandOutput, UpdaterError> {
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| UpdaterError::SpawnError {
            program: program.to_owned(),
            source,
        })?;

    let stdout = child.stdout.take().ok_or_else(|| UpdaterError::Message(format!(
        "{}: не удалось получить stdout",
        program
    )))?;
    let stderr = child.stderr.take().ok_or_else(|| UpdaterError::Message(format!(
        "{}: не удалось получить stderr",
        program
    )))?;

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

fn pump_stream<R: Read + Send + 'static>(reader: R, sender: Sender<String>, label: &'static str) -> Result<String, UpdaterError> {
    let mut reader = BufReader::new(reader);
    let mut buffer = Vec::new();
    let mut collected = String::new();

    loop {
        buffer.clear();
        let read = reader.read_until(b'\n', &mut buffer).map_err(|source| UpdaterError::StreamError {
            program: label.to_owned(),
            source,
        })?;

        if read == 0 {
            break;
        }

        let text = decode_bytes(&buffer);
        let trimmed = text.trim_end();
        if !trimmed.is_empty() {
            collected.push_str(trimmed);
            collected.push('\n');
            let _ = sender.send(format!("[{label}] {trimmed}"));
        }
    }

    Ok(collected)
}

/// Декодирует байтовый буфер в строку с учетом платформенных кодировок.
///
/// # Arguments
/// * `bytes` - Сырые байты текстового вывода.
///
/// # Returns
/// Декодированную строку (`UTF-8`, на Windows с попыткой `CP1251`/`CP866`,
/// затем lossy fallback).
///
/// # Panics
/// Не паникует.
///
/// # Examples
/// ```rust,ignore
/// let text = decode_bytes("hello".as_bytes());
/// assert_eq!(text, "hello");
/// ```
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

/// Эвристически извлекает список обновлений из текстового вывода менеджера.
///
/// # Arguments
/// * `manager` - Имя менеджера пакетов (используется как префикс имени пакета).
/// * `text` - Исходный текст для разбора.
///
/// # Returns
/// Список распознанных [`PackageUpdate`].
///
/// # Panics
/// Не паникует.
///
/// # Examples
/// ```rust,ignore
/// let updates = heuristic_parse_updates("apt", "foo 1.0 1.1");
/// assert_eq!(updates.len(), 1);
/// ```
pub fn heuristic_parse_updates(manager: &'static str, text: &str) -> Vec<PackageUpdate> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !looks_like_noise(line))
        .filter_map(|line| parse_update_line(manager, line))
        .collect()
}

fn looks_like_noise(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("no packages")
        || lower.contains("no updates")
        || lower.contains("nothing to do")
        || lower.contains("up to date")
        || lower.starts_with("warning")
        || lower.starts_with("error")
        || lower.starts_with("name")
        || lower.starts_with("version")
        || lower.starts_with("package")
        || line.chars().all(|ch| ch == '-' || ch == '=' || ch.is_whitespace())
}

fn parse_update_line(manager: &'static str, line: &str) -> Option<PackageUpdate> {
    let tokens: Vec<&str> = line
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect();

    if tokens.is_empty() {
        return None;
    }

    let name_token = tokens[0].trim_matches(|ch: char| ch == ':' || ch == '|' || ch == ',');
    if name_token.is_empty() {
        return None;
    }

    let current = tokens.get(1).copied().unwrap_or("?");
    let available = tokens.get(2).copied().unwrap_or("?");
    Some(PackageUpdate::new(format!("{manager}:{name_token}"), current, available))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn decode_bytes_reads_utf8_text() {
        let bytes = "Привет".as_bytes();
        assert_eq!(decode_bytes(bytes), "Привет");
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
}
