# UpdateAllModules

[Русская версия](docs/ru/README.md)

A cross-platform Rust utility for checking and installing updates for system package managers, Python packages, development tools, and extensions for VS Code-compatible editors. The application version is determined by the `package.version` value in `Cargo.toml`.

The project supports two operating modes:

- A CLI for automation and scripts.
- An `egui` / `eframe` GUI for interactive use.

## Features

- Checks and updates system package managers.
- Integrates with Windows Update through the built-in COM API.
- Checks and updates `pip` through `python -m pip`.
- Checks and updates `npm` and `pnpm` themselves, as well as globally installed packages managed by them.
- Checks Node.js against the official release index and updates it according to the installation method (`nvm`, `fnm`, `winget`, Homebrew, or the official MSI).
- Checks published versions and updates extensions for VS Code, VS Code Insiders, VSCodium, Cursor, Windsurf, and Positron.
- Checks and updates extensions across all local VS Code and VS Code Insiders profiles.
- Updates the editors themselves through the system package manager (`winget`, `choco`, `apt`, `brew`, and others).
- Supports `rustup`.
- Supports `msys2` through `pacman` on Windows.
- Applies package-level selections where the updater supports targeted upgrades; `pacman`, MSYS2, `apk`, `xbps`, and `emerge` clearly use full-system updates without misleading package checkboxes.
- Provides tabular CLI output.
- Scans modules in parallel.
- Updates independent modules in parallel with a limited number of worker threads.
- Streams update logs in the CLI when `--verbose` is enabled, including the module and stage.
- Runs scans and updates in the background in the GUI.
- Groups logs by module in a dedicated GUI tab.
- Displays each module's update stage directly in the GUI (`queued`, `running`, `completed`, `error`).
- Uses English by default and includes Russian translations; the language catalog is designed to accommodate additional translations.

## Supported Tools

### Windows

- `windows-update` (Windows Update)
- `winget`
- `choco`
- `msys2` through a `pacman` executable available in `PATH`

### Linux

- `apt`
- `apt-get`
- `dnf`
- `yum`
- `zypper`
- `pacman`
- `apk`
- `xbps`
- `emerge`
- `flatpak`
- `snap`
- `pkcon` (PackageKit)

### macOS

- `brew`

### Cross-platform Tools

- `rustup`
- `node` (official release index; updates through the original manager or the official MSI)
- `npm` (self-update and global packages)
- `pnpm` (`self-update` and global packages)
- `vscode-extensions` (`code`)
- `vscode-insiders-extensions` (`code-insiders`)
- `vscodium-extensions` (`codium`)
- `cursor-extensions` (`cursor`)
- `windsurf-extensions` (`windsurf`)
- `positron-extensions` (`positron`)
- `pip` through `python -m pip`
- `uv` through `uv pip`

Installed extensions are listed through the corresponding editor's CLI. For VS Code and VS Code Insiders, the application detects local profiles, scans each profile separately with `--profile`, and installs the selected update back into the source profile. The same extension in different profiles is displayed and selected independently.

Published versions are queried from Visual Studio Marketplace for VS Code and VS Code Insiders, and from Open VSX for the other compatible editors. Marketplace requests are batched; when a pre-release version is listed first, the application makes a targeted version-history request. It selects a stable version for the current platform without falling back to an outdated universal build.

Checking extensions requires HTTPS access to the corresponding registry. Extensions missing from the selected registry are skipped. To update an editor itself, the system package manager through which it was installed must be enabled.

## Operating Modes

The application uses a single binary and selects its operating mode based on command-line arguments:

- With no arguments, the GUI starts.
- When arguments are provided, the CLI starts.
- The `--gui` flag explicitly opens the graphical interface.

## CLI

Examples:

```powershell
cargo run -- --help
cargo run -- --check
cargo run -- --check --skip-system
cargo run -- --check --only vscode-extensions
cargo run -- --check --only node --only npm --only pnpm
cargo run -- --language ru --check
cargo run -- --yes --verbose
cargo run -- --gui
```

Available flags:

