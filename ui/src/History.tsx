import { type Change, STATUS_LABEL, changeTitle, describeStep, when } from "./format";
import { Check, Clock } from "./Icons";
import type { Machine } from "./types";

interface Props {
  changes: Change[];
  error: string | null;
  undoing: Record<number, string>;
  machine: Machine | null;
  onUndo: (c: Change) => void;
}

/** Every change Forge made, newest first, read from the helper's journal. Steps applied together
 *  (one fix) are one change here, and undo together. */
export default function History({ changes, error, undoing, machine, onUndo }: Props) {
  return (
    <main className="content">
      <section className="card">
        <h2>Every change Forge made</h2>
        <p className="tiny muted">Newest first. Undo puts back exactly what was there before.</p>
        {error && <p className="bad" role="alert">{error}</p>}
        {changes.length === 0 && !error ? (
          <p className="empty-state"><Clock /> No changes yet. Forge hasn't changed anything on this PC.</p>
        ) : (
          <ul className="rows flush">
            {changes.map((c) => {
              const state = undoing[c.key];
              const steps = c.items.length > 1 ? ` · ${c.items.length} settings` : "";
              const isRemoval = c.first.step.op === "remove_appx";
              return (
                <li key={c.key} className="row quiet">
                  <span className={`row-icon ${c.status === "applied" ? "done" : "muted-icon"}`}><Check /></span>
                  <span className="row-main static">
                    <span className="row-title">{changeTitle(c, machine)}</span>
                    <span className="row-sub" title={c.items.map((h) => describeStep(h.step, machine)).join("\n")}>
                      {when(c.first.unix_ts)}{steps}
                      {isRemoval && c.status === "applied" && " · Reinstall from the Microsoft Store if needed"}
                    </span>
                    {state && state !== "busy" && <span className="row-sub bad" role="alert">{state}</span>}
                  </span>
                  <span className={`status ${c.status}`}>{STATUS_LABEL[c.status]}</span>
                  {c.undoable && (
                    <button className="btn ghost small" onClick={() => onUndo(c)} disabled={state === "busy"}>
                      {state === "busy" ? "Undoing" : "Undo"}
                    </button>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </section>
    </main>
  );
}
