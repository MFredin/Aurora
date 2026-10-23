import { useEffect, useRef } from "react";
import type { Status } from "./App";
import { openLink } from "./api";
import { RISK_LABEL, describeImpact, describeStep, kindOf } from "./format";
import { Close } from "./Icons";
import type { Finding, Machine } from "./types";

interface Props {
  f: Finding;
  machine: Machine | null;
  status: Status;
  onApply: () => void;
  onRemove: () => void;
  onClose: () => void;
}

/** Everything about one finding: why, the evidence, exactly what changes, and whether it can be undone. */
export default function Detail({ f, machine, status, onApply, onRemove, onClose }: Props) {
  const closeRef = useRef<HTMLButtonElement>(null);
  useEffect(() => closeRef.current?.focus(), [f.id]);
  const impact = describeImpact(f.impact);
  const kind = kindOf(f);
  const busy = status.kind === "applying" || status.kind === "applied";

  return (
    <div className="scrim side" onClick={onClose}>
      <aside className="drawer" role="dialog" aria-modal="true" aria-labelledby="detail-title" onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => { if (e.key === "Escape") onClose(); }}>
        <div className="drawer-head">
          <span className={`pill ${f.risk}`}>{f.fix ? RISK_LABEL[f.risk] : "Advice"}</span>
          <button ref={closeRef} className="icon-btn" aria-label="Close" onClick={onClose}><Close /></button>
        </div>
        <h2 id="detail-title">{f.title}</h2>
        <p>{f.why}</p>
        {impact.length > 0 && (
          <p className="estimate"><strong>Rough estimate.</strong> {impact.join(", ")}. A typical figure, not a measurement of this PC.</p>
        )}
        <h3>What Forge found</h3>
        <ul className="evidence">{f.evidence.map((e) => <li key={e}><code>{e}</code></li>)}</ul>
        {f.fix ? (
          <>
            <h3>What will change</h3>
            <ul className="steps">{f.fix.steps.map((st, i) => <li key={i}>{describeStep(st, machine)}</li>)}</ul>
            <p className={f.fix.reversible ? "muted" : "warn-text"}>
              {f.fix.reversible ? "You can undo this from History." : "Forge can't undo this. You can reinstall the app from the Microsoft Store."}
            </p>
            {f.advice && <p className="muted"><LinkText text={f.advice} /></p>}
            <button className={`btn ${kind === "app" ? "danger" : "primary"} wide`} disabled={busy} onClick={kind === "app" ? onRemove : onApply}>
              {status.kind === "applying" ? (kind === "app" ? "Removing" : "Applying")
                : status.kind === "applied" ? (kind === "app" ? "Removed" : "Applied")
                : status.kind === "failed" ? "Try again"
                : kind === "app" ? "Remove app" : "Apply fix"}
            </button>
            {status.kind === "failed" && <p className="bad" role="alert">{status.message}</p>}
            <p className="tiny muted">The first change in a session shows one Windows permission prompt, and Forge asks Windows for a restore point.</p>
          </>
        ) : (
          f.advice && (
            <>
              <h3>What to do</h3>
              <p className="advice-box"><LinkText text={f.advice} /></p>
              <p className="muted">Forge doesn't change this for you.</p>
            </>
          )
        )}
      </aside>
    </div>
  );
}

/** Advice text with its web address shown as a link that opens in the browser. */
function LinkText({ text }: { text: string }) {
  const parts = text.split(/(https:\/\/[^\s]+?)(?=\.?(?:\s|$))/);
  return (
    <>
      {parts.map((part, i) => part.startsWith("https://")
        ? <a key={i} href={part} className="ext-link" onClick={(e) => { e.preventDefault(); openLink(part).catch(() => {}); }}>{part}</a>
        : part)}
    </>
  );
}
