import { useEffect, useRef } from "react";
import type { MessageView } from "../simulation/types";
import { messageId } from "../simulation/format";

export function statusText(message: MessageView, now: number) {
  if (message.status === "failed") return "Delivery failed";
  if (message.status === "delivered") return "Delivered";
  return `In flight · ${Math.max(0, message.dueAt - now).toFixed(0)} ms`;
}

export function MessageHistory({ history, selected, follow, onSelect, now }: {
  history: readonly MessageView[]; selected?: number; follow: boolean; onSelect: (id: number) => void; now: number;
}) {
  const list = useRef<HTMLOListElement>(null);
  const latestId = history[0]?.id;
  useEffect(() => { if (follow && list.current) list.current.scrollTop = 0; }, [follow, latestId]);
  return <div className="message-history">
    <div className="history-heading"><h2>History</h2><span>{history.length} messages</span></div>
    <ol ref={list} id="message-list" aria-label="Message history; select a message to inspect its contents">
      {history.map((message) => <li key={message.id}>
        <button className="message-row" data-status={message.status} data-message-id={message.id} aria-pressed={selected === message.id} onClick={() => onSelect(message.id)}>
          <span className="message-id">{messageId(message.id)}</span>
          <span className="message-summary" title={message.summary}>{message.summary}</span>
          <span><span className="message-size">{message.size} B</span><span className="message-state">{statusText(message, now)}</span></span>
        </button>
      </li>)}
    </ol>
    <p className="history-note">Latest 60 delivered + all in flight.</p>
  </div>;
}
