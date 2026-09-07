---
name: scv-development
description: Develop and review the SCV product repository, including Rust core behavior, metadata contracts, agent generation, activation and sync boundaries, platform paths, installers, CI, and releases. Use for changes to the SCV application itself; do not use to author or operate user-owned SCV command packages.
---

# SCV Development

Keep this skill focused on the SCV product. User-owned command packages live outside
the product repository and are not an implementation workflow for this skill.

## Establish context

1. Resolve the repository root and read `AGENTS.md` and `docs/SCRIPT_GUIDE.md`
   completely before changing product behavior.
2. Read only the architecture document relevant to the change:
   - `docs/CREATE_ARCHITECTURE.md` for generation and agent adapters;
   - `docs/STORAGE_ARCHITECTURE.md` for paths, source, and activation;
   - `docs/SYNC_ARCHITECTURE.md` for Git synchronization;
   - `docs/INSTALLATION.md` for installers and releases.
3. Inspect the affected Rust modules, builtin TOML, embedded assets, templates, and
   workflows before choosing an implementation.

Treat `docs/SCRIPT_GUIDE.md` as the canonical product contract. Do not duplicate its
full command schema or platform rules in this skill.

## Classify the change

Determine which product area is in scope:

- Rust CLI dispatch, help, builtin behavior, configuration, or agent adapters;
- metadata parsing, validation, safety fields, runtime selection, or templates;
- isolated `scv create` generation and embedded provider-neutral resources;
- source, immutable activation, rollback, tamper detection, or Git sync;
- native platform paths, installation, CI, release packaging, or documentation.

Ask for clarification only when a missing decision changes public behavior, data
layout, security boundaries, or external side effects.

## Preserve product invariants

- Ship only the `scv` or `scv.exe` namespace. Do not add compatibility names,
  legacy environment variables, flat metadata readers, or migration commands.
- Keep management commands and dispatch in the Rust core. Keep builtin metadata in
  `src/builtin/metadata/` and detailed help owned by TOML.
- Do not add a root `commands/`, root `scv.toml`, or repository `bin/`. Use inline or
  temporary command fixtures and `cargo run -- <arguments>` for development.
- Treat external command packages as a product contract implemented by validators,
  templates, and isolated fixtures—not as repository-owned commands.
- Never dispatch installed commands directly from `SCV_HOME`. Validate the complete
  source, create an immutable machine-local activation, and switch `current`
  atomically.
- Keep `scv create` optional. Agents may write only to an isolated workspace; SCV
  owns validation and installation. Embedded resources under `assets/generation/`
  must be complete without runtime access to repository docs or agent files.
- Keep one-shot generation on the same isolated boundary. Execute only the
  revalidated, SHA-256-recorded machine-local history copy after explicit consent;
  reruns must revalidate integrity, require new consent, and use the caller's current
  working directory.
- Compose generation prompts from minimal shared contracts and exactly one mode
  contract. Materialize only that mode's metadata and source templates; never expose
  persistent argument, help, interaction, or dry-run scaffolding to one-shot
  generation. Keep a runtime-neutral inline metadata skeleton in each mode;
  templates remain optional references. Instruct straightforward requests to batch
  writes and skip planning, probes, rereads, and validation tool calls. SCV owns
  package and syntax validation; agents finish with the package name only.
- Follow the guide's implementation-choice rules in both modes: prefer available
  system utilities, retain necessary correctness handling, and avoid mandatory
  function scaffolding in one-shot templates. Localize authored messages without
  rebuilding external utilities to translate their output.
- Keep generation transcripts hidden at the adapter process boundary. Use SCV-owned
  localized stage progress and a terminal-only spinner; stop it before the validated
  summary and explicit installation or execution consent. Keep approved command
  output visible and redirected progress free of animation. Read bounded completion
  events for reported token usage and show elapsed generation time before approval;
  follow the guide's timing and accounting rules and never estimate missing usage.
  Distinguish cumulative input from its cache-read subset and count deduplicated
  tool operations, not model requests. Bound temporary identifier storage and show
  unavailable metrics when events or cache details are incomplete.
- Keep credentials with the provider CLI or system Git. Do not weaken sandbox,
  approval, confirmation, non-TTY, or dry-run boundaries through model options.
- Keep the Codex, Claude, and Agy adapters explicit and provider-specific. Reject
  unsupported effort or option translation instead of weakening or approximating
  the provider boundary.
- Keep public behavior native on macOS, Linux, and Windows. Do not introduce Unix-only
  assumptions into Rust core behavior or Windows paths.
- Keep builtin metadata and machine-readable JSON canonical in English. Embedded
  locale catalogs may localize core-owned human views without rewriting external
  command metadata or requiring a runtime download.
- Keep first-run setup equivalent to the explicit `scv init` builtin. It may run
  implicitly only for an argument-free TTY session and must not contact a remote,
  commit, push, or overwrite an existing Git origin. Explicit reconfiguration must
  preserve source commands, change or remove `origin` only when selected, and leave
  source, Git, and preferences unchanged when the interactive selector is cancelled.
- Keep generated-package localization explicit: `--locale` on persistent or
  one-shot generation overrides `ui.locale`, applies only to free-form user-facing
  text, and stores one authored language without translating identifiers or existing
  packages.

## Implement and review

1. Preserve unrelated user changes and reuse existing modules and contracts.
2. Keep metadata, parser behavior, help output, and safety declarations aligned with
   the implementation. Safety metadata describes effects; it never grants authority.
3. Require a fully specified non-interactive path for prompt-capable builtins.
   `--no-input` disables prompting but never implies destructive consent.
4. Ensure a declared `--dry-run` performs no local or remote mutation.
5. Keep generated and synchronized source untrusted until SCV validation and
   activation complete.
6. When a public convention changes, update the guide, relevant architecture docs,
   templates, embedded assets, `AGENTS.md`, README, and CI in the same change.
7. For reviews, prioritize correctness, data safety, authorization, cross-platform
   behavior, and contract drift over style preferences. Do not modify files unless
   the user requested changes.

## Verify proportionally

- Core changes: run Rust formatting, tests, and Clippy; exercise relevant help, JSON,
  valid-input, invalid-input, non-interactive, and dry-run paths.
- Metadata or template changes: use inline or temporary package fixtures and verify
  schema rejection, platform resolution, entry confinement, bounds, and syntax checks.
- Generation changes: test adapter translation, embedded resource completeness,
  workspace confinement and cleanup, validator rejection, and isolated installation.
  Do not make a live paid agent request merely for validation.
- Storage or sync changes: verify source is not executed before apply, invalid source
  preserves `current`, activation tampering is rejected, rollback works, and pull uses
  an isolated validated worktree with a local bare remote.
- Installer or platform changes: keep the macOS/Linux/Windows CI and release matrix
  aligned and test native behavior where the current environment permits it.

Use isolated paths, repositories, fixtures, and mocks for destructive, credentialed,
networked, or externally mutating checks. Respect an explicit user instruction to
skip tests and report any platform, runtime, credential, or service limitation.

## Report

Summarize the product behavior changed, the architectural invariants preserved, the
exact verification performed, and any remaining platform or external dependency.
Link directly to the important changed repository files.
