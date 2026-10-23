import type { ReactNode } from "react";
import type { Status } from "./App";
import { RISK_LABEL, firstSentence, settingTitle, switchFor } from "./format";
import { Bolt, Chip, Disk, Monitor, Mouse, Pad, Power, Record, Restart, Store } from "./Icons";
import type { Finding } from "./types";

export function iconFor(f: Finding): ReactNode {
  if (f.id.startsWith("startup:")) return <Power />;
  if (f.id === "gaming:dvr") return <Record />;
  if (f.id === "gaming:mouse-accel") return <Mouse />;
  if (f.id === "gaming:game-mode" || f.id === "gaming:windowed") return <Pad />;
  if (f.id === "bloat:suggestions") return <Store />;
  if (f.category === "power") return <Bolt />;
  if (f.category === "memory" || f.id === "gaming:hags") return <Chip />;
  if (f.category === "storage") return <Disk />;
  if (f.category === "system") return <Restart />;
  return <Monitor />;
}

export function Ring({ open, done, scanning, idle = false, size = 150 }: { open: number; done: number; scanning: boolean; idle?: boolean; size?: number }) {
  const c = 2 * Math.PI * 82;
  const frac = open + done === 0 ? 0 : done / (open + done);
  return (
    <svg className={`ring${scanning ? " is-scanning" : ""}`} width={size} height={size} viewBox="0 0 200 200" role="img"
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

/** A Fluent-style settings row: icon, title, one-line reason, risk, and a switch that applies or undoes. */
export function SettingRow({ f, st, compact = false, onToggle, onDetail }: { f: Finding; st: Status; compact?: boolean; onToggle: () => void; onDetail: () => void }) {
  const sw = switchFor(f);
  const on = st.kind === "applied" ? !sw.on : sw.on;
  return (
    <li className={`row${compact ? " compact" : ""}`}>
      <span className={`row-icon ${f.category}`}>{iconFor(f)}</span>
      <button className="row-main" onClick={onDetail}>
        <span className="row-title">{settingTitle(f)}</span>
        <span className="row-sub">{st.kind === "failed" ? <span className="bad">{st.message}</span> : firstSentence(f.why)}</span>
      </button>
      {!compact && <span className={`pill ${f.risk}`}>{RISK_LABEL[f.risk]}</span>}
      <span className="switch-text">{st.kind === "applying" ? "Changing" : on ? sw.onText : sw.offText}</span>
      <button role="switch" aria-checked={on} aria-label={settingTitle(f)} className={`switch${on ? " on" : ""}`}
        disabled={st.kind === "applying"} onClick={onToggle}><span /></button>
    </li>
  );
}
