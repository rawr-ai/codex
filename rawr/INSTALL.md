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

## Update flow (keep rebases boring)
See `codex/rawr/UPDATING.md`.
