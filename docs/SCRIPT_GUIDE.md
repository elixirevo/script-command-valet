# SCV Platform and Command Development Guide

This document is the canonical contract for developing the SCV platform and command
packages. If the implementation, templates, embedded prompts, Skill, or README
conflicts with this guide, update them together using this guide as the authority.

## 1. Product boundary and public interface

- The only public executable and namespace is `scv`, or `scv.exe` on Windows.
- Releases, development code, documentation, and environment variables must not
  expose another executable name or compatibility alias.
- Dispatch, help, `init`, `add`, `rm`, `list`, `info`, `config`, `create`, `history`,
  `apply`, `status`, `rollback`, `paths`, and `sync` are native Rust builtins.
- External commands exist only in the user's `SCV_HOME/commands/<command>/` packages.
  The product repository does not contain a user command library.
- Builtin metadata lives in `src/builtin/metadata/`; external metadata lives in each
  package's `metadata.toml`.
- The product repository must not contain `bin/`, a root `commands/`, or a root
  `scv.toml`. Use `cargo run -- <arguments>` for development.
- Persistent commands use `scv <command> [arguments]`. A natural-language request
  containing whitespace or non-ASCII text uses `scv "<request>" [options]` for one
  validated, confirmed execution.

SCV is a pre-release greenfield application. Do not implement legacy data paths,
environment variables, flat metadata, migration commands, or deprecation aliases.

## 2. Source and execution boundary

An installed SCV never executes code directly from its Git source or an agent's
generation workspace.

```text
~/.scv/commands/                 portable source
          ↓ full validation
<data>/scv/activations/<id>/     immutable machine-local copy
          ↓ atomic current switch
scv <command>                    current activation only
```

- The source manifest is `~/.scv/scv.toml`.
- Source commands live in `~/.scv/commands/<command>/`.
- Only `apply`, `add`, `create`, `rm`, and a validated `sync pull` create an
  activation.
- An activation records the package copy, manifest, and SHA-256 library and package
  digests.
- SCV fully copies and revalidates a new activation before atomically replacing the
  `current` file.
- Validation or apply failure leaves the previous activation active.
- Product development has no exception that dispatches source directly.
- A one-shot request is copied from the isolated generation workspace to
  `<data>/history/<id>/package/<command>/`, revalidated, hashed, and recorded in a
  machine-local manifest before execution. It never enters `SCV_HOME` or an
  activation.
- SCV executes one-shot history only after revalidating the stored package and
  comparing its SHA-256 digest. Both initial runs and reruns require explicit
  confirmation, or `--yes` on the fully specified non-interactive path.
- One-shot execution and `scv history run <id>` use the caller's current working
  directory. The original directory is stored for audit display only.
- SCV retains the newest 100 one-shot entries and removes older entries only after a
  new entry has been committed. Report cleanup only when entries were removed or
  cleanup failed; keep retention and validation limits in detailed help.

Follow `docs/STORAGE_ARCHITECTURE.md` and `docs/SYNC_ARCHITECTURE.md` for detailed
path and synchronization contracts.

## 3. Command package structure

```text
~/.scv/commands/<command>/
├── metadata.toml
├── <implementation entry>
└── <required package resources>
```

- The package directory and metadata `name` must match.
- Command, category, and option names use kebab-case.
- Names begin with an ASCII letter or digit and contain only ASCII letters, digits,
  `.`, `_`, and `-`.
- `help` is a reserved command name.
- An implementation `entry` is a direct-child file in the package. It must not
  contain `/`, `\`, an absolute path, a drive prefix, `..`, or a symlink.
- A package contains every resource required at runtime.
- Reject symlinks, sockets, devices, special files, and packages that exceed file
  count, size, or depth limits.

## 4. Metadata contract

The minimal external command shape is:

```toml
name = "gh-org-clone"
category = "github"
description = "Clone repositories from a GitHub organization."
usage = "scv gh-org-clone <organization> [--directory <path>]"
builtin = false
risk = "write"
network = true
supports_dry_run = true
effects = [
  "creates local directories",
  "downloads Git repositories",
]

[[implementations]]
runtime = "bash"
platforms = ["linux", "macos"]
entry = "main.sh"

[[arguments]]
name = "organization"
required = true
description = "GitHub organization name"

[[options]]
short = "-d"
long = "--directory"
value = "<path>"
description = "Destination directory for clones"
default = "Current directory"

[[options]]
long = "--dry-run"
description = "Show planned changes without modifying anything"

