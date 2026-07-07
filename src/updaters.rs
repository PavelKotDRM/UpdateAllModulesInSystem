//! Реестр всех поддерживаемых обновляторов и их метаданных.

use crate::model::ModuleKind;

mod core;
mod common;
mod parsers;
mod python_tools;
mod unix_managers;
mod windows_tools;

pub use core::UpdaterDescriptor;
use core::UpdaterSpec;
use python_tools::*;
use unix_managers::*;
use windows_tools::*;

const REGISTRY_SPECS: &[UpdaterSpec] = &[
    UpdaterSpec {
        name: "windows-update",
        kind: ModuleKind::System,
        requires_elevation: true,
        installed: windows_update_installed,
        check: windows_update_check_updates,
        apply: windows_update_apply_updates,
    },
    UpdaterSpec {
        name: "winget",
        kind: ModuleKind::System,
        requires_elevation: true,
        installed: winget_installed,
        check: winget_check_updates,
        apply: winget_apply_updates,
    },
    UpdaterSpec {
        name: "choco",
        kind: ModuleKind::System,
        requires_elevation: true,
        installed: chocolatey_installed,
        check: chocolatey_check_updates,
        apply: chocolatey_apply_updates,
    },
    UpdaterSpec {
        name: "apt",
        kind: ModuleKind::System,
        requires_elevation: true,
        installed: apt_installed,
        check: apt_check_updates,
        apply: apt_apply_updates,
    },
    UpdaterSpec {
        name: "apt-get",
        kind: ModuleKind::System,
        requires_elevation: true,
        installed: apt_get_installed,
        check: apt_get_check_updates,
        apply: apt_get_apply_updates,
    },
    UpdaterSpec {
        name: "dnf",
        kind: ModuleKind::System,
        requires_elevation: true,
        installed: dnf_installed,
        check: dnf_check_updates,
        apply: dnf_apply_updates,
    },
    UpdaterSpec {
        name: "yum",
        kind: ModuleKind::System,
        requires_elevation: true,
        installed: yum_installed,
        check: yum_check_updates,
        apply: yum_apply_updates,
    },
    UpdaterSpec {
        name: "zypper",
        kind: ModuleKind::System,
        requires_elevation: true,
        installed: zypper_installed,
        check: zypper_check_updates,
        apply: zypper_apply_updates,
    },
    UpdaterSpec {
        name: "pacman",
        kind: ModuleKind::System,
        requires_elevation: true,
        installed: pacman_installed,
        check: pacman_check_updates,
        apply: pacman_apply_updates,
    },
    UpdaterSpec {
        name: "apk",
        kind: ModuleKind::System,
        requires_elevation: true,
        installed: apk_installed,
        check: apk_check_updates,
        apply: apk_apply_updates,
    },
    UpdaterSpec {
        name: "xbps",
        kind: ModuleKind::System,
        requires_elevation: true,
        installed: xbps_installed,
        check: xbps_check_updates,
        apply: xbps_apply_updates,
    },
    UpdaterSpec {
        name: "emerge",
        kind: ModuleKind::System,
        requires_elevation: true,
        installed: emerge_installed,
        check: emerge_check_updates,
        apply: emerge_apply_updates,
    },
    UpdaterSpec {
        name: "flatpak",
        kind: ModuleKind::System,
        requires_elevation: true,
        installed: flatpak_installed,
        check: flatpak_check_updates,
        apply: flatpak_apply_updates,
    },
    UpdaterSpec {
        name: "snap",
        kind: ModuleKind::System,
        requires_elevation: true,
        installed: snap_installed,
        check: snap_check_updates,
        apply: snap_apply_updates,
    },
    UpdaterSpec {
        name: "pkcon",
        kind: ModuleKind::System,
        requires_elevation: true,
        installed: pkcon_installed,
        check: pkcon_check_updates,
        apply: pkcon_apply_updates,
    },
    UpdaterSpec {
        name: "brew",
        kind: ModuleKind::System,
        requires_elevation: true,
        installed: brew_installed,
        check: brew_check_updates,
        apply: brew_apply_updates,
    },
    UpdaterSpec {
        name: "rustup",
        kind: ModuleKind::Tool,
        requires_elevation: false,
        installed: installed_rustup,
        check: check_rustup_updates,
        apply: apply_rustup_updates,
    },
    UpdaterSpec {
        name: "msys2",
        kind: ModuleKind::Tool,
        requires_elevation: true,
        installed: msys2_installed,
        check: msys2_check_updates,
        apply: msys2_apply_updates,
    },
    UpdaterSpec {
        name: "pip",
        kind: ModuleKind::Python,
        requires_elevation: false,
        installed: installed_pip,
        check: check_pip_updates,
        apply: apply_pip_updates,
    },
    UpdaterSpec {
        name: "uv",
        kind: ModuleKind::Python,
        requires_elevation: false,
        installed: uv_installed,
        check: check_uv_updates,
        apply: apply_uv_updates,
    },
];

/// Возвращает полный реестр обновляторов, доступных приложению.
///
/// # Arguments
/// Функция не принимает аргументов.
///
/// # Returns
/// Вектор дескрипторов, каждый из которых содержит динамический обработчик,
/// тип модуля и требования по привилегиям.
///
/// # Panics
/// Не паникует.
///
/// # Examples
/// ```rust,ignore
/// let handlers = registry();
/// assert!(!handlers.is_empty());
/// ```
pub fn registry() -> Vec<UpdaterDescriptor> {
    REGISTRY_SPECS
        .iter()
        .copied()
        .map(UpdaterSpec::into_descriptor)
        .collect()
}
