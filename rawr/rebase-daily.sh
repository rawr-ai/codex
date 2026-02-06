#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

log() {
  echo "rawr/rebase-daily: $*"
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

RUN_DATE="$(date +%F)"
RUN_DIR="$ROOT/.scratch/rebase-daily/$RUN_DATE"
REPORT_JSON="$RUN_DIR/report.json"
SUMMARY_MD="$RUN_DIR/summary.md"

mkdir -p "$RUN_DIR"

PATCH_BRANCH="${1:-codex/integration-upstream-main}"
if [[ "$PATCH_BRANCH" != "codex/integration-upstream-main" ]]; then
  die "this daily orchestrator only supports codex/integration-upstream-main (got '$PATCH_BRANCH')"
fi

EXIT_OK=0
EXIT_VERIFY_CONFLICT=10
EXIT_APPLY_CONFLICT=11
EXIT_LEASE_OR_PUSH_BLOCKED=12
EXIT_GRAPHITE_RESTACK_FAILED=13
EXIT_TESTS_FAILED=14
EXIT_LOCK_HELD=15

LOCK_DIR="$ROOT/.scratch/locks/rebase-daily.lock"
mkdir -p "$ROOT/.scratch/locks"

lock_acquired=0
start_branch="$(git rev-parse --abbrev-ref HEAD)"
validate_tmpdir=""

export RAWR_REPORT_JSON_PATH="$REPORT_JSON"

write_report() {
  # Keep report writing best-effort; it should never hide the real failure.
  python3 - <<'PY' 2>/dev/null || true
import json, os, time

def read_env(name, default=""):
    return os.environ.get(name, default)

payload = {
    "status": read_env("RAWR_REPORT_STATUS"),
    "exitCode": int(read_env("RAWR_REPORT_EXIT_CODE", "0") or "0"),
    "startedAt": int(read_env("RAWR_REPORT_STARTED_AT", "0") or "0"),
    "finishedAt": int(time.time()),
    "repoRoot": read_env("RAWR_REPORT_REPO_ROOT"),
    "patchBranch": read_env("RAWR_REPORT_PATCH_BRANCH"),
    "startBranch": read_env("RAWR_REPORT_START_BRANCH"),
    "before": {
        "upstreamMain": read_env("RAWR_REPORT_BEFORE_UPSTREAM_MAIN"),
        "originMain": read_env("RAWR_REPORT_BEFORE_ORIGIN_MAIN"),
        "originPatchBranch": read_env("RAWR_REPORT_BEFORE_ORIGIN_PATCH"),
        "localPatchBranch": read_env("RAWR_REPORT_BEFORE_LOCAL_PATCH"),
    },
    "after": {
        "upstreamMain": read_env("RAWR_REPORT_AFTER_UPSTREAM_MAIN"),
        "originMain": read_env("RAWR_REPORT_AFTER_ORIGIN_MAIN"),
        "originPatchBranch": read_env("RAWR_REPORT_AFTER_ORIGIN_PATCH"),
        "localPatchBranch": read_env("RAWR_REPORT_AFTER_LOCAL_PATCH"),
    },
    "notes": read_env("RAWR_REPORT_NOTES"),
}

path = read_env("RAWR_REPORT_JSON_PATH")
if path:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(payload, f, indent=2, sort_keys=True)
PY

  {
    echo "# Daily Rebase Run Summary ($RUN_DATE)"
    echo
    echo "- Status: ${RAWR_REPORT_STATUS:-}"
    echo "- Exit code: ${RAWR_REPORT_EXIT_CODE:-}"
    echo "- Patch branch: $PATCH_BRANCH"
    echo "- Start branch: $start_branch"
    echo "- Report JSON: $REPORT_JSON"
    echo
    echo "## Before"
    echo "- upstream/main: ${RAWR_REPORT_BEFORE_UPSTREAM_MAIN:-}"
    echo "- origin/main: ${RAWR_REPORT_BEFORE_ORIGIN_MAIN:-}"
    echo "- origin/$PATCH_BRANCH: ${RAWR_REPORT_BEFORE_ORIGIN_PATCH:-}"
    echo "- local $PATCH_BRANCH: ${RAWR_REPORT_BEFORE_LOCAL_PATCH:-}"
    echo
    echo "## After"
    echo "- upstream/main: ${RAWR_REPORT_AFTER_UPSTREAM_MAIN:-}"
    echo "- origin/main: ${RAWR_REPORT_AFTER_ORIGIN_MAIN:-}"
    echo "- origin/$PATCH_BRANCH: ${RAWR_REPORT_AFTER_ORIGIN_PATCH:-}"
    echo "- local $PATCH_BRANCH: ${RAWR_REPORT_AFTER_LOCAL_PATCH:-}"
    echo
    if [[ -n "${RAWR_REPORT_NOTES:-}" ]]; then
      echo "## Notes"
      echo "${RAWR_REPORT_NOTES}"
    fi
  } >"$SUMMARY_MD" 2>/dev/null || true
}

cleanup() {
  local code=$?

  if [[ -n "${validate_tmpdir:-}" ]]; then
    git worktree remove -f "$validate_tmpdir" >/dev/null 2>&1 || true
    rm -rf "$validate_tmpdir" >/dev/null 2>&1 || true
    validate_tmpdir=""
  fi

  if in_rebase; then
    log "aborting in-progress rebase to keep workspace clean"
    git rebase --abort || true
  fi

  if [[ "$(git rev-parse --abbrev-ref HEAD)" != "$start_branch" ]]; then
    git checkout "$start_branch" >/dev/null 2>&1 || true
  fi

  if [[ "$lock_acquired" == "1" && -d "$LOCK_DIR" ]]; then
    rm -rf "$LOCK_DIR"
  fi

  write_report
  exit "$code"
}
trap cleanup EXIT INT TERM

if mkdir "$LOCK_DIR" 2>/dev/null; then
  lock_acquired=1
  printf '{"pid":%s,"startedAt":%s,"startBranch":"%s","patchBranch":"%s"}\n' \
    "$$" \
    "$(date +%s)" \
    "$start_branch" \
    "$PATCH_BRANCH" \
    >"$LOCK_DIR/meta.json" 2>/dev/null || true
else
  # Stale lock override (6 hours) controlled by FORCE_STALE_LOCK=1.
  lock_age_seconds="$(
    python3 - <<PY 2>/dev/null || echo ""
import os, time
print(int(time.time() - os.path.getmtime("$LOCK_DIR")))
PY
  )"

  if [[ "${FORCE_STALE_LOCK:-0}" == "1" && -n "$lock_age_seconds" && "$lock_age_seconds" -gt 21600 ]]; then
    rm -rf "$LOCK_DIR" || true
    if mkdir "$LOCK_DIR" 2>/dev/null; then
      lock_acquired=1
      printf '{"pid":%s,"startedAt":%s,"startBranch":"%s","patchBranch":"%s","forceStaleLock":true}\n' \
        "$$" \
        "$(date +%s)" \
        "$start_branch" \
        "$PATCH_BRANCH" \
        >"$LOCK_DIR/meta.json" 2>/dev/null || true
    else
      RAWR_REPORT_STATUS="lock_held"
      RAWR_REPORT_EXIT_CODE="$EXIT_LOCK_HELD"
      exit "$EXIT_LOCK_HELD"
    fi
  else
    RAWR_REPORT_STATUS="lock_held"
    RAWR_REPORT_EXIT_CODE="$EXIT_LOCK_HELD"
    exit "$EXIT_LOCK_HELD"
  fi
