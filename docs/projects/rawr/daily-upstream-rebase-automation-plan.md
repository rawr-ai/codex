## Step 0: Write Down This Plan Exactly As-Is (Before Doing Anything Else)

1. Create a committed planning artifact at `/Users/mateicanavra/Documents/.nosync/DEV/rawr-ai/codex/docs/projects/rawr/daily-upstream-rebase-automation-plan.md`.
2. Paste the full plan below into that file **verbatim**, with no edits, no reformatting, and no rewording (including headings, bullets, paths, and acceptance criteria).
3. Commit this as its own first Graphite slice off `codex/integration-upstream-main` so we have an immutable baseline to execute against.

---

## Parallel Activity (Ongoing During Slices 1-4): Background Graphite Practices Agent

While Slices 1-4 are being implemented, run one dedicated background agent whose only job is to review and report on Graphite usage for this exact approach.

**Agent mission**
- Deep-read:
  - `/Users/mateicanavra/.codex-rawr/skills/graphite/SKILL.md`
  - `/Users/mateicanavra/.codex-rawr/skills/fork-rebase-maintenance/SKILL.md` (and references, especially `graphite-optional.md`)
- Consult official Graphite docs as needed to answer: “Given we are doing a controlled upstream checkpoint rebase on a Graphite trunk, what extra Graphite best practices or commands should we adopt?”
- Pay special attention to:
  - Safe sync patterns (`gt sync --no-restack`) and restack scope
  - Tracking/parentage drift recovery
  - Submitting/PR best practices in a trunk-plus-descendants model
  - Any official guidance around rebases/history rewrites in a Graphite-managed repo
- Produce a memo with:
  - “We’re already doing the right thing” (ideal), or
  - A concrete list of recommended upgrades (commands, guardrails, doc callouts)
  - Each recommendation must include: why, when to use, and how it fits the plan without expanding scope.

**Output**
- A short report (Markdown) that can be dropped into the repo as a doc addendum in a follow-up slice if needed.

---

## Automation Boundaries (Agent-First, Non-Deterministic Rebase)

This plan automates **mechanical, idempotent, and atomic** steps (scripts/CLI/tools) and explicitly does **not** assume deterministic rebasing is possible.

**Operating model**
- A primary LLM “rebase orchestrator agent” owns the rebase end-to-end.
- Scripts/CLI are *tools* the agent runs to perform mechanical steps safely (locks, snapshots, verify attempts, leases, reports, restacks, tests).
- Anything requiring judgment (conflict resolution, policy/logic decisions, semantic merges, deciding what changes to keep) is owned by the agent itself (and escalated to a human only at clearly defined gates).

**Scheduling model**
- “Daily schedule” means: a scheduler launches the **agent with the dedicated prompt**, and the agent runs the tools and performs judgment work as needed.
- Direct cron-running of scripts is allowed only as a fallback for purely mechanical verification/reporting, not as a replacement for agent-owned conflict resolution.

---

# Daily Upstream Rebase Automation Hardening (RAWR Codex Fork)

## Summary

We will turn the current “checkpoint rebase” docs and scripts into a **repeatable, daily-schedulable, agent-executable** workflow that:

- Uses the **explicit trunk** `codex/integration-upstream-main` as the only upstream-rebase target.
- Uses **Graphite where relevant** (stack topology, restacks, PR/branch hygiene), while keeping **plain Git** for the **controlled trunk rebase** itself (policy exception).
- Is **idempotent**, **lock-protected**, and **self-cleaning** (never strands the repo mid-rebase).
- Produces a **structured run report** every time (success or failure).
- Avoids stale, cycle-specific constants (dated branch names, PR numbers) in “canonical” docs.

Deliverables include:
- Script hardening (real dry-run verification, safe leases, idempotent version bump, locks, reports).
- Runbook refactor (automation contract: inputs, invariants, stop conditions, recovery).
- A dedicated **agent prompt** that instructs a “rebase orchestrator agent” exactly how to execute the runbook daily.

