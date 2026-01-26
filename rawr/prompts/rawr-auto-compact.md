---
version: 0
trigger:
  # The watcher only considers compacting when remaining context drops below this percent.
  percent_remaining_lt: 75
  # Safety valve: compact even without a “natural boundary” when remaining context drops below this.
  emergency_percent_remaining_lt: 15
  # In `auto` mode, compact only when at least one boundary signal is present (unless emergency threshold triggers).
  auto_requires_any_boundary:
    - commit
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
- `plan_checkpoint`: the plan was updated and at least one step was marked `completed`.
- `agent_done`: the assistant explicitly indicates completion (e.g. “done”, “completed”, “finished”).
