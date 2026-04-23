use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

use tracing::warn;

pub const RAWR_PROMPT_DIR_NAME: &str = "auto-compact";
pub const RAWR_AUTO_COMPACT_PROMPT_FILE: &str = "auto-compact.md";
pub const RAWR_SCRATCH_WRITE_PROMPT_FILE: &str = "scratch-write.md";

const DEFAULT_AUTO_COMPACT_PROMPT: &str = "\
[rawr] Before we compact this thread, produce a continuation context packet for yourself.

Requirements:
- Keep it short and structured.
- Include: overarching goal, current state, next steps, invariants/decisions, and a final directive to continue after compaction.
- Do not include secrets; redact tokens/keys.
";

const DEFAULT_SCRATCH_WRITE_PROMPT: &str = "\
[rawr] Before we compact this thread, write a scratchpad file with what you just worked on.

Target file: `{scratch_file}`

Requirements:
- Create the `.scratch/` directory if it doesn't exist.
- Append a new section; do not delete prior scratch content.
- Prefer verbatim notes/drafts over summaries.
- Include links/paths to any important files you edited or created.
";

#[derive(Debug, Clone, Copy)]
pub enum RawrPromptKind {
    AutoCompact,
    ScratchWrite,
}

#[derive(Debug, Clone)]
pub struct RawrPromptPaths {
    pub auto_compact: PathBuf,
    pub scratch_write: PathBuf,
}

pub fn rawr_prompt_dir(codex_home: &Path) -> PathBuf {
    codex_home.join(RAWR_PROMPT_DIR_NAME)
}

pub fn ensure_rawr_prompt_files(codex_home: &Path) -> io::Result<RawrPromptPaths> {
    let dir = rawr_prompt_dir(codex_home);
    fs::create_dir_all(&dir)?;

    let auto_compact = dir.join(RAWR_AUTO_COMPACT_PROMPT_FILE);
    let scratch_write = dir.join(RAWR_SCRATCH_WRITE_PROMPT_FILE);

    write_default_if_missing(&auto_compact, DEFAULT_AUTO_COMPACT_PROMPT)?;
    write_default_if_missing(&scratch_write, DEFAULT_SCRATCH_WRITE_PROMPT)?;

    Ok(RawrPromptPaths {
        auto_compact,
        scratch_write,
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
        out = out.replace(&format!("{{{key}}}"), value);
    }
    out
}

fn default_prompt(kind: RawrPromptKind) -> &'static str {
    match kind {
        RawrPromptKind::AutoCompact => DEFAULT_AUTO_COMPACT_PROMPT,
        RawrPromptKind::ScratchWrite => DEFAULT_SCRATCH_WRITE_PROMPT,
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
}
