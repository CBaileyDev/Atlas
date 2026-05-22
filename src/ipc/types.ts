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

// ---- Diff ---------------------------------------------------------------

export interface SymbolRef {
  fqn: string;
  kind: number;
  module: string;
}

export type MatchMethod = "exact" | "fingerprint" | "user_override";

export interface SymbolMatch {
  base: SymbolRef;
  head: SymbolRef;
  method: MatchMethod;
  score: number;
}

export interface RenameSuggestion {
  base: SymbolRef;
  head: SymbolRef;
  score: number;
}

export type ChangeKind =
  | {
      kind: "offset_changed";
      field: string;
      old_offset: number;
      new_offset: number;
    }
  | { kind: "size_changed"; field: string; old_size: number; new_size: number }
  | {
      kind: "vtable_shift";
      fn_name: string;
      old_slot: number;
      new_slot: number;
    }
  | {
      kind: "parent_class_changed";
      old_parent: string | null;
      new_parent: string | null;
    }
  | { kind: "field_added"; field: string }
  | { kind: "field_removed"; field: string }
  | {
      kind: "function_signature_changed";
      fn_name: string;
      old_return: string | null;
      new_return: string | null;
    }
  | {
      kind: "field_type_substituted";
      field: string;
      old_type: string;
      new_type: string;
    };

export interface FieldChange {
  parent_fqn: string;
  change: ChangeKind;
}

export interface Diff {
  game_id: string;
  base_version: string;
  head_version: string;
  matches: SymbolMatch[];
  added: SymbolRef[];
  removed: SymbolRef[];
  renamed_suggestions: RenameSuggestion[];
  field_changes: FieldChange[];
}

// ---- Settings + watcher --------------------------------------------------

export interface AtlasSettings {
  watcher_roots: string[];
  watcher_debounce_ms: number;
}

export interface DumpDetectedEvent {
  path: string;
  watched_root: string;
}

// ---- Export -------------------------------------------------------------

export interface TemplateInfo {
  name: string;
  description: string;
  default_filename: string;
  overridden: boolean;
}

export interface ResolvedSymbol {
  fqn: string;
  id_hex: string | null;
  kind_i: number | null;
}

export interface ExportRequest {
  dump_id: number;
  symbol_ids_hex: string[];
  template_name: string;
  project_name: string;
  trainer_class_name: string;
  process_name: string;
}

export interface WriteResult {
  rendered_path: string;
  sidecar_path: string;
}

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
