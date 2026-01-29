# Linear Issue: Auto-compaction v0 — boundary heuristics + continuation packet quality

## Context
We have a `rawr-ai/codex` fork that adds a feature-flagged, in-TUI watcher to control compaction timing and inject a post-compact continuation context packet so the in-session agent can resume without drift.

Current implementation status (as of `rawr/main`):
- Feature flag: `[features] rawr_auto_compaction = true`
- Fork isolation: defaults `CODEX_HOME=~/.codex-rawr` when unset
- Built-in auto-compaction bypassed when rawr watcher enabled
- Watcher can trigger compaction at turn completion and **mid-turn** (between sampling requests) once boundary heuristics fire.
- Auto gating now requires “natural boundary” signals unless emergency threshold triggers:
  - `commit` (successful `git commit` observed)
  - `pr_checkpoint` (PR lifecycle checkpoint observed: publish/review/open/close heuristics)
  - `plan_checkpoint` (plan updated with at least one `completed` step)
  - `agent_done` (best-effort heuristic on last agent message)
- Heuristics prompt is embedded from `rawr/prompts/rawr-auto-compact.md` (YAML frontmatter defaults + Markdown body)

References:
- Watcher + settings loader: `codex-rs/tui/src/chatwidget.rs`
- Prompt template: `rawr/prompts/rawr-auto-compact.md`
- Install/switching docs: `rawr/INSTALL.md`

## Problem
We need higher correctness and robustness around “when to compact” and a better continuation packet so agents reliably continue work after compaction (especially when auto-compaction triggers).

## Goals
- Increase correctness of natural-boundary detection (commit/plan completion/agent completion).
- Improve continuation packet content so post-compaction resumption is immediate and non-redundant.
- Make heuristics easy to audit/edit (single source of truth: prompt YAML + prompt body).
- Keep fork delta small, feature-flagged, and rebase-friendly.

## Non-goals (v0)
- Full semantic “task initiated” classifier across session history (separate workstream).
- External terminal automation / separate TUI wrapper.

## Proposed approach (incremental)
### A) Tighten boundary signals
1) Commit boundary
   - Improve `git commit` detection (handle `git` aliases/wrappers, multi-command shells, `--amend`, etc.).
   - Only count commit boundary if the command actually committed (exit code 0) and output indicates success where possible.
2) Plan checkpoint boundary
   - Track “plan checkpoint” only when:
     - At least one step transitions to `completed`, and
     - The plan is non-empty (ignore transient plan clears).
   - Consider requiring that the completed step belongs to the active plan (guard against older-plan edits).
3) Agent done boundary
   - Replace string-contains heuristic with an explicit, local ruleset in the prompt YAML (e.g., list of “done markers”).
   - Optionally: gate agent-done boundary on the agent announcing completion *and* the turn having “work activity” (exec/tool/patch) to reduce false positives.

### B) Improve continuation packet quality
Enhance the watcher-built packet to include the multi-level context we want:
- Overarching goal / scope
- Current state (what was done this slice)
- Next steps (immediate actions)
- Locked invariants / decisions / “do not repeat” findings
- Memory trigger: last agent output tail (bounded)
- Final directive: “continue now”

Additionally, implement the “ask agent for packet” flow using the editable prompt body:
- When `rawr_auto_compaction.packet_author = "agent"`:
  1) Inject the prompt (from prompt file body)
  2) Wait for the agent’s packet response
  3) Trigger compaction
  4) Inject that packet as the first post-compact user message

### C) Make YAML knobs the single source of truth (with config overrides)
- Keep default trigger values in YAML:
  - `early_percent_remaining_lt`
  - `ready_percent_remaining_lt`
  - `asap_percent_remaining_lt`
  - `emergency_percent_remaining_lt`
  - `auto_requires_any_boundary`
- Allow config overrides for explicit, per-user control.
- Add a “reload-on-change” story later; for now, load at turn completion (cheap + safe).

## Acceptance criteria
- In `auto` mode, watcher compacts only when:
  - Remaining context falls below one of `early` / `ready` / `asap` thresholds, and
  - A tier-appropriate boundary is observed (unless the emergency threshold triggers).
- In `auto` mode + `packet_author=agent`, injected agent prompt matches the prompt file body.
- Post-compaction injection happens exactly once per compaction and never mid-turn.
- Unit tests cover:
  - Boundary-required gating vs emergency override
  - Plan-checkpoint detection from `UpdatePlanArgs` (completed step)
  - Commit detection from `ExecCommandEndEvent` variants
  - YAML frontmatter parsing failure → safe fallback defaults (no panic)

## Task breakdown
1) Boundary correctness
   - Extend commit detection logic + tests
   - Tighten plan-checkpoint detection rules + tests
   - Improve agent-done signal (prompt-configurable markers) + tests
2) Packet quality
   - Implement richer watcher packet format
   - Implement agent-authored packet flow (prompt body) end-to-end
3) Hardening
   - Ensure malformed YAML never breaks the session
   - Add logs/telemetry breadcrumbs (minimal, gated, no spam)

## Notes / risks
- Avoid “cross wires”: keep default `CODEX_HOME=~/.codex-rawr`.
- Keep changes isolated under `Feature::RawrAutoCompaction`.
- Don’t trigger compaction when there are queued user messages or modals active.
