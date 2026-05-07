#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SELECTOR_SOURCE="$SCRIPT_DIR/codex-selector.sh"
MANAGER_SOURCE="$SCRIPT_DIR/$(basename "${BASH_SOURCE[0]}")"

SWITCH_HOME="${CODEX_SWITCH_HOME:-$HOME/.codex-switch}"
BIN_DIR="$SWITCH_HOME/bin"
TARGETS_DIR="$SWITCH_HOME/targets"
STATE_DIR="$SWITCH_HOME/state"
BACKUP_DIR="$STATE_DIR/backups"
PROTECTED_SNAPSHOT_DIR="$STATE_DIR/protected-home-snapshots"
LOG_DIR="${CODEX_SWITCH_LOG_DIR:-$HOME/Library/Logs/codex-switchboard}"
CURRENT_JSON="$SWITCH_HOME/current.json"
ENV_SH="$SWITCH_HOME/env.sh"
MANIFEST_JSON="$STATE_DIR/install-manifest.json"
LOCK_DIR="$STATE_DIR/lock"

DEFAULT_UPSTREAM_BIN="/opt/homebrew/bin/codex"
if [[ ! -x "$DEFAULT_UPSTREAM_BIN" && -x "/usr/local/bin/codex" ]]; then
  DEFAULT_UPSTREAM_BIN="/usr/local/bin/codex"
elif [[ ! -x "$DEFAULT_UPSTREAM_BIN" ]]; then
  DEFAULT_UPSTREAM_BIN="$HOME/.volta/bin/codex"
fi

UPSTREAM_BIN="${CODEX_SWITCH_UPSTREAM_BIN:-$DEFAULT_UPSTREAM_BIN}"
UPSTREAM_HOME="${CODEX_SWITCH_UPSTREAM_HOME:-$HOME/.codex}"
RAWR_BIN="${CODEX_SWITCH_RAWR_BIN:-$HOME/.local/bin/codex-rawr-bin}"
RAWR_HOME="${CODEX_SWITCH_RAWR_HOME:-$HOME/.codex-rawr}"

LOCAL_CODEX="$HOME/.local/bin/codex"
BUN_CODEX="$HOME/.bun/bin/codex"
VSCODE_SETTINGS="$HOME/Library/Application Support/Code/User/settings.json"

DESKTOP_APP="${CODEX_DESKTOP_APP_PATH:-/Applications/Codex.app}"
DESKTOP_HELPER="${CODEX_DESKTOP_BUNDLED_BIN:-$DESKTOP_APP/Contents/Resources/codex}"
DESKTOP_LABEL="com.codex.switchboard.desktop"
DESKTOP_PLIST="$HOME/Library/LaunchAgents/$DESKTOP_LABEL.plist"
DESKTOP_ENSURE="$BIN_DIR/codex-desktop-ensure"
ENV_LABEL="com.codex.switchboard.env"
ENV_PLIST="$HOME/Library/LaunchAgents/$ENV_LABEL.plist"
LAUNCHD_DOMAIN="gui/$(id -u)"

DRY_RUN=0
NO_RESTART=0
NO_DESKTOP=0
NO_USER_WIRING=0
NO_VSCODE=0
NO_LAUNCHD=0
STRICT=0

usage() {
  cat <<'EOF'
Usage: rawr/codex-switch/codex-use.sh <command> [options]

Commands:
  install              Install the local switchboard. Defaults to the current target if present, otherwise upstream.
  upstream             Select upstream Codex and reconcile controlled consumers.
  rawr                 Select RAWR Codex and reconcile controlled consumers.
  status               Print selected target and quick selector state.
  doctor               Print the full switchboard surface matrix.
  desktop-install      Patch Codex Desktop's helper to the selector and install the restore LaunchAgent.
  uninstall            Remove switchboard-owned artifacts and restore manifest-backed backups where possible.

Options:
  --target <name>      Initial install target: upstream or rawr.
  --dry-run            Print mutations without performing them.
  --no-restart         Do not restart Happy after switching.
  --no-desktop         Skip Desktop helper/LaunchAgent work.
  --no-user-wiring     Skip shell/local/bin/bun/bin wiring.
  --no-vscode          Skip VS Code settings mutation.
  --no-launchd         Skip launchctl and LaunchAgent env mutation.
  --strict             Treat target version/hash warnings as switch-blocking where supported.
  -h, --help           Show help.
EOF
}

log() {
  printf '%s\n' "$*"
}

warn() {
  printf 'warning: %s\n' "$*" >&2
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

run() {
  if [[ "$DRY_RUN" -eq 1 ]]; then
    printf '[dry-run] '
    printf '%q ' "$@"
    printf '\n'
  else
    "$@"
  fi
}

now_iso() {
  date -u '+%Y-%m-%dT%H:%M:%SZ'
}

now_epoch() {
  date '+%s'
}

sha256_file() {
  local path="$1"
  [[ -e "$path" ]] || {
    printf '<missing>'
    return 0
  }
  shasum -a 256 "$path" | awk '{print $1}'
}

version_of() {
  local path="$1"
  if [[ -x "$path" ]]; then
    "$path" --version 2>/dev/null | head -n 1 || printf '<version failed>'
  elif [[ -e "$path" ]]; then
    printf '<not executable>'
  else
    printf '<missing>'
  fi
}

real_path() {
  python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$1"
}

require_python() {
  command -v python3 >/dev/null 2>&1 || die "python3 is required for codex-use management commands"
}

acquire_lock() {
  mkdir -p "$STATE_DIR"
  if ! mkdir "$LOCK_DIR" 2>/dev/null; then
    die "another codex-use operation is active: $LOCK_DIR"
  fi
  trap 'rmdir "$LOCK_DIR" 2>/dev/null || true' EXIT
}

ensure_dirs() {
  run mkdir -p "$BIN_DIR" "$TARGETS_DIR" "$STATE_DIR" "$BACKUP_DIR" "$PROTECTED_SNAPSHOT_DIR" "$LOG_DIR"
}

target_bin() {
  case "$1" in
    upstream) printf '%s\n' "$UPSTREAM_BIN" ;;
    rawr) printf '%s\n' "$RAWR_BIN" ;;
    *) die "unsupported target: $1" ;;
  esac
}

target_home() {
  case "$1" in
    upstream) printf '%s\n' "$UPSTREAM_HOME" ;;
    rawr) printf '%s\n' "$RAWR_HOME" ;;
    *) die "unsupported target: $1" ;;
  esac
}

