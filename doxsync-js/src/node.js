import { readFile } from "node:fs/promises";
import { initialize } from "./runtime.js";

export { Producer, Consumer, supportedProtocols } from "./runtime.js";

export function init(source) {
  return initialize(() => {
    if (source === undefined) {
      return readFile(new URL("./wasm/doxsync_bg.wasm", import.meta.url));
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
