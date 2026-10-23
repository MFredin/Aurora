import type { ReactNode } from "react";
import type { Status } from "./App";
import { RISK_LABEL, firstSentence, friendlyPackage, kindOf, settingTitle, switchFor, when } from "./format";
import { Bolt, Check, Chip, Disk, Monitor, Mouse, Pad, Power, Record, Store } from "./Icons";
import type { Finding, HistoryItem, Machine } from "./types";

interface Props {
  phase: "idle" | "scanning" | "done" | "failed";
  scanError: string | null;
  machine: Machine | null;
  findings: Finding[];
  status: Record<string, Status>;
  history: HistoryItem[];
  undoing: Record<number, string>;
  picked: string[];
  lowRisk: string[];
  onScan: () => void;
  onApply: (ids: string[]) => void;
  onToggle: (f: Finding) => void;
  onUndo: (h: HistoryItem) => void;
  onDetail: (id: string) => void;
  onPick: (id: string, on: boolean) => void;
  onRemove: (ids: string[]) => void;
  onOpenApps: () => void;
}

function iconFor(f: Finding): ReactNode {
  if (f.id.startsWith("startup:")) return <Power />;
  if (f.id === "gaming:dvr") return <Record />;
  if (f.id === "gaming:mouse-accel") return <Mouse />;
  if (f.id === "gaming:game-mode") return <Pad />;
  if (f.id === "bloat:suggestions") return <Store />;
  if (f.category === "power") return <Bolt />;
  if (f.category === "memory") return <Chip />;
  if (f.category === "storage") return <Disk />;
  return <Monitor />;
}

function Ring({ open, done, scanning, idle = false }: { open: number; done: number; scanning: boolean; idle?: boolean }) {
  const c = 2 * Math.PI * 82;
  const frac = open + done === 0 ? 0 : done / (open + done);
  return (
    <svg className={`ring${scanning ? " is-scanning" : ""}`} width="160" height="160" viewBox="0 0 200 200" role="img"
      aria-label={scanning ? "Scanning" : idle ? "Not scanned yet" : `${open} to tune, ${done} done`}>
      <defs>
        <linearGradient id="ringGrad" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stopColor="var(--from)" /><stop offset="1" stopColor="var(--to)" /></linearGradient>
      </defs>
      <circle cx="100" cy="100" r="82" fill="none" stroke="rgba(255,255,255,0.07)" strokeWidth="14" />
      <g className="ring-arc">
        {(scanning || done > 0) && (
          <circle cx="100" cy="100" r="82" fill="none" stroke="url(#ringGrad)" strokeWidth="14" strokeLinecap="round"
            strokeDasharray={scanning ? `${c * 0.22} ${c}` : `${Math.max(frac * c, 6)} ${c}`} transform="rotate(-90 100 100)" />
        )}
      </g>
      <text x="100" y="100" textAnchor="middle" className="ring-num">{scanning ? "…" : idle ? "–" : open}</text>
      <text x="100" y="128" textAnchor="middle" className="ring-label">{scanning ? "scanning" : idle ? "not scanned" : "to tune"}</text>
    </svg>
  );
}

function Switch({ f, st, onToggle }: { f: Finding; st: Status; onToggle: () => void }) {
  const sw = switchFor(f);
  const applied = st.kind === "applied";
  const on = applied ? !sw.on : sw.on;
  return (
    <>
      <span className="switch-text">{st.kind === "applying" ? "Changing" : on ? sw.onText : sw.offText}</span>
      <button role="switch" aria-checked={on} aria-label={settingTitle(f)} className={`switch${on ? " on" : ""}`}
        disabled={st.kind === "applying"} onClick={onToggle}><span /></button>
    </>
  );
}

