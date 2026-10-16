import type { SyncValue } from "@doxsync/core";

/** String keys and numeric array indexes remain distinct. Empty paths mean root. */
export type MessagePath = (string | number)[];

export type MessageAction =
  | { type: "snapshot"; value: SyncValue }
  | { type: "add" | "replace"; path: MessagePath; value: SyncValue }
  | { type: "delete"; path: MessagePath }
  | { type: "copy"; path: MessagePath; from: MessagePath }
  | { type: "stringAppend" | "stringPrepend"; path: MessagePath; value: string }
  | { type: "arrayPatch"; path: MessagePath; edits: { start: number; deleteCount: number; values: SyncValue[] }[] };

/** Exact byte span; compound children partition their parent's span. */
export interface WireField {
  label: string;
  offset: number;
  length: number;
  hex: string;
  explanation: string;
  children: WireField[];
  annotation: string;
  prefix: number;
}
export interface InspectedMessage {
  protocol: number;
  metadataCount: number;
  actions: MessageAction[];
  wire: WireField[];
}
