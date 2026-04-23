#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

tag_name=""

usage() {
  cat <<'EOF'
Usage: rawr/release-local.sh [--tag TAG]

Golden-path local release for the fork:
- bumps fork version
- builds + installs codex-rawr
- links codex wrapper + bun symlink
- restarts Happy
- (optional) tags the repo for traceability

Options:
  --tag TAG    Create an annotated tag after a successful release.
  -h, --help   Show help.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --tag)
      tag_name="${2:-}"
      if [[ -z "$tag_name" ]]; then
        echo "Missing tag name for --tag" >&2
        exit 2
      fi
      shift 2
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

if [[ -n "$tag_name" ]]; then
  if [[ -n "$(git status --porcelain)" ]]; then
    echo "Refusing to tag with a dirty working tree." >&2
    echo "Commit or stash changes, then rerun." >&2
    exit 1
  fi
fi

bash "$ROOT/publish-local.sh" --happy --force

if [[ -n "$tag_name" ]]; then
  git tag -a "$tag_name" -m "rawr local release ${tag_name}"
  echo "Created tag: $tag_name"
fi