fi

export RAWR_REPORT_REPO_ROOT="$ROOT"
export RAWR_REPORT_PATCH_BRANCH="$PATCH_BRANCH"
export RAWR_REPORT_START_BRANCH="$start_branch"
export RAWR_REPORT_STARTED_AT="$(date +%s)"

if [[ -n "$(git status --porcelain)" ]]; then
  RAWR_REPORT_STATUS="preflight_failed"
  RAWR_REPORT_EXIT_CODE=1
  export RAWR_REPORT_NOTES="Working tree must be clean before running daily rebase."
  exit 1
fi

if in_rebase; then
  RAWR_REPORT_STATUS="preflight_failed"
  RAWR_REPORT_EXIT_CODE=1
  export RAWR_REPORT_NOTES="Repository is already in a rebase state; abort/clean before retrying."
  exit 1
fi

log "preflight: fetching remotes"
git fetch upstream --prune
git fetch origin --prune

export RAWR_REPORT_BEFORE_UPSTREAM_MAIN="$(git rev-parse upstream/main)"
export RAWR_REPORT_BEFORE_ORIGIN_MAIN="$(git rev-parse origin/main)"
export RAWR_REPORT_BEFORE_ORIGIN_PATCH="$(git rev-parse "origin/$PATCH_BRANCH" 2>/dev/null || true)"
export RAWR_REPORT_BEFORE_LOCAL_PATCH="$(git rev-parse "$PATCH_BRANCH")"

