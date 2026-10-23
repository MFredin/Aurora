import { STATUS_LABEL, describeStep, when } from "./format";
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
    <main className="history">
      <div className="history-inner">
        <h2>History</h2>
        <p className="muted">Every change Forge made on this PC, newest first. Undo puts back exactly what was there before.</p>
        {error && <p className="error" role="alert">{error}</p>}
        {items.length === 0 && !error ? (
          <p className="empty-box">No changes yet. Forge hasn't changed anything on this PC.</p>
        ) : (
          <ul className="history-list">
            {items.map((h) => {
              const state = undoing[h.id];
              return (
                <li key={h.id}>
                  <span className="when">{when(h.unix_ts)}</span>
                  <div className="what">
                    <div>{h.title}</div>
                    <div className="muted small">{describeStep(h.step, machine)}</div>
                    {state && state !== "busy" && <div className="error small" role="alert">{state}</div>}
                  </div>
                  <span className={`status ${h.status}`}>{STATUS_LABEL[h.status]}</span>
                  {!h.undoable && h.status === "applied" && h.step.op === "remove_appx" && (
                    <span className="muted small">Reinstall it from the Microsoft Store if you need it</span>
                  )}
                  {h.undoable && (
                    <button className="btn" onClick={() => onUndo(h)} disabled={state === "busy"}>
                      {state === "busy" ? "Undoing" : "Undo"}
                    </button>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </main>
  );
}
