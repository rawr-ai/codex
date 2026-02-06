# Fork Rebase Runbook (Canonical)

Status: executable playbook
Date: 2026-02-06

## Scope
Fork-specific rebase workflow for `rawr-ai/codex`, aligned with the permanent model where `codex/integration-upstream-main` is the Graphite operational trunk.

## Permanent branch model
- Graphite operational trunk: `codex/integration-upstream-main`.
- Tracked descendants (if any): discovered via `gt ls --all` (trunk-only is a valid steady state).
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
- Scheduled daily runs: auto-run `cargo test --all-features`.
- Interactive/manual runs: request explicit go/no-go before `cargo test --all-features`.

## Automation boundaries (agent-first)
- Tools/scripts automate mechanical, idempotent, atomic steps (locks, snapshots, verify attempts, lease-safe pushes, reports, restacks, tests).
- Conflict resolution and semantic decisions are owned by the orchestrator agent (and escalated to a human only at explicit gates).
- “Daily schedule” means a scheduler launches the orchestrator agent (with the dedicated prompt) and the agent runs these tools; it does not assume deterministic rebases.

## Mechanical steps
1. Preflight
- Clean tree required.
- Verify remotes (`origin`, `upstream`).
- Verify tracked chain with `gt ls --all` (may be trunk-only).

2. Checkpoint sync on integration trunk
- Prefer the daily orchestrator wrapper:
  - `bash rawr/rebase-daily.sh`
- Or run the checkpoint script directly (verify-first, apply-second):
  - `DRY_RUN=1 rawr/sync-upstream.sh codex/integration-upstream-main`
  - `rawr/sync-upstream.sh codex/integration-upstream-main`

3. Restack tracked descendants
- Run `gt sync --no-restack` (never a global restack in parallel workflows).
- If `gt ls --all` shows tracked descendants above trunk, run `gt restack --upstack`.

4. Validation
- `just fmt` in `codex-rs`.
- Run changed-crate tests.
- Scheduled daily runs: run full suite (`cargo test --all-features`).
- Interactive/manual runs: full suite only after explicit go/no-go.

5. Publish descendants
- Push any rewritten descendant branches with `--force-with-lease`.

6. Recovery (if needed)
- Local rollback via reflog/reset to known-good checkpoint.
- Remote rollback via lease-protected force-push to known-good commit.

## Command skeleton
```bash
git status --porcelain
gt ls --all
git fetch --all --prune

DRY_RUN=1 rawr/sync-upstream.sh codex/integration-upstream-main
rawr/sync-upstream.sh codex/integration-upstream-main

gt sync --no-restack
# If descendants exist:
gt restack --upstack

cd codex-rs
just fmt
cargo test -p codex-core
cargo test -p codex-tui
cargo test -p codex-app-server-protocol
# Scheduled daily: cargo test --all-features
# Interactive/manual: ask before cargo test --all-features
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
