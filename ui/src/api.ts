import { Channel, invoke } from "@tauri-apps/api/core";
import type { ScanEvent, SelfStats } from "./types";
import sample from "./sample.json";

const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Streams scan events. In a plain browser (npm run dev) it replays engine-generated sample data. */
export async function scan(onEvent: (e: ScanEvent) => void): Promise<void> {
  if (inTauri) {
    const channel = new Channel<ScanEvent>();
    channel.onmessage = onEvent;
    return invoke("scan", { onEvent: channel });
  }
  await sleep(500);
  onEvent({ event: "machine", data: sample.machine as never });
  for (const f of sample.findings) {
    await sleep(380);
    onEvent({ event: "finding", data: f as never });
  }
  await sleep(300);
  onEvent({ event: "done", data: { count: sample.findings.length } });
}

export async function applyFix(findingId: string): Promise<void> {
  if (inTauri) return invoke("apply_fix", { findingId });
  await sleep(650);
}

export async function selfStats(): Promise<SelfStats> {
  if (inTauri) return invoke("self_stats");
  return { rss_mb: 41 + Math.random() * 3, cpu_pct: Math.random() * 0.3 };
}
