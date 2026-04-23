# rawr fork workflows

## Scope
- Applies to `rawr/**`.

## Parent Contract
- Start from `../AGENTS.md` and override only where rawr workflows differ.

## Local Invariants
- The Graphite operational trunk is `codex/integration-upstream-main`. New stacks must start there via `gt create`.
- Upstream sync happens only at controlled checkpoints on `codex/integration-upstream-main`.
- Prefer the scripts in `rawr/` and the canonical runbooks; do not freestyle rebase/publish workflows.

## Local Process Rules
- Local install: `bash rawr/install-local.sh`
- Local publish (recommended): `bash rawr/publish-local.sh`
- Golden-path local release: `bash rawr/release-local.sh`
- Upstream checkpoint (preferred): `bash rawr/rebase-daily.sh`
- Upstream checkpoint (fallback/manual):
  - `DRY_RUN=1 rawr/sync-upstream.sh codex/integration-upstream-main`
  - `rawr/sync-upstream.sh codex/integration-upstream-main`

## Local Routing (Canonical)
- Install + local switching: `rawr/INSTALL.md`
- Upstream updates model: `rawr/UPDATING.md`
- Checkpoint rebase runbook: `docs/projects/rawr/rebase-runbook.md`
- Rebase gotchas/checklist: `docs/projects/rawr/rebase-gotchas.md`
- Scripts (executable truth):
  - `rawr/install-local.sh`
  - `rawr/publish-local.sh`
  - `rawr/release-local.sh`
  - `rawr/rebase-daily.sh`
  - `rawr/sync-upstream.sh`

## Divergence Rationale
This file exists because `rawr/**` has fork-specific operating rules and high-stakes workflows (release/publish/rebase) that must be routed to canonical runbooks and scripts.

