import { useState } from "react";
import type { MessageView } from "../simulation/types";
import { messageId } from "../simulation/format";
import { statusText } from "./MessageHistory";

export function MessageDetails({ message, follow, setFollow, now }: {
  message?: MessageView; follow: boolean; setFollow: (value: boolean) => void; now: number;
}) {
  const [view, setView] = useState<"wire" | "actions" | "raw">("wire");
  const facts = [
    ["Size", message ? `${message.size} B` : "—"],
    ["Status", message ? message.status === "queued" ? "In flight" : statusText(message, now) : "—"],
    ["Set delay", message ? `${message.delay} ms` : "—"],
    ["Elapsed", message ? `${Math.round(Math.max(0, (message.deliveredAt ?? now) - message.sentAt))} ms` : "—"],
  ];
  return <section className="message-detail" aria-label="Selected message details">
    <div className="detail-heading">
      <h3>{message ? `Message ${messageId(message.id)}` : "Waiting for a message"}</h3>
      <label className="follow"><input type="checkbox" checked={follow} onChange={(event) => setFollow(event.target.checked)} />Follow latest</label>
    </div>
    <p className="message-description">{message?.summary ?? "Select a message to inspect the actual bytes sent."}</p>
    <dl className="message-facts">{facts.map(([label, value]) => <div key={label}><dt>{label}</dt><dd>{value}</dd></div>)}</dl>
    <div className="message-views" role="group" aria-label="Message detail view">
      <button aria-pressed={view === "wire"} onClick={() => setView("wire")}>Wire contents</button>
      <button aria-pressed={view === "actions"} onClick={() => setView("actions")}>Decoded actions</button>
      <button aria-pressed={view === "raw"} onClick={() => setView("raw")}>Raw bytes</button>
    </div>
    {view === "wire" ? <section className="wire-message" aria-label="Wire contents and pool updates">
      <div className="bytes-heading"><span>WIRE CONTENTS</span><span>{message ? `Protocol ${message.protocol} · ${message.metadataCount} metadata · ${message.actions.length} actions` : "Waiting for a message"}</span></div>
      <pre className="wire-listing" tabIndex={0} aria-label="Annotated wire bytes">{message?.wire ?? "No messages yet"}</pre>
    </section> : view === "actions" ? <section className="decoded-message" aria-label="Decoded message operations">
      <div className="bytes-heading"><span>DECODED OPERATIONS</span><span>{message ? `Protocol ${message.protocol} · ${message.actions.length} action${message.actions.length === 1 ? "" : "s"}` : "Waiting for a message"}</span></div>
      <p className="decode-note">Shared references are expanded. Actions run in order; array patch indexes refer to the array before that action.</p>
      <ol className="decoded-actions" tabIndex={0} aria-label="Actions carried by this message">
        {message?.actions.map((action, index) => <li key={index}>
          <h4>{index + 1}. {action.title}</h4>
          <pre>{action.content}</pre>
        </li>)}
      </ol>
    </section> : <section className="raw-message" aria-label="Raw message">
      <div className="bytes-heading"><span>Hexadecimal · offsets on the left</span><span>{message ? `${message.size} bytes` : "No messages yet"}</span></div>
      <pre className="message-bytes" tabIndex={0} aria-label="Message bytes in hexadecimal">{message?.hex ?? "No messages yet"}</pre>
    </section>}
  </section>;
}
