# RAWR Auto-Compaction — Issues/Traps + Next Architecture Draft

Status: draft (notes captured verbatim + initial outline)

This doc is intentionally a **draft staging area**:

- first: capture the current “issues/traps” and “better foundational approach” notes verbatim
- then: outline the next-architecture direction (core arbiter + structured boundary signals + persistent structured state)

---

## Draft notes (captured verbatim)

### Issues / traps I see in the current diagrams/design (real, not hypothetical)

- **Two RAWR orchestrators + one upstream**: TUI watcher (turn-complete), core mid-turn (inside sampling loop), plus upstream built-in auto-compact. That’s inherently a “3 sources can compact” world; we’ve made their *policies align*, but it’s still a coordination hazard and a debugging tax.
- **Race/correlation risk on judgment results**: the non-transcript judgment path is Op/Event driven; if there’s ever more than one judgment in flight, you want a correlation id (or strict single-flight guarantee) or you can apply the wrong result to the wrong pending decision. (Even if we “intend” single-flight, the system should enforce it.)
- **Heuristic fragility (“semantic break”)**: the plan-boundary semantic-break gate depends on the agent’s language. If the agent doesn’t say “done/topic shift/concluding” (or says it loosely), policy can misfire. This is the single biggest “it still feels bad” lever after config.
- **Preflight + queued-message prefixing is subtle**: prefixing the next real user message with a handoff avoids an extra turn, but it can also change the user’s message semantics and can be confusing when debugging (“why did my message change?”).
- **Judgment prompt context may be insufficient**: the judgment call is only as good as what we feed it. If it doesn’t reliably see the plan trail + user-request chain + “what just changed”, it will approve too often.

### Better foundational approach (more modular/reliable) than what we have now

- **Single arbiter in core**: move the *decision* authority entirely into core (watcher becomes UI-only + boundary signal reporter). Core would emit something like “compaction recommended/required + why + what pre-compact requests to run”, and the TUI just renders/executes. This removes duplicated policy logic and eliminates watcher/core divergence as a class of bugs.
- **Structured boundary signals, not language heuristics**: instead of relying on “semantic break” inferred from text, persist explicit “phase boundary” signals (plan checkpoint completion events, commit/PR events, user-request chain milestones) in structured state and use that for gating + judgment context.
- **Treat “pre-compact requests” as a first-class queue** shared by watcher + mid-turn, so scratch/packet/judgment sequencing is unified and testable in one place.

Net: what we have is *pretty good for an incremental fork* (and it’s testable), but it’s not the theoretical best architecture; the “single arbiter in core + structured boundary state” direction would reduce complexity and improve reliability a lot.

### Branch / worktree correctness

- The spec doc was initially created in the primary `main` checkout as an untracked file (wrong place for the stack).
- I moved it onto the top-of-stack branch and committed it there: `agent-codex-rawr-midturn-guard` now contains `docs/projects/rawr/auto-compaction-system.md` as commit `1633cebd7`.
- Primary repo `/Users/mateicanavra/Documents/.nosync/DEV/rawr-ai/codex` is clean on `main`; worktree `/Users/mateicanavra/Documents/.nosync/DEV/worktrees/wt-agent-codex-rawr-midturn-guard` is clean on `agent-codex-rawr-midturn-guard` and is the correct place for this work.

---

## Next direction: core arbiter + structured boundary signals + persistent structured state (outline)

### Goals

- One logical **decision authority** for compaction: the **core arbiter**.
- Decisions primarily based on **structured state** and **structured boundary signals**, with text heuristics as a secondary input.
- The arbiter can, for each boundary trigger, decide:
  - whether to compact
  - which pre-compact requests to schedule (scratch, continuation packet, other future types)
  - what to include in any pre-/during-/post-compact user-facing messages
- Keep upstream drift minimal: do not replace/remove upstream compaction; build a fork-specific layer that can rebase cleanly.

### Non-goals (for the first iteration)

- No “perfect” semantic understanding from free-form language.
- No requirement for a long-lived daemon process; persistence should allow cold-start arbiter instances.
- No rewriting of upstream built-in auto-compact; RAWR remains fork-specific and feature-flag gated.

