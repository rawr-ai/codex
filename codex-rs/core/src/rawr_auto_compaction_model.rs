use std::path::Path;
use std::path::PathBuf;

use crate::client_common::Prompt;
use crate::rawr_prompts;
use crate::session::session::Session;
use crate::session::turn_context::TurnContext;
use crate::stream_events_utils::last_assistant_message_from_item;
use codex_protocol::ThreadId;
use codex_protocol::error::CodexErr;
use codex_protocol::error::Result as CodexResult;
use codex_protocol::models::BaseInstructions;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_rollout_trace::InferenceTraceContext;
use futures::StreamExt;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::json;
use tokio::fs;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct RawrAutoCompactionJudgment {
    pub should_compact: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct RawrAutoCompactionAgentArtifacts {
    pub continuation_packet: Option<String>,
    pub scratchpad_contents: Option<String>,
}

pub(crate) async fn request_rawr_auto_compaction_judgment(
    sess: &Session,
    turn_context: &TurnContext,
    decision_prompt_path: &str,
    tier: &str,
    percent_remaining: i64,
    boundaries_present: &[String],
    last_agent_message: &str,
) -> CodexResult<RawrAutoCompactionJudgment> {
    let codex_home = turn_context.config.codex_home.clone();
    if let Err(err) = rawr_prompts::ensure_rawr_prompt_files(&codex_home) {
        tracing::warn!("failed to ensure rawr prompt directory: {err}");
    }

    let prompt_path = resolve_prompt_path(&turn_context.cwd, &codex_home, decision_prompt_path);
    let prompt_contents = fs::read_to_string(&prompt_path)
        .await
        .map_err(CodexErr::from)?;
    let instructions = strip_yaml_frontmatter(&prompt_contents).trim().to_string();

    let excerpt = build_recent_transcript_excerpt(sess, 12, 800).await;
    let context = build_decision_context(
        &rawr_prompts::read_prompt_or_default(
            &codex_home,
            rawr_prompts::RawrPromptKind::JudgmentContext,
        ),
        tier,
        percent_remaining,
        boundaries_present,
        last_agent_message,
        &excerpt,
        sess.conversation_id,
        &turn_context.sub_id,
        sess.get_total_token_usage().await,
        turn_context.model_context_window(),
    );

    run_structured_prompt(
        sess,
        turn_context,
        instructions,
        context,
        judgment_output_schema(),
    )
    .await
}

#[expect(
    clippy::too_many_arguments,
    reason = "focused internal RAWR request surface"
)]
pub(crate) async fn request_rawr_auto_compaction_agent_artifacts(
    sess: &Session,
    turn_context: &TurnContext,
    packet_prompt: Option<&str>,
    scratch_prompt: Option<&str>,
    tier: &str,
    percent_remaining: i64,
    boundaries_present: &[String],
    last_agent_message: &str,
    scratch_file: Option<&str>,
) -> CodexResult<RawrAutoCompactionAgentArtifacts> {
    let instructions = build_artifact_instructions(packet_prompt, scratch_prompt);
    let excerpt = build_recent_transcript_excerpt(sess, 12, 800).await;
    let context = build_artifact_context(
        tier,
        percent_remaining,
        boundaries_present,
        last_agent_message,
        &excerpt,
        sess.conversation_id,
        &turn_context.sub_id,
        sess.get_total_token_usage().await,
        turn_context.model_context_window(),
        scratch_file,
        packet_prompt.is_some(),
        scratch_prompt.is_some(),
    );

    run_structured_prompt(
        sess,
        turn_context,
        instructions,
        context,
        artifact_output_schema(packet_prompt.is_some(), scratch_prompt.is_some()),
    )
    .await
}

