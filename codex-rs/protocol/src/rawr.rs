use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, TS)]
pub struct RawrAutoCompactionJudgmentResultEvent {
    /// Correlates this result event with the originating request.
    pub request_id: String,
    /// Tier name (`early`/`ready`/`asap`/`emergency`).
    pub tier: String,
    pub should_compact: bool,
    pub reason: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CompactionTrigger {
    AutoWatcher {
        /// Remaining context window percent at the time the watcher triggered compaction.
        trigger_percent_remaining: i64,
        /// Whether the watcher observed a successful `git commit` boundary this turn.
        saw_commit: bool,
        /// Whether the watcher observed a plan checkpoint boundary this turn.
        saw_plan_checkpoint: bool,
        /// Whether the watcher observed any plan update this turn.
        #[serde(default)]
        saw_plan_update: bool,
        /// Whether the watcher observed a PR lifecycle checkpoint this turn.
        #[serde(default)]
        saw_pr_checkpoint: bool,
        /// Who authored the post-compact continuation packet (when applicable).
        packet_author: CompactionPacketAuthor,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum CompactionPacketAuthor {
    Watcher,
    Agent,
}
