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
- The watcher only runs at natural boundaries (turn completion) and will not compact mid-turn.

Default behavior: suggest mode (prints a recommendation once context window drops below 75% remaining).

Optional env vars:
- `RAWR_AUTO_COMPACTION_MODE=tag|suggest|auto` (default: `suggest`)
- `RAWR_AUTO_COMPACTION_PACKET_AUTHOR=watcher|agent` (default: `watcher`, only used in `auto` mode)

## Heuristics prompt (auditable/editable)
Copy the template `codex/rawr/prompts/rawr-auto-compact.md` into:
- `~/.codex-rawr/prompts/rawr-auto-compact.md`

The YAML frontmatter controls thresholds and “auto requires boundary” gating; the Markdown body is used as the prompt when `RAWR_AUTO_COMPACTION_PACKET_AUTHOR=agent`.

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
