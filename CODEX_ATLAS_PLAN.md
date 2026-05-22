# Codex Atlas — Autonomous Build Plan (v1.0)

**Audience:** Claude Code (Opus 4.7) running unattended.
**Target:** Carter (solo dev, Windows 11 primary, macOS secondary).
**Read this entire document before writing any code.**

---

## 0. How to use this document

You are going to build **Codex Atlas** from an empty repository to a working Phase 3 product in a single long autonomous coding session. Follow these rules without exception:

1. **Read the whole plan before touching the keyboard.** Do not skim. The locked decisions in §2 exist so you do not waste time re-litigating them.
2. **Work phase by phase, in order.** Do not start Phase N+1 until Phase N's Acceptance section passes.
3. **Commit at every green test suite.** Use conventional commits (`feat:`, `fix:`, `test:`, `chore:`, `refactor:`, `docs:`).
4. **One branch per phase.** Branch name format: `phase-N-shortname` (e.g., `phase-1-ingest`). Merge to `main` only after the phase's Acceptance gate passes.
5. **Stop only at the conditions in §13.** Any other ambiguity → pick the option most consistent with the rest of the plan, write an ADR explaining why, keep going.
6. **Write an ADR for every non-trivial choice you make.** ADRs go in `docs/adr/NNNN-slug.md`. Use the MADR template (Context, Decision, Consequences).
7. **No TODOs in code.** If you can't finish something, either don't start it or open a tracked task in `TASKS.md` with a clear description.
8. **The user wants undergraduate-authentic writing in any user-facing strings:** plain vocabulary, no em-dashes in UI copy, no AI-tell phrases ("delve", "leverage", "robust"), no overly clever lines. Code comments can be normal.

---

## 1. Mission (one paragraph)

Codex Atlas is a single-user Tauri 2 desktop app that ingests game-SDK dumper output (Dumper-7 for Unreal Engine first; plugin-extensible to Unity IL2CPP later), stores each dump as a versioned semantic graph in SQLite, lets the user browse 200k+ symbols with sub-second fuzzy search and faceted filters, computes structural diffs between any two versions of the same game, and exports selected symbols as trainer scaffolds via Tera templates. Everything is local. No accounts, no network, no services.

---

## 2. Locked decisions (do not re-debate)

| Concern | Decision |
|---|---|
| Backend language | Rust (stable, edition 2021) |
| Desktop framework | Tauri 2 |
| Frontend framework | React 19 + TypeScript 5 (strict mode) |
| Build tool (FE) | Vite 6 |
| Styling | Tailwind CSS v4 |
| Animation | Framer Motion 12 (use sparingly — performance over polish) |
| Frontend state | Zustand 5 |
| UI primitives | Radix UI primitives + custom components. **Do not pull in shadcn CLI** — copy the few primitives you need by hand. |
| Storage | SQLite via `rusqlite` 0.31+ (bundled feature) with `refinery` for migrations |
| Search | `tantivy` 0.22+ |
| Templating | `tera` 1.x |
| Async | `tokio` (multi-thread runtime; Tauri requires this) |
| Errors | `thiserror` for library-style errors, `anyhow` only at the binary edge |
| Logging | `tracing` + `tracing-subscriber` (JSON to file, pretty to stderr in dev) |
| Serialization | `serde` + `serde_json` |
| Hashing | `blake3` for content hashes; `xxhash-rust` for structural fingerprints |
| Testing (Rust) | Built-in `#[test]` + `insta` for snapshots + `proptest` for parser invariants |
| Testing (TS) | `vitest` + `@testing-library/react` |
| Cargo layout | Workspace with separate crates for parsers (see §3) |
| Database location | `%APPDATA%\CodexAtlas\` on Windows, `~/Library/Application Support/CodexAtlas/` on macOS |
| Symbol identity | Per-dump rows. Symbols are never globally canonical. Cross-version links live in a separate table populated by the diff engine. |
| ID format | `BLAKE3(fully_qualified_name + kind + dump_id)[..16]` as hex |
| Diff engine signature | Pure function: `(base_graph, head_graph, config, overrides) -> Diff`. No DB access. |
| Tauri IPC | Commands return `Result<DTO, AtlasError>`. The frontend never touches SQLite. |
| Bundler target | Windows MSI (primary), DMG for macOS (secondary, smoke-test only) |

If a decision is missing here, default to the simplest option that fits the existing patterns. Document it in an ADR.

---

## 3. Architecture (the five boundaries)

```
┌─────────────────────────────────────────────────────────────────┐
│                     React 19 + Tauri 2 UI                       │
│  (Cmd-K palette, Browse, Diff, Export, Settings)                │
└──────────────────────────────┬──────────────────────────────────┘
                               │   Tauri IPC (typed DTOs only)
