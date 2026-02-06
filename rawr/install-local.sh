#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT/codex-rs"

cargo build -p codex-cli --release

mkdir -p "$HOME/.local/bin"
install -m 0755 "$ROOT/codex-rs/target/release/codex" "$HOME/.local/bin/codex-rawr-bin"

cat >"$HOME/.local/bin/codex-rawr" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

# Fork isolation defaults: preserve upstream resolver semantics in core while
# ensuring the fork runs in a separate state dir unless explicitly overridden.
export CODEX_HOME="${CODEX_HOME:-$HOME/.codex-rawr}"

exec "$HOME/.local/bin/codex-rawr-bin" "$@"
EOF

chmod 0755 "$HOME/.local/bin/codex-rawr"

echo "Installed: $HOME/.local/bin/codex-rawr"
echo "Installed: $HOME/.local/bin/codex-rawr-bin"
echo "Wrapper default CODEX_HOME for this fork: ~/.codex-rawr (unless you override CODEX_HOME)."
