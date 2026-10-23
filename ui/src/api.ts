import { Channel, invoke } from "@tauri-apps/api/core";
import type { ApplyResult, Finding, HistoryItem, ScanEvent, SelfStats } from "./types";
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
  const fixed = new Set(mockHistory.filter((h) => h.status === "applied").map((h) => h.finding_id));
  const findings = (sample.findings as Finding[]).filter((f) => !fixed.has(f.id));
  for (const f of findings) {
    await sleep(320);
    onEvent({ event: "finding", data: f });
  }
  await sleep(250);
  onEvent({ event: "done", data: { order: sample.order.filter((id) => !fixed.has(id)) } });
}

export async function applyFix(findingId: string): Promise<ApplyResult> {
  if (inTauri) return invoke("apply_fix", { findingId });
  await sleep(650);
  const f = (sample.findings as Finding[]).find((x) => x.id === findingId);
  if (!f?.fix) throw "That finding has no automatic fix. Follow the advice shown instead.";
  const first = mockHistory.length === 0;
  mockHistory.unshift({
    id: Date.now(), unix_ts: Math.floor(Date.now() / 1000), finding_id: f.id, title: f.title,
    step: f.fix.steps[0], status: "applied", undoable: f.fix.reversible,
  });
  return {
    restore_point: first ? "Restore point requested. Windows makes at most one a day, so an earlier one may be reused." : null,
    warning: null,
  };
}

export async function undo(journalId: number): Promise<void> {
  if (inTauri) return invoke("undo", { journalId });
  await sleep(500);
  const h = mockHistory.find((x) => x.id === journalId);
  if (!h || !h.undoable) throw "That change cannot be undone.";
  h.status = "undone";
  h.undoable = false;
}

export async function history(): Promise<HistoryItem[]> {
  if (inTauri) return invoke("history");
  return mockHistory.map((h) => ({ ...h }));
}

export async function selfStats(): Promise<SelfStats> {
  if (inTauri) return invoke("self_stats");
  return { memory_mb: 70 + Math.random() * 3, cpu_pct: Math.random() * 0.2, processes: 7 };
}

/** Tells the shell the first frame is on screen (cold-start measurement; no-op unless measuring). */
export function uiReady(): void {
  if (inTauri) invoke("ui_ready").catch(() => {});
}

const mockHistory: HistoryItem[] = [];
