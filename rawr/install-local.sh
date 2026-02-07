#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT/codex-rs"

cargo build -p codex-cli --release

mkdir -p "$HOME/.local/bin"
install -m 0755 "$ROOT/codex-rs/target/release/codex" "$HOME/.local/bin/codex-rawr-bin"

if [[ "$(uname -s)" == "Darwin" ]]; then
  CODEX_RAWR_CODESIGN="${CODEX_RAWR_CODESIGN:-1}"
  CODEX_RAWR_CODESIGN_IDENTIFIER="${CODEX_RAWR_CODESIGN_IDENTIFIER:-com.rawr-ai.codex.rawr-cli}"
  CODEX_RAWR_CODESIGN_IDENTITY="${CODEX_RAWR_CODESIGN_IDENTITY:-}"

  if [[ "$CODEX_RAWR_CODESIGN" == "0" ]]; then
    echo "Skipping codesign (CODEX_RAWR_CODESIGN=0)."
  elif ! command -v codesign >/dev/null 2>&1 || ! command -v security >/dev/null 2>&1; then
    echo "warning: missing macOS signing tools (codesign/security). Keychain prompts may repeat."
  else
    pick_identity() {
      if [[ -n "$CODEX_RAWR_CODESIGN_IDENTITY" ]]; then
        echo "$CODEX_RAWR_CODESIGN_IDENTITY"
        return 0
      fi

      local identities
      # NOTE: We intentionally do NOT pass -v here. `security find-identity -v` only lists
      # identities that are trusted by the system. Self-signed/local identities are often
      # untrusted (e.g. CSSMERR_TP_NOT_TRUSTED) but still usable for codesign, and they
      # are sufficient to give the binary a stable identity for Keychain ACL persistence.
      identities="$(security find-identity -p codesigning 2>/dev/null || true)"

      local id
      id="$(printf '%s\n' "$identities" | sed -n 's/^.*"\\(Developer ID Application:.*\\)".*$/\\1/p' | head -n 1)"
      if [[ -n "$id" ]]; then
        echo "$id"
        return 0
      fi

      id="$(printf '%s\n' "$identities" | sed -n 's/^.*"\\(Apple Development:.*\\)".*$/\\1/p' | head -n 1)"
      if [[ -n "$id" ]]; then
        echo "$id"
        return 0
      fi

      id="$(printf '%s\n' "$identities" | sed -n 's/^.*"\\(Codex Rawr Local Codesigning\\)".*$/\\1/p' | head -n 1)"
      if [[ -n "$id" ]]; then
        echo "$id"
        return 0
      fi

      return 1
    }

    if identity="$(pick_identity)"; then
      echo "Codesigning codex-rawr-bin with identity: $identity"
      if ! codesign \
        --force \
        --sign "$identity" \
        --identifier "$CODEX_RAWR_CODESIGN_IDENTIFIER" \
        "$HOME/.local/bin/codex-rawr-bin"; then
        echo "warning: codesign failed; Keychain prompts may repeat."
      fi

      codesign -dv --verbose=4 "$HOME/.local/bin/codex-rawr-bin" 2>&1 || true
      codesign --verify --strict --verbose=2 "$HOME/.local/bin/codex-rawr-bin" 2>&1 || true
    else
      echo "warning: no macOS code signing identity found. Keychain prompts for MCP OAuth may repeat."
      echo
      echo "To fix this, install a signing identity (preferred):"
      echo "  - Developer ID Application: <Org> (<TEAMID>)"
      echo "  - Apple Development: <Name> (<TEAMID>)"
      echo
      echo "Or create a local self-signed Code Signing cert (works for local dev):"
      echo "  1. Open Keychain Access"
      echo "  2. Certificate Assistant -> Create a Certificate..."
      echo "  3. Name: Codex Rawr Local Codesigning"
      echo "  4. Certificate Type: Code Signing; Identity Type: Self Signed Root"
      echo
      echo "Then re-run: rawr/install-local.sh"
      echo
      echo "Advanced:"
      echo "  - Set CODEX_RAWR_CODESIGN_IDENTITY to pick a specific identity"
      echo "  - Set CODEX_RAWR_CODESIGN_IDENTIFIER to override the signing identifier"
      echo "  - Set CODEX_RAWR_CODESIGN=0 to opt out"
    fi
  fi
fi

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
