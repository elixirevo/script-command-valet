# SCV persistent command generation mode

Generate a reusable command package that SCV will install into the user's Git-backed
source after validation and explicit consent. The command may accept runtime inputs
and may support multiple platforms. It is not executed during generation.

## Usage, arguments, options, and help

- Metadata `usage` begins with `scv <command>` and truthfully shows every positional
  argument and option accepted by the implementation.
- Keep only essential targets or identifiers positional, normally one and at most
  two.
- Every `[[arguments]]` entry has `name`, `required`, and `description`; `default` is
  allowed only when it matches real implementation behavior.
- Model paths, formats, configuration values, and behavior switches as named
  options.
- Every `[[options]]` entry contains at least one of `short` or `long`, preferably a
  stable `long`, plus `description`. There is no option `name` field.
- A value-taking option uses `value = "<value>"` and may declare a truthful
  `default`. A boolean flag omits `value`.
- Never declare `-h` or `--help`; SCV intercepts both and renders help from metadata.
- `usage`, arguments, options, examples, and implementation parsing must agree. Do
  not expose one value as both positional and named input.

Detailed help belongs in metadata. Implementations validate input but do not include
a `show_help` function, usage block, or help-option branch. Invalid input goes to
stderr, exits non-zero, and ends with:

```text
Try 'scv <command> --help' for more information.
```

Keep that required protocol literal in canonical English even when the generated
human-facing language is different.

## Interaction and automation

Every workflow has a fully specified non-interactive invocation. Prompt only for a
missing value when interactive behavior was requested and stdin is a TTY.

If the command can prompt:

- implement and declare `--no-input`;
- refuse to prompt when stdin is not a TTY;
- fail with the exact missing argument or option when input is disabled; and
- keep destructive consent in a separate `--yes` flag.

`--no-input` never implies destructive authorization. Do not add interaction when
the request can be satisfied with ordinary arguments and options.

Set `supports_dry_run = true` only when the implementation exposes and declares a
`--dry-run` flag that makes no local or remote change, prints the planned targets,
and clearly states that no changes were made. A dry-run may read state but cannot
create cache files, temporary output in the package, or remote mutations.

## Persistent-mode completion checks

1. Confirm metadata usage, arguments, options, examples, and implementation parsing
   agree.
2. Confirm every option uses `short` or `long` and no option uses `name`.
3. Confirm every prompt-capable workflow has a complete `--no-input` invocation and
   separate destructive consent.
4. Confirm declared dry-run behavior cannot mutate local or remote state.
