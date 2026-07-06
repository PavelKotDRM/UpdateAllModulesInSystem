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

fn main() -> Result<()> {
    let cli = Cli::parse();
    let raw_args: Vec<String> = std::env::args().collect();
    let gui_mode = cli.gui || raw_args.len() == 1;

    system::hide_windows_console_if_needed(gui_mode);

    let mut filter = app::SelectionFilter::default();
    filter.skip_system = cli.skip_system;
    filter.skip_pip = cli.skip_pip;
    filter.skip_tools = cli.skip_tools;
    filter.only_tools = cli.only_tools;
    filter.only = cli.only.clone().into_iter().collect::<BTreeSet<_>>();

    if gui_mode {
        gui::launch_gui(filter, cli.yes)?;
        return Ok(());
    }

    run_cli(cli, filter)
}

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
