use crate::config::Config;
use crate::config::types::RawrAutoCompactionBoundary;
use crate::features::Feature;
use crate::protocol::CompactionPacketAuthor;
use codex_protocol::plan_tool::StepStatus;
use codex_protocol::plan_tool::UpdatePlanArgs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RawrAutoCompactionTier {
    Early,
    Ready,
    Asap,
    Emergency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RawrAutoCompactionThresholds {
    pub early_percent_remaining_lt: i64,
    pub ready_percent_remaining_lt: i64,
    pub asap_percent_remaining_lt: i64,
    pub emergency_percent_remaining_lt: i64,
}

impl RawrAutoCompactionThresholds {
    pub(crate) fn from_config(config: &Config) -> Self {
        let defaults = Self {
            early_percent_remaining_lt: 85,
            ready_percent_remaining_lt: 75,
            asap_percent_remaining_lt: 65,
            emergency_percent_remaining_lt: 15,
        };

        let Some(rawr) = config.rawr_auto_compaction.as_ref() else {
            return defaults;
        };
        let Some(trigger) = rawr.trigger.as_ref() else {
            return defaults;
        };

        let ready = trigger
            .ready_percent_remaining_lt
            .unwrap_or(defaults.ready_percent_remaining_lt);

        Self {
            early_percent_remaining_lt: trigger
                .early_percent_remaining_lt
                .unwrap_or(defaults.early_percent_remaining_lt),
            ready_percent_remaining_lt: ready,
            asap_percent_remaining_lt: trigger
                .asap_percent_remaining_lt
                .unwrap_or(defaults.asap_percent_remaining_lt),
            emergency_percent_remaining_lt: trigger
                .emergency_percent_remaining_lt
                .unwrap_or(defaults.emergency_percent_remaining_lt),
        }
    }
}

#[derive(Default, Debug, Clone)]
pub(crate) struct RawrAutoCompactionSignals {
    active_turn_id: Option<String>,
    pub saw_commit: bool,
    pub saw_plan_checkpoint: bool,
    pub saw_plan_update: bool,
    pub saw_pr_checkpoint: bool,
    pub saw_agent_done: bool,
    pub saw_topic_shift: bool,
    pub saw_concluding_thought: bool,
}

impl RawrAutoCompactionSignals {
    pub(crate) fn reset_for_turn(&mut self, turn_id: String) {
        self.active_turn_id = Some(turn_id);
        self.saw_commit = false;
        self.saw_plan_checkpoint = false;
        self.saw_plan_update = false;
        self.saw_pr_checkpoint = false;
        self.saw_agent_done = false;
        self.saw_topic_shift = false;
        self.saw_concluding_thought = false;
    }

    pub(crate) fn is_active_turn(&self, turn_id: &str) -> bool {
        self.active_turn_id.as_deref() == Some(turn_id)
    }
}

pub(crate) fn rawr_packet_author(config: &Config) -> CompactionPacketAuthor {
    config
        .rawr_auto_compaction
        .as_ref()
        .and_then(|rawr| rawr.packet_author)
        .map_or(CompactionPacketAuthor::Watcher, |author| match author {
            crate::config::types::RawrAutoCompactionPacketAuthor::Watcher => {
                CompactionPacketAuthor::Watcher
            }
            crate::config::types::RawrAutoCompactionPacketAuthor::Agent => {
                CompactionPacketAuthor::Agent
            }
        })
}

pub(crate) fn rawr_command_looks_like_git_commit(command: &[String]) -> bool {
    if command.is_empty() {
        return false;
    }

    let joined = command.join(" ").to_ascii_lowercase();
    if joined.contains("git commit") {
        return true;
    }

    fn basename(s: &str) -> &str {
        std::path::Path::new(s)
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or(s)
    }

    command
        .windows(2)
        .any(|pair| basename(pair[0].as_str()) == "git" && pair[1].eq_ignore_ascii_case("commit"))
}

pub(crate) fn rawr_command_looks_like_pr_checkpoint(command: &[String]) -> bool {
    if command.is_empty() {
        return false;
    }
    let joined = command.join(" ").to_ascii_lowercase();

    // Publish-ish checkpoints.
    if joined.contains("git push") {
        return true;
    }
    if joined.contains("gt submit") || joined.contains("gt ss") {
        return true;
    }
    if joined.contains("gt create") || joined.contains("gt review") || joined.contains("gt land") {
        return true;
    }

    // GitHub PR lifecycle / review-ish checkpoints.
    if joined.contains("gh pr create")
        || joined.contains("gh pr close")
        || joined.contains("gh pr merge")
        || joined.contains("gh pr reopen")
        || joined.contains("gh pr review")
    {
        return true;
    }

    false
}

pub(crate) fn rawr_plan_update_is_checkpoint(update: &UpdatePlanArgs) -> bool {
    update
        .plan
        .iter()
        .any(|item| matches!(item.status, StepStatus::Completed))
}

pub(crate) fn rawr_agent_message_looks_done(message: &str) -> bool {
    let lower = message.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return false;
    }
    if lower.contains("not done")
        || lower.contains("not completed")
        || lower.contains("not finished")
    {
        return false;
    }
    ["done", "completed", "finished", "shipped", "pushed"]
        .into_iter()
        .any(|needle| lower.contains(needle))
}

