---
name: scv-commit
description: Create a scoped local Git commit after each completed and verified work unit in the SCV product repository, even without a separate commit request. Do not use for read-only work, incomplete changes, pushes, or unrelated changes.
---

# SCV Commit

Create a reviewable local commit after each cohesive SCV work unit is complete and
verified. Repository `AGENTS.md` provides standing authorization for these local
commits, so a separate commit request is not required. An explicit user instruction
not to commit overrides this default. The standing authorization does not include a
push, tag, release, amend, or history rewrite.

Do not create a commit for read-only answers, reviews, or diagnoses that made no
files change. Do not commit incomplete work, known failing changes, or a blocked task
as if it were complete.

## Establish the commit scope

1. Resolve the repository root and read its `AGENTS.md`.
2. Treat the active user request and the smallest cohesive work unit completed for it
   as the scope authority. Commit independent completed units separately instead of
   batching them into an unrelated catch-all commit.
3. Inspect `git status --short`, unstaged and staged diffs, and recent commit subjects
   before changing the index.
4. Preserve pre-existing or unrelated changes as user-owned. Stage explicit paths
   with `git add -- <paths>`; do not use `git add .` or `git add -A`.
5. If a file mixes in-scope and unrelated hunks, or the index already contains
   unrelated staged changes, do not disturb the index with reset, checkout, restore,
   or stash. Commit only when the intended scope can be separated safely; otherwise
   report the ambiguity and ask for direction.

## Validate the result

- Review the complete in-scope diff and exclude credentials, build output, caches,
  temporary fixtures, and unrelated generated files.
- Reuse verification already completed for the active task. Run any missing checks
  required by `docs/SCRIPT_GUIDE.md` in proportion to the change; do not repeat paid,
  credentialed, network-mutating, or unavailable-platform checks merely to commit.
- Run `git diff --check` before staging and `git diff --cached --check` afterward.
- Re-read `git diff --cached` and confirm every staged hunk belongs to the requested
  work. Do not create an empty commit.

## Write the commit message

- Inspect recent subjects with `git log` and follow the repository's established
  language and style without inventing a new convention from a single example.
- Use a concise, outcome-focused, imperative English subject. Mention the behavior or
  invariant changed rather than listing filenames or referring to an agent.
- Add a body only when it materially explains motivation, safety boundaries, a
  migration, or verification that is not evident from the subject.

## Commit and report

1. Create the commit as soon as the scoped unit is complete and verified. Never
   bypass hooks with `--no-verify` and never amend unless the user explicitly
   requests it.
2. If a hook fails, fix only in-scope issues, rerun the relevant validation, and
   retry. Leave unrelated failures visible rather than expanding the task silently.
3. Inspect the new commit and remaining worktree with `git show --stat --oneline HEAD`
   and `git status --short`.
4. Report the commit hash and subject, checks performed, and any remaining uncommitted
   changes. Never push, tag, publish a release, or change Git configuration or remotes
   unless the user separately requests that action.
