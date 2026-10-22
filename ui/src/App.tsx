import { useCallback, useEffect, useMemo, useReducer } from "react";
import { applyFix, history as loadHistory, scan, undo as undoChange } from "./api";
import { LANES, RISK_LABEL, barWeight, describeImpact, describeStep } from "./format";
import History from "./History";
import Overhead from "./Overhead";
import type { Finding, HistoryItem, Machine, ScanEvent } from "./types";

type Status = { kind: "idle" } | { kind: "applying" } | { kind: "applied" } | { kind: "failed"; message: string };
interface State {
  phase: "idle" | "scanning" | "done" | "failed";
  scanError: string | null;
  machine: Machine | null;
  findings: Finding[];
  status: Record<string, Status>;
  selected: string | null;
  view: "findings" | "history";
  notices: string[];
  history: HistoryItem[];
  historyError: string | null;
  undoing: Record<number, string | "busy">;
}
type Action =
  | { type: "start" }
  | { type: "event"; e: ScanEvent }
  | { type: "scanFailed"; message: string }
  | { type: "select"; id: string }
  | { type: "status"; id: string; s: Status }
  | { type: "view"; view: State["view"] }
  | { type: "notice"; text: string }
  | { type: "dismiss"; index: number }
  | { type: "history"; items: HistoryItem[] | null; error?: string }
  | { type: "undoing"; id: number; v: string | "busy" | null };

const initial: State = {
  phase: "idle", scanError: null, machine: null, findings: [], status: {}, selected: null,
  view: "findings", notices: [], history: [], historyError: null, undoing: {},
};

function reducer(s: State, a: Action): State {
  switch (a.type) {
    case "start":
      return { ...s, phase: "scanning", scanError: null, machine: null, findings: [], status: {}, selected: null, view: "findings" };
    case "scanFailed":
      return { ...s, phase: "failed", scanError: a.message };
    case "select": return { ...s, selected: a.id };
    case "status": return { ...s, status: { ...s.status, [a.id]: a.s } };
    case "view": return { ...s, view: a.view };
    case "notice": return s.notices.includes(a.text) ? s : { ...s, notices: [...s.notices, a.text] };
    case "dismiss": return { ...s, notices: s.notices.filter((_, i) => i !== a.index) };
    case "history":
      return a.items ? { ...s, history: a.items, historyError: null } : { ...s, historyError: a.error ?? "Could not read History." };
    case "undoing": {
      const undoing = { ...s.undoing };
      if (a.v === null) delete undoing[a.id]; else undoing[a.id] = a.v;
      return { ...s, undoing };
    }
    case "event":
      if (a.e.event === "machine") return { ...s, machine: a.e.data };
      if (a.e.event === "finding") {
        const findings = [...s.findings, a.e.data];
        return { ...s, findings };
      }
      {
        // Done: show findings in the engine's ranking.
        const rank = new Map(a.e.data.order.map((id, i) => [id, i]));
        const findings = [...s.findings].sort((x, y) => (rank.get(x.id) ?? 1e9) - (rank.get(y.id) ?? 1e9));
        return { ...s, phase: "done", findings, selected: s.selected ?? findings[0]?.id ?? null };
      }
  }
}

