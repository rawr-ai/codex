#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

run() {
  echo "+ $*"
  if [[ "${DRY_RUN:-0}" != "1" ]]; then
    "$@"
  fi
}

if [[ -n "$(git status --porcelain)" ]]; then
  echo "error: working tree must be clean before sync-upstream" >&2
  exit 1
fi

START_BRANCH="$(git rev-parse --abbrev-ref HEAD)"
PATCH_BRANCH="${1:-${RAWR_PATCH_BRANCH:-$START_BRANCH}}"

if [[ "$PATCH_BRANCH" == "main" ]]; then
  echo "error: patch branch cannot be main" >&2
  exit 1
fi

if ! git rev-parse --verify "$PATCH_BRANCH" >/dev/null 2>&1; then
  echo "error: patch branch '$PATCH_BRANCH' does not exist locally" >&2
  exit 1
fi

echo "Using patch branch: $PATCH_BRANCH"

run git fetch upstream

# Keep origin/main as an upstream mirror.
run git checkout main
run git pull --ff-only upstream main
run git push origin main

# Rebase our patch series.
run git checkout "$PATCH_BRANCH"
run git rebase upstream/main

# Keep the fork semver ahead of upstream so external tooling can feature-detect
# against `codex --version` (e.g. MCP server launch behavior).
run bash rawr/bump-fork-version.sh --commit

run git push --force-with-lease origin "$PATCH_BRANCH"

if [[ "$PATCH_BRANCH" != "$START_BRANCH" ]]; then
  run git checkout "$START_BRANCH"
fi

if [[ "${DRY_RUN:-0}" == "1" ]]; then
  echo "Dry-run complete: upstream sync + patch-branch replay verified."
else
  echo "Synced upstream -> main and rebased '$PATCH_BRANCH'."
fi
