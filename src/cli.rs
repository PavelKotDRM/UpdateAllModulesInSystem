use clap::Parser;

#[derive(Debug, Clone, Parser)]
#[command(
    name = "UpdateAllModules",
    version,
    about = "Кроссплатформенная утилита обновления системы, pip и dev-tools"
)]
pub struct Cli {
    #[arg(short = 'c', long = "check")]
    pub check: bool,

    #[arg(short = 'y', long = "yes")]
    pub yes: bool,

    #[arg(long = "gui")]
    pub gui: bool,

    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,

    #[arg(long = "skip-system")]
    pub skip_system: bool,

    #[arg(long = "skip-pip")]
    pub skip_pip: bool,

    #[arg(long = "skip-tools")]
    pub skip_tools: bool,

    #[arg(long = "only-tools")]
    pub only_tools: bool,

    #[arg(long = "only", value_name = "NAME")]
    pub only: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::Parser;

    #[test]
    fn parses_check_and_skip_flags() {
        let cli = Cli::parse_from(["update_all_modules", "--check", "--skip-system", "--skip-pip"]);
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
}
