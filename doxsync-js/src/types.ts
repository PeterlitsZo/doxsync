/** Numbers are floats; bigint values are integers in [-2^64, 2^64 - 1]. */
export type SyncValue =
  | null
  | boolean
  | string
  | number
  | bigint
  | Uint8Array
  | SyncValue[]
  | { [key: string]: SyncValue };

export type InitSource =
  | string
  | URL
  | ArrayBuffer
  | Uint8Array
  | WebAssembly.Module;

/** Rust errors thrown by synchronization and value conversion. */
export interface DoxsyncError extends Error {
  kind: "Internal" | "InvalidData" | "UnexpectedType";
}
