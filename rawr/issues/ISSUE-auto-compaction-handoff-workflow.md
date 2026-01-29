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
  subgraph ART[Non code triggers]
    A[User starts a turn]
    F[UI reports turn complete]
  end

  subgraph CLI[Codex CLI process]
    B[core run_turn sampling loop]
    C{Rawr auto compaction feature on}
    D[CLI built in auto compact only]
    E[Rawr mid turn compaction logic active]

    G[rawr watcher entry maybe_rawr_auto_compact]
    H{Rawr auto compaction feature on}
    I[return do nothing]
    J[Rawr watcher state machine active]
  end

  A --> B
  B --> C
  C --> C_yes[yes]
  C_yes --> E
  C --> C_no[no]
  C_no --> D

  F --> G
  G --> H
  H --> H_yes[yes]
  H_yes --> J
  H --> H_no[no]
  H_no --> I

  R1[Potential failure mismatched feature gating]
  C -.-> R1
  H -.-> R1

  subgraph Legend[Legend]
    L1[CLI native]
    L2[Rawr custom code]
    L3[Glue or orchestration]
    L4[OpenAI backend]
    L5[Non code artifact]
    L6[Handoff or risk]
  end

  classDef native fill:#E3F2FD,stroke:#1E88E5,color:#0D47A1;
  classDef backend fill:#F3E5F5,stroke:#8E24AA,color:#4A148C;
  classDef custom fill:#E8F5E9,stroke:#43A047,color:#1B5E20;
  classDef customFn fill:#C8E6C9,stroke:#2E7D32,stroke-width:2px,color:#1B5E20;
  classDef glue fill:#FFF3E0,stroke:#FB8C00,color:#E65100;
  classDef artifact fill:#FAFAFA,stroke:#757575,stroke-dasharray:4 3,color:#424242;
  classDef handoff fill:#FFEBEE,stroke:#E53935,stroke-width:2px,color:#B71C1C;
  classDef risk fill:#FFCDD2,stroke:#C62828,stroke-width:2px,stroke-dasharray:2 2,color:#B71C1C;

  class A,F artifact;
  class B,D native;
  class C,H,C_yes,C_no,H_yes,H_no glue;
  class E,J custom;
  class G customFn;
  class I glue;
  class R1 risk;

  class L1 native;
  class L2 custom;
  class L3 glue;
  class L4 backend;
  class L5 artifact;
  class L6 handoff;

  style CLI fill:#ffffff,stroke:#1E88E5,stroke-width:2px;
  style ART fill:#ffffff,stroke:#757575,stroke-width:1px,stroke-dasharray:4 3;
  style Legend fill:#ffffff,stroke:#757575,stroke-width:1px,stroke-dasharray:4 3;
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
  subgraph CLI[Codex CLI process]
    subgraph Core[core mid turn compaction]
      A1[core run_turn sampling loop]
      A2{needs follow up}
      A3[compute percent remaining]
      A4[read signals and boundaries required]
      A5{should compact mid turn}
      A6[set next compaction trigger AutoWatcher]
      A7[run compaction task now]
    end

    subgraph TUI[TUI watcher]
      B1[TUI on_task_complete]
      B2[Rawr watcher maybe_rawr_auto_compact]
      B3{watcher state}
      B4[check thresholds and boundaries]
      B5[handoff send Op Compact]
      B6[observe ContextCompacted and compaction completion]
      B7[inject packet optional]
    end
  end

  subgraph BACKEND[OpenAI backend]
    O1[model responds to sampling request]
  end

  subgraph ART[Non code artifacts]
    CFG1[rawr_auto_compaction config]
  end

  A1 --> H1[handoff sampling request] --> O1
  O1 --> H2[handoff sampling result] --> A2
  A2 --> A2_yes[yes] --> A3
  A2 --> A2_no[no] --> A1
  A3 --> A4
  CFG1 -.-> A4
  A4 --> A5
  A5 --> A5_yes[yes] --> A6 --> A7 --> A1
  A5 --> A5_no[no] --> A1

  B1 --> B2 --> B3
  B3 --> B3_idle[Idle] --> B4 --> B5 --> A7
  A7 --> H3[handoff ContextCompacted event] --> B6 --> B7

  R2[Risk compaction then continue same turn can repeat]
  A7 -.-> R2

  subgraph Legend[Legend]
    L1[CLI native]
    L2[Rawr custom function]
    L3[Glue or orchestration]
    L4[OpenAI backend]
    L5[Non code artifact]
    L6[Handoff]
    L7[Risk point]
  end

  classDef native fill:#E3F2FD,stroke:#1E88E5,color:#0D47A1;
  classDef backend fill:#F3E5F5,stroke:#8E24AA,color:#4A148C;
  classDef custom fill:#E8F5E9,stroke:#43A047,color:#1B5E20;
  classDef customFn fill:#C8E6C9,stroke:#2E7D32,stroke-width:2px,color:#1B5E20;
  classDef glue fill:#FFF3E0,stroke:#FB8C00,color:#E65100;
  classDef artifact fill:#FAFAFA,stroke:#757575,stroke-dasharray:4 3,color:#424242;
  classDef handoff fill:#FFEBEE,stroke:#E53935,stroke-width:2px,color:#B71C1C;
  classDef risk fill:#FFCDD2,stroke:#C62828,stroke-width:2px,stroke-dasharray:2 2,color:#B71C1C;

  class A1,A2,A7,B1,B6 native;
  class B2 customFn;
  class A5,B3 custom;
  class A3,A4,B4 glue;
  class CFG1 artifact;
  class O1 backend;
  class H1,H2,H3,B5,A5_yes,A5_no,A2_yes,A2_no,B3_idle handoff;
  class R2 risk;

  class L1 native;
  class L2 customFn;
  class L3 glue;
  class L4 backend;
  class L5 artifact;
  class L6 handoff;
  class L7 risk;

  style CLI fill:#ffffff,stroke:#1E88E5,stroke-width:2px;
  style BACKEND fill:#ffffff,stroke:#8E24AA,stroke-width:2px;
  style ART fill:#ffffff,stroke:#757575,stroke-width:1px,stroke-dasharray:4 3;
  style Legend fill:#ffffff,stroke:#757575,stroke-width:1px,stroke-dasharray:4 3;
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
  subgraph ART[Non code artifacts]
    CFG2[rawr_auto_compaction mode]
  end

  subgraph CLI[Codex CLI process]
    A[percent remaining below threshold]

    B{TUI watcher mode}
    C[emit info message only]
    D[emit suggestion only]
    E[handoff send Op Compact]

    F[core needs follow up true]
    G{Rawr auto compaction enabled}
    H[core may compact mid turn ignores mode]
    I[core built in auto compact only]
  end

  CFG2 -.-> B
  A --> B
  B --> B_tag[tag] --> C
  B --> B_suggest[suggest] --> D
  B --> B_auto[auto] --> E

  F --> G
  G --> G_yes[yes] --> H
  G --> G_no[no] --> I

  R3[Potential confusion mode affects TUI only]
  B -.-> R3
  G -.-> R3

  subgraph Legend[Legend]
    L1[CLI native]
    L2[Rawr custom code]
    L3[Glue or orchestration]
    L4[Non code artifact]
    L5[Handoff]
    L6[Risk point]
  end

  classDef native fill:#E3F2FD,stroke:#1E88E5,color:#0D47A1;
  classDef custom fill:#E8F5E9,stroke:#43A047,color:#1B5E20;
  classDef customFn fill:#C8E6C9,stroke:#2E7D32,stroke-width:2px,color:#1B5E20;
  classDef glue fill:#FFF3E0,stroke:#FB8C00,color:#E65100;
  classDef artifact fill:#FAFAFA,stroke:#757575,stroke-dasharray:4 3,color:#424242;
  classDef handoff fill:#FFEBEE,stroke:#E53935,stroke-width:2px,color:#B71C1C;
  classDef risk fill:#FFCDD2,stroke:#C62828,stroke-width:2px,stroke-dasharray:2 2,color:#B71C1C;

  class A,F,H,I native;
  class B,G custom;
  class C,D glue;
  class E,B_tag,B_suggest,B_auto,G_yes,G_no handoff;
  class CFG2 artifact;
  class R3 risk;

  class L1 native;
  class L2 custom;
  class L3 glue;
  class L4 artifact;
  class L5 handoff;
  class L6 risk;

  style CLI fill:#ffffff,stroke:#1E88E5,stroke-width:2px;
  style ART fill:#ffffff,stroke:#757575,stroke-width:1px,stroke-dasharray:4 3;
  style Legend fill:#ffffff,stroke:#757575,stroke-width:1px,stroke-dasharray:4 3;
