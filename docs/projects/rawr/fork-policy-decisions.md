# Fork Policy Decisions (Maintainability-First)

Status: approved defaults for current cycle
Date: 2026-02-06

## Canonical references
- `/Users/mateicanavra/.codex-rawr/skills/fork-rebase-maintenance/SKILL.md`
- `/Users/mateicanavra/.codex-rawr/skills/graphite/SKILL.md`
- `/Users/mateicanavra/.codex-rawr/skills/parallel-development-workflow/SKILL.md`

## Decision 0: Branch governance model
Chosen:
- `codex/integration-upstream-main` is the Graphite operational trunk.
- `main` is not the day-to-day stack base.
- Upstream sync is checkpointed on the integration trunk, then descendants are restacked.
- Active tracked chain for this cycle is:
  - `codex/integration-upstream-main`
  - `codex/incremental-rebase-2026-02-06`
- Canonical open fork PR for this cycle is `#18` (`codex/incremental-rebase-2026-02-06`).

Rejected:
- Treating `main` as the default parent for daily Graphite work.
- Allowing ad hoc upstream rebases from any feature/child branch.

Reason:
- Prevents branch-base ambiguity.
- Restricts history rewrites to explicit checkpoints.
- Makes parentage/auditability visible and stable in `gt ls`.

## Decision 1: Home-dir strategy
Chosen:
- Keep upstream-compatible resolver semantics in shared code.
- Enforce fork isolation in launcher/env/documented fork entrypoints via `CODEX_HOME`.

Rejected:
- Fork-wide change of upstream shared resolver default (`~/.codex`) to `~/.codex-rawr`.

Reason:
- Larger drift and repeated cross-crate rebase conflicts.

## Decision 2: Internal judgment tool eligibility
Chosen:
- Force internal non-transcript compaction judgment calls to run with web-search disabled.

Rejected:
- Using user-facing web-search eligibility for internal judgment.

Reason:
- Policy leakage and avoidable divergence.

## Decision 3: Compaction-audit lifecycle
Chosen:
- Add deterministic stale-trigger cleanup at turn start.
- Preserve take-on-compaction behavior.

Deferred:
- Full migration from global map to session-owned storage.

Reason:
- Higher change surface for current cycle.

## Decision 4: Protocol delta discipline
Chosen:
- Keep RAWR additions additive + optional.
- Isolate RAWR protocol types in a dedicated module to reduce conflict radius.
- Avoid new wire variants unless explicitly required by policy change.

## Why this policy set prevents recurrence
- Branch governance prevents repeating the "stale main as implicit base" failure mode.
- Isolation and additive protocol policy keep fork delta small, reducing future conflict radius.
- Deterministic lifecycle cleanup avoids latent state drift that only surfaces during large rebases.

## Implementation mapping
- Branch governance model: Phase B canonicalization (this update).
- Home-dir policy: implemented in Slice 5.
- Judgment eligibility policy: implemented in Slice 5.
- Compaction-audit cleanup: implemented in Slice 5.
- Protocol isolation policy: implemented in Slice 2.

## Follow-up tracking
- If hotspot conflicts persist for two cycles after seam + policy changes, run refork/reset evaluation.
