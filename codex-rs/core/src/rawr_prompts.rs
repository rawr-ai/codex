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

pub const DEFAULT_AUTO_COMPACT_PROMPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../rawr/prompts/rawr-auto-compact.md"
));
pub const DEFAULT_SCRATCH_WRITE_PROMPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../rawr/prompts/rawr-scratch-write.md"
));
pub const DEFAULT_JUDGMENT_PROMPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../rawr/prompts/rawr-auto-compaction-judgment.md"
));
pub const DEFAULT_JUDGMENT_CONTEXT_PROMPT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../rawr/prompts/rawr-auto-compaction-judgment-context.md"
));

#[derive(Debug, Clone, Copy)]
pub enum RawrPromptKind {
    AutoCompact,
    ScratchWrite,
    Judgment,
    JudgmentContext,
}

#[derive(Debug, Clone)]
pub struct RawrPromptPaths {
    pub auto_compact: PathBuf,
    pub scratch_write: PathBuf,
    pub judgment: PathBuf,
    pub judgment_context: PathBuf,
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

    write_default_if_missing(&auto_compact, DEFAULT_AUTO_COMPACT_PROMPT)?;
    write_default_if_missing(&scratch_write, DEFAULT_SCRATCH_WRITE_PROMPT)?;
    write_default_if_missing(&judgment, DEFAULT_JUDGMENT_PROMPT)?;
    write_default_if_missing(&judgment_context, DEFAULT_JUDGMENT_CONTEXT_PROMPT)?;

    Ok(RawrPromptPaths {
        auto_compact,
        scratch_write,
        judgment,
        judgment_context,
    })
}

pub fn read_prompt_or_default(codex_home: &Path, kind: RawrPromptKind) -> String {
    let paths = match ensure_rawr_prompt_files(codex_home) {
        Ok(paths) => paths,
        Err(err) => {
            warn!("failed to ensure rawr prompt directory: {err}");
            return default_prompt(kind).to_string();
        }
    };

    let path = match kind {
        RawrPromptKind::AutoCompact => paths.auto_compact,
        RawrPromptKind::ScratchWrite => paths.scratch_write,
        RawrPromptKind::Judgment => paths.judgment,
        RawrPromptKind::JudgmentContext => paths.judgment_context,
    };

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
        let placeholder = format!("{{{key}}}");
        out = out.replace(&placeholder, value);
    }
    out
}

fn default_prompt(kind: RawrPromptKind) -> &'static str {
    match kind {
        RawrPromptKind::AutoCompact => DEFAULT_AUTO_COMPACT_PROMPT,
        RawrPromptKind::ScratchWrite => DEFAULT_SCRATCH_WRITE_PROMPT,
        RawrPromptKind::Judgment => DEFAULT_JUDGMENT_PROMPT,
        RawrPromptKind::JudgmentContext => DEFAULT_JUDGMENT_CONTEXT_PROMPT,
    }
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
        assert!(paths.judgment.exists());
        assert!(paths.judgment_context.exists());
    }

    #[test]
    fn expand_placeholders_replaces_values() {
        let template = "tier={tier} percent={percentRemaining} list={boundariesJson}";
        let output = expand_placeholders(
            template,
            &[
                ("tier", "ready".to_string()),
                ("percentRemaining", "42".to_string()),
                ("boundariesJson", "[\"commit\"]".to_string()),
            ],
        );
        assert_eq!(output, "tier=ready percent=42 list=[\"commit\"]");
    }
}
