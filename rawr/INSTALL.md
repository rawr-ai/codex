# rawr Codex fork: install + safe switching

Goal: run the fork side-by-side with upstream Codex without sharing state by default.

## Recommended usage
- Keep upstream installed as `codex` (untouched).
- Install this fork as `codex-rawr`.
- By default, this fork uses `CODEX_HOME=~/.codex-rawr` when `CODEX_HOME` is unset.

## Local install (macOS/Linux)
```bash
cd codex/codex-rs
cargo build -p codex-cli --release
install -m 0755 target/release/codex ~/.local/bin/codex-rawr
```

## One-command publish (recommended)
From the repo root:

```bash
bash rawr/publish-local.sh
```

To also point Happy at the fork (overwrites `~/.local/bin/codex`):

```bash
bash rawr/publish-local.sh --happy --force
```

Notes:
- `publish-local.sh` now bumps the fork version before building (to keep `codex --version` ahead of upstream so Happy selects `mcp-server`).
- If you need to skip version bumping, use `--no-bump-version`.

## Release builds (golden path)
Use a dedicated release command for deterministic local releases:

```bash
bash rawr/release-local.sh
```

If you want an explicit tag for traceability:

```bash
bash rawr/release-local.sh --tag rawr-local-YYYYMMDD-HHMM
```

## Make `codex` run the fork (optional)
If you want `codex` to resolve to the fork without uninstalling upstream, the simplest approach is a symlink (or wrapper) earlier in your `PATH`.

Example:
```bash
ln -sf "$HOME/.local/bin/codex-rawr" "$HOME/.local/bin/codex"
```

This is safe with respect to state because the fork defaults `CODEX_HOME=~/.codex-rawr` (unless you override `CODEX_HOME`).

## Enable the watcher (v0 skeleton)
Edit `~/.codex-rawr/config.toml`:
```toml
[features]
rawr_auto_compaction = true
```

When enabled:
- This fork owns compaction timing (Codex’s built-in auto-compaction is bypassed).
- The watcher can compact **mid-turn** (between sampling requests) at natural boundaries, and can also compact at turn completion.
- Core writes a best-effort, side-channel structured state store under `~/.codex-rawr/rawr/auto_compaction/threads/<thread_id>/` for later inspectability (no transcript pollution).

Default behavior: suggest mode (prints a recommendation once context window drops below 75% remaining).

Config settings (explicit, recommended):
```toml
[rawr_auto_compaction]
mode = "auto" # tag | suggest | auto
packet_author = "agent" # watcher | agent
scratch_write_enabled = true
packet_max_tail_chars = 1200
# Defaults to "GPT-5.2 (high)" for watcher-triggered compactions:
# gpt-5.2 + ReasoningEffort::High.
# compaction_model = "gpt-5.2"
# compaction_reasoning_effort = "high"
# compaction_verbosity = "high"

[rawr_auto_compaction.repo_observation]
graphite_enabled = true
graphite_max_chars = 4096

# Preferred: config-driven per-tier policy matrix (overrides thresholds + boundaries).
[rawr_auto_compaction.policy.early]
percent_remaining_lt = 85
requires_any_boundary = ["plan_checkpoint", "plan_update", "pr_checkpoint", "topic_shift"]
plan_boundaries_require_semantic_break = true
# decision_prompt_path = "~/.codex-rawr/prompts/rawr-auto-compaction-judgment.md"

[rawr_auto_compaction.policy.ready]
percent_remaining_lt = 75
requires_any_boundary = ["commit", "plan_checkpoint", "plan_update", "pr_checkpoint", "topic_shift"]
plan_boundaries_require_semantic_break = true
# decision_prompt_path = "~/.codex-rawr/prompts/rawr-auto-compaction-judgment.md"

[rawr_auto_compaction.policy.asap]
percent_remaining_lt = 65
requires_any_boundary = ["commit", "plan_checkpoint", "plan_update", "pr_checkpoint", "agent_done", "topic_shift", "concluding_thought"]

[rawr_auto_compaction.policy.emergency]
percent_remaining_lt = 15
# Emergency tier is a hard bypass; boundaries/judgment are ignored.
```

## Packet prompt + defaults (auditable/editable)
The prompt lives in-repo at `rawr/prompts/rawr-auto-compact.md` and is embedded into the binary at build time.

- YAML frontmatter: default thresholds/boundaries (config overrides win).
- Markdown body: continuation packet prompt when `packet_author = "agent"`.
- Compaction decision: code-driven tier policy + boundary gating; plan-based boundaries additionally require a semantic break (agent-done/topic-shift/concluding) in Early/Ready tiers so we don’t compact mid-thought just because the plan tool ran.
- Scratch write prompt: `rawr/prompts/rawr-scratch-write.md` (enabled by `scratch_write_enabled = true`).

## Using with Happy Coder
Happy Coder’s CLI supports `happy codex` (Codex mode). If your `PATH` resolves `codex` to this fork (e.g. via the symlink above), `happy codex` will launch the fork.

### Pointing `codex` at the fork (optional)
If you want `codex` (not just `codex-rawr`) to run the fork, install a small wrapper at `~/.local/bin/codex` that delegates to `codex-rawr` and sets safe defaults:

- `CODEX_HOME` defaults to `~/.codex-rawr` (keeps upstream Codex isolated)
- watcher defaults to full auto + agent-authored packets

If you use Happy, ensure it also resolves `codex` to this wrapper (Happy typically has `~/.bun/bin` early in `PATH`):

```bash
ln -sf "$HOME/.local/bin/codex" "$HOME/.bun/bin/codex"
happy daemon stop
happy daemon start
```

### MCP compatibility note
Happy Coder launches Codex’s MCP server using `codex mcp-server` (stdio transport). Older version-based heuristics can pick `codex mcp` (the management command) when `codex --version` is `0.0.0`.

This fork avoids that by keeping `codex --version` ahead of upstream (see `rawr/bump-fork-version.sh`), so launchers can reliably select `mcp-server` without shims.

## Update flow (keep rebases boring)
See `codex/rawr/UPDATING.md`.