┌──────────────────────────────▼──────────────────────────────────┐
│                       atlas-core (lib crate)                    │
│  ┌──────────┐  ┌────────┐  ┌──────┐  ┌──────────┐  ┌─────────┐  │
│  │ Parsers  │→ │ Storage│←→│ Diff │  │  Search  │  │ Export  │  │
│  │ (trait)  │  │(SQLite)│  │(pure)│  │(Tantivy) │  │  (Tera) │  │
│  └──────────┘  └────────┘  └──────┘  └──────────┘  └─────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

**Crate layout (workspace):**

```
codex-atlas/
├── Cargo.toml                    # workspace
├── crates/
│   ├── atlas-core/               # types, storage, search, diff, export
│   ├── atlas-parser-trait/       # SdkParser trait + SymbolGraph types
│   ├── atlas-parser-ue/          # Dumper-7 parser
│   └── atlas-parser-unity/       # stub for Phase 5+, empty for now
├── src-tauri/                    # Tauri shell, IPC commands
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── src/
│       ├── main.rs
│       ├── commands/
│       └── error.rs
├── src/                          # React frontend
│   ├── routes/
│   ├── components/
│   ├── stores/
│   ├── ipc/                      # generated TS types + thin wrappers
│   └── styles/
├── fixtures/                     # synthetic + real dumper output
├── docs/
│   └── adr/
└── TASKS.md
```

**Five boundary invariants — never violate:**

1. **Parsers produce graphs and walk away.** A parser takes a path-on-disk and returns an in-memory `SymbolGraph`. It does not know SQLite exists. It does not know Tantivy exists. It does not log to a global logger — it takes a `&Reporter` if it needs to report progress.
2. **Storage owns identity.** Parsers produce `Symbol` values with no DB id. Storage assigns ids on insert. Cross-dump links never exist until the diff engine creates them.
3. **Diff is pure.** No I/O. No DB. Easily snapshot-tested with `insta`.
4. **Search indexes are derived.** The SQLite tables are the source of truth. The Tantivy index can be deleted and rebuilt at any time. Always provide a `rebuild_index` command.
5. **The UI never opens SQLite.** All FE reads go through `invoke()` returning typed DTOs.

---

## 4. Data model

### 4.1 SymbolGraph (in-memory, parser output)

```rust
// crates/atlas-parser-trait/src/lib.rs

pub struct SymbolGraph {
    pub source: SourceMeta,
    pub symbols: Vec<Symbol>,
    pub relations: Vec<Relation>,
}

pub struct SourceMeta {
    pub parser: String,           // "dumper7-ue"
    pub parser_version: String,
    pub game_id: String,          // user-supplied or auto from SDKInfo.json
    pub game_version: String,     // "32.10"
    pub ingested_at: DateTime<Utc>,
    pub sdk_root: PathBuf,
}

pub struct Symbol {
    pub local_id: u32,            // unique within this graph, NOT a DB id
    pub fqn: String,              // "FortniteGame.AFortPlayerController"
    pub name: String,             // "AFortPlayerController"
    pub kind: SymbolKind,
    pub module: String,           // "FortniteGame"
    pub size: Option<u32>,
    pub align: Option<u32>,
    pub offset: Option<u32>,      // for fields
    pub vtable_slot: Option<u32>, // for virtual functions
    pub type_ref: Option<TypeRef>,// for fields, return types, params
    pub flags: SymbolFlags,
    pub source_loc: Option<SourceLoc>,
}

pub enum SymbolKind {
    Class, Struct, Enum, EnumValue,
    Function, Field, Parameter, Module,
}

pub struct Relation {
    pub from: u32,                // local_id
    pub to: u32,                  // local_id, OR external via TypeRef::Unresolved
    pub kind: RelationKind,
}

pub enum RelationKind {
    Inherits, Contains, Returns, TakesParam, OfType, Overrides,
}

pub trait SdkParser: Send + Sync {
    fn name(&self) -> &str;
    fn can_handle(&self, root: &Path) -> bool;
    fn parse(&self, root: &Path, reporter: &dyn Reporter) -> Result<SymbolGraph, ParseError>;
}
```

