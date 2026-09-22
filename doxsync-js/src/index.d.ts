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

/**
 * Initialize once before constructing Producer or Consumer.
 * Defaults to the packaged WASM. Concurrent calls share the first call's source
 * and promise; failures can be retried. Later successful calls are no-ops.
 * Strings are URLs, not filesystem paths; Node also accepts file: URLs.
 */
export function init(source?: InitSource): Promise<void>;

export class Producer {
  #private;
  /** Copies the value into WASM. */
  constructor(value: SyncValue);
  /** Replaces the document after validating and copying the entire value. */
  replace(value: SyncValue): void;
  /**
   * Returns a snapshot on the first call, then changes since the last message.
   * Returns undefined if unchanged. Retain each message until delivered in order.
   */
  produceDiff(): Uint8Array | undefined;
  /** Release WASM resources. Safe to call repeatedly. */
  free(): void;
}

export class Consumer {
  #private;
  constructor();
  /** Applies one complete message; failures leave the document and pools intact. */
  consumeDiff(bytes: Uint8Array): void;
  /** Returns an independent copy, or undefined before the first snapshot. */
  document(): SyncValue | undefined;
  /** Release WASM resources. Safe to call repeatedly. */
  free(): void;
}
