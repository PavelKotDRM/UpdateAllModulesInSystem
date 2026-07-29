//! Информация о версии и сборке приложения.

/// Версия пакета из `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Полный идентификатор Git-коммита.
pub const GIT_HASH: &str = env!("VERGEN_GIT_SHA");

/// Описание сборки, сформированное командой `git describe`.
pub const DESCRIPTION: &str = env!("VERGEN_GIT_DESCRIBE");

/// Сгруппированная подробная информация, сформированная `vergen`.
pub const DETAILS: &[(&str, &[(&str, &str)])] = &[
    (
        "Сборка",
        &[
            ("Версия", VERSION),
            ("Дата", env!("VERGEN_BUILD_DATE")),
            ("Время", env!("VERGEN_BUILD_TIMESTAMP")),
        ],
    ),
    (
        "Cargo",
        &[
            ("Отладочная сборка", env!("VERGEN_CARGO_DEBUG")),
            ("Возможности", env!("VERGEN_CARGO_FEATURES")),
            ("Уровень оптимизации", env!("VERGEN_CARGO_OPT_LEVEL")),
            ("Целевая платформа", env!("VERGEN_CARGO_TARGET_TRIPLE")),
        ],
    ),
    (
        "Git",
        &[
            ("Ветка", env!("VERGEN_GIT_BRANCH")),
            ("Автор", env!("VERGEN_GIT_COMMIT_AUTHOR_NAME")),
            ("Email автора", env!("VERGEN_GIT_COMMIT_AUTHOR_EMAIL")),
            ("Количество коммитов", env!("VERGEN_GIT_COMMIT_COUNT")),
            ("Дата коммита", env!("VERGEN_GIT_COMMIT_DATE")),
            ("Время коммита", env!("VERGEN_GIT_COMMIT_TIMESTAMP")),
            ("Сообщение коммита", env!("VERGEN_GIT_COMMIT_MESSAGE")),
            ("Описание", DESCRIPTION),
            ("SHA", GIT_HASH),
            ("Есть изменения", env!("VERGEN_GIT_DIRTY")),
        ],
    ),
    (
        "Rust",
        &[
            ("Версия rustc", env!("VERGEN_RUSTC_SEMVER")),
            ("Канал", env!("VERGEN_RUSTC_CHANNEL")),
            ("Дата коммита rustc", env!("VERGEN_RUSTC_COMMIT_DATE")),
            ("SHA rustc", env!("VERGEN_RUSTC_COMMIT_HASH")),
            ("Платформа rustc", env!("VERGEN_RUSTC_HOST_TRIPLE")),
            ("Версия LLVM", env!("VERGEN_RUSTC_LLVM_VERSION")),
        ],
    ),
    (
        "Система сборки",
        &[
            ("Система", env!("VERGEN_SYSINFO_NAME")),
            ("Версия ОС", env!("VERGEN_SYSINFO_OS_VERSION")),
            ("Пользователь", env!("VERGEN_SYSINFO_USER")),
            ("Оперативная память", env!("VERGEN_SYSINFO_TOTAL_MEMORY")),
            ("Производитель CPU", env!("VERGEN_SYSINFO_CPU_VENDOR")),
            ("Модель CPU", env!("VERGEN_SYSINFO_CPU_BRAND")),
            ("Ядра CPU", env!("VERGEN_SYSINFO_CPU_CORE_COUNT")),
            ("Имена CPU", env!("VERGEN_SYSINFO_CPU_NAME")),
            ("Частота CPU", env!("VERGEN_SYSINFO_CPU_FREQUENCY")),
        ],
    ),
];
