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

## Product repository boundary

The SCV product repository does not contain a user command library, root `scv.toml`,
or repository launcher. `cargo run -- <arguments>` follows the same path discovery
and activation rules as an installed binary. Automated tests use isolated path
overrides and never dispatch source directly.

SCV does not auto-discover a nearby repository and never interprets `SCV_DATA_DIR`
as source storage.