[[examples]]
command = "scv gh-org-clone project --directory ./backup"
```

External commands do not use top-level `runtime`, `platforms`, or `entry`. They
declare the execution contract only through one or more `[[implementations]]`
entries.

Builtins use `builtin = true`, `runtime = "builtin"`, and top-level `platforms`.
They have neither `entry` nor `[[implementations]]`.

### Detailed help

- `usage` begins with `scv <command>`.
- Keep only essential targets positional, normally one and at most two.
- Model paths, formats, configuration, and behavioral changes as long options.
- Frequently used options may have an unambiguous short alias.
- Every `[[options]]` entry contains at least one of `short` or `long`.
- A value-taking option uses `value = "<value>"`. A boolean flag omits `value`.
- Do not declare `-h` or `--help`; the SCV core adds both.
- Keep `[[notes]]` and `[[examples]]` aligned with the implementation.
- Source executables do not contain a `show_help` function, detailed Usage block, or
  help-option branch.

The SCV core renders the same metadata for:

```bash
scv <command> -h
scv <command> --help
scv list --json
scv info <command> --json
```

Do not mix progress messages or explanatory text into JSON stdout.

### Localization

- English is the canonical language for Rust builtin metadata, embedded generation
  resources, errors that are not yet catalogued, and documentation examples.
- SCV embeds complete `en` and `ko` catalogs at build time. The default UI locale is
  `en`; `scv config set ui.locale <en|ko>` changes the machine-local preference.
- Locale catalogs may translate only core-owned human output and builtin metadata
  views. They do not rewrite user-owned external command metadata.
- `--locale <en|ko>` on persistent or one-shot generation selects the authored
  language for a new package; when omitted it uses `ui.locale`. The generated package
  stores one language for its free-form metadata and runtime messages, so changing
  `ui.locale` later does not translate an existing user command or history package.
- Generated localization never translates schema keys, enum values, identifiers,
  command or option names, usage syntax, file names, or required protocol literals.
- Output and diagnostics from external utilities retain their native language.
  Do not reimplement or wrap a utility solely to translate its output; localize
  package-authored messages instead.
- `scv list --json`, `scv info <command> --json`, and other JSON contracts remain
  locale-independent. In particular, builtin metadata serialized as JSON remains
  the canonical English metadata.
- Keep a locale catalog structurally aligned with every builtin description,
  effect, argument, option, note, and localized default. A missing translation is a
  test failure, not a release-time network fetch.

## 5. Runtimes and platforms

| Runtime | SCV execution | Requirement |
|---|---|---|
| `bash` | `bash <entry>` | Bash, normally on Linux and macOS |
| `node` | `node <entry>` | Node.js |
| `python` | Unix `python3`, Windows `python` | Python |
| `pwsh` | `pwsh <entry>` | PowerShell 7+ |
| `binary` | Execute entry directly | Native binary for the target OS and CPU |

- Exactly one implementation must resolve for the current OS.
- Group platforms only when they share the same runtime and entry.
- Never assume Bash or WSL for a Windows implementation.
- Do not register raw Rust or Go source as `binary`.
- Shebangs, extensions, and Unix executable bits are not part of the dispatch
  contract.
- Source validation runs a non-executing syntax check when the runtime is available.
  It explicitly reports `skipped` when the runtime is unavailable.
- Syntax checks pass source paths as literal data, including spaces, Unicode, and
  shell metacharacters. PowerShell validation uses its parser without executing
  the source; Windows CI requires `pwsh` so this check cannot silently be skipped.

## 6. Input, interaction, and errors

- Use positional arguments for required targets, options for configuration, and flags
  for booleans.
- Do not expose the same value as both positional and named input.
- Unknown options, missing values, and invalid input go to stderr and return a
  non-zero status.
- End input errors with `Try 'scv <command> --help' for more information.`
- Send successful results to stdout and warnings or errors to stderr.
- Quote paths and user-controlled values.
- Every interactive workflow has a fully specified non-interactive invocation.
- Prompt only when stdin is a TTY.
- A prompt-capable command implements and declares `--no-input`.
- `--no-input` is not consent. Destructive behavior requires a separate `--yes`.
- Do not add compatibility input aliases.

When `supports_dry_run = true`, declare `--dry-run` in metadata. It must make no
local or remote change and must print the exact targets and the no-change result.

## 7. Safety metadata

- `read`: reads state without creating a local or remote change.
- `write`: creates or modifies local or remote state.
- `destructive`: deletes, overwrites, prunes, or may cause a hard-to-reverse change.
- Record the highest risk across every possible branch.
- Set `network = true` for any network access, including read-only access.
- Describe observable outcomes in `effects`; do not use vague text such as `runs a
  script`.

Safety metadata is not authorization. Dry-run, preview, confirmation, and external
system approval remain separate behavioral contracts.

## 8. Language-specific source rules

- Prefer the smallest clear implementation that satisfies the request. Reuse
  available system utilities when their behavior matches; do not recreate directory
  traversal, disk-usage measurement, size formatting, or sorting already provided by
  those utilities. Use Python or Node.js when structured data or complex logic makes
  them simpler and more accurate than a shell pipeline.
- Preserve requested semantics, including files versus directories, hidden entries,
  disk usage versus apparent size, and ordering. Keep necessary path quoting,
  empty-input handling, and failure propagation. Simplicity does not justify hiding
  errors or dropping requested behavior.
- Add functions, classes, configuration, and platform fallbacks only when the task
  needs them. A short straight-line script does not need a `main` wrapper.
- Self-contained packages may call standard OS utilities and their declared runtime.
  Package authored resources, choose utilities and flags supported on every declared
  platform, and do not assume optional tools are installed. Do not add dependency
  installation to a task that can use existing tools.
- Manage the source starting points used by agent-powered generation under
  `assets/generation/templates/` and materialize them into the temporary generation
  workspace.
- Keep repository-owned text, embedded prompts, and generation templates on LF line
  endings through `.gitattributes` so compiled resources are platform-independent.
- Bash uses `#!/usr/bin/env bash` and `set -euo pipefail`.
- Node.js, Python, and PowerShell follow the input and error conventions in their
  generation templates.
