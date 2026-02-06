# Rebase Gotchas and Checklist

Status: operational notes
Date: 2026-02-06

## Frequent traps
1. Treating `main` as the day-to-day stack base instead of `codex/integration-upstream-main`.
2. Running upstream sync from an implicit/current branch rather than explicitly targeting the integration trunk.
3. Rebasing child branches directly onto `upstream/main` instead of restacking from the integration trunk.
4. Running broad Graphite operations that restack unrelated stacks.
5. Mixing architecture seam changes with policy behavior changes in one commit.

## Fork-specific gotchas
1. Home-dir behavior can drift across crates if resolver policy is inconsistent.
2. Internal judgment calls should not inherit user web-search eligibility.
3. Compaction-trigger state can go stale if never consumed; clean at turn start.

## Canonical checkpoint drill (this cycle)
- Verify active tracked chain in Graphite:
  - `gt ls` should show `codex/integration-upstream-main -> codex/incremental-rebase-2026-02-06`.
- Rehearse the checkpoint sync explicitly against integration trunk:
  - `DRY_RUN=1 rawr/sync-upstream.sh codex/integration-upstream-main`
- If dry-run is correct, execute the real checkpoint and then restack descendants:
  - `rawr/sync-upstream.sh codex/integration-upstream-main`
  - `git checkout codex/incremental-rebase-2026-02-06`
  - `gt sync --no-restack`
  - `gt restack --upstack`

## Why this prevents recurrence
- The canonical parent branch is explicit and stable.
- Checkpoint-only sync constrains where history rewriting can happen.
- Required post-checkpoint restack prevents long-lived child branches from drifting onto stale bases.

## Recurring checklist
- [ ] clean tree
- [ ] remotes fetched/pruned
- [ ] `gt ls` confirms integration-trunk parentage
- [ ] integration trunk selected explicitly for checkpoint sync
- [ ] checkpoint rebase completed and conflicts resolved semantically
- [ ] descendants restacked (`gt sync --no-restack` + `gt restack --upstack`)
- [ ] `just fmt` executed (if Rust touched)
- [ ] crate-scoped tests passed
- [ ] full-suite gate decision recorded
- [ ] lease-protected push used
- [ ] post-push CI verified
