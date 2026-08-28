//! Формирует минимальные Git-метаданные для `build_info`.

use anyhow::Result;
use std::{env, process::Command};
use vergen_gitcl::{Emitter, Gitcl};

fn main() -> Result<()> {
    let git = Gitcl::builder()
        .branch(true)
        .commit_timestamp(true)
        .sha(true)
        .dirty(true)
        .build();
    Emitter::default().add_instructions(&git)?.emit()?;

    for name in ["HOST", "TARGET", "PROFILE", "OPT_LEVEL", "DEBUG"] {
        println!("cargo:rustc-env=BUILD_{name}={}", env::var(name)?);
    }

    let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned());
    let rustc_version = Command::new(rustc)
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|version| version.trim().to_owned())
        .unwrap_or_else(|| "неизвестно".to_owned());
    println!("cargo:rustc-env=BUILD_RUSTC_VERSION={rustc_version}");

    Ok(())
}
