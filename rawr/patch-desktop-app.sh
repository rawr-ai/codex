#!/usr/bin/env bash
set -euo pipefail

APP_PATH="${CODEX_DESKTOP_APP_PATH:-/Applications/Codex.app}"
BUNDLED_BIN="${CODEX_DESKTOP_BUNDLED_BIN:-$APP_PATH/Contents/Resources/codex}"
RAWR_BIN="${CODEX_RAWR_BIN:-$HOME/.local/bin/codex-rawr-bin}"
BACKUP_DIR="${CODEX_DESKTOP_PATCH_BACKUP_DIR:-$APP_PATH/Contents/Resources/rawr-backups}"

mode="report"
rollback_backup=""

usage() {
  cat <<'EOF'
Usage: rawr/patch-desktop-app.sh [OPTIONS]

Patch the local Desktop app bundle to run the current rawr CLI binary.

Modes:
  --dry-run, --report       Report current state without changing files (default).
  --apply                   Backup bundled binary, install ~/.local/bin/codex-rawr-bin, then verify.
  --ensure                  Apply only when bundled binary does not match rawr binary.
  --rollback [BACKUP_PATH]  Restore from a backup. Defaults to the latest rawr backup.

Options:
  -h, --help                Show help.

Environment:
  CODEX_DESKTOP_APP_PATH             App bundle path. Default: /Applications/Codex.app
  CODEX_DESKTOP_BUNDLED_BIN          Bundled binary path.
  CODEX_RAWR_BIN                     Rawr binary to install. Default: ~/.local/bin/codex-rawr-bin
  CODEX_DESKTOP_PATCH_BACKUP_DIR     Backup directory inside the app bundle by default.

Note: Codex app updates can overwrite this local patch. Re-run --apply after updates.
EOF
}

log() {
  echo "rawr/patch-desktop-app: $*"
}

die() {
  echo "error: $*" >&2
  exit 1
}

hash_file() {
  shasum -a 256 "$1" | awk '{print $1}'
}

version_of() {
  local path="$1"
  if [[ -x "$path" ]]; then
    "$path" --version 2>/dev/null || echo "<version failed>"
  elif [[ -e "$path" ]]; then
    echo "<not executable>"
  else
    echo "<missing>"
  fi
}

require_file() {
  local path="$1"
  local label="$2"

  [[ -e "$path" ]] || die "$label does not exist: $path"
  [[ -f "$path" ]] || die "$label is not a regular file: $path"
  [[ -x "$path" ]] || die "$label is not executable: $path"
}

report_state() {
  echo "Desktop app:      $APP_PATH"
  echo "Bundled binary:   $BUNDLED_BIN"
  echo "Rawr binary:      $RAWR_BIN"
  echo "Backup directory: $BACKUP_DIR"
  echo

  if [[ -e "$BUNDLED_BIN" ]]; then
    echo "Bundled version:  $(version_of "$BUNDLED_BIN")"
    echo "Bundled sha256:   $(hash_file "$BUNDLED_BIN")"
  else
    echo "Bundled version:  <missing>"
    echo "Bundled sha256:   <missing>"
  fi

  if [[ -e "$RAWR_BIN" ]]; then
    echo "Rawr version:     $(version_of "$RAWR_BIN")"
    echo "Rawr sha256:      $(hash_file "$RAWR_BIN")"
  else
    echo "Rawr version:     <missing>"
    echo "Rawr sha256:      <missing>"
  fi

  if [[ -d "$BACKUP_DIR" ]]; then
    echo
    echo "Backups:"
    find "$BACKUP_DIR" -maxdepth 1 -type f -name 'codex.*.bak' -print | sort | sed 's/^/  /'
  fi

  echo
  echo "Warning: Codex app updates can overwrite this local patch. Re-run --apply after updates."
}

latest_backup() {
  find "$BACKUP_DIR" -maxdepth 1 -type f -name 'codex.*.bak' -print 2>/dev/null | sort | tail -n 1
}

write_backup_metadata() {
  local backup_path="$1"
  local metadata_path="$2"
  local source_version="$3"
  local source_hash="$4"

  {
    echo "created_at=$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    echo "app_path=$APP_PATH"
    echo "bundled_binary=$BUNDLED_BIN"
    echo "backup_path=$backup_path"
    echo "backup_version=$(version_of "$backup_path")"
    echo "backup_sha256=$(hash_file "$backup_path")"
    echo "rawr_binary=$RAWR_BIN"
    echo "rawr_version=$source_version"
    echo "rawr_sha256=$source_hash"
  } >"$metadata_path"
}

