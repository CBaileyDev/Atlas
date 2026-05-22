import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";

// Mock the Tauri IPC layer before importing the App. In a real Tauri
// window the bridge is injected by the runtime; in jsdom we substitute
// our own implementation.
vi.mock("@/ipc/client", async () => {
  return {
    ping: vi.fn(async (message?: string) => ({
      pong: "pong",
      echoed: message ?? null,
      timestamp: "2026-01-01T00:00:00Z",
      version: "0.0.0",
    })),
    listDumps: vi.fn(async () => []),
    openDump: vi.fn(async () => ({
      id: 0,
      game_id: "test",
      game_version: "0",
      symbol_count: 0,
      modules: [],
    })),
    searchSymbols: vi.fn(async () => ({
      query: "",
      total_matched: 0,
      hits: [],
    })),
    getSymbol: vi.fn(async () => null),
    listMembers: vi.fn(async () => []),
    ingestDump: vi.fn(),
    diffDumps: vi.fn(async () => ({
      game_id: "",
      base_version: "",
      head_version: "",
      matches: [],
      added: [],
      removed: [],
      renamed_suggestions: [],
      field_changes: [],
    })),
  };
});

import App from "./App";

describe("App", () => {
  it("renders the four route tabs", () => {
    render(<App />);
    expect(screen.getByRole("button", { name: /browse/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /diff/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /export/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /settings/i })).toBeInTheDocument();
  });

  it("calls ping on mount and displays the pong response", async () => {
    render(<App />);
    const badge = screen.getByTestId("conn-badge");
    await waitFor(() => {
      expect(badge.textContent).toMatch(/Connected/);
    });
    expect(badge.textContent).toMatch(/pong/);
  });
});
