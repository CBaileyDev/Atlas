/**
 * Hand-written IPC types. ts-rs generation is on the Phase 3+ roadmap.
 */

export interface PingResponse {
  pong: string;
  echoed: string | null;
  timestamp: string;
  version: string;
}

// ---- Dumps + ingest ------------------------------------------------------

export interface DumpListItem {
  id: number;
  game_id: string;
  game_version: string;
  parser: string;
  symbol_count: number;
  ingested_at: string;
}

export interface OpenDumpInfo {
  id: number;
  game_id: string;
  game_version: string;
  symbol_count: number;
  modules: string[];
}

export interface IngestReport {
  dump_id: number;
  game_id: string;
  game_version: string;
  parser: string;
  symbols_inserted: number;
  symbols_skipped: number;
  relations_inserted: number;
  relations_skipped: number;
  warnings: string[];
}

// ---- Search --------------------------------------------------------------

export interface SearchHit {
  id_hex: string;
  fqn: string;
  kind_i: number;
  module: string;
  score: number;
}

export interface SearchResult {
  query: string;
  total_matched: number;
  hits: SearchHit[];
}

export const SYMBOL_KIND_LABEL: Record<number, string> = {
  0: "module",
  1: "class",
  2: "struct",
  3: "enum",
  4: "enum value",
  5: "function",
  6: "field",
  7: "parameter",
};

// ---- Symbol detail -------------------------------------------------------

export interface SymbolRow {
  id: number[]; // bytes
  dump_id: number;
  fqn: string;
  name: string;
  kind: number;
  module: string;
  size: number | null;
  align: number | null;
  offset: number | null;
  vtable_slot: number | null;
  type_ref_json: string | null;
  flags: number;
  source_file: string | null;
  source_line: number | null;
}

/**
 * Mirrors `AppError` on the Rust side. Tauri serializes Result::Err with
 * the `kind` / `message` discriminated-union shape because that's what
 * #[serde(tag, content)] produces.
 */
export type AppError =
  | { kind: "Atlas"; message: AtlasError }
  | { kind: "Tauri"; message: string }
  | { kind: "Internal"; message: string };

export type AtlasError =
  | { kind: "Parser"; message: string }
  | { kind: "Storage"; message: string }
  | { kind: "Search"; message: string }
  | { kind: "Diff"; message: string }
  | { kind: "Export"; message: string }
  | { kind: "Io"; message: string }
  | { kind: "InvalidInput"; message: string }
  | { kind: "NotFound"; message: string };
