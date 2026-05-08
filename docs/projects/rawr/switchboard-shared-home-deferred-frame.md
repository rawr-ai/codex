# Deferred Frame: Switchboard Shared-Home Inversion

Status: deferred decision, not approved for implementation.

Last captured: 2026-05-08

## Loud And Clear

We might redesign the Codex switchboard later, but we are not doing that now.

This document records the pending frame so the reasoning survives cleanup and
branch closure. It is not an implementation plan, not an approval to change the
switchboard, and not a request to migrate homes again.

## The Decision We Are Deferring

The current switchboard swaps both:

- the selected Codex binary; and
- the selected `CODEX_HOME`.

The deferred idea is to invert that model: make upstream Codex the default daily
driver, keep durable user data centered on the upstream home, and only swap the
minimum runtime posture needed when using the fork.

The hoped-for benefit is lower fork coupling. Upstream can keep moving quickly,
the fork can rebase in the background, and fork-specific behavior can ideally
become additive configuration or runtime overlay rather than a reason to keep
rebasing urgently.

## Current Frame

The better mental model is not "make switching faster." It is:

> Separate durable user data from volatile runtime posture.

`CODEX_HOME` is not just configuration. It is a state root containing config,
auth, sessions, SQLite state, logs, plugin caches, system skills, app state,
temporary runtime files, and process-owned files. Treating it as a simple config
directory is the unsafe assumption.

## Current Recommendation

Do not move to one live shared `CODEX_HOME` yet.

The safer future direction is:

```text
shared config intent
  + target-specific generated config.toml
  + target-specific runtime state overlays
  + explicit compatibility gates
```

In practice, that means:

- upstream remains the default/canonical daily-driver home at `~/.codex`;
- the fork remains isolated enough that a lagging fork cannot corrupt upstream
  runtime state;
- shared settings should be represented as declarative intent and generated
  into target-specific effective config files;
- mutable state such as SQLite, plugin caches, system skill caches, and temp
  runtime data should remain target-specific unless proven compatible.

## What Can Likely Be Shared Later

These are candidates for shared declarative intent, with validation:

- model/profile/sandbox/project settings;
- MCP server definitions;
- plugin enablement intent;
- trusted project entries;
- notice and UI preference intent;
- session/history/archive data, but only if both binaries prove compatible with
  the same rollout/event schema.

## What Must Remain Target-Specific Unless Proven Otherwise

These are not safe to blindly share:

- effective `config.toml`, because RAWR has fork-only sections and feature flags;
- RAWR-only `[rawr_auto_compaction]` and related feature flags;
- `state_*.sqlite`, `logs_*.sqlite`, and their `-wal` / `-shm` sidecars;
- plugin cache directories and marketplace checkouts;
- `skills/.system` and other version-managed skill caches;
- `.tmp`, runtime caches, shell snapshots, logs, and other process-owned files;
- launchd/Desktop/Happy/app-server process state and target markers.

## Compatibility Gates For Any Future Work

Before considering one shared home or a split shared-data model, the switchboard
must fail closed unless it can prove:

- both binaries agree on state DB and logs DB versions;
- SQLite migrations, schemas, and WAL behavior are compatible;
- both binaries can parse and write the selected config view;
- upstream mode has no RAWR-only top-level config;
- RAWR mode receives RAWR-only config only through a target-specific overlay;
- session JSONL, `history.jsonl`, and `session_index.jsonl` are compatible;
- plugin cache and system skill behavior are namespaced or byte-compatible;
- no live Codex/Desktop/Happy/app-server process is writing the relevant home
  during a switch.

## Falsifier

If a lagging fork can delete, rewrite, or misinterpret upstream-owned state by
being pointed at the same home, the shared-home design is rejected.

The known hard risk is SQLite: state/log DB filenames are versioned, WAL-backed,
and process-owned. If upstream and the fork diverge on versions or migrations,
one binary may clean up or ignore files owned by the other.

## Next Time This Is Reopened

Start from a disposable cloned home. Do not test on live `~/.codex`.

The first useful experiment is not a migration. It is a compatibility probe:

1. clone the consolidated upstream home to a temporary directory;
2. run upstream and fork read-only probes against it;
3. compare feature parsing, config parsing, plugin/skill listing, SQLite schema,
   and session resume/read behavior;
4. record every mutation outside the selected overlay as a failure.

Until that passes, keep the current two-home switchboard model and treat this as
a deferred design question.
