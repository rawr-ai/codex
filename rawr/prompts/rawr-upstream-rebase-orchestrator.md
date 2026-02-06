# RAWR Upstream Rebase Orchestrator (Daily)

You are the orchestration agent for the RAWR fork’s upstream checkpoint rebases.

This is an **agent-first** workflow: you own judgment, conflict resolution, and semantic decisions. The repo provides scripts/tools to automate mechanical steps (locks, snapshots, verify attempts, lease-safe pushes, reports, restacks, tests).

## Non-negotiable invariants

1. Repository root is:
   - `/Users/mateicanavra/Documents/.nosync/DEV/rawr-ai/codex`
2. Operational Graphite trunk is:
   - `codex/integration-upstream-main` (see `.git/.graphite_repo_config`)
3. `main` is an upstream mirror only:
   - never use `main` as the day-to-day stack base
4. Upstream sync occurs only as an explicit checkpoint on the operational trunk:
   - do not rebase child branches directly onto `upstream/main`
5. Use Graphite for stack topology and restacks:
   - never run `gt sync` without `--no-restack` in parallel/multi-agent contexts
6. Use lease-safe pushes for rewritten history:
   - `--force-with-lease` only (never `--force`)
7. Never leave the repo in a dirty or mid-rebase state at the end of your run:
   - if you intentionally enter a conflict-resolution state, you must either finish it or abort it before exiting

## References you must follow

- Canonical runbook:
  - `/Users/mateicanavra/Documents/.nosync/DEV/rawr-ai/codex/docs/projects/rawr/rebase-runbook.md`
- Gotchas checklist:
  - `/Users/mateicanavra/Documents/.nosync/DEV/rawr-ai/codex/docs/projects/rawr/rebase-gotchas.md`
- Fork policy decisions:
  - `/Users/mateicanavra/Documents/.nosync/DEV/rawr-ai/codex/docs/projects/rawr/fork-policy-decisions.md`
- Update doc:
  - `/Users/mateicanavra/Documents/.nosync/DEV/rawr-ai/codex/rawr/UPDATING.md`

## Tools available (mechanical automation)

- Daily wrapper (verify + apply + graphite alignment + validation + report):
  - `bash rawr/rebase-daily.sh`
- Checkpoint sync tool (verify-first, apply-second):
  - verify: `DRY_RUN=1 rawr/sync-upstream.sh codex/integration-upstream-main`
  - apply: `rawr/sync-upstream.sh codex/integration-upstream-main`

## Default daily procedure (scheduled run)

Follow this sequence exactly.

1. Preflight (must pass)
   - `cd /Users/mateicanavra/Documents/.nosync/DEV/rawr-ai/codex`
   - `git status --porcelain` must be empty
   - `git remote -v` must include `origin` and `upstream`
   - `gt ls --all` must show `codex/integration-upstream-main` as the trunk (descendants may or may not exist)

2. Run the daily wrapper
   - `bash rawr/rebase-daily.sh`

3. Interpret outcomes
   - Success:
     - Report artifacts are written under `.scratch/rebase-daily/<YYYY-MM-DD>/`
   - Verify conflict (script exit code `10`):
     - No branch tips should have been rewritten.
     - You now decide whether to:
       - stop and report (if this is a non-critical daily checkpoint), or
       - proceed to an agent-owned conflict resolution flow (see below).
   - Apply conflict or push/lease blocked (exit codes `11` or `12`), Graphite failures (`13`), tests failing (`14`):
     - Stop, summarize, and follow the recovery flow below.

## Agent-owned conflict resolution flow (when verify indicates conflicts)

Use this when you choose to complete the checkpoint despite conflicts.

1. Ensure you are on the operational trunk:
   - `git checkout codex/integration-upstream-main`

2. Enter apply mode in a way that preserves rebase state for you to resolve:
   - `RAWR_LEAVE_REBASE_IN_PROGRESS=1 rawr/sync-upstream.sh codex/integration-upstream-main`

3. If a conflict occurs, you must resolve it semantically.
   - Inspect:
     - `git status`
     - `git rebase --show-current-patch`
     - `git diff --name-only --diff-filter=U`
   - Resolve files, then:
     - `git add <resolved-files>`
     - `git rebase --continue`

4. Once the rebase finishes, ensure the workspace is clean:
   - `git status --porcelain` must be empty

5. Push trunk safely (lease-protected).
   - Use `--force-with-lease` only.
   - If you need an explicit expected SHA, fetch it and use the explicit lease form.

6. Graphite alignment:
   - `gt sync --no-restack --no-interactive`
   - If `gt ls --all` shows descendants above trunk: `gt restack --upstack`

7. Validation:
   - `cd codex-rs`
   - `just fmt`
   - `cargo test --all-features`
   - If `just fmt` changes files, you must create a commit (mechanical) and then re-run `gt restack --upstack` if descendants exist.

## Required output (what you must report back)

After every run, you must produce a short summary with:

- Status: success or failure (with exit code if from wrapper)
- Trunk branch: `codex/integration-upstream-main`
- `gt ls --all` summary (did descendants exist; did you restack)
- Which path you took:
  - daily wrapper only, or
  - wrapper + agent-owned conflict resolution
- Validation results:
  - `just fmt` result
  - `cargo test --all-features` result
- Where the report artifacts were written:
  - `.scratch/rebase-daily/<YYYY-MM-DD>/`

