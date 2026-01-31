use codex_protocol::ThreadId;
use codex_protocol::protocol::CompactionTrigger;
use serde::Deserialize;
use serde::Serialize;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::Duration as TokioDuration;
use tokio::time::timeout;
use uuid::Uuid;

const RAWR_STORE_VERSION: u32 = 1;

const DEFAULT_GRAPHITE_MAX_CHARS: usize = 4_096;
const GRAPHITE_OBSERVATION_TIMEOUT: TokioDuration = TokioDuration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RawrBoundarySource {
    Core,
    Tool,
    Compaction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum RawrBoundaryKind {
    TurnStarted,
    PlanUpdated {
        checkpoint: bool,
    },
    Commit,
    PrCheckpoint,
    AgentDone,
    TopicShift,
    ConcludingThought,
    CompactionCompleted {
        trigger: Option<CompactionTrigger>,
        total_tokens_before: i64,
        total_tokens_after: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RawrBoundaryEvent {
    pub id: String,
    pub occurred_at_ms: i64,
    pub thread_id: ThreadId,
    pub turn_id: String,
    pub seq: u64,
    pub source: RawrBoundarySource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<RawrRepoSnapshot>,
    #[serde(flatten)]
    pub kind: RawrBoundaryKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RawrGitSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RawrGraphiteSnapshot {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RawrRepoSnapshot {
    pub git: RawrGitSnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graphite: Option<RawrGraphiteSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RawrTurnSignals {
    pub turn_id: String,
    pub saw_plan_update: bool,
    pub saw_plan_checkpoint: bool,
    pub saw_commit: bool,
    pub saw_pr_checkpoint: bool,
    pub saw_agent_done: bool,
    pub saw_topic_shift: bool,
    pub saw_concluding_thought: bool,
}

impl RawrTurnSignals {
    fn new(turn_id: &str) -> Self {
        Self {
            turn_id: turn_id.to_string(),
            saw_plan_update: false,
            saw_plan_checkpoint: false,
            saw_commit: false,
            saw_pr_checkpoint: false,
            saw_agent_done: false,
            saw_topic_shift: false,
            saw_concluding_thought: false,
        }
    }

    fn ensure_turn(&mut self, turn_id: &str) {
        if self.turn_id != turn_id {
            *self = Self::new(turn_id);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RawrLastCompaction {
    pub occurred_at_ms: i64,
    pub turn_id: String,
    pub seq: u64,
    pub total_tokens_before: i64,
    pub total_tokens_after: i64,
    pub trigger: Option<CompactionTrigger>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RawrDecisionStatus {
    Shadow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RawrDecisionAction {
    NoAction,
    ConsiderCompaction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum RawrDecisionReason {
    MissingContextWindow,
    AboveThreshold,
    BoundaryGatingNotSatisfied,
    EligibleByPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "trigger_kind", rename_all = "snake_case")]
pub(crate) enum RawrDecisionTrigger {
    BoundaryEvent { event_id: String },
    TokenPressureMidTurn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RawrCompactionDecision {
    pub id: String,
    pub occurred_at_ms: i64,
    pub thread_id: ThreadId,
    pub turn_id: String,
    pub seq: u64,
    #[serde(flatten)]
    pub trigger: RawrDecisionTrigger,
    pub status: RawrDecisionStatus,
    pub action: RawrDecisionAction,
    pub total_usage_tokens: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_context_window: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent_remaining: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_signals: Option<RawrTurnSignals>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<RawrDecisionReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RawrLastDecision {
    pub id: String,
    pub occurred_at_ms: i64,
    pub turn_id: String,
    pub seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger_event_id: Option<String>,
    pub action: RawrDecisionAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<RawrDecisionReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RawrStructuredState {
    pub version: u32,
    pub thread_id: ThreadId,
    pub updated_at_ms: i64,
    pub last_event_id: Option<String>,
    pub last_seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_turn: Option<RawrTurnSignals>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_repo: Option<RawrRepoSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_decision: Option<RawrLastDecision>,
    pub last_compaction: Option<RawrLastCompaction>,
}

impl RawrStructuredState {
    pub(crate) fn new(thread_id: ThreadId) -> Self {
        Self {
            version: RAWR_STORE_VERSION,
            thread_id,
            updated_at_ms: now_ms(),
            last_event_id: None,
            last_seq: 0,
            current_turn: None,
            last_repo: None,
            last_decision: None,
            last_compaction: None,
        }
    }

    pub(crate) fn reduce_boundary_event(&mut self, event: &RawrBoundaryEvent) {
        self.updated_at_ms = now_ms();
        self.last_event_id = Some(event.id.clone());
        self.last_seq = event.seq;

        if let Some(repo) = &event.repo {
            self.last_repo = Some(repo.clone());
        }

        match &event.kind {
            RawrBoundaryKind::TurnStarted => {
                self.current_turn = Some(RawrTurnSignals::new(event.turn_id.as_str()));
            }
            RawrBoundaryKind::PlanUpdated { checkpoint } => {
                let signals = self
                    .current_turn
                    .get_or_insert_with(|| RawrTurnSignals::new(event.turn_id.as_str()));
                signals.ensure_turn(event.turn_id.as_str());
                signals.saw_plan_update = true;
                if *checkpoint {
                    signals.saw_plan_checkpoint = true;
                }
            }
            RawrBoundaryKind::Commit => {
                let signals = self
                    .current_turn
                    .get_or_insert_with(|| RawrTurnSignals::new(event.turn_id.as_str()));
                signals.ensure_turn(event.turn_id.as_str());
                signals.saw_commit = true;
            }
            RawrBoundaryKind::PrCheckpoint => {
                let signals = self
                    .current_turn
                    .get_or_insert_with(|| RawrTurnSignals::new(event.turn_id.as_str()));
                signals.ensure_turn(event.turn_id.as_str());
                signals.saw_pr_checkpoint = true;
            }
            RawrBoundaryKind::AgentDone => {
                let signals = self
                    .current_turn
                    .get_or_insert_with(|| RawrTurnSignals::new(event.turn_id.as_str()));
                signals.ensure_turn(event.turn_id.as_str());
                signals.saw_agent_done = true;
            }
            RawrBoundaryKind::TopicShift => {
                let signals = self
                    .current_turn
                    .get_or_insert_with(|| RawrTurnSignals::new(event.turn_id.as_str()));
                signals.ensure_turn(event.turn_id.as_str());
                signals.saw_topic_shift = true;
            }
            RawrBoundaryKind::ConcludingThought => {
                let signals = self
                    .current_turn
                    .get_or_insert_with(|| RawrTurnSignals::new(event.turn_id.as_str()));
                signals.ensure_turn(event.turn_id.as_str());
                signals.saw_concluding_thought = true;
            }
            RawrBoundaryKind::CompactionCompleted { .. } => {}
        }

        if let RawrBoundaryKind::CompactionCompleted {
            trigger,
            total_tokens_before,
            total_tokens_after,
        } = &event.kind
        {
            self.last_compaction = Some(RawrLastCompaction {
                occurred_at_ms: event.occurred_at_ms,
                turn_id: event.turn_id.clone(),
                seq: event.seq,
                total_tokens_before: *total_tokens_before,
                total_tokens_after: *total_tokens_after,
                trigger: trigger.clone(),
            });
        }
    }
}

pub(crate) struct RawrStorePaths {
    pub base_dir: PathBuf,
    pub events_jsonl: PathBuf,
    pub decisions_jsonl: PathBuf,
    pub state_json: PathBuf,
}

pub(crate) fn rawr_store_paths(codex_home: &Path, thread_id: ThreadId) -> RawrStorePaths {
    let base_dir = codex_home
        .join("rawr")
        .join("auto_compaction")
        .join("threads")
        .join(thread_id.to_string());
    RawrStorePaths {
        events_jsonl: base_dir.join("events.jsonl"),
        decisions_jsonl: base_dir.join("decisions.jsonl"),
        state_json: base_dir.join("state.json"),
        base_dir,
    }
}

pub(crate) async fn append_boundary_event(
    codex_home: &Path,
    event: &RawrBoundaryEvent,
) -> std::io::Result<RawrStructuredState> {
    let paths = rawr_store_paths(codex_home, event.thread_id);
    tokio::fs::create_dir_all(&paths.base_dir).await?;

    let mut state = load_state(paths.state_json.as_path(), event.thread_id).await?;
    state.reduce_boundary_event(event);
    write_json_line(paths.events_jsonl.as_path(), event).await?;
    write_state_atomic(paths.state_json.as_path(), &state).await?;
    Ok(state)
}

pub(crate) async fn load_thread_state(
    codex_home: &Path,
    thread_id: ThreadId,
) -> std::io::Result<RawrStructuredState> {
    let paths = rawr_store_paths(codex_home, thread_id);
    load_state(paths.state_json.as_path(), thread_id).await
}

async fn load_state(path: &Path, thread_id: ThreadId) -> std::io::Result<RawrStructuredState> {
    let contents = match tokio::fs::read_to_string(path).await {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(RawrStructuredState::new(thread_id));
        }
        Err(err) => return Err(err),
    };
    serde_json::from_str::<RawrStructuredState>(&contents)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

async fn write_json_line<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    let mut line = serde_json::to_vec(value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    line.push(b'\n');
    file.write_all(&line).await?;
    file.flush().await?;
    Ok(())
}

async fn write_state_atomic(path: &Path, state: &RawrStructuredState) -> std::io::Result<()> {
    let tmp_path = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(state)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    tokio::fs::write(&tmp_path, bytes).await?;
    tokio::fs::rename(&tmp_path, path).await?;
    Ok(())
}

fn now_ms() -> i64 {
    let Ok(dur) = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) else {
        return 0;
    };
    dur.as_millis().min(i64::MAX as u128) as i64
}

pub(crate) fn new_boundary_event(
    thread_id: ThreadId,
    turn_id: &str,
    seq: u64,
    source: RawrBoundarySource,
    kind: RawrBoundaryKind,
) -> RawrBoundaryEvent {
    RawrBoundaryEvent {
        id: Uuid::new_v4().to_string(),
        occurred_at_ms: now_ms(),
        thread_id,
        turn_id: turn_id.to_string(),
        seq,
        source,
        repo: None,
        kind,
    }
}

pub(crate) fn new_compaction_decision(
    event: &RawrBoundaryEvent,
    decision_seq: u64,
    total_usage_tokens: i64,
) -> RawrCompactionDecision {
    RawrCompactionDecision {
        id: Uuid::new_v4().to_string(),
        occurred_at_ms: now_ms(),
        thread_id: event.thread_id,
        turn_id: event.turn_id.clone(),
        seq: decision_seq,
        trigger: RawrDecisionTrigger::BoundaryEvent {
            event_id: event.id.clone(),
        },
        status: RawrDecisionStatus::Shadow,
        action: RawrDecisionAction::NoAction,
        total_usage_tokens,
        model_context_window: None,
        percent_remaining: None,
        tier: None,
        turn_signals: None,
        reasons: Vec::new(),
    }
}

pub(crate) fn new_token_pressure_decision(
    thread_id: ThreadId,
    turn_id: &str,
    decision_seq: u64,
    total_usage_tokens: i64,
) -> RawrCompactionDecision {
    RawrCompactionDecision {
        id: Uuid::new_v4().to_string(),
        occurred_at_ms: now_ms(),
        thread_id,
        turn_id: turn_id.to_string(),
        seq: decision_seq,
        trigger: RawrDecisionTrigger::TokenPressureMidTurn,
        status: RawrDecisionStatus::Shadow,
        action: RawrDecisionAction::NoAction,
        total_usage_tokens,
        model_context_window: None,
        percent_remaining: None,
        tier: None,
        turn_signals: None,
        reasons: Vec::new(),
    }
}

pub(crate) async fn append_compaction_decision(
    codex_home: &Path,
    decision: &RawrCompactionDecision,
    state: &mut RawrStructuredState,
) -> std::io::Result<()> {
    let paths = rawr_store_paths(codex_home, decision.thread_id);
    tokio::fs::create_dir_all(&paths.base_dir).await?;

    write_json_line(paths.decisions_jsonl.as_path(), decision).await?;
    state.updated_at_ms = now_ms();
    state.last_decision = Some(RawrLastDecision {
        id: decision.id.clone(),
        occurred_at_ms: decision.occurred_at_ms,
        turn_id: decision.turn_id.clone(),
        seq: decision.seq,
        trigger_event_id: match &decision.trigger {
            RawrDecisionTrigger::BoundaryEvent { event_id } => Some(event_id.clone()),
            RawrDecisionTrigger::TokenPressureMidTurn => None,
        },
        action: decision.action,
        tier: decision.tier.clone(),
        reasons: decision.reasons.clone(),
    });
    write_state_atomic(paths.state_json.as_path(), state).await?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RawrRepoObservationConfig {
    pub graphite_enabled: bool,
    pub graphite_max_chars: usize,
}

impl Default for RawrRepoObservationConfig {
    fn default() -> Self {
        Self {
            graphite_enabled: false,
            graphite_max_chars: DEFAULT_GRAPHITE_MAX_CHARS,
        }
    }
}

pub(crate) fn should_observe_repo_for_boundary(kind: &RawrBoundaryKind) -> bool {
    matches!(
        kind,
        RawrBoundaryKind::TurnStarted
            | RawrBoundaryKind::PlanUpdated { .. }
            | RawrBoundaryKind::Commit
            | RawrBoundaryKind::PrCheckpoint
            | RawrBoundaryKind::CompactionCompleted { .. }
    )
}

pub(crate) async fn observe_repo_snapshot(
    cwd: &Path,
    cfg: RawrRepoObservationConfig,
    kind: &RawrBoundaryKind,
) -> Option<RawrRepoSnapshot> {
    if !should_observe_repo_for_boundary(kind) {
        return None;
    }

    let repo_root = crate::git_info::get_git_repo_root(cwd);
    let (repo_root_string, git_info) = if let Some(repo_root) = repo_root.as_deref() {
        (
            Some(repo_root.display().to_string()),
            crate::git_info::collect_git_info(repo_root).await,
        )
    } else {
        (None, None)
    };

    let git = RawrGitSnapshot {
        repo_root: repo_root_string,
        branch: git_info.as_ref().and_then(|info| info.branch.clone()),
        commit_hash: git_info.as_ref().and_then(|info| info.commit_hash.clone()),
    };

    let graphite = if cfg.graphite_enabled
        && matches!(
            kind,
            RawrBoundaryKind::TurnStarted | RawrBoundaryKind::PrCheckpoint
        )
        && let Some(repo_root) = repo_root
    {
        Some(observe_graphite(repo_root.as_path(), cfg.graphite_max_chars).await)
    } else {
        None
    };

    Some(RawrRepoSnapshot { git, graphite })
}

async fn observe_graphite(repo_root: &Path, max_chars: usize) -> RawrGraphiteSnapshot {
    let mut cmd = Command::new("gt");
    cmd.arg("status");
    cmd.current_dir(repo_root);
    cmd.env("NO_COLOR", "1");

    let output = match timeout(GRAPHITE_OBSERVATION_TIMEOUT, cmd.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(err)) => {
            return RawrGraphiteSnapshot {
                enabled: true,
                status: None,
                error: Some(err.to_string()),
            };
        }
        Err(_) => {
            return RawrGraphiteSnapshot {
                enabled: true,
                status: None,
                error: Some("timeout".to_string()),
            };
        }
    };

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let status = stdout.chars().take(max_chars).collect::<String>();
        RawrGraphiteSnapshot {
            enabled: true,
            status: (!status.is_empty()).then_some(status),
            error: None,
        }
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let error = stderr.chars().take(max_chars).collect::<String>();
        RawrGraphiteSnapshot {
            enabled: true,
            status: None,
            error: (!error.is_empty()).then_some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    fn fixed_thread_id() -> ThreadId {
        ThreadId::try_from("00000000-0000-0000-0000-000000000000").expect("valid uuid")
    }

    fn normalize_state(mut state: RawrStructuredState) -> RawrStructuredState {
        state.updated_at_ms = 0;
        state
    }

    #[tokio::test]
    async fn append_boundary_event_writes_jsonl_and_state_snapshot() {
        let tmp = TempDir::new().expect("tempdir");
        let thread_id = fixed_thread_id();

        let event = RawrBoundaryEvent {
            id: "evt-1".to_string(),
            occurred_at_ms: 123,
            thread_id,
            turn_id: "turn-1".to_string(),
            seq: 1,
            source: RawrBoundarySource::Core,
            repo: Some(RawrRepoSnapshot {
                git: RawrGitSnapshot {
                    repo_root: Some("/repo".to_string()),
                    branch: Some("main".to_string()),
                    commit_hash: Some("deadbeef".to_string()),
                },
                graphite: None,
            }),
            kind: RawrBoundaryKind::TurnStarted,
        };

        let stored = append_boundary_event(tmp.path(), &event)
            .await
            .expect("append");

        let paths = rawr_store_paths(tmp.path(), thread_id);
        let events = tokio::fs::read_to_string(paths.events_jsonl.as_path())
            .await
            .expect("read events.jsonl");
        let line = events.lines().next().expect("one event line");
        let roundtrip: RawrBoundaryEvent =
            serde_json::from_str(line).expect("deserialize RawrBoundaryEvent");
        assert_eq!(roundtrip, event);

        let state_json = tokio::fs::read_to_string(paths.state_json.as_path())
            .await
            .expect("read state.json");
        let state: RawrStructuredState =
            serde_json::from_str(&state_json).expect("deserialize RawrStructuredState");

        let mut expected = RawrStructuredState::new(thread_id);
        expected.reduce_boundary_event(&event);
        expected.updated_at_ms = state.updated_at_ms;
        assert_eq!(normalize_state(state), normalize_state(expected));
        assert_eq!(stored.thread_id, thread_id);
        assert_eq!(stored.last_event_id.as_deref(), Some("evt-1"));
        assert_eq!(stored.last_seq, 1);
    }

    #[tokio::test]
    async fn append_compaction_decision_writes_jsonl_and_updates_state_last_decision() {
        let tmp = TempDir::new().expect("tempdir");
        let thread_id = fixed_thread_id();

        let event = RawrBoundaryEvent {
            id: "evt-1".to_string(),
            occurred_at_ms: 123,
            thread_id,
            turn_id: "turn-1".to_string(),
            seq: 1,
            source: RawrBoundarySource::Core,
            repo: None,
            kind: RawrBoundaryKind::TurnStarted,
        };

        let mut state = append_boundary_event(tmp.path(), &event)
            .await
            .expect("append boundary");

        let decision = RawrCompactionDecision {
            id: "dec-1".to_string(),
            occurred_at_ms: 456,
            thread_id,
            turn_id: "turn-1".to_string(),
            seq: 1,
            trigger: RawrDecisionTrigger::BoundaryEvent {
                event_id: "evt-1".to_string(),
            },
            status: RawrDecisionStatus::Shadow,
            action: RawrDecisionAction::ConsiderCompaction,
            total_usage_tokens: 900,
            model_context_window: Some(1000),
            percent_remaining: Some(10),
            tier: Some("asap".to_string()),
            turn_signals: None,
            reasons: vec![RawrDecisionReason::EligibleByPolicy],
        };

        append_compaction_decision(tmp.path(), &decision, &mut state)
            .await
            .expect("append decision");

        let paths = rawr_store_paths(tmp.path(), thread_id);
        let decisions = tokio::fs::read_to_string(paths.decisions_jsonl.as_path())
            .await
            .expect("read decisions.jsonl");
        let line = decisions.lines().next().expect("one decision line");
        let roundtrip: RawrCompactionDecision =
            serde_json::from_str(line).expect("deserialize RawrCompactionDecision");
        assert_eq!(roundtrip, decision);

        let state_json = tokio::fs::read_to_string(paths.state_json.as_path())
            .await
            .expect("read state.json");
        let persisted: RawrStructuredState =
            serde_json::from_str(&state_json).expect("deserialize RawrStructuredState");

        assert_eq!(
            persisted.last_decision.as_ref().map(|d| d.id.as_str()),
            Some("dec-1")
        );
        assert_eq!(
            persisted
                .last_decision
                .as_ref()
                .map(|d| d.trigger_event_id.as_deref()),
            Some(Some("evt-1"))
        );
        assert_eq!(
            persisted.last_decision.as_ref().map(|d| d.action),
            Some(RawrDecisionAction::ConsiderCompaction)
        );
        assert_eq!(
            persisted.last_decision.as_ref().map(|d| d.tier.as_deref()),
            Some(Some("asap"))
        );
        assert_eq!(
            persisted
                .last_decision
                .as_ref()
                .map(|d| d.reasons.as_slice()),
            Some([RawrDecisionReason::EligibleByPolicy].as_slice())
        );
    }
}