pub(crate) fn rawr_agent_message_looks_like_topic_shift(message: &str) -> bool {
    let lower = message.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return false;
    }
    [
        "moving on",
        "switching to",
        "next,",
        "next:",
        "next up",
        "now, let's",
        "now let's",
        "we'll now",
    ]
    .into_iter()
    .any(|needle| lower.contains(needle))
}

pub(crate) fn rawr_agent_message_looks_like_concluding_thought(message: &str) -> bool {
    let lower = message.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return false;
    }
    [
        "in summary",
        "to summarize",
        "to wrap up",
        "wrapping up",
        "conclusion",
        "concluding",
        "final thoughts",
        "next steps",
    ]
    .into_iter()
    .any(|needle| lower.contains(needle))
}

pub(crate) fn rawr_pick_tier(
    thresholds: RawrAutoCompactionThresholds,
    percent_remaining: i64,
) -> Option<RawrAutoCompactionTier> {
    if percent_remaining < thresholds.emergency_percent_remaining_lt {
        return Some(RawrAutoCompactionTier::Emergency);
    }
    if percent_remaining < thresholds.asap_percent_remaining_lt {
        return Some(RawrAutoCompactionTier::Asap);
    }
    if percent_remaining < thresholds.ready_percent_remaining_lt {
        return Some(RawrAutoCompactionTier::Ready);
    }
    if percent_remaining < thresholds.early_percent_remaining_lt {
        return Some(RawrAutoCompactionTier::Early);
    }
    None
}

pub(crate) fn rawr_should_compact_mid_turn(
    config: &Config,
    percent_remaining: i64,
    signals: &RawrAutoCompactionSignals,
    boundaries_required: &[RawrAutoCompactionBoundary],
) -> bool {
    if !config.features.enabled(Feature::RawrAutoCompaction) {
        return false;
    }

    let tier = match rawr_pick_tier(
        RawrAutoCompactionThresholds::from_config(config),
        percent_remaining,
    ) {
        Some(tier) => tier,
        None => return false,
    };

    // Emergency ignores boundary gating.
    if tier == RawrAutoCompactionTier::Emergency {
        return true;
    }

    let allowed = match tier {
        RawrAutoCompactionTier::Early => &[
            RawrAutoCompactionBoundary::PlanCheckpoint,
            RawrAutoCompactionBoundary::PlanUpdate,
            RawrAutoCompactionBoundary::PrCheckpoint,
            RawrAutoCompactionBoundary::TopicShift,
        ][..],
        RawrAutoCompactionTier::Ready => &[
            RawrAutoCompactionBoundary::Commit,
            RawrAutoCompactionBoundary::PlanCheckpoint,
            RawrAutoCompactionBoundary::PlanUpdate,
            RawrAutoCompactionBoundary::PrCheckpoint,
            RawrAutoCompactionBoundary::TopicShift,
        ][..],
        RawrAutoCompactionTier::Asap => &[
            RawrAutoCompactionBoundary::Commit,
            RawrAutoCompactionBoundary::PlanCheckpoint,
            RawrAutoCompactionBoundary::PlanUpdate,
            RawrAutoCompactionBoundary::PrCheckpoint,
            RawrAutoCompactionBoundary::AgentDone,
            RawrAutoCompactionBoundary::TopicShift,
            RawrAutoCompactionBoundary::ConcludingThought,
        ][..],
        RawrAutoCompactionTier::Emergency => unreachable!(),
    };

    let required = if boundaries_required.is_empty() {
        allowed
    } else {
        boundaries_required
    };

    required.iter().any(|boundary| match boundary {
        RawrAutoCompactionBoundary::Commit => signals.saw_commit,
        RawrAutoCompactionBoundary::PlanCheckpoint => signals.saw_plan_checkpoint,
        RawrAutoCompactionBoundary::PlanUpdate => signals.saw_plan_update,
        RawrAutoCompactionBoundary::PrCheckpoint => signals.saw_pr_checkpoint,
        RawrAutoCompactionBoundary::AgentDone => signals.saw_agent_done,
        RawrAutoCompactionBoundary::TopicShift => signals.saw_topic_shift,
        RawrAutoCompactionBoundary::ConcludingThought => signals.saw_concluding_thought,
        RawrAutoCompactionBoundary::TurnComplete => false,
    })
}
