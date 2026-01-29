# Rawr auto-compaction handoff — exact workflows, invariants, and loop incentives

This doc is intended to be “mechanically precise”: what executes, in what order, what state/flags gate it, and where the handoff points are between:

- **Core** (`codex-rs/core`) sampling loop + compaction task execution
- **TUI** (`codex-rs/tui`) watcher state machine + post-compaction packet injection
- **Prompts** (local compaction prompt + rawr packet prompt) and what they “incentivize”

It focuses on **rawr auto-compaction**, but includes the closely related **built-in auto-compaction** and the **remote compaction** variant because they can change the effective behavior and the loop surfaces.

## Terminology

- **Turn**: a user input processed by the core `run_turn(...)` sampling loop.
- **Sampling request**: one “round trip” to the model within the same user turn; the loop can execute multiple sampling requests per user turn.
- **Compaction task**: `Op::Compact` → `TaskKind::Compact` which rewrites history (locally or via a remote provider feature).
- **Rawr mid-turn compaction**: core compaction **inside** `run_turn(...)` while `needs_follow_up == true`.
- **Rawr watcher compaction**: TUI-driven compaction at **turn completion** (and optionally “preflight” before the next user message).
- **Packet**: a “continuation context packet” injected as a synthetic user message after compaction to help the agent continue.

## Invariant: Rawr auto-compaction is feature-flag gated (both paths)

### A) Core gating (`Feature::RawrAutoCompaction`)

1. A user/agent turn starts in core `run_turn`. `codex-rs/core/src/codex.rs` (near `run_turn`).
2. Core computes `rawr_auto_compaction_enabled = sess.enabled(Feature::RawrAutoCompaction)`.
3. If disabled:
   - Core may run **built-in auto-compaction** (token-limit based) and **rawr mid-turn compaction never runs**.
   - In the sampling loop, built-in compaction can trigger when `token_limit_reached && needs_follow_up && !rawr_auto_compaction_enabled`.

### B) TUI gating (`Feature::RawrAutoCompaction`)

1. Turn completes in UI and `ChatWidget::maybe_rawr_auto_compact(...)` runs.
2. It early-returns if the feature flag is disabled.

```mermaid
graph TD
  A[User turn starts] --> B[core run_turn]
  B --> C{Rawr auto compaction enabled}
  C --> C_yes[yes]
  C_yes --> E[core rawr mid turn eligible]
  C --> C_no[no]
  C_no --> D[core built in auto compact only]

  F[Turn completes in TUI] --> G[tui maybe_rawr_auto_compact]
  G --> H{Rawr auto compaction enabled}
  H --> H_yes[yes]
  H_yes --> J[tui watcher state machine eligible]
  H --> H_no[no]
  H_no --> I[return]
```

## Invariant: There are two distinct rawr compaction workflows

These workflows are independent and can both fire over the lifetime of a thread.

### A) Core mid-turn compaction (between sampling requests)

1. Core runs the sampling loop for the current user turn (`run_turn`).
2. After each sampling request completes, core reads:
   - `needs_follow_up`
   - `sampling_request_last_agent_message`
3. If rawr enabled and the sampling request included a `last_agent_message`, core updates “semantic boundary” signals via heuristics:
   - `rawr_agent_message_looks_done`
   - `rawr_agent_message_looks_like_topic_shift`
   - `rawr_agent_message_looks_like_concluding_thought`
4. If `needs_follow_up == true`, core computes `percent_remaining` based on context window vs total usage.
5. Core reads `boundaries_required` from **config only**:
   - `rawr_auto_compaction.trigger.auto_requires_any_boundary` (defaults to `[]`).
6. Core reads the current per-turn boundary signals from the session (commit/plan/pr/etc).
7. Core decides whether to compact mid-turn via `rawr_should_compact_mid_turn(config, percent_remaining, signals, boundaries_required)`:
   - Uses **tiered thresholds** (Early/Ready/Asap/Emergency) from config.
   - Uses tier-allowed boundary sets unless `auto_requires_any_boundary` is set; emergency bypasses boundary gating.
