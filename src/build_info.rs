//! Информация о версии и сборке приложения.

/// Версия пакета из `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Короткий идентификатор Git-коммита.
pub const GIT_HASH: &str = env!("BUILD_GIT_HASH");

/// Описание сборки, сформированное командой `git describe`.
pub const DESCRIPTION: &str = env!("BUILD_DESCRIPTION");