//! Обновляторы Windows-инструментов: `winget`, `choco`, Windows Update и MSYS2.

use crate::model::PackageUpdate;
use crate::system;
use crate::updater::{CommandOutput, UpdaterError, capture_command, find_command, stream_command};
use crate::updaters::common::stream_checked;
use crate::updaters::parsers::{
    parse_choco_updates, parse_msys2_updates, parse_windows_update_items, parse_winget_updates,
};
use std::path::PathBuf;
use std::sync::mpsc::Sender;

pub(super) fn winget_installed() -> bool {
    cfg!(target_os = "windows") && system::command_available("winget")
}

pub(super) fn windows_update_installed() -> bool {
    cfg!(target_os = "windows")
        && (system::command_available("powershell") || system::command_available("pwsh"))
}

pub(super) fn powershell_program() -> Option<&'static str> {
    if system::command_available("powershell") {
        Some("powershell")
    } else if system::command_available("pwsh") {
        Some("pwsh")
    } else {
        None
    }
}

pub(super) fn run_powershell_capture(script: &str) -> Result<CommandOutput, UpdaterError> {
    let program = powershell_program().ok_or_else(|| {
        UpdaterError::Message(
            crate::tr!(
                crate::localization::current_language(),
                updater,
                powershell_missing
            )
            .to_owned(),
        )
    })?;
    let args = vec![
        "-NoProfile".to_owned(),
        "-NonInteractive".to_owned(),
        "-Command".to_owned(),
        script.to_owned(),
    ];
    capture_command(program, &args)
}

pub(super) fn windows_update_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    if !windows_update_installed() {
        return Ok(Vec::new());
    }

    let script = r#"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8
$ErrorActionPreference = 'Stop'
$session = New-Object -ComObject Microsoft.Update.Session
$searcher = $session.CreateUpdateSearcher()
$result = $searcher.Search("IsInstalled=0 and IsHidden=0 and Type='Software'")
if ($result.Updates.Count -eq 0) { Write-Output '[]'; exit 0 }
$items = @()
for ($i = 0; $i -lt $result.Updates.Count; $i++) {
    $u = $result.Updates.Item($i)
    $items += [PSCustomObject]@{ title = $u.Title }
}
$items | ConvertTo-Json -Compress
"#;

    let output = run_powershell_capture(script)?;
    if !output.success {
        let program = powershell_program().unwrap_or("powershell");
        return Err(UpdaterError::CommandFailed {
            program: program.to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        });
    }

    parse_windows_update_items(&output.stdout)
}

pub(super) fn windows_update_apply_updates(
    _force_yes: bool,
    selected_updates: &[PackageUpdate],
    _log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    Err(UpdaterError::Message(crate::tr!(
        crate::localization::current_language(),
        updater,
        windows_update_summary,
        count = selected_updates.len()
    )))
}

pub(super) fn winget_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    if !winget_installed() {
        return Ok(Vec::new());
    }
    let output = capture_command(
        "winget",
        &[
            "upgrade".to_owned(),
            "--accept-source-agreements".to_owned(),
        ],
    )?;
    let updates = parse_winget_updates(&output.merged_text());
    if !output.success && !winget_reports_no_upgrades(&output) {
        return Err(UpdaterError::CommandFailed {
            program: "winget".to_owned(),
            code: output.exit_code,
            stderr: output.merged_text().trim().to_owned(),
        });
    }

    let mut pending_updates = Vec::with_capacity(updates.len());
    for update in updates {
        if !winget_target_version_is_installed(&update)? {
            pending_updates.push(update);
        }
    }
    Ok(pending_updates)
}

fn winget_reports_no_upgrades(output: &CommandOutput) -> bool {
    let text = output.merged_text().to_ascii_lowercase();
    output.exit_code == Some(0x8a15002b_u32 as i32)
        || [
            "no applicable upgrade found",
            "no installed package found matching input criteria",
            "no upgrades available",
        ]
        .iter()
        .any(|message| text.contains(message))
}

