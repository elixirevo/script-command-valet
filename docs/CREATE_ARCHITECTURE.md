# SCV Agent-Powered Generation Architecture

## Product goal and boundary

SCV exposes two optional agent-powered generation modes. `scv create` turns a
natural-language request into a persistent managed command package. `scv
"<request>"` generates a zero-input package for one confirmed execution, stores its
validated copy in machine-local history, and supports later confirmed reruns through
`scv history run <id>`.

The required SCV core continues to work without an agent. `add`, `rm`, `list`,
`info`, help, history replay, and persistent dispatch do not depend on AI or
authentication. Only a new generation request invokes an agent CLI that the user has
already installed and authenticated. SCV does not store agent accounts or
credentials.

## Trust boundary

```text
user request + provider-neutral embedded prompt
    ├── shared generation boundary
    ├── shared command-package contract
    └── exactly one mode contract: persistent or one-shot
            ↓ SCV combines and explicitly injects
       provider adapter
            ↓ uses workspace as working directory
isolated temporary workspace
    ├── generation-only templates
    └── generated/<command>/
            ↓
SCV package validator
    ├── metadata and safety schema
    ├── package/name/entry boundaries
    ├── symlink and special-file rejection
    ├── file count/size/depth limits
    └── available runtime syntax checks
            ↓ preview and explicit consent
       ┌────┴──────────────────────────────┐
       │ persistent create                │ one-shot request
       ↓                                  ↓
shared atomic package installer     staged machine-local history copy
       ↓                                  ↓ revalidation + SHA-256 manifest
~/.scv/commands/<command>/ source   <data>/history/<id>/package/<command>/
       ↓ full library validation           ↓
immutable machine-local activation  execute in caller's current directory
       ↓ atomic current switch             ↓
scv <command>                       scv history run <id>
```

SCV sets the temporary workspace as the adapter's working directory. The Codex
adapter uses the `workspace-write` sandbox and disables persistence and user
customization. The Claude adapter uses `--safe-mode` with a restricted file-tool set.
The Agy adapter uses `--sandbox`, `accept-edits` mode, and disables slash-command and
skill expansion. None of the adapters receives the user's SCV command-library path,
and SCV consumes exactly one package directly under `generated/` as the result.

SCV does not trust or execute generated workspace output; the SCV validator is the
authority. Persistent create revalidates a staging copy before updating source, then
copies and revalidates the complete library into a new activation. One-shot mode
requires no package arguments or options, requires the current platform, copies the
package into history, revalidates and hashes the copy, and executes only that stored
copy after approval. History reruns repeat package and digest validation and require
new approval.

## Rust module boundaries

- `src/agent/`: common requests and provider adapters. Codex, Claude, and Agy are
  supported.
- `src/agent/usage.rs`: reads bounded provider events and normalizes reported token
  totals while discarding all transcript text.
- `src/generation.rs`: combines shared provider-neutral contracts with exactly one
  mode contract from `assets/generation/`, materializes only that mode's templates
  under stable workspace names, and removes the complete workspace on exit.
- `src/package.rs`: generated-package validation and the atomic installer shared by
  `add` and `create`.
- `src/activation.rs`: complete source validation, immutable activation, digests,
  and the `current` switch.
- `src/config.rs`: storage, resolution, and precedence for create and agent settings.
- `src/oneshot.rs`: top-level natural-language detection, generation, preview,
  consent, history commit, and immediate dispatch.
- `src/history.rs`: machine-local one-shot manifests, package copies, digest checks,
  listing, and retention.
- `src/builtin/create.rs`: coordinates only the user flow and does not own provider
  or storage details.
- `src/progress.rs`: owns generation stage progress on stderr and the terminal-only
  spinner, stopping and joining the animation before previews, prompts, or errors.
- `src/builtin/history.rs`: lists history and coordinates confirmed reruns.
- `docs/SCRIPT_GUIDE.md`: canonical repository-development and platform-behavior
  guide. It is not a runtime resource for the installed binary.
- `assets/generation/prompts/`: `instructions.md` and `command-package.md` are shared;
  `persistent.md` and `one-shot.md` define mutually exclusive mode behavior. All are
  embedded and do not refer to external `AGENTS.md`, `CLAUDE.md`, Skill, or `docs/`.
- `assets/generation/templates/`: persistent `command.*.tmpl` and one-shot
  `one-shot.*.tmpl` starting points are embedded. Materialization exposes only the
  selected set as `templates/command.toml`, `command.sh`, `command.js`, `command.py`,
  and `command.ps1` so an adapter cannot select the wrong mode template. They are
  optional references; mode prompts also contain an inline metadata skeleton.

Root `AGENTS.md` and `CLAUDE.md` are entrypoints for developers who run an agent from
the product repository. They are not copied into a temporary generation workspace.
SCV supplies the prompt directly, so provider-specific file discovery is not part of
the generation contract.

