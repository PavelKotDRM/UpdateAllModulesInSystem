//! Обновляторы Windows-инструментов: `winget`, `choco`, Windows Update и MSYS2.

use crate::model::PackageUpdate;
use crate::system;
use crate::updater::{capture_command, heuristic_parse_updates, stream_command, CommandOutput, UpdaterError};
use crate::updaters::common::stream_checked;
use crate::updaters::parsers::{parse_choco_updates, parse_winget_updates, parse_windows_update_items};
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
    let program =
        powershell_program().ok_or_else(|| UpdaterError::Message("powershell не найден".to_owned()))?;
    let args = vec![
        "-NoProfile".to_owned(),
        "-NonInteractive".to_owned(),
        "-Command".to_owned(),
        script.to_owned(),
    ];
    capture_command(program, &args)
}

pub(super) fn run_powershell_stream(
    script: &str,
    log_sender: &Sender<String>,
) -> Result<CommandOutput, UpdaterError> {
    let program =
        powershell_program().ok_or_else(|| UpdaterError::Message("powershell не найден".to_owned()))?;
    let args = vec![
        "-NoProfile".to_owned(),
        "-NonInteractive".to_owned(),
        "-Command".to_owned(),
        script.to_owned(),
    ];
    stream_command(program, &args, log_sender)
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
    if !windows_update_installed() {
        return Ok(());
    }

    let script = r#"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8
$ErrorActionPreference = 'Stop'
$session = New-Object -ComObject Microsoft.Update.Session
$searcher = $session.CreateUpdateSearcher()
$searchResult = $searcher.Search("IsInstalled=0 and IsHidden=0 and Type='Software'")
if ($searchResult.Updates.Count -eq 0) {
    Write-Output 'Windows Update: обновления не найдены'
    exit 0
}

$selected = @()
if ('__SELECTED_UPDATES_JSON__' -ne '') {
    $selected = '__SELECTED_UPDATES_JSON__' | ConvertFrom-Json
}

$updates = New-Object -ComObject Microsoft.Update.UpdateColl
for ($i = 0; $i -lt $searchResult.Updates.Count; $i++) {
    $candidate = $searchResult.Updates.Item($i)
    if ($selected.Count -eq 0 -or $selected -contains $candidate.Title) {
        [void]$updates.Add($candidate)
    }
}

if ($updates.Count -eq 0) {
    Write-Output 'Windows Update: по текущему выбору обновлений нет'
    exit 0
}

Write-Output ("Windows Update: найдено обновлений: " + $updates.Count)
$downloader = $session.CreateUpdateDownloader()
$downloader.Updates = $updates
[void]$downloader.Download()

$installer = $session.CreateUpdateInstaller()
$installer.Updates = $updates
$installResult = $installer.Install()

$resultCode = [int]$installResult.ResultCode
$resultDescription = switch ($resultCode) {
    0 { 'не начато (NotStarted)' }
    1 { 'в процессе (InProgress)' }
    2 { 'успешно (Succeeded)' }
    3 { 'успешно с ошибками (SucceededWithErrors)' }
    4 { 'ошибка (Failed)' }
    5 { 'прервано (Aborted)' }
    default { 'неизвестный код' }
}
Write-Output ("Windows Update: код результата: " + $resultCode + " (" + $resultDescription + ")")
if ($installResult.RebootRequired) {
    Write-Output 'Windows Update: требуется перезагрузка системы'
}

if ($resultCode -gt 3) {
    [Console]::Error.WriteLine('Windows Update: установка завершилась с ошибкой')
    exit 1
}
exit 0
"#;

    let selected_titles: Vec<String> = selected_updates
        .iter()
        .map(|update| {
            update
                .name
                .strip_prefix("windows-update:")
                .unwrap_or(update.name.as_str())
                .to_owned()
        })
        .collect();
    let selected_json = if selected_titles.is_empty() {
        String::new()
    } else {
        serde_json::to_string(&selected_titles)?
    };
    let script = script.replace("__SELECTED_UPDATES_JSON__", &selected_json.replace('"', "''"));

    let output = run_powershell_stream(&script, log_sender)?;
    if output.success {
        Ok(())
    } else {
        let program = powershell_program().unwrap_or("powershell");
        Err(UpdaterError::CommandFailed {
            program: program.to_owned(),
            code: output.exit_code,
            stderr: output.stderr,
        })
    }
}

pub(super) fn winget_check_updates() -> Result<Vec<PackageUpdate>, UpdaterError> {
    if !winget_installed() {
        return Ok(Vec::new());
    }
    let output = capture_command(
        "winget",
        &["upgrade".to_owned(), "--accept-source-agreements".to_owned()],
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

    for update in selected_updates {
        let name = update
            .name
            .strip_prefix("winget:")
            .unwrap_or(update.name.as_str());
        let mut args = vec![
            "upgrade".to_owned(),
            "--name".to_owned(),
            name.to_owned(),
            "--accept-source-agreements".to_owned(),
            "--accept-package-agreements".to_owned(),
        ];
        if force_yes {
            args.push("--silent".to_owned());
        }
        stream_checked("winget", &args, log_sender)?;
    }
    Ok(())
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
    let output = msys2_capture_pacman(&["-Qu"])?;
    Ok(heuristic_parse_updates("msys2", &output.merged_text()))
}

pub(super) fn msys2_apply_updates(
    force_yes: bool,
    _selected_updates: &[PackageUpdate],
    log_sender: &Sender<String>,
) -> Result<(), UpdaterError> {
    let mut args = vec!["-Syu"];
    if force_yes {
        args.push("--noconfirm");
    } else {
        args.push("--needed");
    }
    let output = msys2_stream_pacman(&args, log_sender)?;
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