log "verify: dry-run sync-upstream"
set +e
verify_out="$(DRY_RUN=1 rawr/sync-upstream.sh "$PATCH_BRANCH" 2>&1)"
verify_code=$?
set -e

if [[ "$verify_code" -ne 0 ]]; then
  RAWR_REPORT_STATUS="verify_conflict"
  RAWR_REPORT_EXIT_CODE="$EXIT_VERIFY_CONFLICT"
  export RAWR_REPORT_NOTES="$verify_out"
  exit "$EXIT_VERIFY_CONFLICT"
fi

log "apply: sync-upstream"
set +e
apply_out="$(rawr/sync-upstream.sh "$PATCH_BRANCH" 2>&1)"
apply_code=$?
set -e

if [[ "$apply_code" -ne 0 ]]; then
  if echo "$apply_out" | grep -q "CONFLICT"; then
    RAWR_REPORT_STATUS="apply_conflict"
    RAWR_REPORT_EXIT_CODE="$EXIT_APPLY_CONFLICT"
    export RAWR_REPORT_NOTES="$apply_out"
    exit "$EXIT_APPLY_CONFLICT"
  fi

  if echo "$apply_out" | grep -qiE "force-with-lease|rejected|protected branch|non-fast-forward|cannot lock ref"; then
    RAWR_REPORT_STATUS="push_blocked"
    RAWR_REPORT_EXIT_CODE="$EXIT_LEASE_OR_PUSH_BLOCKED"
    export RAWR_REPORT_NOTES="$apply_out"
    exit "$EXIT_LEASE_OR_PUSH_BLOCKED"
  fi

  RAWR_REPORT_STATUS="apply_failed"
  RAWR_REPORT_EXIT_CODE=1
  export RAWR_REPORT_NOTES="$apply_out"
  exit 1
fi

log "graphite: sync --no-restack"
local_patch="$(git rev-parse "$PATCH_BRANCH")"
remote_patch="$(git rev-parse "origin/$PATCH_BRANCH" 2>/dev/null || true)"
if [[ -z "$remote_patch" ]]; then
  RAWR_REPORT_STATUS="graphite_sync_failed"
  RAWR_REPORT_EXIT_CODE="$EXIT_GRAPHITE_RESTACK_FAILED"
  export RAWR_REPORT_NOTES="failed to resolve origin/$PATCH_BRANCH; refusing to run gt sync with --force"
  exit "$EXIT_GRAPHITE_RESTACK_FAILED"
fi

if [[ "$local_patch" != "$remote_patch" ]]; then
  RAWR_REPORT_STATUS="graphite_sync_failed"
  RAWR_REPORT_EXIT_CODE="$EXIT_GRAPHITE_RESTACK_FAILED"
  export RAWR_REPORT_NOTES="local $PATCH_BRANCH ($local_patch) does not match origin/$PATCH_BRANCH ($remote_patch); refusing to run gt sync with --force"
  exit "$EXIT_GRAPHITE_RESTACK_FAILED"
fi