- Do not hardcode a home directory, repository absolute path, or credential.
- If a package command needs machine-local state, use the environment supplied by
  SCV.

SCV provides these variables at execution time:

- `SCV_HOME`
- `SCV_DATA_DIR`
- `SCV_CONFIG_DIR`
- `SCV_CACHE_DIR`
- `SCV_ACTIVE_DIR`
- `SCV_COMMAND_PACKAGE_DIR`
- `SCV_COMMAND_METADATA`

## 9. Management commands

### First-run setup and `scv init`

- An argument-free first run starts interactive setup only when `scv.toml` is
  missing and stdin is a terminal. Help, version, explicit commands, and non-TTY
  runs never trigger setup as a side effect.
- Setup selects the embedded `en` or `ko` locale, a default `codex`, `claude`, or
  `agy` create adapter, and an optional Git `origin`.
- Interactive setup uses arrow-key selection and Enter for locale, agent, and Git
  origin actions. It requests free-form text only for a new origin. Esc or Ctrl+C
  cancels the workflow before source, Git configuration, or preferences change.
- `scv init --reconfigure` reruns the explicit setup workflow while preserving the
  source manifest and command packages. Current values are the initially selected
  choices; origin actions explicitly keep, replace, or remove it.
- Non-interactive reconfiguration uses explicit `--locale`, `--agent`, and
  `--remote`; omitted values keep their current settings, while `--no-remote`
  removes `origin`. `--remote` and `--no-remote` are mutually exclusive.
- Reconfiguration repairs missing local Git metadata inside an otherwise initialized
  `SCV_HOME`. It never contacts the remote, commits, pushes, or replaces command
  source content.
- `scv init --no-input` is fully specified by defaults. `--dry-run` validates and
  prints all target paths and values without creating source, config, or Git state.
- Init creates the local source manifest and Git repository. An optional remote is
  configuration only: init does not contact a network, create a hosted repository,
  store credentials, commit, or push.
- Reject HTTP(S) remotes containing user information, query credentials, or
  fragments. Initial setup never overwrites a conflicting existing `origin`;
  replacement or removal requires explicit reconfiguration input.

### `scv add`

- Do not modify the input source file.
- Create the implementation and metadata under `~/.scv/commands/<name>/`.
- Validate the package, then apply the complete source as an activation.
- Automation specifies category, description, risk, network, dry-run support, and
  every effect, and uses `--no-input`.

### `scv rm`

- A builtin cannot be removed.
- `--dry-run` prints only the source package and apply plan.
- Actual removal requires TTY confirmation or `--yes`.
- Reapply the complete source after removal.

### `scv apply/status/rollback`

- `apply --dry-run` validates source only.
- `apply` creates a new activation.
- `status` compares the source and active digests and reports pending state.
- `rollback` revalidates an existing activation and atomically switches `current`.

### `scv sync`

- Delegate authentication and repository operations to system Git or `gh`. Do not
  treat remote source as executable before validation and approval.
- Follow `docs/SYNC_ARCHITECTURE.md` for the detailed pull/push trust boundary and
  recovery contract.

## 10. Agent-powered generation

Agent-powered generation is optional. `scv create` turns natural language into a
persistent command package; `scv "<request>"` generates a zero-input package for one
confirmed execution and machine-local history.

- Keep deployed resources under `assets/generation/`.
- Embedded prompts must be complete without external `AGENTS.md`, `CLAUDE.md`, Skill,
  or `docs/` content.
- Compose every request from a minimal shared generation/package contract and exactly
  one mode contract. Persistent-only argument, option, help, prompt, and dry-run
  guidance must not appear in a one-shot prompt. Include a runtime-neutral metadata
  skeleton in the selected mode contract so ordinary requests need no template read.
