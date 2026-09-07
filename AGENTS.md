# SCV Development Instructions

These instructions apply to the entire repository.

Use the repository skill for SCV product development and review when available. Its
canonical source is `.agents/skills/scv-development`; Claude Code reaches the same
skill through the `.claude/skills/scv-development` symlink. Update only the canonical
source.

Use `.agents/skills/scv-commit` after every completed and verified work unit, even
when the user does not separately ask for a commit. Claude Code reaches it through
`.claude/skills/scv-commit`. This is standing authorization for scoped local commits
only; do not push without a separate explicit request. Skip the commit when the user
explicitly asks not to commit.

When developing SCV:

1. Read `docs/SCRIPT_GUIDE.md` before making changes and treat it as the single
   source of truth.
2. Do not add a user command library, root `scv.toml`, `commands/`, or `bin/` to this
   product repository. Model external command behavior through validators, templates,
   and inline or temporary fixtures.
3. Keep Rust builtin metadata in `src/builtin/metadata/`; do not add SCV metadata
   comments to executable files. Also declare accurate `risk`, `network`, `effects`,
   and `supports_dry_run` safety metadata.
4. Keep detailed help in TOML arguments/options/examples; executable commands must
   validate input but must not duplicate `show_help` or help-option branches.
5. Keep SCV management builtins and dispatch/help behavior in the native Rust core.
6. Use
   `cargo run -- <arguments>` for local development.
7. Verify `cargo run -- --help`, metadata-rendered help, normal execution,
   `scv list --json`, `scv info <command> --json`, metadata, and at least one
   invalid-input path. Verify dry-run without mutations when declared and verify
   platform-specific implementation selection when a package has multiple entries.
8. Keep only one or two core targets positional; model configuration and behavior
   as long options, with short aliases for frequent options. Every interactive
   workflow must have a fully specified non-interactive invocation. Prompt-capable
   commands must support `--no-input`, refuse prompts when stdin is not a TTY, and
   keep destructive confirmation such as `--yes` separate from input control.
   Safety metadata informs decisions but never replaces authorization for mutations.
9. When platform behavior changes, update the guide, relevant architecture documents,
    affected builtin metadata and embedded assets, Skill, and README in the same
    change.
10. Keep `scv create` optional and follow `docs/CREATE_ARCHITECTURE.md`: agents may
    write only to an isolated generation workspace, while SCV validates and installs.
11. Keep deployed `scv create` prompts and generation-only templates under
    `assets/generation/`. SCV must inject the provider-neutral prompt explicitly;
    generated workspaces must not depend on `AGENTS.md` or `CLAUDE.md` discovery.
    Compose a minimal shared contract with exactly one persistent or one-shot mode
    contract, and materialize only that mode's templates into the workspace.
    Follow the guide's implementation-choice rules in both generation modes.
    Keep inline contracts sufficient for simple batched writes; templates are
    optional references and SCV owns package and syntax validation.
    Keep transcripts hidden when collecting provider-reported generation statistics;
    distinguish cumulative tokens, cache reads, and observed tool calls.
12. Keep installed source under `SCV_HOME`, execute only the validated current
    activation, and route managed source changes through the shared apply transaction.
13. Do not add old executable names, environment aliases, flat metadata readers, or
    data migration commands; SCV is a pre-release greenfield application.
14. At the end of each cohesive work unit, stage only its changes, preserve unrelated
    worktree and index changes, run proportional verification, and use the canonical
    `scv-commit` skill. Do not batch independent work units into one catch-all commit.
15. Keep builtin metadata and machine-readable JSON canonical in English. Embedded
    locale catalogs may localize native human output without rewriting external
    command metadata or requiring runtime downloads.
16. Keep first-run setup equivalent to explicit `scv init`: implicit setup is only
    for an argument-free TTY session and never contacts a remote, commits, pushes, or
    overwrites an existing Git origin. Explicit `scv init --reconfigure` preserves
    source commands, keeps omitted settings, and changes or removes `origin` only
    when the user selects that action. Interactive setup cancellation must happen
    before any source, Git, or preference mutation.
17. Keep generated-package localization explicit: `--locale` on persistent or
    one-shot generation overrides `ui.locale`, applies only to free-form user-facing
    text, and stores one authored language without translating identifiers or
    existing packages.
18. Never execute an agent generation workspace. One-shot requests must run only a
    revalidated, SHA-256-recorded machine-local history copy after explicit consent;
    history reruns revalidate, require consent again, and use the caller's current
    working directory.

Do not duplicate the full project rules in agent- or model-specific files. Point
repository-development files back to `docs/SCRIPT_GUIDE.md` instead. Root
`AGENTS.md` and `CLAUDE.md` are development entrypoints, not deployed generation
resources.
