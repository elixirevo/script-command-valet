# SCV generated command-package contract

This document is the complete authoring contract for packages created by `scv create`.

## Package shape and names

```text
generated/<command>/
  metadata.toml
  <implementation entries and package resources>
```

- The package directory and metadata `name` must be identical.
- Use a flat, concise kebab-case public name. Category is metadata, not a directory.
- Names begin with an ASCII letter or digit and may contain ASCII letters, digits,
  `.`, `_`, and `-`. `help` and names listed as reserved in the request are forbidden.
- Every implementation entry is a direct child of the package. It begins with an
  ASCII letter or digit and may contain only ASCII letters, digits, `.`, `_`, and `-`.
- Do not use `/`, `\\`, `..`, a drive prefix, an absolute path, or a symlink entry.
- Keep the package small, self-contained, and free of generated caches or build output.

## Required metadata

Start from `templates/command.toml`. A generated external command uses this shape:

```toml
name = "path-size"
category = "filesystem"
description = "Show the size of a path."
usage = "scv path-size <path> [--all]"
builtin = false
risk = "read"
network = false
supports_dry_run = false
effects = ["reads local path metadata", "prints path sizes"]

[[implementations]]
runtime = "python"
platforms = ["linux", "macos", "windows"]
entry = "main.py"

[[arguments]]
name = "path"
required = true
description = "Path to inspect"

[[options]]
long = "--all"
description = "Include hidden entries"

[[examples]]
command = "scv path-size . --all"
```

All top-level fields shown above are required. `network` and `supports_dry_run` are
unquoted TOML booleans, `true` or `false`; they are never strings. `effects` contains
one or more concrete observable effects.

`builtin` is always `false`. Generated packages never use builtin metadata fields.

### Localization

The SCV generation request specifies one output language. Write all free-form
human-facing text in that language:

- metadata `description`, `effects`, argument and option descriptions, defaults, and
  notes;
- implementation success, warning, validation, and failure messages; and
- other package-owned text shown to the command user.

Keep schema keys and enum values, command/category/argument/option names, usage and
example syntax, file names, runtime/platform/risk tokens, and the required
`Try 'scv <command> --help' for more information.` suffix in their canonical form.
Do not create parallel locale files or multiple translations in one package. The
generated package stores the single selected language as authored text.

### Implementations

Declare one or more `[[implementations]]` entries. Every entry requires:

- `runtime`: one of `bash`, `node`, `python`, `pwsh`, or `binary`.
- `platforms`: one or more of `linux`, `macos`, or `windows`.
- `entry`: an existing direct-child file in the package.

A platform must resolve to exactly one implementation. Group platforms only when
they share the same runtime and entry. Do not claim a platform the implementation
does not actually support.

Runtime behavior is:

| Runtime | SCV execution | Platform expectation |
|---|---|---|
| `bash` | `bash <entry>` | Normally Linux and macOS; never assume Bash on Windows |
| `node` | `node <entry>` | Only platforms actually supported by the source |
| `python` | `python3 <entry>`; Windows uses `python` | Only platforms supported by the source |
| `pwsh` | `pwsh <entry>` | PowerShell 7+ |
| `binary` | Execute entry directly | A built binary compatible with each declared OS/CPU |

Do not package raw Rust, Go, or other compiled-language source as `binary`; that
runtime requires a compiled artifact.

### Arguments and options

- Keep only essential targets or identifiers positional, normally one and at most two.
- Every `[[arguments]]` entry has `name`, `required`, and `description`; `default` is
  allowed only when it matches real implementation behavior.
- Model paths, formats, configuration values, and behavior switches as named options.
- Every `[[options]]` entry contains at least one of `short` or `long`, preferably a
  stable `long`, plus `description`.
- A value-taking option uses `value = "<value>"` and may declare a truthful `default`.
- A boolean flag omits `value`.
- There is no option `name` field.
- Never declare `-h` or `--help`; SCV intercepts both and renders help from metadata.
- `usage`, arguments, options, examples, and implementation parsing must agree.
- Do not expose one value as both positional and named input.

Detailed help belongs in metadata. Implementations validate input but do not include
a `show_help` function, usage block, or help-option branch. Invalid input goes to
stderr, exits non-zero, and ends with:

```text
Try 'scv <command> --help' for more information.
```

## Interaction and automation

Every workflow must have a fully specified non-interactive invocation. Prompt only
for a missing value when interactive behavior was requested and stdin is a TTY.

If the command can prompt:

- implement and declare `--no-input`;
- refuse to prompt when stdin is not a TTY;
- fail with the exact missing argument or option when input is disabled; and
- keep destructive consent in a separate `--yes` flag.

`--no-input` never implies destructive authorization. Do not add interaction when
the request can be satisfied with ordinary arguments and options.

## Safety contract

Declare the maximum possible risk:

- `read`: reads state and makes no local or remote change;
- `write`: creates or modifies local or remote state;
- `destructive`: deletes, overwrites, prunes, or performs a hard-to-reverse change.

Use the highest risk of any branch. Set `network = true` for any network access,
including read-only requests and dry-run network lookups. Describe actual outcomes in
`effects`, such as `creates local directories`; do not use vague effects such as
`runs a script`.

Set `supports_dry_run = true` only when the implementation exposes and declares a
`--dry-run` flag that makes no local or remote change, prints the planned targets,
and clearly states that no changes were made. A dry-run may read state but cannot
create cache files, temporary output in the package, or remote mutations.

Safety metadata describes behavior and never grants authorization. Destructive
operations still require a separate explicit confirmation mechanism.

## Source implementation rules

- Use the matching file in `templates/` as the starting point.
- Keep metadata out of source comments; `metadata.toml` is the execution contract.
- Quote paths and user-controlled values.
- Do not hardcode machine-specific absolute paths, credentials, or undeclared runtime
  assumptions.
- Keep stdout for successful results and stderr for warnings and errors.
- Use non-zero exit status for invalid input and execution failures.
- Bash starts with `#!/usr/bin/env bash` and `set -euo pipefail`.
- Node.js, Python, and PowerShell implementations follow their supplied templates.
- Package every implementation and static resource required at runtime.

## Completion checks

Before finishing:

1. Confirm exactly one directory exists directly under `generated/`.
2. Confirm directory name, metadata `name`, usage, entries, runtimes, and platforms agree.
3. Confirm every option uses `short` or `long` and no option uses `name`.
4. Confirm TOML booleans are unquoted and safety fields describe the maximum effect.
5. Confirm every declared entry and resource remains inside the package.
6. Run only non-executing syntax checks available for source entries; never execute
   or import an implementation.
7. Remove caches, bytecode, compiled output, test artifacts, and unrelated files.
