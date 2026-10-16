import { Decimal, type SyncValue } from "@doxsync/core";

export const messageId = (id: number) => `#${String(id).padStart(4, "0")}`;
export const bytesLabel = (size: number) => size < 1024 ? `${size} B` : `${(size / 1024).toFixed(1)} KiB`;

export function format(value: SyncValue | undefined, depth = 0): string {
  if (typeof value === "bigint") return `${value}n`;
  if (Decimal.isDecimal(value)) return `Decimal("${value.toFixed()}")`;
  if (value instanceof Uint8Array) return `Uint8Array([${Array.from(value).join(", ")}])`;
  if (value === null || typeof value !== "object") return JSON.stringify(value) ?? "undefined";
  const array = Array.isArray(value);
  const entries = array ? value : Object.keys(value).sort();
  if (!entries.length) return array ? "[]" : "{}";
  const indent = "  ".repeat(depth + 1);
  const lines = entries.map((item) => indent + (array
    ? format(item, depth + 1)
    : `${JSON.stringify(item)}: ${format((value as { [key: string]: SyncValue })[String(item)], depth + 1)}`));
  return `${array ? "[" : "{"}\n${lines.join(",\n")}\n${"  ".repeat(depth)}${array ? "]" : "}"}`;
}

export function hex(bytes: Uint8Array) {
  const lines = [];
  for (let offset = 0; offset < bytes.length; offset += 16) {
    const chunk = bytes.subarray(offset, offset + 16);
    const values = Array.from(chunk, (byte) => byte.toString(16).padStart(2, "0")).join(" ");
    lines.push(`${offset.toString(16).padStart(4, "0")}   ${values}`);
  }
  return lines.join("\n");
}

export function errorText(error: unknown): string {
  if (error && typeof error === "object" && "message" in error) {
    return `${"kind" in error ? `[${error.kind}] ` : ""}${error.message}`;
  }
  return String(error);
}