set +e
gt_sync_out="$(gt sync --no-restack --force --no-interactive 2>&1)"
gt_sync_code=$?
set -e
if [[ "$gt_sync_code" -ne 0 ]]; then
  RAWR_REPORT_STATUS="graphite_sync_failed"
  RAWR_REPORT_EXIT_CODE="$EXIT_GRAPHITE_RESTACK_FAILED"
  export RAWR_REPORT_NOTES="$gt_sync_out"
  exit "$EXIT_GRAPHITE_RESTACK_FAILED"
fi

ls_out="$(gt ls --all)"
branch_count="$(echo "$ls_out" | awk '{print $2}' | sed '/^$/d' | wc -l | tr -d ' ')"

if [[ "$branch_count" -gt 1 ]]; then
  log "graphite: restack --upstack (descendants detected)"
  set +e
  gt_restack_out="$(gt restack --upstack 2>&1)"
  gt_restack_code=$?
  set -e
  if [[ "$gt_restack_code" -ne 0 ]]; then
    RAWR_REPORT_STATUS="graphite_restack_failed"
    RAWR_REPORT_EXIT_CODE="$EXIT_GRAPHITE_RESTACK_FAILED"
    export RAWR_REPORT_NOTES="$gt_restack_out"
    exit "$EXIT_GRAPHITE_RESTACK_FAILED"
  fi
else
  log "graphite: no descendants detected; skipping restack"
fi

log "validation: just fmt + cargo test --all-features"
validate_tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/rawr-rebase-daily-validate.XXXXXX")"
git worktree add --detach "$validate_tmpdir" "$PATCH_BRANCH" >/dev/null

set +e
(cd "$validate_tmpdir/codex-rs" && just fmt) 2>&1
fmt_code=$?
set -e
if [[ "$fmt_code" -ne 0 ]]; then
  RAWR_REPORT_STATUS="tests_failed"
  RAWR_REPORT_EXIT_CODE="$EXIT_TESTS_FAILED"
  export RAWR_REPORT_NOTES="just fmt failed"
  exit "$EXIT_TESTS_FAILED"
fi

if [[ -n "$(git -C "$validate_tmpdir" status --porcelain)" ]]; then
  fmt_files="$(git -C "$validate_tmpdir" status --porcelain | head -n 50)"
  RAWR_REPORT_STATUS="tests_failed"
  RAWR_REPORT_EXIT_CODE="$EXIT_TESTS_FAILED"
  export RAWR_REPORT_NOTES="just fmt produced changes on $PATCH_BRANCH (agent must commit and may need a second restack):\n$fmt_files"
  exit "$EXIT_TESTS_FAILED"
fi

set +e
(cd "$validate_tmpdir/codex-rs" && cargo test --all-features) 2>&1
test_code=$?
set -e
if [[ "$test_code" -ne 0 ]]; then
  RAWR_REPORT_STATUS="tests_failed"
  RAWR_REPORT_EXIT_CODE="$EXIT_TESTS_FAILED"
  export RAWR_REPORT_NOTES="cargo test --all-features failed"
  exit "$EXIT_TESTS_FAILED"
fi

git worktree remove -f "$validate_tmpdir" >/dev/null 2>&1 || true
rm -rf "$validate_tmpdir" >/dev/null 2>&1 || true
validate_tmpdir=""

git fetch upstream --prune
git fetch origin --prune

export RAWR_REPORT_AFTER_UPSTREAM_MAIN="$(git rev-parse upstream/main)"
export RAWR_REPORT_AFTER_ORIGIN_MAIN="$(git rev-parse origin/main)"
export RAWR_REPORT_AFTER_ORIGIN_PATCH="$(git rev-parse "origin/$PATCH_BRANCH" 2>/dev/null || true)"
export RAWR_REPORT_AFTER_LOCAL_PATCH="$(git rev-parse "$PATCH_BRANCH")"

RAWR_REPORT_STATUS="success"
RAWR_REPORT_EXIT_CODE="$EXIT_OK"
export RAWR_REPORT_JSON_PATH="$REPORT_JSON"
exit "$EXIT_OK"
