/**
 * Thin, typed wrappers around `@tauri-apps/api/core.invoke`. The frontend
 * never calls `invoke` directly — it goes through these functions so the
 * IPC surface stays auditable from one file.
 */

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import type { PingResponse } from "./types";

function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return tauriInvoke<T>(cmd, args);
}

/**
 * Calls the Phase-0 `ping` command. Used by the App on first mount to
 * prove the IPC bridge works end-to-end.
 */
export function ping(message?: string): Promise<PingResponse> {
  return invoke<PingResponse>("ping", { message });
}
