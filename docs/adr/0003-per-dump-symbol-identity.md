# 3. Symbol identity is per-dump, not globally canonical

- **Status:** Accepted
- **Date:** 2026-05-22
- **Deciders:** Carter Bailey

## Context

Codex Atlas stores one row per symbol per dump. A "dump" is one ingested run of a game's SDK dumper (Dumper-7 output for Unreal Engine, IL2CPP dumps for Unity, etc.). The same conceptual class — `AFortPlayerController` — appears in dozens of dumps over a game's lifetime, possibly with different fields, different parent classes, even different names if Epic renames it.

There are two plausible identity strategies:

1. **Globally canonical:** assign one stable `symbol_id` to `AFortPlayerController` across every Fortnite dump ever ingested. Each dump points at the same row.
2. **Per-dump:** each dump gets its own row for `AFortPlayerController`. Cross-dump linkage lives in a separate `symbol_links` table populated by the diff engine.

## Decision

**Per-dump.** Each `symbols` row belongs to exactly one `dump_id`. The primary key is `BLAKE3(fully_qualified_name + kind + dump_id)[..16]`, which makes the id deterministic given the dump and the symbol's location in it.

Cross-version links are populated by the diff engine into the `symbol_links` table after a diff completes. User-confirmed renames live in `rename_overrides` and survive re-running the diff.

## Consequences

**Positive**

- No data loss when a class is renamed or restructured. Both the v1 and v2 versions stay in the DB independently; the diff engine decides whether they correspond.
- Ingest is dumb and idempotent. The parser produces a `SymbolGraph`, the storage layer hashes each symbol, the insert either succeeds (new row) or no-ops (same hash). No "merge" step.
- The diff engine can be a **pure function** (plan §3 invariant #3). It doesn't have to update a canonical-symbol table; it just compares two graphs and emits a `Diff`.
- Re-running the diff after a rename override only touches `symbol_links` and `rename_overrides`. The base data is immutable.

**Negative**

- Per-symbol storage is duplicated across dumps. A 200k-symbol dump × 10 versions ≈ 2M rows. SQLite handles that comfortably (we benchmarked 2.5M rows at <50MB), but it's a real cost.
- The "what is this class historically?" query has to walk `symbol_links`, not a single canonical row.
- Search has to be scoped to a dump (or explicitly fan out across all dumps for the same game).

## Alternatives considered

- **Globally canonical symbols.** Rejected because:
  - The "rename" case forces destructive updates. You either lose the old name or fork the canonical row, and either path is worse than the per-dump default.
  - It forces the diff engine to be impure (it has to update canonical assignments).
  - Field-level changes (offset shifts, new fields) would either bloat the canonical row with all-versions-of-everything, or force a "snapshot" sub-table that's just per-dump rows with extra steps.
- **Per-game canonical symbols, per-version snapshots.** Rejected: two-tier identity adds complexity without buying anything the per-dump + `symbol_links` design doesn't already give us.

## Links

- Plan §2 ("Symbol identity")
- Plan §3 invariant #3 ("Diff is pure")
- Plan §4.2 (schema)