apply_patch_to_app() {
  require_file "$BUNDLED_BIN" "bundled Codex binary"
  require_file "$RAWR_BIN" "rawr Codex binary"

  local source_version source_hash timestamp backup_path metadata_path installed_version installed_hash
  source_version="$(version_of "$RAWR_BIN")"
  source_hash="$(hash_file "$RAWR_BIN")"
  timestamp="$(date -u '+%Y%m%dT%H%M%SZ')"
  backup_path="$BACKUP_DIR/codex.$timestamp.bak"
  metadata_path="$backup_path.meta"

  mkdir -p "$BACKUP_DIR"

  if [[ -e "$backup_path" || -e "$metadata_path" ]]; then
    die "backup path already exists: $backup_path"
  fi

  log "backing up bundled binary to $backup_path"
  cp -p "$BUNDLED_BIN" "$backup_path"
  write_backup_metadata "$backup_path" "$metadata_path" "$source_version" "$source_hash"

  log "installing rawr binary into Desktop app bundle"
  install -m 0755 "$RAWR_BIN" "$BUNDLED_BIN"

  installed_version="$(version_of "$BUNDLED_BIN")"
  installed_hash="$(hash_file "$BUNDLED_BIN")"

  if [[ "$installed_version" != "$source_version" ]]; then
    die "patched binary version mismatch: got '$installed_version', expected '$source_version'"
  fi

  if [[ "$installed_hash" != "$source_hash" ]]; then
    die "patched binary hash mismatch: got '$installed_hash', expected '$source_hash'"
  fi

  log "patched Desktop app bundled binary with rawr build: $installed_version"
  log "backup metadata: $metadata_path"
  echo "Warning: Codex app updates can overwrite this local patch. Re-run --apply after updates."
}

ensure_patch() {
  require_file "$BUNDLED_BIN" "bundled Codex binary"
  require_file "$RAWR_BIN" "rawr Codex binary"

  local bundled_hash rawr_hash
  bundled_hash="$(hash_file "$BUNDLED_BIN")"
  rawr_hash="$(hash_file "$RAWR_BIN")"

  if [[ "$bundled_hash" == "$rawr_hash" ]]; then
    log "Desktop app already uses rawr build: $(version_of "$BUNDLED_BIN")"
    return 0
  fi

  log "Desktop app bundled binary differs from rawr build; patching"
  apply_patch_to_app
}

rollback_patch() {
  local backup_path="$rollback_backup"
  if [[ -z "$backup_path" ]]; then
    backup_path="$(latest_backup)"
  fi

  [[ -n "$backup_path" ]] || die "no backup found in $BACKUP_DIR"
  require_file "$backup_path" "backup binary"

  local backup_version backup_hash restored_version restored_hash
  backup_version="$(version_of "$backup_path")"
  backup_hash="$(hash_file "$backup_path")"

  log "restoring bundled binary from $backup_path"
  install -m 0755 "$backup_path" "$BUNDLED_BIN"

  restored_version="$(version_of "$BUNDLED_BIN")"
  restored_hash="$(hash_file "$BUNDLED_BIN")"

  if [[ "$restored_version" != "$backup_version" ]]; then
    die "rollback version mismatch: got '$restored_version', expected '$backup_version'"
  fi

  if [[ "$restored_hash" != "$backup_hash" ]]; then
    die "rollback hash mismatch: got '$restored_hash', expected '$backup_hash'"
  fi

  log "rolled back Desktop app bundled binary: $restored_version"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run|--report)
      mode="report"
      shift
      ;;
    --apply)
      mode="apply"
      shift
      ;;
    --ensure)
      mode="ensure"
      shift
      ;;
    --rollback)
      mode="rollback"
      rollback_backup="${2:-}"
      if [[ -n "$rollback_backup" && "$rollback_backup" != -* ]]; then
        shift 2
      else
        rollback_backup=""
        shift
      fi
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

case "$mode" in
  report)
    report_state
    ;;
  apply)
    apply_patch_to_app
    ;;
  ensure)
    ensure_patch
    ;;
  rollback)
    rollback_patch
    ;;
  *)
    die "unexpected mode: $mode"
    ;;
esac
