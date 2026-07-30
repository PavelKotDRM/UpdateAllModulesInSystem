//! Системные утилиты: проверка прав, доступности команд и поведение консоли в GUI-режиме.

use crate::updater::command_exists;
use anyhow::{Context, Result, bail};

/// Проверяет, запущен ли процесс с правами администратора или `root`.
pub fn is_admin() -> bool {
    is_admin_impl()
}

#[cfg(target_os = "windows")]
fn is_admin_impl() -> bool {
    is_elevated::is_elevated()
}

#[cfg(unix)]
fn is_admin_impl() -> bool {
    unsafe extern "C" {
        fn geteuid() -> u32;
    }

    unsafe { geteuid() == 0 }
}

#[cfg(not(any(target_os = "windows", unix)))]
fn is_admin_impl() -> bool {
    false
}

/// Сообщает, нужно ли предупредить о запуске без повышенных прав.
pub fn should_warn_about_elevation() -> bool {
    !is_admin()
}

/// Повторно запускает текущий процесс с повышенными правами.
///
/// # Errors
/// Возвращает ошибку, если исполняемый файл не найден, средство повышения прав
/// недоступно или новый процесс не удалось запустить.
pub fn restart_elevated() -> Result<()> {
    if is_admin() {
        bail!("приложение уже запущено с повышенными правами");
    }

    restart_elevated_impl()
}

#[cfg(target_os = "windows")]
fn restart_elevated_impl() -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let executable =
        std::env::current_exe().context("не удалось определить путь к исполняемому файлу")?;
    let executable = executable
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let operation = "runas\0".encode_utf16().collect::<Vec<_>>();
    let parameters = elevated_arguments()
        .encode_utf16()
        .chain(Some(0))
        .collect::<Vec<_>>();

    let result = unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            operation.as_ptr(),
            executable.as_ptr(),
            parameters.as_ptr(),
            ptr::null(),
            SW_SHOWNORMAL,
        )
    };

    if result as isize <= 32 {
        bail!(
            "Windows отклонила запуск с правами администратора (код {})",
            result as isize
        );
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn elevated_arguments() -> String {
    let mut arguments = std::env::args_os()
        .skip(1)
        .map(|argument| quote_windows_argument(&argument.to_string_lossy()))
        .collect::<Vec<_>>();
    if arguments.is_empty() {
        arguments.push("--gui".to_owned());
    }
    arguments.join(" ")
}

#[cfg(target_os = "windows")]
fn quote_windows_argument(argument: &str) -> String {
    let mut quoted = String::from("\"");
    let mut backslashes = 0;

    for character in argument.chars() {
        match character {
            '\\' => backslashes += 1,
            '"' => {
                quoted.push_str(&"\\".repeat(backslashes * 2 + 1));
                quoted.push('"');
                backslashes = 0;
            }
            _ => {
                quoted.push_str(&"\\".repeat(backslashes));
                quoted.push(character);
                backslashes = 0;
            }
        }
    }

    quoted.push_str(&"\\".repeat(backslashes * 2));
    quoted.push('"');
    quoted
}

#[cfg(target_os = "linux")]
fn restart_elevated_impl() -> Result<()> {
    use std::process::Command;

    if !command_available("pkexec") {
        bail!("не найден pkexec; установите PolicyKit для запуска GUI с правами root");
    }

    let executable =
        std::env::current_exe().context("не удалось определить путь к исполняемому файлу")?;
    let mut command = Command::new("pkexec");
    command.args([
        "sh",
        "-c",
        "nohup \"$@\" >/dev/null 2>&1 &",
        "update-all-modules-elevated",
    ]);
    command.arg(executable);
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.is_empty() {
        command.arg("--gui");
    } else {
        command.args(arguments);
    }
    if let Ok(current_dir) = std::env::current_dir() {
        command.current_dir(current_dir);
    }

    let status = command.status().context("не удалось запустить pkexec")?;
    if !status.success() {
        bail!("запуск с правами root отменён или отклонён (код {status})");
    }

    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn restart_elevated_impl() -> Result<()> {
    bail!("перезапуск с повышенными правами поддерживается только в Windows и Linux")
}

/// Проверяет наличие исполняемой команды в `PATH`.
pub fn command_available(program: &str) -> bool {
    command_exists(program)
}

/// Скрывает текущее окно консоли Windows при запуске GUI.
///
/// На остальных платформах функция ничего не делает.
pub fn hide_windows_console_if_needed(gui_mode: bool) {
    if !cfg!(target_os = "windows") || !gui_mode {
        return;
    }

    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::System::Console::GetConsoleWindow;
        use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, ShowWindow};

        let window = GetConsoleWindow();
        if !window.is_null() {
            ShowWindow(window, SW_HIDE);
        }
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::quote_windows_argument;

    #[test]
    fn quotes_windows_arguments_for_shell_execute() {
        assert_eq!(quote_windows_argument(""), "\"\"");
        assert_eq!(quote_windows_argument("two words"), "\"two words\"");
        assert_eq!(
            quote_windows_argument(r#"say "hello""#),
            r#""say \"hello\"""#
        );
        assert_eq!(
            quote_windows_argument(r"C:\Program Files\"),
            r#""C:\Program Files\\""#
        );
    }
}
