#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT/codex-rs"

cargo build -p codex-cli --release

mkdir -p "$HOME/.local/bin"
install -m 0755 "$ROOT/codex-rs/target/release/codex" "$HOME/.local/bin/codex-rawr"

echo "Installed: $HOME/.local/bin/codex-rawr"
echo "Default CODEX_HOME for this fork is ~/.codex-rawr (unless you override CODEX_HOME)."
