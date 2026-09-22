import initWasm, {
  Producer as WasmProducer,
  Consumer as WasmConsumer,
} from "./wasm/doxsync.js";

let initialization;
let initialized = false;

export function initialize(loadSource) {
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
    throw new Error("Call and await init() before creating a doxsync object");
  }
}

export class Producer {
  #handle;

  constructor(value) {
    requireInitialized();
    this.#handle = new WasmProducer(value);
  }

  #live() {
    if (!this.#handle) throw new Error("Producer has been freed");
    return this.#handle;
  }

  replace(value) {
    this.#live().replace(value);
  }

  produceDiff() {
    return this.#live().produceDiff();
  }

  free() {
    if (this.#handle) {
      this.#handle.free();
      this.#handle = undefined;
    }
  }
}

export class Consumer {
  #handle;

  constructor() {
    requireInitialized();
    this.#handle = new WasmConsumer();
  }

  #live() {
    if (!this.#handle) throw new Error("Consumer has been freed");
    return this.#handle;
  }

  consumeDiff(bytes) {
    this.#live().consumeDiff(bytes);
  }

  document() {
    return this.#live().document();
  }

  free() {
    if (this.#handle) {
      this.#handle.free();
      this.#handle = undefined;
    }
  }
}
