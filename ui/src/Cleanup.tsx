import { useEffect, useRef } from "react";
import type { Phase, Status } from "./App";
import { firstSentence, friendlyPackage, gb } from "./format";
import { Disk, Trash } from "./Icons";
import type { Finding, HistoryItem, Machine } from "./types";

const pkgOf = (f: Finding) => (f.fix?.steps[0]?.op === "remove_appx" ? f.fix.steps[0].package_name : f.id);

interface Props {
  phase: Phase;
  machine: Machine | null;
  apps: Finding[];
  status: Record<string, Status>;
  picked: string[];
  history: HistoryItem[];
  cleaning: Record<string, string>;
  onPick: (id: string, on: boolean) => void;
  onPickAll: (ids: string[]) => void;
  onRemove: (ids: string[]) => void;
  onClean: (what: "temp" | "recycle") => void;
  onDetail: (id: string) => void;
  onScan: () => void;
}

/** Temporary files, the Recycle Bin, and preinstalled apps Forge's database marks safe to remove. */
export default function Cleanup(p: Props) {
  const removable = p.apps.filter((f) => !["applied", "applying"].includes(p.status[f.id]?.kind ?? ""));
  const removed = p.history.filter((h) => h.step.op === "remove_appx" && h.status === "applied");
  const allPicked = removable.length > 0 && removable.every((f) => p.picked.includes(f.id));
  const m = p.machine;

  if (p.phase === "idle" || !m) {
    return (
      <main className="content">
        <section className="card">
          <p className="muted">{p.phase === "scanning" ? "Measuring what can be cleaned up." : <>Scan this PC to see what can be cleaned up. <button className="link" onClick={p.onScan}>Scan now</button></>}</p>
        </section>
      </main>
    );
  }

  const space = (what: "temp" | "recycle", title: string, sub: string, bytes: number | null, action: string) => {
    const st = p.cleaning[what];
    return (
      <li className="row">
        <span className="row-icon storage">{what === "temp" ? <Disk /> : <Trash />}</span>
        <span className="row-main static">
          <span className="row-title">{title} <span className="muted">· {gb(bytes)}</span></span>
          <span className="row-sub">{st && st !== "busy" ? <span className="bad">{st}</span> : sub}</span>
        </span>
        <button className="btn small" disabled={!bytes || st === "busy"} onClick={() => p.onClean(what)}>
          {st === "busy" ? "Cleaning" : bytes ? action : "Nothing to clean"}
        </button>
      </li>
    );
  };

  return (
    <main className="content">
      <section aria-label="Free up space">
        <div className="section-head">
          <h2>Free up space</h2>
          <span className="tiny muted">These can't be undone. Only files older than a day are removed, and files in use are skipped.</span>
        </div>
        <ul className="rows">
          {space("temp", "Temporary files", "Left behind by installers, updates and apps in your temp folder.", m.temp_bytes, "Clean up")}
          {space("recycle", "Recycle Bin", "Files you deleted. Emptying it removes them for good.", m.recycle_bin_bytes, "Empty")}
        </ul>
      </section>

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
        {p.apps.length === 0 ? (
          <p className="muted">{p.phase === "scanning" ? "Checking installed apps." : "No removable preinstalled apps found."}</p>
        ) : (
          <div className="app-grid">
            {p.apps.map((f) => {
              const st = p.status[f.id] ?? { kind: "idle" };
              const picked = p.picked.includes(f.id);
              return (
                <label key={f.id} className={`app-pick${picked ? " is-picked" : ""}${st.kind === "applied" ? " is-done" : ""}`} title={firstSentence(f.why)}>
                  <input type="checkbox" checked={picked} disabled={st.kind === "applied" || st.kind === "applying"}
                    onChange={(e) => p.onPick(f.id, e.target.checked)} />
                  <span>{friendlyPackage(pkgOf(f))}</span>
                  {st.kind === "applied" && <span className="tag ok">Removed</span>}
                  {st.kind === "applying" && <span className="tag">Removing</span>}
                  {st.kind === "failed" && <button className="tag bad link" onClick={() => p.onDetail(f.id)}>Failed</button>}
                </label>
              );
            })}
          </div>
        )}
        {removed.length > 0 && (
          <p className="tiny muted note">Removed by Forge: {removed.map((h) => (h.step.op === "remove_appx" ? friendlyPackage(h.step.package_name) : h.title)).join(", ")}. Reinstall any of them from the Microsoft Store.</p>
        )}
      </section>
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
