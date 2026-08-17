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
mod model;
mod repaint;
mod system;
mod updater;
mod updaters;

use anyhow::Result;
use clap::Parser;
use cli::Cli;
use std::collections::BTreeSet;

/// Запускает CLI или GUI в зависимости от аргументов командной строки.
///
/// # Errors
/// Возвращает ошибку запуска GUI либо аварийного завершения управляющего
/// потока обновлений в CLI.
fn main() -> Result<()> {
    let cli = Cli::parse();
    let raw_args: Vec<String> = std::env::args().collect();
    let gui_mode = cli.gui || raw_args.len() == 1;

    system::hide_windows_console_if_needed(gui_mode);

    let filter = build_selection_filter(&cli);

    if gui_mode {
        gui::launch_gui(
            filter,
            cli.yes,
            cli.elevation_ready_file,
            cli.elevation_state_file,
        )?;
        return Ok(());
    }

    run_cli(cli, filter)
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
    let update_modules: Vec<_> = modules
        .into_iter()
        .filter(|module| module.selected)
        .collect();
    let force_yes = cli.yes;
    let verbose = cli.verbose;

    let update_handle =
        std::thread::spawn(move || app::run_updates(&update_modules, force_yes, &log_tx));

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
