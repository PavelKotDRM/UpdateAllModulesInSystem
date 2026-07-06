use crate::model::PackageUpdate;
use crate::system;
use crate::updater::{capture_command, command_exists, stream_command, CommandOutput, UpdaterError};
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
            run_uv_pip_stream(&args, log_sender)
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
    parse_uv_updates()
}

pub(super) fn apply_uv_updates(
    force_yes: bool,
    selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let updates = if selected_updates.is_empty() {
        parse_uv_updates()?
    } else {
        selected_updates.to_vec()
    };

    if updates.is_empty() {
        let _ = log_sender.send("uv: обновления не найдены".to_owned());
        return Ok(());
    }

    let mut args = vec!["install".to_owned(), "--upgrade".to_owned()];
    if force_yes {
        args.push("--no-input".to_owned());
    }
    args.extend(updates.into_iter().map(|update| update.name));
    run_uv_pip_stream(&args, log_sender)
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
