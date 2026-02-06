# RAWR Release Readiness (2026-02-06)

## Scope
- Phase: `C` (validation + hygiene + publish/release completion)
- Evidence timestamp (UTC): `2026-02-06T09:07:30Z`

## Trunk / Branch State
- Operational trunk: `codex/integration-upstream-main`
- `main` role: upstream mirror branch; not the day-to-day stack base
- Active tracked chain (`gt ls --all`): `codex/integration-upstream-main`
- Current branch/HEAD: `codex/integration-upstream-main` at `b59ec7afb`
- Remote relation now: in sync with `origin/codex/integration-upstream-main`
- Canonical fork PR state (`rawr-ai/codex`): PR `#18` is merged; no open PRs

## Optional Next Upstream Checkpoint (Handoff)
- Next optional checkpoint is to replay `codex/integration-upstream-main` on `upstream/main`, then restack any in-flight branches.
- This optional checkpoint was **not executed now**; Phase C stopped after validation + publish/release completion.

## Publish / Release Evidence
- Local publish: `bash rawr/publish-local.sh --no-bump-version --happy --force` (installed wrappers: `~/.local/bin/codex` + `~/.local/bin/codex-rawr`; local version: `codex-cli 0.100.0-alpha.3`)
- Remote tag pushed: annotated tag `rust-v0.100.0-alpha.3` (`refs/tags/rust-v0.100.0-alpha.3`); tag object `ac17f1f4c5cac22edb395dc0205cea11fb834b08`
- GitHub Release: https://github.com/rawr-ai/codex/releases/tag/rust-v0.100.0-alpha.3 (manual; no CI artifacts attached; assets: none; notes: generated from merged PR `#18`)
- Why manual: `rust-release` workflow exists and is active, but this fork currently has no `push`-triggered Actions runs visible (only `workflow_dispatch` history), so the tag push did not produce a `rust-release` run.

## Validation Evidence (codex-rs)
Executed from `codex-rs` during Phase C:

| Command | Status | Exit | Duration (s) |
| --- | --- | --- | --- |
| `just fmt` | PASS | 0 | 2 |
| `cargo test -p codex-core` | PASS | 0 | 601 |
| `cargo test -p codex-protocol` | PASS | 0 | 15 |
| `cargo test -p codex-app-server-protocol` | PASS | 0 | 43 |
| `cargo test -p codex-tui` | PASS | 0 | 62 |
| `cargo test --all-features` | PASS | 0 | 812 |

Raw logs:
- `.scratch/worker-C-validation-2026-02-06.command-status.tsv`
- `.scratch/worker-C-validation-2026-02-06.commands.log`

## Outcome
- Phase C validation bar: **PASS**
- Trunk model and canonical PR state reflect the post-merge topology.
- Local publish: **PASS**
- Remote tag + GitHub Release: **PASS** (manual Release; no artifacts)
