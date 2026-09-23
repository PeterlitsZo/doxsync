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
 * Initialize once before constructing Producer or Consumer or calling supportedProtocols.
 * Defaults to the packaged WASM. Concurrent calls share the first call's source
 * and promise; failures can be retried. Later successful calls are no-ops.
 * Strings are URLs, not filesystem paths; Node also accepts file: URLs.
 */
export function init(source?: InitSource): Promise<void>;

/**
 * Returns all protocol versions supported by this build, currently [1].
 * Requires await init(). Each call returns an independent ordinary array.
 * Consumers can advertise this list to producers for protocol negotiation.
 */
export function supportedProtocols(): number[];

export class Producer {
  #private;
  /**
   * Copies the value into WASM and selects the highest common protocol version.
   * protocols lists the consumer's supported versions as u32 integers.
   * Currently only 1 is supported. Missing, invalid, empty, or incompatible
   * lists throw InvalidData errors. Order and duplicates do not matter.
   */
  constructor(value: SyncValue, protocols: readonly number[]);
  /** Replaces the document after validating and copying the entire value. */
  replace(value: SyncValue): void;
  /**
   * Returns a snapshot with protocol metadata on the first call, then changes.
   * Returns undefined if unchanged. Retain each message until delivered in order.
   */
  produceDiff(): Uint8Array | undefined;
  /** Release WASM resources. Safe to call repeatedly. */
  free(): void;
}

export class Consumer {
  #private;
  constructor();
  /**
   * Applies one complete message. The first message must declare protocol 1.
   * Failures leave the document, pools, and protocol state intact.
   */
  consumeDiff(bytes: Uint8Array): void;
  /** Returns an independent copy, or undefined before the first snapshot. */
  document(): SyncValue | undefined;
  /** Release WASM resources. Safe to call repeatedly. */
  free(): void;
}
