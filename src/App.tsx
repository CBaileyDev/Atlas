import { useEffect, useState } from "react";
import { ping } from "@/ipc/client";
import type { PingResponse } from "@/ipc/types";
import BrowseRoute from "@/routes/browse";
import DiffRoute from "@/routes/diff";
import ExportRoute from "@/routes/export";

type ConnState =
  | { status: "idle" }
  | { status: "loading" }
  | { status: "ok"; response: PingResponse }
  | { status: "error"; message: string };

const TABS = ["browse", "diff", "export", "settings"] as const;
type Tab = (typeof TABS)[number];

export default function App() {
  const [tab, setTab] = useState<Tab>("browse");
  const [conn, setConn] = useState<ConnState>({ status: "idle" });

  useEffect(() => {
    let cancelled = false;
    setConn({ status: "loading" });
    ping("hello from frontend")
      .then((response) => {
        if (!cancelled) setConn({ status: "ok", response });
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        const message =
          err instanceof Error
            ? err.message
            : typeof err === "string"
              ? err
              : JSON.stringify(err);
        setConn({ status: "error", message });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="flex h-screen flex-col">
      <header className="flex items-center justify-between border-b border-atlas-border bg-atlas-surface px-4 py-2">
        <div className="flex items-center gap-3">
          <span className="grid h-7 w-7 place-items-center rounded-md bg-atlas-accent/15 font-bold text-atlas-accent">
            A
          </span>
          <div>
            <div className="text-sm font-semibold">Codex Atlas</div>
            <div className="text-xs text-atlas-muted">
              v{conn.status === "ok" ? conn.response.version : "0.0.0"}
            </div>
          </div>
        </div>
        <nav className="flex items-center gap-1">
          {TABS.map((t) => (
            <button
              key={t}
              type="button"
              onClick={() => setTab(t)}
              className={`rounded px-3 py-1.5 text-sm capitalize transition-colors ${
                tab === t
                  ? "bg-atlas-accent/15 text-atlas-accent"
                  : "text-atlas-muted hover:bg-atlas-surface-2 hover:text-atlas-text"
              }`}
            >
              {t}
            </button>
          ))}
        </nav>
        <ConnBadge conn={conn} />
      </header>

      <main className="flex-1 overflow-auto bg-atlas-bg p-6">
        <RouteView tab={tab} />
      </main>

      <footer className="flex items-center justify-between border-t border-atlas-border bg-atlas-surface px-4 py-2 text-xs text-atlas-muted">
        <span>Local data. No network.</span>
        <span>
          {conn.status === "ok"
            ? `Connected at ${new Date(conn.response.timestamp).toLocaleTimeString()}`
            : conn.status === "loading"
              ? "Connecting…"
              : conn.status === "error"
                ? `Error: ${conn.message}`
                : "Idle"}
        </span>
      </footer>
    </div>
  );
}

function ConnBadge({ conn }: { conn: ConnState }) {
  const labels: Record<ConnState["status"], string> = {
    idle: "Idle",
    loading: "Pinging…",
    ok: "Connected",
    error: "Disconnected",
  };
  const colors: Record<ConnState["status"], string> = {
    idle: "bg-atlas-border text-atlas-muted",
    loading: "bg-amber-500/15 text-amber-400",
    ok: "bg-emerald-500/15 text-emerald-400",
    error: "bg-rose-500/15 text-rose-400",
  };
  return (
    <span
      className={`rounded-full px-2.5 py-0.5 text-xs font-medium ${colors[conn.status]}`}
      data-testid="conn-badge"
    >
      {labels[conn.status]}
      {conn.status === "ok" ? `: ${conn.response.pong}` : ""}
    </span>
  );
}

function RouteView({ tab }: { tab: Tab }) {
  switch (tab) {
    case "browse":
      return <BrowseRoute />;
    case "diff":
      return <DiffRoute />;
    case "export":
      return <ExportRoute />;
    case "settings":
      return <PlaceholderRoute title="Settings" body="Settings land in Phase 5." />;
  }
}

function PlaceholderRoute({ title, body }: { title: string; body: string }) {
  return (
    <section className="mx-auto max-w-2xl rounded-lg border border-atlas-border bg-atlas-surface p-6">
      <h1 className="text-lg font-semibold">{title}</h1>
      <p className="mt-2 text-sm text-atlas-muted">{body}</p>
    </section>
  );
}
