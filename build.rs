//! Формирует минимальные Git-метаданные для `build_info`.

use anyhow::Result;
use vergen_gitcl::{Emitter, Gitcl};

fn main() -> Result<()> {
    let git = Gitcl::builder().sha(true).dirty(true).build();
    Emitter::default().add_instructions(&git)?.emit()?;

    Ok(())
}
