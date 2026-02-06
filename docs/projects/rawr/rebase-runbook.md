# Fork Rebase Runbook (Canonical)

Status: executable playbook
Date: 2026-02-05

## Scope
Fork-specific rebase workflow for `rawr-ai/codex`, aligned with canonical fork-maintenance guidance.

## Branch model
- `main`: upstream mirror branch (no fork behavior).
- integration/patch branch: replay queue branch for fork behavior.
  - default: current branch unless explicitly provided.
  - historical examples: `rawr/main`, `codex/rebase-upstream-YYYY-MM-DD`.

## Human decision gates
1. Mode gate
- Frequent small rebase
- Infrequent large rebase
- Refork/reset evaluation

2. Conflict semantics gate
- Resolve conflicts preserving fork invariants.
- Pause if conflict implies behavior change beyond intended patch.

3. Full-suite gate
- Run crate-scoped tests first.
- Request explicit go/no-go before full suite.

4. Force-push gate
- Require explicit authorization to update remote rewritten history.

## Mechanical steps
1. Preflight
- Clean tree required.
- Verify remotes (`origin`, `upstream`).
- Fetch/prune all remotes.

2. Mirror sync
- Checkout `main`.
- Fast-forward to `upstream/main`.
- Push `main` to `origin`.

3. Patch replay
- Checkout patch branch.
- Rebase patch branch onto `upstream/main`.

4. Conflict loop
- Inspect current patch intent.
- Resolve conflict files.
- Continue rebase.

5. Validation
- `just fmt` in `codex-rs`.
- run changed-crate tests.
- if core/protocol/common touched, full suite only after explicit go/no-go.

6. Publish
- Push patch branch with `--force-with-lease`.

7. Recovery (if needed)
- Local rollback via reflog/reset.
- Remote rollback via lease-protected force-push to known good commit.

## Command skeleton
```bash
git status --porcelain
git fetch --all --prune

git checkout main
git pull --ff-only upstream main
git push origin main

git checkout <patch-branch>
git rebase upstream/main

cd codex-rs
just fmt
cargo test -p codex-core
cargo test -p codex-tui
cargo test -p codex-app-server-protocol
# ask before: cargo test --all-features

cd ..
git push --force-with-lease origin <patch-branch>
```

## Known hotspot files during conflicts
- `codex-rs/core/src/codex.rs`
- `codex-rs/core/src/compact.rs`
- `codex-rs/core/src/config/mod.rs`
- `codex-rs/core/src/rollout/policy.rs`
- `codex-rs/tui/src/chatwidget.rs`
- `codex-rs/mcp-server/tests/common/mcp_process.rs`
- `codex-rs/Cargo.lock`

## Operational checklists
### Before push
- range-diff intent preserved (recommended for large rebases).
- tests pass.
- no debug artifacts left in tests.

### After push
- CI green.
- replayed patch behavior verified in core+tui integration paths.