Skills used: `fork-rebase-maintenance` (mirror + patch-queue discipline, range-diff, leases), `graphite` (safe restack/sync), `parallel-development-workflow` (no global restacks in multi-agent contexts).

---

## Current Ground Truth (What This Plan Assumes)

- Repo is Graphite-initialized and trunk is already set to `codex/integration-upstream-main` (confirmed by `.git/.graphite_repo_config`).
- Remotes exist: `origin` (fork) and `upstream` (OpenAI Codex).
- The daily job runs on a **single orchestrator host** that already has auth for `git`, `gt`, and (optionally) `gh`.
- `main` is an upstream mirror branch (not the day-to-day base).

---

## Design Decisions (Locked In)

1. **Daily process = verify-first, apply-second.**
   - Every run starts with a *real* “dry-run verification” that proves the rebase can complete without conflicts **without rewriting the real trunk**.
   - Only if verification passes do we proceed to apply the real checkpoint rebase.

2. **Controlled exception to Graphite-first:**
   - The only allowed history rewrite via plain `git rebase` is the upstream checkpoint on `codex/integration-upstream-main` performed by the official script(s).
   - Everything else that mutates stack topology uses `gt` (sync/restack/submit/delete).

3. **No more cycle-specific constants in canonical docs.**
   - Canonical docs cannot hardcode branch names like `codex/incremental-rebase-YYYY-MM-DD` or PR numbers like `#18`.
   - The run discovers active descendants via Graphite (`gt ls --all`) and/or configured inputs.

4. **Full-suite validation becomes automatic for scheduled runs.**
   - The previous “ask before `cargo test --all-features`” becomes:  
     - Interactive/manual runs: still a human gate.  
     - Scheduled daily runs: **auto-run full suite** (nightly safety bar).
   - This is the main policy change needed to make the daily run truly autonomous.

---

## Implementation Slices (Graphite Stack Plan)

All slices branch off **`codex/integration-upstream-main`** and are stacked using Graphite. Suggested branch names follow your repo convention (`codex/...`).

### Slice 1: Make the Sync Scripts Actually Automatable (Correctness + Idempotency)

**Goal:** Fix the two biggest automation blockers:
- “DRY_RUN does nothing but claims success”
- “version bump can fail when already up-to-date”

**Changes**
1. Update `/Users/mateicanavra/Documents/.nosync/DEV/rawr-ai/codex/rawr/bump-fork-version.sh`
   - Make it **idempotent**:
     - If `current_version == fork_version`, then:
       - `--apply`: do nothing and exit 0.
       - `--commit`: do nothing and exit 0 (no commit attempted).
   - This prevents daily runs from failing on “nothing to commit”.

2. Update `/Users/mateicanavra/Documents/.nosync/DEV/rawr-ai/codex/rawr/sync-upstream.sh`
   - Replace the current `DRY_RUN` no-op wrapper with a **real verification mode** that does not touch real branch tips.
   - Add a true “verify” path that:
     - Creates a **temporary worktree** + **temporary branch** from the target patch branch tip.
     - Attempts `git rebase upstream/main` on the temp branch.
     - If conflicts: abort and report; if clean: report success.
     - Always cleans up the temp worktree/branch (even on failure).
   - Keep “apply” path that performs the real sequence (update mirror `main`, rebase patch branch, bump version, push).
   - Strengthen push safety:
     - Fetch the remote branch SHA first and use explicit lease form:
       - `--force-with-lease=refs/heads/<branch>:<expected_sha>`
     - This avoids “stale remote-tracking branch” pitfalls.
   - Ensure the script never leaves you mid-rebase:
     - `trap` that runs `git rebase --abort` if in-progress and restores the starting branch.
   - Add a lock (see Slice 2 for shared lock design) or at minimum a lock inside the script.