export default function App() {
  const [s, dispatch] = useReducer(reducer, initial);
  const scanning = s.phase === "scanning";

  const refreshHistory = useCallback(() => {
    loadHistory()
      .then((items) => dispatch({ type: "history", items }))
      .catch((err) => dispatch({ type: "history", items: null, error: String(err) }));
  }, []);
  useEffect(refreshHistory, [refreshHistory]);

  const runScan = useCallback(() => {
    dispatch({ type: "start" });
    scan((e) => dispatch({ type: "event", e })).catch((err) => {
      dispatch({ type: "scanFailed", message: String(err) });
    });
  }, []);

  const apply = useCallback(async (ids: string[]) => {
    for (const id of ids) {
      dispatch({ type: "status", id, s: { kind: "applying" } });
      try {
        const r = await applyFix(id);
        dispatch({ type: "status", id, s: { kind: "applied" } });
        if (r.restore_point) dispatch({ type: "notice", text: r.restore_point });
        if (r.warning) dispatch({ type: "notice", text: r.warning });
      } catch (err) {
        dispatch({ type: "status", id, s: { kind: "failed", message: String(err) } });
        break; // stop a batch at the first failure so the user sees why
      } finally {
        refreshHistory();
      }
    }
  }, [refreshHistory]);

  const undo = useCallback(async (item: HistoryItem) => {
    dispatch({ type: "undoing", id: item.id, v: "busy" });
    try {
      await undoChange(item.id);
      dispatch({ type: "undoing", id: item.id, v: null });
      dispatch({ type: "status", id: item.finding_id, s: { kind: "idle" } });
    } catch (err) {
      dispatch({ type: "undoing", id: item.id, v: String(err) });
    }
    refreshHistory();
  }, [refreshHistory]);

  const safeIds = useMemo(
    () => s.findings.filter((f) => f.fix && f.risk === "safe" && !["applied", "applying"].includes(s.status[f.id]?.kind ?? "")).map((f) => f.id),
    [s.findings, s.status],
  );
  const selected = s.findings.find((f) => f.id === s.selected) ?? null;
  const laneTotals = LANES.map((l) => s.findings.filter((f) => l.cats.includes(f.category)).reduce((n, f) => n + barWeight(f), 0));
  const maxTotal = Math.max(1, ...laneTotals);
  const showNav = s.phase !== "idle" || s.history.length > 0;

  return (
    <div className="app">
      <header className="top">
        <h1>Forge</h1>
        <p className="specs" aria-label="This PC">
          {s.machine ? (<>
            <span>{s.machine.cpu_name}</span>
            {s.machine.gpus.map((g) => <span key={g}>{g}</span>)}
            <span>
              {Math.round(s.machine.ram_total_mb / 1024)} GB RAM
              {s.machine.ram_configured_mts ? ` at ${s.machine.ram_configured_mts} MT/s` : ""}
            </span>
            <span>{s.machine.os}</span>
            {s.machine.is_laptop && <span>{s.machine.on_battery ? "Laptop on battery" : "Laptop"}</span>}
          </>) : <span className="muted">{scanning ? "Reading this PC" : "Scan to see what is slowing this PC down."}</span>}
        </p>
        {showNav && (
          <div className="seg" role="group" aria-label="View">
            <button aria-pressed={s.view === "findings"} onClick={() => dispatch({ type: "view", view: "findings" })}>Findings</button>
            <button aria-pressed={s.view === "history"} onClick={() => { refreshHistory(); dispatch({ type: "view", view: "history" }); }}>
              History{s.history.length ? ` (${s.history.length})` : ""}
            </button>
          </div>
        )}
        <button className="btn primary" onClick={runScan} disabled={scanning}>
          {scanning ? "Scanning" : s.phase === "idle" ? "Scan this PC" : "Scan again"}
        </button>
      </header>

      {s.notices.length > 0 && (
        <div className="notices" role="status">
          {s.notices.map((n, i) => (
            <p key={n} className="notice">
              <span>{n}</span>
              <button className="link" onClick={() => dispatch({ type: "dismiss", index: i })}>Dismiss</button>
            </p>
          ))}
        </div>
      )}

      {s.view === "history" ? (
        <History items={s.history} error={s.historyError} undoing={s.undoing} machine={s.machine} onUndo={undo} />
      ) : (
        <main className="body">
          <section className={`lanes${scanning ? " is-scanning" : ""}`} aria-label="Findings by area">
            {s.phase === "failed" && (
              <p className="scan-error" role="alert">
                The scan didn't finish, so these results are incomplete. Nothing was changed. {s.scanError}
              </p>
            )}
            {LANES.map(({ cats, label, clear }, li) => {
              const items = s.findings.filter((f) => cats.includes(f.category));
              if (label === "Other" && items.length === 0) return null;
              const note = s.phase === "idle" ? "Not scanned yet"
                : items.length ? `${items.length} to review`
                : scanning ? "Checking"
                : s.phase === "failed" ? "Not checked"
                : clear;
              return (
                <div className="lane" key={label}>
                  <div className="lane-head">
                    <h2>{label}</h2>
                    <span className="lane-note">{note}</span>
                  </div>
                  <div className="bar" aria-hidden><i style={{ transform: `scaleX(${items.length ? Math.max(0.06, laneTotals[li] / maxTotal) : 0})` }} /></div>
                  <ul>
                    {items.map((f) => {
                      const st = s.status[f.id]?.kind ?? "idle";
                      return (
                        <li key={f.id}>
                          <button className={`row${f.id === s.selected ? " is-selected" : ""}`} aria-pressed={f.id === s.selected} onClick={() => dispatch({ type: "select", id: f.id })}>
                            <span className={`risk ${f.risk}`} role="img" aria-label={RISK_LABEL[f.risk]} />
                            <span className="row-title">{f.title}</span>
                            {!f.fix && st === "idle" && <span className="tag">Advice</span>}
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
            {selected ? <Detail f={selected} machine={s.machine} status={s.status[selected.id] ?? { kind: "idle" }} onApply={() => apply([selected.id])} />
              : <div className="empty">
                  <h2>{s.phase === "idle" ? "Start with a scan" : scanning ? "Scanning" : "Pick a finding"}</h2>
                  <p>{s.phase === "idle"
                    ? "Forge reads your hardware, settings and startup items. It changes nothing until you apply a fix, and every fix can be undone from History."
                    : scanning ? "Results appear as Forge finds them." : "Details and the exact changes appear here."}</p>
                </div>}
          </aside>
        </main>
      )}

      <footer className="foot">
        <Overhead />
        {s.view === "findings" && safeIds.length > 0 && s.phase === "done" && (
          <button className="btn" onClick={() => apply(safeIds)}>Apply low-risk fixes ({safeIds.length})</button>
        )}
      </footer>
    </div>
  );
}

function Detail({ f, machine, status, onApply }: { f: Finding; machine: Machine | null; status: Status; onApply: () => void }) {
  const impact = describeImpact(f.impact);
  return (
    <article>
      <h2>{f.title}</h2>
      <p className="risk-line"><span className={`risk ${f.risk}`} aria-hidden /> {f.fix ? RISK_LABEL[f.risk] : f.risk === "expert" ? "Expert change, advice only" : "Advice only"}</p>
      <p>{f.why}</p>
      {impact.length > 0 && (
        <p className="impact"><strong>Rough estimate.</strong> {impact.join(", ")}. This is a typical figure, not a measurement of this PC.</p>
      )}
      <h3>What Forge found</h3>
      <ul className="evidence">{f.evidence.map((e) => <li key={e}><code>{e}</code></li>)}</ul>
      {f.fix ? (<>
        <h3>What will change</h3>
        <ul className="steps">{f.fix.steps.map((st, i) => <li key={i}>{describeStep(st, machine)}</li>)}</ul>
        <p className="muted">{f.fix.reversible ? "You can undo this from History." : "This cannot be undone automatically."}</p>
        {f.advice && <p className="muted">{f.advice}</p>}
        <p className="muted small">Windows asks for permission the first time you apply a fix in a session. Forge also asks Windows for a restore point first.</p>
        <button className="btn primary" onClick={onApply} disabled={status.kind === "applying" || status.kind === "applied"}>
          {status.kind === "applying" ? "Applying" : status.kind === "applied" ? "Applied" : status.kind === "failed" ? "Try again" : "Apply fix"}
        </button>
        {status.kind === "failed" && <p className="error" role="alert">{status.message}</p>}
      </>) : f.advice && (<>
        <h3>What to do</h3>
        <p className="advice">{f.advice}</p>
        <p className="muted">Forge doesn't change this for you.</p>
      </>)}
    </article>
  );
}
