# `scv create` Architecture

## Product goal and boundary

`scv create` does not translate natural language into a shell string for one-time
execution. It is an optional generation layer that turns a natural-language request
into a persistent command package with SCV metadata, implementations,
platform/runtime contracts, help, safety information, and a managed lifecycle.

The required SCV core continues to work without an agent. `add`, `rm`, `list`,
`info`, help, and dispatch do not depend on AI or authentication. Only `create`
invokes an agent CLI that the user has already installed and authenticated. SCV does
not store agent accounts or credentials.

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
            ↓
preview and explicit install consent
            ↓
shared atomic package installer
            ↓
~/.scv/commands/<command>/ source
            ↓ full library validation
immutable machine-local activation
            ↓ atomic current switch
scv <command>
```

SCV sets the temporary workspace as the adapter's working directory. The Codex
adapter uses the `workspace-write` sandbox and disables persistence and user
customization. The Claude adapter uses `--safe-mode` with a restricted file-tool set.
The Agy adapter uses `--sandbox`, `accept-edits` mode, and disables slash-command and
skill expansion. None of the adapters receives the user's SCV command-library path,
and SCV consumes exactly one package directly under `generated/` as the result.

SCV does not trust or install generated output immediately; the SCV validator is the
authority. SCV revalidates a staging copy immediately before updating source. After
source changes, it copies and revalidates the complete library into a new activation.
The previous execution state remains active until the atomic `current` switch.

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
- `src/builtin/create.rs`: coordinates only the user flow and does not own provider
  or storage details.
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
the product repository. They are not copied into the temporary workspace created by
`scv create`. SCV supplies the prompt directly, so provider-specific file discovery
is not part of the generation contract.

## Generated package language

`scv create --locale <en|ko>` selects the single authored language for free-form
package metadata and runtime messages. Without the option, create uses the
machine-local `ui.locale` preference. SCV injects this policy before the escaped
untrusted request, so text in the requested behavior cannot replace it.

Descriptions, effects, argument and option descriptions, defaults, notes, and
package-owned user messages use the selected language. Schema keys and enum values,
identifiers, command/category/argument/option names, usage and example syntax, file
names, runtime/platform/risk tokens, and required protocol literals stay canonical.
The package does not contain parallel translations, and changing `ui.locale` later
does not rewrite or dynamically translate an installed user command.

## Configuration and adapter contract

Configuration precedence is:

```text
create CLI option
    ↓
[agents.<selected-agent>]
    ↓
[create]
    ↓
agent CLI default
```

Generated output language has its own simpler precedence:

```text
create --locale
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
