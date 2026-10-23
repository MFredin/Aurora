import { STATUS_LABEL, describeStep, when } from "./format";
import { Check, Clock } from "./Icons";
import type { HistoryItem, Machine } from "./types";

interface Props {
  items: HistoryItem[];
  error: string | null;
  undoing: Record<number, string>;
  machine: Machine | null;
  onUndo: (item: HistoryItem) => void;
}

/** Every change Forge made, newest first, read from the helper's journal. */
export default function History({ items, error, undoing, machine, onUndo }: Props) {
  return (
    <main className="content">
      <section className="card">
        <h2>Every change Forge made</h2>
        <p className="tiny muted">Newest first. Undo puts back exactly what was there before.</p>
        {error && <p className="bad" role="alert">{error}</p>}
        {items.length === 0 && !error ? (
          <p className="empty-state"><Clock /> No changes yet. Forge hasn't changed anything on this PC.</p>
        ) : (
          <ul className="rows flush">
            {items.map((h) => {
              const state = undoing[h.id];
              return (
                <li key={h.id} className="row quiet">
                  <span className={`row-icon ${h.status === "applied" ? "done" : "muted-icon"}`}><Check /></span>
                  <span className="row-main static">
                    <span className="row-title">{h.title}</span>
                    <span className="row-sub">
                      {when(h.unix_ts)} · {describeStep(h.step, machine)}
                      {!h.undoable && h.status === "applied" && h.step.op === "remove_appx" && " · Reinstall from the Microsoft Store if needed"}
                    </span>
                    {state && state !== "busy" && <span className="row-sub bad" role="alert">{state}</span>}
                  </span>
                  <span className={`status ${h.status}`}>{STATUS_LABEL[h.status]}</span>
                  {h.undoable && (
                    <button className="btn ghost small" onClick={() => onUndo(h)} disabled={state === "busy"}>
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
