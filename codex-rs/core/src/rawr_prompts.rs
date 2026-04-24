use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

use tracing::warn;

pub const RAWR_PROMPT_DIR_NAME: &str = "auto-compact";
pub const RAWR_AUTO_COMPACT_PROMPT_FILE: &str = "auto-compact.md";
pub const RAWR_SCRATCH_WRITE_PROMPT_FILE: &str = "scratch-write.md";
pub const RAWR_JUDGMENT_PROMPT_FILE: &str = "judgment.md";
pub const RAWR_JUDGMENT_CONTEXT_PROMPT_FILE: &str = "judgment-context.md";
pub const RAWR_WATCHER_PACKET_PROMPT_FILE: &str = "watcher-packet.md";

const DEFAULT_AUTO_COMPACT_PROMPT: &str = "\
[rawr] Agent: before we compact this thread, you must self-reflect and write a continuation context packet for yourself.

This is not a generic compact. This is your tight, intra-turn handoff: you are responsible for capturing the minimum, precise context you will need to resume smoothly after compaction and continue exactly where you left off (no drift, no restart).

Precedence (important):
- This continuation context packet is the authoritative source of what to do next after compaction.
- The generic compacted context is background only and must not override or supersede this packet.

Accountability:
- You own what gets carried forward. Be deliberate: reflect on your actual goal, state, decisions, and immediate next action.
- If something is uncertain, name the assumption you are carrying forward rather than hand-waving it.

Write the packet in my voice, as if I (the user) am speaking directly to you (the in-session agent). But the content must come from your self-reflection on this conversation and your work so far.

Keep it short and structured. Do not include secrets; redact tokens/keys.

Include exactly these sections:

1) Overarching goal
- Briefly restate the overall objective you are trying to accomplish (higher-level than the last message, but still concise).

2) Current state / progress snapshot
- State the very last thing that just happened (commit, PR checkpoint, plan step completion, etc.).
- Explain how that action relates to the overarching goal and where it leaves you right now.

3) Invariants and decisions (for this continuation)
- Enumerate the rules/choices that must continue to hold when you resume (specific to this ongoing effort).

4) Next step / immediate continuation
- Specify the single next thing to do when you resume.
- Tie it explicitly to the overarching goal and the just-completed action.

5) Verbatim continuation snippet (programmatically inserted)
- Include a literal placeholder for a verbatim memory trigger snippet to be inserted later from your most recent messages:
  - {{RAWR_VERBATIM_CONTINUATION_SNIPPET}}

Final directive:
- End with one clear directive to immediately continue from Next step / immediate continuation after compaction (do not restart or re-plan from scratch).

Heuristic notes (for auditing)
- commit: a successful git commit occurred in this turn.
- pr_checkpoint: a PR lifecycle checkpoint occurred (publish/review/open/close heuristics).
- plan_checkpoint: the plan was updated and at least one step was marked completed.
- agent_done: the assistant explicitly indicates completion (for example done, completed, finished).
";

const DEFAULT_SCRATCH_WRITE_PROMPT: &str = "\
[rawr] Before auto-compaction, write a verbatim scratchpad of the work you just completed so it survives compaction.

Target file: `{scratch_file}`

Requirements:
- Create the `.scratch/` directory if it doesn't exist.
- Create your scratch document if it doesn't exist, in whatever working space you're already keep scratch documents and transient context.
- Append a new section (do not delete prior scratch content).
- Prefer verbatim notes/drafts over summaries; include raw details that are useful later.
- Include links/paths to any important files you edited or created.
- After writing, confirm in your next message that the scratch file was written and include the exact path.

Overall goal:
Create (or rewrite) the scratch document to have two sections, optimized for continuity + fast reorientation, not as a status report.

1) Current frame / objective / vision (high level)
- the user’s communicated frame, objective, vision
- the overall process/workflow as a short but complete relevant narration
- the current state of things: what we’re working on and what we’re driving toward