fn winget_target_version_is_installed(update: &PackageUpdate) -> Result<bool, UpdaterError> {
    let (display_name, _, _) = winget_target_from_update_name(&update.name);
    if !display_name.contains(&update.current_version) {
        return Ok(false);
    }

    let target_name = display_name.replacen(&update.current_version, &update.available_version, 1);
    if target_name == display_name {
        return Ok(false);
    }

    let output = capture_command(
        "winget",
        &[
            "list".to_owned(),
            "--name".to_owned(),
            target_name.clone(),
            "--exact".to_owned(),
            "--accept-source-agreements".to_owned(),
            "--disable-interactivity".to_owned(),
        ],
    )?;

    Ok(output.success
        && winget_list_contains_version(
            &output.merged_text(),
            &target_name,
            &update.available_version,
        ))
}

fn winget_list_contains_version(text: &str, package_name: &str, version: &str) -> bool {
    text.lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix(package_name))
        .any(|rest| rest.split_whitespace().any(|field| field == version))
}

pub(super) fn winget_apply_updates(
    force_yes: bool,
    selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    if selected_updates.is_empty() {
        let mut args = vec![
            "upgrade".to_owned(),
            "--all".to_owned(),
            "--include-unknown".to_owned(),
            "--accept-source-agreements".to_owned(),
            "--accept-package-agreements".to_owned(),
        ];
        if force_yes {
            args.push("--silent".to_owned());
        }
        return stream_checked("winget", &args, log_sender);
    }

    let mut failed_packages = Vec::new();

    for update in selected_updates {
        let (display_name, target_kind, target_value) =
            winget_target_from_update_name(&update.name);
        let mut args = vec!["upgrade".to_owned()];
        match target_kind {
            WingetTargetKind::Id => {
                args.push("--id".to_owned());
                args.push(target_value.to_owned());
                args.push("--exact".to_owned());
            }
            WingetTargetKind::Name => {
                args.push("--name".to_owned());
                args.push(target_value.to_owned());
            }
        }
        args.extend([
            "--accept-source-agreements".to_owned(),
            "--accept-package-agreements".to_owned(),
        ]);
        if force_yes {
            args.push("--silent".to_owned());
        }

        let output = stream_command("winget", &args, log_sender)?;
        if !output.success {
            let reason = winget_failure_reason(&output, &target_value);
            let _ = log_sender.send(crate::tr!(
                crate::localization::current_language(),
                updater,
                winget_package_update_failed,
                package = display_name,
                reason = reason
            ));
            failed_packages.push(display_name);
        }
    }

    if failed_packages.is_empty() {
        Ok(())
    } else {
        Err(UpdaterError::Message(crate::tr!(
            crate::localization::current_language(),
            updater,
            winget_packages_update_failed,
            count = failed_packages.len(),
            errors = failed_packages.join(", ")
        )))
    }
}

fn winget_failure_reason(output: &CommandOutput, package_id: &str) -> String {
    let merged = output.merged_text();
    let merged_lower = merged.to_ascii_lowercase();

    if output.exit_code == Some(0x80072ee2_u32 as i32)
        || merged_lower.contains("0x80072ee2")
        || merged_lower.contains("internetopenurl() failed")
    {
        return crate::tr!(
            crate::localization::current_language(),
            updater,
            winget_download_timeout,
            package_id = package_id
        );
    }

    if merged_lower.contains("install technology is different") {
        return crate::tr!(
            crate::localization::current_language(),
            updater,
            winget_install_method_changed,
            package_id = package_id
        );
    }

    if !output.stderr.trim().is_empty() {
        return output.stderr.trim().to_owned();
    }

    let stdout = output.stdout.trim();
    if !stdout.is_empty() {
        return crate::tr!(
            crate::localization::current_language(),
            updater,
            command_exit_code_with_output,
            output = stdout,
            code = output
                .exit_code
                .map_or_else(|| "None".to_owned(), |code| code.to_string())
        );
    }

    crate::tr!(
        crate::localization::current_language(),
        updater,
        command_exit_code,
        code = output
            .exit_code
            .map_or_else(|| "None".to_owned(), |code| code.to_string())
    )
}

