//! Интеграционные проверки кодов завершения CLI.

use std::process::Command;

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_update_all_modules"))
}

#[test]
fn help_exits_successfully() {
    let output = binary().arg("--help").output().unwrap();

    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("Usage:"));
    assert!(help.contains("Select the application language"));
    assert!(
        !help
            .chars()
            .any(|character| matches!(character, 'А'..='я' | 'Ё' | 'ё'))
    );
}

#[test]
fn unknown_only_module_exits_with_failure() {
    let output = binary()
        .args(["--check", "--only", "definitely-unknown-module"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Unknown module names"));
    assert!(stderr.contains("definitely-unknown-module"));
}

#[test]
fn language_option_selects_russian_cli_messages() {
    let output = binary()
        .args([
            "--language",
            "ru",
            "--check",
            "--only",
            "definitely-unknown-module",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Неизвестные имена модулей"));
    assert!(stderr.contains("Доступные имена"));
}
