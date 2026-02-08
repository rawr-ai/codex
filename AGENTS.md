# rawr-ai/codex Router

## Scope
- Applies to this repo root (`./**`) unless a deeper `AGENTS.md` exists in the working directory subtree.

## Operating Invariants (Non-Negotiable)
- MUST use Graphite (`gt`) for all work in this repo.
- MUST start any new stack from `codex/integration-upstream-main` via `gt create`.
  - Exception: only stack on an existing PR branch when the change is tightly coupled and intended to merge together.
- NEVER start unrelated work on a branch that already has an unrelated PR attached.
- MUST keep the repo clean (no dirty worktree) when finishing a task/turn.

## Process Rules
- Entry (always):
  - `git status --porcelain`
  - `gt branch info --no-interactive`
- Execution:
  - Follow pointer-first routing; do not invent workflows when scripts/runbooks exist.
- Exit (always):
  - Ensure clean worktree.
  - Use `gt modify` / `gt submit` for commits and PR updates.

## Routing
- Human-friendly workflows and setup pathways: `docs/agents.md` (root `agents.md` collides with `AGENTS.md` on case-insensitive filesystems)
- rawr fork workflows and runbooks: `rawr/AGENTS.md`
- Rust/Codex workspace rules (clippy/tests/docs/schema): `codex-rs/AGENTS.md`
- AGENTS mechanics/precedence reference: `docs/agents_md.md`

## Ownership
- Scope owner: rawr maintainers
- Update cadence: when workflows or safety invariants change (avoid temporal status text)
