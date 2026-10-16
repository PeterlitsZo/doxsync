import initWasm, { MessageInspector as WasmInspector } from "./wasm/doxsync_inspector.js";
import { Decimal, type SyncValue, type InitSource } from "@doxsync/core";
import type { InspectedMessage } from "./types.js";
export type { InspectedMessage, MessageAction, MessagePath, WireField } from "./types.js";

let initialization: Promise<void> | undefined;
let ready = false;
/** Initialize the separate diagnostic WASM. Node callers supply WASM bytes. */
export function init(source?: InitSource): Promise<void> {
  return initialization ??= Promise.resolve().then(() => initWasm({ module_or_path:
    source ?? new URL(/* @vite-ignore */ "./wasm/doxsync_inspector_bg.wasm", import.meta.url),
  })).then(() => { ready = true; }).catch((error) => {
    initialization = undefined;
    throw error;
  });
}

type EncodedValue = { kind: string; value: unknown };
const OutputDecimal = Decimal.clone({ precision: 29, minE: -100, maxE: 100 });
// All values are explicitly tagged, so user map keys cannot collide with markers.
function value(encoded: EncodedValue): SyncValue {
  switch (encoded.kind) {
    case "null": return null;
    case "string": return encoded.value as string;
    case "boolean": return encoded.value as boolean;
    case "number": return Number(encoded.value === "inf" ? "Infinity" : encoded.value === "-inf" ? "-Infinity" : encoded.value);
    case "bigint": return BigInt(encoded.value as string);
    case "decimal": return new OutputDecimal(encoded.value as string);
    case "bytes": return Uint8Array.from(encoded.value as number[]);
    case "array": return (encoded.value as EncodedValue[]).map(value);
    case "map": return Object.fromEntries(Object.entries(encoded.value as Record<string, EncodedValue>).map(([key, item]) => [key, value(item)]));
    default: throw new Error(`Unknown inspector value kind: ${encoded.kind}`);
  }
}

/** Independent stateful disassembler; inspect each message once in stream order. */
export class MessageInspector {
  #handle: WasmInspector | undefined;
  constructor() {
    if (!ready) throw new Error("Call and await inspector init() first");
    this.#handle = new WasmInspector();
  }
  inspect(bytes: Uint8Array): InspectedMessage {
    if (!this.#handle) throw new Error("MessageInspector has been freed");
    const result = JSON.parse(this.#handle.inspect(bytes));
    for (const action of result.actions) {
      if ("value" in action) action.value = value(action.value);
      if ("edits" in action) for (const edit of action.edits) edit.values = edit.values.map(value);
    }
    return result;
  }
  free(): void { this.#handle?.free(); this.#handle = undefined; }
}
