const $ = (id) => document.getElementById(id);
const controls = ["toggle", "step", "reset", "interval", "delay"].map($);
const historyLimit = 60;
let bindings;
let state;
let mutationTimer;
let deliveryTimer;
let displayTimer;
let loading = false;

const initialDocument = () => ({
  title: "doxsync playground",
  visitors: 12n,
  online: true,
  settings: { theme: "light", volume: 60 },
  tags: ["rust", "wasm"],
});
const pick = (items) => items[Math.floor(Math.random() * items.length)];
const number = (min, max) => min + Math.floor(Math.random() * (max - min + 1));
const messageId = (id) => `#${String(id).padStart(4, "0")}`;
const bytesLabel = (size) => size < 1024 ? `${size} B` : `${(size / 1024).toFixed(1)} KiB`;
const delayValue = () => {
  const value = Number($("delay").value);
  return Math.round(Math.min(10000, Math.max(0, Number.isFinite(value) ? value : 1000)));
};

// Canonical display order lets the two documents be compared directly. The n
// suffix preserves bigint's type instead of presenting it as a JSON string.
function format(value, depth = 0) {
  if (typeof value === "bigint") return `${value}n`;
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  const array = Array.isArray(value);
  const entries = array ? value : Object.keys(value).sort();
  if (!entries.length) return array ? "[]" : "{}";
  const indent = "  ".repeat(depth + 1);
  const lines = entries.map((item) => indent + (array
    ? format(item, depth + 1)
    : `${JSON.stringify(item)}: ${format(value[item], depth + 1)}`));
  return `${array ? "[" : "{"}\n${lines.join(",\n")}\n${"  ".repeat(depth)}${array ? "]" : "}"}`;
}

function renderDocument(id, text, previous = "") {
  const oldLines = new Set(previous.split("\n"));
  const fragment = document.createDocumentFragment();
  for (const [index, line] of text.split("\n").entries()) {
    const row = document.createElement("span");
    row.className = `code-line${previous && !oldLines.has(line) ? " changed" : ""}`;
    row.dataset.line = index + 1;
    const tokens = /"(?:\\.|[^"\\])*"|\b(?:true|false|null)\b|-?\b\d+(?:\.\d+)?n?\b/g;
    let cursor = 0;
    for (const token of line.matchAll(tokens)) {
      row.append(document.createTextNode(line.slice(cursor, token.index)));
      const span = document.createElement("span");
      const quoted = token[0].startsWith('"');
      const key = quoted && line.slice(token.index + token[0].length).trimStart().startsWith(":");
      span.className = key ? "token-key" : quoted ? "token-string"
        : /^(true|false|null)$/.test(token[0]) ? "token-literal" : "token-number";
      span.textContent = token[0];
      row.append(span);
      cursor = token.index + token[0].length;
    }
    row.append(document.createTextNode(line.slice(cursor) + "\n"));
    fragment.append(row);
  }
  $(id).replaceChildren(fragment);
}

function hex(bytes) {
  const lines = [];
  for (let offset = 0; offset < bytes.length; offset += 16) {
    const chunk = bytes.subarray(offset, offset + 16);
    const values = Array.from(chunk, (byte) => byte.toString(16).padStart(2, "0")).join(" ");
    lines.push(`${offset.toString(16).padStart(4, "0")}   ${values}`);
  }
  return lines.join("\n");
}

function statusText(message, now = performance.now()) {
  if (message.status === "failed") return "Delivery failed";
  if (message.status === "delivered") return "Delivered";
  return `In flight · ${Math.max(0, message.dueAt - now).toFixed(0)} ms`;
}

function selectMessage(message) {
  state.selected = message;
  for (const item of state.history) {
    item.row.setAttribute("aria-pressed", String(item === message));
  }
  $("message-title").textContent = `MESSAGE ${messageId(message.id)}`;
  $("message-description").textContent = message.summary;
  $("detail-size").textContent = `${message.bytes.length} B`;
  $("detail-delay").textContent = `${message.delay} ms`;
  $("message-bytes").textContent = hex(message.bytes);
  updateTransport();
}

function updateTransport() {
  if (!state) return;
  const now = performance.now();
  for (const message of state.queue) message.statusNode.textContent = statusText(message, now);
  $("metrics").textContent = `Sent ${state.sent} · ${bytesLabel(state.totalBytes)} · In flight ${state.queue.length} · Unchanged ${state.unchanged}`;
  if (!state.failed) {
    $("sync-state").dataset.state = state.queue.length ? "pending" : "synced";
    $("sync-state").textContent = state.queue.length
      ? `Syncing · ${state.queue.length} in flight`
      : "Documents are in sync";
  }
  if (state.selected) {
    const message = state.selected;
    $("detail-status").textContent = message.status === "queued" ? "In flight" : statusText(message, now);
    $("detail-elapsed").textContent = `${Math.round((message.deliveredAt ?? now) - message.sentAt)} ms`;
  }
}

