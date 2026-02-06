# rawr Codex fork: upstream updates

The permanent operating model is:
- Graphite operational trunk: `codex/integration-upstream-main`.
- Day-to-day fork work stacks above that trunk (current tracked child: `codex/incremental-rebase-2026-02-06`).
- `main` is not the day-to-day stack base.
- Canonical open fork PR (`rawr-ai/codex`) is `#18` from `codex/incremental-rebase-2026-02-06`.

Upstream sync is performed only at controlled checkpoints against `codex/integration-upstream-main`.

## Checkpoint sync (preferred)
Always pass the integration trunk explicitly:

```bash
rawr/sync-upstream.sh codex/integration-upstream-main
```

Dry-run rehearsal:

```bash
DRY_RUN=1 rawr/sync-upstream.sh codex/integration-upstream-main
```

## After checkpoint sync
Restack descendants of the integration trunk:

```bash
git checkout codex/incremental-rebase-2026-02-06
gt sync --no-restack
gt restack --upstack
```

## Manual checkpoint flow (fallback)

```bash
git fetch --all --prune

git checkout codex/integration-upstream-main
git rebase upstream/main
git push --force-with-lease origin codex/integration-upstream-main

git checkout codex/incremental-rebase-2026-02-06
gt sync --no-restack
gt restack --upstack
```

Notes:
- Do not rely on implicit "current branch" behavior for sync scripts.
- Use `--force-with-lease` (not `--force`) for rewritten history.
- `main` may still be updated as a mirror side effect by legacy tooling, but it is not the stack base.
- For large rebases, run `git range-diff` before push.

## Why this prevents recurrence
- Explicit trunk targeting removes ambiguity about where upstream replay occurs.
- Required restack after checkpoint keeps descendants aligned with the canonical base.
- Keeping `main` out of daily stack parentage blocks accidental rebases onto stale bases.

## Fork versioning

This fork keeps `codex --version` ahead of upstream by computing:

`fork_version = (latest upstream rust-v tag) + 1 minor`

`rawr/sync-upstream.sh` runs `rawr/bump-fork-version.sh --commit` during checkpoint sync so the fork version stays ahead automatically.
