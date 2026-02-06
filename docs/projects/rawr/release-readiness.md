# RAWR Release Readiness (2026-02-06)

## Scope
- Stack: `codex/rebase-upstream-2026-02-05` -> `codex/fork-maintenance-spike` -> `codex/fork-seams-core-tui` -> `codex/rebase-runbook-canonical` -> `codex/policy-home-judgment-audit` -> `codex/high-risk-remediation` -> `codex/rebase-drill-and-gates`
- Top branch commit: `91065dec7` (`docs(rawr): add release readiness artifact with merge blockers`)

## Stable Integration Checkpoint (this cycle)
- Stable branch: `codex/rebase-drill-and-gates`
- Stable commit SHA: `91065dec76d4eece71f0d20a83f2f10ac05a03ee`
- Stable annotated tag: `rawr-stable-20260206-2238`
- Policy: no new commits should land on the stable branch after this checkpoint without reopening validation gates.

## PR Stack State (`rawr-ai/codex`)
- #11 `codex/rebase-upstream-2026-02-05 -> main` (DIRTY): <https://github.com/rawr-ai/codex/pull/11>
- #12 `codex/fork-maintenance-spike -> codex/rebase-upstream-2026-02-05` (CLEAN): <https://github.com/rawr-ai/codex/pull/12>
- #13 `codex/fork-seams-core-tui -> codex/fork-maintenance-spike` (CLEAN): <https://github.com/rawr-ai/codex/pull/13>
- #14 `codex/rebase-runbook-canonical -> codex/fork-seams-core-tui` (CLEAN): <https://github.com/rawr-ai/codex/pull/14>
- #15 `codex/policy-home-judgment-audit -> codex/rebase-runbook-canonical` (CLEAN): <https://github.com/rawr-ai/codex/pull/15>
- #16 `codex/high-risk-remediation -> codex/policy-home-judgment-audit` (CLEAN): <https://github.com/rawr-ai/codex/pull/16>
- #17 `codex/rebase-drill-and-gates -> codex/high-risk-remediation` (CLEAN): <https://github.com/rawr-ai/codex/pull/17>

## Local Validation Evidence (this cycle)
All run from `codex-rs` on branch `codex/rebase-drill-and-gates`:
- `just fmt` passed
- `just fix -p codex-core` passed
- `just fix -p codex-protocol` passed
- `just fix -p codex-tui` passed
- `cargo test -p codex-core` passed
- `cargo test -p codex-protocol` passed
- `cargo test -p codex-app-server-protocol` passed
- `cargo test -p codex-tui` passed
- `cargo test --all-features` passed
- `cargo insta pending-snapshots` reported: `No pending snapshots.`

## CI Evidence (this cycle)
- Current stack PR check rollups remain empty (`checkCount=0`) for #11..#17.
- Manual CI dispatch was executed for stack head branch:
  - Workflow: `rust-ci.yml`
  - Branch: `codex/rebase-drill-and-gates`
  - Run: <https://github.com/rawr-ai/codex/actions/runs/21737722689>
  - Result: `failure`
- Primary failure causes from run annotations:
  - Required runner group `codex-runners` not available in this fork.
  - Some jobs blocked by account billing/spending-limit constraints.

## Local Release Evidence
- Local release command executed previously: `bash rawr/release-local.sh --tag rawr-local-20260205-2046`
- Existing local release tag: `rawr-local-20260205-2046`
- Version verification (previous run):
  - `codex --version` => `codex-cli 0.100.0-alpha.3`
  - `codex-rawr --version` => `codex-cli 0.100.0-alpha.3`
  - `codex-rawr-bin --version` => `codex-cli 0.100.0-alpha.3`

## Blockers to Full Green Merge
1. Base PR #11 is not mergeable into `main` (`mergeStateStatus=DIRTY`) and requires a dedicated rebase conflict-resolution pass.
2. Fork CI cannot currently reach required-check green due infrastructure constraints (`codex-runners` runner-group availability and billing constraints).
3. Because #11 is blocked, bottom-up stack merge and post-merge sync remain blocked.

## Current Decision for This Cycle
- Treat `codex/rebase-drill-and-gates` at `rawr-stable-20260206-2238` as the single stable integration tip.
- Do not start incremental rebase in this cycle.
- Hand off with explicit local-green / CI-blocked status and next-step runbook context.
