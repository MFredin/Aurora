import { useCallback, useMemo, useReducer } from "react";
import { applyFix, scan } from "./api";
import { LANES, RISK_LABEL, barWeight, describeImpact, describeStep } from "./format";
import Overhead from "./Overhead";
import type { Finding, Machine, ScanEvent } from "./types";

type Status = { kind: "idle" } | { kind: "applying" } | { kind: "applied" } | { kind: "failed"; message: string };
interface State {
  phase: "idle" | "scanning" | "done";
  machine: Machine | null;
  findings: Finding[];
  status: Record<string, Status>;
  selected: string | null;
}
type Action =
  | { type: "start" }
  | { type: "event"; e: ScanEvent }
  | { type: "select"; id: string }
  | { type: "status"; id: string; s: Status };

const initial: State = { phase: "idle", machine: null, findings: [], status: {}, selected: null };

function reducer(s: State, a: Action): State {
  switch (a.type) {
    case "start": return { ...initial, phase: "scanning" };
    case "select": return { ...s, selected: a.id };
    case "status": return { ...s, status: { ...s.status, [a.id]: a.s } };
    case "event":
      if (a.e.event === "machine") return { ...s, machine: a.e.data };
      if (a.e.event === "finding") {
        const findings = [...s.findings, a.e.data];
        return { ...s, findings, selected: s.selected ?? a.e.data.id };
      }
      return { ...s, phase: "done" };
  }
}

export default function App() {
  const [s, dispatch] = useReducer(reducer, initial);
  const scanning = s.phase === "scanning";

  const runScan = useCallback(() => {
    dispatch({ type: "start" });
    scan((e) => dispatch({ type: "event", e })).catch((err) => {
      dispatch({ type: "event", e: { event: "done", data: { count: 0 } } });
      console.error(err);
    });
  }, []);

  const apply = useCallback(async (ids: string[]) => {
    for (const id of ids) {
      dispatch({ type: "status", id, s: { kind: "applying" } });
      try { await applyFix(id); dispatch({ type: "status", id, s: { kind: "applied" } }); }
      catch (err) { dispatch({ type: "status", id, s: { kind: "failed", message: String(err) } }); }
    }
  }, []);

  const safeIds = useMemo(
    () => s.findings.filter((f) => f.fix && f.risk === "safe" && s.status[f.id]?.kind !== "applied").map((f) => f.id),
    [s.findings, s.status],
  );
  const selected = s.findings.find((f) => f.id === s.selected) ?? null;

  return (
    <div className="app">
      <header className="top">
        <h1>Forge</h1>
        <p className="specs" aria-label="This PC">
          {s.machine ? (<>
            <span>{s.machine.cpu_name}</span>
            {s.machine.gpus.map((g) => <span key={g}>{g}</span>)}
            <span>{Math.round(s.machine.ram_total_mb / 1024)} GB RAM</span>
            <span>{s.machine.os}</span>
          </>) : <span className="muted">Scan to see what is slowing this PC down.</span>}
        </p>
        <button className="btn primary" onClick={runScan} disabled={scanning}>
          {scanning ? "Scanning" : s.phase === "idle" ? "Scan this PC" : "Scan again"}
        </button>
      </header>

      <main className="body">
        <section className={`lanes${scanning ? " is-scanning" : ""}`} aria-label="Findings by area">
          {LANES.map(({ cat, label, clear }) => {
            const items = s.findings.filter((f) => f.category === cat);
            const total = items.reduce((n, f) => n + barWeight(f), 0);
            const max = Math.max(1, ...LANES.map((l) => s.findings.filter((f) => f.category === l.cat).reduce((n, f) => n + barWeight(f), 0)));
            return (
              <div className="lane" key={cat}>
                <div className="lane-head">
                  <h2>{label}</h2>
                  <span className="lane-note">
                    {s.phase === "idle" ? "Not scanned yet" : items.length ? `${items.length} to review` : scanning ? "Checking" : clear}
                  </span>
                </div>
                <div className="bar" aria-hidden><i style={{ transform: `scaleX(${items.length ? Math.max(0.08, total / max) : 0})` }} /></div>
                <ul>
                  {items.map((f) => {
                    const st = s.status[f.id]?.kind ?? "idle";
                    return (
                      <li key={f.id}>
                        <button className={`row${f.id === s.selected ? " is-selected" : ""}`} onClick={() => dispatch({ type: "select", id: f.id })}>
                          <span className={`risk ${f.risk}`} role="img" aria-label={RISK_LABEL[f.risk]} />
                          <span className="row-title">{f.title}</span>
                          {st === "applied" && <span className="tag done">Applied</span>}
                          {st === "failed" && <span className="tag failed">Failed</span>}
                          {st === "applying" && <span className="tag">Applying</span>}
                        </button>
                      </li>
                    );
                  })}
                </ul>
              </div>
            );
          })}
        </section>

        <aside className="detail" aria-live="polite">
          {selected ? <Detail f={selected} status={s.status[selected.id] ?? { kind: "idle" }} onApply={() => apply([selected.id])} />
            : <div className="empty"><h2>{s.phase === "idle" ? "Start with a scan" : "Pick a finding"}</h2>
                <p>{s.phase === "idle" ? "Forge reads your hardware, drivers and startup items. It changes nothing until you apply a fix." : "Details and the exact changes appear here."}</p></div>}
        </aside>
      </main>

      <footer className="foot">
        <Overhead />
        {safeIds.length > 0 && !scanning && (
          <button className="btn" onClick={() => apply(safeIds)}>Apply safe fixes ({safeIds.length})</button>
        )}
      </footer>
    </div>
  );
}

function Detail({ f, status, onApply }: { f: Finding; status: Status; onApply: () => void }) {
  const impact = describeImpact(f.impact);
  return (
    <article>
      <h2>{f.title}</h2>
      <p className="risk-line"><span className={`risk ${f.risk}`} aria-hidden /> {RISK_LABEL[f.risk]}</p>
      <p>{f.why}</p>
      {impact.length > 0 && <p className="impact"><strong>Estimated gain.</strong> {impact.join(", ")}. Forge measures the real change after you apply it.</p>}
      <h3>What Forge found</h3>
      <ul className="evidence">{f.evidence.map((e) => <li key={e}><code>{e}</code></li>)}</ul>
      {f.fix ? (<>
        <h3>What will change</h3>
        <ul className="steps">{f.fix.steps.map((st, i) => <li key={i}>{describeStep(st)}</li>)}</ul>
        <p className="muted">{f.fix.reversible ? "You can undo this from History." : "This cannot be undone automatically."}</p>
        {f.advice && <p className="muted">{f.advice}</p>}
        <button className="btn primary" onClick={onApply} disabled={status.kind === "applying" || status.kind === "applied"}>
          {status.kind === "applying" ? "Applying" : status.kind === "applied" ? "Applied" : "Apply fix"}
        </button>
        {status.kind === "failed" && <p className="error" role="alert">{status.message}</p>}
      </>) : f.advice && (<><h3>What to do</h3><p className="advice">{f.advice}</p></>)}
    </article>
  );
}
