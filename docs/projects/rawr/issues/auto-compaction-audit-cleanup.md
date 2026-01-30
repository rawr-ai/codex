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
