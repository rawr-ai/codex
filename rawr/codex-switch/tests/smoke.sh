#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

make_fake_codex() {
  local path="$1" name="$2"
  cat >"$path" <<EOF
#!/usr/bin/env bash
set -euo pipefail
if [[ "\${1:-}" == "--version" ]]; then
  echo "codex-cli $name"
  exit 0
fi
printf '%s|%s|%s|%s\n' "$name" "\${CODEX_HOME-}" "\${CODEX_FORK_HOME-}" "\${CODEX_SWITCHBOARD_TARGET-}"
EOF
  chmod 0755 "$path"
}

mkdir -p "$TMP_DIR/bin" "$TMP_DIR/upstream-home" "$TMP_DIR/rawr-home"
make_fake_codex "$TMP_DIR/bin/upstream-codex" "upstream-test"
make_fake_codex "$TMP_DIR/bin/rawr-codex" "rawr-test"
printf 'model = "gpt-5.5"\n[features]\nunified_exec = true\n' >"$TMP_DIR/upstream-home/config.toml"
printf 'model = "gpt-5.5"\n[features]\nunified_exec = true\nrawr_auto_compaction = true\n' >"$TMP_DIR/rawr-home/config.toml"
printf '{}\n' >"$TMP_DIR/upstream-home/.codex-global-state.json"
printf '{}\n' >"$TMP_DIR/rawr-home/.codex-global-state.json"
printf '{}\n' >"$TMP_DIR/upstream-home/auth.json"
printf '{}\n' >"$TMP_DIR/rawr-home/auth.json"
touch "$TMP_DIR/upstream-home/session_index.jsonl" "$TMP_DIR/rawr-home/session_index.jsonl"

common_env=(
  CODEX_SWITCH_HOME="$TMP_DIR/switch"
  CODEX_SWITCH_UPSTREAM_BIN="$TMP_DIR/bin/upstream-codex"
  CODEX_SWITCH_RAWR_BIN="$TMP_DIR/bin/rawr-codex"
  CODEX_SWITCH_UPSTREAM_HOME="$TMP_DIR/upstream-home"
  CODEX_SWITCH_RAWR_HOME="$TMP_DIR/rawr-home"
)

env "${common_env[@]}" bash "$ROOT/rawr/codex-switch/codex-use.sh" install \
  --target upstream \
  --no-user-wiring \
  --no-vscode \
  --no-launchd \
  --no-desktop \
  --no-restart >/dev/null

out="$("$TMP_DIR/switch/bin/codex")"
[[ "$out" == "upstream-test|$TMP_DIR/upstream-home||upstream" ]] || {
  echo "unexpected upstream selector output: $out" >&2
  exit 1
}

env "${common_env[@]}" bash "$ROOT/rawr/codex-switch/codex-use.sh" rawr \
  --no-user-wiring \
  --no-vscode \
  --no-launchd \
  --no-desktop \
  --no-restart >/dev/null

out="$("$TMP_DIR/switch/bin/codex")"
[[ "$out" == "rawr-test|$TMP_DIR/rawr-home|$TMP_DIR/rawr-home|rawr" ]] || {
  echo "unexpected rawr selector output: $out" >&2
  exit 1
}

env "${common_env[@]}" bash "$ROOT/rawr/codex-switch/codex-use.sh" status >/tmp/codex-switch-status.out
grep -q 'Selected target: rawr' /tmp/codex-switch-status.out

env "${common_env[@]}" bash "$ROOT/rawr/codex-switch/codex-use.sh" doctor \
  --no-launchd \
  --no-desktop >/tmp/codex-switch-doctor.out
grep -q 'current.json' /tmp/codex-switch-doctor.out
find "$TMP_DIR/switch/state/protected-home-snapshots" -name config.toml | grep -q .

if env "${common_env[@]}" bash "$ROOT/rawr/codex-switch/codex-use.sh" uninstall \
  --no-launchd \
  --no-desktop >/tmp/codex-switch-uninstall.out 2>/tmp/codex-switch-uninstall.err; then
  echo "uninstall unexpectedly succeeded without explicit confirmation" >&2
  exit 1
fi
grep -q 'uninstall is guarded' /tmp/codex-switch-uninstall.err

printf '{not-json' >"$TMP_DIR/switch/current.json"
if "$TMP_DIR/switch/bin/codex" >/tmp/codex-switch-smoke.out 2>/tmp/codex-switch-smoke.err; then
  echo "selector unexpectedly succeeded with malformed current.json" >&2
  exit 1
fi

echo "codex-switch smoke passed"
