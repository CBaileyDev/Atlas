import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  addWatcherRoot,
  getSettings,
  ingestDump,
  listDumps,
  removeWatcherRoot,
} from "@/ipc/client";
import type { AtlasSettings, DumpListItem, IngestReport } from "@/ipc/types";
import { listen } from "@tauri-apps/api/event";
import { useTheme } from "@/stores/theme";

interface ProgressState {
  current: number;
  total: number | null;
  label: string;
}

export default function SettingsRoute() {
  const theme = useTheme((s) => s.theme);
  const setTheme = useTheme((s) => s.setTheme);
  const [dumps, setDumps] = useState<DumpListItem[]>([]);
  const [settings, setSettings] = useState<AtlasSettings | null>(null);
  const [pickedPath, setPickedPath] = useState<string | null>(null);
  const [ingesting, setIngesting] = useState(false);
  const [progress, setProgress] = useState<ProgressState | null>(null);
  const [report, setReport] = useState<IngestReport | null>(null);
  const [warnings, setWarnings] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);

  const refreshDumps = () => {
    listDumps()
      .then(setDumps)
      .catch((e) => setError(formatErr(e)));
  };

  useEffect(() => {
    refreshDumps();
    getSettings()
      .then(setSettings)
      .catch(() => undefined);
  }, []);

  const pickAndAddRoot = async () => {
    try {
      const picked = (await open({ directory: true, multiple: false })) as
        | string
        | null;
      if (!picked) return;
      const next = await addWatcherRoot(picked);
      setSettings(next);
    } catch (e) {
      setError(formatErr(e));
    }
  };

  const removeRoot = async (root: string) => {
    try {
      const next = await removeWatcherRoot(root);
      setSettings(next);
    } catch (e) {
      setError(formatErr(e));
    }
  };

  useEffect(() => {
    const unsubs: Array<() => void> = [];
    listen<ProgressState>("ingest:progress", (e) => {
      setProgress(e.payload);
    }).then((u) => unsubs.push(u));
    listen<string>("ingest:warn", (e) => {
      setWarnings((w) => [...w, e.payload]);
    }).then((u) => unsubs.push(u));
    listen("ingest:started", () => {
      setWarnings([]);
      setProgress({ current: 0, total: null, label: "starting" });
    }).then((u) => unsubs.push(u));
    listen("ingest:finished", () => {
      setProgress(null);
    }).then((u) => unsubs.push(u));
    return () => {
      for (const u of unsubs) u();
    };
  }, []);

  const pickFolder = async () => {
    setError(null);
    setReport(null);
    try {
      const result = (await open({ directory: true, multiple: false })) as
        | string
        | null;
      setPickedPath(result);
    } catch (e) {
      setError(formatErr(e));
    }
  };

  const runIngest = async () => {
    if (!pickedPath) return;
    setIngesting(true);
    setError(null);
    setReport(null);
    setWarnings([]);
    try {
      const r = await ingestDump(pickedPath);
      setReport(r);
      refreshDumps();
    } catch (e) {
      setError(formatErr(e));
    } finally {
      setIngesting(false);
    }
  };

  return (
    <div className="mx-auto max-w-3xl space-y-6">
      <section className="flex items-center justify-between rounded-lg border border-atlas-border bg-atlas-surface p-4">
        <div>
          <h2 className="text-base font-semibold">Appearance</h2>
          <p className="text-xs text-atlas-muted">
            Theme switches instantly. Persisted across launches.
          </p>
        </div>
        <div className="flex items-center gap-1 rounded-md border border-atlas-border p-0.5">
          <ThemeButton
            value="dark"
            label="Dark"
            current={theme}
            onSelect={setTheme}
          />
          <ThemeButton
            value="light"
            label="Light"
            current={theme}
            onSelect={setTheme}
          />
        </div>
      </section>

      <section className="rounded-lg border border-atlas-border bg-atlas-surface p-4">
        <h2 className="text-base font-semibold">Ingest a Dumper-7 SDK</h2>
        <p className="mt-1 text-sm text-atlas-muted">
          Point Atlas at the root of a Dumper-7 output (the folder that
          contains <code className="font-mono text-xs">CppSDK/</code> and
          <code className="font-mono text-xs"> _SDKInfo.json</code>).
        </p>

        <div className="mt-3 flex items-center gap-2">
          <button
            type="button"
            onClick={pickFolder}
            disabled={ingesting}
            className="rounded-md border border-atlas-border bg-atlas-surface-2 px-3 py-1.5 text-sm hover:bg-atlas-border disabled:opacity-50"
          >
            Pick folder…
          </button>
          <code className="flex-1 truncate font-mono text-xs text-atlas-muted">
            {pickedPath ?? "(none selected)"}
          </code>
          <button
            type="button"
            onClick={runIngest}
            disabled={!pickedPath || ingesting}
            className="rounded-md bg-atlas-accent/20 px-3 py-1.5 text-sm font-medium text-atlas-accent disabled:opacity-50"
          >
            {ingesting ? "Ingesting…" : "Ingest"}
          </button>
        </div>

        {progress && (
          <div className="mt-3 rounded border border-atlas-border bg-atlas-surface-2 px-3 py-2 text-xs">
            <div className="flex items-center justify-between text-atlas-muted">
              <span className="truncate">{progress.label}</span>
              <span>
                {progress.current}
                {progress.total != null ? ` / ${progress.total}` : ""}
              </span>
            </div>
            <div className="mt-1 h-1 overflow-hidden rounded bg-atlas-border">
              {progress.total ? (
                <div
                  className="h-full bg-atlas-accent transition-all"
                  style={{
                    width: `${Math.min(100, (progress.current / progress.total) * 100)}%`,
                  }}
                />
              ) : (
                <div className="h-full w-1/4 animate-pulse bg-atlas-accent/40" />
              )}
            </div>
          </div>
        )}

        {report && (
          <div className="mt-3 rounded-md border border-emerald-500/30 bg-emerald-500/10 px-3 py-2 text-sm text-emerald-300">
            <div>
              Ingested <span className="font-mono">{report.game_id}</span>{" "}
              <span className="font-mono">{report.game_version}</span> as dump
              #{report.dump_id}.
            </div>
            <div className="mt-1 text-xs text-atlas-muted">
              {report.symbols_inserted.toLocaleString()} symbols (
              {report.symbols_skipped} skipped duplicates),{" "}
              {report.relations_inserted.toLocaleString()} relations,{" "}
              {warnings.length} runtime warnings,{" "}
              {report.warnings.length} report warnings.
            </div>
          </div>
        )}

        {error && (
          <div className="mt-3 rounded-md border border-rose-500/30 bg-rose-500/10 px-3 py-2 text-sm text-rose-300">
            {error}
          </div>
        )}

        {warnings.length > 0 && (
          <details className="mt-3 rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs text-amber-200">
            <summary className="cursor-pointer">
              {warnings.length} runtime warnings
            </summary>
            <ul className="mt-2 max-h-40 overflow-auto font-mono">
              {warnings.slice(-50).map((w, i) => (
                <li key={i} className="truncate">
                  {w}
                </li>
              ))}
            </ul>
          </details>
        )}
      </section>

      <section className="rounded-lg border border-atlas-border bg-atlas-surface p-4">
        <h2 className="text-base font-semibold">Stored dumps</h2>
        {dumps.length === 0 ? (
          <p className="mt-1 text-sm text-atlas-muted">No dumps yet.</p>
        ) : (
          <table className="mt-2 w-full text-sm">
            <thead className="text-left text-xs text-atlas-muted">
              <tr>
                <th className="py-1">id</th>
                <th>game</th>
                <th>version</th>
                <th>parser</th>
                <th className="text-right">symbols</th>
                <th>ingested</th>
              </tr>
            </thead>
            <tbody className="font-mono text-xs">
              {dumps.map((d) => (
                <tr
                  key={d.id}
                  className="border-t border-atlas-border/40"
                >
                  <td className="py-1 text-atlas-muted">{d.id}</td>
                  <td>{d.game_id}</td>
                  <td>{d.game_version}</td>
                  <td className="text-atlas-muted">{d.parser}</td>
                  <td className="text-right">
                    {d.symbol_count.toLocaleString()}
                  </td>
                  <td className="text-atlas-muted">
                    {new Date(d.ingested_at).toLocaleString()}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>

      <section className="rounded-lg border border-atlas-border bg-atlas-surface p-4">
        <div className="flex items-center justify-between">
          <div>
            <h2 className="text-base font-semibold">Folder watcher</h2>
            <p className="mt-1 text-xs text-atlas-muted">
              Atlas watches each root for new Dumper-7-style folders. When one
              appears and stops changing for{" "}
              {settings ? Math.round(settings.watcher_debounce_ms / 1000) : 5}
              {" "}seconds, a toast in the corner offers a one-click ingest.
              Changes here take effect after the next app launch.
            </p>
          </div>
          <button
            type="button"
            onClick={pickAndAddRoot}
            className="rounded-md border border-atlas-border bg-atlas-surface-2 px-3 py-1.5 text-sm hover:bg-atlas-border"
          >
            Add root…
          </button>
        </div>
        {settings?.watcher_roots.length === 0 ? (
          <p className="mt-3 text-xs text-atlas-muted">
            No roots configured.
          </p>
        ) : (
          <ul className="mt-3 space-y-1">
            {settings?.watcher_roots.map((r) => (
              <li
                key={r}
                className="flex items-center justify-between rounded border border-atlas-border bg-atlas-surface-2 px-2 py-1 font-mono text-xs"
              >
                <span className="truncate">{r}</span>
                <button
                  type="button"
                  onClick={() => removeRoot(r)}
                  className="text-atlas-muted hover:text-rose-300"
                  aria-label={`Remove ${r}`}
                >
                  remove
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="rounded-lg border border-atlas-border bg-atlas-surface p-4 text-sm">
        <h2 className="text-base font-semibold">Where Atlas keeps its data</h2>
        <p className="mt-1 text-atlas-muted">
          On Windows the database, indexes, logs, and user-editable
          templates live under
          <code className="ml-1 font-mono text-xs">%APPDATA%\CodexAtlas\</code>.
        </p>
        <ul className="mt-2 list-disc space-y-0.5 pl-5 text-xs text-atlas-muted">
          <li>
            <span className="font-mono">atlas.sqlite</span> — symbols, relations,
            dumps
          </li>
          <li>
            <span className="font-mono">index/&lt;dump_id&gt;/</span> — tantivy
            indexes (auto-rebuild if schema changes)
          </li>
          <li>
            <span className="font-mono">logs/atlas.log</span> — daily-rotated
            JSON logs (kept 7 days)
          </li>
          <li>
            <span className="font-mono">templates/&lt;name&gt;.tera</span> —
            optional overrides for the bundled export templates
          </li>
        </ul>
      </section>
    </div>
  );
}

function ThemeButton({
  value,
  label,
  current,
  onSelect,
}: {
  value: "dark" | "light";
  label: string;
  current: "dark" | "light";
  onSelect: (t: "dark" | "light") => void;
}) {
  const active = current === value;
  return (
    <button
      type="button"
      onClick={() => onSelect(value)}
      className={`rounded px-3 py-1 text-xs transition-colors ${
        active
          ? "bg-atlas-accent/20 text-atlas-accent"
          : "text-atlas-muted hover:text-atlas-text"
      }`}
    >
      {label}
    </button>
  );
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
