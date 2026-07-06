use crate::model::{ModuleKind, PackageUpdate};
use crate::system;
use crate::updater::{capture_command, command_exists, heuristic_parse_updates, stream_command, Updater, UpdaterError};
use serde::Deserialize;
use std::sync::mpsc::Sender;

pub struct UpdaterDescriptor {
    pub updater: Box<dyn Updater>,
    pub kind: ModuleKind,
    pub requires_elevation: bool,
}

macro_rules! define_simple_updater {
    ($struct_name:ident, $display_name:expr, $kind:expr, $requires_elevation:expr, $installed_fn:path, $check_fn:path, $apply_fn:path) => {
        pub struct $struct_name;

        impl Updater for $struct_name {
            fn name(&self) -> &'static str {
                $display_name
            }

            fn is_installed(&self) -> bool {
                $installed_fn()
            }

            fn check_updates(&self) -> Result<Vec<PackageUpdate>, UpdaterError> {
                $check_fn()
            }

            fn apply_updates(&self, force_yes: bool, log_sender: &Sender<String>) -> Result<(), UpdaterError> {
                $apply_fn(force_yes, log_sender)
            }
        }

        impl $struct_name {
            pub fn descriptor() -> UpdaterDescriptor {
                UpdaterDescriptor {
                    updater: Box::new(Self),
                    kind: $kind,
                    requires_elevation: $requires_elevation,
                }
            }
        }
    };
}

pub fn registry() -> Vec<UpdaterDescriptor> {
    vec![
        WingetUpdater::descriptor(),
        ChocolateyUpdater::descriptor(),
        AptUpdater::descriptor(),
        DnfUpdater::descriptor(),
        PacmanUpdater::descriptor(),
        BrewUpdater::descriptor(),
        RustupUpdater::descriptor(),
        Msys2Updater::descriptor(),
        PipUpdater::descriptor(),
    ]
}

pub fn lookup_updater(name: &str) -> Option<UpdaterDescriptor> {
    registry().into_iter().find(|descriptor| descriptor.updater.name() == name)
}

fn python_invocation() -> Option<(String, Vec<String>)> {
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

fn run_python_module(args: &[String]) -> Result<crate::updater::CommandOutput, UpdaterError> {
    let (program, mut prefix) = python_invocation().ok_or_else(|| UpdaterError::Message("python не найден".to_owned()))?;
    prefix.push("-m".to_owned());
    prefix.push("pip".to_owned());
    prefix.extend(args.iter().cloned());
    capture_command(&program, &prefix)
}

fn stream_python_module(args: &[String], log_sender: &Sender<String>) -> Result<(), UpdaterError> {
    let (program, mut prefix) = python_invocation().ok_or_else(|| UpdaterError::Message("python не найден".to_owned()))?;
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

fn parse_pip_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    #[derive(Debug, Deserialize)]
    struct PipPackage {
        name: String,
        version: String,
        latest_version: String,
    }

    let output = run_python_module(&["list".to_owned(), "--outdated".to_owned(), "--format=json".to_owned()])?;
    let packages: Vec<PipPackage> = serde_json::from_str(&output.stdout)?;
    Ok(packages
        .into_iter()
        .map(|package| PackageUpdate::new(package.name, package.version, package.latest_version))
        .collect())
}

fn apply_pip_updates(force_yes: bool, log_sender: &Sender<String>) -> Result<(), UpdaterError> {
    let updates = parse_pip_updates()?;
    if updates.is_empty() {
        let _ = log_sender.send("pip: обновления не найдены".to_owned());
        return Ok(());
    }

    let mut args = vec!["install".to_owned(), "--upgrade".to_owned()];
    if force_yes {
        args.push("--no-input".to_owned());
    }
    args.extend(updates.into_iter().map(|update| update.name));
    stream_python_module(&args, log_sender)
}

fn installed_pip() -> bool {
    python_invocation().is_some()
}

fn check_pip_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    parse_pip_updates()
}

