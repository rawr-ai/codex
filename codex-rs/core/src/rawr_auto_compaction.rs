use crate::config::Config;
use crate::config::types::RawrAutoCompactionBoundary;
use crate::features::Feature;
use codex_protocol::ThreadId;
use codex_protocol::plan_tool::StepStatus;
use codex_protocol::plan_tool::UpdatePlanArgs;
use codex_protocol::protocol::SessionSource;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;

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

const RAWR_AUTO_COMPACT_PROMPT_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../rawr/prompts/rawr-auto-compact.md"
));

const RAWR_SCRATCH_WRITE_PROMPT_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../rawr/prompts/rawr-scratch-write.md"
));

const RAWR_SCRATCH_FALLBACK_AGENT_NAMES: [&str; 24] = [
    "Aria", "Atlas", "Beau", "Cleo", "Ezra", "Jade", "Juno", "Luna", "Milo", "Nova", "Orion",
    "Pax", "Quinn", "Reid", "Remy", "Rhea", "Rory", "Sage", "Skye", "Toby", "Vera", "Wren", "Zane",
    "Zoe",
];

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

pub(crate) fn rawr_load_agent_packet_prompt() -> String {
    let (frontmatter, body) = split_yaml_frontmatter(RAWR_AUTO_COMPACT_PROMPT_TEMPLATE);
    let prompt = body.trim();
    if !prompt.is_empty() {
        return prompt.to_string();
    }
    if let Some(frontmatter) = frontmatter {
        let trimmed = frontmatter.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    default_rawr_agent_packet_prompt()
}

pub(crate) fn rawr_load_scratch_write_prompt() -> String {
    let prompt = RAWR_SCRATCH_WRITE_PROMPT_TEMPLATE.trim();
    if !prompt.is_empty() {
        return prompt.to_string();
    }
    default_rawr_scratch_write_prompt()
}

pub(crate) fn rawr_build_scratch_write_prompt(prompt: &str, scratch_file: &str) -> String {
    if prompt.contains("{scratch_file}") {
        prompt.replace("{scratch_file}", scratch_file)
    } else {
        let prompt = prompt.trim_end();
        format!("{prompt}\n\nTarget file: `{scratch_file}`")
    }
}

pub(crate) fn rawr_build_agent_continuation_packet_prompt(
    packet_prompt: &str,
    scratch_prompt: &str,
    do_scratch: bool,
    scratch_file: Option<&str>,
) -> String {
    if !do_scratch {
        if let Some(scratch_file) = scratch_file {
            let packet_prompt = packet_prompt.trim();
            return format!("Scratchpad: `{scratch_file}`\n\n{packet_prompt}");
        }
        return packet_prompt.trim().to_string();
    }

    let scratch_prompt = if let Some(scratch_file) = scratch_file {
        rawr_build_scratch_write_prompt(scratch_prompt, scratch_file)
    } else {
        scratch_prompt.to_string()
    };

    format!(
        "{scratch_prompt}\n\n---\n\n{packet_prompt}",
        packet_prompt = packet_prompt.trim()
    )
}

pub(crate) fn rawr_build_post_compact_handoff_message(
    packet: String,
    scratch_file: Option<&str>,
) -> String {
    if let Some(scratch_file) = scratch_file {
        format!("Scratchpad: `{scratch_file}`\n\n{packet}")
    } else {
        packet
    }
}

pub(crate) fn rawr_should_schedule_scratch_write(
    scratch_write_enabled: bool,
    is_emergency: bool,
    signals: &RawrAutoCompactionSignals,
) -> bool {
    if !scratch_write_enabled || is_emergency {
        return false;
    }
    signals.saw_commit
        || signals.saw_plan_checkpoint
        || signals.saw_plan_update
        || signals.saw_pr_checkpoint
        || signals.saw_agent_done
}

pub(crate) fn rawr_scratch_file_rel_path(
    session_source: &SessionSource,
    thread_id: &ThreadId,
) -> String {
    let agent_name = rawr_scratch_agent_name(session_source, thread_id);
    format!(".scratch/agent-{agent_name}.scratch.md")
}

fn rawr_scratch_agent_name(session_source: &SessionSource, thread_id: &ThreadId) -> String {
    let name = rawr_agent_identity_from_session_source(session_source)
        .unwrap_or_else(|| rawr_random_agent_name(thread_id));
    let name = if name.is_empty() {
        "codex".to_string()
    } else {
        name
    };
    name
}

fn rawr_agent_identity_from_session_source(source: &SessionSource) -> Option<String> {
    let identity = source.to_string();
    let identity = identity.strip_prefix("subagent_")?;
    let sanitized = rawr_sanitize_agent_name(identity);
    if sanitized.is_empty() {
        None
    } else {
        Some(sanitized)
    }
}

fn rawr_random_agent_name(thread_id: &ThreadId) -> String {
    if RAWR_SCRATCH_FALLBACK_AGENT_NAMES.is_empty() {
        return "codex".to_string();
    }
    let mut hasher = DefaultHasher::new();
    thread_id.hash(&mut hasher);
    let seed = hasher.finish() as usize;
    RAWR_SCRATCH_FALLBACK_AGENT_NAMES[seed % RAWR_SCRATCH_FALLBACK_AGENT_NAMES.len()].to_string()
}

fn rawr_sanitize_agent_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_dash = false;
    for ch in name.chars() {
        let ch = ch.to_ascii_lowercase();
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn split_yaml_frontmatter(contents: &str) -> (Option<&str>, &str) {
    let mut iter = contents.split_inclusive('\n');
    let Some(first_line) = iter.next() else {
        return (None, contents);
    };
    if first_line.trim_end_matches(['\r', '\n']) != "---" {
        return (None, contents);
    }

    let frontmatter_start = first_line.len();
    let mut cursor = frontmatter_start;

    for piece in iter {
        let piece_start = cursor;
        let line = piece.trim_end_matches(['\r', '\n']);
        if line == "---" {
            let frontmatter = &contents[frontmatter_start..piece_start];
            let body_start = piece_start.saturating_add(piece.len());
            let body = contents.get(body_start..).unwrap_or("");
            return (Some(frontmatter), body);
        }
        cursor = cursor.saturating_add(piece.len());
    }

    (None, contents)
}

fn default_rawr_agent_packet_prompt() -> String {
    [
        "[rawr] Before we compact this thread, produce a **continuation context packet** for yourself.",
        "",
        "Requirements:",
        "- Keep it short and structured.",
        "- Include: overarching goal, current state, next steps, invariants/decisions, and a final directive to continue after compaction.",
        "- Do not include secrets; redact tokens/keys.",
    ]
    .join("\n")
}

fn default_rawr_scratch_write_prompt() -> String {
    [
        "[rawr] Before we compact this thread, write a scratchpad file with what you just worked on.",
        "",
        "Target file: `{scratch_file}`",
        "",
        "Requirements:",
        "- Create the `.scratch/` directory if it doesn't exist.",
        "- Append a new section (do not delete prior scratch content).",
        "- Prefer verbatim notes/drafts over summaries; include raw details that are useful later.",
        "- Include links/paths to any important files you edited or created.",
        "- After writing, confirm in your next message that the scratch file was written and include the exact path.",
    ]
    .join("\n")
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
