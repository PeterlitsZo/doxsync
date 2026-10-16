import { useEffect, useState, useSyncExternalStore } from "react";
import { init } from "@doxsync/core";
import { init as initInspector } from "@doxsync/inspector";
import inspectorUrl from "@doxsync/inspector/inspector.wasm?url";
import wasmUrl from "@doxsync/core/doxsync.wasm?url";
import { SimulationController } from "../simulation/controller";
import { errorText } from "../simulation/format";

export function useSimulation() {
  const [controller, setController] = useState<SimulationController>();
  const [error, setError] = useState<string>();
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let active = true;
    let current: SimulationController | undefined;
    let generation = 0;
    const stop = () => {
      generation++;
      current?.dispose();
      current = undefined;
    };
    const boot = async () => {
      stop();
      const token = generation;
      setController(undefined);
      setError(undefined);
      try {
        await Promise.all([init(wasmUrl), initInspector(inspectorUrl)]);
        if (!active || token !== generation) return;
        current = new SimulationController();
        setController(current);
      } catch (error) {
        if (active && token === generation) setError(errorText(error));
      }
    };
    const restore = (event: PageTransitionEvent) => { if (event.persisted) void boot(); };
    window.addEventListener("pagehide", stop);
    window.addEventListener("pageshow", restore);
    void boot();
    return () => {
      active = false;
      stop();
      window.removeEventListener("pagehide", stop);
      window.removeEventListener("pageshow", restore);
    };
  }, [attempt]);
  return { controller, error, retry: () => setAttempt((value) => value + 1) };
}

export function useSnapshot(controller: SimulationController) {
  return useSyncExternalStore(controller.subscribe, controller.getSnapshot);
}

export function useDisplayClock() {
  const [now, setNow] = useState(() => performance.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(performance.now()), 100);
    return () => clearInterval(timer);
  }, []);
  return now;
}
