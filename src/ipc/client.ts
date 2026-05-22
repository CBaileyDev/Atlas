/**
 * Thin, typed wrappers around `@tauri-apps/api/core.invoke`. The frontend
 * never calls `invoke` directly — it goes through these functions so the
 * IPC surface stays auditable from one file.
 */

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import type {
  AtlasSettings,
  Diff,
  DumpListItem,
  ExportRequest,
  IngestReport,
  OpenDumpInfo,
  PingResponse,
  ResolvedSymbol,
  SearchResult,
  SymbolRow,
  TemplateInfo,
  WriteResult,
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

/** Compute a structural diff between two dumps. */
export function diffDumps(args: {
  baseDumpId: number;
  headDumpId: number;
}): Promise<Diff> {
  return invoke<Diff>("diff_dumps", {
    baseDumpId: args.baseDumpId,
    headDumpId: args.headDumpId,
  });
}

/** List bundled + overridden templates available to the export pipeline. */
export function listTemplates(): Promise<TemplateInfo[]> {
  return invoke<TemplateInfo[]>("list_templates");
}

/** Look up a list of FQNs against a dump; returns id_hex per FQN (null if not found). */
export function resolveFqns(args: {
  dumpId: number;
  fqns: string[];
}): Promise<ResolvedSymbol[]> {
  return invoke<ResolvedSymbol[]>("resolve_fqns", {
    dumpId: args.dumpId,
    fqns: args.fqns,
  });
}

/** Render the export to a string for live preview. */
export function renderExportPreview(req: ExportRequest): Promise<string> {
  return invoke<string>("render_export_preview", { req });
}

/** Write the export and its `_atlas.json` sidecar to a directory. */
export function writeExport(args: {
  req: ExportRequest;
  destDir: string;
  outputFilename: string;
}): Promise<WriteResult> {
  return invoke<WriteResult>("write_export", {
    req: {
      ...args.req,
      dest_dir: args.destDir,
      output_filename: args.outputFilename,
    },
  });
}

/** Read the persisted settings. */
export function getSettings(): Promise<AtlasSettings> {
  return invoke<AtlasSettings>("get_settings");
}

/** Write the full settings object. */
export function saveSettings(settings: AtlasSettings): Promise<void> {
  return invoke<void>("save_settings", { settings });
}

/** Append a directory to the watcher roots and persist. */
export function addWatcherRoot(root: string): Promise<AtlasSettings> {
  return invoke<AtlasSettings>("add_watcher_root", { root });
}

/** Remove a directory from the watcher roots and persist. */
export function removeWatcherRoot(root: string): Promise<AtlasSettings> {
  return invoke<AtlasSettings>("remove_watcher_root", { root });
}