validate_target() {
  local target="$1" bin home selector_real bin_real
  bin="$(target_bin "$target")"
  home="$(target_home "$target")"

  [[ "$target" == "upstream" || "$target" == "rawr" ]] || die "unsupported target: $target"
  [[ "$bin" = /* ]] || die "$target binary must be absolute: $bin"
  [[ "$home" = /* ]] || die "$target CODEX_HOME must be absolute: $home"
  [[ -x "$bin" ]] || die "$target binary is missing or not executable: $bin"
  [[ -d "$home" ]] || run mkdir -p "$home"

  if [[ -e "$BIN_DIR/codex" ]]; then
    selector_real="$(real_path "$BIN_DIR/codex")"
    bin_real="$(real_path "$bin")"
    [[ "$selector_real" != "$bin_real" ]] || die "$target binary resolves to the selector itself"
  fi

  if ! "$bin" --version >/dev/null 2>&1; then
    if [[ "$STRICT" -eq 1 ]]; then
      die "$target binary failed --version: $bin"
    fi
    warn "$target binary failed --version: $bin"
  fi
}

write_manifest() {
  [[ "$DRY_RUN" -eq 1 ]] && {
    log "[dry-run] write manifest $MANIFEST_JSON"
    return 0
  }

  python3 - "$MANIFEST_JSON" "$SWITCH_HOME" <<'PY'
import json
import os
import sys
from datetime import datetime, timezone

path, switch_home = sys.argv[1], sys.argv[2]
data = {
    "schema": 1,
    "switch_home": switch_home,
    "installed_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "managed_artifacts": [],
}
if os.path.exists(path):
    try:
        with open(path, "r", encoding="utf-8") as handle:
            old = json.load(handle)
        data["installed_at"] = old.get("installed_at", data["installed_at"])
        data["managed_artifacts"] = old.get("managed_artifacts", [])
    except Exception:
        pass
os.makedirs(os.path.dirname(path), exist_ok=True)
tmp = f"{path}.tmp"
with open(tmp, "w", encoding="utf-8") as handle:
    json.dump(data, handle, indent=2, sort_keys=True)
    handle.write("\n")
os.replace(tmp, path)
PY
}

record_artifact() {
  local kind="$1" path="$2" backup="${3:-}" ownership="${4:-switchboard}"
  [[ "$DRY_RUN" -eq 1 ]] && {
    log "[dry-run] record $kind $path"
    return 0
  }

  python3 - "$MANIFEST_JSON" "$kind" "$path" "$backup" "$ownership" <<'PY'
import json
import os
import sys
from datetime import datetime, timezone

manifest, kind, path, backup, ownership = sys.argv[1:]
try:
    with open(manifest, "r", encoding="utf-8") as handle:
        data = json.load(handle)
except Exception:
    data = {"schema": 1, "managed_artifacts": []}

old_items = [
    item
    for item in data.get("managed_artifacts", [])
    if item.get("kind") == kind and item.get("path") == path
]
old = old_items[-1] if old_items else {}

history = []
for item in old_items:
    for key in ("first_backup", "backup"):
        value = item.get(key)
        if value and value not in history:
            history.append(value)
    for value in item.get("backup_history", []) or []:
        if value and value not in history:
            history.append(value)
if backup and backup not in history:
    history.append(backup)

first_backup = old.get("first_backup") or (history[0] if history else None)

entry = {
    "kind": kind,
    "path": path,
    "backup": backup or None,
    "first_backup": first_backup,
    "backup_history": history,
    "ownership": ownership,
    "first_recorded_at": old.get("first_recorded_at") or old.get("recorded_at"),
    "recorded_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
}
items = [item for item in data.get("managed_artifacts", []) if not (item.get("kind") == kind and item.get("path") == path)]
items.append(entry)
data["managed_artifacts"] = items

tmp = f"{manifest}.tmp"
with open(tmp, "w", encoding="utf-8") as handle:
    json.dump(data, handle, indent=2, sort_keys=True)
    handle.write("\n")
os.replace(tmp, manifest)
PY
}

backup_path_if_needed() {
  local path="$1" label="$2" backup=""
  if [[ -e "$path" || -L "$path" ]]; then
    backup="$BACKUP_DIR/${label}.$(now_epoch).bak"
    if [[ "$DRY_RUN" -eq 1 ]]; then
      log "[dry-run] backup $path -> $backup"
    else
      mkdir -p "$BACKUP_DIR"
      cp -pR "$path" "$backup"
    fi
  fi
  printf '%s\n' "$backup"
}

snapshot_protected_home() {
  local home="$1" label="$2" target="$3" snapshot epoch metadata
  [[ -d "$home" ]] || return 0
  epoch="$(now_epoch)"
  snapshot="$PROTECTED_SNAPSHOT_DIR/$epoch-$target-$label"

  if [[ "$DRY_RUN" -eq 1 ]]; then
    log "[dry-run] snapshot protected $label home $home -> $snapshot"
    return 0
  fi

  mkdir -p "$snapshot"
  for file in config.toml .codex-global-state.json session_index.jsonl; do
    if [[ -e "$home/$file" || -L "$home/$file" ]]; then
      mkdir -p "$snapshot/$(dirname "$file")"
      cp -p "$home/$file" "$snapshot/$file"
    fi
  done

  metadata="$snapshot/auth.json.metadata"
  if [[ -e "$home/auth.json" || -L "$home/auth.json" ]]; then
    {
      printf 'path=%s\n' "$home/auth.json"
      stat -f 'mtime=%Sm' "$home/auth.json" 2>/dev/null || true
      stat -f 'size=%z' "$home/auth.json" 2>/dev/null || true
      printf 'sha256=%s\n' "$(sha256_file "$home/auth.json")"
    } >"$metadata"
    chmod 0600 "$metadata"
  fi

  record_artifact "protected-home-snapshot" "$snapshot" "" "protected-backup"
}

snapshot_protected_homes() {
  local target="$1"
  snapshot_protected_home "$UPSTREAM_HOME" "upstream" "$target"
  snapshot_protected_home "$RAWR_HOME" "rawr" "$target"
}

install_selector() {
  [[ -x "$SELECTOR_SOURCE" ]] || die "missing selector source: $SELECTOR_SOURCE"
  ensure_dirs
  run cp "$SELECTOR_SOURCE" "$BIN_DIR/codex-selector.sh"
  run chmod 0755 "$BIN_DIR/codex-selector.sh"
  run cp "$SELECTOR_SOURCE" "$BIN_DIR/codex"
  run chmod 0755 "$BIN_DIR/codex"
  record_artifact "selector-source" "$BIN_DIR/codex-selector.sh" "" "switchboard"
  record_artifact "selector" "$BIN_DIR/codex" "" "switchboard"
}

install_manager() {
  ensure_dirs
  run cp "$MANAGER_SOURCE" "$BIN_DIR/codex-use"
  run chmod 0755 "$BIN_DIR/codex-use"
  record_artifact "manager" "$BIN_DIR/codex-use" "" "switchboard"
}

write_target_file() {
  local target="$1" bin home version sha target_json tmp
  bin="$(target_bin "$target")"
  home="$(target_home "$target")"
  version="$(version_of "$bin")"
  sha="$(sha256_file "$bin")"
  target_json="$TARGETS_DIR/$target.json"
  tmp="$target_json.tmp"

  [[ "$DRY_RUN" -eq 1 ]] && {
    log "[dry-run] write target $target_json"
    return 0
  }

  python3 - "$target_json" "$target" "$bin" "$home" "$version" "$sha" <<'PY'
import json
import os
import sys

path, target, bin_path, home, version, sha = sys.argv[1:]
data = {
    "schema": 1,
    "name": target,
    "bin": bin_path,
    "home": home,
    "version": version,
    "sha256": sha,
}
tmp = f"{path}.tmp"
with open(tmp, "w", encoding="utf-8") as handle:
    json.dump(data, handle, indent=2, sort_keys=True)
    handle.write("\n")
os.replace(tmp, path)
PY
  record_artifact "target" "$target_json" "" "switchboard"
}

write_current_json() {
  local target="$1" bin home version sha updated epoch tmp
  bin="$(target_bin "$target")"
  home="$(target_home "$target")"
  version="$(version_of "$bin")"
  sha="$(sha256_file "$bin")"
  updated="$(now_iso)"
  epoch="$(now_epoch)"
  tmp="$CURRENT_JSON.tmp"

  [[ "$DRY_RUN" -eq 1 ]] && {
    log "[dry-run] atomically write $CURRENT_JSON for $target"
    return 0
  }

  python3 - "$tmp" "$target" "$bin" "$home" "$updated" "$epoch" "$version" "$sha" <<'PY'
import json
import os
import sys

path, target, bin_path, home, updated, epoch, version, sha = sys.argv[1:]
if not os.path.isabs(bin_path):
    raise SystemExit("bin must be absolute")
if not os.path.isabs(home):
    raise SystemExit("home must be absolute")
data = {
    "schema": 1,
    "selected": target,
    "bin": bin_path,
    "home": home,
    "updated_at": updated,
    "selected_at_epoch": int(epoch),
    "version": version,
    "sha256": sha,
}
with open(path, "w", encoding="utf-8") as handle:
    json.dump(data, handle, indent=2, sort_keys=True)
    handle.write("\n")
with open(path, "r", encoding="utf-8") as handle:
    json.load(handle)
PY
  mv "$tmp" "$CURRENT_JSON"
  record_artifact "current" "$CURRENT_JSON" "" "switchboard"
}

write_env_sh() {
  local target="$1" home
  home="$(target_home "$target")"

  [[ "$DRY_RUN" -eq 1 ]] && {
    log "[dry-run] write $ENV_SH"
    return 0
  }

  cat >"$ENV_SH" <<EOF
# Managed by Codex switchboard. Do not edit inside this file.
export CODEX_SWITCH_HOME="$SWITCH_HOME"
if [ -x /usr/bin/awk ]; then
  _codex_switch_rest="\$(printf '%s' "\$PATH" | /usr/bin/awk -v RS=: -v ORS=: -v switch="$BIN_DIR" 'length(\$0) && \$0 != switch && !seen[\$0]++ { print }')"
  _codex_switch_rest="\${_codex_switch_rest%:}"
  if [ -n "\$_codex_switch_rest" ]; then
    export PATH="$BIN_DIR:\$_codex_switch_rest"
  else
    export PATH="$BIN_DIR"
  fi
  unset _codex_switch_rest
else
  export PATH="$BIN_DIR:\$PATH"
fi
export CODEX_HOME="$home"
export CODEX_SWITCHBOARD_TARGET="$target"
if [ "$target" = "rawr" ]; then
  export CODEX_FORK_HOME="$home"
else
  unset CODEX_FORK_HOME
fi
EOF
  chmod 0644 "$ENV_SH"
  record_artifact "env" "$ENV_SH" "" "switchboard"
}

install_managed_shell_block() {
  [[ "$NO_USER_WIRING" -eq 1 ]] && return 0

  local files=("$HOME/.zshenv" "$HOME/.zprofile" "$HOME/.zshrc" "$HOME/.profile" "$HOME/.bashrc")
  local file backup
  for file in "${files[@]}"; do
    if [[ ! -e "$file" ]]; then
      [[ "$DRY_RUN" -eq 1 ]] && log "[dry-run] create shell startup file $file"
      [[ "$DRY_RUN" -eq 0 ]] && touch "$file"
    fi
    backup="$(backup_path_if_needed "$file" "$(basename "$file")")"

    if [[ "$DRY_RUN" -eq 1 ]]; then
      log "[dry-run] install managed codex-switchboard block in $file"
    else
      python3 - "$file" "$ENV_SH" <<'PY'
import sys

path, env_path = sys.argv[1], sys.argv[2]
start = "# >>> codex-switchboard >>>"
end = "# <<< codex-switchboard <<<"
block = f"""{start}
if [ -f "{env_path}" ]; then
  . "{env_path}"
fi
{end}
"""
try:
    with open(path, "r", encoding="utf-8") as handle:
        text = handle.read()
except FileNotFoundError:
    text = ""
if start in text and end in text:
    before = text.split(start, 1)[0]
    after = text.split(end, 1)[1]
    text = before + block + after.lstrip("\n")
else:
    if text and not text.endswith("\n"):
        text += "\n"
    text += "\n" + block
with open(path, "w", encoding="utf-8") as handle:
    handle.write(text)
PY
    fi
    record_artifact "shell-block" "$file" "$backup" "managed-block"
  done
  neutralize_direct_shell_home_exports "${files[@]}"
}

neutralize_direct_shell_home_exports() {
  local file
  for file in "$@"; do
    [[ -f "$file" ]] || continue
    if [[ "$DRY_RUN" -eq 1 ]]; then
      grep -Eq '^[[:space:]]*(export[[:space:]]+)?CODEX_HOME=' "$file" && log "[dry-run] comment direct CODEX_HOME exports in $file"
      continue
    fi
    python3 - "$file" <<'PY'
import re
import sys

path = sys.argv[1]
text = open(path, "r", encoding="utf-8").read()
changed = False
out = []
pattern = re.compile(r'^(\s*)(export\s+)?CODEX_HOME=.*$')
for line in text.splitlines(keepends=True):
    body = line[:-1] if line.endswith("\n") else line
    newline = "\n" if line.endswith("\n") else ""
    if pattern.match(body) and "codex-switchboard disabled" not in body:
        out.append(f"# codex-switchboard disabled direct CODEX_HOME export: {body}{newline}")
        changed = True
    else:
        out.append(line)
if changed:
    with open(path, "w", encoding="utf-8") as handle:
        handle.write("".join(out))
PY
  done
}

wire_symlink() {
  local path="$1" label="$2" backup=""
  [[ "$NO_USER_WIRING" -eq 1 ]] && return 0

  if [[ -L "$path" && "$(readlink "$path")" == "$BIN_DIR/codex" ]]; then
    record_artifact "symlink" "$path" "" "switchboard"
    return 0
  fi

  backup="$(backup_path_if_needed "$path" "$label")"
  run mkdir -p "$(dirname "$path")"
  run ln -sfn "$BIN_DIR/codex" "$path"
  record_artifact "symlink" "$path" "$backup" "switchboard"
}

update_vscode_setting() {
  [[ "$NO_VSCODE" -eq 1 ]] && return 0
  [[ -e "$VSCODE_SETTINGS" ]] || return 0

  local backup
  backup="$(backup_path_if_needed "$VSCODE_SETTINGS" "vscode-settings.json")"

  if [[ "$DRY_RUN" -eq 1 ]]; then
    log "[dry-run] set chatgpt.cliExecutable in $VSCODE_SETTINGS"
    record_artifact "vscode-setting" "$VSCODE_SETTINGS" "$backup" "managed-setting"
    return 0
  fi

  if ! python3 - "$VSCODE_SETTINGS" "$BIN_DIR/codex" <<'PY'
import json
import re
import sys

path, selector = sys.argv[1], sys.argv[2]
with open(path, "r", encoding="utf-8") as handle:
    text = handle.read()

try:
    data = json.loads(text)
except json.JSONDecodeError:
    line_re = re.compile(r'(?m)^([ \t]*"chatgpt[.]cliExecutable"[ \t]*:[ \t]*")[^"]*("[ \t]*,?[ \t]*)$')
    if line_re.search(text):
        updated = line_re.sub(lambda match: f"{match.group(1)}{selector}{match.group(2)}", text, count=1)
    else:
        closing = text.rfind("}")
        if closing == -1:
            raise
        prefix = text[:closing].rstrip()
        comma = "" if (not prefix or prefix.endswith("{") or prefix.endswith(",")) else ","
        updated = f'{prefix}{comma}\n\t"chatgpt.cliExecutable": {json.dumps(selector)}\n{text[closing:]}'
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(updated)
else:
    data["chatgpt.cliExecutable"] = selector
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(data, handle, indent="\t")
        handle.write("\n")
PY
  then
    warn "could not parse VS Code settings as JSON; leaving untouched: $VSCODE_SETTINGS"
    return 0
  fi

  record_artifact "vscode-setting" "$VSCODE_SETTINGS" "$backup" "managed-setting"
}

write_env_launch_agent() {
  local target="$1" home
  [[ "$NO_LAUNCHD" -eq 1 ]] && return 0
  [[ "$(uname -s)" == "Darwin" ]] || return 0
  home="$(target_home "$target")"

  if [[ "$DRY_RUN" -eq 1 ]]; then
    log "[dry-run] write and bootstrap $ENV_PLIST"
    return 0
  fi

  mkdir -p "$(dirname "$ENV_PLIST")"
  cat >"$ENV_PLIST" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>$ENV_LABEL</string>
  <key>ProgramArguments</key>
  <array>
    <string>/bin/sh</string>
    <string>-c</string>
    <string>/bin/launchctl setenv CODEX_HOME "$home"; /bin/launchctl setenv CODEX_SWITCHBOARD_TARGET "$target"; /bin/launchctl setenv CODEX_SWITCH_HOME "$SWITCH_HOME"</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
</dict>
</plist>
EOF
  plutil -lint "$ENV_PLIST" >/dev/null
  launchctl bootout "$LAUNCHD_DOMAIN" "$ENV_PLIST" >/dev/null 2>&1 || true
  launchctl bootstrap "$LAUNCHD_DOMAIN" "$ENV_PLIST" >/dev/null 2>&1 || true
  launchctl setenv CODEX_HOME "$home" || true
  launchctl setenv CODEX_SWITCHBOARD_TARGET "$target" || true
  launchctl setenv CODEX_SWITCH_HOME "$SWITCH_HOME" || true
  record_artifact "launch-agent" "$ENV_PLIST" "" "switchboard"
}

retire_conflicting_launch_agents() {
  [[ "$NO_LAUNCHD" -eq 1 ]] && return 0
  [[ "$(uname -s)" == "Darwin" ]] || return 0

  local candidates=(
    "$HOME/Library/LaunchAgents/com.openai.codex.env.plist"
    "$HOME/Library/LaunchAgents/com.rawr.codex-desktop-patch.plist"
  )
  local plist label backup

  for plist in "${candidates[@]}"; do
    [[ -e "$plist" ]] || continue
    if ! grep -Eq 'codex-rawr|rawr-ai/codex|CODEX_HOME /Users/.*/\.codex-rawr|CODEX_RAWR_BIN' "$plist" 2>/dev/null; then
      continue
    fi
    label="$(basename "$plist" .plist)"
    backup="$(backup_path_if_needed "$plist" "$label.plist")"
    if [[ "$DRY_RUN" -eq 1 ]]; then
      log "[dry-run] retire conflicting LaunchAgent $plist"
    else
      launchctl bootout "$LAUNCHD_DOMAIN" "$plist" >/dev/null 2>&1 || true
      mv "$plist" "$plist.codex-switchboard-disabled"
    fi
    record_artifact "retired-launch-agent" "$plist" "$backup" "disabled-conflict"
  done
}

restart_happy_if_running() {
  [[ "$NO_RESTART" -eq 1 ]] && return 0
  command -v happy >/dev/null 2>&1 || return 0
  pgrep -f 'happy-coder.*daemon' >/dev/null 2>&1 || return 0

  local target="$1" home
  home="$(target_home "$target")"
  if [[ "$DRY_RUN" -eq 1 ]]; then
    log "[dry-run] restart Happy daemon with CODEX_HOME=$home and PATH=$BIN_DIR:\$PATH"
    return 0
  fi

  CODEX_HOME="$home" PATH="$BIN_DIR:$PATH" happy daemon stop >/dev/null 2>&1 || true
  CODEX_HOME="$home" PATH="$BIN_DIR:$PATH" happy daemon start >/dev/null 2>&1 || warn "Happy daemon restart failed; run codex-use doctor"
}

generate_desktop_ensure() {
  [[ "$NO_DESKTOP" -eq 1 ]] && return 0
  [[ "$(uname -s)" == "Darwin" ]] || return 0
  ensure_dirs

  if [[ "$DRY_RUN" -eq 1 ]]; then
    log "[dry-run] generate $DESKTOP_ENSURE"
    return 0
  fi

  cat >"$DESKTOP_ENSURE" <<EOF
#!/usr/bin/env bash
set -euo pipefail

SWITCH_HOME="\${CODEX_SWITCH_HOME:-$SWITCH_HOME}"
SELECTOR="\$SWITCH_HOME/bin/codex"
APP_PATH="\${CODEX_DESKTOP_APP_PATH:-$DESKTOP_APP}"
BUNDLED_BIN="\${CODEX_DESKTOP_BUNDLED_BIN:-\$APP_PATH/Contents/Resources/codex}"
BACKUP_DIR="\${CODEX_SWITCH_DESKTOP_BACKUP_DIR:-$BACKUP_DIR}"
STATE_DIR="\${CODEX_SWITCH_DESKTOP_STATE_DIR:-$STATE_DIR/desktop}"
LOG_DIR="\${CODEX_SWITCH_LOG_DIR:-$LOG_DIR}"
LOCK_DIR=""

log() {
  mkdir -p "\$LOG_DIR"
  printf '%s codex-switch desktop-ensure: %s\n' "\$(date -u '+%Y-%m-%dT%H:%M:%SZ')" "\$*" | tee -a "\$LOG_DIR/desktop-ensure.log" >/dev/null
}

hash_file() {
  shasum -a 256 "\$1" | awk '{print \$1}'
}

file_signature() {
  if [[ ! -e "\$1" ]]; then
    echo '<missing>'
  elif stat -f '%z:%m:%N' "\$1" >/dev/null 2>&1; then
    stat -f '%z:%m:%N' "\$1"
  else
    stat -c '%s:%Y:%n' "\$1"
  fi
}

wait_for_stable_bundle() {
  local first second attempt
  for attempt in {1..10}; do
    first="\$(file_signature "\$BUNDLED_BIN")"
    sleep 2
    second="\$(file_signature "\$BUNDLED_BIN")"
    [[ "\$first" == "\$second" ]] && return 0
    log "Desktop helper is still changing; waiting before restore attempt \$attempt"
  done
}

sparkle_running() {
  ps -axo command= | grep -E 'Sparkle.*(com\\.openai\\.codex|Codex)|Autoupdate com\\.openai\\.codex' | grep -v grep >/dev/null 2>&1
}

wait_for_sparkle() {
  local attempt
  for attempt in {1..12}; do
    sparkle_running || return 0
    log "Sparkle updater appears active; waiting before restore attempt \$attempt"
    sleep 5
  done
}

desktop_app_server_pids() {
  ps -axo pid=,command= | awk '
    index(\$0, "app-server") && (index(\$0, "Codex.app") || index(\$0, "codex-rawr") || index(\$0, "codex")) {
      print \$1
    }
  '
}

mkdir -p "\$STATE_DIR" "\$BACKUP_DIR"
LOCK_DIR="\$STATE_DIR/lock"
if ! mkdir "\$LOCK_DIR" 2>/dev/null; then
  log "another ensure run is active; skipping"
  exit 0
fi
trap 'rmdir "\$LOCK_DIR" 2>/dev/null || true' EXIT

[[ -x "\$SELECTOR" ]] || { log "selector missing or not executable: \$SELECTOR"; exit 0; }
[[ -e "\$BUNDLED_BIN" ]] || { log "Desktop helper missing: \$BUNDLED_BIN"; exit 0; }

selector_hash="\$(hash_file "\$SELECTOR")"
bundled_hash="\$(hash_file "\$BUNDLED_BIN")"
if [[ "\$selector_hash" == "\$bundled_hash" ]]; then
  log "Desktop helper already matches switchboard selector"
  exit 0
fi

wait_for_sparkle
wait_for_stable_bundle

backup="\$BACKUP_DIR/desktop-codex.\$(date -u '+%Y%m%dT%H%M%SZ').bak"
cp -p "\$BUNDLED_BIN" "\$backup"
cp "\$SELECTOR" "\$BUNDLED_BIN"
chmod 0755 "\$BUNDLED_BIN"
log "restored Desktop helper to switchboard selector; backup: \$backup"

pids="\$(desktop_app_server_pids | tr '\n' ' ' | sed 's/[[:space:]]*\$//')"
if [[ -n "\$pids" ]]; then
  log "Desktop app-server is running (pid: \$pids); restart Codex Desktop to adopt the helper"
  osascript -e 'display notification "Codex Desktop helper was restored to the switchboard selector. Restart Codex Desktop to adopt the current target." with title "Codex Switchboard"' >/dev/null 2>&1 || true
fi
EOF
  chmod 0755 "$DESKTOP_ENSURE"
  record_artifact "desktop-ensure" "$DESKTOP_ENSURE" "" "switchboard"
}

install_desktop_launch_agent() {
  [[ "$NO_DESKTOP" -eq 1 ]] && return 0
  [[ "$(uname -s)" == "Darwin" ]] || return 0

  if [[ "$DRY_RUN" -eq 1 ]]; then
    log "[dry-run] write and bootstrap $DESKTOP_PLIST"
    return 0
  fi

  mkdir -p "$(dirname "$DESKTOP_PLIST")" "$LOG_DIR"
  cat >"$DESKTOP_PLIST" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>$DESKTOP_LABEL</string>
  <key>ProgramArguments</key>
  <array>
    <string>/bin/bash</string>
    <string>$DESKTOP_ENSURE</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>StartInterval</key>
  <integer>600</integer>
  <key>ThrottleInterval</key>
  <integer>60</integer>
  <key>WatchPaths</key>
  <array>
    <string>$DESKTOP_HELPER</string>
    <string>$DESKTOP_APP/Contents/Info.plist</string>
  </array>
  <key>StandardOutPath</key>
  <string>$LOG_DIR/desktop-agent.out.log</string>
  <key>StandardErrorPath</key>
  <string>$LOG_DIR/desktop-agent.err.log</string>
</dict>
</plist>
EOF
  plutil -lint "$DESKTOP_PLIST" >/dev/null
  launchctl bootout "$LAUNCHD_DOMAIN" "$DESKTOP_PLIST" >/dev/null 2>&1 || true
  launchctl bootstrap "$LAUNCHD_DOMAIN" "$DESKTOP_PLIST" >/dev/null 2>&1 || true
  record_artifact "launch-agent" "$DESKTOP_PLIST" "" "switchboard"
}

patch_desktop_helper() {
  [[ "$NO_DESKTOP" -eq 1 ]] && return 0
  [[ "$(uname -s)" == "Darwin" ]] || return 0
  [[ -x "$BIN_DIR/codex" ]] || die "selector missing: $BIN_DIR/codex"
  [[ -e "$DESKTOP_HELPER" ]] || {
    warn "Desktop helper not found: $DESKTOP_HELPER"
    return 0
  }

  local selector_hash bundled_hash backup
  selector_hash="$(sha256_file "$BIN_DIR/codex")"
  bundled_hash="$(sha256_file "$DESKTOP_HELPER")"
  if [[ "$selector_hash" == "$bundled_hash" ]]; then
    record_artifact "desktop-helper" "$DESKTOP_HELPER" "" "switchboard"
    return 0
  fi

  backup="$(backup_path_if_needed "$DESKTOP_HELPER" "desktop-codex")"
  run cp "$BIN_DIR/codex" "$DESKTOP_HELPER"
  run chmod 0755 "$DESKTOP_HELPER"
  record_artifact "desktop-helper" "$DESKTOP_HELPER" "$backup" "switchboard"
}

install_desktop() {
  [[ "$NO_DESKTOP" -eq 1 ]] && return 0
  generate_desktop_ensure
  patch_desktop_helper
  install_desktop_launch_agent
}

selected_target() {
  if [[ -f "$CURRENT_JSON" ]]; then
    python3 - "$CURRENT_JSON" <<'PY' 2>/dev/null || true
import json, sys
with open(sys.argv[1], "r", encoding="utf-8") as handle:
    print(json.load(handle).get("selected", ""))
PY
  fi
}

read_current_field() {
  local field="$1"
  [[ -f "$CURRENT_JSON" ]] || return 1
  python3 - "$CURRENT_JSON" "$field" <<'PY' 2>/dev/null
import json, sys
with open(sys.argv[1], "r", encoding="utf-8") as handle:
    data = json.load(handle)
print(data.get(sys.argv[2], ""))
PY
}

install_all() {
  local target="$1"
  require_python
  acquire_lock
  ensure_dirs
  write_manifest
  install_manager
  install_selector
  validate_target upstream
  validate_target rawr
  write_target_file upstream
  write_target_file rawr
  select_target "$target" "install"
}

select_target() {
  local target="$1" source="${2:-switch}"
  require_python
  [[ "$source" == "install" ]] || acquire_lock
  ensure_dirs
  validate_target "$target"
  install_manager
  install_selector
  snapshot_protected_homes "$target"
  write_current_json "$target"
  write_env_sh "$target"
  install_managed_shell_block
  wire_symlink "$LOCAL_CODEX" "local-codex"
  wire_symlink "$BUN_CODEX" "bun-codex"
  update_vscode_setting
  retire_conflicting_launch_agents
  write_env_launch_agent "$target"
  install_desktop
  restart_happy_if_running "$target"
  log "Selected Codex target: $target"
}

row() {
  local status="$1" surface="$2" detail="$3"
  printf '%-16s %-24s %s\n' "$status" "$surface" "$detail"
}

status_cmd() {
  require_python
  local target bin home updated epoch
  target="$(read_current_field selected || true)"
  bin="$(read_current_field bin || true)"
  home="$(read_current_field home || true)"
  updated="$(read_current_field updated_at || true)"
  epoch="$(read_current_field selected_at_epoch || true)"
  if [[ -z "$target" ]]; then
    die "switchboard is not installed or current.json is invalid: $CURRENT_JSON"
  fi

  printf 'Selected target: %s\n' "$target"
  printf 'Binary:          %s\n' "$bin"
  printf 'CODEX_HOME:      %s\n' "$home"
  printf 'Updated at:      %s\n' "$updated"
  printf 'Selected epoch:  %s\n' "$epoch"
  printf 'Selector:        %s\n' "$BIN_DIR/codex"
}

check_link_target() {
  local path="$1"
  if [[ -L "$path" && "$(readlink "$path")" == "$BIN_DIR/codex" ]]; then
    row "OK" "$path" "points to selector"
  elif [[ -e "$path" || -L "$path" ]]; then
    row "Wrong target" "$path" "$(ls -ld "$path" 2>/dev/null)"
  else
    row "Missing" "$path" "not present"
  fi
}

doctor_cmd() {
  require_python
  local target bin home selected_epoch selector_hash helper_hash cmd_path login_probe launch_home happy_pid happy_env desktop_pids
  target="$(read_current_field selected || true)"
  bin="$(read_current_field bin || true)"
  home="$(read_current_field home || true)"
  selected_epoch="$(read_current_field selected_at_epoch || true)"

  printf 'Codex Switchboard Doctor\n'
  printf 'Switch home: %s\n' "$SWITCH_HOME"
  printf 'Selected:    %s\n' "${target:-<none>}"
  printf '\n'
  printf '%-16s %-24s %s\n' "Status" "Surface" "Detail"
  printf '%-16s %-24s %s\n' "------" "-------" "------"

  if [[ -f "$CURRENT_JSON" && -n "$target" && -n "$bin" && -n "$home" ]]; then
    row "OK" "current.json" "$target -> $bin with CODEX_HOME=$home"
  else
    row "Missing" "current.json" "$CURRENT_JSON"
  fi

  if [[ -x "$BIN_DIR/codex" ]]; then
    row "OK" "selector" "$BIN_DIR/codex"
  else
    row "Missing" "selector" "$BIN_DIR/codex"
  fi

  if cmd_path="$(command -v codex 2>/dev/null)"; then
    current_home="${CODEX_HOME:-}"
    if [[ "$(real_path "$cmd_path")" == "$(real_path "$BIN_DIR/codex" 2>/dev/null || printf x)" ]]; then
      row "OK" "current shell" "$cmd_path selector; shell CODEX_HOME=${current_home:-<unset>}"
    elif [[ "$(real_path "$cmd_path")" == "$(real_path "$bin")" && "$current_home" == "$home" ]]; then
      row "OK" "current shell" "$cmd_path direct target; CODEX_HOME=$current_home"
    elif [[ "$(real_path "$cmd_path")" == "$(real_path "$bin")" ]]; then
      row "Wrong target" "current shell" "$cmd_path direct target but CODEX_HOME=${current_home:-<unset>}"
    else
      row "Wrong target" "current shell" "$cmd_path wins PATH"
    fi
  else
    row "Missing" "current shell" "codex not found"
  fi

  if command -v zsh >/dev/null 2>&1; then
    login_probe="$(zsh -ilc 'printf "%s|%s|%s\n" "$(command -v codex 2>/dev/null || true)" "${CODEX_HOME-}" "$(codex --version 2>/dev/null | head -n 1 || true)"' 2>/dev/null || true)"
    if [[ "$login_probe" == "$BIN_DIR/codex|"* || "$login_probe" == "$BIN_DIR/codex|$home|"* || "$login_probe" == "$bin|$home|"* ]]; then
      row "OK" "login shell" "$login_probe"
    else
      row "Wrong target" "login shell" "${login_probe:-<no probe output>}"
    fi
  fi

  check_link_target "$LOCAL_CODEX"
  check_link_target "$BUN_CODEX"

  if [[ -e "$VSCODE_SETTINGS" ]]; then
    local vscode_path
    vscode_path="$(python3 - "$VSCODE_SETTINGS" <<'PY' 2>/dev/null || true
import json, sys
import re
with open(sys.argv[1], "r", encoding="utf-8") as handle:
    text = handle.read()
try:
    print(json.loads(text).get("chatgpt.cliExecutable", ""))
except json.JSONDecodeError:
    match = re.search(r'(?m)^[ \t]*"chatgpt[.]cliExecutable"[ \t]*:[ \t]*"([^"]*)"', text)
    print(match.group(1) if match else "")
PY
)"
    if [[ "$vscode_path" == "$BIN_DIR/codex" ]]; then
      row "OK" "VS Code" "chatgpt.cliExecutable=$vscode_path"
    elif [[ -n "$vscode_path" ]]; then
      row "Wrong target" "VS Code" "chatgpt.cliExecutable=$vscode_path"
    else
      row "Manual" "VS Code" "setting absent or unreadable"
    fi
  else
    row "Missing" "VS Code" "settings file absent"
  fi

  if [[ "$(uname -s)" == "Darwin" && "$NO_LAUNCHD" -ne 1 ]]; then
    launch_home="$(launchctl getenv CODEX_HOME 2>/dev/null || true)"
    if [[ "$launch_home" == "$home" ]]; then
      row "OK" "launchctl env" "CODEX_HOME=$launch_home"
    else
      row "Wrong target" "launchctl env" "CODEX_HOME=${launch_home:-<unset>}"
    fi
  fi

  if happy_pid="$(pgrep -f 'happy-coder.*daemon' | head -n 1)"; [[ -n "${happy_pid:-}" ]]; then
    happy_env="$(ps eww -p "$happy_pid" 2>/dev/null | tr ' ' '\n' | grep -E '^(PATH|CODEX_HOME|CODEX_FORK_HOME|CODEX_SWITCHBOARD_TARGET)=' | tr '\n' ' ' || true)"
    if [[ "$happy_env" == *"CODEX_HOME=$home"* && "$happy_env" == *"$BIN_DIR"* && "$happy_env" == *"CODEX_SWITCHBOARD_TARGET=$target"* ]]; then
      row "OK" "Happy daemon" "pid=$happy_pid $happy_env"
    else
      row "Needs restart" "Happy daemon" "pid=$happy_pid $happy_env"
    fi
  else
    row "Missing" "Happy daemon" "not running"
  fi

  if [[ "$(uname -s)" == "Darwin" && -e "$DESKTOP_HELPER" ]]; then
    selector_hash="$(sha256_file "$BIN_DIR/codex")"
    helper_hash="$(sha256_file "$DESKTOP_HELPER")"
    if [[ "$selector_hash" == "$helper_hash" ]]; then
      row "OK" "Desktop helper" "$DESKTOP_HELPER matches selector"
    else
      row "Wrong target" "Desktop helper" "$DESKTOP_HELPER hash=$helper_hash selector=$selector_hash"
    fi

    if launchctl print "$LAUNCHD_DOMAIN/$DESKTOP_LABEL" >/dev/null 2>&1; then
      row "OK" "Desktop LaunchAgent" "$DESKTOP_LABEL loaded"
    else
      row "Missing" "Desktop LaunchAgent" "$DESKTOP_LABEL not loaded"
    fi

    desktop_pids="$(python3 - <<'PY' 2>/dev/null || true
import os
import subprocess

try:
    output = subprocess.check_output(["ps", "-axo", "pid=,args="], text=True)
except Exception:
    raise SystemExit(0)

for line in output.splitlines():
    parts = line.strip().split(None, 2)
    if len(parts) < 3:
        continue
    pid, argv0, rest = parts
    if os.path.basename(argv0) in {"codex", "codex-rawr-bin"} and rest.startswith("app-server"):
        print(pid)
PY
)"
    if [[ -n "$desktop_pids" ]]; then
      local desktop_ok=1 desktop_detail="" pid process_env process_cmd
      for pid in $desktop_pids; do
        process_env="$(ps eww -p "$pid" 2>/dev/null | tr ' ' '\n' | grep -E '^(CODEX_HOME|CODEX_SWITCHBOARD_TARGET|CODEX_SWITCHBOARD_SELECTED_AT)=' | tr '\n' ' ' || true)"
        process_cmd="$(ps -p "$pid" -o command= 2>/dev/null || true)"
        desktop_detail="${desktop_detail}pid=$pid ${process_env:-$process_cmd}; "
        if [[ "$process_env" != *"CODEX_HOME=$home"* || "$process_env" != *"CODEX_SWITCHBOARD_TARGET=$target"* ]]; then
          desktop_ok=0
        fi
        if [[ -n "$selected_epoch" && "$process_env" != *"CODEX_SWITCHBOARD_SELECTED_AT=$selected_epoch"* ]]; then
          desktop_ok=0
        fi
      done
      if [[ "$desktop_ok" -eq 1 ]]; then
        row "OK" "Desktop app-server" "$desktop_detail"
      else
        row "Needs restart" "Desktop app-server" "$desktop_detail"
      fi
    else
      row "Missing" "Desktop app-server" "not running"
    fi
  fi

  if [[ -e "$bin" ]]; then
    row "OK" "target install" "$bin $(version_of "$bin")"
  else
    row "Missing" "target install" "$bin"
  fi

  for stale in "$HOME/.cargo/bin/codex" "$HOME/.bun/install/global/node_modules/@openai/codex/bin/codex.js" "$HOME/.volta/bin/codex" "/opt/homebrew/bin/codex" "/usr/local/bin/codex"; do
    [[ -e "$stale" ]] || continue
    if [[ "$stale" == "$bin" ]]; then
      continue
    else
      row "Stale install" "extra codex" "$stale $(version_of "$stale")"
    fi
  done

  check_config_home "$UPSTREAM_HOME" "upstream config" "$RAWR_HOME" "upstream"
  check_config_home "$RAWR_HOME" "RAWR config" "$UPSTREAM_HOME" "rawr"
  check_shell_codex_home_exports

  [[ -d "$UPSTREAM_HOME" ]] && row "OK" "upstream home" "$UPSTREAM_HOME" || row "Missing" "upstream home" "$UPSTREAM_HOME"
  [[ -d "$RAWR_HOME" ]] && row "OK" "RAWR home" "$RAWR_HOME" || row "Missing" "RAWR home" "$RAWR_HOME"
}

