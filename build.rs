use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads/");

    // Получаем git-описание (опционально)
    let describe = Command::new("git")
        .args(&["describe", "--tags", "--dirty", "--always"])
        .output()
        .ok()
        .and_then(|o| if o.status.success() {
            Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
        } else {
            None
        });

    // Получаем текущий git-хэш (опционально)
    let git_hash = Command::new("git")
        .args(&["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| if o.status.success() {
            Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
        } else {
            None
        });

    let git_hash_val = git_hash.as_deref().unwrap_or("no-git");
    let description = describe.as_deref().unwrap_or("unknown");

    println!("cargo:rustc-env=BUILD_GIT_HASH={git_hash_val}");
    println!("cargo:rustc-env=BUILD_DESCRIPTION={description}");
}