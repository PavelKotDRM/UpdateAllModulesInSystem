//! Определение CLI-аргументов приложения через `clap`.

use crate::localization::Language;
use clap::Parser;
use std::path::PathBuf;

const AFTER_HELP: &str = "MODES:
    No arguments                Start the graphical interface
    With CLI arguments          Start command-line mode
    --gui                       Explicitly start the graphical interface

EXAMPLES:
    update_all_modules --check
    update_all_modules --check --skip-system
    update_all_modules --check --only vscode-extensions
    update_all_modules --check --only node --only npm --only pnpm
    update_all_modules --yes --verbose

MODULE NAMES:
    System: windows-update, winget, choco, apt, apt-get, dnf, yum, zypper,
                         pacman, apk, xbps, emerge, flatpak, snap, pkcon, brew
    Python: pip, uv
    Tools: rustup, node, npm, pnpm, msys2, vscode-extensions,
                             vscode-insiders-extensions, vscodium-extensions,
                             cursor-extensions, windsurf-extensions, positron-extensions";

#[derive(Debug, Clone, Parser)]
#[command(
    name = "UpdateAllModules",
    version,
    about = "Cross-platform utility for updating system packages, Python packages, and development tools",
    long_about = "Checks for and installs updates for system package managers, Python packages, and development tools.\nStarts the GUI without arguments and command-line mode when CLI arguments are provided.",
    after_long_help = AFTER_HELP,
    help_template = "{before-help}{name} {version}\n{about-with-newline}\nUsage: {usage}\n\n{all-args}{after-help}"
)]
/// Command-line arguments for the application.
pub struct Cli {
    /// Only check for available updates without installing them.
    #[arg(short = 'c', long = "check", help_heading = "EXECUTION MODE")]
    pub check: bool,

    /// Automatically confirm package-manager prompts (`-y`).
    #[arg(short = 'y', long = "yes", help_heading = "EXECUTION MODE")]
    pub yes: bool,

    /// Start the graphical interface.
    #[arg(long = "gui", help_heading = "EXECUTION MODE")]
    pub gui: bool,

    /// Select the application language (en or ru).
    #[arg(
        long = "language",
        value_enum,
        value_name = "LANGUAGE",
        help_heading = "APPLICATION SETTINGS"
    )]
    pub language: Option<Language>,

    /// Служебный файл подтверждения запуска GUI после повышения прав.
    #[arg(long, hide = true)]
    pub elevation_ready_file: Option<PathBuf>,

    /// Служебный снимок результатов до повышения прав.
    #[arg(long, hide = true)]
    pub elevation_state_file: Option<PathBuf>,

    /// Print detailed CLI logs.
    #[arg(short = 'v', long = "verbose", help_heading = "EXECUTION MODE")]
    pub verbose: bool,

    /// Skip system package managers.
    #[arg(long = "skip-system", help_heading = "MODULE FILTERS")]
    pub skip_system: bool,

    /// Skip Python packages.
    #[arg(long = "skip-pip", help_heading = "MODULE FILTERS")]
    pub skip_pip: bool,

    /// Skip development tools.
    #[arg(long = "skip-tools", help_heading = "MODULE FILTERS")]
    pub skip_tools: bool,

    /// Include only development tools.
    #[arg(
        long = "only-tools",
        conflicts_with = "skip_tools",
        help_heading = "MODULE FILTERS"
    )]
    pub only_tools: bool,

    /// Limit processing to the specified module names.
    #[arg(long = "only", value_name = "NAME", help_heading = "MODULE FILTERS")]
    pub only: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use crate::localization::Language;
    use clap::{CommandFactory, Parser};

    #[test]
    fn parses_check_and_skip_flags() {
        let cli = Cli::parse_from([
            "update_all_modules",
            "--check",
            "--skip-system",
            "--skip-pip",
        ]);
        assert!(cli.check);
        assert!(cli.skip_system);
        assert!(cli.skip_pip);
        assert!(!cli.skip_tools);
    }

    #[test]
    fn parses_yes_verbose_gui_and_only_flags() {
        let cli = Cli::parse_from([
            "update_all_modules",
            "--yes",
            "--verbose",
            "--gui",
            "--only-tools",
            "--only",
            "pip",
            "--only",
            "rustup",
        ]);

        assert!(cli.yes);
        assert!(cli.verbose);
        assert!(cli.gui);
        assert!(cli.only_tools);
        assert_eq!(cli.only, vec!["pip".to_owned(), "rustup".to_owned()]);
    }

    #[test]
    fn parses_language_override() {
        let cli = Cli::parse_from(["update_all_modules", "--language", "ru"]);
        assert_eq!(cli.language, Some(Language::Russian));
    }

    #[test]
    fn parses_elevation_ready_file_without_showing_it_in_help() {
        let cli = Cli::parse_from([
            "update_all_modules",
            "--gui",
            "--elevation-ready-file",
            "/tmp/update-all-modules-ready",
        ]);

        assert_eq!(
            cli.elevation_ready_file.as_deref(),
            Some(std::path::Path::new("/tmp/update-all-modules-ready"))
        );
        assert!(
            !Cli::command()
                .render_long_help()
                .to_string()
                .contains("elevation-ready-file")
        );
    }

    #[test]
    fn rejects_conflicting_tool_filters() {
        let result = Cli::try_parse_from(["update_all_modules", "--skip-tools", "--only-tools"]);
        assert!(result.is_err());
    }

    #[test]
    fn long_help_describes_modes_examples_and_module_names() {
        let help = Cli::command().render_long_help().to_string();

        assert!(help.contains("MODES:"));
        assert!(help.contains("EXAMPLES:"));
        assert!(help.contains("MODULE NAMES:"));
        assert!(help.contains("vscode-extensions"));
        assert!(help.contains("Starts the GUI without arguments"));
    }
}
