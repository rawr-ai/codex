#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

log() {
  echo "rawr/sync-upstream: $*"
}

run() {
  echo "+ $*"
  "$@"
}

die() {
  echo "error: $*" >&2
  exit 1
}

in_rebase() {
  local apply_dir merge_dir
  apply_dir="$(git rev-parse --git-path rebase-apply)"
  merge_dir="$(git rev-parse --git-path rebase-merge)"
  [[ -d "$apply_dir" || -d "$merge_dir" ]]
}

if [[ -n "$(git status --porcelain)" ]]; then
  die "working tree must be clean before sync-upstream"
fi

START_BRANCH="$(git rev-parse --abbrev-ref HEAD)"
PATCH_BRANCH="${1:-${RAWR_PATCH_BRANCH:-$START_BRANCH}}"

if [[ "$PATCH_BRANCH" == "main" ]]; then
  die "patch branch cannot be main"
fi

if ! git rev-parse --verify "$PATCH_BRANCH" >/dev/null 2>&1; then
  die "patch branch '$PATCH_BRANCH' does not exist locally"
fi

echo "Using patch branch: $PATCH_BRANCH"

LOCK_DIR="$ROOT/.scratch/locks/sync-upstream.lock"
lock_acquired=0
cleanup_lock() {
  if [[ "$lock_acquired" == "1" && -d "$LOCK_DIR" ]]; then
    rm -rf "$LOCK_DIR"
  fi
}

cleanup_restore() {
  local code=$?

  if in_rebase; then
    if [[ "${RAWR_LEAVE_REBASE_IN_PROGRESS:-0}" == "1" ]]; then
      log "leaving in-progress rebase state (RAWR_LEAVE_REBASE_IN_PROGRESS=1)"
    else
      log "aborting in-progress rebase to keep workspace clean"
      git rebase --abort || true
    fi
  fi

  if [[ "$(git rev-parse --abbrev-ref HEAD)" != "$START_BRANCH" ]]; then
    git checkout "$START_BRANCH" >/dev/null 2>&1 || true
  fi

  cleanup_lock
  exit "$code"
}

trap cleanup_restore EXIT INT TERM

mkdir -p "$ROOT/.scratch/locks"
if mkdir "$LOCK_DIR" 2>/dev/null; then
  lock_acquired=1
  printf '{"pid":%s,"startedAt":%s,"startBranch":"%s","patchBranch":"%s"}\n' \
    "$$" \
    "$(date +%s)" \
    "$START_BRANCH" \
    "$PATCH_BRANCH" \
    >"$LOCK_DIR/meta.json" 2>/dev/null || true
else
  die "lock is held ($LOCK_DIR); another sync-upstream may be running"
fi

run git fetch upstream --prune
run git fetch origin --prune

# Keep origin/main as an upstream mirror.
verify_main_ff_only() {
  local origin_main upstream_main merge_base
  origin_main="$(git rev-parse origin/main)"
  upstream_main="$(git rev-parse upstream/main)"
  merge_base="$(git merge-base origin/main upstream/main)"

  if [[ "$merge_base" != "$origin_main" ]]; then
    die "origin/main is not fast-forwardable to upstream/main (ff-only would fail)"
  fi

  log "origin/main can fast-forward to upstream/main"
}

verify_patch_rebase_clean() {
  local tmpdir
  tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/rawr-sync-upstream-verify.XXXXXX")"

  cleanup_verify() {
    local code=$?

    if [[ -d "$tmpdir" ]]; then
      if [[ -d "$(git -C "$tmpdir" rev-parse --git-path rebase-apply 2>/dev/null)" || -d "$(git -C "$tmpdir" rev-parse --git-path rebase-merge 2>/dev/null)" ]]; then
        git -C "$tmpdir" rebase --abort >/dev/null 2>&1 || true
      fi
      git worktree remove -f "$tmpdir" >/dev/null 2>&1 || true
      rm -rf "$tmpdir" >/dev/null 2>&1 || true
    fi
    return "$code"
  }
  trap cleanup_verify RETURN

  if ! git worktree add --detach "$tmpdir" "$PATCH_BRANCH"; then
    echo "Failed to create temporary verification worktree." >&2
    return 1
  fi

  if ! git -C "$tmpdir" fetch upstream --prune; then
    echo "Failed to fetch upstream in temporary verification worktree." >&2
    return 1
  fi

  if git -C "$tmpdir" rebase upstream/main; then
    log "patch branch rebase verified cleanly in temporary worktree"
    return 0
  fi

  echo "Rebase verification failed (conflicts) while replaying '$PATCH_BRANCH' onto upstream/main." >&2
  git -C "$tmpdir" rebase --show-current-patch >&2 || true
  git -C "$tmpdir" diff --name-only --diff-filter=U >&2 || true

  if [[ "${RAWR_LEAVE_REBASE_IN_PROGRESS:-0}" == "1" ]]; then
    echo "Note: RAWR_LEAVE_REBASE_IN_PROGRESS=1 is set, but verify mode always aborts and cleans up." >&2
  fi

  return 1
}

apply_sync() {
  local expect_remote=""

  run git checkout main
  run git pull --ff-only upstream main
  run git push origin main

  run git checkout "$PATCH_BRANCH"

  expect_remote="$(git ls-remote --heads origin "$PATCH_BRANCH" | awk '{print $1}')"
  if [[ -z "$expect_remote" ]]; then
    die "failed to resolve expected remote SHA for '$PATCH_BRANCH' on origin"
  fi

  run git rebase upstream/main

  # Keep the fork semver ahead of upstream so external tooling can feature-detect
  # against `codex --version` (e.g. MCP server launch behavior).
  run bash rawr/bump-fork-version.sh --commit

  run git push --force-with-lease="refs/heads/$PATCH_BRANCH:$expect_remote" origin "HEAD:refs/heads/$PATCH_BRANCH"
}

if [[ "${DRY_RUN:-0}" == "1" ]]; then
  verify_main_ff_only
  verify_patch_rebase_clean
  echo "Dry-run complete: upstream sync + patch-branch replay verified."
  exit 0
fi

apply_sync
echo "Synced upstream -> main and rebased '$PATCH_BRANCH'."
