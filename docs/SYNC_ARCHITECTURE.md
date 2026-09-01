# SCV Git Sync Architecture

## Trust boundary

Git transport integrity does not authorize execution. `scv sync pull` treats the
remote revision as untrusted source until SCV has validated the complete library and
the user has approved the preview.

SCV delegates authentication, credential storage, remotes, branches, commits, and
conflict resolution to system Git or `gh`. SCV never stores GitHub tokens.

## Setup

`scv init` initializes the local source repository and may save a user-supplied URL
or path as `origin`. This is a local configuration operation: it does not access the
remote, create a hosted repository, commit, or push. Credential-bearing HTTP(S) URLs
are rejected, and initial setup preserves an existing conflicting `origin`.
`scv init --reconfigure` preserves the command source while allowing the user to
replace or remove `origin` explicitly and repairing missing local Git metadata.

## Status

`scv sync status` is local and reports:

- repository path, branch, and HEAD;
- configured upstream;
- dirty state; and
- ahead/behind counts against the locally known upstream ref.

It does not fetch.

## Pull

```text
require clean source worktree
        ↓
git fetch --prune origin
        ↓
require origin/<branch> and fast-forward ancestry
        ↓
detached temporary Git worktree in SCV cache
        ↓
validate scv.toml and every package
        ↓
show commit IDs and diff stat
        ↓ explicit approval
git merge --ff-only
        ↓
full SCV activation transaction
```

The temporary worktree is removed after validation, cancellation, or merge. Pull
does not support `--dry-run` because fetch changes local Git refs. It never resolves
divergence, overwrites dirty source, creates commits, rebases, or executes code from
the temporary worktree.

If source fast-forward succeeds but activation fails, Git source remains at the new
revision while the previous activation remains executable. `scv status` exposes the
pending source/active difference.

## Push

`scv sync push` requires a clean source and pushes `HEAD` to `origin`. It never
creates a commit. `scv sync push --dry-run` delegates to `git push --dry-run`.

Pull and push require TTY confirmation unless `--yes` is supplied. `--no-input`
forbids prompting and never implies approval.

## Conflicts and recovery

- Dirty working tree: commit or discard changes with Git before sync.
- Diverged branch: resolve with Git; SCV accepts only a fast-forward pull.
- Invalid remote package: fix/revert it remotely; current activation is unchanged.
- Activation failure after merge: inspect with `scv status`, fix source, then run
  `scv apply`.
- Bad but valid activation: use `scv rollback`.
