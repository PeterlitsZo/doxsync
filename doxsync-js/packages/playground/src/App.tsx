import { useState } from "react";
import { SimulationControls } from "./components/SimulationControls";
import { DocumentPanel } from "./components/DocumentPanel";
import { MessageHistory } from "./components/MessageHistory";
import { MessageDetails } from "./components/MessageDetails";
import { useDisplayClock, useSimulation, useSnapshot } from "./hooks/useSimulation";
import type { SimulationController } from "./simulation/controller";
import type { Snapshot } from "./simulation/types";
import { bytesLabel } from "./simulation/format";

function Header({ status, state }: { status: string; state: string }) {
  return <header className="app-header">
    <div className="brand"><span className="brand-mark" aria-hidden="true">⇄</span><h1>doxsync</h1><span className="app-name">Playground</span></div>
    <p className="sync-state" data-state={state} role="status"><span className="status-dot" aria-hidden="true" />{status}</p>
  </header>;
}

function Transport({ snapshot }: { snapshot: Snapshot }) {
  const now = useDisplayClock();
  const [follow, setFollow] = useState(true);
  const [selectedId, setSelectedId] = useState<number>();
  const [historyVisible, setHistoryVisible] = useState(true);
  const selected = (follow ? undefined : snapshot.history.find((message) => message.id === selectedId)) ?? snapshot.history[0];
  if (!follow && selected?.id !== selectedId) setSelectedId(selected?.id);
  return <section className="transport" aria-label="Message delivery">
    <div className="transport-toolbar"><span>Producer <span aria-hidden="true">→</span> Consumer <span className="subtle"> / ordered stream</span></span>
      <button className="text-button" aria-expanded={historyVisible} aria-controls="history-panel" onClick={() => setHistoryVisible(!historyVisible)}>{historyVisible ? "Hide history" : "Show history"}</button>
    </div>
    <div className="message-workspace" data-history={historyVisible}>
      <aside id="history-panel" hidden={!historyVisible} aria-label="Message history">
        <MessageHistory history={snapshot.history} selected={selected?.id} follow={follow} now={now} onSelect={(id) => { setFollow(false); setSelectedId(id); }} />
      </aside>
      <MessageDetails message={selected} follow={follow} setFollow={(value) => { setSelectedId(selected?.id); setFollow(value); }} now={now} />
    </div>
  </section>;
}

function Playground({ controller }: { controller: SimulationController }) {
  const snapshot = useSnapshot(controller);
  const [messagesVisible, setMessagesVisible] = useState(true);
  const [documentsVisible, setDocumentsVisible] = useState(true);
  const panels = messagesVisible ? documentsVisible ? "both" : "messages" : documentsVisible ? "documents" : "none";
  return <>
    <header className="app-toolbar">
      <div className="brand"><span className="brand-mark" aria-hidden="true">⇄</span><h1>doxsync</h1></div>
      <SimulationControls controller={controller} snapshot={snapshot} />
      <div className="panel-controls" role="group" aria-label="Visible panels">
        <button aria-expanded={messagesVisible} aria-controls="messages-workspace" onClick={() => setMessagesVisible(!messagesVisible)}>{messagesVisible ? "Hide messages" : "Show messages"}</button>
        <button aria-expanded={documentsVisible} aria-controls="documents-workspace" onClick={() => setDocumentsVisible(!documentsVisible)}>{documentsVisible ? "Hide documents" : "Show documents"}</button>
      </div>
    </header>
    {snapshot.error && <div className="error" role="alert">{snapshot.error}{"\nThe simulation has stopped. Reset to try again."}</div>}
    <main id="workspace" tabIndex={-1} className="workspace" data-panels={panels}>
      <div id="messages-workspace" className="workspace-view" hidden={!messagesVisible}>
        <Transport key={snapshot.generation} snapshot={snapshot} />
      </div>
      <section id="documents-workspace" className="document-workspace workspace-view" hidden={!documentsVisible} aria-label="Producer and consumer documents">
        <header className="documents-heading"><h2>Documents</h2><span>Live state</span></header>
        <div className="documents">
          <DocumentPanel role="Producer" text={snapshot.producerText} previous={snapshot.previousProducerText} version={snapshot.sent} change={snapshot.producerChange} />
          <DocumentPanel role="Consumer" text={snapshot.consumerText} previous={snapshot.previousConsumerText} version={snapshot.consumerVersion} change={snapshot.consumerChange} />
        </div>
      </section>
      {panels === "none" && <div className="empty-workspace"><h2>Both panels are hidden</h2><p>Show a panel to continue inspecting.</p><button onClick={() => { setMessagesVisible(true); setDocumentsVisible(true); }}>Show both panels</button></div>}
    </main>
    <footer className="status-bar"><span><span className={`activity-dot${snapshot.running ? " active" : ""}`} aria-hidden="true" />{snapshot.running ? "Auto updates running" : "Auto updates paused"}</span>
      <span id="metrics">{snapshot.sent} sent <span>·</span> {bytesLabel(snapshot.totalBytes)} <span>·</span> {snapshot.pending} in flight <span>·</span> {snapshot.unchanged} unchanged</span>
      <span className="status-hint">{documentsVisible ? "Highlight = latest change · n = bigint" : "Local session · simulated delivery"}</span>
    </footer>
  </>;
}

export default function App() {
  const { controller, error, retry } = useSimulation();
  return <div className="app-shell">
    <a className="skip-link" href="#workspace">Skip to workspace</a>
    {controller ? <Playground controller={controller} /> : <>
      <Header state={error ? "error" : "loading"} status={error ? "Loading failed" : "Initializing"} />
      <main id="workspace" className="boot-state" tabIndex={-1}>
        <h2>{error ? "Unable to open the playground" : "Preparing your workspace"}</h2>
        {error ? <><p className="error" role="alert">{error}</p><button onClick={retry}>Retry loading</button></> : <p role="status">Loading the synchronization engine and wire inspector…</p>}
      </main>
    </>}
  </div>;
}