fn apply_rustup_updates(_force_yes: bool, log_sender: &Sender<String>) -> Result<(), UpdaterError> {
    let args = vec!["update".to_owned()];
    let output = stream_command("rustup", &args, log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program: "rustup".to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

fn check_rustup_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    if !system::command_available("rustup") {
        return Ok(Vec::new());
    }
    let output = capture_command("rustup", &vec!["check".to_owned()])?;
    Ok(heuristic_parse_updates("rustup", &output.merged_text()))
}

fn installed_rustup() -> bool {
    system::command_available("rustup")
}

fn winget_installed() -> bool {
    cfg!(target_os = "windows") && system::command_available("winget")
}

fn winget_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    if !winget_installed() {
        return Ok(Vec::new());
    }
    let output = capture_command("winget", &vec!["upgrade".to_owned(), "--accept-source-agreements".to_owned()])?;
    Ok(parse_winget_updates(&output.merged_text()))
}

fn winget_apply_updates(force_yes: bool, log_sender: &Sender<String>) -> Result<(), UpdaterError> {
    let mut args = vec!["upgrade".to_owned(), "--all".to_owned(), "--include-unknown".to_owned(), "--accept-source-agreements".to_owned(), "--accept-package-agreements".to_owned()];
    if force_yes {
        args.push("--silent".to_owned());
    }
    let output = stream_command("winget", &args, log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program: "winget".to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

fn chocolatey_installed() -> bool {
    cfg!(target_os = "windows") && system::command_available("choco")
}

fn chocolatey_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command("choco", &vec!["outdated".to_owned(), "--no-color".to_owned(), "--limit-output".to_owned()])?;
    Ok(parse_choco_updates(&output.merged_text()))
}

fn chocolatey_apply_updates(force_yes: bool, log_sender: &Sender<String>) -> Result<(), UpdaterError> {
    let mut args = vec!["upgrade".to_owned(), "all".to_owned()];
    if force_yes {
        args.push("--yes".to_owned());
    }
    let output = stream_command("choco", &args, log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program: "choco".to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

fn apt_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("apt")
}

fn apt_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command("apt", &vec!["list".to_owned(), "--upgradable".to_owned()])?;
    Ok(heuristic_parse_updates("apt", &output.merged_text()))
}

fn apt_apply_updates(force_yes: bool, log_sender: &Sender<String>) -> Result<(), UpdaterError> {
    let mut args = vec!["upgrade".to_owned()];
    if force_yes {
        args.push("-y".to_owned());
    }
    let output = stream_command("apt-get", &args, log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program: "apt-get".to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

fn dnf_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("dnf")
}

fn dnf_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command("dnf", &vec!["check-update".to_owned()])?;
    Ok(heuristic_parse_updates("dnf", &output.merged_text()))
}

fn dnf_apply_updates(force_yes: bool, log_sender: &Sender<String>) -> Result<(), UpdaterError> {
    let mut args = vec!["upgrade".to_owned()];
    if force_yes {
        args.push("-y".to_owned());
    }
    let output = stream_command("dnf", &args, log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program: "dnf".to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

fn pacman_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("pacman")
}

fn pacman_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command("pacman", &vec!["-Qu".to_owned()])?;
    Ok(heuristic_parse_updates("pacman", &output.merged_text()))
}

fn pacman_apply_updates(force_yes: bool, log_sender: &Sender<String>) -> Result<(), UpdaterError> {
    let mut args = vec!["-Syu".to_owned()];
    if force_yes {
        args.push("--noconfirm".to_owned());
    } else {
        args.push("--needed".to_owned());
    }
    let output = stream_command("pacman", &args, log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program: "pacman".to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

fn brew_installed() -> bool {
    system::command_available("brew")
}

fn brew_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command("brew", &vec!["outdated".to_owned()])?;
    Ok(heuristic_parse_updates("brew", &output.merged_text()))
}

fn brew_apply_updates(_force_yes: bool, log_sender: &Sender<String>) -> Result<(), UpdaterError> {
    let output = stream_command("brew", &vec!["upgrade".to_owned()], log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program: "brew".to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

fn msys2_installed() -> bool {
    cfg!(target_os = "windows") && system::command_available("pacman")
}

fn msys2_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command("pacman", &vec!["-Qu".to_owned()])?;
    Ok(heuristic_parse_updates("msys2", &output.merged_text()))
}

fn msys2_apply_updates(force_yes: bool, log_sender: &Sender<String>) -> Result<(), UpdaterError> {
    let mut args = vec!["-Syu".to_owned()];
    if force_yes {
        args.push("--noconfirm".to_owned());
    } else {
        args.push("--needed".to_owned());
    }
    let output = stream_command("pacman", &args, log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(UpdaterError::CommandFailed {
            program: "pacman".to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

define_simple_updater!(WingetUpdater, "winget", ModuleKind::System, true, winget_installed, winget_check_updates, winget_apply_updates);
define_simple_updater!(ChocolateyUpdater, "choco", ModuleKind::System, true, chocolatey_installed, chocolatey_check_updates, chocolatey_apply_updates);
define_simple_updater!(AptUpdater, "apt", ModuleKind::System, true, apt_installed, apt_check_updates, apt_apply_updates);
define_simple_updater!(DnfUpdater, "dnf", ModuleKind::System, true, dnf_installed, dnf_check_updates, dnf_apply_updates);
define_simple_updater!(PacmanUpdater, "pacman", ModuleKind::System, true, pacman_installed, pacman_check_updates, pacman_apply_updates);
define_simple_updater!(BrewUpdater, "brew", ModuleKind::System, true, brew_installed, brew_check_updates, brew_apply_updates);
define_simple_updater!(RustupUpdater, "rustup", ModuleKind::Tool, false, installed_rustup, check_rustup_updates, apply_rustup_updates);
define_simple_updater!(Msys2Updater, "msys2", ModuleKind::Tool, true, msys2_installed, msys2_check_updates, msys2_apply_updates);
define_simple_updater!(PipUpdater, "pip", ModuleKind::Python, false, installed_pip, check_pip_updates, apply_pip_updates);

fn parse_winget_updates(text: &str) -> Vec<PackageUpdate> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("Name") && !line.starts_with("-") && !line.starts_with("----"))
        .filter(|line| !line.to_ascii_lowercase().contains("upgrades available"))
        .filter_map(|line| {
            let cols = split_columns_by_wide_spaces(line);
            if cols.len() < 4 {
                return None;
            }

            let name = cols.first()?.trim();
            let current = cols.get(2)?.trim();
            let available = cols.get(3)?.trim();
            if name.is_empty() || current.is_empty() || available.is_empty() {
                return None;
            }

            Some(PackageUpdate::new(
                format!("winget:{name}"),
                current,
                available,
            ))
        })
        .collect()
}

fn parse_choco_updates(text: &str) -> Vec<PackageUpdate> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("Chocolatey") && !line.starts_with("Outdated Packages"))
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() < 3 {
                return None;
            }

            let name = parts[0].trim();
            let current = parts[1].trim();
            let available = parts[2].trim();
            if name.is_empty() || current.is_empty() || available.is_empty() {
                return None;
            }

            Some(PackageUpdate::new(
                format!("choco:{name}"),
                current,
                available,
            ))
        })
        .collect()
}

fn split_columns_by_wide_spaces(line: &str) -> Vec<String> {
    let mut columns = Vec::new();
    let mut current = String::new();
    let mut spaces = 0usize;

    for ch in line.chars() {
        if ch == ' ' {
            spaces += 1;
            if spaces >= 2 {
                if !current.trim().is_empty() {
                    columns.push(current.trim().to_owned());
                    current.clear();
                }
                continue;
            }
        } else {
            spaces = 0;
        }
        current.push(ch);
    }

    if !current.trim().is_empty() {
        columns.push(current.trim().to_owned());
    }

    columns
}
