//! Типы доменной модели для описания модулей, статусов и доступных обновлений.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Категория модуля, определяющая источник обновлений.
pub enum ModuleKind {
    System,
    Tool,
    Python,
}

#[derive(Debug, Clone)]
/// Описание одного доступного обновления пакета.
pub struct PackageUpdate {
    /// Логическое имя пакета в формате, зависящем от менеджера.
    pub name: String,
    /// Текущая установленная версия.
    pub current_version: String,
    /// Версия, доступная для установки.
    pub available_version: String,
    /// Признак, выбран ли пакет пользователем для обновления.
    pub selected: bool,
}

impl PackageUpdate {
    /// Создает новую запись об обновлении пакета.
    ///
    /// # Arguments
    /// * `name` - Имя пакета.
    /// * `current_version` - Текущая версия.
    /// * `available_version` - Доступная версия.
    ///
    /// # Returns
    /// Возвращает [`PackageUpdate`] с включенным флагом выбора (`selected = true`).
    ///
    /// # Panics
    /// Не паникует.
    ///
    /// # Examples
    /// ```rust,ignore
    /// let update = PackageUpdate::new("pip:requests", "2.31.0", "2.32.0");
    /// assert!(update.selected);
    /// ```
    pub fn new(name: impl Into<String>, current_version: impl Into<String>, available_version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            current_version: current_version.into(),
            available_version: available_version.into(),
            selected: true,
        }
    }
}

#[derive(Debug, Clone)]
/// Состояние модуля после проверки обновлений.
pub enum ModuleStatus {
    /// Менеджер или инструмент не обнаружен в системе.
    NotFound,
    /// Обновления отсутствуют.
    UpToDate,
    /// Доступно указанное число обновлений.
    UpdatesAvailable(usize),
    /// Ошибка при проверке или выполнении операции.
    Error(String),
}

impl ModuleStatus {
    /// Возвращает человекочитаемую метку состояния.
    ///
    /// # Arguments
    /// Функция не принимает аргументов.
    ///
    /// # Returns
    /// Локализованная строка для отображения в CLI/GUI.
    ///
    /// # Panics
    /// Не паникует.
    ///
    /// # Examples
    /// ```rust,ignore
    /// let label = ModuleStatus::UpToDate.label();
    /// assert_eq!(label, "Актуален");
    /// ```
    pub fn label(&self) -> String {
        match self {
            Self::NotFound => "Не найден в системе".to_owned(),
            Self::UpToDate => "Актуален".to_owned(),
            Self::UpdatesAvailable(count) => format!("Доступны обновления ({count} пакетов)"),
            Self::Error(message) => format!("Ошибка: {message}"),
        }
    }

    /// Проверяет, указывает ли состояние на наличие обновлений.
    ///
    /// # Arguments
    /// Функция не принимает аргументов.
    ///
    /// # Returns
    /// `true`, если состояние равно [`ModuleStatus::UpdatesAvailable`], иначе `false`.
    ///
    /// # Panics
    /// Не паникует.
    ///
    /// # Examples
    /// ```rust,ignore
    /// assert!(ModuleStatus::UpdatesAvailable(1).has_updates());
    /// assert!(!ModuleStatus::UpToDate.has_updates());
    /// ```
    pub fn has_updates(&self) -> bool {
        matches!(self, Self::UpdatesAvailable(_))
    }
}

#[derive(Debug, Clone)]
/// Снимок состояния одного обновляемого модуля.
pub struct ModuleSnapshot {
    /// Уникальное имя модуля.
    pub name: String,
    /// Категория модуля.
    pub kind: ModuleKind,
    /// Установлен ли модуль в системе.
    pub installed: bool,
    /// Выбран ли модуль пользователем для запуска обновления.
    pub selected: bool,
    /// Требуются ли повышенные привилегии для обновления.
    pub requires_elevation: bool,
    /// Текущее состояние проверки.
    pub status: ModuleStatus,
    /// Список доступных обновлений внутри модуля.
    pub updates: Vec<PackageUpdate>,
}

