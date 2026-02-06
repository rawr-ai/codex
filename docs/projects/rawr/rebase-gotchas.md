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
- Performed with clean tree and `DRY_RUN=1`:
  - `DRY_RUN=1 rawr/sync-upstream.sh codex/rebase-upstream-2026-02-05`
- Observed command path in dry-run output:
  - `git fetch upstream`
  - `git checkout main`
  - `git pull --ff-only upstream main`
  - `git push origin main`
  - `git checkout codex/rebase-upstream-2026-02-05`
  - `git rebase upstream/main`
  - `bash rawr/bump-fork-version.sh --commit`
  - `git push --force-with-lease origin codex/rebase-upstream-2026-02-05`
  - restore starting branch
- Interpretation:
  - clean-tree gate and explicit patch-branch handling are both enforced.
  - dry-run command ordering matches the runbook.

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
