import type { ActionView } from "./inspect";

export interface MessageView {
  readonly id: number;
  readonly summary: string;
  readonly size: number;
  readonly hex: string;
  readonly protocol: number;
  readonly actions: readonly ActionView[];
  readonly wire: string;
  readonly metadataCount: number;
  readonly delay: number;
  readonly sentAt: number;
  readonly dueAt: number;
  readonly deliveredAt?: number;
  readonly status: "queued" | "delivered" | "failed";
}

export interface Snapshot {
  readonly generation: number;
  readonly running: boolean;
  readonly error?: string;
  readonly interval: number;
  readonly delay: number;
  readonly sent: number;
  readonly totalBytes: number;
  readonly unchanged: number;
  readonly pending: number;
  readonly producerText: string;
  readonly previousProducerText: string;
  readonly consumerText: string;
  readonly previousConsumerText: string;
  readonly consumerVersion: number;
  readonly producerChange: string;
  readonly consumerChange: string;
  readonly history: readonly MessageView[];
}