**Acceptance criteria**
- `DRY_RUN=1 rawr/sync-upstream.sh codex/integration-upstream-main` becomes a *real* conflict detector.
- Running the script multiple times on the same upstream tag does not fail due to version bump no-op.
- Script exits with:
  - `0` on verify success or apply success
  - non-zero on conflicts, lease failure, branch protection failure, or unexpected state
- Working tree remains clean at end of both success and failure paths.

**Tests / validation**
- Shell sanity: `bash -n rawr/sync-upstream.sh` and `bash -n rawr/bump-fork-version.sh`
- Manual local rehearsal (non-mutating verify mode) to confirm it actually tries a rebase on a temp branch.

---

### Slice 2: Introduce “Daily Rebase Orchestrator” Script (Locking + Reports + Deterministic Cleanup)

**Goal:** Provide a single entrypoint that a scheduler/agent can run daily and trust.

**Add**
- New script: `/Users/mateicanavra/Documents/.nosync/DEV/rawr-ai/codex/rawr/rebase-daily.sh` (or similarly named)
  - Responsibilities:
    - Acquire an **exclusive lock** (local single-writer guarantee).
    - Capture “before” SHAs for:
      - `upstream/main`, `origin/main`, `origin/codex/integration-upstream-main`
    - Run verify:
      - `DRY_RUN=1 rawr/sync-upstream.sh codex/integration-upstream-main`
    - If verify passes, run apply:
      - `rawr/sync-upstream.sh codex/integration-upstream-main`
    - Graphite alignment:
      - `gt sync --no-restack`
      - If there are tracked descendants: restack them safely (details below).
    - Validation gates (scheduled-run policy):
      - `cd codex-rs && just fmt`
      - `cargo test --all-features`
    - If tests fail:
      - Do **not** push further branches.
      - Write report and stop (human follow-up required).
    - Produce structured output:
      - Write a JSON report file under `.scratch/rebase-daily/<YYYY-MM-DD>/report.json`
      - Write a Markdown summary under `.scratch/rebase-daily/<YYYY-MM-DD>/summary.md`
    - Exit codes (decision-complete):
      - `0`: success (rebased + validated)
      - `10`: verify conflict (no changes applied)
      - `11`: apply conflict (changes started but aborted cleanly)
      - `12`: lease/push blocked
      - `13`: Graphite restack failure
      - `14`: tests failed
      - `15`: lock acquisition failed (another run in progress)

**Lock design**
- Local lock dir: `.scratch/locks/rebase-daily.lock/`
  - Acquire via `mkdir` (atomic).
  - Write `meta.json` with pid, start time, hostname, git sha snapshot.
  - Stale lock policy:
    - If lock age > 6 hours, allow override only when `FORCE_STALE_LOCK=1` is set, and record that in report.

**Graphite descendant restack rule (precise)**
- After trunk rewrite:
  - Always run `gt sync --no-restack`.
  - Then:
    - If `gt ls --all` shows only trunk, do nothing else.
    - If there are tracked descendants, run `gt restack --upstack` starting from trunk (or from the first child, depending on Graphite behavior in this repo) and verify with `gt ls --all` that parentage is consistent.
- Multi-agent safety rule:
  - Never run `gt sync` without `--no-restack` inside the daily script.

**Acceptance criteria**
- One command (`rawr/rebase-daily.sh`) is sufficient for a daily run.
- Any failure path leaves:
  - No in-progress rebase
  - Working tree clean
  - A report file written describing exactly what happened and where it stopped

---

### Slice 3: Refactor Canonical Runbook and Gotchas to Remove Stale Constants and Encode the Automation Contract

**Goal:** Make the docs execute identically whether a human follows them or an agent does.

