//! Определение CLI-аргументов приложения через `clap`.

use clap::Parser;

#[derive(Debug, Clone, Parser)]
#[command(
    name = "UpdateAllModules",
    version,
    about = "Кроссплатформенная утилита обновления системы, pip и dev-tools"
)]
/// Параметры командной строки приложения.
pub struct Cli {
    /// Только проверить обновления без применения.
    #[arg(short = 'c', long = "check")]
    pub check: bool,

    /// Автоматически подтверждать действия (`-y`).
    #[arg(short = 'y', long = "yes")]
    pub yes: bool,

    /// Запустить графический интерфейс.
    #[arg(long = "gui")]
    pub gui: bool,

    /// Выводить подробные логи в CLI.
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,

    /// Пропустить системные менеджеры пакетов.
    #[arg(long = "skip-system")]
    pub skip_system: bool,

    /// Пропустить Python-пакеты.
    #[arg(long = "skip-pip")]
    pub skip_pip: bool,

    /// Пропустить инструменты разработки.
    #[arg(long = "skip-tools")]
    pub skip_tools: bool,

    /// Включить только инструменты разработки.
    #[arg(long = "only-tools", conflicts_with = "skip_tools")]
    pub only_tools: bool,

    /// Ограничить обработку перечисленными именами модулей.
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

    #[test]
    fn rejects_conflicting_tool_filters() {
        let result = Cli::try_parse_from([
            "update_all_modules",
            "--skip-tools",
            "--only-tools",
        ]);
        assert!(result.is_err());
    }
}
