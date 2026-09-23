//! Обновляторы системных менеджеров пакетов Unix/Linux и Homebrew.

use crate::model::PackageUpdate;
use crate::system;
use crate::updater::{UpdaterError, capture_command, stream_command};
use crate::updaters::common::{
    check_with_parser, ensure_success, heuristic_check, selected_package_names, stream_checked,
    stream_checked_refs,
};
use crate::updaters::parsers::{
    parse_apt_updates, parse_brew_updates, parse_dnf_updates, parse_flatpak_updates,
    parse_pkcon_updates, parse_snap_updates, parse_zypper_updates,
};
use std::sync::mpsc::Sender;

pub(super) fn apt_installed() -> bool {
    cfg!(target_os = "linux")
        && should_enable_apt(
            system::command_available("apt"),
            system::command_available("apt-get"),
        )
}

fn should_enable_apt(has_apt: bool, has_apt_get: bool) -> bool {
    has_apt && !has_apt_get
}

pub(super) fn apt_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command("apt", &["list".to_owned(), "--upgradable".to_owned()])?;
    ensure_success("apt", &output)?;
    Ok(parse_apt_updates(&output.merged_text(), "apt"))
}

pub(super) fn detect_apt_get_upgrade_subcommand() -> String {
    let candidates = ["dist-upgrade", "full-upgrade", "upgrade"];

    for candidate in candidates {
        let args = vec!["-s".to_owned(), candidate.to_owned()];
        if let Ok(output) = capture_command("apt-get", &args)
            && output.success
        {
            return candidate.to_owned();
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
    ensure_success("apt-get", &output)?;
    Ok(parse_apt_updates(&output.merged_text(), "apt-get"))
}

pub(super) fn apply_apt_get_updates(
    force_yes: bool,
    selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let _ = log_sender.send(
        crate::tr!(
            crate::localization::current_language(),
            updater,
            apt_get_refreshing
        )
        .to_owned(),
    );
    stream_checked_refs("apt-get", &["update"], log_sender)?;

    let subcommand = detect_apt_get_upgrade_subcommand();
    let args = apt_get_update_args(force_yes, selected_updates, &subcommand)?;

    let description = if selected_updates.is_empty() {
        crate::tr!(
            crate::localization::current_language(),
            updater,
            selected_update_mode,
            subcommand = subcommand
        )
    } else {
        crate::tr!(
            crate::localization::current_language(),
            updater,
            updating_selected_packages
        )
        .to_owned()
    };
    let _ = log_sender.send(format!("apt-get: {description}"));

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

fn apt_get_update_args(
    force_yes: bool,
    selected_updates: &[PackageUpdate],
    subcommand: &str,
) -> Result<Vec<String>, UpdaterError> {
    let (mut args, targets) = if selected_updates.is_empty() {
        (vec![subcommand.to_owned()], Vec::new())
    } else {
        (
            vec!["install".to_owned()],
            selected_package_names("apt-get", selected_updates)?,
        )
    };
    if force_yes {
        args.push("-y".to_owned());
    }
    args.extend(targets);
    Ok(args)
}

pub(super) fn apt_apply_updates(
    force_yes: bool,
    selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let (mut args, targets) = if selected_updates.is_empty() {
        (vec!["upgrade".to_owned()], Vec::new())
    } else {
        (
            vec!["install".to_owned(), "--only-upgrade".to_owned()],
            selected_package_names("apt", selected_updates)?,
        )
    };
    if force_yes {
        args.push("-y".to_owned());
    }
    args.extend(targets);
    stream_checked("apt", &args, log_sender)
}

pub(super) fn apt_get_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("apt-get")
}

pub(super) fn apt_get_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let subcommand = detect_apt_get_upgrade_subcommand();
    check_apt_get_updates_with_simulation(&subcommand)
}

pub(super) fn apt_get_apply_updates(
    force_yes: bool,
    selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    apply_apt_get_updates(force_yes, selected_updates, log_sender)
}

pub(super) fn dnf_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("dnf")
}

pub(super) fn dnf_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    check_with_parser("dnf", &["check-update".to_owned()], "dnf", |text| {
        parse_dnf_updates(text, "dnf")
    })
}

pub(super) fn dnf_apply_updates(
    force_yes: bool,
    selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["upgrade".to_owned()];
    if force_yes {
        args.push("-y".to_owned());
    }
    args.extend(selected_package_names("dnf", selected_updates)?);
    stream_checked("dnf", &args, log_sender)
}

pub(super) fn yum_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("yum")
}

pub(super) fn yum_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    check_with_parser("yum", &["check-update".to_owned()], "yum", |text| {
        parse_dnf_updates(text, "yum")
    })
}

pub(super) fn yum_apply_updates(
    force_yes: bool,
    selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["upgrade".to_owned()];
    if force_yes {
        args.push("-y".to_owned());
    }
    args.extend(selected_package_names("yum", selected_updates)?);
    stream_checked("yum", &args, log_sender)
}

pub(super) fn zypper_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("zypper")
}

pub(super) fn zypper_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    check_with_parser(
        "zypper",
        &["list-updates".to_owned()],
        "zypper",
        parse_zypper_updates,
    )
}

