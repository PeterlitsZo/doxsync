import { Consumer, Producer, supportedProtocols } from "@doxsync/core";
import { MessageInspector } from "@doxsync/inspector";
import { initialDocument, mutateDocument } from "./document";
import { errorText, format, hex, messageId } from "./format";
import { describeAction, describeWire } from "./inspect";
import type { MessageView, Snapshot } from "./types";

interface PendingMessage {
  view: MessageView;
  bytes: Uint8Array;
  expected: string;
}

export class SimulationController {
  private producer?: Producer;
  private consumer?: Consumer;
  private inspector?: MessageInspector;
  private document = initialDocument();
  private queue: PendingMessage[] = [];
  private listeners = new Set<() => void>();
  private mutationTimer?: ReturnType<typeof setTimeout>;
  private deliveryTimer?: ReturnType<typeof setTimeout>;
  private disposed = false;
  private snapshot!: Snapshot;

  constructor() { this.reset(); }

  getSnapshot = (): Snapshot => this.snapshot;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => { this.listeners.delete(listener); };
  };

  private publish(patch: Partial<Snapshot>) {
    this.snapshot = Object.freeze({ ...this.snapshot, ...patch });
    this.listeners.forEach((listener) => listener());
  }

  private stop() {
    clearTimeout(this.mutationTimer);
    clearTimeout(this.deliveryTimer);
  }

  private release() {
    this.stop();
    this.producer?.free();
    this.consumer?.free();
    this.inspector?.free();
    this.producer = undefined;
    this.consumer = undefined;
    this.inspector = undefined;
    this.queue = [];
  }

  dispose = () => {
    if (this.disposed) return;
    this.disposed = true;
    this.release();
    this.listeners.clear();
  };

  private fail(error: unknown) {
    this.stop();
    this.publish({ error: errorText(error), running: false });
  }

  reset = () => {
    if (this.disposed) return;
    this.release();
    this.document = initialDocument();
    this.snapshot = Object.freeze({
      generation: (this.snapshot?.generation ?? 0) + 1,
      interval: this.snapshot?.interval ?? 1500,
      delay: this.snapshot?.delay ?? 1000,
      running: true, sent: 0, totalBytes: 0, unchanged: 0, pending: 0,
      producerText: format(this.document), previousProducerText: "",
      consumerText: "Waiting for the first snapshot…", previousConsumerText: "",
      consumerVersion: 0, producerChange: "Preparing the initial document",
      consumerChange: "Updates when a message arrives", history: Object.freeze([]),
    });
    try {
      this.producer = new Producer(this.document, supportedProtocols());
      this.consumer = new Consumer();
      this.inspector = new MessageInspector();
      this.send("Initial snapshot");
      this.scheduleMutation();
    } catch (error) {
      this.release();
      this.fail(error);
    }
  };

  toggle = () => {
    if (this.disposed || this.snapshot.error) return;
    this.publish({ running: !this.snapshot.running });
    this.scheduleMutation();
  };

  setInterval = (interval: number) => {
    if (this.disposed || this.snapshot.error || ![500, 1000, 1500, 3000, 10000, 30000].includes(interval)) return;
    this.publish({ interval });
    this.scheduleMutation();
  };

  setDelay = (delay: number) => {
    if (this.disposed || this.snapshot.error) return;
    this.publish({ delay: Math.round(Math.max(0, Math.min(10000, Number.isFinite(delay) ? delay : 1000))) });
  };

  step = () => {
    if (this.disposed || this.snapshot.error) return;
    try {
      const next = mutateDocument(this.document);
      this.producer!.replace(next.document);
      this.document = next.document;
      const text = format(this.document);
      if (text !== this.snapshot.producerText) {
        this.publish({ previousProducerText: this.snapshot.producerText, producerText: text });
      }
      this.send(next.summary);
    } catch (error) { this.fail(error); }
  };

  private scheduleMutation() {
    clearTimeout(this.mutationTimer);
    if (this.disposed || !this.snapshot.running || this.snapshot.error) return;
    this.mutationTimer = setTimeout(() => {
      this.step();
      this.scheduleMutation();
    }, this.snapshot.interval);
  }

  private send(summary: string) {
    const bytes = this.producer!.produceDiff();
    if (bytes === undefined) {
      this.publish({ unchanged: this.snapshot.unchanged + 1, producerChange: "Document unchanged; no message generated" });
      return;
    }
    // Inspect in send order; selecting history never advances decoder state.
    const decoded = this.inspector!.inspect(bytes);
    const actions = Object.freeze(decoded.actions.map((action) => Object.freeze(describeAction(action))));
    const now = performance.now();
    const view: MessageView = Object.freeze({
      id: this.snapshot.sent + 1, summary, size: bytes.length, hex: hex(bytes),
      protocol: decoded.protocol,
      actions,
      wire: describeWire(decoded.wire),
      metadataCount: decoded.metadataCount,
      delay: this.snapshot.delay, sentAt: now,
      // Stateful messages must not overtake one another when delay decreases.
      dueAt: Math.max(now + this.snapshot.delay, this.queue.at(-1)?.view.dueAt ?? now),
      status: "queued",
    });
    this.queue.push({ view, bytes, expected: this.snapshot.producerText });
    this.publish({
      sent: view.id, totalBytes: this.snapshot.totalBytes + bytes.length,
      pending: this.queue.length, producerChange: summary,
      history: Object.freeze([view, ...this.snapshot.history]),
    });
    this.scheduleDelivery();
  }

  private scheduleDelivery() {
    clearTimeout(this.deliveryTimer);
    if (this.disposed || this.snapshot.error || !this.queue.length) return;
    this.deliveryTimer = setTimeout(this.deliver, Math.max(0, this.queue[0].view.dueAt - performance.now()));
  }

  private deliver = () => {
    if (this.disposed || this.snapshot.error) return;
    try {
      while (this.queue.length && this.queue[0].view.dueAt <= performance.now()) {
        const message = this.queue[0];
        this.consumer!.consumeDiff(message.bytes);
        const text = format(this.consumer!.document());
        if (text !== message.expected) throw new Error(`Message ${messageId(message.view.id)} produced a document that differs from the document at send time`);
        const delivered: MessageView = Object.freeze({ ...message.view, status: "delivered", deliveredAt: performance.now() });
        this.queue.shift();
        let deliveredCount = 0;
        const history = this.snapshot.history.map((item) => item.id === delivered.id ? delivered : item)
          .filter((item) => item.status !== "delivered" || ++deliveredCount <= 60);
        this.publish({
          history: Object.freeze(history), pending: this.queue.length,
          previousConsumerText: this.snapshot.consumerVersion ? this.snapshot.consumerText : "",
          consumerText: text, consumerVersion: delivered.id, consumerChange: delivered.summary,
        });
      }
      this.scheduleDelivery();
    } catch (error) {
      const failedId = this.queue[0]?.view.id;
      this.publish({ history: Object.freeze(this.snapshot.history.map((item) => item.id === failedId
        ? Object.freeze({ ...item, status: "failed" as const }) : item)) });
      this.fail(error);
    }
  };
}
