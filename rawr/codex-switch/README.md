# Codex switchboard

The switchboard installs one stable `codex` selector and one `codex-use`
management command so local consumers can flip between upstream Codex and the
RAWR fork without mixing binaries and homes.

```bash
bash rawr/codex-switch/install.sh --target upstream
~/.codex-switch/bin/codex-use rawr
~/.codex-switch/bin/codex-use upstream
~/.codex-switch/bin/codex-use status
~/.codex-switch/bin/codex-use doctor
```

Targets are intentionally limited in v1:

- `upstream`: Homebrew Codex at `/opt/homebrew/bin/codex` or
  `/usr/local/bin/codex` when present, otherwise `/Users/mateicanavra/.volta/bin/codex`,
  with `~/.codex`
- `rawr`: `~/.local/bin/codex-rawr-bin` with `~/.codex-rawr`

Override paths for another machine with:

```bash
CODEX_SWITCH_UPSTREAM_BIN=/path/to/upstream/codex \
CODEX_SWITCH_UPSTREAM_HOME=/path/to/upstream-home \
CODEX_SWITCH_RAWR_BIN=/path/to/rawr/codex \
CODEX_SWITCH_RAWR_HOME=/path/to/rawr-home \
bash rawr/codex-switch/install.sh --target upstream
```

## What gets managed

The installer writes under `~/.codex-switch/`:

- `bin/codex`: selector; reads `current.json`, exports `CODEX_HOME`, then execs
  the selected target.
- `bin/codex-use`: management command.
- `targets/{upstream,rawr}.json`: discovered target records.
- `current.json`: atomic selected-target state.
- `env.sh`: shell/GUI environment projection for the selected target.
- `state/install-manifest.json`: manifest and backup ledger.
- `state/protected-home-snapshots/`: pre-switch snapshots of protected
  config/state files from both homes.

It can also reconcile:

- `~/.local/bin/codex`
- `~/.bun/bin/codex`
- managed shell startup blocks
- VS Code `chatgpt.cliExecutable`
- launchd GUI `CODEX_HOME`
- Codex Desktop helper and restore LaunchAgent on macOS

The selector always overrides inherited `CODEX_HOME`. That is deliberate:
upstream binary plus RAWR home is not a clean upstream switch.

Switching never copies `config.toml` between homes. Config transfer is a
separate recovery/migration operation. Before each live switch, `codex-use`
snapshots protected files from both homes: `config.toml`,
`.codex-global-state.json`, `session_index.jsonl`, and `auth.json` metadata
only.

`codex-use doctor` also checks for config shrinkage, cross-home paths, stale
process target markers, direct shell `CODEX_HOME` exports, and app-servers that
bypass the selector.

## Desktop behavior

Codex Desktop does not obey shell `PATH`; it runs the helper inside the app
bundle. The switchboard patches that helper to the selector and installs a
LaunchAgent that restores the selector after app updates.

`codex-use` does not force-quit Desktop. `codex-use doctor` reports running
Desktop app-server processes as `Needs restart` when disk wiring is correct but
the live process may predate the selected target.

## Rollback

```bash
~/.codex-switch/bin/codex-use uninstall
```

Uninstall is guarded: run `codex-use uninstall --dry-run` first. A real
uninstall requires `CODEX_SWITCHBOARD_UNINSTALL_CONFIRM=1` so manifest-backed
restores are deliberate. It leaves config homes, logs, and stale global installs
in place by default.
