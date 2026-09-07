# SCV shared package contract

## Implementation

- Use the smallest clear, correct implementation. Prefer available system utilities
  for traversal, disk usage (`du`), size formatting, and sorting. Use Python or Node.js
  when structured data or complex logic is simpler there than in a shell pipeline.
- Preserve files versus directories, hidden entries, disk versus apparent size, and
  ordering as requested. Quote paths/values; handle empty input and propagate errors.
  Add functions, classes, configuration, or platform fallbacks only when needed.
  Short scripts need no `main` wrapper.
- Use utilities/flags supported on every declared platform. Do not assume optional
  tools or install dependencies for work existing tools can do. Bundle authored
  resources; never hardcode machine-specific paths or credentials.
- Bash starts with `#!/usr/bin/env bash` and `set -euo pipefail`. PowerShell uses
  `$ErrorActionPreference = "Stop"` and checks native program failures. Keep results
  on stdout, warnings/errors on stderr, and failures non-zero. Metadata belongs only
  in `metadata.toml`, not source comments.

## Package and metadata

- Directory and metadata `name` must match. Choose a concise kebab-case name;
  `help` and the supplied reserved names are forbidden. Names start with an ASCII
  letter/digit and contain only ASCII letters/digits, `.`, `_`, or `-`.
- Entries are existing direct-child files with those same name characters. No `..`,
  slashes, absolute paths, drive prefixes, symlinks, or special files. Keep only
  required source/resources inside the package; category is metadata, not a folder.
- Adapt the selected mode's inline TOML skeleton; replace every placeholder.
  `builtin` is false. `network` and `supports_dry_run` are unquoted TOML booleans,
  `true` or `false`. `effects` is a nonempty list of concrete observable outcomes.
- Declare execution only through `[[implementations]]`, never top-level runtime,
  platforms, or entry. Runtime is `bash`, `node`, `python`, `pwsh` (PowerShell 7+),
  or `binary` (a compiled artifact for the declared OS/CPU, never raw Rust/Go).
  Platforms are `linux`, `macos`, `windows`; each supported OS resolves to exactly
  one implementation. Group OSes only for the same runtime/entry. Never assume
  Bash/WSL on Windows. Python runs as `python3` on Unix and `python` on Windows.

## Safety and language

- Declare the highest risk of any branch: `read` makes no change; `write` creates or
  modifies state; `destructive` deletes, overwrites, prunes, or is hard to reverse.
  Any network access sets `network = true`. Metadata never grants authorization;
  destructive operations need explicit consent as defined by the selected mode.
- All package-authored human-facing metadata and runtime messages use the specified
  language. Keep schema keys, enums, identifiers, names, usage/example syntax, file
  names, and required protocol literals canonical. Store one language, no parallel
  translations. External utility output/diagnostics keep their native language;
  never reimplement or wrap a utility just to translate it or hide its failures.
