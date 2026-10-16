import { init, Producer, Consumer, supportedProtocols } from "../dist/index.js";

const output = document.getElementById("output");
let producer;
let consumer;

try {
  await init();
  producer = new Producer({ title: "Hello, browser", count: 1n }, supportedProtocols());
  consumer = new Consumer();

  const snapshot = producer.produceDiff();
  consumer.consumeDiff(snapshot);
  const initial = consumer.document();

  producer.replace({ title: "Hello, browser", count: 2n });
  const update = producer.produceDiff();
  consumer.consumeDiff(update);
  const changed = consumer.document();

  output.textContent = [
    `Snapshot: ${snapshot.length} bytes → count: ${initial.count}n`,
    `Update: ${update.length} bytes → count: ${changed.count}n`,
    `Unchanged: ${producer.produceDiff() === undefined ? "no message" : "unexpected message"}`,
  ].join("\n");
} catch (error) {
  output.setAttribute("role", "alert");
  output.textContent = `${error.kind ? `[${error.kind}] ` : ""}${error.message ?? error}`;
} finally {
  producer?.free();
  consumer?.free();
}
