import type { SimulationController } from "../simulation/controller";
import type { Snapshot } from "../simulation/types";

export function SimulationControls({ controller, snapshot }: { controller: SimulationController; snapshot: Snapshot }) {
  const disabled = !!snapshot.error;
  return <section className="simulation-toolbar" aria-label="Simulation">
    <section className="controls" aria-label="Simulation controls">
      <div className="buttons">
        <button className="primary" disabled={disabled} onClick={controller.toggle}>{disabled ? "Auto updates stopped" : snapshot.running ? "Pause auto updates" : "Resume auto updates"}</button>
        <button disabled={disabled} onClick={controller.step}>Change once</button>
        <button onClick={controller.reset}>Reset</button>
      </div>
      <label>Interval
        <select disabled={disabled} value={snapshot.interval} onChange={(event) => controller.setInterval(Number(event.target.value))}>
          {[500, 1000, 1500, 3000, 10000, 30000].map((interval) => <option key={interval} value={interval}>{interval / 1000} s</option>)}
        </select>
      </label>
      <label>Delay
        <span className="delay-input"><input type="number" min="0" max="10000" step="100" disabled={disabled} value={snapshot.delay} onChange={(event) => controller.setDelay(event.target.valueAsNumber)} /><span>ms</span></span>
      </label>
    </section>
    <details className="control-help"><summary aria-label="Simulation behavior">?</summary><p>Delay changes apply to new messages. Delivery stays ordered, and queued messages continue arriving while updates are paused.</p></details>
  </section>;
}
