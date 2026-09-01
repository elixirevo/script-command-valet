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
    ├── complete generation boundary
    └── complete command-package contract
            ↓ create builtin combines and explicitly injects
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
- `src/generation.rs`: combines the provider-neutral prompt from
  `assets/generation/`, materializes generation-only templates into a temporary
  workspace, and removes the complete workspace on exit.
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
- `src/builtin/history.rs`: lists history and coordinates confirmed reruns.
- `docs/SCRIPT_GUIDE.md`: canonical repository-development and platform-behavior
  guide. It is not a runtime resource for the installed binary.
- `assets/generation/prompts/`: complete provider-neutral generation contracts
  embedded in the installed binary. They do not refer to external `AGENTS.md`,
  `CLAUDE.md`, Skill, or `docs/` paths.
- `assets/generation/templates/`: generation-only source and metadata starting points
  embedded in the installed binary. The repository stores them as
  `command.toml.tmpl`, `command.sh.tmpl`, `command.js.tmpl`, `command.py.tmpl`, and
  `command.ps1.tmpl`; materialization removes `.tmpl` in the temporary workspace.

Root `AGENTS.md` and `CLAUDE.md` are entrypoints for developers who run an agent from
the product repository. They are not copied into a temporary generation workspace.
SCV supplies the prompt directly, so provider-specific file discovery is not part of
the generation contract.

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

The Codex adapter disables user configuration and execpolicy rules, sets the project
instruction byte limit to zero, and sends the common prompt over stdin. The Claude
adapter uses `--safe-mode` to disable customization, including `CLAUDE.md`, and sends
the same common prompt over stdin. Provider adapters do not own or duplicate the
generation contract in provider-specific files.

The Agy CLI accepts its print-mode prompt as an argument. SCV supplies the complete
common prompt there, uses a bounded print timeout, and does not use
`--dangerously-skip-permissions`. Agy owns its installed authentication and other
provider state; SCV does not copy that state into the generation workspace.

## Validation limits

The validator runs only non-executing syntax checks available through installed
runtimes. It reports a check as `skipped` when its runtime is unavailable, while
structure and metadata validation always run. Do not treat SCV as having verified the
executability of a binary for another OS or an unavailable runtime.
