//! Обновляторы для Python-экосистемы (`pip`, `uv`) и `rustup`.

use crate::model::PackageUpdate;
use crate::system;
use crate::updater::{
    CommandOutput, UpdaterError, capture_command, command_exists, stream_command,
};
use crate::updaters::common::{heuristic_check, stream_checked, stream_checked_refs};
use serde::Deserialize;
use std::sync::mpsc::Sender;

pub(super) fn python_invocation() -> Option<(String, Vec<String>)> {
    if command_exists("python") {
        Some(("python".to_owned(), Vec::new()))
    } else if command_exists("python3") {
        Some(("python3".to_owned(), Vec::new()))
    } else if command_exists("py") {
        Some(("py".to_owned(), vec!["-3".to_owned()]))
    } else {
        None
    }
}

pub(super) fn run_python_module(args: &[String]) -> Result<CommandOutput, UpdaterError> {
    let (program, mut prefix) =
        python_invocation().ok_or_else(|| UpdaterError::Message("python не найден".to_owned()))?;
    prefix.push("-m".to_owned());
    prefix.push("pip".to_owned());
    prefix.extend(args.iter().cloned());
    capture_command(&program, &prefix)
}

pub(super) fn stream_python_module(
    args: &[String],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let (program, mut prefix) =
        python_invocation().ok_or_else(|| UpdaterError::Message("python не найден".to_owned()))?;
    prefix.push("-m".to_owned());
    prefix.push("pip".to_owned());
    prefix.extend(args.iter().cloned());
    let output = stream_command(&program, &prefix, log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program,
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

pub(super) fn parse_pip_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    #[derive(Debug, Deserialize)]
    struct PipPackage {
        name: String,
        version: String,
        latest_version: String,
    }

    let output = run_python_module(&[
        "list".to_owned(),
        "--outdated".to_owned(),
        "--format=json".to_owned(),
    ])?;
    let packages: Vec<PipPackage> = serde_json::from_str(&output.stdout)?;
    Ok(packages
        .into_iter()
        .map(|package| PackageUpdate::new(package.name, package.version, package.latest_version))
        .collect())
}

pub(super) fn apply_pip_updates(
    force_yes: bool,
    selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let updates = if selected_updates.is_empty() {
        match parse_pip_updates() {
            Ok(values) => values,
            Err(error) if uv_installed() => {
                let _ = log_sender.send(format!(
                    "pip: не удалось получить список через python -m pip ({error}); пробую uv"
                ));
                parse_uv_updates()?
            }
            Err(error) => return Err(error),
        }
    } else {
        selected_updates.to_vec()
    };

    if updates.is_empty() {
        let _ = log_sender.send("pip: обновления не найдены".to_owned());
        return Ok(());
    }

    let mut args = vec!["install".to_owned(), "--upgrade".to_owned()];
    if force_yes {
        args.push("--no-input".to_owned());
    }
    args.extend(updates.into_iter().map(|update| update.name));

    match stream_python_module(&args, log_sender) {
        Ok(()) => Ok(()),
        Err(error) if uv_installed() => {
            let _ = log_sender.send(format!(
                "pip: обновление через python -m pip завершилось ошибкой ({error}); пробую uv"
            ));
            // uv pip не понимает флаг pip `--no-input`
            let uv_args: Vec<String> = args
                .iter()
                .filter(|arg| arg.as_str() != "--no-input")
                .cloned()
                .collect();
            run_uv_pip_stream(&uv_args, log_sender)
        }
        Err(error) => Err(error),
    }
}

pub(super) fn installed_pip() -> bool {
    python_invocation().is_some()
}

pub(super) fn check_pip_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    match parse_pip_updates() {
        Ok(updates) => Ok(updates),
        Err(_) if uv_installed() => parse_uv_updates(),
        Err(error) => Err(error),
    }
}

pub(super) fn uv_installed() -> bool {
    system::command_available("uv")
}

pub(super) fn run_uv_self_capture(args: &[String]) -> Result<CommandOutput, UpdaterError> {
    let mut command_args = vec!["self".to_owned()];
    command_args.extend(args.iter().cloned());
    capture_command("uv", &command_args)
}

pub(super) fn run_uv_self_stream(
    args: &[String],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut command_args = vec!["self".to_owned()];
    command_args.extend(args.iter().cloned());
    stream_checked("uv", &command_args, log_sender)
}

