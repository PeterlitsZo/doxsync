import { memo, type ReactNode } from "react";
import { messageId } from "../simulation/format";

function tokenize(line: string) {
  const nodes: ReactNode[] = [];
  const tokens = /"(?:\\.|[^"\\])*"|\b(?:true|false|null)\b|-?\b\d+(?:\.\d+)?n?\b/g;
  let cursor = 0;
  for (const token of line.matchAll(tokens)) {
    nodes.push(line.slice(cursor, token.index));
    const quoted = token[0].startsWith('"');
    const key = quoted && line.slice(token.index + token[0].length).trimStart().startsWith(":");
    const kind = key ? "key" : quoted ? "string" : /^(true|false|null)$/.test(token[0]) ? "literal" : "number";
    nodes.push(<span key={token.index} className={`token-${kind}`}>{token[0]}</span>);
    cursor = token.index + token[0].length;
  }
  nodes.push(line.slice(cursor) + "\n");
  return nodes;
}

export const DocumentPanel = memo(function DocumentPanel({ role, text, previous, version, change }: {
  role: "Producer" | "Consumer"; text: string; previous: string; version: number; change: string;
}) {
  const oldLines = new Set(previous.split("\n"));
  return <article className={`document-panel ${role.toLowerCase()}-panel`}>
    <header className="panel-header">
      <div><span className="role-dot" /><h2>{role}</h2><span className="subtle">{role === "Producer" ? "Source document" : "Received document"}</span></div>
      <span className="version">{version ? messageId(version) : "—"}</span>
    </header>
    <pre className="document-code" tabIndex={0} aria-label={`Current ${role.toLowerCase()} document`}><code>
      {text.split("\n").map((line, index) => <span key={index} className={`code-line${previous && !oldLines.has(line) ? " changed" : ""}`} data-line={index + 1}>{tokenize(line)}</span>)}
    </code></pre>
    <footer className="document-footer">{change}</footer>
  </article>;
});
