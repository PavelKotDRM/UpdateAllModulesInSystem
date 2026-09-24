//! Общие вспомогательные функции для запуска команд и эвристической проверки.

use crate::model::PackageUpdate;
use crate::updater::{
    CommandOutput, UpdaterError, capture_command, heuristic_parse_updates, stream_command,
};
use std::collections::HashSet;
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

fn ensure_check_success(
    program: &str,
    manager: &str,
    output: &CommandOutput,
    updates: &[PackageUpdate],
) -> Result<(), UpdaterError> {
    if output.success {
        return Ok(());
    }

    let no_pacman_updates = manager == "pacman"
        && output.exit_code == Some(1)
        && output.merged_text().trim().is_empty();
    let updates_available = matches!(manager, "dnf" | "yum" | "rustup")
        && output.exit_code == Some(100)
        && !updates.is_empty();
    if no_pacman_updates || updates_available {
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
    check_with_parser(program, &owned_args, manager, |text| {
        heuristic_parse_updates(manager, text)
    })
}

pub(super) fn check_with_parser(
    program: &str,
    args: &[String],
    manager: &str,
    parse: impl FnOnce(&str) -> Vec<PackageUpdate>,
) -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command(program, args)?;
    let text = output.merged_text();
    let updates = parse(&text);
    ensure_check_success(program, manager, &output, &updates)?;
    Ok(updates)
}

pub(super) fn selected_package_names(
    manager: &str,
    selected_updates: &[PackageUpdate],
) -> Result<Vec<String>, UpdaterError> {
    let prefix = format!("{manager}:");
    let mut seen = HashSet::new();
    selected_updates
        .iter()
        .map(|update| {
            update
                .name
                .strip_prefix(&prefix)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| {
                    UpdaterError::Message(crate::tr!(
                        crate::localization::current_language(),
                        updater,
                        invalid_package_name,
                        manager = manager,
                        package = update.name
                    ))
                })
        })
        .filter(|name| match name {
            Ok(name) => seen.insert(name.clone()),
            Err(_) => true,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{ensure_check_success, selected_package_names};
    use crate::model::PackageUpdate;
    use crate::updater::{CommandOutput, UpdaterError};

    fn output(exit_code: i32, success: bool, text: &str) -> CommandOutput {
        CommandOutput {
            stdout: text.to_owned(),
            stderr: String::new(),
            exit_code: Some(exit_code),
            success,
        }
    }

    #[test]
    fn check_errors_are_not_treated_as_no_updates() {
        let failure = output(1, false, "Error: unable to reach repository");

        assert!(matches!(
            ensure_check_success("dnf", "dnf", &failure, &[]),
            Err(UpdaterError::CommandFailed { .. })
        ));
    }

    #[test]
    fn dnf_and_yum_update_exit_codes_require_parseable_updates() {
        let update = PackageUpdate::new("dnf:git.x86_64", "installed", "2.0");
        let available = output(100, false, "git.x86_64 2.0 updates");
        assert!(
            ensure_check_success("dnf", "dnf", &available, std::slice::from_ref(&update)).is_ok()
        );
        assert!(
            ensure_check_success("yum", "yum", &available, std::slice::from_ref(&update)).is_ok()
        );

        let unparsable = output(100, false, "");
        assert!(ensure_check_success("dnf", "dnf", &unparsable, &[]).is_err());
    }

    #[test]
    fn rustup_update_exit_code_requires_parseable_updates() {
        let update = PackageUpdate::new(
            "rustup:nightly-x86_64-pc-windows-msvc",
            "1.100.0-nightly",
            "1.100.0-nightly",
        );
        let available = output(
            100,
            false,
            "nightly-x86_64-pc-windows-msvc - update available: 1.100.0-nightly -> 1.100.0-nightly",
        );

        assert!(
            ensure_check_success(
                "rustup",
                "rustup",
                &available,
                std::slice::from_ref(&update)
            )
            .is_ok()
        );

        let unparsable = output(100, false, "");
        assert!(ensure_check_success("rustup", "rustup", &unparsable, &[]).is_err());
    }

    #[test]
    fn pacman_empty_exit_one_is_a_successful_no_update_result() {
        let no_updates = output(1, false, "");
        assert!(ensure_check_success("pacman", "pacman", &no_updates, &[]).is_ok());

        let failure = output(1, false, "error: failed to synchronize databases");
        assert!(ensure_check_success("pacman", "pacman", &failure, &[]).is_err());
    }

    #[test]
    fn selected_package_names_validate_prefix_and_deduplicate_targets() {
        let updates = vec![
            PackageUpdate::new("apt:git", "1.0", "2.0"),
            PackageUpdate::new("apt:git", "1.0", "2.0"),
            PackageUpdate::new("apt:curl", "1.0", "2.0"),
        ];

        assert_eq!(
            selected_package_names("apt", &updates).unwrap(),
            vec!["git".to_owned(), "curl".to_owned()]
        );
        assert!(
            selected_package_names("apt", &[PackageUpdate::new("other:git", "1.0", "2.0")])
                .is_err()
        );
    }
}