8. If it returns `true`:
   - Core sets a “next compaction trigger” audit marker (`CompactionTrigger::AutoWatcher{...}`).
   - Core runs compaction immediately and then continues the same user-turn sampling loop (`continue;`).
9. No TUI packet injection state machine is involved in this path.

### B) TUI turn-complete watcher (and preflight)

1. A task/turn completes in the UI: `ChatWidget::on_task_complete(...)` runs.
2. It calls `maybe_rawr_auto_compact(last_agent_message)`.
3. This is a state machine over `rawr_auto_compaction_state`:
   - `Idle`
   - `AwaitingPacket{ trigger_percent_remaining }`
   - `Compacting{ packet, saw_context_compacted, saw_turn_complete, should_inject_packet, trigger_percent_remaining }`
4. If it decides to compact, it sends `Op::Compact` (which the core executes as a compaction task).
5. After compaction completes, the watcher injects a packet exactly once (depending on state + packet_author).

```mermaid
graph TD
  subgraph Core[A core mid turn compaction]
    A1[run_turn sampling loop] --> A2{needs follow up}
    A2 --> A2_yes[yes]
    A2_yes --> A3[compute percent remaining]
    A3 --> A4[read signals and boundaries required]
    A4 --> A5{should compact mid turn}
    A5 --> A5_yes[yes]
    A5_yes --> A6[set next compaction trigger AutoWatcher]
    A6 --> A7[run compaction task now]
    A7 --> A1
    A5 --> A5_no[no]
    A5_no --> A1
    A2 --> A2_no[no]
    A2_no --> A1
  end

  subgraph TUI[B TUI watcher]
    B1[on_task_complete] --> B2[maybe_rawr_auto_compact]
    B2 --> B3{state}
    B3 --> B3_idle[Idle]
    B3_idle --> B4[check thresholds and boundaries]
    B4 --> B4_compact[compact]
    B4_compact --> B5[send Op Compact]
    B5 --> B6[observe ContextCompacted and compaction completion]
    B6 --> B7[inject packet optional]
  end
```

## Invariant: Mode affects only the TUI watcher (core mid-turn ignores mode)

### A) TUI watcher honors `rawr_auto_compaction.mode`

1. TUI loads `mode` from config, defaulting to `suggest`.
2. When below threshold:
   - `tag`: prints “would compact now” only.
   - `suggest`: prints “recommend compact now” only.
   - `auto`: may actually compact.

### B) Core mid-turn compaction does not check mode

1. Core checks only `Feature::RawrAutoCompaction` and `needs_follow_up` gating.
2. So even if the config is `mode = "suggest"`, the core mid-turn path can still compact (if the thresholds + boundary gating say so).

```mermaid
graph TD
  A[percent remaining below threshold] --> B{tui mode}
  B --> B_tag[tag]
  B_tag --> C[emit info message only]
  B --> B_suggest[suggest]
  B_suggest --> D[emit suggestion only]
  B --> B_auto[auto]
  B_auto --> E[tui may send Op Compact]

  F[core needs follow up true] --> G{Rawr auto compaction enabled}
  G --> G_yes[yes]
  G_yes --> H[core may compact mid turn no mode check]
  G --> G_no[no]
  G_no --> I[core built in auto compact only]
```

## Invariant: Exactly how the TUI watcher decides “should we compact now?”

1. Preconditions (any failure returns early):
   - `Feature::RawrAutoCompaction` enabled
   - not currently running a task/turn
   - not in review mode
2. Requires token info; computes `percent_remaining`.
3. Computes a single trigger threshold (current behavior):
   - `trigger_percent_remaining = ready_percent_remaining_lt OR percent_remaining_lt OR 75`
   - Note: `early_percent_remaining_lt` and `asap_percent_remaining_lt` exist in prompt YAML, but are **not used** in this TUI decision (as of current code).
