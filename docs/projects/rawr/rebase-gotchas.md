# Rebase Gotchas and Checklist

Status: operational notes
Date: 2026-02-05

## Frequent traps
1. Replaying fork changes in hotspot files without tight boundaries (`chatwidget.rs`, `codex.rs`, `config/mod.rs`, `protocol.rs`).
2. Forgetting to enforce clean-tree preflight before upstream sync.
3. Force-pushing without lease guard.
4. Mixing architecture seam changes with policy behavior changes in one commit.

## Fork-specific gotchas
1. Home-dir behavior can drift across crates if resolver policy is inconsistent.
2. Internal judgment calls should not inherit user web-search eligibility.
3. Compaction-trigger state can go stale if never consumed; clean at turn start.

## Rebase drill (this cycle)
- Performed with `DRY_RUN=1` and branch preflight checks.
- Observed result on implementation branch:
  - failed fast with `error: working tree must be clean before sync-upstream`.
- Interpretation:
  - clean-tree gate is functioning as intended.
  - full replay path (`mirror sync -> patch replay -> validation -> publish/rollback`) remains pending until a clean checkpoint.

## Recurring checklist
- [ ] clean tree
- [ ] remotes fetched/pruned
- [ ] mirror main synced to upstream
- [ ] patch branch selected explicitly
- [ ] rebase completed and conflicts resolved semantically
- [ ] `just fmt` executed (if Rust touched)
- [ ] crate-scoped tests passed
- [ ] full-suite gate decision recorded
- [ ] lease-protected push used
- [ ] post-push CI verified
