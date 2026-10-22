import type { Category, Finding, FixStep, Impact } from "./types";

export const LANES: { cat: Category; label: string; clear: string }[] = [
  { cat: "startup", label: "Startup", clear: "Nothing slows your sign-in." },
  { cat: "bloat", label: "Preinstalled apps", clear: "No removable preinstalled apps found." },
  { cat: "power", label: "Power", clear: "Power settings suit this PC." },
  { cat: "memory", label: "Memory", clear: "Memory runs at its rated speed." },
  { cat: "storage", label: "Storage", clear: "Drives have room and suit the job." },
];

const PLANS: Record<string, string> = {
  "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c": "High performance",
  "381b4222-f694-41f0-9685-ff5bb260df2e": "Balanced",
  "a1841308-3541-4fab-bc81-f71556f20b4a": "Power saver",
};

export function describeStep(s: FixStep): string {
  switch (s.op) {
    case "disable_startup_item": return `Stop ${s.name} from starting with Windows`;
    case "set_power_plan": return `Switch the power plan to ${PLANS[s.guid] ?? "another plan"}`;
    case "remove_appx": return `Uninstall ${s.package_name}`;
    case "set_registry_dword": return "Change one Windows setting";
  }
}

export function describeImpact(i: Impact): string[] {
  const out: string[] = [];
  if (i.boot_ms >= 1000) out.push(`${(i.boot_ms / 1000).toFixed(1)} s faster startup`);
  else if (i.boot_ms > 0) out.push(`${(i.boot_ms / 1000).toFixed(1)} s faster startup`);
  if (i.idle_ram_mb > 0) out.push(`${i.idle_ram_mb} MB less RAM in use`);
  if (i.idle_cpu_pct > 0) out.push(`${i.idle_cpu_pct.toFixed(1)}% less idle CPU`);
  return out;
}

/** Display-only weight for lane bars. Ranking itself happens in the Rust engine. */
export function barWeight(f: Finding): number {
  return Math.max(0.25, f.impact.boot_ms / 1000 + f.impact.idle_ram_mb / 200 + f.impact.idle_cpu_pct * 2);
}

export const RISK_LABEL = { safe: "Low risk", caution: "Check first", expert: "Expert change" } as const;