4. If `percent_remaining >= trigger_percent_remaining`, returns (no action).
5. Computes “boundary present”:
   - Uses `auto_requires_any_boundary` from prompt YAML frontmatter, overridden by config if set.
   - Maps each boundary to a boolean (commit/pr_checkpoint/plan_checkpoint/plan_update/agent_done/topic_shift/concluding/turn_complete).
6. Computes emergency:
   - `is_emergency = percent_remaining < emergency_percent_remaining_lt`
7. In `mode = "auto"` only:
   - If `!is_emergency && !has_any_required_boundary`: prints “skipping auto compact (no natural boundary)” and returns.
   - Otherwise proceeds to compact (packet_author dependent).

```mermaid
graph TD
  A[maybe_rawr_auto_compact] --> B{preconditions ok}
  B --> B_no[no]
  B_no --> C[return]
  B --> B_yes[yes]
  B_yes --> D[compute percent remaining]
  D --> E[compute trigger threshold ready or percent or 75]
  E --> F{percent remaining below trigger}
  F --> F_no[no]
  F_no --> C
  F --> F_yes[yes]
  F_yes --> G[compute has any required boundary]
  G --> H[compute is emergency]
  H --> I{mode is auto}
  I --> I_no[no]
  I_no --> J[tag or suggest info only]
  I --> I_yes[yes]
  I_yes --> K{is emergency or has boundary}
  K --> K_no[no]
  K_no --> L[skip auto compact]
  K --> K_yes[yes]
  K_yes --> M[proceed to compact packet author]
```

## Invariant: `packet_author = "watcher"` (turn-complete auto) exact steps

1. TUI reaches `mode = auto`, and gating passes.
2. If there are queued user messages:
   - Sets `rawr_auto_compaction_state = Compacting{ packet: "", should_inject_packet: false, ... }`
   - Triggers compaction and returns (no post-compact injection).
3. Else, checks whether `auto_requires_any_boundary` includes `turn_complete`:
   - If yes (and this is the first time), sets `rawr_preflight_compaction_pending = Some(percent_remaining)`, sets state back to `Idle`, and returns (defers compaction).
4. Else (normal watcher auto):
   - Builds a continuation packet string (bounded tail; includes why compaction happened).
   - Sets `rawr_auto_compaction_state = Compacting{ packet, should_inject_packet: true, saw_context_compacted: false, saw_turn_complete: false, ... }`
   - Calls `rawr_trigger_compact()`.
5. `rawr_trigger_compact()`:
   - Records `CompactionTrigger::AutoWatcher{...}` for audit attribution (the upcoming compaction task).
   - Clears token usage and sends `AppEvent::CodexOp(Op::Compact)`.
6. Core receives `Op::Compact` and runs `codex::compact(...)`.
   - If the pending trigger is `AutoWatcher`, core uses a special compaction turn-context (model/effort/verbosity overrides).
7. Core runs the compaction task; when it finishes, the UI receives `EventMsg::ContextCompacted`.
8. On `ContextCompacted`:
   - Sets `saw_context_compacted = true` in the state (or injects immediately if `saw_turn_complete` was already true).
9. On the subsequent `on_task_complete` for the compaction task:
   - The watcher sees `Compacting{ saw_context_compacted: true, should_inject_packet: true }` and injects exactly once:
     - `rawr_inject_post_compact_packet(packet)`
     - `rawr_submit_injected_user_turn(packet)` → sends `Op::UserTurn`

```mermaid
graph TD
  A[tui mode auto watcher] --> B{queued user messages}
  B --> B_yes[yes]
  B_yes --> C[state Compacting should_inject false]
  C --> D[send Op Compact]
  B --> B_no[no]
  B_no --> E{turn_complete boundary required}
  E --> E_yes[yes]
  E_yes --> F[set preflight_pending return]
  E --> E_no[no]
  E_no --> G[build packet state Compacting should_inject true]
  G --> D
  D --> H[core run compaction task]
  H --> I[tui ContextCompacted]
  I --> J[mark saw_context_compacted]
  H --> K[tui on_task_complete compaction]
  K --> L{should inject packet}
  L --> L_yes[yes]
  L_yes --> M[inject packet once send Op UserTurn]
  L --> L_no[no]
  L_no --> N[state Idle]
```

