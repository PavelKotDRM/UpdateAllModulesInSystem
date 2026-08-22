//! Информация о версии и сборке приложения.

/// Версия пакета из `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Полный идентификатор Git-коммита.
pub const GIT_HASH: &str = env!("VERGEN_GIT_SHA");

/// Признак наличия незакоммиченных изменений при сборке.
pub const GIT_DIRTY: &str = env!("VERGEN_GIT_DIRTY");

/// Сгруппированная информация о версии и исходном Git-состоянии.
pub const DETAILS: &[(&str, &[(&str, &str)])] = &[(
    "Сборка",
    &[
        ("Версия", VERSION),
        ("SHA", GIT_HASH),
        ("Есть изменения", GIT_DIRTY),
    ],
)];