export default function Overview(p: Props) {
  const { phase, findings, status, machine: m } = p;
  const scanning = phase === "scanning";
  const isOpen = (f: Finding) => status[f.id]?.kind !== "applied";
  const settings = findings.filter((f) => kindOf(f) === "setting");
  const apps = findings.filter((f) => kindOf(f) === "app");
  const advice = findings.filter((f) => kindOf(f) === "advice");
  const open = findings.filter((f) => isOpen(f) && kindOf(f) !== "advice").length + advice.length;
  const done = findings.filter((f) => !isOpen(f)).length;
  const recent = p.history.filter((h) => h.status === "applied" && h.undoable && !findings.some((f) => f.id === h.finding_id)).slice(0, 4);

  if (phase === "idle") {
    return (
      <main className="content">
        <section className="hero card">
          <Ring open={0} done={0} scanning={false} idle />
          <div className="hero-text">
            <h2>See what's slowing this PC down</h2>
            <p className="muted">Forge reads your hardware, settings, startup apps and preinstalled apps. It changes nothing until you say so, and every settings change can be undone.</p>
            <button className="btn primary big" onClick={p.onScan}>Scan this PC</button>
          </div>
        </section>
        {recent.length > 0 && <Recent items={recent} undoing={p.undoing} onUndo={p.onUndo} />}
      </main>
    );
  }

  const display = m?.displays[0];
  const startupOpen = settings.filter((f) => f.id.startsWith("startup:") && isOpen(f)).length;
  const xmp = findings.some((f) => f.id === "ram:xmp");
  const lowRiskLabel = p.lowRisk.length === 1 ? "Apply 1 low-risk fix" : `Apply ${p.lowRisk.length} low-risk fixes`;

  return (
    <main className="content">
      {phase === "failed" && (
        <p className="banner error" role="alert">The scan didn't finish, so these results are incomplete. Nothing was changed. {p.scanError}</p>
      )}
      <section className="top-grid">
        <div className="card score">
          <Ring open={open} done={done} scanning={scanning} />
          {p.lowRisk.length > 0 && !scanning ? (
            <>
              <button className="btn primary wide" onClick={() => p.onApply(p.lowRisk)}>{lowRiskLabel}</button>
              <span className="tiny muted">One Windows prompt · Undo any time</span>
            </>
          ) : (
            <span className="tiny muted">{scanning ? "Results appear as Forge finds them" : open ? "The rest need a look first" : "Nothing left to tune"}</span>
          )}
        </div>

        <div className="stack">
          <div className="tiles">
            <div className="tile">
              <span className="tile-k">Refresh rate</span>
              <span className="tile-v">{display ? display.current_hz : "–"}<small>{display ? ` / ${display.max_hz} Hz` : ""}</small></span>
              <span className={`tile-s ${display && display.max_hz > display.current_hz + 2 ? "warn" : "ok"}`}>
                {display ? (display.max_hz > display.current_hz + 2 ? "Below what it supports" : "At its best") : scanning ? "Checking" : "No display info"}
              </span>
            </div>
            <div className="tile">
              <span className="tile-k">Startup apps</span>
              <span className="tile-v">{startupOpen}<small> flagged</small></span>
              <span className="tile-s muted">{scanning ? "Checking" : startupOpen ? "Review below" : "Nothing heavy"}</span>
            </div>
            <div className="tile">
              <span className="tile-k">Bloatware</span>
              <span className="tile-v">{apps.filter(isOpen).length}<small> apps</small></span>
              <span className="tile-s muted">{scanning ? "Checking" : apps.filter(isOpen).length ? "Safe to remove" : apps.length ? "All removed" : "None found"}</span>
            </div>
            <div className="tile">
              <span className="tile-k">Memory</span>
              <span className="tile-v">{m?.ram_configured_mts ?? "–"}<small>{m?.ram_configured_mts ? " MT/s" : ""}</small></span>
              <span className={`tile-s ${scanning ? "muted" : xmp ? "warn" : "ok"}`}>{scanning ? "Checking" : xmp ? "Below rated speed" : m?.ram_configured_mts ? "At rated speed" : "Not reported"}</span>
            </div>
          </div>

          {apps.length > 0 && (
            <div className="card">
              <div className="card-head">
                <h2>Bloatware</h2>
                <button className="btn danger small" disabled={p.picked.length === 0} onClick={() => p.onRemove(p.picked)}>
                  {p.picked.length ? `Remove ${p.picked.length} selected` : "Select apps to remove"}
                </button>
              </div>
              <div className="app-grid">
                {apps.slice(0, 8).map((f) => {
                  const pkg = f.fix?.steps[0]?.op === "remove_appx" ? f.fix.steps[0].package_name : f.id;
                  const st = status[f.id]?.kind ?? "idle";
                  const picked = p.picked.includes(f.id);
                  return (
                    <label key={f.id} className={`app-pick${picked ? " is-picked" : ""}${st === "applied" ? " is-done" : ""}`}>
                      <input type="checkbox" checked={picked} disabled={st === "applied" || st === "applying"}
                        onChange={(e) => p.onPick(f.id, e.target.checked)} />
                      <span>{friendlyPackage(pkg)}</span>
                      {st === "applied" && <span className="tag ok">Removed</span>}
                      {st === "applying" && <span className="tag">Removing</span>}
                      {st === "failed" && <span className="tag bad">Failed</span>}
                    </label>
                  );
                })}
              </div>
              <p className="tiny muted note">Removed for your account. Forge can't put them back; you can reinstall them from the Microsoft Store.
                {apps.length > 8 && <> <button className="link" onClick={p.onOpenApps}>See all {apps.length}</button></>}</p>
            </div>
          )}
        </div>
      </section>

      {settings.length > 0 && (
        <section aria-label="Settings">
          <div className="section-head">
            <h2>Settings</h2>
            <span className="tiny muted">Flip a switch to change it. Flip it back to undo.</span>
          </div>
          <ul className="rows">
            {settings.map((f) => {
              const st = status[f.id] ?? { kind: "idle" };
              return (
                <li key={f.id} className="row">
                  <span className={`row-icon ${f.category}`}>{iconFor(f)}</span>
                  <button className="row-main" onClick={() => p.onDetail(f.id)}>
                    <span className="row-title">{settingTitle(f)}</span>
                    <span className="row-sub">{st.kind === "failed" ? <span className="bad">{st.message}</span> : firstSentence(f.why)}</span>
                  </button>
                  <span className={`pill ${f.risk}`}>{RISK_LABEL[f.risk]}</span>
                  <Switch f={f} st={st} onToggle={() => p.onToggle(f)} />
                </li>
              );
            })}
          </ul>
        </section>
      )}

      {advice.length > 0 && (
        <section aria-label="Advice">
          <div className="section-head">
            <h2>Worth doing yourself</h2>
            <span className="tiny muted">Hardware and Windows settings Forge doesn't change for you.</span>
          </div>
          <div className="advice-grid">
            {advice.map((f) => (
              <article key={f.id} className="card advice">
                <span className={`row-icon ${f.category}`}>{iconFor(f)}</span>
                <h3>{f.title}</h3>
                <p className="muted">{firstSentence(f.why)}</p>
                <button className="link" onClick={() => p.onDetail(f.id)}>Show me how</button>
              </article>
            ))}
          </div>
        </section>
      )}

      {phase === "done" && findings.length === 0 && (
        <p className="card empty-state"><Check /> Nothing to tune. This PC is set up well.</p>
      )}

      {recent.length > 0 && <Recent items={recent} undoing={p.undoing} onUndo={p.onUndo} />}
    </main>
  );
}

function Recent({ items, undoing, onUndo }: { items: HistoryItem[]; undoing: Record<number, string>; onUndo: (h: HistoryItem) => void }) {
  return (
    <section aria-label="Changed by Forge">
      <div className="section-head"><h2>Changed by Forge</h2></div>
      <ul className="rows">
        {items.map((h) => (
          <li key={h.id} className="row quiet">
            <span className="row-icon done"><Check /></span>
            <span className="row-main static">
              <span className="row-title">{h.title}</span>
              <span className="row-sub">{undoing[h.id] && undoing[h.id] !== "busy" ? <span className="bad">{undoing[h.id]}</span> : `Changed ${when(h.unix_ts)}`}</span>
            </span>
            <button className="btn ghost small" disabled={undoing[h.id] === "busy"} onClick={() => onUndo(h)}>
              {undoing[h.id] === "busy" ? "Undoing" : "Undo"}
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}