## Invariant: `packet_author = "agent"` (turn-complete auto) exact steps

1. TUI reaches `mode = auto`, gating passes, `packet_author = agent`.
2. Sets `rawr_auto_compaction_state = AwaitingPacket{ trigger_percent_remaining }`.
3. Injects the “agent packet prompt” as a synthetic user turn (`Op::UserTurn`):
   - Prompt body comes from `rawr/prompts/rawr-auto-compact.md` (Markdown body; YAML frontmatter is settings).
   - If the embedded prompt cannot be loaded, it falls back to `default_rawr_agent_packet_prompt()`.
4. The agent responds in the next turn; when that agent turn completes, `maybe_rawr_auto_compact` runs again.
5. It consumes the `AwaitingPacket` state:
   - Takes `last_agent_message` (or fallback text), stores it as `packet`.
   - Transitions to `Compacting{ should_inject_packet: true, saw_context_compacted: false, ... }`.
   - Triggers `Op::Compact`.
6. After compaction completes, the watcher injects the stored packet once (same injection mechanics as the watcher-authored flow).

```mermaid
graph TD
  A[tui mode auto packet_author agent] --> B[state AwaitingPacket]
  B --> C[inject agent packet prompt send Op UserTurn]
  C --> D[agent replies with packet text]
  D --> E[tui on_task_complete runs maybe_rawr_auto_compact]
  E --> F[consume AwaitingPacket set packet]
  F --> G[state Compacting should_inject true]
  G --> H[send Op Compact]
  H --> I[core run compaction task]
  I --> J[tui ContextCompacted and compaction completion]
  J --> K[inject stored packet once send Op UserTurn]
```

## Invariant: `turn_complete` boundary preflight (defer until next user message) exact steps

1. TUI watcher (in `auto` + `packet_author=watcher`) sees `auto_requires_any_boundary` contains `turn_complete`.
2. It sets `rawr_preflight_compaction_pending = Some(percent_remaining)` and returns (no compaction yet).
3. Later, when the user submits a message:
   - In `submit_user_message`, if `rawr_preflight_compaction_pending` is set and the message is not a local shell command (`!...`), the TUI:
     - pushes that user message back into the queue,
     - triggers compaction first,
     - returns.
4. After compaction completes, `maybe_send_next_queued_input()` submits the queued message as the next user turn.

```mermaid
graph TD
  A[tui maybe_rawr_auto_compact] --> B{turn_complete required boundary}
  B --> B_yes[yes]
  B_yes --> C[preflight_pending set percent remaining]
  C --> D[return no compaction yet]
  D --> E[user submits message]
  E --> F{preflight_pending and not local cmd}
  F --> F_yes[yes]
  F_yes --> G[queue user message send Op Compact]
  G --> H[core run compaction task]
  H --> I[tui after compaction send queued message]
  F --> F_no[no]
  F_no --> J[normal submit path]
```

## Invariant: What compaction actually does to the thread (history rewrite) — exact steps

This applies to: manual compact, built-in auto-compact, rawr mid-turn, and rawr watcher.

### Local compaction task (uses a prompt)

1. A compaction task runs `run_compact_task_inner(...)` (`codex-rs/core/src/compact.rs`).
2. It takes and clears the pending `compaction_trigger` (audit metadata) for this thread.
3. It records the compaction prompt input in the session history, then streams the compaction prompt to the model; output items are recorded into history.
4. It builds:
   - `summary_suffix = get_last_assistant_message_from_turn(history_items)`
   - `summary_text = SUMMARY_PREFIX + "\n" + summary_suffix`
5. It builds a new history:
   - initial context items
   - a bounded set of recent user messages
   - `summary_text` as a user message
   - plus any ghost snapshots
6. It replaces session history with that compacted history and recomputes token usage.
7. It persists `RolloutItem::Compacted{ trigger: compaction_trigger }`.

