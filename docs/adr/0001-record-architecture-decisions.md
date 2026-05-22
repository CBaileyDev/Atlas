# 1. Record architecture decisions

- **Status:** Accepted
- **Date:** 2026-05-22
- **Deciders:** Carter Bailey

## Context

Codex Atlas is being built end-to-end by an autonomous Claude Code session. The plan in `CODEX_ATLAS_PLAN.md` locks the big-picture choices; everything else is decided in flight. Without a written record, those in-flight choices vanish into the git history and become impossible to challenge later without re-doing the analysis.

The project will go through multiple phases, and decisions made in Phase 0 may be reconsidered in Phase 3 or Phase 5. We need a format that:

- captures the decision and the alternatives that were rejected,
- captures the reasoning so the trade-offs survive,
- stays out of the way (no tooling, no portal, no review process — markdown files in the repo),
- composes cleanly with code review (each ADR is one file in a PR).

## Decision

Adopt **MADR** (Markdown Architectural Decision Records). Each ADR lives at `docs/adr/NNNN-slug.md` and follows this template:

```markdown
# N. Title in imperative

- Status: Proposed | Accepted | Deprecated | Superseded by NNNN
- Date: YYYY-MM-DD
- Deciders: who

## Context
Why we needed to decide. What forces were in play.

## Decision
What we chose, in present tense.

## Consequences
What follows. Include the downsides honestly.

## Alternatives considered
Other options and why each was rejected.
```

ADRs are numbered sequentially. Superseded ADRs are kept in place with a `Superseded by NNNN` status header rather than deleted.

## Consequences

**Positive**

- Future contributors (including future-Claude) can read the trail and understand *why*, not just *what*.
- Each decision has an explicit reversibility cost: if the consequences section is honest, reversing the decision is a known quantity.
- Pull requests that change architecture are easy to spot — they touch `docs/adr/`.

**Negative**

- One more thing to write. The discipline only pays off if ADRs actually get written; a stale `docs/adr/` is worse than no ADRs because it implies false rigor.

## Alternatives considered

- **No ADRs, rely on commit messages.** Rejected: commits are change-shaped, not decision-shaped, and decisions span many commits.
- **A single `DESIGN.md` document.** Rejected: it would either grow unbounded or lose old decisions; granular files survive renames and refactors better.
- **An external tool (ADR-tools, Confluence).** Rejected: extra tooling for a one-person project, and we explicitly want decisions reviewable in the same PR as the code.