check_config_home() {
  local check_home surface other_home kind config report
  check_home="$1"
  surface="$2"
  other_home="$3"
  kind="$4"
  config="$check_home/config.toml"
  if [[ ! -f "$config" ]]; then
    row "Missing" "$surface" "$config"
    return 0
  fi

  report="$(python3 - "$config" "$check_home" "$other_home" "$kind" <<'PY' 2>/dev/null || true
import pathlib
import re
import sys
import tomllib

path = pathlib.Path(sys.argv[1])
home, other_home, kind = sys.argv[2:]
text = path.read_text(encoding="utf-8")
issues = []
try:
    data = tomllib.loads(text)
except Exception as exc:
    print(f"Invalid TOML: {exc}")
    raise SystemExit(0)

if path.stat().st_size < 1500:
    issues.append(f"small config ({path.stat().st_size} bytes)")
if other_home and re.search(re.escape(other_home) + r"(?=/|$)", text):
    issues.append(f"contains other CODEX_HOME path {other_home}")
if kind == "upstream" and ("rawr_auto_compaction" in text or "remote_compaction" in data.get("features", {})):
    issues.append("contains RAWR-only config")
for section in ("projects", "mcp_servers", "features"):
    if not data.get(section):
        issues.append(f"missing [{section}]")
print("; ".join(issues) if issues else "OK")
PY
)"
  if [[ "$report" == "OK" ]]; then
    row "OK" "$surface" "$config"
  else
    row "Needs review" "$surface" "${report:-unreadable config}: $config"
  fi
}

