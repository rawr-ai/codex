#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

git fetch upstream

# Keep origin/main as an upstream mirror.
git checkout main
git pull --ff-only upstream main
git push origin main

# Rebase our patch series.
git checkout rawr/main
git rebase upstream/main

# Keep the fork semver ahead of upstream so external tooling can feature-detect
# against `codex --version` (e.g. MCP server launch behavior).
bash rawr/bump-fork-version.sh --commit

git push --force-with-lease origin rawr/main

echo "Synced upstream -> main and rebased rawr/main."