## Generation progress and approval

Both modes show four SCV-owned stages: preparing the request, generating the
command, validating the package, and readiness for installation or execution
approval. Human progress follows `ui.locale`, independently of the package's
`--locale`. A capable stderr terminal gets a rotating indicator during work; pipes,
files, and `TERM=dumb` get plain stage lines without animation or control sequences.
The animation is stopped on success and on errors before any preview or prompt.

All three adapters use the shared quiet process runner. Provider stderr is discarded;
a concurrent reader drains structured stdout before stdin delivery can block. It
buffers at most one 1 MiB event, skips oversized lines through their newline, and
retains numeric usage, completion status, and bounded temporary tool identifiers
for deduplication. Prompts, source code, tool inputs/results, and final responses never appear in the terminal or get persisted by SCV. Failures
report the provider and exit status without replaying its transcript; recognized
failed completion events also prevent approval even if the process exits zero.
SCV still sends Codex and Claude their prompt over stdin and closes the pipe after
writing it; Agy receives its prompt as an argument with null stdin.

The final preview retains the description and declared risk, network use, effects,
implementations, and validation checks. Consent remains explicit: `create` asks to
install, while one-shot mode briefly reminds the user to review the command and asks
to execute. History retention and validation limits are explained in `scv history
--help`. After saving, report how many old entries were removed only when cleanup
actually removed entries; report cleanup failures separately. No spinner runs
during consent or execution. The executed command's own stdout and stderr remain
visible after approval. `--yes` and `--no-input` keep their existing semantics.

### Completion statistics

Before approval, both modes show one localized stderr summary with elapsed seconds
and reported cumulative total, input, and output tokens. A second line shows cache
reads, non-cached input, and distinct observed tool calls when available. The
monotonic timer starts before workspace preparation and stops after validation, excluding the user's approval
wait, installation or history storage, and execution. Statistics are a view of this
generation request; they do not change history manifests or machine-readable JSON.

Adapters request these documented event formats:

