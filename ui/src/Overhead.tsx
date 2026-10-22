import { useEffect, useState } from "react";
import { selfStats } from "./api";
import type { SelfStats } from "./types";

/** Forge reports its own cost. Polls every 2 s, and only while the window is visible. */
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
  if (!s) return <span className="overhead" />;
  return (
    <span className="overhead" title="What Forge itself is using right now">
      Forge is using {s.rss_mb.toFixed(0)} MB and {s.cpu_pct.toFixed(1)}% CPU
    </span>
  );
}
