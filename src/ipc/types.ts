/**
 * Hand-written IPC types for Phase 0. Once we wire ts-rs in Phase 1,
 * this file will be replaced by a generated bindings file. The
 * `// @generated` marker below tells reviewers and the linter to stop
 * looking here for handwritten changes once that switch happens.
 *
 * @generated:false
 */

export interface PingResponse {
  pong: string;
  echoed: string | null;
  timestamp: string;
  version: string;
}

/**
 * Mirrors `AppError` on the Rust side. Tauri serializes Result::Err with
 * the `kind` / `message` discriminated-union shape because that's what
 * #[serde(tag, content)] produces.
 */
export type AppError =
  | { kind: "Atlas"; message: AtlasError }
  | { kind: "Tauri"; message: string }
  | { kind: "Internal"; message: string };

export type AtlasError =
  | { kind: "Parser"; message: string }
  | { kind: "Storage"; message: string }
  | { kind: "Search"; message: string }
  | { kind: "Diff"; message: string }
  | { kind: "Export"; message: string }
  | { kind: "Io"; message: string }
  | { kind: "InvalidInput"; message: string }
  | { kind: "NotFound"; message: string };