- `-h, --help` - show built-in help, examples, and module names.
- `-V, --version` - show the application version.
- `-c, --check` - only check for available updates.
- `-y, --yes` - automatically confirm updates.
- `--gui` - explicitly open the GUI.
- `--language <en|ru>` - choose the CLI language or override the saved GUI language for this launch.
- `-v, --verbose` - show detailed output.
- `--skip-system` - skip system package managers.
- `--skip-pip` - skip `pip`.
- `--skip-tools` - skip development tools.
- `--only-tools` - include only development tools.
- `--only <NAME>` - limit the list to specific modules.

CLI behavior:

- In update mode, status output remains compact by default.
- With `--verbose`, external command logs are streamed in the format `[module] message`.
- Module errors are always displayed, even without `--verbose`.
- The `-y, --yes` flag enables non-interactive mode where supported by the package manager.
- Unknown `--only` values are rejected before scanning starts, and the valid module names are displayed.

Parallelism in the CLI and GUI:

- Update checks run in parallel for all eligible modules.
- Updates for `pip`, `uv`, `rustup`, and other non-system tools run in parallel.
- The number of parallel worker threads is limited to avoid overloading the system when many modules are enabled.
- System package managers (`winget`, `choco`, `apt`, `pacman`, and similar tools) remain in a separate sequential queue to prevent package database locks and conflicting system changes.
- Logs from each updater are automatically tagged with the module name, so the current stage of each process remains visible during parallel execution.

## GUI

The graphical mode provides:

- a button to restart with administrator or `root` privileges when the current privileges are insufficient;
- a button to check for updates;
- a button to update selected modules;
- a button to update all modules;
- a cancel button that stops the queue and terminates already started process trees;
- a module selection menu with checkboxes for precise control over what is updated;
- `selected/total` and `with updates/total` counters;
- persistence of selected modules between GUI restarts;
- a language selector in Settings; the selected language is saved between restarts;
- storage of settings in the system configuration directory with atomic JSON writes;
- persistence of the `-y` automatic confirmation checkbox between GUI restarts;
- an automatic confirmation `-y` checkbox;
- hidden unavailable modules by default;
- a setting to show unavailable modules;
- a status list of detected package managers and tools;
- expandable update details with package checkboxes where targeted updates are supported; full-system-only managers show an explicit notice instead;
- a dedicated logs tab with expandable groups by module;
- export of logs grouped by module to a text file;
- automatic status rescanning after an update finishes;
- a colored status for the current update stage directly on each module card.

On Windows, the privilege elevation button restarts the application through the system UAC prompt. On Linux, it requires `pkexec` and an active PolicyKit agent. If no elevation mechanism is available or the request is rejected, the current window remains open and the reason is shown in the status and logs. Privilege-elevation support files are accepted only as one-time regular files in the expected format located directly in the system temporary directory.

## Build and Run

### Build

```powershell
cargo build
```

### Start the GUI by Default

```powershell
cargo run
```

### Start the CLI

```powershell
cargo run -- --check
```

### Developer Documentation

Generate local documentation without dependency documentation:

```powershell
cargo doc --no-deps --open
```

The project enables the `missing_docs` and `rustdoc::all` checks; broken intra-doc links are denied in `Cargo.toml`.

### Rust Code Quality

Install the formatting and static analysis components:

```powershell
rustup component add rustfmt clippy
cargo install cargo-audit --locked
```

