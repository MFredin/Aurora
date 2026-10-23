import { type ReactNode, useCallback, useEffect, useMemo, useReducer } from "react";
import { applyFix, history as loadHistory, scan, undo as undoChange } from "./api";
import Apps, { ConfirmRemove } from "./Apps";
import Detail from "./Detail";
import { kindOf } from "./format";
import History from "./History";
import { Clock, Gauge, Grid, Logo, Refresh } from "./Icons";
import Overhead from "./Overhead";
import Overview from "./Overview";
import type { Finding, HistoryItem, Machine, ScanEvent } from "./types";

export type Status = { kind: "idle" } | { kind: "applying" } | { kind: "applied" } | { kind: "failed"; message: string };
export type View = "overview" | "apps" | "history";

interface State {
  phase: "idle" | "scanning" | "done" | "failed";
  scanError: string | null;
  machine: Machine | null;
  findings: Finding[];
  status: Record<string, Status>;
  view: View;
  detail: string | null;
  picked: string[];
  confirm: string[] | null;
  notices: string[];
  history: HistoryItem[];
  historyError: string | null;
  undoing: Record<number, string>;
}
type Action =
  | { type: "start" }
  | { type: "event"; e: ScanEvent }
  | { type: "scanFailed"; message: string }
  | { type: "status"; id: string; s: Status }
  | { type: "view"; view: View }
  | { type: "detail"; id: string | null }
  | { type: "pick"; id: string; on: boolean }
  | { type: "pickAll"; ids: string[] }
  | { type: "confirm"; ids: string[] | null }
  | { type: "notice"; text: string }
  | { type: "dismiss"; index: number }
  | { type: "history"; items: HistoryItem[] | null; error?: string }
  | { type: "undoing"; id: number; v: string | null };

const initial: State = {
  phase: "idle", scanError: null, machine: null, findings: [], status: {}, view: "overview", detail: null,
  picked: [], confirm: null, notices: [], history: [], historyError: null, undoing: {},
};

