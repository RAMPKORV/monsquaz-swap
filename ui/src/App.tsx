import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type Health = {
  status: string;
  appVersion: string;
  coreVersion: string;
};

export default function App() {
  const [health, setHealth] = useState<Health | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function checkCore() {
    setBusy(true);
    setError(null);
    try {
      setHealth(await invoke<Health>("health"));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="shell">
      <header className="masthead">
        <p className="kicker">anonymous album swap</p>
        <h1>
          Monsquaz <span className="arrow" aria-hidden="true">↕</span> Swap
        </h1>
        <p className="lede">
          A Discord-native exquisite-corpse music event. Everyone seeds a
          track, hands it on, and nobody knows who made what until the reveal.
        </p>
      </header>

      <section className="panel" aria-labelledby="scaffold-title">
        <div className="panel-heading">
          <h2 id="scaffold-title">Scaffold online</h2>
          <span className="badge">v0.1</span>
        </div>
        <p>
          The Tauri shell, Rust core, tracker, and release wiring are in place.
          The event engine is the next slice.
        </p>
        <button type="button" onClick={checkCore} disabled={busy}>
          {busy ? "Checking…" : "Check core"}
        </button>
        {health ? (
          <p className="status ok" data-testid="health">
            {health.status} · app {health.appVersion} · core{" "}
            {health.coreVersion}
          </p>
        ) : null}
        {error ? (
          <p className="status error" role="alert">
            {error}
          </p>
        ) : null}
      </section>

      <footer>
        <span>Monsquaz Swap</span>
        <span>start with a seed · end with an album</span>
      </footer>
    </main>
  );
}