function pruneHistory() {
  const delivered = state.history.filter((message) => message.status === "delivered");
  for (const message of delivered.slice(historyLimit)) {
    message.row.closest("li").remove();
    state.history.splice(state.history.indexOf(message), 1);
    if (state.selected === message) selectMessage(state.history[0]);
  }
}

function scheduleDelivery() {
  clearTimeout(deliveryTimer);
  if (!state.queue.length || state.failed) return;
  deliveryTimer = setTimeout(deliver, Math.max(0, state.queue[0].dueAt - performance.now()));
}

function deliver() {
  try {
    while (state.queue.length && state.queue[0].dueAt <= performance.now()) {
      const message = state.queue[0];
      state.consumer.consumeDiff(message.bytes);
      const received = format(state.consumer.document());
      if (received !== message.expected) throw new Error(`Message ${messageId(message.id)} produced a document that differs from the document at send time`);
      renderDocument("consumer-document", received, state.consumerText);
      state.consumerText = received;
      $("consumer-version").textContent = messageId(message.id);
      $("consumer-change").textContent = message.summary;
      message.deliveredAt = performance.now();
      message.status = "delivered";
      message.row.dataset.status = "delivered";
      message.statusNode.textContent = "Delivered";
      state.queue.shift();
    }
    pruneHistory();
    updateTransport();
    scheduleDelivery();
  } catch (error) {
    if (state.queue[0]) {
      state.queue[0].status = "failed";
      state.queue[0].row.dataset.status = "failed";
      state.queue[0].statusNode.textContent = "Delivery failed";
    }
    fail(error);
  }
}

function send(summary) {
  const bytes = state.producer.produceDiff();
  if (bytes === undefined) {
    state.unchanged++;
    $("producer-change").textContent = "Document unchanged; no message generated";
    updateTransport();
    return;
  }
  const now = performance.now();
  const delay = delayValue();
  const message = {
    id: ++state.sent, bytes, summary, delay, sentAt: now,
    // A shorter delay must never let a later stateful message overtake an older
    // one. A single FIFO timer applies each complete message exactly once.
    dueAt: Math.max(now + delay, state.queue.at(-1)?.dueAt ?? now),
    status: "queued", expected: state.producerText,
  };
  const item = document.createElement("li");
  const row = document.createElement("button");
  row.type = "button";
  row.className = "message-row";
  row.dataset.status = "queued";
  row.dataset.messageId = message.id;
  row.setAttribute("aria-pressed", "false");
  const id = document.createElement("span");
  id.className = "message-id";
  id.textContent = messageId(message.id);
  const description = document.createElement("span");
  description.className = "message-summary";
  description.textContent = summary;
  description.title = summary;
  const facts = document.createElement("span");
  const size = document.createElement("span");
  size.className = "message-size";
  size.textContent = `${bytes.length} B`;
  const status = document.createElement("span");
  status.className = "message-state";
  status.textContent = statusText(message, now);
  facts.append(size, status);
  row.append(id, description, facts);
  row.addEventListener("click", () => {
    $("follow").checked = false;
    selectMessage(message);
  });
  item.append(row);
  message.row = row;
  message.statusNode = status;
  state.queue.push(message);
  state.history.unshift(message);
  state.totalBytes += bytes.length;
  $("message-list").prepend(item);
  $("producer-version").textContent = messageId(message.id);
  $("producer-change").textContent = summary;
  if ($("follow").checked || !state.selected) {
    selectMessage(message);
    $("message-list").scrollTop = 0;
  }
  updateTransport();
  scheduleDelivery();
}