function reducer(s: State, a: Action): State {
  switch (a.type) {
    case "start":
      return { ...s, phase: "scanning", scanError: null, machine: null, findings: [], status: {}, detail: null, picked: [] };
    case "scanFailed": return { ...s, phase: "failed", scanError: a.message };
    case "status": return { ...s, status: { ...s.status, [a.id]: a.s } };
    case "view": return { ...s, view: a.view, detail: null };
    case "detail": return { ...s, detail: a.id };
    case "pick": return { ...s, picked: a.on ? [...new Set([...s.picked, a.id])] : s.picked.filter((x) => x !== a.id) };
    case "pickAll": return { ...s, picked: a.ids };
    case "confirm": return { ...s, confirm: a.ids };
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
      if (a.e.event === "finding") return { ...s, findings: [...s.findings, a.e.data] };
      {
        // Done: show findings in the engine's ranking.
        const rank = new Map(a.e.data.order.map((id, i) => [id, i]));
        const findings = [...s.findings].sort((x, y) => (rank.get(x.id) ?? 1e9) - (rank.get(y.id) ?? 1e9));
        return { ...s, phase: "done", findings };
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
    scan((e) => dispatch({ type: "event", e })).catch((err) => dispatch({ type: "scanFailed", message: String(err) }));
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

  /** A setting switch applies the fix, or undoes Forge's latest change for that finding. */
  const toggle = useCallback((f: Finding) => {
    const st = s.status[f.id]?.kind;
    if (st === "applying") return;
    const last = s.history.find((h) => h.finding_id === f.id && h.undoable && (h.status === "applied" || h.status === "interrupted"));
    if (st === "applied" && last) undo(last);
    else apply([f.id]);
  }, [s.status, s.history, apply, undo]);

  const lowRisk = useMemo(
    () => s.findings.filter((f) => kindOf(f) === "setting" && f.risk === "safe" && !["applied", "applying"].includes(s.status[f.id]?.kind ?? "")).map((f) => f.id),
    [s.findings, s.status],
  );
  const apps = s.findings.filter((f) => kindOf(f) === "app");
  const openApps = apps.filter((f) => s.status[f.id]?.kind !== "applied");
  const detail = s.findings.find((f) => f.id === s.detail) ?? null;

  const nav: { view: View; label: string; icon: ReactNode; badge?: number }[] = [
    { view: "overview", label: "Overview", icon: <Gauge /> },
    { view: "apps", label: "Apps", icon: <Grid />, badge: openApps.length || undefined },
    { view: "history", label: "History", icon: <Clock /> },
  ];
  const m = s.machine;

  return (
    <div className="app">
      <nav className="rail" aria-label="Main">
        <div className="brand" title="Forge"><Logo /></div>
        {nav.map((n) => (
          <button key={n.view} className="rail-btn" aria-label={n.label} title={n.label} aria-current={s.view === n.view ? "page" : undefined}
            onClick={() => { if (n.view === "history") refreshHistory(); dispatch({ type: "view", view: n.view }); }}>
            {n.icon}
            {n.badge ? <span className="rail-badge">{n.badge}</span> : null}
          </button>
        ))}
      </nav>

      <div className="main">
        <header className="top">
          <div className="top-left">
            <h1>{s.view === "overview" ? "Overview" : s.view === "apps" ? "Apps" : "History"}</h1>
            {m && (
              <span className="chip">
                {[m.cpu_name.replace(/\(R\)|\(TM\)/g, "").replace(/\s+/g, " "), m.gpus[0], `${Math.round(m.ram_total_mb / 1024)} GB`,
                  m.is_laptop ? (m.on_battery ? "Laptop on battery" : "Laptop") : null].filter(Boolean).join(" · ")}
              </span>
            )}
          </div>
          <div className="top-right">
            <Overhead />
            {s.phase !== "idle" && (
              <button className="btn ghost" onClick={runScan} disabled={scanning}>
                <span className={scanning ? "spin" : ""}><Refresh /></span>{scanning ? "Scanning" : "Scan again"}
              </button>
            )}
          </div>
        </header>

        {s.view === "overview" && (
          <Overview phase={s.phase} scanError={s.scanError} machine={m} findings={s.findings} status={s.status}
            history={s.history} undoing={s.undoing} picked={s.picked} lowRisk={lowRisk}
            onScan={runScan} onApply={apply} onToggle={toggle} onUndo={undo}
            onDetail={(id) => dispatch({ type: "detail", id })}
            onPick={(id, on) => dispatch({ type: "pick", id, on })}
            onRemove={(ids) => dispatch({ type: "confirm", ids })}
            onOpenApps={() => dispatch({ type: "view", view: "apps" })} />
        )}
        {s.view === "apps" && (
          <Apps phase={s.phase} apps={apps} status={s.status} picked={s.picked} history={s.history}
            onPick={(id, on) => dispatch({ type: "pick", id, on })}
            onPickAll={(ids) => dispatch({ type: "pickAll", ids })}
            onRemove={(ids) => dispatch({ type: "confirm", ids })}
            onDetail={(id) => dispatch({ type: "detail", id })} onScan={runScan} />
        )}
        {s.view === "history" && (
          <History items={s.history} error={s.historyError} undoing={s.undoing} machine={m} onUndo={undo} />
        )}
      </div>

      {detail && (
        <Detail f={detail} machine={m} status={s.status[detail.id] ?? { kind: "idle" }}
          onApply={() => apply([detail.id])}
          onRemove={() => dispatch({ type: "confirm", ids: [detail.id] })}
          onClose={() => dispatch({ type: "detail", id: null })} />
      )}

      {s.confirm && (
        <ConfirmRemove apps={s.findings.filter((f) => s.confirm?.includes(f.id))}
          onCancel={() => dispatch({ type: "confirm", ids: null })}
          onConfirm={() => {
            const ids = s.confirm ?? [];
            dispatch({ type: "confirm", ids: null });
            dispatch({ type: "pickAll", ids: [] });
            apply(ids);
          }} />
      )}

      {s.notices.length > 0 && (
        <div className="toasts" role="status">
          {s.notices.map((n, i) => (
            <div key={n} className="toast">
              <span>{n}</span>
              <button className="link" onClick={() => dispatch({ type: "dismiss", index: i })}>Dismiss</button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
