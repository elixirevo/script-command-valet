# SCV Storage and Activation Architecture

## Goals

SCV separates portable, user-owned command source from executable machine-local
state. Git synchronization must never make newly pulled code immediately executable.

## Source home

The source home is `~/.scv` on Unix and `%USERPROFILE%\.scv` on Windows. `SCV_HOME`
may override it.

```text
~/.scv/
├── .git/                  optional but required by scv sync
├── scv.toml               source schema and library format
└── commands/
    └── <command>/
        ├── metadata.toml
        └── <entries/resources>
```

Only `scv.toml` and `commands/` are part of the portable SCV contract. Credentials,
agent configuration, cache, activation state, and trust state do not belong here.

## Platform data

| Purpose | macOS | Linux | Windows |
|---|---|---|---|
| Source | `~/.scv` | `~/.scv` | `%USERPROFILE%\.scv` |
| Data | `~/Library/Application Support/scv` | `~/.local/share/scv` | `%LOCALAPPDATA%\scv` |
| Config | Application Support `scv` | `~/.config/scv` | `%APPDATA%\scv` |
| Cache | `~/Library/Caches/scv` | `~/.cache/scv` | `%LOCALAPPDATA%\scv\cache` |

Rust resolves native locations through `directories::ProjectDirs`. Supported
overrides are `SCV_HOME`, `SCV_DATA_DIR`, `SCV_CONFIG_DIR`, and `SCV_CACHE_DIR`.
SCV rejects overrides that make the Git source overlap data, config, or cache.
The config directory contains machine-local preferences such as `ui.locale` and
agent defaults. English and Korean locale catalogs are compiled into the executable;
SCV does not download language packs at runtime.

## Activation layout

```text
<data>/
├── activations/
│   └── <id>/
│       ├── manifest.toml
│       └── commands/
├── history/
│   └── <id>/
│       ├── manifest.toml
│       └── package/<command>/
├── current
└── state/
```

An activation ID contains only ASCII letters, digits, and hyphens. `current` stores
the ID as text rather than relying on cross-platform symlink privileges. The
dispatcher resolves `<data>/activations/<current>/commands` at process startup.

The manifest records:

- activation schema version and ID;
- creation time, SCV version, and platform;
- source Git revision when available;
- SHA-256 digest of the full command library; and
- name, SHA-256 digest, and risk for every package.

## Apply transaction

1. Validate `scv.toml`.
2. Reject non-package entries, symlinks, special files, unsafe names, oversized
   packages, invalid metadata, duplicate platform implementations, and missing
   entries.
3. Run available non-executing runtime syntax checks.
4. Hash the validated source library.
5. Copy all packages into a staging activation.
6. Validate and hash the copied library again.
7. Write `manifest.toml` and rename staging to its immutable activation ID.
8. Atomically replace the `current` text file.
9. Retain the ten newest activations, never deleting the current activation.

Before external dispatch, SCV revalidates the selected package and compares its
SHA-256 digest with the current activation manifest. Local activation tampering is
therefore rejected with an `apply`/`rollback` recovery instruction.

Any error before step 8 leaves the previous activation current. A committed but
unused activation may remain if only the final pointer write fails; it is harmless
and may be cleaned by a later successful apply.

## One-shot history

`scv "<request>"` never writes to the Git-backed source or activation tree. The
selected agent writes only to its isolated generation workspace. After package
validation and user consent, SCV copies the package to a staged history entry,
revalidates the copy, computes its SHA-256 digest, writes a machine-local manifest,
and atomically renames the entry into `<data>/history/<id>/` before execution.

The manifest records the request, output locale, agent and optional model, original
working directory, package safety summary, creation time, and package digest. This
data can contain user-supplied text, so the history directory is private to the user
on Unix. History is not portable and is never synchronized by Git.

`scv history run <id>` validates the package and compares its digest with the
manifest before every rerun. Initial execution and reruns both use the caller's
current working directory and require confirmation; `--no-input` requires separate
`--yes` authorization. SCV retains the newest 100 entries and removes older entries
after a new entry has been committed. History tampering rejects execution.

## Product repository boundary

The SCV product repository does not contain a user command library, root `scv.toml`,
or repository launcher. `cargo run -- <arguments>` follows the same path discovery
and activation rules as an installed binary. Automated tests use isolated path
overrides and never dispatch source directly.

SCV does not auto-discover a nearby repository and never interprets `SCV_DATA_DIR`
as source storage.

On a first interactive argument-free run, SCV offers `scv init`. Init creates the
source manifest and command directory, initializes Git metadata inside `SCV_HOME`,
and writes locale and create-agent preferences to machine-local config. `--dry-run`
creates none of these paths. Help, version, explicit commands, and non-TTY execution
do not implicitly initialize storage.