### 4.2 SQLite schema (sketch — finalize in Phase 0)

```sql
-- dumps: one row per ingested SDK dump
CREATE TABLE dumps (
    id              INTEGER PRIMARY KEY,
    game_id         TEXT NOT NULL,
    game_version    TEXT NOT NULL,
    parser          TEXT NOT NULL,
    parser_version  TEXT NOT NULL,
    sdk_root        TEXT NOT NULL,
    ingested_at     TEXT NOT NULL,
    symbol_count    INTEGER NOT NULL,
    UNIQUE(game_id, game_version, parser)
);

-- symbols: one row per symbol per dump. IDs are dump-scoped.
CREATE TABLE symbols (
    id              BLOB PRIMARY KEY,       -- 16-byte hash
    dump_id         INTEGER NOT NULL REFERENCES dumps(id) ON DELETE CASCADE,
    fqn             TEXT NOT NULL,
    name            TEXT NOT NULL,
    kind            INTEGER NOT NULL,
    module          TEXT NOT NULL,
    size            INTEGER,
    align           INTEGER,
    offset          INTEGER,
    vtable_slot     INTEGER,
    type_ref_json   TEXT,
    flags           INTEGER NOT NULL DEFAULT 0,
    source_file     TEXT,
    source_line     INTEGER
);
CREATE INDEX symbols_dump_kind   ON symbols(dump_id, kind);
CREATE INDEX symbols_dump_module ON symbols(dump_id, module);
CREATE INDEX symbols_dump_fqn    ON symbols(dump_id, fqn);

-- relations: edges. Both ends are symbol ids (no dangling).
CREATE TABLE relations (
    from_symbol  BLOB NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
    to_symbol    BLOB NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
    kind         INTEGER NOT NULL,
    PRIMARY KEY (from_symbol, to_symbol, kind)
);
CREATE INDEX relations_to ON relations(to_symbol);

-- cross-dump links: populated by diff engine
CREATE TABLE symbol_links (
    base_symbol  BLOB NOT NULL,
    head_symbol  BLOB NOT NULL,
    confidence   REAL NOT NULL,
    method       TEXT NOT NULL,           -- "exact", "fingerprint", "user"
    confirmed_by TEXT,                    -- "user" if Carter confirmed
    confirmed_at TEXT,
    PRIMARY KEY (base_symbol, head_symbol)
);

-- user overrides survive re-running the diff
CREATE TABLE rename_overrides (
    game_id       TEXT NOT NULL,
    base_version  TEXT NOT NULL,
    base_fqn      TEXT NOT NULL,
    head_version  TEXT NOT NULL,
    head_fqn      TEXT NOT NULL,
    decision      TEXT NOT NULL,          -- "match", "reject"
    created_at    TEXT NOT NULL,
    PRIMARY KEY (game_id, base_version, base_fqn, head_version, head_fqn)
);

-- export projects
CREATE TABLE projects (
    id            INTEGER PRIMARY KEY,
    name          TEXT NOT NULL UNIQUE,
    dump_id       INTEGER NOT NULL REFERENCES dumps(id),
    template_name TEXT NOT NULL,
    selection_json TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);
```

### 4.3 DTOs (TypeScript, returned by IPC)

Generated by `ts-rs` or hand-rolled — pick `ts-rs` (less drift). Every Rust struct exposed across the IPC boundary derives `TS`.

---

## 5. Project structure (canonical)

Create exactly this layout in Phase 0. Resist the urge to "improve" it.