enum WingetTargetKind {
    Id,
    Name,
}

fn winget_target_from_update_name(update_name: &str) -> (String, WingetTargetKind, String) {
    let raw = update_name
        .strip_prefix("winget:")
        .unwrap_or(update_name)
        .trim();

    if let Some((display, package_id)) = raw.rsplit_once(" | ") {
        let display = display.trim();
        let package_id = package_id.trim();
        if !package_id.is_empty() {
            return (
                display.to_owned(),
                WingetTargetKind::Id,
                package_id.to_owned(),
            );
        }
    }

    (raw.to_owned(), WingetTargetKind::Name, raw.to_owned())
}

pub(super) fn chocolatey_installed() -> bool {
    cfg!(target_os = "windows") && system::command_available("choco")
}

pub(super) fn chocolatey_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command(
        "choco",
        &[
            "outdated".to_owned(),
            "--no-color".to_owned(),
            "--limit-output".to_owned(),
        ],
    )?;
    parse_chocolatey_check_output(&output)
}

fn parse_chocolatey_check_output(
    output: &CommandOutput,
) -> Result<Vec<PackageUpdate>, UpdaterError> {
    let updates = parse_choco_updates(&output.merged_text());
    if output.success || (output.exit_code == Some(2) && !updates.is_empty()) {
        Ok(updates)
    } else {
        Err(UpdaterError::CommandFailed {
            program: "choco".to_owned(),
            code: output.exit_code,
            stderr: output.merged_text().trim().to_owned(),
        })
    }
}

pub(super) fn chocolatey_apply_updates(
    force_yes: bool,
    selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = if selected_updates.is_empty() {
        vec!["upgrade".to_owned(), "all".to_owned()]
    } else {
        let mut values = vec!["upgrade".to_owned()];
        values.extend(selected_updates.iter().map(|update| {
            update
                .name
                .strip_prefix("choco:")
                .unwrap_or(update.name.as_str())
                .to_owned()
        }));
        values
    };
    if force_yes {
        args.push("--yes".to_owned());
    }
    stream_checked("choco", &args, log_sender)
}

pub(super) fn msys2_installed() -> bool {
    cfg!(target_os = "windows")
        && (msys2_bash_path().is_some() || system::command_available("pacman"))
}

pub(super) fn msys2_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    let bash_path = msys2_bash_path().ok_or_else(|| {
        UpdaterError::Message(
            crate::tr!(
                crate::localization::current_language(),
                updater,
                msys2_bash_missing
            )
            .to_owned(),
        )
    })?;
    msys2_check_updates_with_bash(&bash_path)
}

// Keep freshly downloaded sync databases separate from the live installation.
const MSYS2_CHECK_SCRIPT: &str = r#"set -e
export LC_ALL=C
check_db=$(mktemp -d /tmp/update-all-msys2.XXXXXXXX)
trap 'rm -rf -- "$check_db"' EXIT
db_path=$(pacman-conf DBPath)
cp -a -- "${db_path%/}/local" "$check_db/local"
pacman -Sy --dbpath "$check_db" --logfile /dev/null --noconfirm >&2
if updates=$(pacman -Qu --dbpath "$check_db" --color never 2>&1); then
    printf '%s\n' "$updates"
else
    status=$?
    if [ "$status" -eq 1 ] && [ -z "$updates" ]; then
        exit 0
    fi
    printf '%s\n' "$updates" >&2
    exit "$status"
fi
"#;

fn msys2_check_updates_with_bash(bash_path: &str) -> Result<Vec<PackageUpdate>, UpdaterError> {
    let output = capture_command(
        bash_path,
        &["-lc".to_owned(), MSYS2_CHECK_SCRIPT.to_owned()],
    )?;
    parse_msys2_query_output(output)
}