async fn run_structured_prompt<T: DeserializeOwned>(
    sess: &Session,
    turn_context: &TurnContext,
    instructions: String,
    context: String,
    output_schema: serde_json::Value,
) -> CodexResult<T> {
    let prompt = Prompt {
        input: vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText { text: context }],
            end_turn: None,
            phase: None,
        }],
        tools: Vec::new(),
        parallel_tool_calls: false,
        base_instructions: BaseInstructions { text: instructions },
        personality: None,
        output_schema: Some(output_schema),
        output_schema_strict: true,
    };

    let mut client_session = sess.services.model_client.new_session();
    let turn_metadata_header = turn_context.turn_metadata_state.current_header_value();
    let mut stream = client_session
        .stream(
            &prompt,
            &turn_context.model_info,
            &turn_context.session_telemetry,
            turn_context.reasoning_effort,
            turn_context.reasoning_summary,
            turn_context.config.service_tier,
            turn_metadata_header.as_deref(),
            &InferenceTraceContext::disabled(),
        )
        .await?;

    let mut last_message: Option<String> = None;

    loop {
        let Some(event) = stream.next().await else {
            return Err(CodexErr::Stream(
                "stream closed before response.completed".into(),
                None,
            ));
        };
        match event {
            Ok(codex_api::ResponseEvent::OutputItemDone(item)) => {
                if let Some(text) = last_assistant_message_from_item(&item, false) {
                    last_message = Some(text);
                }
            }
            Ok(codex_api::ResponseEvent::ServerReasoningIncluded(included)) => {
                sess.set_server_reasoning_included(included).await;
            }
            Ok(codex_api::ResponseEvent::RateLimits(snapshot)) => {
                sess.update_rate_limits(turn_context, snapshot).await;
            }
            Ok(codex_api::ResponseEvent::Completed { token_usage, .. }) => {
                sess.update_token_usage_info(turn_context, token_usage.as_ref())
                    .await;
                break;
            }
            Ok(_) => continue,
            Err(err) => return Err(err),
        }
    }

    let raw =
        last_message.ok_or_else(|| CodexErr::Stream("missing assistant output".into(), None))?;
    parse_json_from_text(&raw)
}

fn parse_json_from_text<T: DeserializeOwned>(text: &str) -> CodexResult<T> {
    if let Ok(parsed) = serde_json::from_str::<T>(text) {
        return Ok(parsed);
    }

    let trimmed = text.trim();
    let trimmed = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    let trimmed = trimmed.strip_suffix("```").unwrap_or(trimmed).trim();
    if let Ok(parsed) = serde_json::from_str::<T>(trimmed) {
        return Ok(parsed);
    }

    let Some(start) = trimmed.find('{') else {
        return Err(CodexErr::Stream(
            "structured output was not JSON".into(),
            None,
        ));
    };
    let Some(end) = trimmed.rfind('}') else {
        return Err(CodexErr::Stream(
            "structured output was not JSON".into(),
            None,
        ));
    };
    let candidate = &trimmed[start..=end];
    serde_json::from_str::<T>(candidate)
        .map_err(|err| CodexErr::Stream(format!("failed to parse structured JSON: {err}"), None))
}

fn judgment_output_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "should_compact": { "type": "boolean" },
            "reason": { "type": "string" }
        },
        "required": ["should_compact", "reason"]
    })
}

fn artifact_output_schema(include_packet: bool, include_scratch: bool) -> serde_json::Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    if include_packet {
        properties.insert(
            "continuation_packet".to_string(),
            json!({ "type": "string" }),
        );
        required.push("continuation_packet");
    }
    if include_scratch {
        properties.insert(
            "scratchpad_contents".to_string(),
            json!({ "type": "string" }),
        );
        required.push("scratchpad_contents");
    }

    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": properties,
        "required": required
    })
}

fn build_artifact_instructions(
    packet_prompt: Option<&str>,
    scratch_prompt: Option<&str>,
) -> String {
    let mut sections = Vec::new();
    if let Some(packet_prompt) = packet_prompt {
        sections.push(packet_prompt.trim().to_string());
    }
    if let Some(scratch_prompt) = scratch_prompt {
        sections.push(scratch_prompt.trim().to_string());
    }
    sections.push(
        "Return strict JSON matching the requested schema. Do not wrap the JSON in prose."
            .to_string(),
    );
    sections.join("\n\n---\n\n")
}