```
codex-atlas/
├── .github/workflows/ci.yml
├── .gitignore
├── README.md
├── TASKS.md
├── Cargo.toml                    # workspace root
├── rust-toolchain.toml           # pin stable
├── package.json
├── pnpm-lock.yaml                # use pnpm
├── vite.config.ts
├── tsconfig.json
├── tailwind.config.ts
├── index.html
├── crates/
│   ├── atlas-core/
│   ├── atlas-parser-trait/
│   ├── atlas-parser-ue/
│   └── atlas-parser-unity/       # empty stub crate
├── src-tauri/
│   ├── Cargo.toml
│   ├── build.rs
│   ├── tauri.conf.json
│   ├── icons/
│   └── src/
│       ├── main.rs
│       ├── error.rs
│       └── commands/
│           ├── mod.rs
│           ├── dumps.rs
│           ├── symbols.rs
│           ├── search.rs
│           ├── diff.rs
│           └── export.rs
├── src/
│   ├── main.tsx
│   ├── App.tsx
│   ├── routes/
│   │   ├── browse.tsx
│   │   ├── diff.tsx
│   │   ├── export.tsx
│   │   └── settings.tsx
│   ├── components/
│   │   ├── command-palette/
│   │   ├── symbol-table/
│   │   ├── symbol-detail/
│   │   ├── diff-view/
│   │   └── ui/                   # button, input, dialog primitives
│   ├── stores/
│   │   ├── dumps.ts
│   │   ├── selection.ts
│   │   └── settings.ts
│   ├── ipc/
│   │   ├── client.ts             # typed invoke wrappers
│   │   └── types.ts              # generated by ts-rs
│   └── styles/
│       └── globals.css
├── fixtures/
│   ├── synthetic/                # generated, checked in
│   │   ├── tiny-game-v1/
│   │   └── tiny-game-v2/
│   └── real/                     # gitignored, user-supplied
├── docs/
│   ├── adr/
│   │   └── 0001-record-architecture-decisions.md
│   └── parser-format-notes.md
└── scripts/
    ├── gen-synthetic-fixtures.rs # or .py
    └── bench-ingest.rs
```

---

## 6. Phase 0 — Foundation

**Goal:** Working empty shell. App launches, shows an empty Browse screen, can run `cargo test` and `pnpm test` green with one trivial test in each.

### Deliverables

- Workspace `Cargo.toml`, all four crates compile (even if empty).
- `src-tauri` Tauri 2 shell with one IPC command `ping() -> "pong"`.
- React app renders, calls `ping`, displays the response.
- SQLite migration 0001 creates the schema in §4.2.
- `tracing` initialized, logs go to `%APPDATA%\CodexAtlas\logs\atlas.log` (rotated daily).
- `AtlasError` enum with variants for the five categories (Parser, Storage, Search, Diff, Export).
- One trivial Rust test (`assert_eq!(2+2, 4)` in `atlas-core`), one trivial Vitest test.
- GitHub Actions CI: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `pnpm typecheck`, `pnpm test`, `pnpm build`.
- `README.md` with: what this is, how to run locally, how to run tests.
- ADR 0001 (architecture decision record format), ADR 0002 (locked stack from §2), ADR 0003 (per-dump symbol identity).

### Acceptance gate (must all pass)

```
□ cargo build --workspace                          (no warnings on stable)
□ cargo clippy --workspace -- -D warnings
□ cargo test --workspace
□ pnpm typecheck
□ pnpm test
□ pnpm tauri dev                                   (launches, shows "pong")
□ pnpm tauri build                                 (produces an MSI on Windows)
□ All ADRs present
□ CI workflow runs green on a PR to main
```

### Risks / decision points

- **Tauri 2 stable vs RC:** use the latest stable; if a critical bug forces RC, document it in an ADR.
- **rusqlite vs sqlx:** locked to rusqlite. Synchronous is fine — wrap DB calls in `tokio::task::spawn_blocking` at the IPC boundary.
- **Tauri IPC error model:** every command returns `Result<T, AtlasError>` where `AtlasError: Serialize`. Implement `Display` so the frontend can show it raw if needed.

---

## 7. Phase 1 — Ingest (Dumper-7 → SymbolGraph → SQLite)

**Goal:** Point Atlas at a Dumper-7 output folder, get a complete `SymbolGraph` in SQLite, in under 30 seconds for a ~200k-symbol Fortnite-scale dump.

### Deliverables

- `atlas-parser-trait` crate with `SdkParser` trait and all types from §4.1.
- `atlas-parser-ue` crate that parses Dumper-7 output:
  - Reads `SDK.hpp` and the per-package `*.hpp` files under the SDK root.
  - Tokenizes class/struct/enum/function definitions with a hand-written lexer + recursive-descent parser. **Do not use a general C++ parser** (clang-sys, tree-sitter-cpp). Dumper-7 output is predictable; a 1500-line bespoke parser is faster and more maintainable.
  - Handles: classes with parents, fields with offsets and sizes, virtual functions with vtable slots, enums with values, function signatures, comment-driven offset annotations.
  - Tolerant of minor format drift — log warnings, skip the offending symbol, continue.
