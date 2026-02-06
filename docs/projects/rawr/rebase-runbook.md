# Fork Rebase Runbook (Canonical)

Status: executable playbook
Date: 2026-02-06

## Scope
Fork-specific rebase workflow for `rawr-ai/codex`, aligned with the permanent model where `codex/integration-upstream-main` is the Graphite operational trunk.

## Permanent branch model
- Graphite operational trunk: `codex/integration-upstream-main`.
- Active tracked chain (current cycle):
  - `codex/integration-upstream-main`
  - `codex/incremental-rebase-2026-02-06`
- `main`: optional upstream mirror branch only; not the day-to-day stack base.
- Legacy untracked stack branches: ignored unless explicitly resurrected.

## Operational rules
1. Upstream sync happens only at controlled checkpoints on `codex/integration-upstream-main`.
2. Day-to-day branch work stacks on the integration trunk, never on `main`.
3. After checkpoint sync, restack descendants before feature work continues.
4. In parallel-agent workflows, avoid global restacks; use `gt sync --no-restack` and stack-scoped restacks.

## Human decision gates
1. Checkpoint gate
- Is this the right time to rewrite integration-trunk history and restack descendants?

2. Conflict semantics gate
- Resolve conflicts preserving fork invariants.
- Pause if conflict implies behavior change beyond intended patch replay.

3. Full-suite gate
- Run crate-scoped tests first.
- Request explicit go/no-go before `cargo test --all-features`.

## Mechanical steps
1. Preflight
- Clean tree required.
- Verify remotes (`origin`, `upstream`).
- Verify active tracked chain with `gt ls`.

2. Checkpoint sync on integration trunk
- Fetch/prune remotes.
- Checkout `codex/integration-upstream-main`.
- Rebase onto `upstream/main`.
- Push integration trunk with `--force-with-lease`.

3. Restack tracked descendants
- Checkout the active child branch (currently `codex/incremental-rebase-2026-02-06`).
- Run `gt sync --no-restack`.
- Run `gt restack --upstack`.

4. Validation
- `just fmt` in `codex-rs`.
- Run changed-crate tests.
- If common/core/protocol touched, full suite only after explicit go/no-go.

5. Publish descendants
- Push any rewritten descendant branches with `--force-with-lease`.

6. Recovery (if needed)
- Local rollback via reflog/reset to known-good checkpoint.
- Remote rollback via lease-protected force-push to known-good commit.

## Command skeleton
```bash
git status --porcelain
gt ls
git fetch --all --prune

git checkout codex/integration-upstream-main
git rebase upstream/main
git push --force-with-lease origin codex/integration-upstream-main

git checkout codex/incremental-rebase-2026-02-06
gt sync --no-restack
gt restack --upstack

cd codex-rs
just fmt
cargo test -p codex-core
cargo test -p codex-tui
cargo test -p codex-app-server-protocol
# ask before: cargo test --all-features
```

## Known hotspot files during conflicts
- `codex-rs/core/src/codex.rs`
- `codex-rs/core/src/compact.rs`
- `codex-rs/core/src/config/mod.rs`
- `codex-rs/core/src/rollout/policy.rs`
- `codex-rs/tui/src/chatwidget.rs`
- `codex-rs/mcp-server/tests/common/mcp_process.rs`
- `codex-rs/Cargo.lock`

## Why this prevents recurrence
- One explicit operational trunk removes ambiguity over where rebases happen.
- Keeping `main` out of day-to-day stack parentage prevents accidental rebases onto stale bases.
- Checkpoint-only upstream sync localizes history rewrites to predictable windows.
- Graphite-tracked parentage makes drift visible (`gt ls`) before it compounds.
