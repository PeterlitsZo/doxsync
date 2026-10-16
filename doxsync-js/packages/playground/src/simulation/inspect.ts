import type { MessageAction, MessagePath } from "@doxsync/inspector";
import { format } from "./format";

const pathText = (path: MessagePath) => "$" + path.map((segment) => typeof segment === "number"
  ? `[${segment}]` : /^[A-Za-z_$][\w$]*$/.test(segment) ? `.${segment}` : `[${JSON.stringify(segment)}]`).join("");

export interface ActionView {
  readonly title: string;
  readonly content: string;
}

export function describeAction(action: MessageAction): ActionView {
  switch (action.type) {
    case "snapshot":
      return { title: "SNAPSHOT · $", content: format(action.value) };
    case "add":
    case "replace":
      return { title: `${action.type.toUpperCase()} · ${pathText(action.path)}`, content: format(action.value) };
    case "delete":
      return { title: `DELETE · ${pathText(action.path)}`, content: "Remove the value at this path." };
    case "copy":
      return { title: `COPY · ${pathText(action.path)}`, content: `Copy the existing value from ${pathText(action.from)}.` };
    case "stringAppend":
    case "stringPrepend":
      return { title: `${action.type === "stringAppend" ? "APPEND STRING" : "PREPEND STRING"} · ${pathText(action.path)}`, content: format(action.value) };
    case "arrayPatch":
      return {
        title: `ARRAY PATCH · ${pathText(action.path)}`,
        content: action.edits.map((edit) => `At original index ${edit.start}: delete ${edit.deleteCount}, insert ${edit.values.length}${edit.values.length ? `\n${format(edit.values)}` : ""}`).join("\n\n"),
      };
  }
}

/** Render the parser's grouping hints without decoding any bytes in the UI. */
export function describeWire(fields: readonly import("@doxsync/inspector").WireField[]): string {
  const lines: string[] = [];
  function visit(field: import("@doxsync/inspector").WireField, depth: number) {
    const prefixLength = field.children.slice(0, field.prefix).reduce((sum, child) => sum + child.length, 0);
    const bytes = field.children.length ? field.hex.split(" ").slice(0, prefixLength).join(" ") : field.hex;
    lines.push(`${"  ".repeat(depth)}${bytes}${bytes ? " " : ""}# ${field.annotation}`);
    field.children.slice(field.prefix).forEach((child) => visit(child, depth + 1));
  }
  fields.forEach((field) => visit(field, field.label.startsWith("Metadata ") && field.children.length || field.label.startsWith("Action ") && field.children.length ? 1 : 0));
  return lines.join("\n");
}
