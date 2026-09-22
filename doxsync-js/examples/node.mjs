import { init, Producer, Consumer } from "doxsync";

await init();
const producer = new Producer({ count: 1n, data: new Uint8Array([1, 2, 3]) }, [1]);
const consumer = new Consumer();

try {
  consumer.consumeDiff(producer.produceDiff());
  console.log("Snapshot:", consumer.document());

  producer.replace({ count: 2n, data: new Uint8Array([1, 2, 3]) });
  const message = producer.produceDiff();
  if (message !== undefined) consumer.consumeDiff(message);
  console.log("Updated:", consumer.document());
  console.log("Unchanged:", producer.produceDiff());
} finally {
  producer.free();
  consumer.free();
}
