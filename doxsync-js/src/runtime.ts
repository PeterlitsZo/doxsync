import type { InitSource, SyncValue } from "./types.js";
import { decimalFromString, decimalToString } from "./decimal.js";
import initWasm, {
  Producer as WasmProducer,
  Consumer as WasmConsumer,
  supportedProtocols as wasmSupportedProtocols,
} from "./wasm/doxsync.js";

let initialization: Promise<void> | undefined;
let initialized = false;

export function initialize(loadSource: () => InitSource | Promise<InitSource>): Promise<void> {
  if (!initialization) {
    initialization = Promise.resolve()
      .then(loadSource)
      .then((source) => initWasm({ module_or_path: source }))
      .then(() => { initialized = true; })
      .catch((error) => {
        initialization = undefined;
        throw error;
      });
  }
  return initialization;
}

function requireInitialized() {
  if (!initialized) {
    throw new Error("Call and await init() before using the doxsync API");
  }
}

/**
 * Returns all protocol versions supported by this build, currently [1, 2].
 * Requires await init(). Each call returns an independent ordinary array.
 * Consumers can advertise this list to producers for protocol negotiation.
 */
export function supportedProtocols(): number[] {
  requireInitialized();
  return wasmSupportedProtocols();
}

export class Producer {
  #handle: WasmProducer | undefined;

  /**
   * Copies the value into WASM and selects the highest common protocol version.
   * protocols lists the consumer's supported versions as u32 integers.
   * Versions 1 and 2 are supported. Missing, invalid, empty, or incompatible
   * lists throw InvalidData errors. Order and duplicates do not matter.
   * Protocol 1 transmits Decimal values as strings; protocol 2 preserves the type.
   */
  constructor(value: SyncValue, protocols: readonly number[]) {
    requireInitialized();
    this.#handle = new WasmProducer(value, protocols, decimalToString);
  }

  #live() {
    if (!this.#handle) throw new Error("Producer has been freed");
    return this.#handle;
  }

  /** Replaces the document after validating and copying the entire value. */
  replace(value: SyncValue): void {
    this.#live().replace(value);
  }

  /**
   * Returns a snapshot with protocol metadata on the first call, then changes.
   * Returns undefined if unchanged. Retain each message until delivered in order.
   */
  produceDiff(): Uint8Array | undefined {
    return this.#live().produceDiff();
  }

  /** Release WASM resources. Safe to call repeatedly. */
  free(): void {
    if (this.#handle) {
      this.#handle.free();
      this.#handle = undefined;
    }
  }
}

export class Consumer {
  #handle: WasmConsumer | undefined;

  constructor() {
    requireInitialized();
    this.#handle = new WasmConsumer();
  }

  #live() {
    if (!this.#handle) throw new Error("Consumer has been freed");
    return this.#handle;
  }

  /**
   * Applies one complete message. The first message must declare protocol 1 or 2.
   * Failures leave the document, pools, and protocol state intact.
   */
  consumeDiff(bytes: Uint8Array): void {
    this.#live().consumeDiff(bytes);
  }

  /** Returns an independent copy, or undefined before the first snapshot.
   * Protocol 2 decimals become decimal.js instances; their original scale is lost.
   */
  document(): SyncValue | undefined {
    return this.#live().document(decimalFromString);
  }

  /** Release WASM resources. Safe to call repeatedly. */
  free(): void {
    if (this.#handle) {
      this.#handle.free();
      this.#handle = undefined;
    }
  }
}
