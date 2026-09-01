# SCV generated command-package contract

This document is the shared authoring contract for packages created by SCV's
persistent and one-shot generation modes. Exactly one injected mode contract adds
the remaining lifecycle and invocation requirements.

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
usage = "scv path-size"
builtin = false
risk = "read"
network = false
supports_dry_run = false
effects = ["reads local path metadata", "prints path sizes"]

[[implementations]]
runtime = "python"
platforms = ["linux", "macos", "windows"]
entry = "main.py"

[[examples]]
command = "scv path-size"
```

All top-level fields shown above are required. `network` and `supports_dry_run` are
unquoted TOML booleans, `true` or `false`; they are never strings. `effects` contains
one or more concrete observable effects.

`builtin` is always `false`. Generated packages never use builtin metadata fields.

### Localization

The SCV generation request specifies one output language. Write all free-form
human-facing text in that language:

- metadata `description`, `effects`, mode-allowed argument and option descriptions,
  defaults, and notes;
- implementation success, warning, validation, and failure messages; and
- other package-owned text shown to the command user.

Keep schema keys and enum values, command/category/argument/option names, usage and
example syntax, file names, and runtime/platform/risk tokens in their canonical
form. Do not create parallel locale files or multiple translations in one package.
The generated package stores the single selected language as authored text.

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

## Safety contract

Declare the maximum possible risk:

- `read`: reads state and makes no local or remote change;
- `write`: creates or modifies local or remote state;
- `destructive`: deletes, overwrites, prunes, or performs a hard-to-reverse change.

Use the highest risk of any branch. Set `network = true` for any network access,
including read-only requests and dry-run network lookups. Describe actual outcomes in
`effects`, such as `creates local directories`; do not use vague effects such as
`runs a script`.

The selected generation-mode contract defines whether arguments, options, prompts,
or dry-run behavior are allowed. Follow the stricter mode contract whenever it
narrows this common package contract.

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
3. Confirm TOML booleans are unquoted and safety fields describe the maximum effect.
4. Confirm every declared entry and resource remains inside the package.
5. Run only non-executing syntax checks available for source entries; never execute
   or import an implementation.
6. Remove caches, bytecode, compiled output, test artifacts, and unrelated files.