### Core concept: structured boundary signals

Structured boundary signals should be explicit events, not inferred from assistant prose. Examples:

- Plan lifecycle
  - plan created
  - plan updated
  - plan step completed (explicit checklist-style signal)
  - plan checkpoint (explicit “phase boundary” marker)
- Repo/workflow lifecycle
  - branch created / switched
  - commit created
  - PR opened/updated/merged (if/when integrated)
  - graphite stack updated (if/when integrated)
- Session lifecycle
  - turn started / completed
  - tool invocation milestones (optional)
  - compaction completed

Properties of a good signal:

- emitted deterministically by the system that “knows” (e.g., plan checkpoint emitted by plan tool, commit emitted by VCS observer)
- has stable identifiers (timestamp, turn_id, thread_id, and optional correlation ids)
- can be persisted and replayed

### Core concept: persistent structured state (“log/canvas”)

The arbiter reads structured state first on every boundary trigger.

Structured state should be a persisted object (or event-sourced log) containing at least:

- Plan(s)
  - active plan (structured steps)
  - high-level agent actions taken per step (optional)
- Repository workflow metadata
  - current branch
  - (optional) graphite stack shape / parent/child relationships
  - (optional) repo “mode” (research / implement / test / release)
- User intent trail
  - initial user message (verbatim)
  - subsequent “directive” messages (verbatim or summarized)
  - extracted invariants / constraints
- Arbiter annotations (watcher-augmented state)
  - “phase boundary” markers (explicit)
  - “do not compact until …” guards
  - last compaction rationale (tier, boundary, judgment result)

### Core arbiter behavior (high-level)

On boundary trigger:

1. Load structured state (and recent structured boundary events).
2. Evaluate policy (tiers/boundaries/judgment) primarily using structured state/signals.
3. Optionally consult recent text (last agent message, recent turns) for:
   - disambiguation
   - judgment prompt context
4. Decide:
   - compact now vs defer
   - which pre-compact requests to run
   - what user-visible messaging to emit (if any)
5. Execute:
   - schedule pre-compact request(s)
   - schedule compaction
   - schedule post-compaction handoff
6. Persist:
   - decision record (tier, signals, reasons, judgment output)
   - any structured-state updates

### Minimal data model sketch (not final)

- `RawrBoundaryEvent`
  - `id`, `timestamp`, `turn_id`, `thread_id`
  - `kind` (enum)
  - `payload` (typed struct per kind)
- `RawrStructuredState`
  - `session_id`, `thread_id`
  - `plan_state` (optional structured plan)
  - `repo_state` (branch, stack summary)
  - `directives` (ordered list, with stability flags)
  - `invariants` (ordered list)
  - `last_compaction` (summary + decision metadata)
- `RawrCompactionDecision`
  - tier, percent_remaining, satisfied_signals
  - selected pre-compact requests
  - judgment result (optional)
  - outcome (compacted / deferred / vetoed)

### Integration boundaries (how to keep rebasing feasible)

Prefer:

- Additive fork-specific code paths gated behind `Feature::RawrAutoCompaction`
- Minimal additions to protocol types (new events/ops) when necessary
- Centralize fork-specific logic behind a small set of “arbiter” entrypoints

Avoid:

- rewriting upstream compaction
- duplicating policy in multiple components (TUI + core) long-term

### Migration path (incremental)

1. Introduce structured state persistence + boundary signal emission (no behavior change).
2. Move decision logic into core arbiter (TUI watcher becomes reporter/executor only).
3. Keep existing config-driven policy matrix as the policy engine, but feed it better structured inputs.
4. Gradually reduce reliance on language heuristics (semantic break) as more signals become structured.

### Open questions (to resolve before implementation)

- Persistence location/format:
  - in-session state only, or also on disk under `~/.codex-rawr/`?
  - event-sourced log vs periodically snapshotted state?
- Correlation ids:
  - how to correlate decision requests/responses (especially judgment) safely?
- Scope of “structured user intent trail”:
  - how do we detect “directive” vs “chatty” user messages reliably?
- How much of repo/Graphite state can/should be inferred deterministically vs tool-assisted?