fn parse_msys2_query_output(output: CommandOutput) -> Result<Vec<PackageUpdate>, UpdaterError> {
    if output.success {
        return Ok(parse_msys2_updates(&output.stdout));
    }

    Err(msys2_pacman_error(output))
}

fn msys2_pacman_error(output: CommandOutput) -> UpdaterError {
    let details = output.merged_text();
    UpdaterError::CommandFailed {
        program: "pacman".to_owned(),
        code: output.exit_code,
        stderr: if details.trim().is_empty() {
            crate::tr!(
                crate::localization::current_language(),
                updater,
                pacman_no_details
            )
            .to_owned()
        } else {
            details
        },
    }
}

pub(super) fn msys2_apply_updates(
    force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let args = msys2_update_args(force_yes);
    msys2_apply_with_runner(|| msys2_stream_pacman(&args, log_sender), log_sender)
}

fn msys2_apply_with_runner(
    mut run: impl FnMut() -> Result<CommandOutput, UpdaterError>,
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let output = run()?;
    if !output.success {
        return Err(msys2_pacman_error(output));
    }

    let _ = log_sender.send(
        crate::tr!(
            crate::localization::current_language(),
            updater,
            msys2_finishing_update
        )
        .to_owned(),
    );
    // Core upgrades end the first transaction; use a fresh process for the rest.
    let output = run()?;
    if output.success {
        Ok(())
    } else {
        Err(msys2_pacman_error(output))
    }
}

fn msys2_update_args(force_yes: bool) -> Vec<&'static str> {
    let mut args = vec!["-Syu", "--needed"];
    if force_yes {
        args.push("--noconfirm");
    }
    args
}

fn msys2_root_candidates() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Some(root) = std::env::var_os("MSYS2_ROOT") {
        roots.push(PathBuf::from(root));
    }

    roots.push(PathBuf::from("C:\\msys64"));
    roots.push(PathBuf::from("C:\\tools\\msys64"));
    roots
}

