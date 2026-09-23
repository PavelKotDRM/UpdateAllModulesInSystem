//! Проверка и обновление системных менеджеров пакетов и инструментов разработки.
//!
//! Приложение сканирует зарегистрированные обновляторы параллельно, показывает
//! доступные обновления и запускает выбранные операции. Без аргументов открывается
//! графический интерфейс; переданные CLI-флаги включают консольный режим.
//!
//! # Режимы запуска
//!
//! Проверить обновления без их установки:
//!
//! ```console
//! cargo run --release -- --check
//! ```
//!
//! Запустить GUI явно:
//!
//! ```console
//! cargo run --release -- --gui
//! ```
//!
//! Для просмотра всех фильтров и режимов используйте `--help`.

mod app;
mod build_info;
mod cli;
mod gui;
mod localization;
mod model;
mod repaint;
mod system;
mod updater;
mod updaters;

use anyhow::Result;
use clap::Parser;
use cli::Cli;
use std::collections::BTreeSet;
use std::process::ExitCode;

/// Запускает CLI или GUI в зависимости от аргументов командной строки.
///
/// # Errors
/// Возвращает ошибку запуска GUI либо аварийного завершения управляющего
/// потока обновлений в CLI.
fn main() -> Result<ExitCode> {
    let cli = Cli::parse();
    let language = cli.language.unwrap_or_default();
    localization::with_language(language, || run_application(cli))
}

fn run_application(cli: Cli) -> Result<ExitCode> {
    validate_only_modules(&cli.only)?;
    let raw_args: Vec<String> = std::env::args().collect();
    let gui_mode = cli.gui || raw_args.len() == 1;

    system::hide_windows_console_if_needed(gui_mode);

    let filter = build_selection_filter(&cli);

    if gui_mode {
        gui::launch_gui(
            filter,
            cli.yes,
            cli.language,
            cli.elevation_ready_file,
            cli.elevation_state_file,
        )?;
        return Ok(ExitCode::SUCCESS);
    }

    run_cli(cli, filter)
}

fn validate_only_modules(only: &[String]) -> Result<()> {
    let available = updaters::updater_names().collect::<BTreeSet<_>>();
    let unknown = only
        .iter()
        .filter(|name| !available.contains(name.as_str()))
        .cloned()
        .collect::<BTreeSet<_>>();
    if unknown.is_empty() {
        return Ok(());
    }

    anyhow::bail!(crate::tr!(
        localization::current_language(),
        cli,
        unknown_modules,
        unknown = unknown.into_iter().collect::<Vec<_>>().join(", "),
        available = available.into_iter().collect::<Vec<_>>().join(", ")
    ))
}

/// Преобразует параметры CLI в фильтр реестра обновляторов.
fn build_selection_filter(cli: &Cli) -> app::SelectionFilter {
    app::SelectionFilter {
        skip_system: cli.skip_system,
        skip_pip: cli.skip_pip,
        skip_tools: cli.skip_tools,
        only_tools: cli.only_tools,
        only: cli.only.clone().into_iter().collect::<BTreeSet<_>>(),
    }
}

/// Сканирует модули, печатает таблицу и при необходимости запускает обновления.
///
/// # Errors
/// Возвращает ошибку, если управляющий поток обновлений завершился паникой.
fn run_cli(cli: Cli, filter: app::SelectionFilter) -> Result<ExitCode> {
    let modules = app::discover_modules(&filter);
    let scan_failed = modules
        .iter()
        .any(|module| matches!(module.status, model::ModuleStatus::Error(_)));

    if let Some(warning) = app::summarize_elevation_warning(&modules) {
        eprintln!("{warning}");
    }

    println!("{}", app::render_cli_table(&modules));

    if cli.check {
        return Ok(exit_code_for_failures(scan_failed, false));
    }

    let (log_tx, log_rx) = std::sync::mpsc::channel::<String>();
    let update_modules: Vec<_> = modules
        .into_iter()
        .filter(|module| module.selected)
        .collect();
    let force_yes = cli.yes;
    let verbose = cli.verbose;
    let language = localization::current_language();

    let update_handle = std::thread::spawn(move || {
        localization::with_language(language, || {
            app::run_updates(&update_modules, force_yes, &log_tx)
        })
    });

    while let Ok(message) = log_rx.recv() {
        if verbose {
            println!("{message}");
        }
    }

    let results = update_handle.join().map_err(|_| {
        anyhow::anyhow!(crate::tr!(
            localization::current_language(),
            cli,
            update_worker_panicked
        ))
    })?;

    for (name, result) in &results {
        match result {
            Ok(()) if cli.verbose => println!("{name}: ok"),
            Ok(()) => {}
            Err(error) => eprintln!("{name}: {error}"),
        }
    }

    let update_failed = results.iter().any(|(_, result)| result.is_err());
    Ok(exit_code_for_failures(scan_failed, update_failed))
}

fn exit_code_for_failures(scan_failed: bool, update_failed: bool) -> ExitCode {
    if scan_failed || update_failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::{exit_code_for_failures, validate_only_modules};
    use std::process::ExitCode;

    #[test]
    fn cli_fails_when_scan_or_update_fails() {
        assert_eq!(exit_code_for_failures(true, false), ExitCode::FAILURE);
        assert_eq!(exit_code_for_failures(false, true), ExitCode::FAILURE);
        assert_eq!(exit_code_for_failures(true, true), ExitCode::FAILURE);
    }

    #[test]
    fn cli_succeeds_without_failures() {
        assert_eq!(exit_code_for_failures(false, false), ExitCode::SUCCESS);
    }

    #[test]
    fn validates_only_values_against_updater_registry() {
        assert!(validate_only_modules(&["pip".to_owned(), "rustup".to_owned()]).is_ok());

        let error = validate_only_modules(&[
            "missing-z".to_owned(),
            "pip".to_owned(),
            "missing-a".to_owned(),
        ])
        .unwrap_err()
        .to_string();
        assert!(error.contains("missing-a, missing-z"));
        assert!(error.contains("Available names:"));
        assert!(error.contains("vscode-extensions"));
    }
}