const mutations = [
  (doc) => { const delta = BigInt(number(1, 5)); doc.visitors += delta; return `visitors increased by ${delta}`; },
  (doc) => { doc.online = !doc.online; return `online → ${doc.online}`; },
  (doc) => { doc.settings.theme = pick(["light", "dark", "system"].filter((value) => value !== doc.settings.theme)); return `settings.theme → ${doc.settings.theme}`; },
  (doc) => { doc.settings.volume = (doc.settings.volume + number(1, 4) * 10) % 110; return `settings.volume → ${doc.settings.volume}`; },
  (doc) => { doc.title = pick(["doxsync playground", "Hello from Rust", "Hello, WebAssembly", "Small changes, in sync"].filter((value) => value !== doc.title)); return `title → ${doc.title}`; },
  (doc) => {
    if (doc.tags.length >= 5 || (doc.tags.length > 1 && Math.random() < .5)) {
      const [removed] = doc.tags.splice(number(0, doc.tags.length - 1), 1);
      return `Removed ${removed} from tags`;
    }
    const tag = pick(["rust", "wasm", "browser", "sync", "binary", "js"].filter((value) => !doc.tags.includes(value)));
    doc.tags.push(tag);
    return `Added ${tag} to tags`;
  },
  (doc) => {
    if (Object.hasOwn(doc, "note")) { delete doc.note; return "Removed the note field"; }
    doc.note = "This message is on its way";
    return "Added the note field";
  },
  () => "Document unchanged",
];

function mutate() {
  if (!state || state.failed) return;
  try {
    // Producer owns a copy. Keep this display model in step with successfully
    // accepted replacements; no JS implementation of the diff algorithm.
    const next = structuredClone(state.document);
    const summary = pick(mutations)(next);
    state.producer.replace(next);
    state.document = next;
    const text = format(next);
    if (text !== state.producerText) renderDocument("producer-document", text, state.producerText);
    state.producerText = text;
    send(summary);
  } catch (error) { fail(error); }
}

function scheduleMutation() {
  clearTimeout(mutationTimer);
  if (!state.running || state.failed) return;
  mutationTimer = setTimeout(() => { mutate(); scheduleMutation(); }, Number($("interval").value));
}

function stop() {
  clearTimeout(mutationTimer);
  clearTimeout(deliveryTimer);
  clearInterval(displayTimer);
}

function dispose() {
  stop();
  state?.producer.free();
  state?.consumer.free();
  state = undefined;
}

function fail(error) {
  stop();
  if (state) { state.failed = true; state.running = false; }
  controls.forEach((control) => { control.disabled = control.id !== "reset"; });
  $("toggle").textContent = "Auto updates stopped";
  $("reset").textContent = bindings ? "Reset" : "Retry loading";
  $("error").hidden = false;
  $("error").textContent = `${error.kind ? `[${error.kind}] ` : ""}${error.message ?? error}\n${state ? "The simulation has stopped. Reset to try again." : "Run npm run build first, then open this page through an HTTP server."}`;
  $("sync-state").dataset.state = "error";
  $("sync-state").textContent = "Simulation stopped";
  updateTransport();
}

function reset() {
  dispose();
  const document = initialDocument();
  state = {
    document, producer: new bindings.Producer(document, [1]), consumer: new bindings.Consumer(),
    producerText: format(document), consumerText: "", queue: [], history: [],
    selected: undefined, sent: 0, totalBytes: 0, unchanged: 0,
    running: true, failed: false,
  };
  controls.forEach((control) => { control.disabled = false; });
  $("error").hidden = true;
  $("toggle").textContent = "Pause auto updates";
  $("reset").textContent = "Reset";
  $("follow").checked = true;
  $("message-list").replaceChildren();
  renderDocument("producer-document", state.producerText);
  renderDocument("consumer-document", "Waiting for the first snapshot…");
  $("consumer-version").textContent = "—";
  $("consumer-change").textContent = "Updates when a message arrives";
  send("Initial snapshot");
  scheduleMutation();
  displayTimer = setInterval(updateTransport, 100);
}

async function boot() {
  if (loading) return;
  loading = true;
  controls.forEach((control) => { control.disabled = true; });
  try {
    bindings = await import("../dist/index.js");
    await bindings.init();
    reset();
  } catch (error) { fail(error); }
  finally { loading = false; }
}

$("toggle").addEventListener("click", () => {
  state.running = !state.running;
  $("toggle").textContent = state.running ? "Pause auto updates" : "Resume auto updates";
  scheduleMutation();
});
$("step").addEventListener("click", mutate);
$("reset").addEventListener("click", boot);
$("interval").addEventListener("change", scheduleMutation);
$("delay").addEventListener("change", () => {
  $("delay").value = delayValue();
});
$("follow").addEventListener("change", () => {
  if ($("follow").checked && state?.history.length) {
    selectMessage(state.history[0]);
    $("message-list").scrollTop = 0;
  }
});
window.addEventListener("pagehide", dispose);
window.addEventListener("pageshow", (event) => { if (event.persisted) boot(); });
boot();