fn msys2_bash_path() -> Option<String> {
    for root in msys2_root_candidates() {
        let candidate = root.join("usr").join("bin").join("bash.exe");
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    let pacman_path = PathBuf::from(find_command("pacman")?);
    let candidate = pacman_path.parent()?.join("bash.exe");
    candidate
        .is_file()
        .then(|| candidate.to_string_lossy().into_owned())
}

fn msys2_stream_pacman(
    args: &[&str],
    log_sender: &Sender<String>,
) -> Result<CommandOutput, UpdaterError> {
    if let Some(bash_path) = msys2_bash_path() {
        let command = if args.is_empty() {
            "exec pacman".to_owned()
        } else {
            format!("exec pacman {}", args.join(" "))
        };
        return stream_command(&bash_path, &["-lc".to_owned(), command], log_sender);
    }

    let fallback_args: Vec<String> = args.iter().map(|value| (*value).to_owned()).collect();
    stream_command("pacman", &fallback_args, log_sender)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::updater::{CommandExecutor, with_command_executor};
    use std::collections::VecDeque;
    use std::sync::Arc;

    struct Msys2CheckExecutor {
        output: CommandOutput,
    }

    impl CommandExecutor for Msys2CheckExecutor {
        fn capture(&self, program: &str, args: &[String]) -> Result<CommandOutput, UpdaterError> {
            assert_eq!(program, "msys2-bash");
            assert_eq!(args, &["-lc".to_owned(), MSYS2_CHECK_SCRIPT.to_owned()]);
            assert!(args[1].contains("cp -a -- \"${db_path%/}/local\""));
            assert!(args[1].contains("pacman -Sy --dbpath \"$check_db\""));
            assert!(args[1].contains("pacman -Qu --dbpath \"$check_db\""));
            Ok(CommandOutput {
                stdout: self.output.stdout.clone(),
                stderr: self.output.stderr.clone(),
                exit_code: self.output.exit_code,
                success: self.output.success,
            })
        }

        fn stream(
            &self,
            _program: &str,
            _args: &[String],
            _log_sender: &Sender<String>,
        ) -> Result<CommandOutput, UpdaterError> {
            unreachable!("update checks do not install packages")
        }
    }

    fn msys2_output(success: bool, stdout: &str, stderr: &str) -> CommandOutput {
        CommandOutput {
            stdout: stdout.to_owned(),
            stderr: stderr.to_owned(),
            exit_code: Some(if success { 0 } else { 1 }),
            success,
        }
    }

    #[test]
    fn msys2_check_reports_core_and_non_core_updates_from_fresh_database() {
        let executor = Msys2CheckExecutor {
            output: msys2_output(
                true,
                "msys2-runtime 3.5.7-1 -> 3.6.0-1\npacman 6.1.0-3 -> 7.0.0-1\nmingw-w64-ucrt-x86_64-gcc 14.2.0-1 -> 15.1.0-1\n",
                ":: Synchronizing package databases...\n msys downloading...",
            ),
        };
        let updates = with_command_executor(Arc::new(executor), || {
            msys2_check_updates_with_bash("msys2-bash")
        })
        .unwrap();

        assert_eq!(updates.len(), 3);
        assert_eq!(updates[0].name, "msys2:msys2-runtime");
        assert_eq!(updates[0].current_version, "3.5.7-1");
        assert_eq!(updates[0].available_version, "3.6.0-1");
        assert_eq!(updates[1].name, "msys2:pacman");
        assert_eq!(updates[2].name, "msys2:mingw-w64-ucrt-x86_64-gcc");
    }

    #[test]
    fn msys2_check_reports_sync_errors_instead_of_stale_updates() {
        let executor = Msys2CheckExecutor {
            output: msys2_output(false, "", "error: failed to synchronize all databases"),
        };
        let error = with_command_executor(Arc::new(executor), || {
            msys2_check_updates_with_bash("msys2-bash")
        })
        .unwrap_err();
        assert!(error.to_string().contains("failed to synchronize"));
    }

    #[test]
    fn msys2_check_accepts_empty_query_after_successful_sync() {
        let output = msys2_output(true, "", ":: Synchronizing package databases...");
        assert!(parse_msys2_query_output(output).unwrap().is_empty());
    }

    #[test]
    fn empty_msys2_check_failure_is_not_treated_as_no_updates() {
        let output = CommandOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: Some(1),
            success: false,
        };

        assert!(parse_msys2_query_output(output).is_err());
    }

    #[test]
    fn failed_pacman_query_preserves_error_output() {
        let output = CommandOutput {
            stdout: "ошибка базы".to_owned(),
            stderr: String::new(),
            exit_code: Some(1),
            success: false,
        };

        let error = parse_msys2_query_output(output).unwrap_err();
        assert!(error.to_string().contains("ошибка базы"));
    }

    #[test]
    fn windows_update_apply_reports_manual_installation_limit() {
        let updates = vec![
            PackageUpdate::new("windows-update:KB1", "installed", "available"),
            PackageUpdate::new("windows-update:KB2", "installed", "available"),
        ];
        let (log_tx, log_rx) = std::sync::mpsc::channel();

        let result = windows_update_apply_updates(true, &updates, &log_tx);
        let message = result.unwrap_err().to_string();
        assert!(message.contains("available update count: 2"));
        assert!(message.contains("Windows Update"));
        assert!(message.contains("manually"));
        assert!(log_rx.try_recv().is_err());
    }

    #[test]
    fn winget_failure_explains_internet_timeout() {
        let output = CommandOutput {
            stdout: "Downloading https://github.com/Kitware/CMake/releases/download/v4.4.3/cmake-4.4.3-windows-x86_64.msi\nAn unexpected error occurred while executing the command:\nInternetOpenUrl() failed.\n0x80072ee2 : unknown error".to_owned(),
            stderr: String::new(),
            exit_code: Some(0x80072ee2_u32 as i32),
            success: false,
        };

        let reason = winget_failure_reason(&output, "Kitware.CMake");

        assert!(reason.contains("download timed out"));
        assert!(reason.contains("proxy"));
        assert!(reason.contains("winget upgrade --id Kitware.CMake --exact"));
        assert!(!reason.contains("unknown error"));
    }

    #[test]
    fn winget_check_distinguishes_no_upgrades_from_command_failure() {
        let no_upgrades = CommandOutput {
            stdout: "No installed package found matching input criteria.".to_owned(),
            stderr: String::new(),
            exit_code: Some(0x8a15002b_u32 as i32),
            success: false,
        };
        assert!(winget_reports_no_upgrades(&no_upgrades));

        let failure = CommandOutput {
            stdout: String::new(),
            stderr: "Failed to update source".to_owned(),
            exit_code: Some(1),
            success: false,
        };
        assert!(!winget_reports_no_upgrades(&failure));
    }

    #[test]
    fn chocolatey_check_accepts_outdated_exit_code_but_reports_failures() {
        let outdated = CommandOutput {
            stdout: "git|2.0|3.0|false".to_owned(),
            stderr: String::new(),
            exit_code: Some(2),
            success: false,
        };
        assert_eq!(parse_chocolatey_check_output(&outdated).unwrap().len(), 1);

        let failure = CommandOutput {
            stdout: String::new(),
            stderr: "Unable to reach repository".to_owned(),
            exit_code: Some(1),
            success: false,
        };
        assert!(parse_chocolatey_check_output(&failure).is_err());
    }

    #[test]
    fn msys2_only_suppresses_prompts_when_force_yes_is_enabled() {
        assert_eq!(msys2_update_args(false), ["-Syu", "--needed"]);
        assert_eq!(msys2_update_args(true), ["-Syu", "--needed", "--noconfirm"]);
    }

    #[test]
    fn msys2_restarts_full_upgrade_after_core_transaction() {
        let (log_tx, log_rx) = std::sync::mpsc::channel();
        let mut outputs = VecDeque::from([
            msys2_output(true, ":: Starting core system upgrade...", ""),
            msys2_output(true, ":: Starting full system upgrade...", ""),
        ]);
        let mut calls = 0;
        msys2_apply_with_runner(
            || {
                calls += 1;
                Ok(outputs.pop_front().unwrap())
            },
            &log_tx,
        )
        .unwrap();
        assert_eq!(calls, 2);
        assert_eq!(log_rx.try_iter().count(), 1);
    }

    #[test]
    fn msys2_does_not_retry_failed_first_transaction() {
        let (log_tx, log_rx) = std::sync::mpsc::channel();
        let mut calls = 0;
        let error = msys2_apply_with_runner(
            || {
                calls += 1;
                Ok(msys2_output(
                    false,
                    "",
                    "error: failed to commit transaction",
                ))
            },
            &log_tx,
        )
        .unwrap_err();
        assert_eq!(calls, 1);
        assert!(error.to_string().contains("failed to commit transaction"));
        assert!(log_rx.try_recv().is_err());
    }

    #[test]
    fn msys2_reports_failure_in_remaining_package_transaction() {
        let (log_tx, _log_rx) = std::sync::mpsc::channel();
        let mut outputs = VecDeque::from([
            msys2_output(true, "", ""),
            msys2_output(false, "", "error: invalid package signature"),
        ]);
        let error =
            msys2_apply_with_runner(|| Ok(outputs.pop_front().unwrap()), &log_tx).unwrap_err();
        assert!(outputs.is_empty());
        assert!(error.to_string().contains("invalid package signature"));
    }

    #[test]
    fn winget_list_detects_target_version_under_another_package_id() {
        let text = "\
Name             Id                Version       Source\n\
------------------------------------------------------\n\
Unity 6000.6.2f1 Unity.Unity.2020 6000.6.2f1     winget\n";

        assert!(winget_list_contains_version(
            text,
            "Unity 6000.6.2f1",
            "6000.6.2f1"
        ));
        assert!(!winget_list_contains_version(
            text,
            "Unity 6000.6.0f1",
            "6000.6.0f1"
        ));
    }
}
