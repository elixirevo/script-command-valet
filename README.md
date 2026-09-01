# SCV — Script Command Valet

SCV is a cross-platform personal command manager and command-building AI unit. It
exposes Bash, Node.js, Python, PowerShell, and native command packages through one
`scv <command>` namespace on macOS, Linux, and Windows.

The native Rust core provides dispatch, metadata help, safety discovery, immutable
activations, Git synchronization, and optional agent-powered package generation.
AI is never required to run or manage an existing command library.

SCV embeds English and Korean UI catalogs. English is the default; choose Korean
with `scv config set ui.locale ko`. Human help for native builtins follows that
setting, while JSON output and user-owned command metadata remain unchanged.

On the first interactive run, `scv` offers setup for the UI locale, default
`codex`/`claude`/`agy` create agent, and an optional Git upload remote. The equivalent
automation-safe form is:

```bash
scv init --locale ko --agent agy \
  --remote git@github.com:owner/scv-commands.git \
  --no-input
```

Init only initializes local source and Git settings. It does not access the network,
create the hosted repository, commit, or push.

## Architecture

SCV separates the portable Git source from machine-local executable state.

```text
~/.scv/                         user-owned Git source
├── .git/
├── scv.toml
└── commands/<command>/
    ├── metadata.toml
    └── <implementation files>

             scv apply / validated management operation
                              ↓

<platform data>/scv/          machine-local runtime state
├── activations/<id>/
│   ├── manifest.toml
│   └── commands/
├── history/<id>/
│   ├── manifest.toml
│   └── package/<command>/
├── current
└── state/
```

SCV never dispatches code directly from the installed user's `~/.scv/commands`
working tree. It validates every package, copies the full library into a new
immutable activation, verifies the copy and its SHA-256 digest, and then atomically
updates `current`. A failed validation leaves the previous activation active.

See [storage architecture](docs/STORAGE_ARCHITECTURE.md),
[sync architecture](docs/SYNC_ARCHITECTURE.md), and
[installation](docs/INSTALLATION.md) for the complete contracts.

## Command package

Every external command is a flat package under `~/.scv/commands/<command>/`:

```toml
name = "path-size"
category = "filesystem"
description = "Show the size of a path."
usage = "scv path-size <path>"
builtin = false
risk = "read"
network = false
supports_dry_run = false
effects = ["reads local path metadata", "prints a path size"]

[[implementations]]
runtime = "bash"
platforms = ["linux", "macos"]
entry = "unix.sh"

[[implementations]]
runtime = "pwsh"
platforms = ["windows"]
entry = "windows.ps1"
```

Metadata owns detailed help and the execution contract. SCV chooses exactly one
implementation for the current OS; it does not infer runtime from an extension,
shebang, or executable bit.

## Build and development

```bash
cargo build
cargo run -- --help
cargo run -- list --json
cargo run -- paths --json
cargo run -- status --json
```

Required core checks:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

The CI matrix runs these checks and the native dispatcher on macOS, Linux, and
Windows. Tagged releases produce native artifacts for:

- macOS Intel and Apple Silicon;
- Linux x86_64 and arm64; and
- Windows x86_64.

## Installation

Standalone Unix installation defaults to `~/.local/bin/scv`:

```bash
./install.sh --binary ./target/release/scv
```

Windows defaults to `%LOCALAPPDATA%\Programs\SCV\bin\scv.exe`:

```powershell
./install.ps1 -BinaryPath ./target/release/scv.exe
```

See [installation and release](docs/INSTALLATION.md) for download installation,
custom locations, PATH behavior, package-manager boundaries, and release artifacts.

## Paths

Use `scv paths --json` to inspect the resolved paths. Supported overrides are:

- `SCV_HOME`
- `SCV_DATA_DIR`
- `SCV_CONFIG_DIR`
- `SCV_CACHE_DIR`
- `SCV_INSTALL_DIR` for installers

There are no older executable names, environment-variable aliases, flat metadata
formats, or data migration commands. SCV is a pre-release greenfield application.

## Core workflows

```bash
# Discover commands and safety contracts
scv list --json
scv info create --json

# Register a local implementation, update source, and activate it
scv add ./tool.py \
  --name tool \
  --category utility \
  --description "Run my tool" \
  --risk read \
  --network false \
  --supports-dry-run false \
  --effect "reads local input" \
  --no-input

# Validate manual changes and create an activation
scv apply --dry-run
scv apply
scv status --json
scv rollback --dry-run
scv rollback

# Synchronize the Git-backed source
scv sync status --json
scv sync pull --yes --no-input
scv sync push --dry-run
scv sync push --yes --no-input
```

`sync pull` fetches into an isolated Git worktree, validates the remote source, and
shows a change summary before approval. Only a fast-forward is accepted. Git and
`gh` retain ownership of credentials; SCV does not store tokens.

## Agent-powered creation

Generate, inspect, save, and immediately run a one-shot command:

```bash
scv "현재 디렉터리의 큰 파일 10개를 보여줘"
scv "Count files by extension" --agent claude --yes --no-input
scv history
scv history run <id>
scv history run <id> --yes --no-input
```

SCV never executes an agent's workspace. It validates the zero-input package,
previews its declared risk, network use, effects, implementation, and syntax checks,
then asks for consent. After consent it commits a revalidated SHA-256-protected copy
under machine-local history and executes that copy in the current working directory.
Every rerun revalidates integrity, uses the caller's current directory, and requires
fresh consent. The newest 100 entries are retained; history is not added to the Git
source.

Create a persistent reusable command:

```bash
scv create "Show directory sizes in descending order" --agent codex
scv create "Sort a JSON file" --name json-sort --agent codex --effort high
scv create "Count the number of files" --agent claude --yes --no-input
scv create "Summarize TOML keys" --agent agy --effort medium
scv create "현재 디렉터리의 파일 수를 세어줘" --locale ko --agent agy
```

Both generation modes explicitly inject a shared provider-neutral package contract
plus exactly one embedded mode contract from `assets/generation/`. The workspace
contains only that mode's metadata and source templates: persistent generation gets
argument/option/help scaffolding, while one-shot generation gets exact zero-input
usage and no persistent interaction scaffolding. The selected adapter starts in the
isolated temporary workspace. Persistent `scv create` consumes only the package under
`generated/`, validates it, asks for installation consent, writes it to the Git
source, and creates a new activation.
Generated descriptions, effects, argument/option help, notes, and runtime messages
use `--locale`, or `ui.locale` when omitted. Each user package stores that one
authored language; changing the UI locale later does not rewrite existing commands.
See [create architecture](docs/CREATE_ARCHITECTURE.md) for adapter and validation
boundaries.

The `codex`, `claude`, and `agy` adapters invoke their corresponding authenticated
local CLI. The Agy adapter enforces its sandbox, disables slash-command and skill
expansion, and supports `low`, `medium`, or `high` effort.

## Development contract

Read [docs/SCRIPT_GUIDE.md](docs/SCRIPT_GUIDE.md) before changing platform behavior
or command packages. It is the canonical development guide. Changes to platform
behavior must keep the guide, relevant architecture documents, affected builtin
metadata, embedded generation assets, Skill, `AGENTS.md`, README, and CI aligned.