**Update**
1. `/Users/mateicanavra/Documents/.nosync/DEV/rawr-ai/codex/docs/projects/rawr/rebase-runbook.md`
   - Convert from “this cycle” wording to “always true” wording:
     - Remove “Active tracked chain (current cycle)” hardcoding.
     - Remove PR number references.
     - Replace with:
       - “Discover descendants via `gt ls --all`”
       - “If none, restack step is a no-op”
   - Add explicit “Daily automation contract” section:
     - Inputs (branch names, remotes, schedule environment assumptions)
     - Stop conditions + exit codes
     - Deterministic cleanup expectations
     - Where reports live
   - Replace the current command skeleton with:
     - “verify” command (real dry run)
     - “apply” command
     - “validate” command
     - “report” location

2. `/Users/mateicanavra/Documents/.nosync/DEV/rawr-ai/codex/rawr/UPDATING.md`
   - Update it to:
     - point to the daily orchestrator script as the preferred entrypoint
     - keep the “manual fallback” but align it with the same invariants (no cycle constants)

3. `/Users/mateicanavra/Documents/.nosync/DEV/rawr-ai/codex/docs/projects/rawr/rebase-gotchas.md`
   - Remove “this cycle” PR/branch specifics.
   - Add explicit “automation hazards”:
     - DRY_RUN must be real
     - idempotent version bump
     - lock required
     - explicit leases
     - don’t global restack in parallel environments

4. `/Users/mateicanavra/Documents/.nosync/DEV/rawr-ai/codex/docs/projects/rawr/fork-policy-decisions.md`
   - Update policy wording to reflect:
     - scheduled runs auto-run full suite
     - the “only allowed git rebase” exception (trunk checkpoint rebase via script)

**Acceptance criteria**
- A fresh agent can follow the docs without guessing which branch/PR is “current”.
- The docs explicitly allow: “no descendants exists; restack is skipped.”
- The docs explicitly encode “scheduled runs auto-run full suite” (no hidden human gate).

---

### Slice 4: Add the Dedicated Agent Prompt (The Daily Rebase Orchestrator)

**Goal:** Provide a single prompt file that makes an agent reliably run the process end-to-end.

**Add**
- New prompt file (recommended location consistent with other RAWR prompts):
  - `/Users/mateicanavra/Documents/.nosync/DEV/rawr-ai/codex/rawr/prompts/rawr-upstream-rebase-orchestrator.md`

**Prompt content (decision-complete)**
- **Role:** “RAWR Upstream Rebase Orchestrator (Daily)”
- **Non-negotiable invariants:**
  - Operate only on `/Users/mateicanavra/Documents/.nosync/DEV/rawr-ai/codex`
  - Operational trunk is `codex/integration-upstream-main` (from `.git/.graphite_repo_config`)
  - Must hold lock before mutating anything
  - Never leave repo dirty or mid-rebase
  - Use `--force-with-lease` only with explicit expected SHA
  - Never run `gt sync` without `--no-restack`
- **Procedure:** (agent must follow exactly)
  1. Preflight:
     - `git status --porcelain` empty
     - verify remotes
     - `gt ls --all`
  2. Acquire lock (fail-fast if taken)
  3. Baseline snapshot:
     - record SHAs for trunk, origin trunk, upstream/main, origin/main
     - record `gt ls --all`
  4. Verify mode:
     - run the verify command (real dry-run)
     - if verify fails: write report, release lock, stop
  5. Apply mode:
     - run apply command
  6. Graphite alignment:
     - `gt sync --no-restack`
     - restack descendants if any
  7. Validation (scheduled-run policy):
     - `cd codex-rs && just fmt`
     - `cargo test --all-features`
  8. Reporting:
     - write `.scratch/rebase-daily/<date>/report.json` and `summary.md`
     - include: what changed, SHAs before/after, commands run, exit code, failures if any
  9. Release lock
- **Escalation rules (hard stop):**
  - conflicts (verify or apply)
  - lease/push blocked
  - tests failing
  - anything that implies semantic behavior change in fork policy (explicitly listed)
