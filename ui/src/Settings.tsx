import type { Phase, Status } from "./App";
import { type Change, changeTitle, firstSentence, kindOf, when } from "./format";
import { Check } from "./Icons";
import { SettingRow, iconFor } from "./Parts";
import type { Finding, Machine } from "./types";

interface Props {
  phase: Phase;
  settings: Finding[];
  /** All findings; the advice ones are shown as cards. */
  findings: Finding[];
  status: Record<string, Status>;
  changes: Change[];
  undoing: Record<number, string>;
  machine: Machine | null;
  onToggle: (f: Finding) => void;
  onUndo: (c: Change) => void;
  onDetail: (id: string) => void;
  onScan: () => void;
}

/** Every setting Forge can change, plus what it already changed. */
export default function Settings(p: Props) {
  const recent = p.changes
    .filter((c) => c.status === "applied" && c.undoable && !c.first.finding_id.startsWith("startup-app:")
      && !p.settings.some((f) => f.id === c.first.finding_id))
    .slice(0, 6);

  return (
    <main className="content">
      {p.phase === "idle" ? (
        <section className="card">
          <p className="muted">Scan this PC to see which settings are worth changing. <button className="link" onClick={p.onScan}>Scan now</button></p>
        </section>
      ) : (
        <section aria-label="Settings to change">
          <div className="section-head">
            <h2>Settings worth changing</h2>
            <span className="tiny muted">Flip a switch to change it. Flip it back to undo.</span>
          </div>
          {p.settings.length ? (
            <ul className="rows">
              {p.settings.map((f) => (
                <SettingRow key={f.id} f={f} st={p.status[f.id] ?? { kind: "idle" }}
                  onToggle={() => p.onToggle(f)} onDetail={() => p.onDetail(f.id)} />
              ))}
            </ul>
          ) : (
            <p className="card empty-state"><Check /> {p.phase === "scanning" ? "Checking settings." : "Your settings are already tuned."}</p>
          )}
        </section>
      )}

      <AdviceCards findings={p.findings} onDetail={p.onDetail} />

      {recent.length > 0 && (
        <section aria-label="Changed by Forge">
          <div className="section-head"><h2>Changed by Forge</h2></div>
          <ul className="rows">
            {recent.map((c) => {
              const state = p.undoing[c.key];
              return (
                <li key={c.key} className="row quiet">
                  <span className="row-icon done"><Check /></span>
                  <span className="row-main static">
                    <span className="row-title">{changeTitle(c, p.machine)}</span>
                    <span className="row-sub">{state && state !== "busy" ? <span className="bad">{state}</span> : `Changed ${when(c.first.unix_ts)}`}</span>
                  </span>
                  <button className="btn ghost small" disabled={state === "busy"} onClick={() => p.onUndo(c)}>
                    {state === "busy" ? "Undoing" : "Undo"}
                  </button>
                </li>
              );
            })}
          </ul>
        </section>
      )}
    </main>
  );
}

/** Advice findings, shown as cards (used on the Settings page and elsewhere). */
export function AdviceCards({ findings, onDetail }: { findings: Finding[]; onDetail: (id: string) => void }) {
  const advice = findings.filter((f) => kindOf(f) === "advice");
  if (!advice.length) return null;
  return (
    <section aria-label="Worth doing yourself">
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
            <button className="link" onClick={() => onDetail(f.id)}>Show me how</button>
          </article>
        ))}
      </div>
    </section>
  );
}
