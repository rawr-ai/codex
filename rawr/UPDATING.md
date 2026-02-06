# rawr Codex fork: upstream updates

The intent is to keep `origin/main` tracking upstream (no fork changes) and keep fork-specific changes on an explicit patch branch as a small, rebased patch series.

Patch branch options:
- long-lived branch (for example `rawr/main`), or
- cycle branch (for example `codex/rebase-upstream-YYYY-MM-DD`).

The runbook and script require the patch branch to be explicit and never equal to `main`.

## One-liner (preferred)
Run:
```bash
rawr/sync-upstream.sh
```

To target a non-current patch branch:
```bash
rawr/sync-upstream.sh codex/rebase-upstream-2026-02-05
```

Dry-run rehearsal:
```bash
DRY_RUN=1 rawr/sync-upstream.sh codex/rebase-upstream-2026-02-05
```

## Manual steps
```bash
git fetch upstream
git checkout main
git pull --ff-only upstream main
git push origin main

git checkout <patch-branch>
git rebase upstream/main
git push --force-with-lease origin <patch-branch>
```

Notes:
- Use `--force-with-lease` (not `--force`) so you don’t accidentally overwrite someone else’s work.
- If rebases get painful, the fork delta is too big: split features into smaller commits and/or remove experimental changes.
- Keep branch selection explicit and require a clean tree before syncing.
- For large rebases, run `git range-diff` before push.

## Fork versioning

This fork keeps `codex --version` ahead of upstream by computing:

`fork_version = (latest upstream rust-v tag) + 1 minor`

`rawr/sync-upstream.sh` runs `rawr/bump-fork-version.sh --commit` after rebasing so the version stays ahead automatically.
