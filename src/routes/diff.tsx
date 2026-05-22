import { useEffect, useMemo, useState } from "react";
import { diffDumps, listDumps } from "@/ipc/client";
import type { ChangeKind, Diff, DumpListItem, FieldChange } from "@/ipc/types";

const FILTERS = ["matches", "added", "removed", "renamed", "fields"] as const;
type Filter = (typeof FILTERS)[number];

export default function DiffRoute() {
  const [dumps, setDumps] = useState<DumpListItem[]>([]);
  const [baseId, setBaseId] = useState<number | null>(null);
  const [headId, setHeadId] = useState<number | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<Diff | null>(null);

  const [filter, setFilter] = useState<Filter>("fields");

  useEffect(() => {
    listDumps()
      .then((items) => {
        setDumps(items);
        if (items.length >= 2) {
          // Default: oldest = base, newest = head.
          setBaseId(items[items.length - 1]?.id ?? null);
          setHeadId(items[0]?.id ?? null);
        } else if (items.length === 1) {
          setBaseId(items[0]?.id ?? null);
        }
      })
      .catch((e) => setError(formatErr(e)));
  }, []);

  const canRun = baseId != null && headId != null && baseId !== headId;

  const run = () => {
    if (!canRun || baseId == null || headId == null) return;
    setRunning(true);
    setError(null);
    diffDumps({ baseDumpId: baseId, headDumpId: headId })
      .then(setResult)
      .catch((e) => setError(formatErr(e)))
      .finally(() => setRunning(false));
  };

  const summary = useMemo(() => {
    if (!result) return null;
    return {
      matches: result.matches.length,
      added: result.added.length,
      removed: result.removed.length,
      renamed: result.renamed_suggestions.length,
      fields: result.field_changes.length,
    };
  }, [result]);

  return (
    <div className="flex h-[calc(100vh-7rem)] flex-col gap-3">
      <header className="flex items-end gap-3 rounded-lg border border-atlas-border bg-atlas-surface p-3">
        <DumpSelect
          label="Base"
          dumps={dumps}
          value={baseId}
          onChange={setBaseId}
        />
        <span className="pb-2 text-atlas-muted">→</span>
        <DumpSelect
          label="Head"
          dumps={dumps}
          value={headId}
          onChange={setHeadId}
        />
        <button
          type="button"
          onClick={run}
          disabled={!canRun || running}
          className="ml-auto rounded-md bg-atlas-accent/20 px-3 py-1.5 text-sm font-medium text-atlas-accent disabled:opacity-50"
        >
          {running ? "diffing..." : "Run diff"}
        </button>
      </header>

      {error && (
        <div className="rounded-md border border-rose-500/30 bg-rose-500/10 px-3 py-2 text-sm text-rose-300">
          {error}
        </div>
      )}

      {!result && !running && (
        <div className="rounded-md border border-atlas-border bg-atlas-surface p-6 text-center text-sm text-atlas-muted">
          Pick two dumps and click Run diff.
        </div>
      )}

      {result && summary && (
        <>
          <nav className="flex items-center gap-1 rounded-md border border-atlas-border bg-atlas-surface px-2 py-1">
            {FILTERS.map((f) => (
              <button
                key={f}
                type="button"
                onClick={() => setFilter(f)}
                className={`rounded px-2 py-1 text-xs capitalize ${
                  filter === f
                    ? "bg-atlas-accent/20 text-atlas-accent"
                    : "text-atlas-muted hover:bg-atlas-surface-2 hover:text-atlas-text"
                }`}
              >
                {f} ({summary[f]})
              </button>
            ))}
            <span className="ml-auto text-xs text-atlas-muted">
              {result.game_id} · {result.base_version} → {result.head_version}
            </span>
          </nav>

          <div className="flex-1 overflow-auto rounded-md border border-atlas-border bg-atlas-surface">
            {filter === "matches" && <MatchesList rows={result.matches} />}
            {filter === "added" && (
              <RefList glyph="+" rows={result.added} />
            )}
            {filter === "removed" && (
              <RefList glyph="−" rows={result.removed} />
            )}
            {filter === "renamed" && (
              <SuggestionsList rows={result.renamed_suggestions} />
            )}
            {filter === "fields" && (
              <FieldChangesList rows={result.field_changes} />
            )}
          </div>
        </>
      )}
    </div>
  );
}

