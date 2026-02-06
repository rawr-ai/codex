#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

usage() {
  cat <<'EOF'
Usage: rawr/bump-fork-version.sh [--apply] [--commit]

Computes the fork version as: (latest upstream rust-v tag) + 1 minor
and updates codex-rs/Cargo.toml [workspace.package] version accordingly.

Flags:
  --apply     Update codex-rs/Cargo.toml in-place (default: dry-run)
  --commit    Commit the version bump on the current branch (implies --apply)
EOF
}

apply=0
commit=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --apply) apply=1; shift ;;
    --commit) apply=1; commit=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown arg: $1" >&2; usage >&2; exit 2 ;;
  esac
done

git fetch --tags upstream >/dev/null 2>&1 || true

latest_tag=""
latest_version=""

while IFS= read -r tag; do
  if [[ "$tag" =~ ^rust-v([0-9]+)\.([0-9]+)\.([0-9]+)(-.+)?$ ]]; then
    latest_tag="$tag"
    latest_version="${BASH_REMATCH[1]}.${BASH_REMATCH[2]}.${BASH_REMATCH[3]}${BASH_REMATCH[4]:-}"
    break
  fi
done < <(git tag --list 'rust-v*' --sort=-version:refname)

if [[ -z "$latest_tag" || -z "$latest_version" ]]; then
  echo "Failed to find a valid upstream rust-v<semver> tag." >&2
  exit 1
fi

if [[ ! "$latest_version" =~ ^([0-9]+)\.([0-9]+)\.([0-9]+)(-.+)?$ ]]; then
  echo "Unexpected upstream version format: $latest_version" >&2
  exit 1
fi

major="${BASH_REMATCH[1]}"
minor="${BASH_REMATCH[2]}"
suffix="${BASH_REMATCH[4]:-}"

fork_minor=$((minor + 1))
fork_version="${major}.${fork_minor}.0${suffix}"

current_version="$(python3 - <<'PY'
import re
from pathlib import Path

text = Path("codex-rs/Cargo.toml").read_text(encoding="utf-8")
m = re.search(r'(?m)^\[workspace\.package\]\n(?:.*\n)*?^version\s*=\s*"([^"]+)"\s*$', text)
print(m.group(1) if m else "")
PY
)"

echo "Upstream latest tag: ${latest_tag}"
echo "Upstream version:    ${latest_version}"
echo "Fork version:        ${fork_version}"
echo "Current version:     ${current_version:-<missing>}"

if [[ "$apply" -ne 1 ]]; then
  exit 0
fi

python3 - <<PY
import re
from pathlib import Path

path = Path("codex-rs/Cargo.toml")
text = path.read_text(encoding="utf-8")

def repl(match: re.Match[str]) -> str:
    block = match.group(0)
    block = re.sub(
        r'(?m)^version\s*=\s*"[^"]+"\s*$',
        'version = \"${fork_version}\"',
        block,
        count=1,
    )
    return block

new_text, n = re.subn(
    r'(?ms)^\[workspace\.package\].*?(?=^\[|\Z)',
    repl,
    text,
    count=1,
)
if n != 1:
    raise SystemExit("Failed to update [workspace.package] version in codex-rs/Cargo.toml")
path.write_text(new_text, encoding="utf-8")
PY

if [[ "$commit" -eq 1 ]]; then
  git add codex-rs/Cargo.toml
  git commit -m "rawr: bump fork version to ${fork_version}"
fi
