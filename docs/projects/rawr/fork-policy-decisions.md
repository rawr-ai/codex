# Fork Policy Decisions (Maintainability-First)

Status: approved defaults for current cycle
Date: 2026-02-05

## Canonical references
- `/Users/mateicanavra/.codex-rawr/skills/fork-rebase-maintenance/SKILL.md`
- `references/general-principles.md`
- `references/rust-cli-leaf.md`

## Decision 1: Home-dir strategy
Chosen:
- Keep upstream-compatible resolver semantics in shared code.
- Enforce fork isolation in launcher/env/documented fork entrypoints via `CODEX_HOME`.

Rejected:
- Fork-wide change of upstream shared resolver default (`~/.codex`) to `~/.codex-rawr`.
Reason: larger drift and repeated cross-crate rebase conflicts.

## Decision 2: Internal judgment tool eligibility
Chosen:
- Force internal non-transcript compaction judgment calls to run with web-search disabled.

Rejected:
- Using user-facing web-search eligibility for internal judgment.
Reason: policy leakage and avoidable divergence.

## Decision 3: Compaction-audit lifecycle
Chosen:
- Add deterministic stale-trigger cleanup at turn start.
- Preserve take-on-compaction behavior.

Deferred:
- Full migration from global map to session-owned storage.
Reason: higher change surface for current cycle.

## Decision 4: Protocol delta discipline
Chosen:
- Keep RAWR additions additive + optional.
- Isolate RAWR protocol types in a dedicated module to reduce conflict radius.
- Avoid new wire variants unless explicitly required by policy change.

## Implementation mapping
- Home-dir policy: implemented in Slice 5.
- Judgment eligibility policy: implemented in Slice 5.
- Compaction-audit cleanup: implemented in Slice 5.
- Protocol isolation policy: implemented in Slice 2.

## Follow-up tracking
- If hotspot conflicts persist for two cycles after seam + policy changes, run refork/reset evaluation.
