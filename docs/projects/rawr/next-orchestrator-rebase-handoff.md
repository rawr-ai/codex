# Next Orchestrator Handoff: Upstream Checkpoint Rebase

Last updated: 2026-02-06T08:15:23Z (UTC)
Owner: RAWR fork maintenance

## Current state snapshot
- Canonical integration branch: `codex/integration-upstream-main`
- Current commit on integration branch: `5e3a6e2c6f30a91452fe76c41db6f905c8d7ec02`
- Fork `main` commit: `ebc65cb9fb3da9e374e70c6cdea2980c0ec27c71`
- Upstream `main` commit: `dd80e332c45aefc935d68fe067026ccf40312ccd`
- Merge-base (`codex/integration-upstream-main`, `upstream/main`): `048e0f3888a5c5bef8d2272e44b30a1aea4c8f92`
- Divergence:
  - `main...upstream/main`: `26 / 261` (left/right)
  - `codex/integration-upstream-main...upstream/main`: `1 / 3` (left/right)
- PR status:
  - `#18` is merged into `codex/integration-upstream-main`
  - No open canonical rebase PR at this moment

## Policy that must remain true
- Operational Graphite trunk is `codex/integration-upstream-main`.
- `main` is not the day-to-day stack base.
- Upstream sync happens only at explicit checkpoints.
- Use Graphite for stack/PR operations; use Git for inspection and low-level rebase conflict plumbing.

## Ready-to-paste prompt for next orchestrator

```markdown
You are the orchestration agent for the next RAWR upstream checkpoint rebase.

Repository and remotes:
- Path: /Users/mateicanavra/Documents/.nosync/DEV/rawr-ai/codex
- Origin: https://github.com/rawr-ai/codex.git
- Upstream: https://github.com/openai/codex.git

Non-negotiable policy:
1. Operational Graphite trunk: codex/integration-upstream-main
2. main is not the day-to-day stack base
3. Upstream sync is checkpointed and explicit
4. Graphite-first for stack/PR operations

Current baseline:
- integration branch: codex/integration-upstream-main @ 5e3a6e2c6f30a91452fe76c41db6f905c8d7ec02
- upstream/main @ dd80e332c45aefc935d68fe067026ccf40312ccd
- divergence (integration...upstream): 1 left / 3 right
- previous canonical PR #18 is merged

Required references:
- /Users/mateicanavra/Documents/.nosync/DEV/rawr-ai/codex/docs/projects/rawr/rebase-runbook.md
- /Users/mateicanavra/Documents/.nosync/DEV/rawr-ai/codex/docs/projects/rawr/rebase-gotchas.md
- /Users/mateicanavra/Documents/.nosync/DEV/rawr-ai/codex/docs/projects/rawr/fork-policy-decisions.md
- /Users/mateicanavra/Documents/.nosync/DEV/rawr-ai/codex/rawr/UPDATING.md
- /Users/mateicanavra/Documents/.nosync/DEV/rawr-ai/codex/docs/projects/rawr/release-readiness.md
- /Users/mateicanavra/Documents/.nosync/DEV/rawr-ai/codex/docs/projects/rawr/next-orchestrator-rebase-handoff.md

Execution model (multi-agent):
- Agent A: upstream delta analysis + hotspot prediction
- Agent B: conflict-prep and test-plan prep
- Agent C: policy guardrail reviewer during replay
- Orchestrator: executes checkpoint rebase and gates decisions

Process:
1. Preflight: verify clean tree, branch/trunk state, and remotes.
2. Analyze upstream delta and list likely conflict files.
3. Run dry-run checkpoint (`DRY_RUN=1 rawr/sync-upstream.sh codex/integration-upstream-main`).
4. Execute real checkpoint rebase on codex/integration-upstream-main.
5. Restack descendants with Graphite (`gt sync --no-restack`, `gt restack --upstack`).
6. Run validation gates from runbook (format + targeted tests + full suite gate when required).
7. Open/update canonical PR for this checkpoint cycle.
8. Update release-readiness with fresh SHAs, divergence, and test evidence.

Hard stop / escalation points:
- Semantic conflict that may alter fork policy behavior
- Need for force-push without lease-safe guarantees
- Test failures in core/protocol/tui that imply behavior regression

Definition of done:
- integration branch rebased to checkpoint target
- stack restacked cleanly
- validation evidence recorded
- canonical PR opened/updated and in clean state
- docs/readiness updated with current reality
```

## Notes for the next cycle
- Keep one rollback anchor until checkpoint PR is validated.
- Do not rebuild the old `main`-based stack model.
- If rebase pain rises for two consecutive cycles, trigger refork/reset evaluation (see policy doc).
