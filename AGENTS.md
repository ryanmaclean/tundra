# AGENTS.md

## Role

Broad orchestration/telemetry testbed and pattern source.

## Owns

- demo/testbed implementations
- telemetry/provider/context reference patterns

## Do not duplicate

- becoming the lower-bound runtime

## Sibling repos to consult first

- ryanmaclean/bop
- ryanmaclean/smolfire
- ryanmaclean/moth

## Cross-project context

Read `docs/CROSS-PROJECT-LESSONS-2026-09.md` before making architectural changes.

## Agent delegation

- Primary GitHub coding agent: Copilot when assignable/available.
- Fallback: delegate the issue or PR to Codex with `@codex`.
- Do not treat Copilot/Codex state as canonical project state; keep canonical work in repo issues/BOP/filesystem state.