- `atlas-core::ingest` module that takes a `SymbolGraph` and writes it to SQLite in a single transaction.
- Tauri command `ingest_dump(path: String) -> IngestReport` returning counts + warnings.
- `Reporter` trait for progress updates over IPC (emit `tauri::Event` every 1000 symbols).
- Synthetic fixture generator: a script that writes a tiny fake Dumper-7-style output with ~50 symbols for fast tests.
- Property tests: round-trip random SymbolGraphs through SQLite, assert equality.
- Bench: `cargo bench` target that ingests the synthetic fixture and asserts < 100ms.

### Implementation notes

- The Dumper-7 format has a few quirks. **Do not invent these — verify against the user's real fixtures before locking the parser.** See §13 STOP condition #1.
- Use `memmap2` if file I/O becomes a bottleneck, but try `BufReader` first.
- Insert symbols in batches of 1000 with prepared statements inside a single transaction. PRAGMA `journal_mode=WAL`, `synchronous=NORMAL` after migrations.
- The 16-byte BLOB primary key beats TEXT for both space and join speed at this scale.

### Acceptance gate

```
□ Parses the synthetic fixture cleanly, 0 warnings
□ Round-trip property test green (100 cases minimum)
□ ingest_dump command works end-to-end from the frontend
□ Progress events fire and the FE displays them
□ Ingestion of the 200k-symbol real fixture (when supplied) completes in < 30s on the user's machine
□ Re-ingesting the same dump is idempotent (no duplicate rows, no errors)
□ Warnings/errors during ingest surface in IngestReport, not silently swallowed
```

### Risks / decision points

- **Real fixture access:** see §13 STOP #1.
- **Memory:** 200k symbols × ~500 bytes per Symbol = ~100MB. Acceptable. Do not over-engineer streaming. If memory becomes a problem later, redesign.
- **Module detection:** Dumper-7 emits one file per package; the filename minus extension is the module name. Confirm with a real fixture.

---

## 8. Phase 2 — Browse + Search

**Goal:** Replace VS Code grep for one real workflow. User opens a dump, types three letters in Cmd-K, sees the symbol, clicks, sees full surface with hyperlinked members.

### Deliverables