impl ModuleSnapshot {
    /// Создает новый снимок модуля с базовыми значениями.
    ///
    /// # Arguments
    /// * `name` - Имя модуля.
    /// * `kind` - Категория модуля.
    /// * `requires_elevation` - Требуется ли запуск с повышенными правами.
    ///
    /// # Returns
    /// Новый [`ModuleSnapshot`] со статусом [`ModuleStatus::NotFound`].
    ///
    /// # Panics
    /// Не паникует.
    ///
    /// # Examples
    /// ```rust,ignore
    /// let snapshot = ModuleSnapshot::new("pip", ModuleKind::Python, false);
    /// assert_eq!(snapshot.name, "pip");
    /// ```
    pub fn new(name: impl Into<String>, kind: ModuleKind, requires_elevation: bool) -> Self {
        Self {
            name: name.into(),
            kind,
            installed: false,
            selected: true,
            requires_elevation,
            status: ModuleStatus::NotFound,
            updates: Vec::new(),
        }
    }

    /// Возвращает текстовую метку текущего статуса модуля.
    ///
    /// # Arguments
    /// Функция не принимает аргументов.
    ///
    /// # Returns
    /// Строка, пригодная для отображения пользователю.
    ///
    /// # Panics
    /// Не паникует.
    ///
    /// # Examples
    /// ```rust,ignore
    /// let snapshot = ModuleSnapshot::new("pip", ModuleKind::Python, false);
    /// let _label = snapshot.status_label();
    /// ```
    pub fn status_label(&self) -> String {
        self.status.label()
    }

    /// Формирует подробные строки по каждому найденному обновлению.
    ///
    /// # Arguments
    /// Функция не принимает аргументов.
    ///
    /// # Returns
    /// Список строк формата `name: current -> available`.
    /// Если обновлений нет, возвращается строка-заглушка.
    ///
    /// # Panics
    /// Не паникует.
    ///
    /// # Examples
    /// ```rust,ignore
    /// let snapshot = ModuleSnapshot::new("pip", ModuleKind::Python, false);
    /// let lines = snapshot.detail_lines();
    /// assert!(!lines.is_empty());
    /// ```
    pub fn detail_lines(&self) -> Vec<String> {
        if self.updates.is_empty() {
            return vec!["Список конкретных обновлений пуст".to_owned()];
        }

        self.updates
            .iter()
            .map(|update| {
                format!(
                    "{}: {} -> {}",
                    update.name, update.current_version, update.available_version
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_labels_and_has_updates_are_consistent() {
        assert_eq!(ModuleStatus::NotFound.label(), "Не найден в системе");
        assert_eq!(ModuleStatus::UpToDate.label(), "Актуален");
        assert_eq!(
            ModuleStatus::UpdatesAvailable(3).label(),
            "Доступны обновления (3 пакетов)"
        );
        assert!(ModuleStatus::UpdatesAvailable(1).has_updates());
        assert!(!ModuleStatus::UpToDate.has_updates());
    }

    #[test]
    fn detail_lines_returns_placeholder_when_no_updates() {
        let module = ModuleSnapshot::new("pip", ModuleKind::Python, false);
        let details = module.detail_lines();
        assert_eq!(details, vec!["Список конкретных обновлений пуст".to_owned()]);
    }

    #[test]
    fn detail_lines_formats_updates() {
        let mut module = ModuleSnapshot::new("pip", ModuleKind::Python, false);
        module.updates = vec![
            PackageUpdate::new("requests", "2.31.0", "2.32.0"),
            PackageUpdate::new("urllib3", "2.0.0", "2.1.0"),
        ];

        let details = module.detail_lines();
        assert_eq!(details.len(), 2);
        assert_eq!(details[0], "requests: 2.31.0 -> 2.32.0");
        assert_eq!(details[1], "urllib3: 2.0.0 -> 2.1.0");
    }
}
