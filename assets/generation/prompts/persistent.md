# SCV persistent mode

Create a reusable package; SCV validates and installs it after explicit consent.
Generation never executes it. Runtime inputs and multiple supported platforms are
allowed. Start with this metadata shape and adapt it to the actual behavior:

```toml
name = "__COMMAND__"
category = "__CATEGORY__"
description = "__DESCRIPTION__"
usage = "scv __COMMAND__"
builtin = false
risk = "__RISK__"
network = false
supports_dry_run = false
effects = ["__EFFECT__"]
[[implementations]]
runtime = "__RUNTIME__"
platforms = ["__PLATFORM__"]
entry = "__ENTRY__"
```

- `usage` begins with `scv <command>` and matches actual inputs. Keep only one or
  two essential targets positional; configuration/behavior uses named options.
- `[[arguments]]` has `name`, `required`, `description`, and optionally a truthful
  `default`. Never expose one value as both positional and named input.
- Every `[[options]]` entry has `short` or `long` (prefer `long`) and `description`.
  There is no option `name` field. A value-taking option adds `value = "<value>"`
  and optionally a truthful `default`; a boolean flag omits `value`.
- Keep detailed help in metadata, including useful `[[examples]]` with `command`
  and optional `[[notes]]` with `text`. SCV owns `-h`/`--help`: do not declare them
  or write a `show_help`, usage block, or help branch. Validate unknown options,
  missing values, and invalid input; errors end with the exact English literal
  `Try 'scv <command> --help' for more information.`
- Add prompts only if requested. Prompt only on a TTY; declare and implement
  `--no-input`, report the exact missing input when disabled, and support a fully
  specified non-interactive invocation. Destructive consent requires a separate
  `--yes`; `--no-input` never grants it.
- Set `supports_dry_run = true` only with a declared/implemented `--dry-run` that
  prints exact planned targets and a no-change result without any local or remote
  mutation, including caches or temporary output.