2) Precision references (ground truth)
- the specific links, file paths, and precise references needed to act

Style constraints:
- Start high-level, then progressively zoom in until you reach precise links/paths.
- Don’t be overly verbose; be directional and clear.
- Preserve the most important invariants.
- Write it to yourself, with the goal of surviving compaction and making reorientation immediate.
";

const DEFAULT_JUDGMENT_PROMPT: &str = "\
[rawr] Decide whether automatic post-turn compaction should run now.

Requirements:
- Return strict JSON matching the requested schema.
- Approve compaction only when the supplied context shows a real boundary and compaction would help.
- Deny compaction when the boundary is weak, the turn is still in the middle of a cohesive thread, or compaction would likely lose important short-term working context.
- Keep the reason concise and specific.
";

const DEFAULT_JUDGMENT_CONTEXT_PROMPT: &str = "\
Tier: {tier}
Percent remaining: {percentRemaining}
Boundaries present: {boundariesJson}
Last agent message:
{lastAgentMessage}

Recent transcript excerpt:
{transcriptExcerpt}

Thread: {threadId}
Turn: {turnId}
Total usage tokens: {totalUsageTokens}
Model context window: {modelContextWindow}
";

const DEFAULT_WATCHER_PACKET_PROMPT: &str = "\
**Continuation context packet (post-compaction injection)**

Overarching goal
- Continue the work you were doing immediately before compaction.

Why compaction happened
- Triggered by rawr auto-compaction watcher at {triggerPercentRemaining}% context remaining.
- Natural boundary signals: {boundarySignals}

Last agent output (memory trigger)
- {lastAgentMessage}

Directive
- Continue with the remaining work now; do not restart from scratch.
";

#[derive(Debug, Clone, Copy)]
pub enum RawrPromptKind {
    AutoCompact,
    ScratchWrite,
    JudgmentContext,
    WatcherPacket,
}

#[derive(Debug, Clone)]
pub struct RawrPromptPaths {
    pub auto_compact: PathBuf,
    pub scratch_write: PathBuf,
    pub judgment_context: PathBuf,
    pub watcher_packet: PathBuf,
}

pub fn rawr_prompt_dir(codex_home: &Path) -> PathBuf {
    codex_home.join(RAWR_PROMPT_DIR_NAME)
}

pub fn ensure_rawr_prompt_files(codex_home: &Path) -> io::Result<RawrPromptPaths> {
    let dir = rawr_prompt_dir(codex_home);
    fs::create_dir_all(&dir)?;

    let auto_compact = dir.join(RAWR_AUTO_COMPACT_PROMPT_FILE);
    let scratch_write = dir.join(RAWR_SCRATCH_WRITE_PROMPT_FILE);
    let judgment = dir.join(RAWR_JUDGMENT_PROMPT_FILE);
    let judgment_context = dir.join(RAWR_JUDGMENT_CONTEXT_PROMPT_FILE);
    let watcher_packet = dir.join(RAWR_WATCHER_PACKET_PROMPT_FILE);

    write_default_if_missing(&auto_compact, DEFAULT_AUTO_COMPACT_PROMPT)?;
    write_default_if_missing(&scratch_write, DEFAULT_SCRATCH_WRITE_PROMPT)?;
    write_default_if_missing(&judgment, DEFAULT_JUDGMENT_PROMPT)?;
    write_default_if_missing(&judgment_context, DEFAULT_JUDGMENT_CONTEXT_PROMPT)?;
    write_default_if_missing(&watcher_packet, DEFAULT_WATCHER_PACKET_PROMPT)?;

    Ok(RawrPromptPaths {
        auto_compact,
        scratch_write,
        judgment_context,
        watcher_packet,
    })
}

