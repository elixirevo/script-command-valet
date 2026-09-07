# SCV one-shot mode

Create a zero-input package. SCV validates it, obtains explicit user consent, and
copies it to machine-local history. SCV revalidates/hashes and executes only that
stored copy, never the generation workspace. Start with this metadata shape:

```toml
name = "__COMMAND__"
category = "one-shot"
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

- Set usage and every optional example to exactly `scv <generated-name>`.
- Declare no `[[arguments]]` and no `[[options]]`; `supports_dry_run` stays false.
- Complete the request without command-line input or prompts. Do not add help,
  configuration, placeholder inputs, or approval flags; SCV owns consent.
- Target the supplied current platform; add others only when requested/supported.
- Relative paths use the process current working directory. Never depend on the
  workspace path, installation in source, or activation; resources travel in history.

For Linux/macOS disk usage of non-hidden subdirectories in ascending size order:

```bash
#!/usr/bin/env bash
set -euo pipefail
shopt -s nullglob

folders=(./*/)
((${#folders[@]})) || exit 0
du -sh "${folders[@]}" | sort -h
```

This handles spaces, leading hyphens, and no matches. Adapt to the actual request
and platform; do not build a custom recursive walker or size formatter for this task.
