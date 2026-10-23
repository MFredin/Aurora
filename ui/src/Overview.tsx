import type { Phase, Status, View } from "./App";
import { gb, kindOf } from "./format";
import { Check, Disk, Store, Trash } from "./Icons";
import { Ring, SettingRow, iconFor } from "./Parts";
import type { Finding, Machine } from "./types";

interface Props {
  phase: Phase;
  scanError: string | null;
  machine: Machine | null;
  findings: Finding[];
  status: Record<string, Status>;
  lowRisk: string[];
  onScan: () => void;
  onApply: (ids: string[]) => void;
  onToggle: (f: Finding) => void;
  onDetail: (id: string) => void;
  onGo: (v: View) => void;
}

/** A dashboard that fits one window: the full lists live on their own pages. */
export default function Overview(p: Props) {
  const { phase, findings, status, machine: m } = p;
  const scanning = phase === "scanning";
  const isOpen = (f: Finding) => status[f.id]?.kind !== "applied";
  const settings = findings.filter((f) => kindOf(f) === "setting");
  const apps = findings.filter((f) => kindOf(f) === "app");
  const advice = findings.filter((f) => kindOf(f) === "advice");
  const open = findings.filter(isOpen).length;
  const done = findings.length - open;

  if (phase === "idle") {
    return (
      <main className="content">
        <section className="hero card">
          <Ring open={0} done={0} scanning={false} idle />
          <div className="hero-text">
            <h2>See what's slowing this PC down</h2>
            <p className="muted">Forge checks your settings, startup apps, preinstalled apps, drivers, storage and memory. It changes nothing until you say so, and every settings change can be undone.</p>
            <button className="btn primary big" onClick={p.onScan}>Scan this PC</button>
          </div>
        </section>
      </main>
    );
  }

  const display = m?.displays[0];
  const startupOn = m?.startup.filter((i) => i.enabled).length ?? 0;
  const xmp = findings.some((f) => f.id === "ram:xmp");
  const memPct = m && m.ram_total_mb ? Math.round((m.ram_used_mb / m.ram_total_mb) * 100) : null;
  const lowRiskLabel = p.lowRisk.length === 1 ? "Apply 1 low-risk fix" : `Apply ${p.lowRisk.length} low-risk fixes`;
  const recommended = [...settings.filter(isOpen), ...settings.filter((f) => !isOpen(f))].slice(0, 4);
  const openApps = apps.filter(isOpen).length;

  return (
    <main className="content fit">
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
        <div className="tiles">
          <div className="tile">
            <span className="tile-k">Refresh rate</span>
            <span className="tile-v">{display ? display.current_hz : "–"}<small>{display ? ` / ${display.max_hz} Hz` : ""}</small></span>
            <span className={`tile-s ${display && display.max_hz > display.current_hz + 2 ? "warn" : "ok"}`}>
              {display ? (display.max_hz > display.current_hz + 2 ? "Below what it supports" : "At its best") : scanning ? "Checking" : "No display info"}
            </span>
          </div>
          <button className="tile as-link" onClick={() => p.onGo("startup")}>
            <span className="tile-k">Startup apps</span>
            <span className="tile-v">{startupOn}<small> start with Windows</small></span>
            <span className="tile-s muted">{scanning ? "Checking" : "Manage"}</span>
          </button>
          <div className="tile">
            <span className="tile-k">Memory in use</span>
            <span className="tile-v">{memPct ?? "–"}<small>{memPct !== null ? `% of ${Math.round((m?.ram_total_mb ?? 0) / 1024)} GB` : ""}</small></span>
            <span className={`tile-s ${scanning ? "muted" : xmp || (memPct ?? 0) >= 85 ? "warn" : "ok"}`}>
              {scanning ? "Checking" : xmp ? "Running below rated speed" : m?.ram_configured_mts ? `${m.ram_configured_mts} MT/s, at rated speed` : "Speed not reported"}
            </span>
          </div>
          <button className="tile as-link" onClick={() => p.onGo("cleanup")}>
            <span className="tile-k">Can be cleaned up</span>
            <span className="tile-v">{gb((m?.temp_bytes ?? 0) + (m?.recycle_bin_bytes ?? 0))}</span>
            <span className="tile-s muted">{scanning ? "Checking" : openApps ? `Plus ${openApps} removable app${openApps === 1 ? "" : "s"}` : "Temp files and Recycle Bin"}</span>
          </button>
        </div>
      </section>

      <section className="dash-grid">
        <div className="card">
          <div className="card-head">
            <h2>Recommended settings</h2>
            {settings.length > 4 && <button className="link" onClick={() => p.onGo("settings")}>All {settings.length} settings</button>}
          </div>
          {recommended.length ? (
            <ul className="rows">
              {recommended.map((f) => (
                <SettingRow key={f.id} f={f} st={status[f.id] ?? { kind: "idle" }} compact
                  onToggle={() => p.onToggle(f)} onDetail={() => p.onDetail(f.id)} />
              ))}
            </ul>
          ) : (
            <p className="empty-state"><Check /> {scanning ? "Checking settings." : "Your settings are already tuned."}</p>
          )}
        </div>

        <div className="stack">
          <div className="card">
            <div className="card-head"><h2>Clean up</h2><button className="link" onClick={() => p.onGo("cleanup")}>Review</button></div>
            <ul className="mini">
              <li><Disk /><span>Temporary files</span><b>{gb(m?.temp_bytes)}</b></li>
              <li><Trash /><span>Recycle Bin</span><b>{gb(m?.recycle_bin_bytes)}</b></li>
              <li><Store /><span>Removable apps</span><b>{openApps}</b></li>
            </ul>
          </div>
          <div className="card">
            <div className="card-head">
              <h2>Worth knowing</h2>
              {advice.length > 3 && <button className="link" onClick={() => p.onGo("settings")}>{advice.length - 3} more</button>}
            </div>
            {advice.length ? (
              <ul className="mini">
                {advice.slice(0, 3).map((f) => (
                  <li key={f.id}>
                    {iconFor(f)}
                    <button className="link plain" onClick={() => p.onDetail(f.id)}>{f.title}</button>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="empty-state"><Check /> {scanning ? "Checking." : "Nothing to flag."}</p>
            )}
          </div>
        </div>
      </section>
    </main>
  );
}
