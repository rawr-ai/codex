#!/usr/bin/env bash
set -euo pipefail

selector_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
if [[ "$(basename "$selector_dir")" == "bin" && -f "$selector_dir/../current.json" ]]; then
  SWITCH_HOME="$(cd "$selector_dir/.." && pwd -P)"
elif [[ -n "${CODEX_SWITCH_HOME:-}" ]]; then
  SWITCH_HOME="$CODEX_SWITCH_HOME"
else
  SWITCH_HOME="$HOME/.codex-switch"
fi
CURRENT_JSON="${CODEX_SWITCH_CURRENT_JSON:-$SWITCH_HOME/current.json}"

die() {
  printf 'codex-switch selector: %s\n' "$*" >&2
  exit 78
}

json_get() {
  local key="$1"

  if command -v plutil >/dev/null 2>&1; then
    plutil -extract "$key" raw -o - "$CURRENT_JSON" 2>/dev/null && return 0
  fi

  if command -v python3 >/dev/null 2>&1; then
    python3 - "$CURRENT_JSON" "$key" <<'PY'
import json
import sys

path, key = sys.argv[1], sys.argv[2]
with open(path, "r", encoding="utf-8") as handle:
    data = json.load(handle)
value = data
for part in key.split("."):
    value = value[part]
print(value)
PY
    return 0
  fi

  if command -v node >/dev/null 2>&1; then
    node -e 'const fs=require("fs"); const data=JSON.parse(fs.readFileSync(process.argv[1],"utf8")); const value=process.argv[2].split(".").reduce((acc,k)=>acc[k], data); console.log(value);' "$CURRENT_JSON" "$key"
    return 0
  fi

  die "cannot parse $CURRENT_JSON; install plutil, python3, or node"
}

real_path() {
  local path="$1"
  if command -v python3 >/dev/null 2>&1; then
    python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$path"
  else
    cd "$(dirname "$path")" >/dev/null 2>&1 && printf '%s/%s\n' "$(pwd -P)" "$(basename "$path")"
  fi
}

[[ -f "$CURRENT_JSON" ]] || die "missing selected target state: $CURRENT_JSON"

schema="$(json_get schema)" || die "invalid selected target state: $CURRENT_JSON"
selected="$(json_get selected)" || die "missing selected target"
target_bin="$(json_get bin)" || die "missing selected target binary"
target_home="$(json_get home)" || die "missing selected target home"
selected_at_epoch="$(json_get selected_at_epoch 2>/dev/null || true)"

[[ "$schema" == "1" ]] || die "unsupported state schema: $schema"
[[ "$selected" == "upstream" || "$selected" == "rawr" ]] || die "unsupported target: $selected"
[[ "$target_bin" = /* ]] || die "selected binary must be absolute: $target_bin"
[[ "$target_home" = /* ]] || die "selected CODEX_HOME must be absolute: $target_home"
[[ -x "$target_bin" ]] || die "selected binary is missing or not executable: $target_bin"
[[ -d "$target_home" ]] || die "selected CODEX_HOME is missing: $target_home"

selector_real="$(real_path "${BASH_SOURCE[0]}")"
target_real="$(real_path "$target_bin")"
[[ "$selector_real" != "$target_real" ]] || die "selected binary resolves to the selector itself"

export CODEX_HOME="$target_home"
export CODEX_SWITCHBOARD_TARGET="$selected"
export CODEX_SWITCHBOARD_SELECTED_AT="${selected_at_epoch:-}"
export CODEX_SWITCHBOARD_HOME="$SWITCH_HOME"

if [[ "$0" == "/Applications/"*"/Codex.app/Contents/Resources/codex" ]]; then
  export CODEX_SWITCHBOARD_HELPER="$0"
fi

if [[ "$selected" == "rawr" ]]; then
  export CODEX_FORK_HOME="$target_home"
else
  unset CODEX_FORK_HOME
fi

exec "$target_bin" "$@"
