# Fork Maintenance Spike (Canonicalization)

Status: decision complete
Date: 2026-02-05

## Executive verdict
Needs seam refactor first before additional high-risk policy work.

The fork is close to a maintainable shape (additive RAWR modules, feature flags), but it is still too dependent on high-churn upstream integration files. We should reduce the conflict surface before layering new risky behavior.

## Canonical reference
- `/Users/mateicanavra/.codex-rawr/skills/fork-rebase-maintenance/SKILL.md`
- `references/general-principles.md`
- `references/rust-cli-leaf.md`

## Current architecture fitness
Good:
- Mirror + patch queue mindset is already documented.
- Fork behavior is mostly additive and feature-gated (`Feature::RawrAutoCompaction`).

Not good enough yet:
- RAWR logic is still wired through upstream conflict hotspots.
- Policy defaults diverge from upstream in core resolver behavior.

## Rebase hotspot map
- `codex-rs/tui/src/chatwidget.rs`
- `codex-rs/core/src/codex.rs`
- `codex-rs/core/src/config/mod.rs`
- `codex-rs/protocol/src/protocol.rs`

## Why these hotspots hurt rebase health
- They are frequently modified upstream.
- Fork-specific edits in these files create repeated semantic conflicts.
- Mixed concerns (policy + integration + behavior) increase resolution cost per cycle.

## Invariants (must hold)
1. Upstream is timeline authority.
2. Fork behavior stays additive and feature-gated.
3. Protocol changes remain backward-compatible and optional.
4. Architectural seam changes and policy/risk changes are separate slices.

## Anti-invariants (must avoid)
1. Deep fork logic embedded directly in upstream core/tui hotspots.
2. Expanding non-optional protocol surface for fork-only behavior.
3. Carrying unresolved policy divergence across crates.

## Ordered structural recommendations
1. Isolate RAWR protocol types in a dedicated module and keep protocol.rs wiring minimal.
2. Isolate compaction-trigger construction in a reusable core helper module (shared by core + tui callers).
3. Keep `chatwidget.rs` and `codex.rs` integration hooks thin and delegate transformation logic to helper modules.
4. Restore upstream-compatible home-dir resolution behavior in shared code; apply fork defaults through launcher/env policy.

## Decision
Proceed with targeted canonicalization now (not a broad rewrite), then implement high-risk policy fixes on top.