- **Outputs required from the agent (in-chat):**
  - A compact “Run Summary” with:
    - status, SHAs, whether any restack happened, test results, report path
  - If failure:
    - “Next human actions” checklist, based strictly on runbook recovery section

**Optional multi-agent substructure (embedded in the prompt, but not required every day)**
- Agent A: upstream delta analysis + conflict hotspot prediction (read-only)
- Agent B: validation plan + “which crates likely impacted” (read-only)
- Agent C: policy guardrails reviewer (read-only)
- Orchestrator agent runs the actual scripts and owns locks

---

### Slice 5 (Optional but Recommended): Long-Lived Observability PR

**Goal:** Make fork delta continuously reviewable without inventing new PR numbers each cycle.

**Policy**
- Keep a single PR open in `rawr-ai/codex`:
  - Base: `main` (upstream mirror)
  - Head: `codex/integration-upstream-main` (fork delta)
- Label it `fork-delta` and treat it as a dashboard, not something to merge.

**Automation integration (optional)**
- The daily agent checks that this PR exists and is open; if missing, it creates it once and records the link in the report.

---

## Scheduler Integration (How This Becomes “Daily”)

### Recommended default (single orchestrator host)
- Run once per day via cron (or your preferred scheduler) calling:
  - `bash rawr/rebase-daily.sh`
- Ensure the scheduler environment has:
  - `git`, `gt`, `cargo`, `just`, and your usual auth for `origin`
- The lock prevents overlapping runs.

### Success definition (“Typical case”)
A typical day is one where:
- `DRY_RUN` verify succeeds without conflicts
- apply succeeds with lease-safe push
- tests pass
- a report is written
- repo remains clean

---

## Public Interfaces / API Changes

- No user-facing Rust API changes are required for the rebase automation itself.
- This plan changes **operational interfaces**:
  - `rawr/bump-fork-version.sh` becomes idempotent.
  - `rawr/sync-upstream.sh` gets a real verification path (and stronger safety).
  - New `rawr/rebase-daily.sh` becomes the canonical daily entrypoint.
  - Docs shift from “this cycle” constants to discoverable, stable rules.

---

## Test Cases and Scenarios (Must-Pass Before Calling It Automatable)

1. **No-op day**
- Upstream has no new commits.
- Verify succeeds.
- Apply should do nothing harmful (may still update mirror/main; no failures).
- Version bump no-ops cleanly.
- Report indicates “no changes”.

2. **Clean rebase day**
- Upstream advanced with non-conflicting commits.
- Verify succeeds.
- Apply rebases trunk and pushes with lease.
- Full test suite passes.

3. **Conflict detected in verify**
- Verify hits conflicts on temp branch.
- Script exits with conflict code.
- No branch tips changed.
- Report indicates conflict files and current patch where it failed.

4. **Conflict during apply**
- Apply hits conflicts.
- Script aborts rebase, restores starting branch, exits non-zero.
- Repo remains clean at end.
- Report includes “how to reproduce” commands for a human.

5. **Lease failure**
- Remote trunk advanced unexpectedly.
- Push fails with lease.
- Script stops; report includes expected vs actual.

6. **Stale lock**
- Lock exists from previous crash.
- Without override env var, run refuses.
- With override env var, run proceeds and records override.

7. **Graphite present but no descendants**
- `gt ls --all` shows only trunk.
- Restack step is a no-op; report says so.

8. **Graphite descendants present**
- `gt ls --all` includes children.
- Restack runs and parentage remains consistent.

---

## Explicit Assumptions (Defaults Chosen)

- We will keep `codex/integration-upstream-main` as the sole upstream-rebase target (no rebasing children directly).
- Scheduled daily runs will auto-run `cargo test --all-features` (no human gate), because autonomy requires a non-interactive safety bar.
- The daily job runs on a single host with Graphite configured; we use local locking for single-writer protection.
- Reports live under `.scratch/` (not committed) and the agent also summarizes results in-chat.