- Tantivy index built per dump. Schema: `id` (stored), `fqn` (text + fast), `name` (text), `kind` (facet), `module` (facet), `parent_name` (text), `member_names` (text, for "find class containing field X").
- `atlas-core::search` module exposing `query(dump_id, query, facets, limit) -> SearchResult`.
- Index lifecycle: rebuild on demand, invalidate on dump re-ingest, store under `%APPDATA%\CodexAtlas\index\<dump_id>\`.
- IPC commands: `list_dumps`, `open_dump`, `search`, `get_symbol`, `list_members`.
- React Browse route:
  - Left panel: dump selector + faceted filters (kind, module).
  - Center: virtualized table of search hits (use `@tanstack/react-virtual`). Sub-50ms keystroke-to-result for queries hitting <1000 rows.
  - Right: symbol detail panel — parents, members, offset, size, vtable slot, members as a sortable table with type hyperlinks back into the tree.
- Cmd-K command palette: global, modal, fuzzy across symbol names. Use `cmdk` library.
- Keyboard navigation throughout (↑/↓ in lists, Enter to drill, Esc to back out).

### Acceptance gate

```
□ Cmd-K returns results in < 100ms for the 200k-symbol fixture
□ Faceted filters are stackable and update result count in real time
□ Symbol detail loads in < 50ms after click
□ Hyperlink in a member field type navigates to that type's symbol
□ Reload window → state restored (last open dump, last query, last symbol)
□ Bench: index build for 200k symbols < 10s
```

### Risks / decision points

- **Tantivy version churn:** pin to a specific version, do not chase main.
- **Index location collision with previous schemas:** include a `SCHEMA_VERSION` constant; on mismatch, delete and rebuild.
- **Fuzzy vs prefix:** Tantivy's fuzzy is good enough; do not add a separate trigram index unless benches force it.

---

## 9. Phase 3 — Diff engine

**Goal:** Given two dumps of the same `game_id`, produce a structured `Diff` that highlights the changes Carter actually needs to know about as a trainer author.

### Deliverables

- `atlas-core::diff` module. Pure function: `diff(base: &SymbolGraph, head: &SymbolGraph, config: &DiffConfig, overrides: &[RenameOverride]) -> Diff`.
- Three passes, in order:
  1. **Pass 1 — Exact match by FQN.** Linear scan, hash map keyed on `(kind, fqn)`. Catches 90%+.
  2. **Pass 2 — Fingerprint rename detection.** For each unmatched class/struct in base, compute a fingerprint:
     - parent class FQN
     - module
     - sorted set of member names
     - sorted set of member types
     Compare against unmatched head symbols of the same kind. Score = Jaccard(member_names) × 0.6 + Jaccard(member_types) × 0.3 + (same_module ? 0.1 : 0). Threshold for "suggestion" = 0.7; for "high confidence" = 0.9.
  3. **Pass 3 — Field-level classification on matched pairs.** For each matched pair, compute:
     - offset_changed (per field, with delta)
     - size_changed
     - vtable_shift
     - parent_class_changed
     - fields_added / fields_removed
     - function_signature_changed
     - field_type_substituted (e.g., FString → FText)
- `Diff` struct serializable to JSON, stored in `diffs/<base>-<head>.json` for caching.
- Snapshot tests with `insta`: a curated set of synthetic before/after pairs covering each change category.
- React Diff route:
  - Selector for base and head dumps.
  - Filter chips: `Added`, `Removed`, `Modified`, `Renamed`, `Suggested`. Default = all-on except `Unchanged`.
  - Tree view, virtualized. Gutter glyphs: `+ − ~ ⇄ ?`
  - Click → side-by-side detail panel with field-level highlighting (diff-match-patch for member-list rendering).
  - For suggested renames: inline `Confirm` / `Reject` buttons. Confirmed overrides persist to `rename_overrides` and a re-diff respects them.

### Acceptance gate

```
□ Snapshot tests cover all 7 change categories, all green
□ Diff of two 200k-symbol dumps completes in < 10s
□ Confirming a suggested rename and re-running the diff promotes it to a confirmed match
□ Rejecting a suggestion and re-running keeps it out of suggestions
□ JSON diff output round-trips (deserialize → serialize → byte-equal)
□ The "what broke between Fortnite 32.10 and 33.00" question (when real fixtures are supplied) is answerable in under 5 minutes of clicking
```

### Risks / decision points

- **The 0.7 / 0.9 thresholds are guesses.** Make them configurable in `DiffConfig`. Tune against real fixtures.
- **Performance on Pass 2:** O(n²) on the unmatched set is fine for unmatched counts < 5000. If real data has more, bucket by module first.
- **Anonymous/auto-generated names:** Dumper-7 sometimes emits `UnknownData_XX` padding fields. Filter these from fingerprint computation.

---

## 10. Phase 4 — Export

**Goal:** Carter selects a set of symbols, picks a Tera template, sees a live preview, clicks one button, gets a compilable scaffold on disk with an `_atlas.json` sidecar.

### Deliverables

- `atlas-core::export` module:
  - `Project` type (name, dump_id, selection, template_name).
  - Selection = list of symbol ids + transitive closure rules (e.g., "include parents", "include referenced types up to depth N").
  - Tera context builder: hands the template a list of symbol view-models with all the fields a template author needs.
- Tera templates shipped in-binary, copyable to `%APPDATA%\CodexAtlas\templates\` for user editing:
  - `Trainer.cs.tera` — C# trainer scaffold matching Carter's existing trainer layout. **Open `inventory/2HighInternal/` or similar before designing this template — match the user's existing style.**
  - `Offsets.h.tera` — flat C++ header of `static constexpr` offsets.
  - `Snippets/` — small templates for "Copy as C# struct", "Copy as IDA mapping", "Copy as sigscan", "Copy as Cheat Engine pointer chain".
- `_atlas.json` sidecar schema:
  ```json
  {
    "atlas_version": "...",
    "exported_at": "...",
    "game_id": "...",
    "game_version": "...",
    "dump_id": 7,
    "template": "Trainer.cs.tera",
    "template_version": "blake3:...",
    "symbols": ["<16-byte hex>", "..."],
    "selection_rules": { "include_parents": true, "type_depth": 1 }
  }
  ```
- React Export route:
  - Selection builder (drag from search results, or "Add current symbol", or paste FQN list).
  - Template picker dropdown.
  - Live preview pane: re-renders on every selection change, debounced 200ms.
  - "Write to disk" button → file dialog → writes template output + `_atlas.json`.
- Right-click context menu in Browse → "Copy as…" submenu for the snippet templates.

### Acceptance gate

```
□ Export of a 10-symbol selection produces a syntactically valid C# file (compile-check via `dotnet build` in a tiny scaffold project, run in CI optional)
□ Live preview latency < 250ms on a 100-symbol selection
□ "Copy as IDA mapping" produces output identical to the user's existing manual format (verify by diffing against an example the user supplies — see §13 STOP #2)
□ _atlas.json round-trips
□ User-edited template in %APPDATA% overrides the bundled one
```

### Risks / decision points

- **Template style is Carter-specific.** STOP and ask for one real trainer's `.cs` file before designing `Trainer.cs.tera`. Do not invent a style.
- **Tera vs Handlebars:** locked Tera. Period.
- **Snippet templates as Tera or as Rust format strings?** Tera, for consistency. Performance is not a concern for snippets.

---

## 11. Phase 5 — Polish + folder watcher (optional in autonomous run)

**Goal:** Quality-of-life features. Do these only if the autonomous session still has time after Phase 4 lands cleanly.

### Deliverables

- Folder watcher using `notify` crate. Watches a user-configured list of "dumper output" roots. On stable change (debounced 5s, no further writes), prompt the user via tray notification: "New Fortnite dump detected. Ingest?"
- Settings page: paths, watcher roots, template directory, theme (light/dark), font.
- Dark mode (default) + light mode. Use Tailwind v4 dark variant.
- Export the current diff as Markdown or PDF (PDF via the docx skill or a simple HTML-to-PDF route).
- Unity parser crate stub — confirm the trait is sufficient by writing a 1-symbol fake Unity parser and verifying it ingests cleanly.

### Acceptance gate

```
□ Watcher triggers within 10s of a new SDK folder appearing
□ Theme switch is instant, no flash, no relayout
□ Stub Unity parser ingests through the same code path as the UE parser
```

---

## 12. Cross-cutting concerns

### 12.1 Testing strategy

- **Unit tests:** in-module `#[cfg(test)]`. Required for every public function in `atlas-core::diff` and every parser sub-routine.
- **Property tests:** `proptest` for the parser (round-trip), the diff engine (idempotence of empty diffs, symmetry of swap), and storage (CRUD invariants).
- **Snapshot tests:** `insta` for diff output, template rendering, and SQL migration output.
- **Integration tests:** under `tests/` directory in `atlas-core` — exercise the full ingest → search → diff → export pipeline against the synthetic fixture.
- **Frontend tests:** Vitest for stores and IPC wrappers. React Testing Library for components. Do not test Tauri commands from the frontend — too flaky.
- **CI:** every push must pass everything in §6's acceptance gate plus all new tests.

