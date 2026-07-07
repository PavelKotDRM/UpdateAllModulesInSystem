//! Точка входа приложения: разбор CLI, выбор режима и запуск обновлений.

mod app;
mod cli;
mod gui;
mod model;
mod system;
mod updater;
mod updaters;

use anyhow::Result;
use clap::Parser;
use cli::Cli;
use std::collections::BTreeSet;

/// Запускает приложение в CLI или GUI режиме в зависимости от аргументов.
///
/// # Arguments
/// Функция не принимает аргументов.
///
/// # Returns
/// `Ok(())` при корректном выполнении сценария запуска.
///
/// # Errors
/// Возвращает ошибку запуска GUI или аварии потока обновлений в CLI.
///
/// # Panics
/// Не паникует.
///
/// # Examples
/// ```rust,ignore
/// main()?;
/// # Ok::<(), anyhow::Error>(())
/// ```
fn main() -> Result<()> {
    let cli = Cli::parse();
    let raw_args: Vec<String> = std::env::args().collect();
    let gui_mode = cli.gui || raw_args.len() == 1;

    system::hide_windows_console_if_needed(gui_mode);

    let filter = build_selection_filter(&cli);

    if gui_mode {
        gui::launch_gui(filter, cli.yes)?;
        return Ok(());
    }

    run_cli(cli, filter)
}

/// Строит фильтр выбора модулей на основе параметров CLI.
///
/// # Arguments
/// * `cli` - Распарсенные аргументы командной строки.
///
/// # Returns
/// Инициализированный [`app::SelectionFilter`].
///
/// # Panics
/// Не паникует.
///
/// # Examples
/// ```rust,ignore
/// let filter = build_selection_filter(&cli);
/// println!("{}", filter.only.len());
/// ```
fn build_selection_filter(cli: &Cli) -> app::SelectionFilter {
    app::SelectionFilter {
        skip_system: cli.skip_system,
        skip_pip: cli.skip_pip,
        skip_tools: cli.skip_tools,
        only_tools: cli.only_tools,
        only: cli.only.clone().into_iter().collect::<BTreeSet<_>>(),
    }
}

/// Выполняет CLI-сценарий: сканирование, вывод таблицы и запуск обновлений.
///
/// # Arguments
/// * `cli` - Параметры запуска CLI.
/// * `filter` - Фильтр модулей.
///
/// # Returns
/// `Ok(())`, если сценарий завершился штатно.
///
/// # Errors
/// Возвращает ошибку при панике потока обновления.
///
/// # Panics
/// Не паникует.
///
/// # Examples
/// ```rust,ignore
/// run_cli(cli, filter)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
fn run_cli(cli: Cli, filter: app::SelectionFilter) -> Result<()> {
    let modules = app::discover_modules(&filter);

    if let Some(warning) = app::summarize_elevation_warning(&modules) {
        eprintln!("{warning}");
    }

    println!("{}", app::render_cli_table(&modules));

    if cli.check {
        return Ok(());
    }

    let (log_tx, log_rx) = std::sync::mpsc::channel::<String>();
    let update_modules: Vec<_> = modules.into_iter().filter(|module| module.selected).collect();
    let force_yes = cli.yes;
    let verbose = cli.verbose;

    let update_handle = std::thread::spawn(move || app::run_updates(&update_modules, force_yes, &log_tx));

    while let Ok(message) = log_rx.recv() {
        if verbose {
            println!("{message}");
        }
    }

    let results = update_handle
        .join()
        .map_err(|_| anyhow::anyhow!("поток обновления завершился аварийно"))?;

    for (name, result) in &results {
        match result {
            Ok(()) if cli.verbose => println!("{name}: ok"),
            Ok(()) => {}
            Err(error) => eprintln!("{name}: {error}"),
        }
    }

    Ok(())
}
