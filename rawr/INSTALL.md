# rawr Codex fork: install + safe switching

Goal: run the fork side-by-side with upstream Codex without sharing state by default.

## Recommended usage
- Keep upstream installed as `codex` (untouched).
- Install this fork as `codex-rawr`.
- Fork wrappers/scripts default `CODEX_HOME=~/.codex-rawr` when `CODEX_HOME` is unset.

## Launcher surfaces are separate
Changing one launcher does not automatically change the others:

| Surface | What it runs | rawr wiring |
| --- | --- | --- |
| Terminal | First `codex`/`codex-rawr` found in shell `PATH` | `codex-rawr` is the fork. Optionally install a `codex` wrapper with `rawr/publish-local.sh --link-codex --force`. |
| Happy Coder | The `codex` resolved by Happy's daemon environment | Use `rawr/publish-local.sh --happy --force` or `rawr/release-local.sh`, then restart Happy so it sees the wrapper/symlink. |
| VS Code / IDE extensions | Extension-managed Codex integration, not the Desktop app bundle | Configure the extension separately; shell `PATH` and Desktop bundle patching do not guarantee the IDE uses the fork. |
| Codex Desktop | The helper binary bundled inside `Codex.app` | Patch `/Applications/Codex.app/Contents/Resources/codex` explicitly; shell wrappers do not affect it. |

## Local install (macOS/Linux)
```bash
bash rawr/install-local.sh
```

This installs:

- `~/.local/bin/codex-rawr-bin`: the built fork binary.
- `~/.local/bin/codex-rawr`: wrapper that defaults `CODEX_HOME=~/.codex-rawr`.

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

Recommended:
```bash
bash rawr/publish-local.sh --link-codex --force
```

Manual equivalent:
```bash
ln -sf "$HOME/.local/bin/codex-rawr" "$HOME/.local/bin/codex"
```

This is safe with respect to state because the fork wrapper defaults `CODEX_HOME=~/.codex-rawr` (unless you override `CODEX_HOME`).

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
At runtime, editable prompt files live under `CODEX_HOME/auto-compact/`:

- `auto-compact.md`: continuation packet prompt when `packet_author = "agent"`.
- `scratch-write.md`: scratch-write prompt when `scratch_write_enabled = true`.

If these files are missing, Codex creates them with built-in defaults. Config-driven thresholds and boundaries still live in `config.toml`; config overrides win.

- Compaction decision: code-driven tier policy + boundary gating; plan-based boundaries additionally require a semantic break (agent-done/topic-shift/concluding) in Early/Ready tiers so we don’t compact mid-thought just because the plan tool ran.

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

## Using with VS Code / IDE extensions
VS Code, Cursor, and Windsurf use their own extension integration. Treat that as a separate surface from terminal and Desktop:

- Terminal wrappers affect IDEs only if the extension explicitly shells out to the same `codex` path.
- Desktop patching does not change the IDE extension's bundled or configured runtime.
- Verify the extension's effective command/path from the IDE settings or extension logs before assuming it is using rawr.

## Using with Codex Desktop
Codex Desktop is not controlled by shell `PATH`. On macOS, the app bundle includes its own Codex helper binary at:

```bash
/Applications/Codex.app/Contents/Resources/codex
```

If you want Desktop to run this fork, patch that bundled helper directly. This is intentionally a local patch: Codex Desktop app updates can overwrite the bundle and restore the upstream helper, so re-verify after every app update.

### Desktop patch
From the repo root:

```bash
bash rawr/patch-desktop-app.sh --report
bash rawr/patch-desktop-app.sh --apply
```

The script verifies both versions and hashes after patching. If `/Applications/Codex.app` is not writable by your user, rerun the apply command with `sudo` while preserving the relevant environment variables:

```bash
sudo CODEX_RAWR_BIN="$HOME/.local/bin/codex-rawr-bin" bash rawr/patch-desktop-app.sh --apply
```

### Desktop rollback
Use the latest backup created during patching:

```bash
bash rawr/patch-desktop-app.sh --rollback
```

To restore a specific backup, pass its path:

```bash
bash rawr/patch-desktop-app.sh --rollback /Applications/Codex.app/Contents/Resources/rawr-backups/codex.YYYYMMDDTHHMMSSZ.bak
```

### Desktop verification
After patching, rollback, or a Desktop app update:

```bash
/Applications/Codex.app/Contents/Resources/codex --version
command -v codex
codex --version
```

The first command verifies Desktop's bundled helper. The second and third verify the terminal surface. They are expected to differ unless you patched both surfaces.

## Update flow (keep rebases boring)
See `rawr/UPDATING.md`.