### 12.2 Performance budgets (hard limits)

| Operation | Budget | Fixture |
|---|---|---|
| Cold start to interactive UI | < 1.5s | release build |
| Ingest a 200k-symbol dump | < 30s | real Fortnite-scale |
| Index a 200k-symbol dump | < 10s | same |
| Cmd-K keystroke to first result | < 100ms | same |
| Symbol detail open | < 50ms | same |
| Diff two 200k-symbol dumps | < 10s | two real dumps |
| Export 100-symbol selection (preview) | < 250ms | same |

Bench every one of these. If a budget breaks, fix or document why before merging.

### 12.3 Logging

- `tracing::info!` for lifecycle events (app start, dump ingested, diff complete).
- `tracing::warn!` for skipped/tolerated parser issues, missing optional data.
- `tracing::error!` only for things that surface to the user.
- `tracing::debug!` for everything else, off by default.
- Log file: `%APPDATA%\CodexAtlas\logs\atlas.log`, rotated daily, keep 7 days.

### 12.4 Error model

```rust
#[derive(thiserror::Error, Debug, serde::Serialize)]
pub enum AtlasError {
    #[error("parser: {0}")]
    Parser(String),
    #[error("storage: {0}")]
    Storage(String),
    #[error("search: {0}")]
    Search(String),
    #[error("diff: {0}")]
    Diff(String),
    #[error("export: {0}")]
    Export(String),
    #[error("io: {0}")]
    Io(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
}
```

