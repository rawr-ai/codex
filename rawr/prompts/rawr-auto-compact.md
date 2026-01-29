---
version: 0
trigger:
  # Early threshold: only compact on "big step" boundaries (topic/plan/PR).
  early_percent_remaining_lt: 85
  # Ready threshold: compact on common boundaries like commits.
  ready_percent_remaining_lt: 75
  # Asap threshold: compact at the next natural pause boundary.
  asap_percent_remaining_lt: 65
  # Back-compat: older configs used a single threshold.
  percent_remaining_lt: 75
  # Safety valve: compact even without a “natural boundary” when remaining context drops below this.
  emergency_percent_remaining_lt: 15
  # In `auto` mode, compact only when at least one boundary signal is present (unless emergency threshold triggers).
  auto_requires_any_boundary:
    - commit
    - pr_checkpoint
    - plan_checkpoint
    - agent_done
packet:
  max_tail_chars: 1200
---

[rawr] Before we compact this thread, produce a **continuation context packet** for yourself.

Requirements:
- Keep it short and structured.
- Include: overarching goal, current state, next steps, invariants/decisions, and a final directive to continue after compaction.
- Do not include secrets; redact tokens/keys.

Heuristic notes (for auditing)
- `commit`: a successful `git commit` occurred in this turn.
- `pr_checkpoint`: a PR lifecycle checkpoint occurred (publish/review/open/close heuristics).
- `plan_checkpoint`: the plan was updated and at least one step was marked `completed`.
- `agent_done`: the assistant explicitly indicates completion (e.g. “done”, “completed”, “finished”).
