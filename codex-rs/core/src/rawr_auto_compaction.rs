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

#[derive(Default, Debug, Clone)]
pub(crate) struct RawrAutoCompactionSignals {
    pub saw_commit: bool,
    pub saw_plan_checkpoint: bool,
    pub saw_plan_update: bool,
    pub saw_pr_checkpoint: bool,
    pub saw_agent_done: bool,
    pub saw_topic_shift: bool,
    pub saw_concluding_thought: bool,
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
            RawrAutoCompactionBoundary::TurnComplete,
        ][..],
        RawrAutoCompactionTier::Ready => &[
            RawrAutoCompactionBoundary::Commit,
            RawrAutoCompactionBoundary::PlanCheckpoint,
            RawrAutoCompactionBoundary::PlanUpdate,
            RawrAutoCompactionBoundary::PrCheckpoint,
            RawrAutoCompactionBoundary::TopicShift,
            RawrAutoCompactionBoundary::TurnComplete,
        ][..],
        RawrAutoCompactionTier::Asap => &[
            RawrAutoCompactionBoundary::Commit,
            RawrAutoCompactionBoundary::PlanCheckpoint,
            RawrAutoCompactionBoundary::PlanUpdate,
            RawrAutoCompactionBoundary::PrCheckpoint,
            RawrAutoCompactionBoundary::AgentDone,
            RawrAutoCompactionBoundary::TopicShift,
            RawrAutoCompactionBoundary::ConcludingThought,
            RawrAutoCompactionBoundary::TurnComplete,
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
        "[rawr] Before we compact this thread, produce a continuation context packet for yourself.",
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
    use pretty_assertions::assert_eq;

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
}
