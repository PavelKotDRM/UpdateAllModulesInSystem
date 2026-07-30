//! Определение CLI-аргументов приложения через `clap`.

use clap::Parser;

const AFTER_HELP: &str = "РЕЖИМЫ:
    Без аргументов             Запустить графический интерфейс
    С любым CLI-параметром     Запустить режим командной строки
    --gui                      Принудительно запустить графический интерфейс

ПРИМЕРЫ:
    update_all_modules --check
    update_all_modules --check --skip-system
    update_all_modules --check --only vscode-extensions
    update_all_modules --check --only node --only npm --only pnpm
    update_all_modules --yes --verbose

ИМЕНА МОДУЛЕЙ:
    Системные: windows-update, winget, choco, apt, apt-get, dnf, yum, zypper,
                         pacman, apk, xbps, emerge, flatpak, snap, pkcon, brew
    Python:    pip, uv
    Инструменты: rustup, node, npm, pnpm, msys2, vscode-extensions,
                             vscode-insiders-extensions, vscodium-extensions,
                             cursor-extensions, windsurf-extensions, positron-extensions";

#[derive(Debug, Clone, Parser)]
#[command(
    name = "UpdateAllModules",
    version,
    about = "Кроссплатформенная утилита обновления системы, pip и dev-tools",
    long_about = "Проверяет и обновляет системные пакеты, Python-пакеты и инструменты разработки.\nБез аргументов запускает GUI; при наличии CLI-параметров работает в терминале.",
    after_long_help = AFTER_HELP,
    help_template = "{before-help}{name} {version}\n{about-with-newline}\nИспользование: {usage}\n\n{all-args}{after-help}"
)]
/// Параметры командной строки приложения.
pub struct Cli {
    /// Только проверить обновления без применения.
    #[arg(short = 'c', long = "check", help_heading = "РЕЖИМ ВЫПОЛНЕНИЯ")]
    pub check: bool,

    /// Автоматически подтверждать действия (`-y`).
    #[arg(short = 'y', long = "yes", help_heading = "РЕЖИМ ВЫПОЛНЕНИЯ")]
    pub yes: bool,

    /// Запустить графический интерфейс.
    #[arg(long = "gui", help_heading = "РЕЖИМ ВЫПОЛНЕНИЯ")]
    pub gui: bool,

    /// Выводить подробные логи в CLI.
    #[arg(short = 'v', long = "verbose", help_heading = "РЕЖИМ ВЫПОЛНЕНИЯ")]
    pub verbose: bool,

    /// Пропустить системные менеджеры пакетов.
    #[arg(long = "skip-system", help_heading = "ФИЛЬТРЫ МОДУЛЕЙ")]
    pub skip_system: bool,

    /// Пропустить Python-пакеты.
    #[arg(long = "skip-pip", help_heading = "ФИЛЬТРЫ МОДУЛЕЙ")]
    pub skip_pip: bool,

    /// Пропустить инструменты разработки.
    #[arg(long = "skip-tools", help_heading = "ФИЛЬТРЫ МОДУЛЕЙ")]
    pub skip_tools: bool,

    /// Включить только инструменты разработки.
    #[arg(
        long = "only-tools",
        conflicts_with = "skip_tools",
        help_heading = "ФИЛЬТРЫ МОДУЛЕЙ"
    )]
    pub only_tools: bool,

    /// Ограничить обработку перечисленными именами модулей.
    #[arg(long = "only", value_name = "NAME", help_heading = "ФИЛЬТРЫ МОДУЛЕЙ")]
    pub only: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::Cli;
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
    fn rejects_conflicting_tool_filters() {
        let result = Cli::try_parse_from(["update_all_modules", "--skip-tools", "--only-tools"]);
        assert!(result.is_err());
    }

    #[test]
    fn long_help_describes_modes_examples_and_module_names() {
        let help = Cli::command().render_long_help().to_string();

        assert!(help.contains("РЕЖИМЫ:"));
        assert!(help.contains("ПРИМЕРЫ:"));
        assert!(help.contains("ИМЕНА МОДУЛЕЙ:"));
        assert!(help.contains("vscode-extensions"));
        assert!(help.contains("Без аргументов запускает GUI"));
    }
}
