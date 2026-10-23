import type { Phase } from "./App";
import { Lock, Power } from "./Icons";
import type { Machine, StartupLocation } from "./types";

interface Props {
  phase: Phase;
  machine: Machine | null;
  busy: Record<number, string>;
  onToggle: (index: number, enabled: boolean) => void;
  onScan: () => void;
}

const WHERE: Record<StartupLocation, string> = {
  hkcu_run: "Your account",
  hklm_run: "All users",
  hklm_run32: "All users (32-bit)",
  startup_folder_user: "Startup folder",
  startup_folder_common: "Startup folder, all users",
};

/** Every app that starts with Windows, with a switch each. Same effect as Task Manager's Startup apps. */
export default function Startup({ phase, machine, busy, onToggle, onScan }: Props) {
  if (!machine) {
    return (
      <main className="content">
        <section className="card">
          <p className="muted">
            {phase === "scanning" ? "Reading startup apps." : <>Scan this PC to list the apps that start with Windows. <button className="link" onClick={onScan}>Scan now</button></>}
          </p>
        </section>
      </main>
    );
  }
  // Index into the scan is what the shell acts on, so keep it alongside each item.
  const items = machine.startup.map((it, index) => ({ it, index }));
  const sorted = [...items.filter((x) => !x.it.protected), ...items.filter((x) => x.it.protected)];
  const on = machine.startup.filter((i) => i.enabled).length;

  return (
    <main className="content">
      <div className="section-head">
        <h2>{on} of {machine.startup.length} start with Windows</h2>
        <span className="tiny muted">Switched-off apps still work; they just don't open at sign-in. Every change is in History.</span>
      </div>
      <ul className="rows">
        {sorted.map(({ it, index }) => {
          const state = busy[index];
          return (
            <li key={`${it.location}:${it.name}`} className={`row${it.protected ? " quiet" : ""}`}>
              <span className="row-icon startup"><Power /></span>
              <span className="row-main static">
                <span className="row-title">{it.name}</span>
                <span className="row-sub">
                  {state && state !== "busy" ? <span className="bad">{state}</span>
                    : it.note ?? <span className="mono">{it.command || "No command recorded"}</span>}
                </span>
              </span>
              <span className="tag">{WHERE[it.location]}</span>
              {it.protected ? (
                <span className="locked" title="Windows, security or driver component. Forge leaves it on."><Lock /> Kept on</span>
              ) : (
                <>
                  <span className="switch-text">{state === "busy" ? "Changing" : it.enabled ? "On" : "Off"}</span>
                  <button role="switch" aria-checked={it.enabled} aria-label={`${it.name} starts with Windows`}
                    className={`switch${it.enabled ? " on" : ""}`} disabled={state === "busy"}
                    onClick={() => onToggle(index, !it.enabled)}><span /></button>
                </>
              )}
            </li>
          );
        })}
      </ul>
    </main>
  );
}