### Remote compaction task (provider-controlled; no local prompt control)

1. If the provider is OpenAI and `Feature::RemoteCompaction` is enabled, `TaskKind::Compact` uses the remote path.
2. The client performs `compact_conversation_history(&prompt)` and returns a rewritten history.
3. The session replaces history and recomputes tokens; the persisted `CompactedItem` stores `replacement_history` (and the `message` is empty).

```mermaid
graph TD
  A[Op Compact] --> B[TaskKind Compact]
  B --> C{remote compaction enabled and provider supports}
  C --> C_yes[yes]
  C_yes --> D[remote compact conversation history]
  D --> E[replace history recompute tokens]
  E --> F[persist Compacted replacement_history]
  C --> C_no[no]
  C_no --> G[local run_compact_task_inner]
  G --> H[take compaction trigger]
  H --> I[stream compaction prompt record output]
  I --> J[build summary_text]
  J --> K[rewrite history to compacted form]
  K --> L[recompute tokens persist Compacted message]
```

## Variant / invariant: Can we modify “the compaction prompt we don’t control”?

It depends on whether you’re using **local** or **remote** compaction.

- **Local compaction**:
  - Uses `turn_context.compact_prompt()` as the synthesized input text for the compaction turn.
  - That prompt is configurable (config override or prompt-file override, depending on how the config is resolved).
- **Remote compaction**:
  - Does **not** use `turn_context.compact_prompt()`; it calls the provider’s “compact conversation history” operation.
  - In this mode, the “compaction prompt” is effectively provider-controlled; the local code does not send a user-editable compaction prompt string.

Practical implication:
- If you need to directly control the compaction prompt content, you must use **local** compaction (`Feature::RemoteCompaction` disabled).

```mermaid
graph TD
  A[Need editable compaction prompt] --> B{remote compaction enabled}
  B --> B_yes[yes]
  B_yes --> C[No provider controls behavior]
  B --> B_no[no]
  B_no --> D[Yes local uses turn_context compact_prompt]
```

## Prompt incentives and loop surfaces (why this can “want to loop”)

This section is not a claim that loops are guaranteed; it is a map of the incentives and feedback edges that can create repeated compactions.

### 1) Core mid-turn loop edge: “compact, then continue the same turn”

- In the sampling loop, core can:
  - trigger compaction when `needs_follow_up == true`,
  - then `continue;` the same loop.
- If compaction doesn’t sufficiently increase `percent_remaining` (or if a later sampling request re-consumes context rapidly), the mid-turn decision can evaluate `true` again.

### 2) Semantic-boundary heuristics can be “self-fulfilling” via prompts

Core’s semantic boundaries include:
- `rawr_agent_message_looks_like_concluding_thought` matches text like “next steps”.
- `rawr_agent_message_looks_done` matches “done”, “completed”, etc.

Rawr’s **agent packet prompt** explicitly asks for a structured packet including “next steps”, and many “packet-like” responses commonly include “completed” lists.

Consequence:
- The **act of generating a packet** can set semantic boundaries that later **unlock compaction**, especially in the Asap tier where `ConcludingThought` and `AgentDone` are allowed.

### 3) TUI watcher loop edge: “inject packet → run another user turn”

Watcher injection sends an `Op::UserTurn` after compaction, which creates more history items and triggers another agent response.

If after that turn the thread is still below the watcher threshold, the watcher can compact again at the next `on_task_complete`.

## Appendix: Key entrypoints and artifacts

- Core sampling loop: `codex-rs/core/src/codex.rs` (`run_turn`)
- Core boundary heuristics + mid-turn decision: `codex-rs/core/src/rawr_auto_compaction.rs`
- TUI watcher state machine: `codex-rs/tui/src/chatwidget.rs` (`maybe_rawr_auto_compact*`)
- Rawr packet prompt file: `rawr/prompts/rawr-auto-compact.md` (frontmatter + body)
- Local compaction logic: `codex-rs/core/src/compact.rs`
- Remote compaction logic: `codex-rs/core/src/compact_remote.rs`