pub(super) fn zypper_apply_updates(
    force_yes: bool,
    selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["update".to_owned()];
    if force_yes {
        args.push("--non-interactive".to_owned());
    }
    args.extend(selected_package_names("zypper", selected_updates)?);
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
    let mut args = vec!["-Syu".to_owned(), "--needed".to_owned()];
    if force_yes {
        args.push("--noconfirm".to_owned());
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
    let output = capture_command(
        "flatpak",
        &[
            "remote-ls".to_owned(),
            "--updates".to_owned(),
            "--columns=application,version".to_owned(),
        ],
    )?;
    ensure_success("flatpak", &output)?;
    Ok(parse_flatpak_updates(&output.merged_text()))
}

pub(super) fn flatpak_apply_updates(
    force_yes: bool,
    selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["update".to_owned()];
    if force_yes {
        args.push("-y".to_owned());
    }
    args.extend(selected_package_names("flatpak", selected_updates)?);
    stream_checked("flatpak", &args, log_sender)
}

pub(super) fn snap_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("snap")
}

pub(super) fn snap_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    check_with_parser(
        "snap",
        &["refresh".to_owned(), "--list".to_owned()],
        "snap",
        parse_snap_updates,
    )
}

pub(super) fn snap_apply_updates(
    _force_yes: bool,
    selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["refresh".to_owned()];
    args.extend(selected_package_names("snap", selected_updates)?);
    stream_checked("snap", &args, log_sender)
}

pub(super) fn pkcon_installed() -> bool {
    cfg!(target_os = "linux") && system::command_available("pkcon")
}

pub(super) fn pkcon_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command("pkcon", &["--plain".to_owned(), "get-updates".to_owned()])?;
    ensure_success("pkcon", &output)?;
    Ok(parse_pkcon_updates(&output.merged_text()))
}

pub(super) fn pkcon_apply_updates(
    force_yes: bool,
    selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["--plain".to_owned(), "update".to_owned()];
    if force_yes {
        args.push("-y".to_owned());
    }
    args.extend(selected_package_names("pkcon", selected_updates)?);
    stream_checked("pkcon", &args, log_sender)
}

pub(super) fn brew_installed() -> bool {
    system::command_available("brew")
}

pub(super) fn brew_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command("brew", &["outdated".to_owned(), "--json=v2".to_owned()])?;
    ensure_success("brew", &output)?;
    parse_brew_updates(&output.stdout)
}

pub(super) fn brew_apply_updates(
    _force_yes: bool,
    selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    if selected_updates.is_empty() {
        return stream_checked_refs("brew", &["upgrade"], log_sender);
    }

    let mut formulae = Vec::new();
    let mut casks = Vec::new();
    for update in selected_updates {
        let Some(name) = update.name.strip_prefix("brew:") else {
            return Err(UpdaterError::Message(crate::tr!(
                crate::localization::current_language(),
                updater,
                invalid_package_name,
                manager = "brew",
                package = update.name
            )));
        };
        if update.scope.as_deref() == Some("cask") {
            casks.push(name.to_owned());
        } else {
            formulae.push(name.to_owned());
        }
    }

    for (is_cask, names) in [(false, formulae), (true, casks)] {
        if names.is_empty() {
            continue;
        }
        let mut args = vec!["upgrade".to_owned()];
        if is_cask {
            args.push("--cask".to_owned());
        }
        args.extend(names);
        stream_checked("brew", &args, log_sender)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PackageUpdate;
    use crate::updater::{CommandExecutor, CommandOutput, UpdaterError, with_command_executor};
    use std::sync::{Arc, Mutex, mpsc::Sender};

    type CommandCalls = Arc<Mutex<Vec<(String, Vec<String>)>>>;

    struct FakeExecutor {
        calls: CommandCalls,
    }

    impl CommandExecutor for FakeExecutor {
        fn capture(&self, program: &str, args: &[String]) -> Result<CommandOutput, UpdaterError> {
            self.calls
                .lock()
                .unwrap()
                .push((program.to_owned(), args.to_vec()));
            Ok(CommandOutput {
                stdout: r#"{"formulae":[{"name":"git","installed_versions":["2.45.0"],"current_version":"2.46.0"}],"casks":[]}"#.to_owned(),
                stderr: String::new(),
                exit_code: Some(0),
                success: true,
            })
        }

        fn stream(
            &self,
            _program: &str,
            _args: &[String],
            _log_sender: &Sender<String>,
        ) -> Result<CommandOutput, UpdaterError> {
            unreachable!("brew update check only captures output")
        }
    }

    #[test]
    fn apt_is_only_enabled_as_fallback_for_apt_get() {
        assert!(should_enable_apt(true, false));
        assert!(!should_enable_apt(true, true));
        assert!(!should_enable_apt(false, true));
        assert!(!should_enable_apt(false, false));
    }

    #[test]
    fn apt_get_args_only_add_yes_when_requested_and_target_selected_packages() {
        let selected = vec![
            PackageUpdate::new("apt-get:git", "1.0", "2.0"),
            PackageUpdate::new("apt-get:curl", "1.0", "2.0"),
        ];

        assert_eq!(
            apt_get_update_args(false, &selected, "dist-upgrade").unwrap(),
            vec!["install".to_owned(), "git".to_owned(), "curl".to_owned()]
        );
        assert_eq!(
            apt_get_update_args(true, &selected, "dist-upgrade").unwrap(),
            vec![
                "install".to_owned(),
                "-y".to_owned(),
                "git".to_owned(),
                "curl".to_owned()
            ]
        );
        assert_eq!(
            apt_get_update_args(false, &[], "dist-upgrade").unwrap(),
            vec!["dist-upgrade".to_owned()]
        );
    }

    #[test]
    fn brew_check_uses_json_through_substituted_executor() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let executor = FakeExecutor {
            calls: Arc::clone(&calls),
        };

        let updates = with_command_executor(Arc::new(executor), brew_check_updates).unwrap();

        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].name, "brew:git");
        assert_eq!(
            *calls.lock().unwrap(),
            vec![(
                "brew".to_owned(),
                vec!["outdated".to_owned(), "--json=v2".to_owned()],
            )]
        );
    }
}
