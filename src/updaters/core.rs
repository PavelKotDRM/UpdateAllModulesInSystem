//! Базовые строительные блоки для описания обновляторов через функции.

use crate::model::{ModuleKind, PackageUpdate};
use crate::updater::{Updater, UpdaterError};
use std::sync::mpsc::Sender;

/// Тип функции проверки доступных обновлений.
pub type CheckFn = fn() -> Result<Vec<PackageUpdate>, UpdaterError>;
/// Тип функции проверки наличия менеджера пакетов в системе.
pub type InstalledFn = fn() -> bool;
/// Тип функции применения обновлений.
pub type ApplyFn = fn(bool, &[PackageUpdate], &Sender<String>) -> Result<(), UpdaterError>;

/// Дескриптор зарегистрированного обновлятора.
pub struct UpdaterDescriptor {
    /// Экземпляр обработчика, реализующего [`Updater`].
    pub updater: Box<dyn Updater>,
    /// Категория модуля (системный, инструментальный, Python).
    pub kind: ModuleKind,
    /// Требуется ли повышенный уровень прав для запуска.
    pub requires_elevation: bool,
}

struct FunctionUpdater {
    name: &'static str,
    installed: InstalledFn,
    check: CheckFn,
    apply: ApplyFn,
}

#[derive(Clone, Copy)]
/// Спецификация обновлятора в виде набора функциональных указателей.
pub struct UpdaterSpec {
    /// Имя обновлятора.
    pub name: &'static str,
    /// Категория модуля.
    pub kind: ModuleKind,
    /// Признак необходимости повышенных прав.
    pub requires_elevation: bool,
    /// Функция проверки установки.
    pub installed: InstalledFn,
    /// Функция проверки доступных обновлений.
    pub check: CheckFn,
    /// Функция применения обновлений.
    pub apply: ApplyFn,
}

impl Updater for FunctionUpdater {
    fn name(&self) -> &'static str {
        self.name
    }

    fn is_installed(&self) -> bool {
        (self.installed)()
    }

    fn check_updates(&self) -> Result<Vec<PackageUpdate>, UpdaterError> {
        (self.check)()
    }

    fn apply_updates(
        &self,
        force_yes: bool,
        selected_updates: &[PackageUpdate],
        log_sender: &Sender<String>,
    ) -> Result<(), UpdaterError> {
        (self.apply)(force_yes, selected_updates, log_sender)
    }
}

impl UpdaterSpec {
    /// Преобразует спецификацию в готовый дескриптор с динамическим обновлятором.
    ///
    /// # Arguments
    /// * `self` - Спецификация обновлятора.
    ///
    /// # Returns
    /// Новый [`UpdaterDescriptor`], который можно добавить в реестр.
    ///
    /// # Panics
    /// Не паникует.
    ///
    /// # Examples
    /// ```rust,ignore
    /// let descriptor = spec.into_descriptor();
    /// assert!(descriptor.updater.name().len() > 0);
    /// ```
    pub fn into_descriptor(self) -> UpdaterDescriptor {
        UpdaterDescriptor {
            updater: Box::new(FunctionUpdater {
                name: self.name,
                installed: self.installed,
                check: self.check,
                apply: self.apply,
            }),
            kind: self.kind,
            requires_elevation: self.requires_elevation,
        }
    }
}