#[expect(clippy::too_many_arguments, reason = "small prompt context builder")]
fn build_decision_context(
    template: &str,
    tier: &str,
    percent_remaining: i64,
    boundaries_present: &[String],
    last_agent_message: &str,
    transcript_excerpt: &str,
    thread_id: ThreadId,
    turn_id: &str,
    total_usage_tokens: i64,
    model_context_window: Option<i64>,
) -> String {
    let boundaries_json = serde_json::to_string(boundaries_present).unwrap_or_else(|_| "[]".into());
    let model_context_window = model_context_window
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let values = [
        ("tier", tier.to_string()),
        ("percentRemaining", percent_remaining.to_string()),
        ("boundariesJson", boundaries_json),
        ("lastAgentMessage", last_agent_message.trim().to_string()),
        ("transcriptExcerpt", transcript_excerpt.trim().to_string()),
        ("threadId", thread_id.to_string()),
        ("turnId", turn_id.to_string()),
        ("totalUsageTokens", total_usage_tokens.to_string()),
        ("modelContextWindow", model_context_window),
    ];

    rawr_prompts::expand_placeholders(template, &values)
}

#[expect(clippy::too_many_arguments, reason = "small focused prompt builder")]
fn build_artifact_context(
    tier: &str,
    percent_remaining: i64,
    boundaries_present: &[String],
    last_agent_message: &str,
    transcript_excerpt: &str,
    thread_id: ThreadId,
    turn_id: &str,
    total_usage_tokens: i64,
    model_context_window: Option<i64>,
    scratch_file: Option<&str>,
    include_packet: bool,
    include_scratch: bool,
) -> String {
    let mut sections = vec![
        format!("Tier: {tier}"),
        format!("Percent remaining: {percent_remaining}"),
        format!(
            "Boundaries present: {}",
            serde_json::to_string(boundaries_present).unwrap_or_else(|_| "[]".to_string())
        ),
        format!("Thread: {thread_id}"),
        format!("Turn: {turn_id}"),
        format!("Total usage tokens: {total_usage_tokens}"),
        format!(
            "Model context window: {}",
            model_context_window
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ),
        "Last agent message:".to_string(),
        last_agent_message.trim().to_string(),
        String::new(),
        "Recent transcript excerpt:".to_string(),
        transcript_excerpt.trim().to_string(),
    ];

    if let Some(scratch_file) = scratch_file {
        sections.push(String::new());
        sections.push(format!("Scratch file: {scratch_file}"));
    }

    sections.push(String::new());
    sections.push("Return only JSON.".to_string());
    sections.push(format!("Include continuation_packet: {include_packet}"));
    sections.push(format!("Include scratchpad_contents: {include_scratch}"));

    sections.join("\n")
}

fn resolve_prompt_path(cwd: &Path, codex_home: &Path, raw: &str) -> PathBuf {
    let path = Path::new(raw);
    if path.is_absolute() {
        return path.to_path_buf();
    }
    let prompt_dir = rawr_prompts::rawr_prompt_dir(codex_home);
    let candidate = prompt_dir.join(path);
    if candidate.exists() {
        return candidate;
    }
    cwd.join(path)
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

async fn build_recent_transcript_excerpt(
    sess: &Session,
    max_messages: usize,
    max_chars_per_message: usize,
) -> String {
    let history = sess.clone_history().await;
    let mut out: Vec<String> = Vec::new();

    for item in history.raw_items().iter().rev() {
        if out.len() >= max_messages {
            break;
        }
        let ResponseItem::Message { role, content, .. } = item else {
            continue;
        };

        let Some(text) = content
            .iter()
            .rev()
            .find_map(|content_item| match content_item {
                ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                    Some(text.as_str())
                }
                _ => None,
            })
        else {
            continue;
        };

        let text = text.trim();
        if text.is_empty() {
            continue;
        }

        let mut snippet = text.to_string();
        if snippet.len() > max_chars_per_message {
            snippet.truncate(max_chars_per_message);
            snippet.push('…');
        }
        out.push(format!("{role}: {snippet}"));
    }

    out.reverse();
    out.join("\n")
}
