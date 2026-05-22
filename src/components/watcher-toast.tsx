import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { DumpDetectedEvent } from "@/ipc/types";
import { ingestDump } from "@/ipc/client";

interface Toast {
  id: number;
  path: string;
  state: "idle" | "ingesting" | "done" | "error";
  message?: string;
}

/**
 * Listens for `watcher:dump-detected` events emitted by the backend
 * folder watcher and surfaces a toast in the bottom-right corner
 * with an "Ingest" button.
 */
export default function WatcherToast() {
  const [toasts, setToasts] = useState<Toast[]>([]);

  useEffect(() => {
    let nextId = 1;
    const sub = listen<DumpDetectedEvent>("watcher:dump-detected", (e) => {
      const id = nextId++;
      setToasts((cur) => [...cur, { id, path: e.payload.path, state: "idle" }]);
    });
    return () => {
      sub.then((un) => un());
    };
  }, []);

  const dismiss = (id: number) =>
    setToasts((cur) => cur.filter((t) => t.id !== id));

  const ingest = async (t: Toast) => {
    setToasts((cur) =>
      cur.map((x) => (x.id === t.id ? { ...x, state: "ingesting" } : x)),
    );
    try {
      const r = await ingestDump(t.path);
      setToasts((cur) =>
        cur.map((x) =>
          x.id === t.id
            ? {
                ...x,
                state: "done",
                message: `${r.symbols_inserted.toLocaleString()} symbols`,
              }
            : x,
        ),
      );
      // Auto-dismiss after a few seconds on success.
      setTimeout(() => dismiss(t.id), 4_000);
    } catch (e) {
      setToasts((cur) =>
        cur.map((x) =>
          x.id === t.id ? { ...x, state: "error", message: formatErr(e) } : x,
        ),
      );
    }
  };

  if (toasts.length === 0) return null;
  return (
    <div className="pointer-events-none fixed bottom-4 right-4 z-50 flex w-96 flex-col gap-2">
      {toasts.map((t) => (
        <div
          key={t.id}
          className="pointer-events-auto rounded-lg border border-atlas-border bg-atlas-surface p-3 shadow-lg"
        >
          <div className="flex items-start justify-between gap-2">
            <div className="min-w-0 flex-1">
              <div className="text-xs uppercase tracking-wide text-atlas-muted">
                New dump detected
              </div>
              <div className="mt-0.5 truncate font-mono text-xs text-atlas-text">
                {t.path}
              </div>
            </div>
            <button
              type="button"
              onClick={() => dismiss(t.id)}
              className="text-atlas-muted hover:text-atlas-text"
              aria-label="Dismiss"
            >
              ×
            </button>
          </div>

          {t.state === "idle" && (
            <div className="mt-2 flex justify-end gap-2">
              <button
                type="button"
                onClick={() => dismiss(t.id)}
                className="rounded px-2 py-1 text-xs text-atlas-muted hover:text-atlas-text"
              >
                Skip
              </button>
              <button
                type="button"
                onClick={() => ingest(t)}
                className="rounded bg-atlas-accent/20 px-2 py-1 text-xs font-medium text-atlas-accent"
              >
                Ingest
              </button>
            </div>
          )}
          {t.state === "ingesting" && (
            <div className="mt-2 text-xs text-amber-300">Ingesting…</div>
          )}
          {t.state === "done" && (
            <div className="mt-2 text-xs text-emerald-300">
              Done. {t.message}
            </div>
          )}
          {t.state === "error" && (
            <div className="mt-2 break-words text-xs text-rose-300">
              {t.message}
            </div>
          )}
        </div>
      ))}
    </div>
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
