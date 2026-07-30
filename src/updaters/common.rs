//! Общие вспомогательные функции для запуска команд и эвристической проверки.

use crate::model::PackageUpdate;
use crate::updater::{
    CommandOutput, UpdaterError, capture_command, heuristic_parse_updates, stream_command,
};
use std::sync::mpsc::Sender;

fn command_failed(program: &str, output: &CommandOutput) -> UpdaterError {
    UpdaterError::CommandFailed {
        program: program.to_owned(),
        code: output.exit_code,
        stderr: output.merged_text().trim().to_owned(),
    }
}

/// Проверяет успешность завершения уже выполненной внешней команды.
pub(super) fn ensure_success(program: &str, output: &CommandOutput) -> Result<(), UpdaterError> {
    if output.success {
        Ok(())
    } else {
        Err(command_failed(program, output))
    }
}

/// Выполняет команду с потоковым логом и требует нулевой код завершения.
pub(super) fn stream_checked(
    program: &str,
    args: &[String],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let output = stream_command(program, args, log_sender)?;
    ensure_success(program, &output)
}

/// Версия [`stream_checked`] для статического массива строковых аргументов.
pub(super) fn stream_checked_refs(
    program: &str,
    args: &[&str],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let owned_args = args
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    stream_checked(program, &owned_args, log_sender)
}

/// Запускает команду и разбирает её вывод общей эвристикой менеджеров пакетов.
///
/// Ненулевой код завершения не считается ошибкой автоматически: некоторые
/// команды проверки используют его для обозначения доступных обновлений.
pub(super) fn heuristic_check(
    program: &str,
    args: &[&str],
    manager: &'static str,
) -> Result<Vec<PackageUpdate>, UpdaterError> {
    let owned_args = args
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    let output = capture_command(program, &owned_args)?;
    Ok(heuristic_parse_updates(manager, &output.merged_text()))
}
