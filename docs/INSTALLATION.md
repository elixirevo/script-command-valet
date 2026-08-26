# SCV Installation and Release

## Standalone locations

| Platform | Default executable |
|---|---|
| macOS | `~/.local/bin/scv` |
| Linux | `~/.local/bin/scv` |
| Windows | `%LOCALAPPDATA%\Programs\SCV\bin\scv.exe` |

Set `SCV_INSTALL_DIR` or the installer option to choose another location. Homebrew,
WinGet, Cargo, and other package managers own their own installation paths and PATH
configuration.

## Unix installer

```bash
./install.sh --binary ./target/release/scv
./install.sh --version v0.1.0
SCV_NO_MODIFY_PATH=1 ./install.sh
```

When downloading, the installer selects an OS/architecture artifact, requires HTTPS,
downloads `SHA256SUMS`, verifies SHA-256, and then installs the binary.

If the installation directory is not already in PATH, it adds one identifiable,
idempotent block to:

- zsh: `~/.zshenv`;
- bash: `~/.bashrc`; or
- fish: `~/.config/fish/conf.d/scv.fish`.

Unknown shells receive a printed PATH instruction. `--no-modify-path` and
`SCV_NO_MODIFY_PATH=1` disable profile changes.

`SCV_SHELL` and `SCV_SHELL_PROFILE` are installer-test overrides; normal users do
not need them.

## Windows installer

```powershell
./install.ps1 -BinaryPath ./target/release/scv.exe
./install.ps1 -Version v0.1.0
./install.ps1 -NoModifyPath
```

The installer verifies release checksums and adds the exact install directory to the
user PATH only when missing. It does not edit PowerShell profile scripts.

## Release artifacts

`.github/workflows/release.yml` builds tagged releases on native GitHub-hosted
runners:

- `scv-x86_64-apple-darwin.tar.gz`
- `scv-aarch64-apple-darwin.tar.gz`
- `scv-x86_64-unknown-linux-gnu.tar.gz`
- `scv-aarch64-unknown-linux-gnu.tar.gz`
- `scv-x86_64-pc-windows-msvc.zip`
- `SHA256SUMS`

The default release base is `https://github.com/elixir/scv/releases`. Before the
first public release, either use that repository or update the default in both
installers. `SCV_RELEASE_BASE_URL` always overrides it.

## PATH versus application data

The executable location and SCV application data are independent. PATH contains only
the small native launcher binary. Git source, activations, configuration, and cache
use the locations documented in `STORAGE_ARCHITECTURE.md`.
