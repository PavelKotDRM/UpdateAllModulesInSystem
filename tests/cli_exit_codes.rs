//! Интеграционные проверки кодов завершения CLI.

use std::process::Command;

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_update_all_modules"))
}

#[test]
fn help_exits_successfully() {
    let output = binary().arg("--help").output().unwrap();

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Использование:"));
}

#[test]
fn unknown_only_module_exits_with_failure() {
    let output = binary()
        .args(["--check", "--only", "definitely-unknown-module"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("неизвестные имена модулей"));
    assert!(stderr.contains("definitely-unknown-module"));
}
