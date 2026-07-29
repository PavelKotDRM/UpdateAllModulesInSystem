// build.rs
use anyhow::Result;
use vergen_gitcl::{Build, Cargo, Emitter, Gitcl, Rustc, Sysinfo};

fn main() -> Result<()> {
    // 1. Инициализируем сборщики информации.
    // Методы all_* автоматически включают все доступные поля каждого модуля.
    let build = Build::all_build();
    let cargo = Cargo::all_cargo();
    let rustc = Rustc::all_rustc();
    let sysinfo = Sysinfo::all_sysinfo();

    // Для Git метрик (ветка, SHA коммита, теги)
    let git = Gitcl::all_git();

    // 2. Передаем инструкции Cargo через Emitter
    Emitter::default()
        .add_instructions(&build)?
        .add_instructions(&cargo)?
        .add_instructions(&rustc)?
        .add_instructions(&sysinfo)?
        .add_instructions(&git)?
        .emit()?;

    Ok(())
}
