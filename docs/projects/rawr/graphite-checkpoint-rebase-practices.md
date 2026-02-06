# Graphite Practices Memo: Checkpoint Rebases on `codex/integration-upstream-main`

Date: 2026-02-06

This memo captures Graphite-specific best practices for the fork’s chosen approach:

- The operational trunk is `codex/integration-upstream-main`.
- We perform controlled upstream checkpoint rebases on the trunk with Git (via `rawr/sync-upstream.sh`).
- We use Graphite for stack topology and descendant restacks (`gt restack --upstack`), and for PR submission patterns.

## Bottom line

The current approach is coherent and aligns with best practice for a fork maintained as a replayable patch queue: Git performs the controlled trunk history rewrite, then Graphite re-establishes stack topology and restacks descendants.

The two key risks to guard harder are:
- `gt sync` clobbering an in-progress trunk rewrite.
- Branches becoming “untracked” after a vanilla `git rebase`.

## Targeted recommendations (no scope creep)

### 1) Never run `gt sync` during a checkpoint rewrite

Why:
- `gt sync` can overwrite local trunk with remote trunk if trunk cannot be fast-forwarded, which can discard an in-progress local rebase.

When:
- Any time a checkpoint rewrite has started (e.g., `git rebase upstream/main` is running or conflicts are being resolved), and the trunk has not been successfully pushed.

Command guidance:
- During checkpoint work, prefer plain fetch/inspect:
  - `git fetch origin --prune`
  - `git fetch upstream --prune`
- Only after the checkpoint push succeeds should you run:
  - `gt sync --no-restack --no-interactive`
  - `gt restack --upstack` (if descendants exist)

Reference:
- https://graphite.com/docs/sync

### 2) Explicitly check tracking after checkpoint rebases, and repair with `gt track` if needed

Why:
- A plain `git rebase` can cause Graphite branches to become untracked (base commit disappears), breaking stack operations.

When:
- Immediately after the checkpoint rebase completes and trunk is pushed, before restacking and submission.

Commands:
- Verify:
  - `gt ls --all --show-untracked`
- Repair (example pattern):
  - `gt track <branch> --parent codex/integration-upstream-main`

References:
- https://graphite.com/docs/tracking-branches

### 3) Prefer stack-scoped restacks over repo-wide sync in multi-agent workflows

Why:
- Even `gt sync --no-restack` is repo-wide and may prompt for deletions; it’s higher blast radius than a targeted restack.

When:
- Parallel worktrees or multiple agents are operating.

Commands:
- Prefer:
  - `git fetch origin --prune`
  - `gt restack --upstack`
- Reserve:
  - `gt sync --no-restack --no-interactive` for orchestrator moments immediately after a checkpoint push.

Reference:
- https://graphite.com/docs/sync

### 4) PR submission patterns to standardize (when used)

Why:
- `gt submit` is stack-aware and idempotent; it’s safer than ad-hoc push/PR drift for tracked stacks.

Commands (choose the minimal one that fits intent):
- Submit a stack as draft:
  - `gt submit --stack --draft`
- Update existing PRs only:
  - `gt submit --stack --update-only`
- If base targeting is ambiguous:
  - `gt submit --stack --target-trunk codex/integration-upstream-main`

Reference:
- https://graphite.com/docs/command-reference

### 5) Make rollback anchors concrete

Why:
- Naming and pushing a rollback anchor reduces reliance on reflog and makes recovery deterministic.

When:
- Right before rewriting the trunk at a checkpoint.

Commands:
- `git branch codex/rollback-checkpoint-YYYY-MM-DD HEAD`
- `git push origin codex/rollback-checkpoint-YYYY-MM-DD`

