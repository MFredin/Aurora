import { useEffect, useState } from "react";
import { selfStats } from "./api";
import type { SelfStats } from "./types";

/** Forge reports its own cost (private memory, like Task Manager), including its WebView2 processes.
 * Polls every 2 s, only while visible; the shell marks the webview hidden when minimized. */
export default function Overhead() {
  const [s, setS] = useState<SelfStats | null>(null);
  useEffect(() => {
    let timer: number | undefined;
    const tick = () => selfStats().then(setS).catch(() => {});
    const start = () => { if (timer === undefined) { tick(); timer = window.setInterval(tick, 2000); } };
    const stop = () => { if (timer !== undefined) { clearInterval(timer); timer = undefined; } };
    const onVis = () => (document.visibilityState === "visible" ? start() : stop());
    onVis();
    document.addEventListener("visibilitychange", onVis);
    return () => { stop(); document.removeEventListener("visibilitychange", onVis); };
  }, []);
  if (!s) return <span className="chip meter" />;
  return (
    <span className="chip meter" title={`Forge and its ${s.processes} window processes, as Task Manager's Memory column counts them`}>
      Forge {s.memory_mb.toFixed(0)} MB · {s.cpu_pct.toFixed(1)}% CPU
    </span>
  );
}