check_shell_codex_home_exports() {
  local files=("$HOME/.zshenv" "$HOME/.zprofile" "$HOME/.zshrc" "$HOME/.profile" "$HOME/.bashrc")
  local matches="" file
  for file in "${files[@]}"; do
    [[ -f "$file" ]] || continue
    if grep -Eq '^[[:space:]]*(export[[:space:]]+)?CODEX_HOME=' "$file"; then
      matches="${matches}${file} "
    fi
  done
  if [[ -n "$matches" ]]; then
    row "Manual" "shell CODEX_HOME" "direct CODEX_HOME exports present in: $matches"
  else
    row "OK" "shell CODEX_HOME" "no direct CODEX_HOME exports in common shell startup files"
  fi
}

uninstall_cmd() {
  require_python
  acquire_lock

  if [[ "$DRY_RUN" -eq 1 ]]; then
    log "[dry-run] uninstall switchboard-owned artifacts from $MANIFEST_JSON"
    return 0
  fi

  if [[ "${CODEX_SWITCHBOARD_UNINSTALL_CONFIRM:-}" != "1" ]]; then
    die "uninstall is guarded until backup provenance is verified; run with --dry-run first, then set CODEX_SWITCHBOARD_UNINSTALL_CONFIRM=1 to proceed"
  fi

  if [[ -f "$MANIFEST_JSON" ]]; then
    python3 - "$MANIFEST_JSON" <<'PY'
import json
import os
import shutil
import subprocess
import sys

manifest = sys.argv[1]
with open(manifest, "r", encoding="utf-8") as handle:
    data = json.load(handle)

for item in reversed(data.get("managed_artifacts", [])):
    path = item.get("path")
    backup = item.get("first_backup") or item.get("backup")
    kind = item.get("kind")
    ownership = item.get("ownership")
    if not path:
        continue
    if kind == "launch-agent" and os.path.exists(path):
        subprocess.run(["launchctl", "bootout", f"gui/{os.getuid()}", path], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if backup and (os.path.exists(backup) or os.path.islink(backup)):
        if os.path.exists(path) or os.path.islink(path):
            if os.path.isdir(path) and not os.path.islink(path):
                shutil.rmtree(path)
            else:
                os.remove(path)
        parent = os.path.dirname(path)
        if parent:
            os.makedirs(parent, exist_ok=True)
        if os.path.islink(backup):
            os.symlink(os.readlink(backup), path)
        elif os.path.isdir(backup):
            shutil.copytree(backup, path)
        else:
            shutil.copy2(backup, path)
        print(f"restored {path} from {backup}")
    elif ownership == "switchboard":
        if os.path.exists(path) or os.path.islink(path):
            if os.path.isdir(path) and not os.path.islink(path):
                shutil.rmtree(path)
            else:
                os.remove(path)
            print(f"removed {path}")
    else:
        print(f"left {path}; no backup recorded")
PY
  fi

  if [[ "$(uname -s)" == "Darwin" ]]; then
    launchctl unsetenv CODEX_SWITCHBOARD_TARGET >/dev/null 2>&1 || true
    launchctl unsetenv CODEX_SWITCH_HOME >/dev/null 2>&1 || true
  fi

  log "Kept switchboard state/logs unless removed above: $SWITCH_HOME $LOG_DIR"
}

parse_common_options() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --dry-run) DRY_RUN=1; shift ;;
      --no-restart) NO_RESTART=1; shift ;;
      --no-desktop) NO_DESKTOP=1; shift ;;
      --no-user-wiring) NO_USER_WIRING=1; shift ;;
      --no-vscode) NO_VSCODE=1; shift ;;
      --no-launchd) NO_LAUNCHD=1; shift ;;
      --strict) STRICT=1; shift ;;
      --target)
        [[ $# -ge 2 ]] || die "--target requires upstream or rawr"
        INITIAL_TARGET="$2"
        shift 2
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        die "unknown option: $1"
        ;;
    esac
  done
}

main() {
  local command="${1:-}"
  [[ -n "$command" ]] || {
    usage >&2
    exit 2
  }
  shift || true

  local INITIAL_TARGET=""
  parse_common_options "$@"

  case "$command" in
    install)
      if [[ -z "$INITIAL_TARGET" ]]; then
        INITIAL_TARGET="$(selected_target)"
        [[ -n "$INITIAL_TARGET" ]] || INITIAL_TARGET="upstream"
      fi
      install_all "$INITIAL_TARGET"
      ;;
    upstream|rawr)
      select_target "$command"
      ;;
    status)
      status_cmd
      ;;
    doctor)
      doctor_cmd
      ;;
    desktop-install)
      require_python
      acquire_lock
      install_manager
      install_selector
      generate_desktop_ensure
      patch_desktop_helper
      install_desktop_launch_agent
      ;;
    uninstall)
      uninstall_cmd
      ;;
    -h|--help)
      usage
      ;;
    *)
      die "unknown command: $command"
      ;;
  esac
}

main "$@"
