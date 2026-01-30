# Auto-Compaction Audit/Cleanup

## Implementation Decisions

### Derive scratch agent name from session source or seeded fallback
- **Context:** Scratch file naming must prefer a real agent identity when available, otherwise use a stable human-ish fallback instead of `agent-codex`.
- **Options:** Use thread ID directly; add a new config field for agent name; derive from session source when present and otherwise choose a deterministic name from a fixed list.
- **Choice:** Use the session source if it is a subagent name; otherwise pick a deterministic name from a fixed list using the thread ID as seed, and only fall back to `agent-codex` if no name can be derived.
- **Rationale:** Keeps names stable per session without adding new config knobs, and avoids `agent-codex` collisions while respecting explicit subagent identities.
- **Risk:** Subagent source strings may include non-human identifiers; sanitized names may be less friendly than desired.

### Remove legacy `percent_remaining_lt` trigger knob
- **Context:** `percent_remaining_lt` is explicitly labeled as a back-compat threshold and appears in prompts/config examples.
- **Options:** Keep it as an alias; deprecate with warnings; remove and require `ready_percent_remaining_lt`.
- **Choice:** Remove the legacy key and use `ready_percent_remaining_lt` exclusively, with defaults.
- **Rationale:** No backward compatibility needed; simplifies config and eliminates ambiguous thresholds.
- **Risk:** Any lingering configs that still use only `percent_remaining_lt` will no longer affect behavior.

### Drop unused watcher-prompt artifact
- **Context:** `rawr/prompts/rawr-auto-compact-watcher.md` is not referenced in code or docs after the watcher packet moved to code assembly.
- **Options:** Keep it as future scaffolding; move it to docs; remove to reduce confusion.
- **Choice:** Remove the unused prompt file.
- **Rationale:** Eliminates dead artifacts and reduces the chance of confusing it with the active packet prompt.
- **Risk:** If we later implement a watcher-authored packet prompt, we will need to recreate or restore the template.

### Require mid-turn pre-compact packet before compaction
- **Context:** Core mid-turn RAWR compaction currently runs immediately after a sampling request, skipping the pre-compact packet step and allowing silent loops.
- **Options:** Keep mid-turn compaction immediate; add a packet-only guard; always inject an agent packet prompt before any mid-turn compaction.
- **Choice:** Always inject the agent packet prompt (with optional scratch write) before mid-turn compaction, even when config prefers watcher-authored packets.
- **Rationale:** Enforces the pre-compact request invariant and provides a usable handoff for mid-turn compactions.
- **Risk:** Adds one extra model turn during mid-turn compaction; mid-turn ignores `packet_author=watcher`.

### Rearm mid-turn compaction after token growth
- **Context:** Emergency tier can re-trigger compaction repeatedly when token usage stays high after a compact, causing loops.
- **Options:** Allow unlimited repeats; hard cap to one per turn; require token growth before re-triggering.
- **Choice:** Require a minimum token delta (`max(context_window/50, 64)`; default 256 if unknown) before another mid-turn compaction can trigger.
- **Rationale:** Prevents immediate loops while still allowing multiple compactions within a long turn.
- **Risk:** In extremely tight contexts, additional compactions may wait until token usage grows enough.

### Guard built-in auto-compaction rearm by token growth
- **Context:** Remote auto-compaction can return a history that still exceeds the auto-compact limit, causing immediate repeat compactions across turns or within a turn loop.
- **Options:** Allow repeated auto-compacts; hard cap to one per turn; require token growth before another auto-compact.
- **Choice:** Require a minimum token delta (same `max(context_window/50, 64)` threshold) before auto-compaction can re-trigger.
- **Rationale:** Prevents tight loops while still allowing multiple auto-compactions in long-running turns when usage grows again.
- **Risk:** If compaction fails to reduce usage below the limit, another compaction will not trigger until more tokens are added.

### Fallback to local compaction after ineffective remote compact
- **Context:** Remote `/responses/compact` can return history that remains above the auto-compact limit, leaving no viable path to free context.
- **Options:** Re-try remote compact; switch to local compact prompt; warn and stop compacting.
- **Choice:** After a remote compact, if usage still exceeds the auto-compact limit, run a local compact prompt once as a fallback.
- **Rationale:** Provides a reliable escape hatch without removing upstream behavior.
- **Risk:** Two back-to-back compactions can occur; local summary may be more aggressive than remote output.
