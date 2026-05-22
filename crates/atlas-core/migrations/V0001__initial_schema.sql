-- Codex Atlas — initial schema (plan §4.2).
--
-- Notes:
--   * Symbol identity is per-dump (plan §2 + ADR 0003). The `id` column on
--     symbols is a 16-byte BLAKE3-derived BLOB, computed by the application
--     layer, not the DB.
--   * `relations` has a deferred (composite) primary key. We rely on the
--     application layer never inserting symbols that exist in `relations`
--     before they exist in `symbols`. The FK enforces correctness.
--   * `symbol_links` and `rename_overrides` are populated by the diff
--     engine in Phase 3. Their schemas live here so migrations stay linear.

CREATE TABLE dumps (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id         TEXT    NOT NULL,
    game_version    TEXT    NOT NULL,
    parser          TEXT    NOT NULL,
    parser_version  TEXT    NOT NULL,
    sdk_root        TEXT    NOT NULL,
    ingested_at     TEXT    NOT NULL,
    symbol_count    INTEGER NOT NULL,
    UNIQUE(game_id, game_version, parser)
);

CREATE TABLE symbols (
    id              BLOB    PRIMARY KEY,
    dump_id         INTEGER NOT NULL REFERENCES dumps(id) ON DELETE CASCADE,
    fqn             TEXT    NOT NULL,
    name            TEXT    NOT NULL,
    kind            INTEGER NOT NULL,
    module          TEXT    NOT NULL,
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

CREATE TABLE relations (
    from_symbol  BLOB    NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
    to_symbol    BLOB    NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
    kind         INTEGER NOT NULL,
    PRIMARY KEY (from_symbol, to_symbol, kind)
);

CREATE INDEX relations_to ON relations(to_symbol);

CREATE TABLE symbol_links (
    base_symbol  BLOB    NOT NULL,
    head_symbol  BLOB    NOT NULL,
    confidence   REAL    NOT NULL,
    method       TEXT    NOT NULL,
    confirmed_by TEXT,
    confirmed_at TEXT,
    PRIMARY KEY (base_symbol, head_symbol)
);

CREATE TABLE rename_overrides (
    game_id       TEXT NOT NULL,
    base_version  TEXT NOT NULL,
    base_fqn      TEXT NOT NULL,
    head_version  TEXT NOT NULL,
    head_fqn      TEXT NOT NULL,
    decision      TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    PRIMARY KEY (game_id, base_version, base_fqn, head_version, head_fqn)
);

CREATE TABLE projects (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    name           TEXT    NOT NULL UNIQUE,
    dump_id        INTEGER NOT NULL REFERENCES dumps(id),
    template_name  TEXT    NOT NULL,
    selection_json TEXT    NOT NULL,
    created_at     TEXT    NOT NULL,
    updated_at     TEXT    NOT NULL
);