pub(super) fn run_uv_pip_capture(args: &[String]) -> Result<CommandOutput, UpdaterError> {
    let mut command_args = vec!["pip".to_owned()];
    command_args.extend(args.iter().cloned());
    capture_command("uv", &command_args)
}

pub(super) fn run_uv_pip_stream(
    args: &[String],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut command_args = vec!["pip".to_owned()];
    command_args.extend(args.iter().cloned());
    stream_checked("uv", &command_args, log_sender)
}

pub(super) fn parse_uv_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    #[derive(Debug, Deserialize)]
    struct UvPackage {
        name: String,
        version: String,
        latest_version: String,
    }

    let output = run_uv_pip_capture(&[
        "list".to_owned(),
        "--outdated".to_owned(),
        "--format=json".to_owned(),
    ])?;
    let packages: Vec<UvPackage> = serde_json::from_str(&output.stdout)?;
    Ok(packages
        .into_iter()
        .map(|package| PackageUpdate::new(package.name, package.version, package.latest_version))
        .collect())
}

pub(super) fn check_uv_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let current = current_uv_version()?;
    let output = run_uv_self_capture(&["update".to_owned(), "--dry-run".to_owned()])?;
    let versions = extract_uv_versions(&output.merged_text());

    if versions.is_empty() {
        return Ok(Vec::new());
    }

    if versions.len() == 1 {
        if versions[0] == current {
            return Ok(Vec::new());
        }

        return Ok(vec![PackageUpdate::new("uv", current, versions[0].clone())]);
    }

    let available_version = versions
        .into_iter()
        .rev()
        .find(|version| version != &current)
        .unwrap_or_else(|| current.clone());

    if available_version == current {
        return Ok(Vec::new());
    }

    Ok(vec![PackageUpdate::new("uv", current, available_version)])
}

pub(super) fn apply_uv_updates(
    force_yes: bool,
    selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    if selected_updates.is_empty() && check_uv_updates()?.is_empty() {
        let _ = log_sender.send("uv: обновления не найдены".to_owned());
        return Ok(());
    }

    // `uv self update` неинтерактивна и не поддерживает флаги подтверждения
    let _ = force_yes;
    let args = vec!["update".to_owned()];

    run_uv_self_stream(&args, log_sender)
}

fn current_uv_version() -> Result<String, UpdaterError> {
    let output = run_uv_self_capture(&["version".to_owned(), "--short".to_owned()])?;
    let version = output.stdout.trim();
    if version.is_empty() {
        return Err(UpdaterError::Message(
            "uv: не удалось определить текущую версию".to_owned(),
        ));
    }

    Ok(version.trim_start_matches('v').to_owned())
}

fn extract_uv_versions(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|token| {
            token.trim_matches(|ch: char| {
                !ch.is_ascii_alphanumeric() && ch != '.' && ch != '-' && ch != '+'
            })
        })
        .filter(|token| {
            token.starts_with('v')
                && token.len() > 1
                && token[1..].chars().any(|ch| ch.is_ascii_digit())
        })
        .map(|token| token.trim_start_matches('v').to_owned())
        .fold(Vec::new(), |mut versions, version| {
            if !version.is_empty() && !versions.iter().any(|existing| existing == &version) {
                versions.push(version);
            }
            versions
        })
}

pub(super) fn apply_rustup_updates(
    _force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    stream_checked_refs("rustup", &["update"], log_sender)
}

pub(super) fn check_rustup_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    if !system::command_available("rustup") {
        return Ok(Vec::new());
    }
    heuristic_check("rustup", &["check"], "rustup")
}

pub(super) fn installed_rustup() -> bool {
    system::command_available("rustup")
}

#[cfg(test)]
mod tests {
    use super::extract_uv_versions;

    #[test]
    fn extract_uv_versions_reads_update_dry_run_output() {
        let text = "info: Checking for updates...\nsuccess: You're already on version v0.11.28 of uv (the latest version).";

        let versions = extract_uv_versions(text);
        assert_eq!(versions, vec!["0.11.28".to_owned()]);
    }

    #[test]
    fn extract_uv_versions_keeps_distinct_versions_in_order() {
        let text = "info: update available v0.11.27 -> v0.11.28";

        let versions = extract_uv_versions(text);
        assert_eq!(versions, vec!["0.11.27".to_owned(), "0.11.28".to_owned()]);
    }
}
