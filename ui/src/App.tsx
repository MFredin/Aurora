import { type ReactNode, useCallback, useEffect, useMemo, useReducer } from "react";
import { applyFix, cleanTemp, emptyRecycleBin, history as loadHistory, scan, setStartup, undo as undoChange } from "./api";
import Cleanup, { ConfirmRemove } from "./Cleanup";
import Detail from "./Detail";
import { type Change, gb, groupChanges, kindOf } from "./format";
import History from "./History";
import { Broom, Clock, Gauge, Logo, Power, Refresh, Sliders } from "./Icons";
import Overhead from "./Overhead";
import Overview from "./Overview";
import Settings from "./Settings";
import Startup from "./Startup";
import type { Finding, HistoryItem, Machine, ScanEvent } from "./types";

export type Status = { kind: "idle" } | { kind: "applying" } | { kind: "applied" } | { kind: "failed"; message: string };
export type View = "overview" | "settings" | "startup" | "cleanup" | "history";
export type Phase = "idle" | "scanning" | "done" | "failed";

interface State {
  phase: Phase;
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
  /** Startup switches by item index: pending, or an error message. */
  startupBusy: Record<number, string>;
  cleaning: Record<string, string>;
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
  | { type: "undoing"; key: number; v: string | null }
  | { type: "startupBusy"; index: number; v: string | null }
  | { type: "startupSet"; index: number; enabled: boolean }
  | { type: "cleaning"; what: string; v: string | null }
  | { type: "cleaned"; what: "temp" | "recycle" };

const initial: State = {
  phase: "idle", scanError: null, machine: null, findings: [], status: {}, view: "overview", detail: null,
  picked: [], confirm: null, notices: [], history: [], historyError: null, undoing: {}, startupBusy: {}, cleaning: {},
};

function without<T>(rec: Record<string | number, T>, key: string | number, v: T | null): Record<string | number, T> {
  const out = { ...rec };
  if (v === null) delete out[key]; else out[key] = v;
  return out;
}

function reducer(s: State, a: Action): State {
  switch (a.type) {
    case "start":
      return { ...s, phase: "scanning", scanError: null, machine: null, findings: [], status: {}, detail: null, picked: [], startupBusy: {}, cleaning: {} };
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
    case "undoing": return { ...s, undoing: without(s.undoing, a.key, a.v) };
    case "startupBusy": return { ...s, startupBusy: without(s.startupBusy, a.index, a.v) };
    case "startupSet": {
      if (!s.machine) return s;
      const startup = s.machine.startup.map((it, i) => (i === a.index ? { ...it, enabled: a.enabled } : it));
      return { ...s, machine: { ...s.machine, startup } };
    }
    case "cleaning": return { ...s, cleaning: without(s.cleaning, a.what, a.v) };
    case "cleaned":
      if (!s.machine) return s;
      return { ...s, machine: { ...s.machine, ...(a.what === "temp" ? { temp_bytes: 0 } : { recycle_bin_bytes: 0 }) } };
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

  const notify = useCallback((r: { restore_point: string | null; warning: string | null }) => {
    if (r.restore_point) dispatch({ type: "notice", text: r.restore_point });
    if (r.warning) dispatch({ type: "notice", text: r.warning });
  }, []);

  const apply = useCallback(async (ids: string[]) => {
    for (const id of ids) {
      dispatch({ type: "status", id, s: { kind: "applying" } });
      try {
        notify(await applyFix(id));
        dispatch({ type: "status", id, s: { kind: "applied" } });
      } catch (err) {
        dispatch({ type: "status", id, s: { kind: "failed", message: String(err) } });
        break; // stop a batch at the first failure so the user sees why
      } finally {
        refreshHistory();
      }
    }
  }, [notify, refreshHistory]);

  const undo = useCallback(async (c: Change) => {
    dispatch({ type: "undoing", key: c.key, v: "busy" });
    try {
      await undoChange(c.items.filter((h) => h.undoable).map((h) => h.id));
      dispatch({ type: "undoing", key: c.key, v: null });
      dispatch({ type: "status", id: c.first.finding_id, s: { kind: "idle" } });
    } catch (err) {
      dispatch({ type: "undoing", key: c.key, v: String(err) });
    }
    refreshHistory();
  }, [refreshHistory]);

  const changes = useMemo(() => groupChanges(s.history), [s.history]);

  /** A setting switch applies the fix, or undoes Forge's latest change for that finding. */
  const toggle = useCallback((f: Finding) => {
    const st = s.status[f.id]?.kind;
    if (st === "applying") return;
    const last = changes.find((c) => c.first.finding_id === f.id && c.undoable && c.status === "applied");
    if (st === "applied" && last) undo(last);
    else apply([f.id]);
  }, [s.status, changes, apply, undo]);

  const toggleStartup = useCallback(async (index: number, enabled: boolean) => {
    dispatch({ type: "startupBusy", index, v: "busy" });
    try {
      notify(await setStartup(index, enabled));
      dispatch({ type: "startupSet", index, enabled });
      dispatch({ type: "startupBusy", index, v: null });
    } catch (err) {
      dispatch({ type: "startupBusy", index, v: String(err) });
    }
    refreshHistory();
  }, [notify, refreshHistory]);

  const clean = useCallback(async (what: "temp" | "recycle") => {
    dispatch({ type: "cleaning", what, v: "busy" });
    try {
      if (what === "temp") {
        const t = await cleanTemp();
        dispatch({ type: "notice", text: `Removed ${t.files.toLocaleString()} temporary files and freed ${gb(t.bytes)}. Files still in use were left alone.` });
      } else {
        await emptyRecycleBin();
        dispatch({ type: "notice", text: "The Recycle Bin is empty." });
      }
      dispatch({ type: "cleaned", what });
      dispatch({ type: "cleaning", what, v: null });
    } catch (err) {
      dispatch({ type: "cleaning", what, v: String(err) });
    }
  }, []);

  const lowRisk = useMemo(
    () => s.findings.filter((f) => kindOf(f) === "setting" && f.risk === "safe" && f.fix?.reversible && !["applied", "applying"].includes(s.status[f.id]?.kind ?? "")).map((f) => f.id),
    [s.findings, s.status],
  );
  const apps = s.findings.filter((f) => kindOf(f) === "app");
  const settings = s.findings.filter((f) => kindOf(f) === "setting");
  const openApps = apps.filter((f) => s.status[f.id]?.kind !== "applied").length;
  const openSettings = settings.filter((f) => s.status[f.id]?.kind !== "applied").length;
  const detail = s.findings.find((f) => f.id === s.detail) ?? null;
  const m = s.machine;

  const nav: { view: View; label: string; icon: ReactNode; badge?: number }[] = [
    { view: "overview", label: "Overview", icon: <Gauge /> },
    { view: "settings", label: "Settings", icon: <Sliders />, badge: openSettings || undefined },
    { view: "startup", label: "Startup apps", icon: <Power /> },
    { view: "cleanup", label: "Clean up", icon: <Broom />, badge: openApps || undefined },
    { view: "history", label: "History", icon: <Clock /> },
  ];
  const titles: Record<View, string> = { overview: "Overview", settings: "Settings", startup: "Startup apps", cleanup: "Clean up", history: "History" };
  const go = (view: View) => { if (view === "history" || view === "settings") refreshHistory(); dispatch({ type: "view", view }); };

  return (
    <div className="app">
      <nav className="rail" aria-label="Main">
        <div className="brand" title="Forge"><Logo /></div>
        {nav.map((n) => (
          <button key={n.view} className="rail-btn" aria-label={n.label} title={n.label} aria-current={s.view === n.view ? "page" : undefined}
            onClick={() => go(n.view)}>
            {n.icon}
            {n.badge ? <span className="rail-badge">{n.badge}</span> : null}
          </button>
        ))}
      </nav>

      <div className="main">
        <header className="top">
          <div className="top-left">
            <h1>{titles[s.view]}</h1>
            {m && (
              <span className="chip">
                {[m.cpu_name.replace(/\(R\)|\(TM\)/g, "").replace(/\s+/g, " "), m.gpus[0]?.replace(/\(R\)|\(TM\)/g, ""), `${Math.round(m.ram_total_mb / 1024)} GB`,
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
            lowRisk={lowRisk} onScan={runScan} onApply={apply} onToggle={toggle}
            onDetail={(id) => dispatch({ type: "detail", id })} onGo={go} />
        )}
        {s.view === "settings" && (
          <Settings phase={s.phase} settings={settings} findings={s.findings} status={s.status} changes={changes} undoing={s.undoing} machine={m}
            onToggle={toggle} onUndo={undo} onDetail={(id) => dispatch({ type: "detail", id })} onScan={runScan} />
        )}
        {s.view === "startup" && (
          <Startup phase={s.phase} machine={m} busy={s.startupBusy} onToggle={toggleStartup} onScan={runScan} />
        )}
        {s.view === "cleanup" && (
          <Cleanup phase={s.phase} machine={m} apps={apps} status={s.status} picked={s.picked} history={s.history} cleaning={s.cleaning}
            onPick={(id, on) => dispatch({ type: "pick", id, on })}
            onPickAll={(ids) => dispatch({ type: "pickAll", ids })}
            onRemove={(ids) => dispatch({ type: "confirm", ids })}
            onClean={clean}
            onDetail={(id) => dispatch({ type: "detail", id })} onScan={runScan} />
        )}
        {s.view === "history" && (
          <History changes={changes} error={s.historyError} undoing={s.undoing} machine={m} onUndo={undo} />
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
