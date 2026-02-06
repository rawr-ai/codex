# RAWR Release Readiness (2026-02-05)

## Scope
- Stack: `codex/rebase-upstream-2026-02-05` -> `codex/fork-maintenance-spike` -> `codex/fork-seams-core-tui` -> `codex/rebase-runbook-canonical` -> `codex/policy-home-judgment-audit` -> `codex/high-risk-remediation` -> `codex/rebase-drill-and-gates`
- Top branch commit: `384812f67` (`chore(rawr): bump fork workspace version to 0.100.0-alpha.3`)

## PR Stack State
- #11 `codex/rebase-upstream-2026-02-05 -> main` (DIRTY): <https://github.com/rawr-ai/codex/pull/11>
- #12 `codex/fork-maintenance-spike -> codex/rebase-upstream-2026-02-05` (CLEAN): <https://github.com/rawr-ai/codex/pull/12>
- #13 `codex/fork-seams-core-tui -> codex/fork-maintenance-spike` (CLEAN): <https://github.com/rawr-ai/codex/pull/13>
- #14 `codex/rebase-runbook-canonical -> codex/fork-seams-core-tui` (CLEAN): <https://github.com/rawr-ai/codex/pull/14>
- #15 `codex/policy-home-judgment-audit -> codex/rebase-runbook-canonical` (CLEAN): <https://github.com/rawr-ai/codex/pull/15>
- #16 `codex/high-risk-remediation -> codex/policy-home-judgment-audit` (CLEAN): <https://github.com/rawr-ai/codex/pull/16>
- #17 `codex/rebase-drill-and-gates -> codex/high-risk-remediation` (CLEAN): <https://github.com/rawr-ai/codex/pull/17>

## Local Validation Evidence
All run from `codex-rs` on branch `codex/rebase-drill-and-gates` before local release:
- `just fmt` passed
- `just fix -p codex-core` passed
- `just fix -p codex-protocol` passed
- `just fix -p codex-tui` passed
- `cargo test -p codex-core` passed
- `cargo test -p codex-protocol` passed
- `cargo test -p codex-app-server-protocol` passed
- `cargo test -p codex-tui` passed
- `cargo test --all-features` passed

## CI Evidence
- No CI runs are currently reported for stack branches in `rawr-ai/codex` (`gh run list` returned no entries).
- `statusCheckRollup` is currently empty on open stack PRs.

## Local Release Evidence
- Local release command executed: `bash rawr/release-local.sh --tag rawr-local-20260205-2046`
- Created local tag: `rawr-local-20260205-2046`
- Installed binaries/wrappers:
  - `codex` -> `/Users/mateicanavra/.local/bin/codex`
  - `codex-rawr` -> `/Users/mateicanavra/.local/bin/codex-rawr`
  - `codex-rawr-bin` -> `/Users/mateicanavra/.local/bin/codex-rawr-bin`
- Version verification:
  - `codex --version` => `codex-cli 0.100.0-alpha.3`
  - `codex-rawr --version` => `codex-cli 0.100.0-alpha.3`
  - `codex-rawr-bin --version` => `codex-cli 0.100.0-alpha.3`
- Wrapper isolation default verified in `/Users/mateicanavra/.local/bin/codex`:
  - `CODEX_HOME="${CODEX_HOME:-$HOME/.codex-rawr}"`

## Blockers to Full Green Merge
1. Base PR #11 is not mergeable into `main` (`mergeStateStatus=DIRTY`) and requires a full rebase conflict resolution pass.
2. CI status checks are not present for stack branches in `rawr-ai/codex`, so required-check green status cannot be demonstrated yet.
3. Because #11 is blocked, bottom-up stack merge and post-merge sync are blocked.

## Recommended Next Action
- Resolve and re-stack `codex/rebase-upstream-2026-02-05` onto `main` in a dedicated conflict-resolution pass, then re-run the full validation bar and re-submit stack PRs.