Use `#[from]` sparingly. Prefer explicit `.map_err(|e| AtlasError::Storage(e.to_string()))` to keep error messages curated.

### 12.5 Git workflow

- One branch per phase. Format `phase-N-shortname`.
- Conventional commits. Squash-merge to `main`.
- Tag a release after each phase: `v0.<N>.0`.
- Every phase ends with a `CHANGELOG.md` entry written by you, not auto-generated.

---

## 13. 🛑 Stop and ask Carter

You **must** stop and surface a clear question to Carter (in the terminal, with a prefix `🛑 STOP — INPUT NEEDED:`) in exactly these situations. No others.

1. **Phase 1, before locking the Dumper-7 parser.** Synthetic fixtures will get you 80% there. Before claiming Phase 1 complete, you need at least one real Dumper-7 output directory from Carter to validate format assumptions. Ask: *"Please drop a Dumper-7 SDK folder (or a zip of one) at `fixtures/real/<game>-<version>/`. I need it to verify the parser's format assumptions before locking Phase 1."*

2. **Phase 4, before designing `Trainer.cs.tera`.** Ask: *"Please drop one of your existing trainer projects (e.g. `2HighInternal/`) into `fixtures/real/trainer-reference/`. I'll use its style as the template target. Without this, I'll fall back to a generic C# scaffold that probably won't match your conventions."*

3. **Any phase, when a locked decision in §2 demonstrably blocks progress.** Document the blocker. Propose two alternatives. Stop.

4. **CI is failing on a transient external issue you cannot fix from inside the repo** (e.g., GitHub Actions outage, a dependency yanked from crates.io). Document, stop.

5. **You hit a performance budget in §12.2 and have already spent more than 2 hours trying to meet it.** Surface the situation, propose a relaxation or a redesign.

**Do not stop for:** missing icons (use placeholders), ambiguous UI copy (use plain English, no em-dashes, no AI-tells), unclear test targets (pick a reasonable one and document), styling questions (defer to existing patterns).

---

## 14. Appendix

### 14.1 Synthetic fixture generator

Before Phase 1's parser work, write `scripts/gen-synthetic-fixtures.rs` (or `.py`) that emits a tiny fake Dumper-7-style output with:

- 1 module (`TinyGame`)
- 5 classes with inheritance chains
- 1 enum, 4 values
- ~50 total symbols
- Two versions (`v1` and `v2`) where v2 has: 1 added class, 1 removed field, 1 offset shift, 1 renamed class, 1 type substitution

Use this fixture for fast unit tests and as the example for snapshot tests of the diff engine.

### 14.2 Commands cheat sheet

```bash
# Dev
pnpm tauri dev

# Tests
cargo test --workspace
pnpm test

# Lint
cargo clippy --workspace -- -D warnings
cargo fmt --check
pnpm typecheck

# Build
pnpm tauri build

# Bench
cargo bench -p atlas-core

# Regenerate TS types from Rust
cargo test export_bindings   # ts-rs convention
```

### 14.3 Initial Claude Code kickoff prompt (paste this to start)

```
You are implementing Codex Atlas. Read CODEX_ATLAS_PLAN.md in full
before writing anything. Then execute Phase 0 end-to-end.

Rules:
- Follow the plan as written. Do not re-debate locked decisions.
- Commit on every green test suite, conventional commits.
- One branch per phase. Open a PR to main when the phase's
  Acceptance gate passes.
- Write an ADR for every non-trivial choice.
- Stop only at the conditions in section 13. Do not stop for
  anything else; pick the option most consistent with the plan
  and continue.
- When Phase 0 lands, post a summary and start Phase 1
  automatically.

Begin now.
```

### 14.4 If the autonomous run dies partway

- The git history is your recovery log.
- `TASKS.md` should record any deferred work.
- ADRs explain why anything looks unusual.
- Resume by reading the latest ADR, the last 10 commits, and `TASKS.md`, then continuing from the current phase.

---

**End of plan. Version 1.0.**