```

## Invariant: Exactly how the TUI watcher decides “should we compact now?”

1. Preconditions (any failure returns early):
   - `Feature::RawrAutoCompaction` enabled
   - not currently running a task/turn
   - not in review mode
2. Requires token info; computes `percent_remaining`.
3. Computes a single trigger threshold (current behavior):
   - `trigger_percent_remaining = ready_percent_remaining_lt OR 75`
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
  subgraph ART[Non code artifacts]
    CFG3[thresholds and required boundaries]
    TOK1[token usage snapshot]
    MODE1[watcher mode]
  end

  subgraph CLI[Codex CLI process]
    A[Rawr watcher maybe_rawr_auto_compact]
    B{preconditions ok}
    C[return]
    D[compute percent remaining]
    E[compute trigger threshold]
    F{percent remaining below trigger}
    G[compute has any required boundary]
    H[compute is emergency]
    I{mode is auto}
    J[tag or suggest info only]
    L[skip auto compact]
    M[proceed to compact choose packet author]
    K{is emergency or has boundary}
  end

  CFG3 -.-> E
  CFG3 -.-> G
  TOK1 -.-> D
  MODE1 -.-> I

  A --> B
  B --> B_no[no] --> C
  B --> B_yes[yes] --> D --> E --> F
  F --> F_no[no] --> C
  F --> F_yes[yes] --> G --> H --> I
  I --> I_no[no] --> J
  I --> I_yes[yes] --> K
  K --> K_no[no] --> L
  K --> K_yes[yes] --> M

  R4[Potential failure token usage missing or stale]
  TOK1 -.-> R4
  D -.-> R4

  R5[Potential confusion thresholds come from config and prompt frontmatter]
  CFG3 -.-> R5

  subgraph Legend[Legend]
    L1[CLI native]
    L2[Rawr custom function]
    L3[Glue or orchestration]
    L4[Non code artifact]
    L5[Handoff]
    L6[Risk point]
  end

  classDef native fill:#E3F2FD,stroke:#1E88E5,color:#0D47A1;
  classDef custom fill:#E8F5E9,stroke:#43A047,color:#1B5E20;
  classDef customFn fill:#C8E6C9,stroke:#2E7D32,stroke-width:2px,color:#1B5E20;
  classDef glue fill:#FFF3E0,stroke:#FB8C00,color:#E65100;
  classDef artifact fill:#FAFAFA,stroke:#757575,stroke-dasharray:4 3,color:#424242;
  classDef handoff fill:#FFEBEE,stroke:#E53935,stroke-width:2px,color:#B71C1C;
  classDef risk fill:#FFCDD2,stroke:#C62828,stroke-width:2px,stroke-dasharray:2 2,color:#B71C1C;

  class A customFn;
  class B,F,I,K custom;
  class D,E,G,H glue;
  class C,J,L,M native;
  class B_yes,B_no,F_yes,F_no,I_yes,I_no,K_yes,K_no handoff;
  class CFG3,TOK1,MODE1 artifact;
  class R4,R5 risk;

  class L1 native;
  class L2 customFn;
  class L3 glue;
  class L4 artifact;
  class L5 handoff;
  class L6 risk;

  style CLI fill:#ffffff,stroke:#1E88E5,stroke-width:2px;
  style ART fill:#ffffff,stroke:#757575,stroke-width:1px,stroke-dasharray:4 3;
  style Legend fill:#ffffff,stroke:#757575,stroke-width:1px,stroke-dasharray:4 3;
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
  subgraph ART[Non code artifacts]
    Q1[queued user messages]
    CFG4[required boundary list]
    PKT1[watcher built continuation packet]
  end

  subgraph CLI[Codex CLI process]
    T1[Rawr watcher evaluate auto compact]
    B{queued user messages}
    S1[set state Compacting no inject]
    E{turn_complete boundary required}
    P1[set preflight_pending and return]
    G[build packet and set state Compacting inject]
    H1[handoff send Op Compact]

    C1[core runs compaction task]
    H2[handoff ContextCompacted event]
    J[mark saw_context_compacted in watcher state]
    K[TUI on_task_complete for compaction]
    L{should inject packet}
    H3[handoff inject synthetic user turn]
    N[set state Idle]
  end

  Q1 -.-> B
  CFG4 -.-> E
  PKT1 -.-> G

  T1 --> B
  B --> B_yes[yes] --> S1 --> H1
  B --> B_no[no] --> E
  E --> E_yes[yes] --> P1 --> N
  E --> E_no[no] --> G --> H1

  H1 --> C1 --> H2 --> J
  C1 --> K --> L
  L --> L_yes[yes] --> H3 --> N
  L --> L_no[no] --> N

  R6[Potential failure watcher state and events out of sync]
  H2 -.-> R6
  K -.-> R6
  H3 -.-> R6

  subgraph Legend[Legend]
    L1[CLI native]
    L2[Rawr custom function]
    L3[Glue or orchestration]
    L4[Non code artifact]
    L5[Handoff]
    L6[Risk point]
  end

  classDef native fill:#E3F2FD,stroke:#1E88E5,color:#0D47A1;
  classDef custom fill:#E8F5E9,stroke:#43A047,color:#1B5E20;
  classDef customFn fill:#C8E6C9,stroke:#2E7D32,stroke-width:2px,color:#1B5E20;
  classDef glue fill:#FFF3E0,stroke:#FB8C00,color:#E65100;
  classDef artifact fill:#FAFAFA,stroke:#757575,stroke-dasharray:4 3,color:#424242;
  classDef handoff fill:#FFEBEE,stroke:#E53935,stroke-width:2px,color:#B71C1C;
  classDef risk fill:#FFCDD2,stroke:#C62828,stroke-width:2px,stroke-dasharray:2 2,color:#B71C1C;

  class Q1,CFG4,PKT1 artifact;
  class T1 customFn;
  class B,E,L custom;
  class S1,G,P1,J glue;
  class C1,K,N native;
  class H1,H2,H3,B_yes,B_no,E_yes,E_no,L_yes,L_no handoff;
  class R6 risk;

  class L1 native;
  class L2 customFn;
  class L3 glue;
  class L4 artifact;
  class L5 handoff;
  class L6 risk;

  style CLI fill:#ffffff,stroke:#1E88E5,stroke-width:2px;
  style ART fill:#ffffff,stroke:#757575,stroke-width:1px,stroke-dasharray:4 3;
  style Legend fill:#ffffff,stroke:#757575,stroke-width:1px,stroke-dasharray:4 3;
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
  subgraph ART[Non code artifacts]
    PROMPT1[rawr agent packet prompt]
  end

  subgraph BACKEND[OpenAI backend]
    O1[model returns packet text]
  end

  subgraph CLI[Codex CLI process]
    A[Rawr watcher mode auto packet_author agent]
    B[set state AwaitingPacket]
    H1[handoff inject prompt as synthetic user turn]
    H2[handoff sampling request to OpenAI]
    H3[handoff agent output back to CLI]
    E[TUI on_task_complete runs maybe_rawr_auto_compact]
    F[consume AwaitingPacket store packet]
    G[set state Compacting should inject]
    H4[handoff send Op Compact]
    I[core runs compaction task]
    J[watcher observes ContextCompacted and completion]
    H5[handoff inject stored packet as user turn]
  end

  PROMPT1 -.-> H1
  A --> B --> H1 --> H2 --> O1 --> H3 --> E --> F --> G --> H4 --> I --> J --> H5

  R7[Potential failure packet content can be malformed or too long]
  O1 -.-> R7
  H5 -.-> R7

  R8[Potential loop packet turn can trigger watcher again]
  H5 -.-> R8
  A -.-> R8

  subgraph Legend[Legend]
    L1[CLI native]
    L2[Rawr custom function]
    L3[Glue or orchestration]
    L4[OpenAI backend]
    L5[Non code artifact]
    L6[Handoff]
    L7[Risk point]
  end

  classDef native fill:#E3F2FD,stroke:#1E88E5,color:#0D47A1;
  classDef backend fill:#F3E5F5,stroke:#8E24AA,color:#4A148C;
  classDef custom fill:#E8F5E9,stroke:#43A047,color:#1B5E20;
  classDef customFn fill:#C8E6C9,stroke:#2E7D32,stroke-width:2px,color:#1B5E20;
  classDef glue fill:#FFF3E0,stroke:#FB8C00,color:#E65100;
  classDef artifact fill:#FAFAFA,stroke:#757575,stroke-dasharray:4 3,color:#424242;
  classDef handoff fill:#FFEBEE,stroke:#E53935,stroke-width:2px,color:#B71C1C;
  classDef risk fill:#FFCDD2,stroke:#C62828,stroke-width:2px,stroke-dasharray:2 2,color:#B71C1C;

  class PROMPT1 artifact;
  class O1 backend;
  class A customFn;
  class B,F,G glue;
  class I,J native;
  class E custom;
  class H1,H2,H3,H4,H5 handoff;
  class R7,R8 risk;

  class L1 native;
  class L2 customFn;
  class L3 glue;
  class L4 backend;
  class L5 artifact;
  class L6 handoff;
  class L7 risk;

  style CLI fill:#ffffff,stroke:#1E88E5,stroke-width:2px;
  style BACKEND fill:#ffffff,stroke:#8E24AA,stroke-width:2px;
  style ART fill:#ffffff,stroke:#757575,stroke-width:1px,stroke-dasharray:4 3;
  style Legend fill:#ffffff,stroke:#757575,stroke-width:1px,stroke-dasharray:4 3;
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
  subgraph ART[Non code artifacts]
    CFG5[required boundary list includes turn_complete]
    UM1[user message]
  end

  subgraph CLI[Codex CLI process]
    A[TUI watcher maybe_rawr_auto_compact]
    B{turn_complete required boundary}
    C[set preflight_pending store percent remaining]
    D[return no compaction yet]
    E[user submits message]
    F{preflight_pending and not local cmd}
    Q1[queue user message]
    H1[handoff send Op Compact before user turn]
    H[core runs compaction task]
    H2[handoff after compaction submit queued message]
    J[normal submit path]
  end

  CFG5 -.-> B
  UM1 -.-> E

  A --> B
  B --> B_yes[yes] --> C --> D --> E --> F
  F --> F_yes[yes] --> Q1 --> H1 --> H --> H2 --> J
  F --> F_no[no] --> J

  R9[Potential failure user expects message to run but it is queued]
  Q1 -.-> R9
  H1 -.-> R9

  subgraph Legend[Legend]
    L1[CLI native]
    L2[Rawr custom function]
    L3[Glue or orchestration]
    L4[Non code artifact]
    L5[Handoff]
    L6[Risk point]
  end

  classDef native fill:#E3F2FD,stroke:#1E88E5,color:#0D47A1;
  classDef custom fill:#E8F5E9,stroke:#43A047,color:#1B5E20;
  classDef customFn fill:#C8E6C9,stroke:#2E7D32,stroke-width:2px,color:#1B5E20;
  classDef glue fill:#FFF3E0,stroke:#FB8C00,color:#E65100;
  classDef artifact fill:#FAFAFA,stroke:#757575,stroke-dasharray:4 3,color:#424242;
  classDef handoff fill:#FFEBEE,stroke:#E53935,stroke-width:2px,color:#B71C1C;
  classDef risk fill:#FFCDD2,stroke:#C62828,stroke-width:2px,stroke-dasharray:2 2,color:#B71C1C;

  class CFG5,UM1 artifact;
  class A customFn;
  class B,F custom;
  class C,D,Q1 glue;
  class E,J,H native;
  class H1,H2,B_yes,F_yes,F_no handoff;
  class R9 risk;

  class L1 native;
  class L2 customFn;
  class L3 glue;
  class L4 artifact;
  class L5 handoff;
  class L6 risk;

  style CLI fill:#ffffff,stroke:#1E88E5,stroke-width:2px;
  style ART fill:#ffffff,stroke:#757575,stroke-width:1px,stroke-dasharray:4 3;
  style Legend fill:#ffffff,stroke:#757575,stroke-width:1px,stroke-dasharray:4 3;
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
  subgraph ART[Non code artifacts]
    CFG6[feature RemoteCompaction and provider selection]
    PROMPT2[compaction prompt text local only]
  end

  subgraph CLI[Codex CLI process]
    A[Op Compact event]
    B[TaskKind Compact]
    C{remote compaction enabled and provider supports}

    R1[remote compaction RPC]
    L1[local compaction task inner]

    H1[take compaction trigger]
    H2[rewrite history and recompute tokens]
    P1[persist Compacted item]
  end

  subgraph BACKEND[OpenAI backend]
    O1[remote compact conversation history]
    O2[local compaction model summary]
  end

  CFG6 -.-> C
  PROMPT2 -.-> L1

  A --> B --> C
  C --> C_yes[yes] --> H3[handoff remote compact] --> O1 --> H4[handoff replacement history] --> H2 --> P1
  C --> C_no[no] --> L1 --> H1 --> H5[handoff send prompt to model] --> O2 --> H6[handoff model summary] --> H2 --> P1

  R10[Potential failure backend returns invalid replacement history]
  O1 -.-> R10
  H4 -.-> R10

  subgraph Legend[Legend]
    Lg1[CLI native]
    Lg2[Rawr custom code]
    Lg3[Glue or orchestration]
    Lg4[OpenAI backend]
    Lg5[Non code artifact]
    Lg6[Handoff]
    Lg7[Risk point]
  end

  classDef native fill:#E3F2FD,stroke:#1E88E5,color:#0D47A1;
  classDef backend fill:#F3E5F5,stroke:#8E24AA,color:#4A148C;
  classDef custom fill:#E8F5E9,stroke:#43A047,color:#1B5E20;
  classDef customFn fill:#C8E6C9,stroke:#2E7D32,stroke-width:2px,color:#1B5E20;
  classDef glue fill:#FFF3E0,stroke:#FB8C00,color:#E65100;
  classDef artifact fill:#FAFAFA,stroke:#757575,stroke-dasharray:4 3,color:#424242;
  classDef handoff fill:#FFEBEE,stroke:#E53935,stroke-width:2px,color:#B71C1C;
  classDef risk fill:#FFCDD2,stroke:#C62828,stroke-width:2px,stroke-dasharray:2 2,color:#B71C1C;

  class CFG6,PROMPT2 artifact;
  class A,B native;
  class C custom;
  class L1,H1,H2,R1 glue;
  class O1,O2 backend;
  class C_yes,C_no,H3,H4,H5,H6 handoff;
  class P1 native;
  class R10 risk;

  class Lg1 native;
  class Lg2 custom;
  class Lg3 glue;
  class Lg4 backend;
  class Lg5 artifact;
  class Lg6 handoff;
  class Lg7 risk;

  style CLI fill:#ffffff,stroke:#1E88E5,stroke-width:2px;
  style BACKEND fill:#ffffff,stroke:#8E24AA,stroke-width:2px;
  style ART fill:#ffffff,stroke:#757575,stroke-width:1px,stroke-dasharray:4 3;
  style Legend fill:#ffffff,stroke:#757575,stroke-width:1px,stroke-dasharray:4 3;
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
  subgraph ART[Non code artifacts]
    CFG7[Feature RemoteCompaction toggle]
    PROMPT3[editable prompt string local only]
  end

  subgraph CLI[Codex CLI process]
    A[Need editable compaction prompt]
    B{remote compaction enabled}
    C[No provider controls behavior]
    D[Yes local uses turn_context compact_prompt]
  end

  CFG7 -.-> B
  PROMPT3 -.-> D

  A --> B
  B --> B_yes[yes] --> C
  B --> B_no[no] --> D

  R11[Failure mode remote compaction hides prompt control]
  B -.-> R11

  subgraph Legend[Legend]
    L1[CLI native]
    L2[Non code artifact]
    L3[Handoff]
    L4[Risk point]
  end

  classDef native fill:#E3F2FD,stroke:#1E88E5,color:#0D47A1;
  classDef artifact fill:#FAFAFA,stroke:#757575,stroke-dasharray:4 3,color:#424242;
  classDef handoff fill:#FFEBEE,stroke:#E53935,stroke-width:2px,color:#B71C1C;
  classDef risk fill:#FFCDD2,stroke:#C62828,stroke-width:2px,stroke-dasharray:2 2,color:#B71C1C;
  classDef glue fill:#FFF3E0,stroke:#FB8C00,color:#E65100;

  class A,C,D native;
  class B glue;
  class B_yes,B_no handoff;
  class CFG7,PROMPT3 artifact;
  class R11 risk;

  class L1 native;
  class L2 artifact;
  class L3 handoff;
  class L4 risk;

  style CLI fill:#ffffff,stroke:#1E88E5,stroke-width:2px;
  style ART fill:#ffffff,stroke:#757575,stroke-width:1px,stroke-dasharray:4 3;
  style Legend fill:#ffffff,stroke:#757575,stroke-width:1px,stroke-dasharray:4 3;
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
