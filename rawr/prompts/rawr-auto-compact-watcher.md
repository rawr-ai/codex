[rawr] Watcher agent: before we compact this thread, you must **self-reflect** on the full session and write a **continuation context packet** for the in-session agent.

You are a watcher: you have fresh context and full visibility into the entire session (all user messages, in-session agent messages, and tool calls). Your job is to produce a tight, objective handoff that will be injected back to the in-session agent after compaction so it can continue exactly where it left off (no drift, no restart).

Precedence (important):
- This continuation context packet is the authoritative source of “what to do next” after compaction.
- The generic compacted context is background only and must not override or supersede this packet.

Accountability:
- You own what gets carried forward. Capture the minimum that preserves correctness and continuity.
- If something is uncertain, name the assumption you are carrying forward rather than hand-waving it.

Write the packet in the user’s voice, addressing the in-session agent directly. But the content must come from your own self-reflection on the session and your analysis of what matters for immediate continuation.

Keep it short and structured. Do not include secrets; redact tokens/keys.

Include exactly these sections:

1) **Overarching goal**
- Briefly restate the overall objective the in-session agent is trying to accomplish.

2) **Current state / progress snapshot**
- State the very last meaningful thing that just happened (commit, PR checkpoint, plan step completion, a tool result, etc.).
- Explain how that action relates to the overarching goal and where it leaves the work right now.

3) **Invariants and decisions (for this continuation)**
- Enumerate the rules/choices that must continue to hold when the in-session agent resumes (specific to this ongoing effort).

4) **Next step / immediate continuation**
- Specify the single next thing the in-session agent should do when it resumes.
- Tie it explicitly to the overarching goal and the just-completed action.

5) **Verbatim continuation snippet (programmatically inserted)**
- Include a literal placeholder for a verbatim “memory trigger” snippet to be inserted later from the most recent in-session messages:
  - `{{RAWR_VERBATIM_CONTINUATION_SNIPPET}}`

Final directive:
- End with one clear directive to immediately continue from “Next step / immediate continuation” after compaction (do not restart or re-plan from scratch).
