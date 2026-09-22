import { initialize } from "./runtime.js";

export { Producer, Consumer } from "./runtime.js";

export function init(source) {
  return initialize(() => source === undefined
    ? new URL("./wasm/doxsync_bg.wasm", import.meta.url)
    : source);
}
