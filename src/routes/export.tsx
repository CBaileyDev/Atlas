import { useEffect, useMemo, useState } from "react";
import {
  listDumps,
  listTemplates,
  renderExportPreview,
  resolveFqns,
  writeExport,
} from "@/ipc/client";
import type {
  DumpListItem,
  ResolvedSymbol,
  TemplateInfo,
  WriteResult,
} from "@/ipc/types";
import { open } from "@tauri-apps/plugin-dialog";

export default function ExportRoute() {
  const [dumps, setDumps] = useState<DumpListItem[]>([]);
  const [dumpId, setDumpId] = useState<number | null>(null);
  const [templates, setTemplates] = useState<TemplateInfo[]>([]);
  const [templateName, setTemplateName] = useState<string>("");

  const [pasted, setPasted] = useState("");
  const [resolved, setResolved] = useState<ResolvedSymbol[]>([]);

  const [projectName, setProjectName] = useState("AtlasTrainer");
  const [trainerClassName, setTrainerClassName] = useState("AtlasTrainer");
  const [processName, setProcessName] = useState("");

  const [preview, setPreview] = useState<string>("");
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [previewing, setPreviewing] = useState(false);

  const [writeOk, setWriteOk] = useState<WriteResult | null>(null);
  const [writeErr, setWriteErr] = useState<string | null>(null);

  useEffect(() => {
    listDumps()
      .then((items) => {
        setDumps(items);
        if (items.length > 0) setDumpId(items[0]?.id ?? null);
      })
      .catch(() => undefined);
    listTemplates()
      .then((ts) => {
        setTemplates(ts);
        if (ts.length > 0) setTemplateName(ts[0]?.name ?? "");
      })
      .catch(() => undefined);
  }, []);

  const fqnList = useMemo(
    () =>
      pasted
        .split(/[\r\n]+/)
        .map((s) => s.trim())
        .filter(Boolean),
    [pasted],
  );

  // Resolve FQNs whenever the input or selected dump changes.
  useEffect(() => {
    if (dumpId == null || fqnList.length === 0) {
      setResolved([]);
      return;
    }
    let cancelled = false;
    resolveFqns({ dumpId, fqns: fqnList })
      .then((list) => {
        if (!cancelled) setResolved(list);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [dumpId, fqnList]);

  const selectedIds = useMemo(
    () => resolved.filter((r) => r.id_hex).map((r) => r.id_hex as string),
    [resolved],
  );
  const unresolvedCount = useMemo(
    () => resolved.filter((r) => !r.id_hex).length,
    [resolved],
  );

  // Debounced preview render (250 ms — plan §10 budget).
  useEffect(() => {
    if (dumpId == null || !templateName || selectedIds.length === 0) {
      setPreview("");
      return;
    }
    const handle = setTimeout(() => {
      setPreviewing(true);
      setPreviewError(null);
      renderExportPreview({
        dump_id: dumpId,
        symbol_ids_hex: selectedIds,
        template_name: templateName,
        project_name: projectName,
        trainer_class_name: trainerClassName,
        process_name: processName || "Game-Win64-Shipping",
      })
        .then(setPreview)
        .catch((e) => setPreviewError(formatErr(e)))
        .finally(() => setPreviewing(false));
    }, 200);
    return () => clearTimeout(handle);
  }, [
    dumpId,
    templateName,
    selectedIds,
    projectName,
    trainerClassName,
    processName,
  ]);

  const writeDisk = async () => {
    if (dumpId == null || !templateName || selectedIds.length === 0) return;
    setWriteErr(null);
    setWriteOk(null);
    let dest: string | null;
    try {
      dest = (await open({ directory: true, multiple: false })) as string | null;
    } catch (e) {
      setWriteErr(formatErr(e));
      return;
    }
    if (!dest) return;
    const tmpl = templates.find((t) => t.name === templateName);
    const outName = tmpl?.default_filename ?? `${templateName}.txt`;
    try {
      const result = await writeExport({
        req: {
          dump_id: dumpId,
          symbol_ids_hex: selectedIds,
          template_name: templateName,
          project_name: projectName,
          trainer_class_name: trainerClassName,
          process_name: processName || "Game-Win64-Shipping",
        },
        destDir: dest,
        outputFilename: outName,
      });
      setWriteOk(result);
    } catch (e) {
      setWriteErr(formatErr(e));
    }
  };

  return (
    <div className="flex h-[calc(100vh-7rem)] flex-col gap-3">
      <div className="grid grid-cols-2 gap-3">
        <div className="space-y-3 rounded-lg border border-atlas-border bg-atlas-surface p-3">
          <section>
            <label className="block text-xs uppercase tracking-wide text-atlas-muted">
              Dump
            </label>
            <select
              className="mt-1 w-full rounded border border-atlas-border bg-atlas-surface-2 px-2 py-1 text-sm"
              value={dumpId ?? ""}
              onChange={(e) => setDumpId(Number(e.target.value))}
            >
              {dumps.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.game_id} {d.game_version}
                </option>
              ))}
            </select>
          </section>

          <section>
            <label className="block text-xs uppercase tracking-wide text-atlas-muted">
              Template
            </label>
            <select
              className="mt-1 w-full rounded border border-atlas-border bg-atlas-surface-2 px-2 py-1 text-sm"
              value={templateName}
              onChange={(e) => setTemplateName(e.target.value)}
            >
              {templates.map((t) => (
                <option key={t.name} value={t.name}>
                  {t.name}
                  {t.overridden ? "  (user override)" : ""}
                </option>
              ))}
            </select>
            <p className="mt-1 text-xs text-atlas-muted">
              {templates.find((t) => t.name === templateName)?.description}
            </p>
          </section>

          <section className="grid grid-cols-3 gap-2">
            <TextField label="Project" value={projectName} onChange={setProjectName} />
            <TextField
              label="Class"
              value={trainerClassName}
              onChange={setTrainerClassName}
            />
            <TextField
              label="Process"
              value={processName}
              onChange={setProcessName}
              placeholder="Game-Win64-Shipping"
            />
          </section>

          <section>
            <label className="block text-xs uppercase tracking-wide text-atlas-muted">
              Symbol selection (one FQN per line)
            </label>
            <textarea
              className="mt-1 h-44 w-full resize-none rounded border border-atlas-border bg-atlas-surface-2 px-2 py-1 font-mono text-xs"
              value={pasted}
              onChange={(e) => setPasted(e.target.value)}
              placeholder={"Engine.Actor.Health\nEngine.Actor.bAlive"}
            />
            <div className="mt-1 text-xs text-atlas-muted">
              {selectedIds.length} resolved
              {unresolvedCount > 0 && (
                <span className="ml-2 text-amber-300">
                  ({unresolvedCount} unresolved)
                </span>
              )}
            </div>
          </section>

          <button
            type="button"
            disabled={selectedIds.length === 0 || !templateName}
            onClick={writeDisk}
            className="w-full rounded-md bg-atlas-accent/20 px-3 py-2 text-sm font-medium text-atlas-accent disabled:opacity-50"
          >
            Write to disk…
          </button>

          {writeOk && (
            <div className="rounded-md border border-emerald-500/30 bg-emerald-500/10 px-3 py-2 text-xs text-emerald-300">
              wrote {writeOk.rendered_path}
              <br />
              sidecar {writeOk.sidecar_path}
            </div>
          )}
          {writeErr && (
            <div className="rounded-md border border-rose-500/30 bg-rose-500/10 px-3 py-2 text-xs text-rose-300">
              {writeErr}
            </div>
          )}
        </div>

        <div className="flex flex-col rounded-lg border border-atlas-border bg-atlas-surface p-3">
          <div className="mb-2 flex items-center justify-between">
            <span className="text-xs uppercase tracking-wide text-atlas-muted">
              Preview
            </span>
            <span className="text-xs text-atlas-muted">
              {previewing ? "rendering…" : `${preview.length.toLocaleString()} chars`}
            </span>
          </div>
          {previewError ? (
            <pre className="flex-1 overflow-auto whitespace-pre-wrap rounded border border-rose-500/30 bg-rose-500/10 p-2 font-mono text-xs text-rose-300">
              {previewError}
            </pre>
          ) : (
            <pre className="flex-1 overflow-auto whitespace-pre rounded border border-atlas-border/40 bg-atlas-bg p-2 font-mono text-xs">
              {preview || "// Paste FQNs on the left, pick a template, see the result here."}
            </pre>
          )}
        </div>
      </div>
    </div>
  );
}

function TextField({
  label,
  value,
  onChange,
  placeholder,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
}) {
  return (
    <label className="block text-xs text-atlas-muted">
      {label}
      <input
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="mt-1 w-full rounded border border-atlas-border bg-atlas-surface-2 px-2 py-1 text-sm text-atlas-text"
      />
    </label>
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
