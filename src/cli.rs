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
