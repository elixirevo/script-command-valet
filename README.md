<h1 align="center">SCV · Script Command Valet</h1>

<p align="center">
  <strong>Your scripts. One command. AI when you need it.</strong><br>
  Manage your personal CLI toolkit across macOS, Linux, and Windows.
</p>

<p align="center">
  <a href="https://github.com/elixirevo/script-command-valet/actions/workflows/ci.yml"><img src="https://github.com/elixirevo/script-command-valet/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="#installation"><img src="https://img.shields.io/badge/status-pre--release-orange" alt="Status: pre-release"></a>
  <a href="#supported-runtimes"><img src="https://img.shields.io/badge/platforms-macOS%20%7C%20Linux%20%7C%20Windows-blue" alt="Platforms: macOS, Linux, Windows"></a>
</p>

<p align="center">
  <a href="#quick-start">Quick start</a> ·
  <a href="#ai-commands">AI commands</a> ·
  <a href="#everyday-workflows">Workflows</a> ·
  <a href="#documentation">Documentation</a> ·
  <a href="#contributing">Contributing</a>
</p>

SCV turns scripts and native binaries into named commands with consistent help,
inspectable metadata, and Git-backed source. Bring an existing script, ask an agent
to build a reusable command, or describe a task for a single confirmed run.

```bash
# Register a script you already own; SCV prompts for its metadata.
scv add ./my-tool.py --name my-tool
scv my-tool

# With a local coding agent, describe a task and review it before execution.
scv "Show the 10 largest files in the current directory"

# Create a command you can reuse after approving its installation.
scv create "Count files in the current directory" --name file-count
scv file-count
```

**AI is optional.** Running and managing existing commands needs no coding agent.
Generation uses an installed, authenticated `codex`, `claude`, or `agy` CLI.

## Why SCV?

- **One place for your tools.** Run Bash, Node.js, Python, PowerShell, and native
  packages through `scv <command>`.
- **Help travels with the command.** Package metadata powers `--help`, command
  discovery, and JSON output for automation.
- **Create once or run once.** Save generated tools to your library, or keep
  one-shot tasks in local history for later reruns.
- **Choose when changes go live.** Validate source into a versioned activation;
  inspect pending changes and roll back to an earlier activation.
- **Take your library with you.** Synchronize source through Git, with validated
  pulls and platform-specific implementations.

## Installation

SCV is **pre-release**. Build from source with a current stable Rust toolchain,
Cargo, Git, and your platform's native build tools:

```bash
git clone https://github.com/elixirevo/script-command-valet.git
cd script-command-valet
cargo build --release --locked
```

Then install the binary for your platform.

**macOS / Linux**

```bash
./install.sh --binary ./target/release/scv
```

**Windows PowerShell**

```powershell
./install.ps1 -BinaryPath ./target/release/scv.exe
```

Open a new terminal and check `scv --version`. Standalone installers default to
`~/.local/bin` on macOS/Linux and `%LOCALAPPDATA%\Programs\SCV\bin` on Windows,
and configure PATH when needed. See the [installation guide](docs/INSTALLATION.md)
for custom locations, opting out of PATH changes, and release artifacts.

