use crate::client_common::Prompt;
use crate::codex::Session;
use crate::codex::TurnContext;
use crate::error::CodexErr;
use crate::error::Result;
use crate::rawr_prompts;
use crate::stream_events_utils::last_assistant_message_from_item;
use codex_protocol::config_types::WebSearchMode;
use codex_protocol::models::BaseInstructions;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use futures::StreamExt;
use serde::Deserialize;
use serde_json::json;
use std::path::Path;
use std::path::PathBuf;
use tokio::fs;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct RawrAutoCompactionJudgment {
    pub should_compact: bool,
    pub reason: String,
}

pub(crate) async fn request_rawr_auto_compaction_judgment(
    sess: &Session,
    turn_context: &TurnContext,
    decision_prompt_path: &str,
    tier: &str,
    percent_remaining: i64,
    boundaries_present: &[String],
    last_agent_message: &str,
) -> Result<RawrAutoCompactionJudgment> {
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
    let decision_ctx = build_decision_context_from_template(
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

    let prompt = Prompt {
        input: vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText { text: decision_ctx }],
            end_turn: None,
            phase: None,
        }],
        tools: Vec::new(),
        parallel_tool_calls: false,
        base_instructions: BaseInstructions { text: instructions },
        personality: None,
        output_schema: Some(judgment_output_schema()),
    };

    drain_judgment_stream(sess, turn_context, &prompt).await
}

async fn drain_judgment_stream(
    sess: &Session,
    turn_context: &TurnContext,
    prompt: &Prompt,
) -> Result<RawrAutoCompactionJudgment> {
    let mut client_session = sess.services.model_client.new_session();
    let turn_metadata_header = turn_context.resolve_turn_metadata_header().await;
    let web_search_eligible = !matches!(
        turn_context.config.web_search_mode,
        Some(WebSearchMode::Disabled)
    );
    let mut stream = client_session
        .stream(
            prompt,
            &turn_context.model_info,
            &turn_context.otel_manager,
            turn_context.reasoning_effort,
            turn_context.reasoning_summary,
            web_search_eligible,
            turn_metadata_header.as_deref(),
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
            Ok(codex_api::common::ResponseEvent::OutputItemDone(item)) => {
                if let Some(text) = last_assistant_message_from_item(&item, false) {
                    last_message = Some(text);
                }
            }
            Ok(codex_api::common::ResponseEvent::ServerReasoningIncluded(included)) => {
                sess.set_server_reasoning_included(included).await;
            }
            Ok(codex_api::common::ResponseEvent::RateLimits(snapshot)) => {
                sess.update_rate_limits_quiet(snapshot).await;
            }
            Ok(codex_api::common::ResponseEvent::Completed { token_usage, .. }) => {
                sess.update_token_usage_info_quiet(turn_context, token_usage.as_ref())
                    .await;
                break;
            }
            Ok(_) => continue,
            Err(e) => return Err(e),
        }
    }

    let raw =
        last_message.ok_or_else(|| CodexErr::Stream("missing assistant output".into(), None))?;
    parse_judgment_from_text(&raw)
}

fn parse_judgment_from_text(text: &str) -> Result<RawrAutoCompactionJudgment> {
    if let Ok(parsed) = serde_json::from_str::<RawrAutoCompactionJudgment>(text) {
        return Ok(parsed);
    }

    let trimmed = text.trim();
    let trimmed = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    let trimmed = trimmed.strip_suffix("```").unwrap_or(trimmed).trim();
    if let Ok(parsed) = serde_json::from_str::<RawrAutoCompactionJudgment>(trimmed) {
        return Ok(parsed);
    }

    let Some(start) = trimmed.find('{') else {
        return Err(CodexErr::Stream(
            "judgment output was not JSON".into(),
            None,
        ));
    };
    let Some(end) = trimmed.rfind('}') else {
        return Err(CodexErr::Stream(
            "judgment output was not JSON".into(),
            None,
        ));
    };
    let candidate = &trimmed[start..=end];
    serde_json::from_str::<RawrAutoCompactionJudgment>(candidate)
        .map_err(|e| CodexErr::Stream(format!("failed to parse judgment JSON: {e}"), None))
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

fn build_decision_context_from_template(
    template: &str,
    tier: &str,
    percent_remaining: i64,
    boundaries_present: &[String],
    last_agent_message: &str,
    transcript_excerpt: &str,
    thread_id: codex_protocol::ThreadId,
    turn_id: &str,
    total_usage_tokens: i64,
    model_context_window: Option<i64>,
) -> String {
    let boundaries_json = serde_json::to_string(boundaries_present).unwrap_or_else(|_| "[]".into());
    let last_agent_message = last_agent_message.trim();
    let transcript_excerpt = transcript_excerpt.trim();
    let model_context_window = model_context_window
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let values = [
        ("tier", tier.to_string()),
        ("percentRemaining", percent_remaining.to_string()),
        ("boundariesJson", boundaries_json),
        ("lastAgentMessage", last_agent_message.to_string()),
        ("transcriptExcerpt", transcript_excerpt.to_string()),
        ("threadId", thread_id.to_string()),
        ("turnId", turn_id.to_string()),
        ("totalUsageTokens", total_usage_tokens.to_string()),
        ("modelContextWindow", model_context_window),
    ];

    rawr_prompts::expand_placeholders(template, &values)
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

        let Some(text) = content.iter().rev().find_map(|ci| match ci {
            ContentItem::InputText { text } => Some(text.as_str()),
            ContentItem::OutputText { text } => Some(text.as_str()),
            _ => None,
        }) else {
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