pub fn read_prompt_path_or_default(
    codex_home: &Path,
    path_override: Option<&str>,
    kind: RawrPromptKind,
) -> String {
    let paths = match ensure_rawr_prompt_files(codex_home) {
        Ok(paths) => paths,
        Err(err) => {
            warn!("failed to ensure rawr prompt directory: {err}");
            return default_prompt(kind).to_string();
        }
    };

    let path = path_override
        .map(|raw| resolve_prompt_path(codex_home, raw))
        .unwrap_or_else(|| match kind {
            RawrPromptKind::AutoCompact => paths.auto_compact,
            RawrPromptKind::ScratchWrite => paths.scratch_write,
            RawrPromptKind::JudgmentContext => paths.judgment_context,
            RawrPromptKind::WatcherPacket => paths.watcher_packet,
        });

    match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) => {
            warn!("failed to read rawr prompt {}: {err}", path.display());
            default_prompt(kind).to_string()
        }
    }
}

pub fn expand_placeholders(template: &str, values: &[(&str, String)]) -> String {
    let mut out = template.to_string();
    for (key, value) in values {
        out = out.replace(&format!("{{{key}}}"), value);
    }
    out
}

fn default_prompt(kind: RawrPromptKind) -> &'static str {
    match kind {
        RawrPromptKind::AutoCompact => DEFAULT_AUTO_COMPACT_PROMPT,
        RawrPromptKind::ScratchWrite => DEFAULT_SCRATCH_WRITE_PROMPT,
        RawrPromptKind::JudgmentContext => DEFAULT_JUDGMENT_CONTEXT_PROMPT,
        RawrPromptKind::WatcherPacket => DEFAULT_WATCHER_PACKET_PROMPT,
    }
}

fn resolve_prompt_path(codex_home: &Path, raw: &str) -> PathBuf {
    let path = Path::new(raw);
    if path.is_absolute() {
        return path.to_path_buf();
    }
    rawr_prompt_dir(codex_home).join(path)
}

fn write_default_if_missing(path: &Path, contents: &str) -> io::Result<()> {
    if path.exists() {
        return Ok(());
    }
    fs::write(path, contents)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    #[test]
    fn ensure_rawr_prompt_files_creates_defaults() {
        let dir = tempdir().expect("create temp dir");
        let paths = ensure_rawr_prompt_files(dir.path()).expect("create prompt files");
        assert!(paths.auto_compact.exists());
        assert!(paths.scratch_write.exists());
        assert!(paths.watcher_packet.exists());
    }

    #[test]
    fn ensure_rawr_prompt_files_preserves_user_edits() {
        let dir = tempdir().expect("create temp dir");
        let prompt_dir = rawr_prompt_dir(dir.path());
        fs::create_dir_all(&prompt_dir).expect("create prompt dir");
        let auto_compact = prompt_dir.join(RAWR_AUTO_COMPACT_PROMPT_FILE);
        fs::write(&auto_compact, "custom packet prompt").expect("write custom prompt");

        ensure_rawr_prompt_files(dir.path()).expect("ensure prompt files");

        assert_eq!(
            fs::read_to_string(auto_compact).expect("read prompt"),
            "custom packet prompt"
        );
    }

    #[test]
    fn expand_placeholders_replaces_values() {
        let template = "tier={tier} percent={percentRemaining}";
        let output = expand_placeholders(
            template,
            &[
                ("tier", "ready".to_string()),
                ("percentRemaining", "42".to_string()),
            ],
        );
        assert_eq!(output, "tier=ready percent=42");
    }

    #[test]
    fn prompt_path_override_reads_relative_to_prompt_dir() {
        let dir = tempdir().expect("create temp dir");
        let prompt_dir = rawr_prompt_dir(dir.path());
        fs::create_dir_all(&prompt_dir).expect("create prompt dir");
        fs::write(prompt_dir.join("custom.md"), "custom").expect("write prompt");

        let output = read_prompt_path_or_default(
            dir.path(),
            Some("custom.md"),
            RawrPromptKind::WatcherPacket,
        );

        assert_eq!(output, "custom");
    }
}