- Materialize only the selected mode's metadata and source templates under the
  workspace `templates/` names. One-shot templates use exact zero-input usage and do
  not contain persistent help, interaction, or mandatory function scaffolding.
  Templates are optional references; read only a relevant template when needed.
- For straightforward requests, instruct the agent to batch metadata and source
  writes, skip planning/exploration/probing and redundant rereads, and finish with
  the package name only. Agents do not run tests, package validation, or syntax
  checks; SCV performs its full validation after generation. This reduces model
  round trips without changing runtime checks, confinement, or consent.
- Apply the implementation-choice rules in section 8 to both generation modes.
  One-shot generation targets the current platform without speculative extra
  implementations. Shared metadata guidance must not prescribe a runtime by example.
- SCV injects the prompt explicitly and does not depend on provider file discovery.
- Both generation modes hide provider stdout and stderr, including failure
  transcripts. Show SCV-owned preparation, generation, validation, and approval
  stages in `ui.locale` on stderr. Animate a spinner only on a capable terminal;
  redirected output and `TERM=dumb` receive plain stage lines. Stop the spinner
  before displaying errors, the validated summary, or the consent prompt.
- After successful generation and validation, show elapsed time and reported input,
  output, and cumulative total tokens on stderr in `ui.locale`. Show reported cache
  reads and non-cached input separately when their relationship is defined, plus
  distinct observed tool calls (not model requests). Missing cache details or tool
  counts are unavailable, not zero. Measure from workspace
  preparation through validation with a monotonic clock; exclude approval waiting,
  installation, history storage, and command execution. Do not estimate missing
  usage or present it as zero. Parse bounded provider completion events without
  displaying or persisting transcripts, and never double-count cached or reasoning
  tokens already included in a provider's totals. Keep event and temporary identifier
  storage bounded; incomplete or discarded events must not produce partial tool
  counts presented as complete.
- Approval previews retain the command description, risk, network use, and effects.
  Use a short review reminder and execution question for one-shot consent.
  One-shot execution output is visible after consent; persistent creation asks for
  installation consent and does not execute the package.
- The explicit prompt places the selected output language outside the escaped,
  untrusted user request and defines exactly which human-facing fields are localized.
- The adapter uses a temporary workspace as its working directory and receives no SCV
  source or activation path. SCV consumes only results under `generated/`.
- Never execute from the generation workspace. Persistent `create` output is
  installed only after validation and approval. One-shot output is validated and
  previewed before approval, then copied to history, revalidated, hashed, and
  executed from that stored copy.
- One-shot packages declare no arguments or options, support the current platform,
  do not prompt, and resolve relative paths from the process current working
  directory.
- One-shot metadata usage, and every example when present, is exactly
  `scv <generated-name>`; the validator rejects persistent-style usage suffixes.
- Model and provider options cannot alter SCV's sandbox, approval, network, or other
  security boundaries.
- Supported local agent commands are `codex`, `claude`, and `agy`. An adapter must
  reject a requested effort or provider option that its CLI cannot represent rather
  than silently mapping it.

Follow `docs/CREATE_ARCHITECTURE.md` for the detailed contract.

## 11. Installation and path changes

When paths, activation, shell PATH, installers, or platform implementations change,
update these in the same change:

- `docs/SCRIPT_GUIDE.md`
- the relevant architecture document;
- affected builtin metadata and `assets/generation/` resources;
- the canonical Skill and agent entrypoint;
- `AGENTS.md` and README; and
- the macOS, Linux, and Windows CI and release matrix.

The standalone executable defaults to `~/.local/bin/scv` on macOS and Linux and
`%LOCALAPPDATA%\Programs\SCV\bin\scv.exe` on Windows. Package managers own their
installation paths. Follow `docs/INSTALLATION.md` for details.

## 12. Verification and completion criteria

Core changes:

```bash
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo build --locked
```

Command changes:

```bash
cargo run -- --help
cargo run -- list --json
cargo run -- info <command> --json
cargo run -- <command> -h
cargo run -- <command> --help
cargo run -- <command> <valid-input>
cargo run -- <command> <invalid-input>
```

Additional completion criteria:

- Package name, metadata, runtime, platform, and entry agree.
- Every implementation entry is a regular, non-symlink file inside the package.
- Risk, network, effects, and dry-run metadata match the maximum actual effect.
- Verify `--no-input`, consent, and non-TTY behavior for prompt workflows.
- Exercise paths containing spaces and the current-platform implementation.
- Verify that source changes are not executed before activation.
- Verify that invalid source does not change the current activation.
- Verify rollback and local-bare-Git sync end to end.
- Verify repeated installer runs in isolated paths without duplicate PATH entries.
- Run the native `scv` or `scv.exe` in Linux, macOS, and Windows CI.
- Default CI does not use live credentials, mutate networks, or make paid agent
  requests.