Run the checks used before publishing changes:

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
cargo doc --no-deps --locked
cargo audit
```

To apply formatting instead of only checking it:

```powershell
cargo fmt --all
```

## CI/CD

The repository includes GitHub Actions workflows:

- CI: tests and release builds run on Windows, Linux, and macOS for every `push` to `main` / `master` and every `pull_request`.
- Release: pushing a tag matching `v*` automatically builds artifacts for Windows, Linux, and macOS and publishes them to GitHub Releases.

Publish a release for the current version from `Cargo.toml`:

```powershell
$version = (cargo metadata --no-deps --format-version 1 | ConvertFrom-Json).packages[0].version
git tag "v$version"
git push origin "v$version"
```

The compiled binary archives for the supported platforms will then appear in GitHub Releases.

## Technical Details

- Language: Rust, edition 2024.
- CLI: `clap`.
- GUI: `egui`, `eframe`.
- CLI table: `comfy-table`.
- Errors: `anyhow`, `thiserror`.
- JSON: `serde`, `serde_json`.
- HTTP and TLS: `reqwest` with `rustls`.
- Windows encodings: `encoding_rs`.
- Administrator privilege check: `is_elevated`.

## Implementation Details

- The `windows-update` module is available only on Windows when `powershell` or `pwsh` is installed. It is treated as a system module and requires elevated privileges.
- The `windows-update` check uses no external PowerShell modules: the COM API `Microsoft.Update.Session` queries uninstalled, visible updates of type `Software` (`IsInstalled=0 and IsHidden=0 and Type='Software'`). Their titles are passed to the application as JSON and displayed as available updates.
- Automatic Windows Update installation is disabled. When an update is started, the application reports the number of selected items and offers to open `Settings -> Windows Update` for manual installation.
- `winget` and `choco` use specialized output parsing when checking for updates; entries without an actual version increase are discarded. For `winget`, the application also checks whether the available version is already installed under a different package identifier. A successful command is considered sufficient confirmation because some installers, including Unity, retain the old version entry during an immediate follow-up check.
- Homebrew uses the machine-readable `brew outdated --json=v2` output with typed parsing of formulae and casks.
- `pip` is invoked through `python -m pip` to reduce dependency on executable paths.
- For `npm` and `pnpm`, the versions of the CLI tools and globally installed packages are compared separately. `npm` is updated by globally installing the latest version, while `pnpm` uses `pnpm self-update <version>`.
- A missing global `npm` directory is treated as an empty global installation rather than a check failure. The npm 12 response containing a version in a single-element JSON array is also supported.
- The latest Node.js version is queried from the official `https://nodejs.org/dist/index.json`.
- Requests to Node.js, Visual Studio Marketplace, and Open VSX share an HTTP client with a 10-second connection timeout and a 120-second overall request timeout.
- The Node.js installation method is detected from available managers, the `node` path, and the Windows MSI registry entry. Supported methods are `nvm`, `fnm`, `winget`, Homebrew, and the official MSI. On Unix, `nvm` is loaded from `nvm.sh` in a Bash subprocess and the updated version becomes the default alias. For MSI installations, the required installer is downloaded from `nodejs.org` and launched through `msiexec /passive /norestart`.
- Package-level selection is passed to targeted package-manager commands. Updaters that perform a full system transaction (`pacman`, MSYS2, `apk`, `xbps`, and `emerge`) show a whole-system notice instead of per-package checkboxes.
- Extension versions for VS Code and VS Code Insiders are queried in batches from Visual Studio Marketplace, taking the platform and stable channel into account; Open VSX is used for the other supported editors.
- Each selected editor extension is updated independently. If an individual extension is unavailable in the registry, the remaining extensions are still updated, and the final error lists the affected extensions.
- VS Code profiles are detected from local user data. Extensions are checked and installed separately for each existing profile.
- On Windows, non-UTF-8 output from external utilities can be decoded using `WINDOWS-1251`, `CP866`, with lossy UTF-8 as a fallback.
- The module order in the final table and update results remains the same as in the updater registry, even when the tasks themselves run in parallel.

## Implementation Status

The project builds and runs. The basic CLI and GUI scenarios work, and updates are implemented through adapters for each tool.

Current limitations:

- For some Linux package managers (`dnf`, `yum`, `zypper`, `pacman`, `apk`, `xbps`, `emerge`, `snap`), update checks rely on heuristic parsing of text output because a single stable machine-readable format is not available across all supported versions.
- For `msys2`, the preferred method invokes its internal `pacman` through `bash.exe` from common installation directories (`C:\msys64`, `C:\tools\msys64`), with a fallback to `pacman` from `PATH`.
- Checking published extension versions depends on the availability of Visual Studio Marketplace or Open VSX; extensions from private and alternative registries are not matched automatically.
- If the Node.js installation method cannot be detected, the automatic update is canceled with a recommendation to use the original package manager or the official installer. Updating a system-wide MSI installation on Windows may require administrator privileges.

## License

MIT. See the `LICENSE` file.
