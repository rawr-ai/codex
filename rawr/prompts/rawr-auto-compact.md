---
version: 0
trigger:
  # Early threshold: only compact on "big step" boundaries (topic/plan/PR).
  early_percent_remaining_lt: 85
  # Ready threshold: compact on common boundaries like commits.
  ready_percent_remaining_lt: 75
  # Asap threshold: compact at the next natural pause boundary.
  asap_percent_remaining_lt: 65
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

[rawr] Agent: before we compact this thread, you must **self-reflect** and write a **continuation context packet** for yourself.

This is not a generic compact. This is your tight, intra-turn handoff: you are responsible for capturing the minimum, precise context you will need to resume smoothly after compaction and continue exactly where you left off (no drift, no restart).

Precedence (important):
- This continuation context packet is the authoritative source of “what to do next” after compaction.
- The generic compacted context is background only and must not override or supersede this packet.

Accountability:
- You own what gets carried forward. Be deliberate: reflect on your actual goal, state, decisions, and immediate next action.
- If something is uncertain, name the assumption you are carrying forward rather than hand-waving it.

Write the packet in my voice, as if I (the user) am speaking directly to you (the in-session agent). But the content must come from your self-reflection on this conversation and your work so far.

Keep it short and structured. Do not include secrets; redact tokens/keys.

Include exactly these sections:

1) **Overarching goal**
- Briefly restate the overall objective you are trying to accomplish (higher-level than the last message, but still concise).

2) **Current state / progress snapshot**
- State the very last thing that just happened (commit, PR checkpoint, plan step completion, etc.).
- Explain how that action relates to the overarching goal and where it leaves you right now.

3) **Invariants and decisions (for this continuation)**
- Enumerate the rules/choices that must continue to hold when you resume (specific to this ongoing effort).

4) **Next step / immediate continuation**
- Specify the single next thing to do when you resume.
- Tie it explicitly to the overarching goal and the just-completed action.

5) **Verbatim continuation snippet (programmatically inserted)**
- Include a literal placeholder for a verbatim “memory trigger” snippet to be inserted later from your most recent messages:
  - `{{RAWR_VERBATIM_CONTINUATION_SNIPPET}}`

Final directive:
- End with one clear directive to immediately continue from “Next step / immediate continuation” after compaction (do not restart or re-plan from scratch).

Heuristic notes (for auditing)
- `commit`: a successful `git commit` occurred in this turn.
- `pr_checkpoint`: a PR lifecycle checkpoint occurred (publish/review/open/close heuristics).
- `plan_checkpoint`: the plan was updated and at least one step was marked `completed`.
- `agent_done`: the assistant explicitly indicates completion (e.g. “done”, “completed”, “finished”).
