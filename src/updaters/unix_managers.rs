//! Обновляторы системных менеджеров пакетов Unix/Linux и Homebrew.

use crate::model::PackageUpdate;
use crate::system;
use crate::updater::{UpdaterError, capture_command, heuristic_parse_updates, stream_command};
use crate::updaters::common::{heuristic_check, stream_checked, stream_checked_refs};
use std::sync::mpsc::Sender;

pub(super) fn apt_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("apt")
}

pub(super) fn apt_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    heuristic_check("apt", &["list", "--upgradable"], "apt")
}

pub(super) fn detect_apt_get_upgrade_subcommand() -> String {
    let candidates = ["dist-upgrade", "full-upgrade", "upgrade"];

    for candidate in candidates {
        let args = vec!["-s".to_owned(), candidate.to_owned()];
        if let Ok(output) = capture_command("apt-get", &args) {
            if output.success {
                return candidate.to_owned();
            }
        }
    }

    if let Ok(help) = capture_command("apt-get", &["--help".to_owned()]) {
        let text = help.merged_text().to_ascii_lowercase();
        if text.contains("dist-upgrade") {
            return "dist-upgrade".to_owned();
        }
        if text.contains("full-upgrade") {
            return "full-upgrade".to_owned();
        }
    }

    "upgrade".to_owned()
}

pub(super) fn check_apt_get_updates_with_simulation(
    subcommand: &str,
) -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command("apt-get", &["-s".to_owned(), subcommand.to_owned()])?;
    Ok(heuristic_parse_updates("apt-get", &output.merged_text()))
}

pub(super) fn apply_apt_get_updates(
    force_yes: bool,
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let subcommand = detect_apt_get_upgrade_subcommand();
    let mut args = vec![subcommand.clone()];
    if force_yes {
        args.push("-y".to_owned());
    }

    let _ = log_sender.send(format!("apt-get: выбран режим обновления `{subcommand}`"));

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

pub(super) fn apt_apply_updates(
    force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    apply_apt_get_updates(force_yes, log_sender)
}

pub(super) fn apt_get_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("apt-get")
}

pub(super) fn apt_get_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    if system::command_available("apt") {
        let output = capture_command("apt", &["list".to_owned(), "--upgradable".to_owned()])?;
        return Ok(heuristic_parse_updates("apt-get", &output.merged_text()));
    }

    let subcommand = detect_apt_get_upgrade_subcommand();
    check_apt_get_updates_with_simulation(&subcommand)
}

pub(super) fn apt_get_apply_updates(
    force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    apply_apt_get_updates(force_yes, log_sender)
}

pub(super) fn dnf_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("dnf")
}

pub(super) fn dnf_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    heuristic_check("dnf", &["check-update"], "dnf")
}

pub(super) fn dnf_apply_updates(
    force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["upgrade".to_owned()];
    if force_yes {
        args.push("-y".to_owned());
    }
    stream_checked("dnf", &args, log_sender)
}

pub(super) fn yum_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("yum")
}

pub(super) fn yum_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    heuristic_check("yum", &["check-update"], "yum")
}

pub(super) fn yum_apply_updates(
    force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["upgrade".to_owned()];
    if force_yes {
        args.push("-y".to_owned());
    }
    stream_checked("yum", &args, log_sender)
}

pub(super) fn zypper_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("zypper")
}

pub(super) fn zypper_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    heuristic_check("zypper", &["list-updates"], "zypper")
}

pub(super) fn zypper_apply_updates(
    force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["update".to_owned()];
    if force_yes {
        args.push("--non-interactive".to_owned());
    }
    stream_checked("zypper", &args, log_sender)
}

pub(super) fn pacman_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("pacman")
}

pub(super) fn pacman_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    heuristic_check("pacman", &["-Qu"], "pacman")
}

pub(super) fn pacman_apply_updates(
    force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["-Syu".to_owned()];
    if force_yes {
        args.push("--noconfirm".to_owned());
    } else {
        args.push("--needed".to_owned());
    }
    stream_checked("pacman", &args, log_sender)
}

pub(super) fn apk_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("apk")
}

pub(super) fn apk_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    heuristic_check("apk", &["version", "-l", "<"], "apk")
}

pub(super) fn apk_apply_updates(
    force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["upgrade".to_owned()];
    if force_yes {
        args.push("--no-interactive".to_owned());
    }
    stream_checked("apk", &args, log_sender)
}

pub(super) fn xbps_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("xbps-install")
}

pub(super) fn xbps_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    heuristic_check("xbps-install", &["-Mun"], "xbps")
}

pub(super) fn xbps_apply_updates(
    force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["-Su".to_owned()];
    if force_yes {
        args.push("-y".to_owned());
    }
    stream_checked("xbps-install", &args, log_sender)
}

pub(super) fn emerge_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("emerge")
}

pub(super) fn emerge_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    heuristic_check("emerge", &["-puDN", "@world"], "emerge")
}

pub(super) fn emerge_apply_updates(
    force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["-uDN".to_owned(), "@world".to_owned()];
    if force_yes {
        args.push("--ask=n".to_owned());
    }
    stream_checked("emerge", &args, log_sender)
}

pub(super) fn flatpak_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("flatpak")
}

pub(super) fn flatpak_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    heuristic_check(
        "flatpak",
        &[
            "remote-ls",
            "--updates",
            "--columns=application,installed-version,version",
        ],
        "flatpak",
    )
}

pub(super) fn flatpak_apply_updates(
    force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["update".to_owned()];
    if force_yes {
        args.push("-y".to_owned());
    }
    stream_checked("flatpak", &args, log_sender)
}

pub(super) fn snap_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("snap")
}

pub(super) fn snap_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    heuristic_check("snap", &["refresh", "--list"], "snap")
}

pub(super) fn snap_apply_updates(
    _force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    stream_checked_refs("snap", &["refresh"], log_sender)
}

pub(super) fn pkcon_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("pkcon")
}

pub(super) fn pkcon_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    heuristic_check("pkcon", &["get-updates"], "pkcon")
}

pub(super) fn pkcon_apply_updates(
    force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["update".to_owned()];
    if force_yes {
        args.push("-y".to_owned());
    }
    stream_checked("pkcon", &args, log_sender)
}

pub(super) fn brew_installed() -> bool {
    system::command_available("brew")
}

pub(super) fn brew_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    heuristic_check("brew", &["outdated"], "brew")
}

pub(super) fn brew_apply_updates(
    _force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    stream_checked_refs("brew", &["upgrade"], log_sender)
}
