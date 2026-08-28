//! Обновляторы Windows-инструментов: `winget`, `choco`, Windows Update и MSYS2.

use crate::model::PackageUpdate;
use crate::system;
use crate::updater::{CommandOutput, UpdaterError, capture_command, stream_command};
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
    let program = powershell_program()
        .ok_or_else(|| UpdaterError::Message("powershell не найден".to_owned()))?;
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
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let _ = log_sender.send(format!(
        "Windows Update: доступно обновлений: {}. Автоматическая установка отключена; откройте Параметры → Центр обновления Windows и запустите обновление вручную",
        selected_updates.len()
    ));
    Ok(())
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
    Ok(parse_winget_updates(&output.merged_text()))
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
            let _ = log_sender.send(format!(
                "winget: пакет `{display_name}` не обновлен: {reason}"
            ));
            failed_packages.push(display_name);
        }
    }

    if failed_packages.is_empty() {
        Ok(())
    } else {
        Err(UpdaterError::Message(format!(
            "winget: не удалось обновить {} пакетов: {}",
            failed_packages.len(),
            failed_packages.join(", ")
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
        return format!(
            "истекло время ожидания загрузки из Интернета (0x80072EE2); проверьте соединение, прокси или доступ к адресу загрузки и повторите `winget upgrade --id {package_id} --exact`"
        );
    }

    if merged_lower.contains("install technology is different") {
        return format!(
            "технология установки новой версии отличается от установленной; выполните `winget uninstall --id {package_id} --exact`, затем `winget install --id {package_id} --exact`"
        );
    }

    if !output.stderr.trim().is_empty() {
        return output.stderr.trim().to_owned();
    }

    let stdout = output.stdout.trim();
    if !stdout.is_empty() {
        return format!("{stdout} (код {:?})", output.exit_code);
    }

    format!("код завершения {:?}", output.exit_code)
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
    Ok(parse_choco_updates(&output.merged_text()))
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
    let sync_output = msys2_capture_pacman(&["-Sy", "--noconfirm"])?;
    ensure_msys2_pacman_success(sync_output)?;

    let output = msys2_capture_pacman(&["-Qu"])?;
    parse_msys2_query_output(output)
}

fn parse_msys2_query_output(output: CommandOutput) -> Result<Vec<PackageUpdate>, UpdaterError> {
    if output.success {
        return Ok(parse_msys2_updates(&output.stdout));
    }

    if output.exit_code == Some(1) && output.merged_text().trim().is_empty() {
        return Ok(Vec::new());
    }

    Err(msys2_pacman_error(output))
}

fn ensure_msys2_pacman_success(output: CommandOutput) -> Result<CommandOutput, UpdaterError> {
    if output.success {
        Ok(output)
    } else {
        Err(msys2_pacman_error(output))
    }
}

fn msys2_pacman_error(output: CommandOutput) -> UpdaterError {
    let details = output.merged_text();
    UpdaterError::CommandFailed {
        program: "pacman".to_owned(),
        code: output.exit_code,
        stderr: if details.trim().is_empty() {
            "pacman не сообщил подробностей".to_owned()
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
    let mut args = vec!["-Syu", "--noconfirm"];
    if !force_yes {
        args.push("--needed");
    }
    let output = msys2_stream_pacman(&args, log_sender)?;
    if output.success {
        Ok(())
    } else {
        Err(msys2_pacman_error(output))
    }
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
    None
}

fn msys2_capture_pacman(args: &[&str]) -> Result<CommandOutput, UpdaterError> {
    if let Some(bash_path) = msys2_bash_path() {
        let command = if args.is_empty() {
            "pacman".to_owned()
        } else {
            format!("pacman {}", args.join(" "))
        };
        return capture_command(&bash_path, &["-lc".to_owned(), command]);
    }

    let fallback_args: Vec<String> = args.iter().map(|value| (*value).to_owned()).collect();
    capture_command("pacman", &fallback_args)
}

fn msys2_stream_pacman(
    args: &[&str],
    log_sender: &Sender<String>,
) -> Result<CommandOutput, UpdaterError> {
    if let Some(bash_path) = msys2_bash_path() {
        let command = if args.is_empty() {
            "pacman".to_owned()
        } else {
            format!("pacman {}", args.join(" "))
        };
        return stream_command(&bash_path, &["-lc".to_owned(), command], log_sender);
    }

    let fallback_args: Vec<String> = args.iter().map(|value| (*value).to_owned()).collect();
    stream_command("pacman", &fallback_args, log_sender)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_pacman_query_exit_one_means_no_updates() {
        let output = CommandOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: Some(1),
            success: false,
        };

        let updates = parse_msys2_query_output(output).unwrap();
        assert!(updates.is_empty());
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
    fn windows_update_apply_only_recommends_windows_update_center() {
        let updates = vec![
            PackageUpdate::new("windows-update:KB1", "installed", "available"),
            PackageUpdate::new("windows-update:KB2", "installed", "available"),
        ];
        let (log_tx, log_rx) = std::sync::mpsc::channel();

        let result = windows_update_apply_updates(true, &updates, &log_tx);

        assert!(result.is_ok());
        let message = log_rx.recv().expect("recommendation should be logged");
        assert!(message.contains("доступно обновлений: 2"));
        assert!(message.contains("Центр обновления Windows"));
        assert!(message.contains("вручную"));
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

        assert!(reason.contains("истекло время ожидания"));
        assert!(reason.contains("прокси"));
        assert!(reason.contains("winget upgrade --id Kitware.CMake --exact"));
        assert!(!reason.contains("unknown error"));
    }
}
