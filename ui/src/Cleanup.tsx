import { useEffect, useRef } from "react";
import type { Status } from "./App";
import { firstSentence, friendlyPackage, when } from "./format";
import type { Finding, HistoryItem } from "./types";

const pkgOf = (f: Finding) => (f.fix?.steps[0]?.op === "remove_appx" ? f.fix.steps[0].package_name : f.id);

interface Props {
  phase: "idle" | "scanning" | "done" | "failed";
  apps: Finding[];
  status: Record<string, Status>;
  picked: string[];
  history: HistoryItem[];
  onPick: (id: string, on: boolean) => void;
  onPickAll: (ids: string[]) => void;
  onRemove: (ids: string[]) => void;
  onDetail: (id: string) => void;
  onScan: () => void;
}

/** Preinstalled apps Forge's database marks safe to remove, plus what Forge already removed. */
export default function Apps(p: Props) {
  const removable = p.apps.filter((f) => !["applied", "applying"].includes(p.status[f.id]?.kind ?? ""));
  const removed = p.history.filter((h) => h.step.op === "remove_appx" && h.status === "applied");
  const allPicked = removable.length > 0 && removable.every((f) => p.picked.includes(f.id));

  return (
    <main className="content">
      <section className="card">
        <div className="card-head">
          <div>
            <h2>Preinstalled apps</h2>
            <p className="tiny muted">Only apps Forge knows are safe to remove are listed. Windows and Xbox components never are.</p>
          </div>
          <div className="row-actions">
            {removable.length > 0 && (
              <button className="btn ghost small" onClick={() => p.onPickAll(allPicked ? [] : removable.map((f) => f.id))}>
                {allPicked ? "Clear" : "Select all"}
              </button>
            )}
            <button className="btn danger small" disabled={p.picked.length === 0} onClick={() => p.onRemove(p.picked)}>
              {p.picked.length ? `Remove ${p.picked.length} selected` : "Remove selected"}
            </button>
          </div>
        </div>
        {p.phase === "idle" ? (
          <p className="muted">Scan this PC to find removable apps. <button className="link" onClick={p.onScan}>Scan now</button></p>
        ) : p.apps.length === 0 ? (
          <p className="muted">{p.phase === "scanning" ? "Checking installed apps." : "No removable preinstalled apps found."}</p>
        ) : (
          <ul className="rows flush">
            {p.apps.map((f) => {
              const st = p.status[f.id] ?? { kind: "idle" };
              return (
                <li key={f.id} className="row">
                  <input type="checkbox" aria-label={`Select ${friendlyPackage(pkgOf(f))}`} checked={p.picked.includes(f.id)}
                    disabled={st.kind === "applied" || st.kind === "applying"} onChange={(e) => p.onPick(f.id, e.target.checked)} />
                  <button className="row-main" onClick={() => p.onDetail(f.id)}>
                    <span className="row-title">{friendlyPackage(pkgOf(f))}</span>
                    <span className="row-sub">{st.kind === "failed" ? <span className="bad">{st.message}</span> : firstSentence(f.why)}</span>
                  </button>
                  {st.kind === "applied" && <span className="tag ok">Removed</span>}
                  {st.kind === "applying" && <span className="tag">Removing</span>}
                </li>
              );
            })}
          </ul>
        )}
      </section>

      {removed.length > 0 && (
        <section className="card">
          <h2>Removed by Forge</h2>
          <ul className="rows flush">
            {removed.map((h) => (
              <li key={h.id} className="row quiet">
                <span className="row-main static">
                  <span className="row-title">{h.step.op === "remove_appx" ? friendlyPackage(h.step.package_name) : h.title}</span>
                  <span className="row-sub">Removed {when(h.unix_ts)}. Reinstall it from the Microsoft Store if you need it.</span>
                </span>
              </li>
            ))}
          </ul>
        </section>
      )}
    </main>
  );
}

/** App removal can't be undone, so it always asks first. */
export function ConfirmRemove({ apps, onCancel, onConfirm }: { apps: Finding[]; onCancel: () => void; onConfirm: () => void }) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  useEffect(() => cancelRef.current?.focus(), []);
  const n = apps.length;
  return (
    <div className="scrim" onClick={onCancel}>
      <div className="dialog" role="dialog" aria-modal="true" aria-labelledby="confirm-title" onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => { if (e.key === "Escape") onCancel(); }}>
        <h2 id="confirm-title">Remove {n === 1 ? friendlyPackage(pkgOf(apps[0])) : `${n} apps`}?</h2>
        <ul className="dialog-list">{apps.map((f) => <li key={f.id}>{friendlyPackage(pkgOf(f))}</li>)}</ul>
        <p className="muted">They're removed for your account. Forge can't put them back, but you can reinstall any of them from the Microsoft Store.</p>
        <div className="dialog-actions">
          <button ref={cancelRef} className="btn ghost" onClick={onCancel}>Cancel</button>
          <button className="btn danger" onClick={onConfirm}>{n === 1 ? "Remove app" : `Remove ${n} apps`}</button>
        </div>
      </div>
    </div>
  );
}
