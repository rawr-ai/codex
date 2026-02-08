# Repo Workflows (Agents + Humans)

This repo is on macOS a lot, and macOS filesystems are commonly case-insensitive: `AGENTS.md` and `agents.md` collide. So the Codex instruction router lives at `../AGENTS.md`, and this human-friendly workflows doc lives at `docs/agents.md`.

## Non-Negotiable Rule: How Work Starts Here

Any fix or work in this repo MUST:
- Use Graphite (`gt`) for branching/commits/submission.
- Start new stacks from `codex/integration-upstream-main` using `gt create`.
- Be based on the integration upstream main trunk (do not start new unrelated work from an existing PR branch).
  - Exception: only stack on an existing PR branch when the change is tightly coupled and intended to merge together.

Preflight (do this before you write code):
```bash
git status --porcelain
gt branch info --no-interactive
```

Canonical branch start:
```bash
gt checkout codex/integration-upstream-main
gt create codex/<topic>
```

## Setup / Workflow Pathways (Pointer-First)

This file intentionally does not duplicate runbooks. It provides the correct command entrypoints and routes you to canonical docs/scripts.

### 1) Run the rawr fork locally (side-by-side)
- Install/switch locally:
  - `bash rawr/install-local.sh`
- Canonical instructions:
  - `rawr/INSTALL.md`

### 2) Publish locally (PATH switching)
- Recommended:
  - `bash rawr/publish-local.sh`
- Canonical instructions:
  - `rawr/INSTALL.md`
- Script is executable truth:
  - `rawr/publish-local.sh`

### 3) Golden-path local release
- Release local:
  - `bash rawr/release-local.sh`
- Canonical instructions:
  - `rawr/INSTALL.md`
- Script is executable truth:
  - `rawr/release-local.sh`

### 4) Upstream update / rebase checkpoint
- Preferred checkpoint automation:
  - `bash rawr/rebase-daily.sh`
- Fallback/manual checkpoint flow:
  - `DRY_RUN=1 rawr/sync-upstream.sh codex/integration-upstream-main`
  - `rawr/sync-upstream.sh codex/integration-upstream-main`

Canonical runbooks (read these; don’t freestyle rebases):
- `rawr/UPDATING.md`
- `docs/projects/rawr/rebase-runbook.md`
- `docs/projects/rawr/rebase-gotchas.md`

### 5) Rust development (codex-rs)
- Install/dev setup:
  - `docs/install.md`
- Repo tasks:
  - `justfile`
- Rust-specific operational rules (tests/clippy/docs/schema):
  - `codex-rs/AGENTS.md`

## PR / Submission Workflow (Graphite)

Use Graphite for commits and PR updates:
- Commit/update:
  - `gt modify -c -a -m "<type>: <summary>"`
- Submit:
  - `gt submit --ai`

Avoid raw `git push --force` unless a specific runbook explicitly tells you to.

## Recovery: Started Work On The Wrong Branch

High-level guardrail:
- If `gt branch info` shows you’re on a branch with an unrelated PR attached, stop and restart from `codex/integration-upstream-main` (or explicitly stack only if tightly coupled).

If you already have edits:
- Keep the repo clean at the end of the fix (no dangling local changes).
- Follow the relevant runbook/scripts for the workflow you were attempting rather than inventing a new one.

