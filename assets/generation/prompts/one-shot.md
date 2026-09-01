# SCV one-shot generation mode

Generate a zero-input package for one immediate execution after SCV validation and
explicit user consent. SCV copies the validated package into machine-local history,
revalidates and hashes that copy, and executes only the stored copy. The agent must
never execute or import the implementation during generation.

## Exact one-shot contract

- Choose a concise internal kebab-case package name.
- Set metadata `usage` to exactly `scv <generated-name>` with no suffix.
- Declare no `[[arguments]]` and no `[[options]]` entries.
- Set `supports_dry_run = false`.
- If `[[examples]]` is present, every example is exactly `scv <generated-name>`.
- Provide an implementation for the current platform. Additional implementations
  are allowed only when they are complete and truthful; they are not required.
- Perform the complete requested behavior with no command-line input and no prompts.
- Resolve relative paths from the process current working directory. Never hardcode
  or depend on the generation workspace path.
- Do not add `--yes`, `--no-input`, `--dry-run`, help handling, configuration
  options, or placeholder input. SCV owns approval and invocation for this mode.
- Do not assume the package will enter the Git-backed command source or an
  activation. Package resources must still be self-contained for history replay.

## One-shot completion checks

1. Confirm usage and every example use only `scv <generated-name>`.
2. Confirm metadata contains no arguments or options and dry-run support is false.
3. Confirm the current platform resolves to exactly one implementation.
4. Confirm the implementation uses no command-line input, cannot prompt, and uses
   the process current working directory for relative paths.
