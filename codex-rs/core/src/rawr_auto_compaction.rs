use std::collections::hash_map::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;
use std::path::Path;

use crate::config::Config;
use crate::rawr_prompts;
use codex_config::types::RawrAutoCompactionBoundary;
use codex_config::types::RawrAutoCompactionMode;
use codex_config::types::RawrAutoCompactionPacketAuthor;
use codex_config::types::RawrAutoCompactionPolicyTierToml;
use codex_features::Feature;
use codex_protocol::ThreadId;
use codex_protocol::parse_command::ParsedCommand;
use codex_protocol::plan_tool::StepStatus;
use codex_protocol::plan_tool::UpdatePlanArgs;
use codex_protocol::protocol::ExecCommandEndEvent;
use codex_protocol::protocol::ExecCommandStatus;
use codex_protocol::protocol::SessionSource;

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

        let Some(policy) = config
            .rawr_auto_compaction
            .as_ref()
            .and_then(|rawr| rawr.settings())
            .and_then(|settings| settings.policy.as_ref())
        else {
            return defaults;
        };

        Self {
            early_percent_remaining_lt: policy
                .early
                .as_ref()
                .and_then(|tier| tier.percent_remaining_lt)
                .unwrap_or(defaults.early_percent_remaining_lt),
            ready_percent_remaining_lt: policy
                .ready
                .as_ref()
                .and_then(|tier| tier.percent_remaining_lt)
                .unwrap_or(defaults.ready_percent_remaining_lt),
            asap_percent_remaining_lt: policy
                .asap
                .as_ref()
                .and_then(|tier| tier.percent_remaining_lt)
                .unwrap_or(defaults.asap_percent_remaining_lt),
            emergency_percent_remaining_lt: policy
                .emergency
                .as_ref()
                .and_then(|tier| tier.percent_remaining_lt)
                .unwrap_or(defaults.emergency_percent_remaining_lt),
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawrAutoCompactionSignals {
    pub saw_commit: bool,
    pub saw_plan_checkpoint: bool,
    pub saw_plan_update: bool,
    pub saw_pr_checkpoint: bool,
    pub saw_agent_done: bool,
    pub saw_topic_shift: bool,
    pub saw_concluding_thought: bool,
}

pub(crate) fn rawr_note_plan_update(
    signals: &mut RawrAutoCompactionSignals,
    completed_steps_seen: &mut usize,
    update: &UpdatePlanArgs,
) {
    signals.saw_plan_update = true;
    let completed_steps = rawr_completed_plan_steps(update);
    if completed_steps > *completed_steps_seen {
        signals.saw_plan_checkpoint = true;
        *completed_steps_seen = completed_steps;
    }
}

pub(crate) fn rawr_note_exec_command_end(
    signals: &mut RawrAutoCompactionSignals,
    event: &ExecCommandEndEvent,
) {
    if event.status != ExecCommandStatus::Completed {
        return;
    }

    if rawr_command_looks_like_git_commit(&event.command, &event.parsed_cmd) {
        signals.saw_commit = true;
    }
    if rawr_command_looks_like_pr_checkpoint(&event.command) {
        signals.saw_pr_checkpoint = true;
    }
}

pub(crate) fn rawr_note_completion_message(
    signals: &mut RawrAutoCompactionSignals,
    last_agent_message: Option<&str>,
) {
    let Some(last_agent_message) = last_agent_message else {
        return;
    };
    if rawr_agent_message_looks_done(last_agent_message) {
        signals.saw_agent_done = true;
    }
    if rawr_agent_message_looks_like_topic_shift(last_agent_message) {
        signals.saw_topic_shift = true;
    }
    if rawr_agent_message_looks_like_concluding_thought(last_agent_message) {
        signals.saw_concluding_thought = true;
    }
}

const RAWR_SCRATCH_FALLBACK_AGENT_NAMES: [&str; 24] = [
    "Aria", "Atlas", "Beau", "Cleo", "Ezra", "Jade", "Juno", "Luna", "Milo", "Nova", "Orion",
    "Pax", "Quinn", "Reid", "Remy", "Rhea", "Rory", "Sage", "Skye", "Toby", "Vera", "Wren", "Zane",
    "Zoe",
];

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

pub(crate) fn rawr_compaction_tier(
    config: &Config,
    percent_remaining: i64,
) -> Option<RawrAutoCompactionTier> {
    rawr_pick_tier(
        RawrAutoCompactionThresholds::from_config(config),
        percent_remaining,
    )
}

pub(crate) fn rawr_auto_compaction_mode(config: &Config) -> RawrAutoCompactionMode {
    config
        .rawr_auto_compaction
        .as_ref()
        .and_then(|rawr| rawr.settings())
        .and_then(|settings| settings.mode)
        .unwrap_or(RawrAutoCompactionMode::Auto)
}

pub(crate) fn rawr_auto_compaction_packet_author(
    config: &Config,
) -> RawrAutoCompactionPacketAuthor {
    config
        .rawr_auto_compaction
        .as_ref()
        .and_then(|rawr| rawr.settings())
        .and_then(|settings| settings.packet_author)
        .unwrap_or(RawrAutoCompactionPacketAuthor::Watcher)
}

pub(crate) fn rawr_packet_max_tail_chars(config: &Config) -> usize {
    config
        .rawr_auto_compaction
        .as_ref()
        .and_then(|rawr| rawr.settings())
        .and_then(|settings| settings.packet_max_tail_chars)
        .unwrap_or(2_000)
}

pub(crate) fn rawr_compaction_model(config: &Config) -> Option<String> {
    config
        .rawr_auto_compaction
        .as_ref()
        .and_then(|rawr| rawr.settings())
        .and_then(|settings| settings.compaction_model.clone())
}

pub(crate) fn rawr_scratch_write_enabled(config: &Config) -> bool {
    config
        .rawr_auto_compaction
        .as_ref()
        .and_then(|rawr| rawr.settings())
        .and_then(|settings| settings.scratch_write_enabled)
        .unwrap_or(false)
}

pub(crate) fn rawr_should_compact_at_turn_complete(
    config: &Config,
    percent_remaining: i64,
    signals: &RawrAutoCompactionSignals,
) -> bool {
    rawr_should_compact_with_boundary(config, percent_remaining, signals, true)
}

fn rawr_should_compact_with_boundary(
    config: &Config,
    percent_remaining: i64,
    signals: &RawrAutoCompactionSignals,
    turn_complete: bool,
) -> bool {
    if !config.features.enabled(Feature::RawrAutoCompaction) {
        return false;
    }

    let tier = match rawr_compaction_tier(config, percent_remaining) {
        Some(tier) => tier,
        None => return false,
    };

    if tier == RawrAutoCompactionTier::Emergency {
        return true;
    }

    let default_allowed = match tier {
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

    let policy_tier = rawr_policy_tier(config, tier);
    let required = policy_tier
        .and_then(|tier| tier.requires_any_boundary.as_deref())
        .unwrap_or(default_allowed);

    let has_semantic_boundary =
        signals.saw_agent_done || signals.saw_topic_shift || signals.saw_concluding_thought;
    let requires_semantic_boundary_for_plan = policy_tier
        .and_then(|tier| tier.plan_boundaries_require_semantic_break)
        .unwrap_or(matches!(
            tier,
            RawrAutoCompactionTier::Early | RawrAutoCompactionTier::Ready
        ));
    let mut satisfied_any_required_boundary = false;
    let mut satisfied_plan_boundary = false;
    let mut satisfied_non_plan_boundary = false;

    for boundary in required {
        let satisfied = match boundary {
            RawrAutoCompactionBoundary::Commit => signals.saw_commit,
            RawrAutoCompactionBoundary::PlanCheckpoint => signals.saw_plan_checkpoint,
            RawrAutoCompactionBoundary::PlanUpdate => signals.saw_plan_update,
            RawrAutoCompactionBoundary::PrCheckpoint => signals.saw_pr_checkpoint,
            RawrAutoCompactionBoundary::AgentDone => signals.saw_agent_done,
            RawrAutoCompactionBoundary::TopicShift => signals.saw_topic_shift,
            RawrAutoCompactionBoundary::ConcludingThought => signals.saw_concluding_thought,
            RawrAutoCompactionBoundary::TurnComplete => turn_complete,
        };
        if !satisfied {
            continue;
        }

        satisfied_any_required_boundary = true;
        match boundary {
            RawrAutoCompactionBoundary::PlanCheckpoint | RawrAutoCompactionBoundary::PlanUpdate => {
                satisfied_plan_boundary = true;
            }
            RawrAutoCompactionBoundary::Commit | RawrAutoCompactionBoundary::PrCheckpoint => {
                satisfied_non_plan_boundary = true;
            }
            RawrAutoCompactionBoundary::AgentDone
            | RawrAutoCompactionBoundary::TopicShift
            | RawrAutoCompactionBoundary::ConcludingThought
            | RawrAutoCompactionBoundary::TurnComplete => {}
        }
    }

    satisfied_any_required_boundary
        && (!requires_semantic_boundary_for_plan
            || !satisfied_plan_boundary
            || satisfied_non_plan_boundary
            || has_semantic_boundary)
}

fn rawr_policy_tier(
    config: &Config,
    tier: RawrAutoCompactionTier,
) -> Option<&RawrAutoCompactionPolicyTierToml> {
    let policy = config
        .rawr_auto_compaction
        .as_ref()
        .and_then(|rawr| rawr.settings())
        .and_then(|settings| settings.policy.as_ref())?;

    match tier {
        RawrAutoCompactionTier::Early => policy.early.as_ref(),
        RawrAutoCompactionTier::Ready => policy.ready.as_ref(),
        RawrAutoCompactionTier::Asap => policy.asap.as_ref(),
        RawrAutoCompactionTier::Emergency => policy.emergency.as_ref(),
    }
}

pub(crate) fn rawr_policy_decision_prompt_path(
    config: &Config,
    tier: RawrAutoCompactionTier,
) -> Option<String> {
    rawr_policy_tier(config, tier).and_then(|policy| policy.decision_prompt_path.clone())
}

pub(crate) fn rawr_boundaries_present(
    signals: &RawrAutoCompactionSignals,
    turn_complete: bool,
) -> Vec<String> {
    let mut boundaries = Vec::new();
    if signals.saw_commit {
        boundaries.push("commit".to_string());
    }
    if signals.saw_plan_checkpoint {
        boundaries.push("plan_checkpoint".to_string());
    }
    if signals.saw_plan_update {
        boundaries.push("plan_update".to_string());
    }
    if signals.saw_pr_checkpoint {
        boundaries.push("pr_checkpoint".to_string());
    }
    if signals.saw_agent_done {
        boundaries.push("agent_done".to_string());
    }
    if signals.saw_topic_shift {
        boundaries.push("topic_shift".to_string());
    }
    if signals.saw_concluding_thought {
        boundaries.push("concluding_thought".to_string());
    }
    if turn_complete {
        boundaries.push("turn_complete".to_string());
    }
    boundaries
}

fn rawr_completed_plan_steps(update: &UpdatePlanArgs) -> usize {
    update
        .plan
        .iter()
        .filter(|item| matches!(item.status, StepStatus::Completed))
        .count()
}

pub(crate) fn rawr_command_looks_like_git_commit(
    command: &[String],
    parsed_cmd: &[ParsedCommand],
) -> bool {
    if command.is_empty() {
        return false;
    }
    if parsed_cmd.iter().any(|parsed| match parsed {
        ParsedCommand::Unknown { cmd } => cmd.to_ascii_lowercase().contains("git commit"),
        _ => false,
    }) {
        return true;
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

    if joined.contains("gt submit") || joined.contains("gt ss") {
        return true;
    }
    if joined.contains("gt create") || joined.contains("gt review") || joined.contains("gt land") {
        return true;
    }
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

pub(crate) fn rawr_load_agent_packet_prompt(codex_home: &Path) -> String {
    let prompt =
        rawr_prompts::read_prompt_or_default(codex_home, rawr_prompts::RawrPromptKind::AutoCompact);
    let prompt = strip_yaml_frontmatter(&prompt).trim();
    if prompt.is_empty() {
        return default_rawr_agent_packet_prompt();
    }
    prompt.to_string()
}

pub(crate) fn rawr_load_scratch_write_prompt(codex_home: &Path) -> String {
    let prompt = rawr_prompts::read_prompt_or_default(
        codex_home,
        rawr_prompts::RawrPromptKind::ScratchWrite,
    );
    let prompt = strip_yaml_frontmatter(&prompt).trim();
    if prompt.is_empty() {
        return default_rawr_scratch_write_prompt();
    }
    prompt.to_string()
}

pub(crate) fn rawr_build_scratch_write_prompt(
    prompt: &str,
    scratch_file: &str,
    thread_id: Option<ThreadId>,
) -> String {
    let expanded = rawr_expand_prompt_template(prompt, Some(scratch_file), thread_id);
    if prompt.contains("{scratch_file}") || prompt.contains("{scratchFile}") {
        expanded
    } else {
        format!("{expanded}\n\nTarget file: `{scratch_file}`")
    }
}

pub(crate) fn rawr_build_agent_continuation_packet_prompt(
    packet_prompt: &str,
    scratch_prompt: &str,
    do_scratch: bool,
    scratch_file: Option<&str>,
    thread_id: Option<ThreadId>,
) -> String {
    if !do_scratch {
        return rawr_expand_prompt_template(packet_prompt, scratch_file, thread_id);
    }

    let scratch_prompt = if let Some(scratch_file) = scratch_file {
        rawr_build_scratch_write_prompt(scratch_prompt, scratch_file, thread_id)
    } else {
        rawr_expand_prompt_template(scratch_prompt, None, thread_id)
    };
    let packet_prompt = rawr_expand_prompt_template(packet_prompt, scratch_file, thread_id);
    format!("{scratch_prompt}\n\n---\n\n{packet_prompt}")
}

pub(crate) fn rawr_build_watcher_post_compact_packet(
    trigger_percent_remaining: i64,
    signals: &RawrAutoCompactionSignals,
    last_agent_message: Option<&str>,
    max_tail_chars: usize,
) -> String {
    let tail = truncate_chars(last_agent_message.unwrap_or("").trim(), max_tail_chars);
    let tail = if tail.is_empty() {
        "(none)".to_string()
    } else {
        tail
    };

    [
        "**Continuation context packet (post-compaction injection)**".to_string(),
        String::new(),
        "Overarching goal".to_string(),
        "- Continue the work you were doing immediately before compaction.".to_string(),
        String::new(),
        "Why compaction happened".to_string(),
        format!(
            "- Triggered by rawr auto-compaction watcher at {trigger_percent_remaining}% context remaining."
        ),
        format!(
            "- Natural boundary signals: commit={}, plan_checkpoint={}, plan_update={}, pr_checkpoint={}, agent_done={}",
            signals.saw_commit,
            signals.saw_plan_checkpoint,
            signals.saw_plan_update,
            signals.saw_pr_checkpoint,
            signals.saw_agent_done,
        ),
        String::new(),
        "Last agent output (memory trigger)".to_string(),
        format!("- {tail}"),
        String::new(),
        "Directive".to_string(),
        "- Continue with the remaining work now; do not restart from scratch.".to_string(),
    ]
    .join("\n")
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

pub(crate) fn rawr_scratch_file_rel_path(
    session_source: &SessionSource,
    thread_id: &ThreadId,
) -> String {
    let agent_name = rawr_scratch_agent_name(session_source, thread_id);
    format!(".scratch/agent-{agent_name}.scratch.md")
}

fn rawr_scratch_agent_name(session_source: &SessionSource, thread_id: &ThreadId) -> String {
    rawr_agent_identity_from_session_source(session_source)
        .unwrap_or_else(|| rawr_random_agent_name(thread_id))
}

fn rawr_agent_identity_from_session_source(source: &SessionSource) -> Option<String> {
    let identity = source.to_string();
    let identity = identity.strip_prefix("subagent_")?;
    let sanitized = rawr_sanitize_agent_name(identity);
    (!sanitized.is_empty()).then_some(sanitized)
}

fn rawr_random_agent_name(thread_id: &ThreadId) -> String {
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

fn strip_yaml_frontmatter(contents: &str) -> &str {
    let mut iter = contents.split_inclusive('\n');
    let Some(first_line) = iter.next() else {
        return contents;
    };
    if first_line.trim_end_matches(['\r', '\n']) != "---" {
        return contents;
    }

    let mut cursor = first_line.len();
    for piece in iter {
        let piece_start = cursor;
        let line = piece.trim_end_matches(['\r', '\n']);
        if line == "---" {
            let body_start = piece_start.saturating_add(piece.len());
            return contents.get(body_start..).unwrap_or("");
        }
        cursor = cursor.saturating_add(piece.len());
    }

    contents
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let mut out = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        out.push_str("...");
    }
    out
}

fn rawr_expand_prompt_template(
    prompt: &str,
    scratch_file: Option<&str>,
    thread_id: Option<ThreadId>,
) -> String {
    let mut values = Vec::new();
    if let Some(scratch_file) = scratch_file {
        values.push(("scratchFile", scratch_file.to_string()));
        values.push(("scratch_file", scratch_file.to_string()));
    }
    if let Some(thread_id) = thread_id {
        values.push(("threadId", thread_id.to_string()));
    }
    rawr_prompts::expand_placeholders(prompt, &values)
        .trim()
        .to_string()
}

fn default_rawr_agent_packet_prompt() -> String {
    [
        "[rawr] Agent: before we compact this thread, you must self-reflect and write a continuation context packet for yourself.",
        "",
        "Keep it short and structured. Do not include secrets; redact tokens/keys.",
    ]
    .join("\n")
}

fn default_rawr_scratch_write_prompt() -> String {
    [
        "[rawr] Before auto-compaction, write a verbatim scratchpad of the work you just completed so it survives compaction.",
        "",
        "Target file: `{scratch_file}`",
        "",
        "Requirements:",
        "- Create the `.context/` directory if it doesn't exist.",
        "- Append a new section; do not delete prior scratch content.",
        "- Prefer verbatim notes/drafts over summaries.",
        "- Include links/paths to any important files you edited or created.",
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_config::types::RawrAutoCompactionPolicyToml;
    use codex_config::types::RawrAutoCompactionSettingsToml;
    use codex_config::types::RawrAutoCompactionToml;
    use codex_protocol::plan_tool::PlanItemArg;
    use codex_protocol::plan_tool::StepStatus;
    use codex_protocol::plan_tool::UpdatePlanArgs;
    use codex_protocol::protocol::ExecCommandEndEvent;
    use codex_protocol::protocol::ExecCommandSource;
    use codex_protocol::protocol::ExecCommandStatus;
    use codex_utils_absolute_path::AbsolutePathBuf;
    use pretty_assertions::assert_eq;
    use std::time::Duration;

    #[tokio::test]
    async fn rawr_turn_complete_boundary_only_matches_turn_complete_path() {
        let mut config = crate::config::test_config().await;
        config
            .features
            .enable(Feature::RawrAutoCompaction)
            .expect("enable feature");
        config.rawr_auto_compaction = Some(RawrAutoCompactionToml::Config(Box::new(
            RawrAutoCompactionSettingsToml {
                policy: Some(RawrAutoCompactionPolicyToml {
                    early: Some(RawrAutoCompactionPolicyTierToml {
                        requires_any_boundary: Some(vec![RawrAutoCompactionBoundary::TurnComplete]),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )));

        let signals = RawrAutoCompactionSignals::default();

        assert_eq!(
            rawr_should_compact_with_boundary(&config, 80, &signals, false),
            false
        );
        assert_eq!(
            rawr_should_compact_at_turn_complete(&config, 80, &signals),
            true
        );
    }

    #[tokio::test]
    async fn default_turn_complete_alone_is_not_a_boundary() {
        let mut config = crate::config::test_config().await;
        config
            .features
            .enable(Feature::RawrAutoCompaction)
            .expect("enable feature");

        let signals = RawrAutoCompactionSignals::default();

        assert_eq!(
            rawr_should_compact_at_turn_complete(&config, 80, &signals),
            false
        );
    }

    #[test]
    fn watcher_packet_tail_truncation_is_char_safe() {
        assert_eq!(truncate_chars("alpha café beta", 10), "alpha café...");

        let signals = RawrAutoCompactionSignals::default();
        let packet = rawr_build_watcher_post_compact_packet(
            42,
            &signals,
            Some("alpha café beta"),
            /*max_tail_chars*/ 10,
        );
        assert!(packet.contains("- alpha café..."));
    }

    #[test]
    fn agent_packet_prompt_expands_scratch_placeholders() {
        let prompt = rawr_build_agent_continuation_packet_prompt(
            "thread={threadId} scratch={scratchFile}",
            "write {scratch_file}",
            true,
            Some(".scratch/agent-codex.scratch.md"),
            Some(ThreadId::new()),
        );
        assert!(prompt.contains("write .scratch/agent-codex.scratch.md"));
        assert!(prompt.contains("scratch=.scratch/agent-codex.scratch.md"));
    }

    #[test]
    fn scratch_handoff_uses_agent_specific_path_when_enabled() {
        let signals = RawrAutoCompactionSignals {
            saw_commit: true,
            ..Default::default()
        };

        assert!(rawr_should_schedule_scratch_write(
            true, /*is_emergency*/ false, &signals
        ));
        assert!(!rawr_should_schedule_scratch_write(
            true, /*is_emergency*/ true, &signals
        ));

        let thread_id = ThreadId::new();
        let scratch_file = rawr_scratch_file_rel_path(&SessionSource::Cli, &thread_id);
        let packet = rawr_build_agent_continuation_packet_prompt(
            "continue using {scratchFile}",
            "write notes to {scratch_file}",
            true,
            Some(scratch_file.as_str()),
            Some(thread_id),
        );
        let handoff = rawr_build_post_compact_handoff_message(packet, Some(scratch_file.as_str()));

        assert!(scratch_file.starts_with(".scratch/agent-"));
        assert!(handoff.starts_with(&format!("Scratchpad: `{scratch_file}`")));
        assert!(handoff.contains(&format!("write notes to {scratch_file}")));
        assert!(handoff.contains(&format!("continue using {scratch_file}")));
    }

    #[test]
    fn plan_update_checkpoint_sets_plan_signals() {
        let mut signals = RawrAutoCompactionSignals::default();
        let mut completed_steps_seen = 0;
        rawr_note_plan_update(
            &mut signals,
            &mut completed_steps_seen,
            &UpdatePlanArgs {
                explanation: None,
                plan: vec![PlanItemArg {
                    step: "done".to_string(),
                    status: StepStatus::Completed,
                }],
            },
        );

        rawr_note_plan_update(
            &mut signals,
            &mut completed_steps_seen,
            &UpdatePlanArgs {
                explanation: None,
                plan: vec![
                    PlanItemArg {
                        step: "done".to_string(),
                        status: StepStatus::Completed,
                    },
                    PlanItemArg {
                        step: "pending".to_string(),
                        status: StepStatus::Pending,
                    },
                ],
            },
        );

        assert_eq!(completed_steps_seen, 1);
        assert!(signals.saw_plan_checkpoint);
        assert!(signals.saw_plan_update);
    }

    #[test]
    fn completed_exec_command_sets_commit_and_pr_signals() {
        let mut commit_signals = RawrAutoCompactionSignals::default();
        rawr_note_exec_command_end(
            &mut commit_signals,
            &ExecCommandEndEvent {
                call_id: "call-1".to_string(),
                process_id: None,
                turn_id: "turn-1".to_string(),
                command: vec![
                    "git".to_string(),
                    "commit".to_string(),
                    "-m".to_string(),
                    "x".to_string(),
                ],
                cwd: AbsolutePathBuf::try_from(std::path::PathBuf::from("/tmp"))
                    .expect("absolute path"),
                parsed_cmd: Vec::new(),
                source: ExecCommandSource::Agent,
                interaction_input: None,
                stdout: String::new(),
                stderr: String::new(),
                aggregated_output: String::new(),
                exit_code: 0,
                duration: Duration::from_secs(1),
                formatted_output: String::new(),
                status: ExecCommandStatus::Completed,
            },
        );
        assert!(commit_signals.saw_commit);
        assert!(!commit_signals.saw_pr_checkpoint);

        let mut pr_signals = RawrAutoCompactionSignals::default();
        rawr_note_exec_command_end(
            &mut pr_signals,
            &ExecCommandEndEvent {
                call_id: "call-2".to_string(),
                process_id: None,
                turn_id: "turn-1".to_string(),
                command: vec!["gh".to_string(), "pr".to_string(), "create".to_string()],
                cwd: AbsolutePathBuf::try_from(std::path::PathBuf::from("/tmp"))
                    .expect("absolute path"),
                parsed_cmd: Vec::new(),
                source: ExecCommandSource::Agent,
                interaction_input: None,
                stdout: String::new(),
                stderr: String::new(),
                aggregated_output: String::new(),
                exit_code: 0,
                duration: Duration::from_secs(1),
                formatted_output: String::new(),
                status: ExecCommandStatus::Completed,
            },
        );
        assert!(!pr_signals.saw_commit);
        assert!(pr_signals.saw_pr_checkpoint);

        let mut push_signals = RawrAutoCompactionSignals::default();
        rawr_note_exec_command_end(
            &mut push_signals,
            &ExecCommandEndEvent {
                call_id: "call-3".to_string(),
                process_id: None,
                turn_id: "turn-1".to_string(),
                command: vec!["git".to_string(), "push".to_string()],
                cwd: AbsolutePathBuf::try_from(std::path::PathBuf::from("/tmp"))
                    .expect("absolute path"),
                parsed_cmd: Vec::new(),
                source: ExecCommandSource::Agent,
                interaction_input: None,
                stdout: String::new(),
                stderr: String::new(),
                aggregated_output: String::new(),
                exit_code: 0,
                duration: Duration::from_secs(1),
                formatted_output: String::new(),
                status: ExecCommandStatus::Completed,
            },
        );
        assert!(!push_signals.saw_pr_checkpoint);
    }

    #[test]
    fn completion_message_sets_semantic_signals() {
        let mut signals = RawrAutoCompactionSignals::default();
        rawr_note_completion_message(
            &mut signals,
            Some(
                "Completed the implementation. Next, let's update the docs. Final thoughts: keep the hook in tasks.",
            ),
        );

        assert!(signals.saw_agent_done);
        assert!(signals.saw_topic_shift);
        assert!(signals.saw_concluding_thought);
    }
}