function DumpSelect({
  label,
  dumps,
  value,
  onChange,
}: {
  label: string;
  dumps: DumpListItem[];
  value: number | null;
  onChange: (v: number) => void;
}) {
  return (
    <label className="flex flex-1 flex-col gap-1 text-xs text-atlas-muted">
      {label}
      <select
        className="rounded border border-atlas-border bg-atlas-surface-2 px-2 py-1 text-sm text-atlas-text"
        value={value ?? ""}
        onChange={(e) => onChange(Number(e.target.value))}
      >
        <option value="">—</option>
        {dumps.map((d) => (
          <option key={d.id} value={d.id}>
            {d.game_id} {d.game_version} ({d.symbol_count.toLocaleString()} symbols)
          </option>
        ))}
      </select>
    </label>
  );
}

function MatchesList({ rows }: { rows: Diff["matches"] }) {
  if (rows.length === 0)
    return <Empty body="No matches in this filter." />;
  return (
    <ul className="divide-y divide-atlas-border/40 font-mono text-xs">
      {rows.map((m, i) => (
        <li key={`${m.base.fqn}-${m.head.fqn}-${i}`} className="grid grid-cols-3 gap-2 px-3 py-1.5">
          <span className="truncate text-atlas-muted">{m.base.fqn}</span>
          <span className="truncate text-atlas-text">{m.head.fqn}</span>
          <span className="text-right text-atlas-muted">
            {m.method} · {m.score.toFixed(2)}
          </span>
        </li>
      ))}
    </ul>
  );
}

function RefList({
  glyph,
  rows,
}: {
  glyph: string;
  rows: Diff["added"] | Diff["removed"];
}) {
  if (rows.length === 0) return <Empty body="Nothing in this filter." />;
  return (
    <ul className="divide-y divide-atlas-border/40 font-mono text-xs">
      {rows.map((s, i) => (
        <li key={`${s.fqn}-${i}`} className="flex items-center gap-2 px-3 py-1.5">
          <span className="text-atlas-muted">{glyph}</span>
          <span className="truncate text-atlas-text">{s.fqn}</span>
          <span className="ml-auto text-atlas-muted">{s.module}</span>
        </li>
      ))}
    </ul>
  );
}

function SuggestionsList({ rows }: { rows: Diff["renamed_suggestions"] }) {
  if (rows.length === 0)
    return <Empty body="No rename suggestions for this dump pair." />;
  return (
    <ul className="divide-y divide-atlas-border/40 font-mono text-xs">
      {rows.map((s, i) => (
        <li key={`${s.base.fqn}-${s.head.fqn}-${i}`} className="grid grid-cols-3 gap-2 px-3 py-1.5">
          <span className="truncate text-atlas-muted">⇄ {s.base.fqn}</span>
          <span className="truncate text-atlas-text">{s.head.fqn}</span>
          <span className="text-right text-atlas-muted">score {s.score.toFixed(2)}</span>
        </li>
      ))}
    </ul>
  );
}

function FieldChangesList({ rows }: { rows: FieldChange[] }) {
  if (rows.length === 0) return <Empty body="No field-level changes." />;
  return (
    <ul className="divide-y divide-atlas-border/40 font-mono text-xs">
      {rows.map((fc, i) => (
        <li key={`${fc.parent_fqn}-${i}`} className="grid grid-cols-3 gap-2 px-3 py-1.5">
          <span className="truncate text-atlas-muted">{fc.parent_fqn}</span>
          <span className="col-span-2 text-atlas-text">{describeChange(fc.change)}</span>
        </li>
      ))}
    </ul>
  );
}

function describeChange(c: ChangeKind): string {
  switch (c.kind) {
    case "offset_changed":
      return `${c.field}: offset 0x${c.old_offset.toString(16)} → 0x${c.new_offset.toString(16)}`;
    case "size_changed":
      return `${c.field}: size 0x${c.old_size.toString(16)} → 0x${c.new_size.toString(16)}`;
    case "vtable_shift":
      return `${c.fn_name}: vtable [0x${c.old_slot.toString(16)} → 0x${c.new_slot.toString(16)}]`;
    case "parent_class_changed":
      return `parent ${c.old_parent ?? "(none)"} → ${c.new_parent ?? "(none)"}`;
    case "field_added":
      return `+ ${c.field}`;
    case "field_removed":
      return `− ${c.field}`;
    case "function_signature_changed":
      return `${c.fn_name}: signature ${c.old_return ?? "?"} → ${c.new_return ?? "?"}`;
    case "field_type_substituted":
      return `${c.field}: type ${c.old_type} → ${c.new_type}`;
  }
}

function Empty({ body }: { body: string }) {
  return <p className="px-3 py-6 text-center text-sm text-atlas-muted">{body}</p>;
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