- [Codex JSONL](https://learn.chatgpt.com/docs/non-interactive-mode): `--json`,
  using `turn.completed.usage`. Sum completed turns, not individual tool events.
  Input includes cached tokens and output includes reasoning tokens. Extract
  `cached_input_tokens` as a subset, not an additional input amount.
- [Claude streaming JSON](https://code.claude.com/docs/en/headless):
  `--output-format stream-json --verbose`, using the root `result` event. Prefer
  summed `modelUsage` when provided, otherwise use `usage`. Include cache-read and
  cache-creation counts in input; do not add intermediate assistant messages or
  cumulative result snapshots. Cache reads are `cacheReadInputTokens` or
  `cache_read_input_tokens`; non-cached input includes cache creation. See [Claude usage accounting](https://code.claude.com/docs/en/agent-sdk/cost-tracking).
- [Agy streaming JSON](https://www.antigravity.google/docs/cli/headless/):
  `--output-format stream-json`, using `result.usage`. Do not add step updates or
  thinking/cache counts to the final totals. Cache inclusion is not consistently
  defined by the provider, so the cache/non-cached split is unavailable.

Cache breakdown requires a valid reported cache-read count for every aggregated
turn/model, no greater than its normalized input. Missing or invalid cache data
leaves valid token totals intact. An unfinished Codex turn invalidates earlier
partial totals. These cumulative counts include reused context across model calls;
they are not the size of the initial SCV prompt.

Count distinct Codex `item.completed` operations for command execution, file change,
MCP calls, and web search by `item.id`. Exclude messages, reasoning, plans, and
started/updated snapshots. Count Claude assistant `tool_use` blocks by block `id`
(not the shared assistant message id). Count Agy `step_update` tools in `DONE`
state by `(conversation_id, step_index)`. Completed failed tools also count as
attempted operations; a shell operation may contain several shell commands. This
is an observed tool count, not a model request count.

Keep at most 4,096 distinct tool identifiers, each component at most 128 bytes,
only in memory. Malformed/oversized events, unknown tool event shapes, invalid
identifiers, or exceeding that bound make the tool count unavailable instead of displaying an undercount. Require
a provider completion before showing zero or a count. Later intact completion
usage may still supply token totals after a discarded tool event. SCV does not
persist event text or tool identifiers.

Missing, malformed, oversized, incomplete, or overflowing usage is shown as
unavailable, not zero and not an estimate derived from code length. Valid zero usage
is displayed as zero. These are CLI-reported token counts, not billing or account
quota figures. Optional usage does not bypass package validation or explicit consent.

## Implementation choice

Both modes share a system-tool-first authoring policy. For operations already
provided by available utilities, generate a short command or pipeline with necessary
quoting, empty-input handling, and error propagation. When utilities meet the request,
reuse their traversal, disk-usage measurement, size formatting, and sorting.
Python and Node.js remain appropriate when they simplify structured data processing
or complex logic. The actual request determines file selection, size semantics, and
ordering; short code must still be correct.

Self-contained packages may use standard OS utilities and their declared runtime.
They must bundle authored resources, use compatible utilities and flags, and cannot
assume optional dependencies. The shared metadata contract lists required fields
without a runtime-specific sample. One-shot templates omit mandatory `main`
wrappers, and their mode prompt includes a short native-tool example. One-shot
generation targets the current platform without speculative extra implementations.

The shared boundary asks straightforward requests to write metadata and source
in one tool call when supported, without planning, filesystem exploration, utility
probes, or rereading just-written files. Each mode includes a runtime-neutral TOML
skeleton, so templates need not be read unless relevant details are needed. Agents
finish with the package name only and do not run tests or duplicate SCV's package
and syntax checks. SCV's validator and installation/history checks remain unchanged.

The embedded shared-plus-mode contracts are about 5.6 KB for one-shot and 6.1 KB for
persistent generation, excluding the user request and reserved names. A fixture
prompt-size budget guards against accidental expansion. This measures SCV-authored
bytes, not model tokens or the provider's own system/tool context. No fixed latency
or number of model calls is guaranteed by these authoring instructions.

These are authoring instructions, not a code-length validator or a change to model
selection. Evaluate generated results by correctness, utility choice, unnecessary
custom logic, and elapsed generation time. Useful cases include directory disk usage
(native utilities), empty directories and unusual path names (correct handling), and
JSON transformation (a real parser rather than fragile shell text substitution).
Fixture tests verify the embedded example through validation, history storage, and
approved execution; they do not measure a live model's adherence or latency.

## Generated package language

`scv create --locale <en|ko>` and the equivalent one-shot option select the single
authored language for free-form package metadata and runtime messages. Without the
option, generation uses the machine-local `ui.locale` preference. SCV injects this
policy before the escaped untrusted request, so text in the requested behavior
cannot replace it.

Descriptions, effects, argument and option descriptions, defaults, notes, and
package-owned user messages use the selected language. Schema keys and enum values,
identifiers, command/category/argument/option names, usage and example syntax, file
names, runtime/platform/risk tokens, and required protocol literals stay canonical.
The package does not contain parallel translations, and changing `ui.locale` later
does not rewrite or dynamically translate an installed user command.

External utility output and diagnostics retain their native language. Generated
code must not reimplement or wrap a utility solely to translate it; only
package-authored messages follow the selected locale, and utility failures remain
visible.

## Configuration and adapter contract

Configuration precedence is:

```text
generation CLI option
    ↓
[agents.<selected-agent>]
    ↓
[create]
    ↓
agent CLI default
```

Generated output language has its own simpler precedence:

```text
generation --locale
    ↓
ui.locale
    ↓
en
```

`model` is an opaque string whose values SCV does not enumerate. `effort` accepts
only `low`, `medium`, `high`, `xhigh`, and `max`; Agy supports only the first three
and rejects the other levels before starting. Codex provider-specific settings use
`--agent-option key=value`; SCV rejects keys that would change its selected model or
sandbox, approval, and network boundaries. SCV does not invent mappings for portable
effort or generic options when a provider does not support them.

The shared prompt owns workspace confinement, implementation choice, package shape,
runtimes, localization, safety metadata, and the concise writing workflow. SCV
alone performs package and non-executing syntax validation after generation. The
persistent contract alone owns arguments, options, metadata-rendered help,
interactive automation, and dry-run rules. The one-shot contract instead requires
exact zero-input usage, no arguments or options, no prompts, current-platform support,
and current-working-directory path resolution. SCV injects the concrete current
platform into that mode context. The one-shot validator enforces exact usage and
examples in addition to the structural package checks.

The Codex adapter disables user configuration and execpolicy rules, sets the project
instruction byte limit to zero, and sends the composed prompt over stdin. The Claude
adapter uses `--safe-mode` to disable customization, including `CLAUDE.md`, and sends
the same composed prompt over stdin. Provider adapters do not own or duplicate the
generation contract in provider-specific files.

The Agy CLI accepts its print-mode prompt as an argument. SCV supplies the complete
composed prompt there, uses a bounded print timeout, and does not use
`--dangerously-skip-permissions`. Agy owns its installed authentication and other
provider state; SCV does not copy that state into the generation workspace.

## Validation limits

The validator runs only non-executing syntax checks available through installed
runtimes. It reports a check as `skipped` when its runtime is unavailable, while
structure and metadata validation always run. Do not treat SCV as having verified the
executability of a binary for another OS or an unavailable runtime.
