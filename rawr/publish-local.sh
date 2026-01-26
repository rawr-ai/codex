#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

link_codex=0
link_bun=0
restart_happy=0
force=0

usage() {
  cat <<'EOF'
Usage: rawr/publish-local.sh [OPTIONS]

Builds and installs the fork binary as ~/.local/bin/codex-rawr.

Options:
  --link-codex         Install/overwrite ~/.local/bin/codex wrapper that runs codex-rawr.
  --link-bun           Symlink ~/.bun/bin/codex -> ~/.local/bin/codex (helps Happy Coder resolve it).
  --happy              Equivalent to: --link-codex --link-bun --restart-happy
  --restart-happy      Restart Happy daemon (happy daemon stop; happy daemon start).
  --force              Overwrite existing ~/.local/bin/codex without prompting.
  -h, --help           Show help.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --link-codex)
      link_codex=1
      shift
      ;;
    --link-bun)
      link_bun=1
      shift
      ;;
    --restart-happy)
      restart_happy=1
      shift
      ;;
    --happy)
      link_codex=1
      link_bun=1
      restart_happy=1
      shift
      ;;
    --force)
      force=1
      shift
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

"$ROOT/install-local.sh"

if [[ "$link_codex" -eq 1 ]]; then
  mkdir -p "$HOME/.local/bin"

  codex_path="$HOME/.local/bin/codex"
  if [[ -e "$codex_path" && "$force" -ne 1 ]]; then
    echo "Refusing to overwrite existing $codex_path without --force" >&2
    echo "Tip: run with --force, or omit --link-codex to keep upstream codex untouched." >&2
    exit 1
  fi

  cat >"$codex_path" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

# rawr defaults
export CODEX_HOME="${CODEX_HOME:-$HOME/.codex-rawr}"

# rawr watcher defaults: full auto + agent-authored continuation packet
export RAWR_AUTO_COMPACTION_MODE="${RAWR_AUTO_COMPACTION_MODE:-auto}"
export RAWR_AUTO_COMPACTION_PACKET_AUTHOR="${RAWR_AUTO_COMPACTION_PACKET_AUTHOR:-agent}"

exec "$HOME/.local/bin/codex-rawr" "$@"
EOF

  chmod 0755 "$codex_path"
  echo "Installed wrapper: $codex_path"
fi

if [[ "$link_bun" -eq 1 ]]; then
  mkdir -p "$HOME/.bun/bin"
  ln -sf "$HOME/.local/bin/codex" "$HOME/.bun/bin/codex"
  echo "Symlinked: $HOME/.bun/bin/codex -> $HOME/.local/bin/codex"
fi

if [[ "$restart_happy" -eq 1 ]]; then
  if command -v happy >/dev/null 2>&1; then
    happy daemon stop || true
    happy daemon start
    echo "Restarted Happy daemon"
  else
    echo "Skipped restarting Happy daemon (happy not found in PATH)" >&2
  fi
fi