Install only the [runtimes](#supported-runtimes) your commands use. Coding-agent
CLIs are needed only for [generation](#ai-commands).

## Quick start

### 1. Set up your library

```bash
scv init
```

Choose the UI language, default coding agent, and optional Git remote with the
arrow keys and Enter. Setup initializes local source and preferences; it does not
contact a remote, commit, or push. Esc or Ctrl+C cancels before changes are made.
Running `scv` without arguments also offers setup on the first interactive run.

For setup without prompts, use `scv init --no-input`.

### 2. Add your first command

With Python installed, save this as `hello.py` in a directory outside the SCV
product checkout:

```python
print("Hello from SCV!")
```

Register it with explicit metadata. This command works in Bash, zsh, and PowerShell:

```text
scv add ./hello.py --name hello --runtime python --category utility --description "Print a greeting" --risk read --network false --supports-dry-run false --effect "prints a greeting" --no-input
```

SCV leaves the original file unchanged, copies it into your command library, and
activates the validated package for the current platform.

### 3. Run and inspect it

```bash
scv hello
# Hello from SCV!

scv hello --help
scv info hello --json
scv list
```

Your command is now available from any directory. Use the same workflow for your
own tools, with metadata that describes their actual inputs and effects.

## AI commands

Install and authenticate one of the supported local CLIs: `codex`, `claude`, or
`agy`. Select it during setup or pass `--agent` per request. Generation uses that
provider's existing authentication and may incur provider usage charges.

| Goal | Command | Where it goes |
| --- | --- | --- |
| Run a task once | `scv "Count files by extension"` | Machine-local history, after execution consent |
| Build a reusable tool | `scv create "Count files in the current directory" --name file-count` | Git-backed library, after installation consent |
| Run a previous task again | `scv history run <id>` | Existing history copy, with fresh consent |

SCV shows preparation, generation, validation, and approval stages. Before consent,
review the description, declared risk, network use, and effects. Generation time
and available provider-reported token, cache, and tool-call statistics appear in
the summary; provider transcripts stay hidden.

```bash
# Use a specific provider.
scv "Count files by extension" --agent claude

# Keep a tool for later use. Creation installs it; execution is a separate step.
scv create "Count files in the current directory" --name file-count --agent codex
scv file-count

# Find an earlier one-shot task, then rerun it by ID.
scv history
scv history run <id>
```

One-shot runs and history reruns use your **current working directory**. History
retains the newest 100 entries and stays outside Git synchronization.

<details>
<summary><strong>Use AI commands without prompts</strong></summary>

```bash
scv "Count files by extension" --agent claude --yes --no-input
scv create "Count files in the current directory" --agent codex --yes --no-input
scv history run <id> --yes --no-input
```

`--no-input` disables prompts; `--yes` explicitly grants execution or installation
consent. Without a terminal, SCV never prompts. See `scv create --help` for model
and effort options; supported values depend on the selected provider.

</details>

## Everyday workflows

| Task | Command |
| --- | --- |
| Discover commands | `scv list` or `scv list --json` |
| Inspect a command | `scv info <command> --json` |
| Read command help | `scv <command> --help` |
| Preview manual source changes | `scv apply --dry-run` |
| Activate source changes | `scv apply` |
| Compare source with the active library | `scv status --json` |
| Preview / perform a rollback | `scv rollback --dry-run` / `scv rollback` |
| Inspect resolved storage paths | `scv paths --json` |

### Sync with Git

Configure your command repository's remote during `scv init`, or use
`scv init --reconfigure` to update it while preserving your source. Create the
hosted repository separately and commit source changes with Git before syncing;
SCV does not create commits for you.

```bash
scv sync status --json
scv sync pull
scv sync push --dry-run
scv sync push
```

Pull validates the fetched source in an isolated Git worktree and asks for approval
before accepting a fast-forward update. Push also asks for approval. For automation,
use `scv sync pull --yes --no-input` or `scv sync push --yes --no-input`.
See the [sync guide](docs/SYNC_ARCHITECTURE.md) for remote setup and recovery.

### Use Korean

```bash
scv config set ui.locale ko
scv "현재 디렉터리의 큰 파일 10개를 보여줘" --locale ko
scv create "현재 디렉터리의 파일 수를 세어줘" --locale ko --agent agy
```

English is the default; English and Korean UI catalogs ship with the binary.
`--locale` sets the language of newly generated descriptions, help, and authored
messages, defaulting to `ui.locale`. Existing packages and command identifiers stay
unchanged. Machine-readable JSON keeps canonical builtin metadata in English.

## How execution works

Persistent commands pass through validation before becoming runnable:

```text
Your scripts / generated packages / Git source
                      │
                      ▼
            Validate the complete library
                      │
                      ▼
        Copy and verify a new activation (SHA-256)
                      │
                      ▼
          Switch the current activation atomically
                      │
                      ▼
                 scv <command>
```

SCV executes persistent commands only from the validated current activation.
Failed validation leaves the previous activation active. One-shot tasks take a
separate path: consent, a revalidated history copy, an integrity check, then
execution. SCV never executes the agent's generation workspace directly.

Validation checks package structure, metadata, integrity, and source syntax when
the runtime is available. It does **not** verify intended behavior; declared safety
metadata does not replace your review or consent.

Your portable source lives under `~/.scv` (`%USERPROFILE%\.scv` on Windows).
Activations, history, preferences, and cache use native platform directories.
Override them with `SCV_HOME`, `SCV_DATA_DIR`, `SCV_CONFIG_DIR`, and `SCV_CACHE_DIR`.
See the [storage guide](docs/STORAGE_ARCHITECTURE.md) for the layout and boundaries.

## Supported runtimes

| Package runtime | Required on the target machine |
| --- | --- |
| Bash | Bash |
| Node.js | `node` |
| Python | `python3` on macOS/Linux; `python` on Windows |
| PowerShell | PowerShell 7+ (`pwsh`) |
| Native binary | Executable built for the target OS and CPU |

Each package declares its runtime and supported platforms in `metadata.toml`.
SCV selects exactly one implementation for the current OS. A package can include
different implementations for Unix and Windows; cross-platform SCV support does
not make every script portable. See the [command package guide](docs/SCRIPT_GUIDE.md)
for the schema, argument help, and platform rules.

## Documentation

| Guide | What you will find |
| --- | --- |
| [Platform and command guide](docs/SCRIPT_GUIDE.md) | Canonical behavior, package metadata, runtimes, and validation |
| [Installation](docs/INSTALLATION.md) | Installer options, PATH, and release artifacts |
| [Generation](docs/CREATE_ARCHITECTURE.md) | Agent adapters, persistent and one-shot modes, consent, and usage reporting |
| [Storage](docs/STORAGE_ARCHITECTURE.md) | Source paths, activations, integrity checks, and history |
| [Git sync](docs/SYNC_ARCHITECTURE.md) | Pull/push requirements, validation, and recovery |

## Contributing

Start with [AGENTS.md](AGENTS.md) and the [development guide](docs/SCRIPT_GUIDE.md).
Keep personal command packages outside this product repository. For local
experiments, use an isolated library through the documented path overrides.

```bash
cargo run -- --help
cargo run -- list --json
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo build --locked
```

[CI](https://github.com/elixirevo/script-command-valet/actions/workflows/ci.yml)
runs native checks on macOS, Linux, and Windows. Found a bug or have a concrete
workflow to improve? [Open an issue](https://github.com/elixirevo/script-command-valet/issues)
with your OS, SCV version, reproduction steps, and expected behavior.
