import { useEffect, useMemo, useRef, useState } from "react";
import {
  getSymbol,
  listDumps,
  listMembers,
  openDump,
  searchSymbols,
} from "@/ipc/client";
import type {
  DumpListItem,
  OpenDumpInfo,
  SearchHit,
  SymbolRow,
} from "@/ipc/types";
import { SYMBOL_KIND_LABEL } from "@/ipc/types";

const KIND_OPTIONS = [
  { value: 1, label: "class" },
  { value: 2, label: "struct" },
  { value: 3, label: "enum" },
  { value: 5, label: "function" },
  { value: 6, label: "field" },
];

export default function BrowseRoute() {
  const [dumps, setDumps] = useState<DumpListItem[]>([]);
  const [openInfo, setOpenInfo] = useState<OpenDumpInfo | null>(null);
  const [selectedDumpId, setSelectedDumpId] = useState<number | null>(null);

  const [query, setQuery] = useState("");
  const [kinds, setKinds] = useState<number[]>([]);
  const [moduleFilter, setModuleFilter] = useState<string>("");
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [totalMatched, setTotalMatched] = useState(0);
  const [searching, setSearching] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);

  const [selectedHit, setSelectedHit] = useState<SearchHit | null>(null);
  const [detail, setDetail] = useState<SymbolRow | null>(null);
  const [members, setMembers] = useState<SymbolRow[]>([]);

  // Load dumps on mount.
  useEffect(() => {
    listDumps()
      .then((items) => {
        setDumps(items);
        if (items.length > 0 && selectedDumpId == null) {
          setSelectedDumpId(items[0]?.id ?? null);
        }
      })
      .catch((e) => setSearchError(formatErr(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Open the selected dump (builds index if needed).
  useEffect(() => {
    if (selectedDumpId == null) return;
    openDump(selectedDumpId)
      .then(setOpenInfo)
      .catch((e) => setSearchError(formatErr(e)));
  }, [selectedDumpId]);

  // Debounced search whenever query/filters change.
  const searchSeqRef = useRef(0);
  useEffect(() => {
    if (selectedDumpId == null) return;
    const seq = ++searchSeqRef.current;
    const handle = setTimeout(() => {
      setSearching(true);
      searchSymbols({
        dumpId: selectedDumpId,
        query,
        kinds,
        modules: moduleFilter ? [moduleFilter] : [],
        limit: 200,
      })
        .then((result) => {
          if (seq !== searchSeqRef.current) return;
          setHits(result.hits);
          setTotalMatched(result.total_matched);
          setSearchError(null);
        })
        .catch((e) => {
          if (seq !== searchSeqRef.current) return;
          setSearchError(formatErr(e));
        })
        .finally(() => {
          if (seq === searchSeqRef.current) setSearching(false);
        });
    }, 80);
    return () => clearTimeout(handle);
  }, [selectedDumpId, query, kinds, moduleFilter]);

  // Load detail + members when a hit is selected.
  useEffect(() => {
    if (!selectedHit) {
      setDetail(null);
      setMembers([]);
      return;
    }
    getSymbol(selectedHit.id_hex)
      .then(setDetail)
      .catch((e) => setSearchError(formatErr(e)));
    // Members only make sense for class/struct/enum kinds.
    if (
      selectedHit.kind_i === 1 ||
      selectedHit.kind_i === 2 ||
      selectedHit.kind_i === 3
    ) {
      listMembers(selectedHit.id_hex)
        .then(setMembers)
        .catch((e) => setSearchError(formatErr(e)));
    } else {
      setMembers([]);
    }
  }, [selectedHit]);

  const toggleKind = (k: number) => {
    setKinds((prev) =>
      prev.includes(k) ? prev.filter((x) => x !== k) : [...prev, k],
    );
  };

  const headerLabel = useMemo(() => {
    if (!openInfo) return "Select a dump";
    return `${openInfo.game_id} ${openInfo.game_version} (${openInfo.symbol_count.toLocaleString()} symbols)`;
  }, [openInfo]);

  return (
    <div className="flex h-[calc(100vh-7rem)] gap-4">
      {/* Left rail: dump selector + facets */}
      <aside className="w-64 shrink-0 space-y-4 overflow-auto rounded-lg border border-atlas-border bg-atlas-surface p-3">
        <section>
          <h2 className="text-xs uppercase tracking-wide text-atlas-muted">
            Dump
          </h2>
          {dumps.length === 0 ? (
            <p className="mt-2 text-sm text-atlas-muted">
              No dumps yet. Ingest one from the Settings route once it lands,
              or use the `ingest_dump` command directly.
            </p>
          ) : (
            <select
              className="mt-2 w-full rounded border border-atlas-border bg-atlas-surface-2 px-2 py-1 text-sm"
              value={selectedDumpId ?? ""}
              onChange={(e) => setSelectedDumpId(Number(e.target.value))}
            >
              {dumps.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.game_id} {d.game_version}
                </option>
              ))}
            </select>
          )}
        </section>

        <section>
          <h2 className="text-xs uppercase tracking-wide text-atlas-muted">
            Kind
          </h2>
          <div className="mt-2 flex flex-wrap gap-1">
            {KIND_OPTIONS.map((opt) => {
              const active = kinds.includes(opt.value);
              return (
                <button
                  key={opt.value}
                  type="button"
                  onClick={() => toggleKind(opt.value)}
                  className={`rounded px-2 py-0.5 text-xs ${
                    active
                      ? "bg-atlas-accent/20 text-atlas-accent"
                      : "bg-atlas-surface-2 text-atlas-muted hover:text-atlas-text"
                  }`}
                >
                  {opt.label}
                </button>
              );
            })}
          </div>
        </section>

        <section>
          <h2 className="text-xs uppercase tracking-wide text-atlas-muted">
            Module
          </h2>
          <select
            className="mt-2 w-full rounded border border-atlas-border bg-atlas-surface-2 px-2 py-1 text-sm"
            value={moduleFilter}
            onChange={(e) => setModuleFilter(e.target.value)}
          >
            <option value="">all modules</option>
            {(openInfo?.modules ?? []).map((m) => (
              <option key={m} value={m}>
                {m}
              </option>
            ))}
          </select>
        </section>
      </aside>

      {/* Center: search input + hit list */}
      <section className="flex flex-1 flex-col gap-3 overflow-hidden">
        <header className="flex items-center justify-between gap-3">
          <input
            type="text"
            placeholder="Search symbols by name or FQN..."
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            className="flex-1 rounded-md border border-atlas-border bg-atlas-surface px-3 py-2 text-sm focus:border-atlas-accent focus:outline-none"
            autoFocus
          />
          <div className="text-xs text-atlas-muted">
            {searching ? "searching..." : `${totalMatched.toLocaleString()} matches`}
          </div>
        </header>

        <div className="rounded-md border border-atlas-border bg-atlas-surface px-3 py-1 text-xs text-atlas-muted">
          {headerLabel}
        </div>

        {searchError && (
          <div className="rounded-md border border-rose-500/30 bg-rose-500/10 px-3 py-2 text-sm text-rose-300">
            {searchError}
          </div>
        )}

        <ul className="flex-1 overflow-auto rounded-md border border-atlas-border bg-atlas-surface">
          {hits.length === 0 && !searching && (
            <li className="px-3 py-4 text-sm text-atlas-muted">
              No results.
            </li>
          )}
          {hits.map((h) => {
            const selected = selectedHit?.id_hex === h.id_hex;
            return (
              <li key={h.id_hex}>
                <button
                  type="button"
                  onClick={() => setSelectedHit(h)}
                  className={`flex w-full items-center justify-between px-3 py-1.5 text-left text-sm hover:bg-atlas-surface-2 ${
                    selected ? "bg-atlas-accent/10" : ""
                  }`}
                >
                  <span className="truncate font-mono text-atlas-text">
                    {h.fqn}
                  </span>
                  <span className="ml-3 shrink-0 text-xs text-atlas-muted">
                    {SYMBOL_KIND_LABEL[h.kind_i] ?? "?"} · {h.module}
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
      </section>

      {/* Right rail: detail */}
      <aside className="w-96 shrink-0 overflow-auto rounded-lg border border-atlas-border bg-atlas-surface p-3">
        {!detail ? (
          <p className="text-sm text-atlas-muted">
            Select a symbol to see its details.
          </p>
        ) : (
          <SymbolDetail row={detail} members={members} />
        )}
      </aside>
    </div>
  );
}

function SymbolDetail({
  row,
  members,
}: {
  row: SymbolRow;
  members: SymbolRow[];
}) {
  return (
    <div className="space-y-3">
      <div>
        <div className="text-xs uppercase tracking-wide text-atlas-muted">
          {SYMBOL_KIND_LABEL[row.kind] ?? "?"} · {row.module}
        </div>
        <div className="break-words font-mono text-sm text-atlas-text">
          {row.fqn}
        </div>
      </div>

      <dl className="grid grid-cols-2 gap-x-3 gap-y-1 text-xs">
        {row.size != null && (
          <Field label="size" value={`0x${row.size.toString(16)}`} />
        )}
        {row.offset != null && (
          <Field label="offset" value={`0x${row.offset.toString(16)}`} />
        )}
        {row.vtable_slot != null && (
          <Field
            label="vtable"
            value={`0x${row.vtable_slot.toString(16).padStart(2, "0")}`}
          />
        )}
        {row.align != null && <Field label="align" value={row.align.toString()} />}
        {row.source_file && (
          <Field
            label="source"
            value={`${row.source_file}:${row.source_line ?? "?"}`}
          />
        )}
      </dl>

      {members.length > 0 && (
        <section>
          <h3 className="text-xs uppercase tracking-wide text-atlas-muted">
            Members ({members.length})
          </h3>
          <table className="mt-1 w-full text-xs">
            <thead className="text-atlas-muted">
              <tr>
                <th className="text-left">offset</th>
                <th className="text-left">name</th>
                <th className="text-left">size</th>
              </tr>
            </thead>
            <tbody className="font-mono">
              {members.map((m) => (
                <tr key={hexFromBytes(m.id)} className="border-t border-atlas-border/40">
                  <td className="py-0.5">
                    {m.offset != null ? `0x${m.offset.toString(16)}` : "—"}
                  </td>
                  <td className="py-0.5 text-atlas-text">{m.name}</td>
                  <td className="py-0.5">
                    {m.size != null ? `0x${m.size.toString(16)}` : "—"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      )}
    </div>
  );
}

function Field({ label, value }: { label: string; value: string }) {
  return (
    <>
      <dt className="text-atlas-muted">{label}</dt>
      <dd className="font-mono text-atlas-text">{value}</dd>
    </>
  );
}

function hexFromBytes(bytes: number[]): string {
  return bytes.map((b) => b.toString(16).padStart(2, "0")).join("");
}

function formatErr(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  try {
    return JSON.stringify(e);
  } catch {
    return String(e);
  }
}
