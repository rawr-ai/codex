# rawr Codex fork: upstream updates

The intent is to keep `origin/main` tracking upstream (no fork changes) and keep all rawr-specific changes on `origin/rawr/main` as a small, rebased patch series.

## One-liner (preferred)
Run:
```bash
codex/rawr/sync-upstream.sh
```

## Manual steps
```bash
git fetch upstream
git checkout main
git pull --ff-only upstream main
git push origin main

git checkout rawr/main
git rebase upstream/main
git push --force-with-lease origin rawr/main
```

Notes:
- Use `--force-with-lease` (not `--force`) so you don’t accidentally overwrite someone else’s work.
- If rebases get painful, the fork delta is too big: split features into smaller commits and/or remove experimental changes.
