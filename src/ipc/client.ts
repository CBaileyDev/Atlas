/**
 * Thin, typed wrappers around `@tauri-apps/api/core.invoke`. The frontend
 * never calls `invoke` directly — it goes through these functions so the
 * IPC surface stays auditable from one file.
 */

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import type {
  DumpListItem,
  IngestReport,
  OpenDumpInfo,
  PingResponse,
  SearchResult,
  SymbolRow,
} from "./types";

function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return tauriInvoke<T>(cmd, args);
}

/** Phase 0 sanity ping. */
export function ping(message?: string): Promise<PingResponse> {
  return invoke<PingResponse>("ping", { message });
}

/** Ingest a Dumper-7 SDK folder. */
export function ingestDump(path: string): Promise<IngestReport> {
  return invoke<IngestReport>("ingest_dump", { path });
}

/** List every dump in the database, most-recent first. */
export function listDumps(): Promise<DumpListItem[]> {
  return invoke<DumpListItem[]>("list_dumps");
}

/** Open a dump for browsing. Builds the tantivy index if needed. */
export function openDump(dumpId: number): Promise<OpenDumpInfo> {
  return invoke<OpenDumpInfo>("open_dump", { dumpId });
}

/** Search a dump. `kinds` and `modules` are AND-combined; empty = no filter. */
export function searchSymbols(args: {
  dumpId: number;
  query: string;
  kinds?: number[];
  modules?: string[];
  limit?: number;
}): Promise<SearchResult> {
  return invoke<SearchResult>("search_symbols", {
    dumpId: args.dumpId,
    query: args.query,
    kinds: args.kinds ?? [],
    modules: args.modules ?? [],
    limit: args.limit,
  });
}

/** Fetch a symbol by its 32-char hex id. */
export function getSymbol(idHex: string): Promise<SymbolRow | null> {
  return invoke<SymbolRow | null>("get_symbol", { idHex });
}

/** List the contained members of a class/struct/enum. */
export function listMembers(classIdHex: string): Promise<SymbolRow[]> {
  return invoke<SymbolRow[]>("list_members", { classIdHex });
}
