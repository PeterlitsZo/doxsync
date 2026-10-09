import type { InitSource } from "./types.js";
export type { SyncValue, InitSource, DoxsyncError } from "./types.js";
export { Decimal } from "decimal.js";

import { readFile } from "node:fs/promises";
import { initialize } from "./runtime.js";

export { Producer, Consumer, supportedProtocols } from "./runtime.js";

/**
 * Initialize once before constructing Producer or Consumer or calling supportedProtocols.
 * Defaults to the packaged WASM. Concurrent calls share the first call's source
 * and promise; failures can be retried. Later successful calls are no-ops.
 * Strings are URLs, not filesystem paths; Node also accepts file: URLs.
 */
export function init(source?: InitSource): Promise<void> {
  return initialize(() => {
    if (source === undefined) {
      return readFile(new URL(/* @vite-ignore */ "./wasm/doxsync_bg.wasm", import.meta.url));
    }
    if (source instanceof URL && source.protocol === "file:") {
      return readFile(source);
    }
    if (typeof source === "string" && source.startsWith("file:")) {
      return readFile(new URL(source));
    }
    return source;
  });
}
