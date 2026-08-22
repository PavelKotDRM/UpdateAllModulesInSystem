//! Реестр всех поддерживаемых обновляторов и их метаданных.

use crate::model::ModuleKind;

mod common;
mod core;
mod editor_tools;
mod http_client;
mod node_tools;
mod parsers;
mod python_tools;
mod unix_managers;
mod windows_tools;

pub use core::UpdaterDescriptor;
use core::UpdaterSpec;
use editor_tools::*;
use node_tools::*;
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
        name: "vscode-extensions",
        kind: ModuleKind::Tool,
        requires_elevation: false,
        installed: code_installed,
        check: code_check_updates,
        apply: code_apply_updates,
    },
    UpdaterSpec {
        name: "vscode-insiders-extensions",
        kind: ModuleKind::Tool,
        requires_elevation: false,
        installed: code_insiders_installed,
        check: code_insiders_check_updates,
        apply: code_insiders_apply_updates,
    },
    UpdaterSpec {
        name: "vscodium-extensions",
        kind: ModuleKind::Tool,
        requires_elevation: false,
        installed: codium_installed,
        check: codium_check_updates,
        apply: codium_apply_updates,
    },
    UpdaterSpec {
        name: "cursor-extensions",
        kind: ModuleKind::Tool,
        requires_elevation: false,
        installed: cursor_installed,
        check: cursor_check_updates,
        apply: cursor_apply_updates,
    },
    UpdaterSpec {
        name: "windsurf-extensions",
        kind: ModuleKind::Tool,
        requires_elevation: false,
        installed: windsurf_installed,
        check: windsurf_check_updates,
        apply: windsurf_apply_updates,
    },
    UpdaterSpec {
        name: "positron-extensions",
        kind: ModuleKind::Tool,
        requires_elevation: false,
        installed: positron_installed,
        check: positron_check_updates,
        apply: positron_apply_updates,
    },
    UpdaterSpec {
        name: "node",
        kind: ModuleKind::Tool,
        requires_elevation: cfg!(target_os = "windows"),
        installed: node_installed,
        check: node_check_updates,
        apply: node_apply_updates,
    },
    UpdaterSpec {
        name: "npm",
        kind: ModuleKind::Tool,
        requires_elevation: false,
        installed: npm_installed,
        check: npm_check_updates,
        apply: npm_apply_updates,
    },
    UpdaterSpec {
        name: "pnpm",
        kind: ModuleKind::Tool,
        requires_elevation: false,
        installed: pnpm_installed,
        check: pnpm_check_updates,
        apply: pnpm_apply_updates,
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

/// Возвращает имена всех зарегистрированных обновляторов в стабильном порядке.
pub fn updater_names() -> impl Iterator<Item = &'static str> {
    REGISTRY_SPECS.iter().map(|spec| spec.name)
}

/// Создаёт дескрипторы обновляторов в стабильном порядке [`REGISTRY_SPECS`].
pub fn registry() -> Vec<UpdaterDescriptor> {
    REGISTRY_SPECS
        .iter()
        .copied()
        .map(UpdaterSpec::into_descriptor)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::registry;

    #[test]
    fn registry_contains_supported_editor_extension_updaters() {
        let names = registry()
            .into_iter()
            .map(|descriptor| descriptor.updater.name())
            .collect::<Vec<_>>();

        for expected in [
            "vscode-extensions",
            "vscode-insiders-extensions",
            "vscodium-extensions",
            "cursor-extensions",
            "windsurf-extensions",
            "positron-extensions",
        ] {
            assert!(names.contains(&expected), "missing updater: {expected}");
        }
    }
}
