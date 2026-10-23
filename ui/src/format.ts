import type { Category, Finding, FixStep, HistoryStatus, Impact, Machine } from "./types";

export const LANES: { cats: Category[]; label: string; clear: string }[] = [
  { cats: ["gaming"], label: "Gaming", clear: "Display and Game Bar settings suit gaming." },
  { cats: ["startup"], label: "Startup", clear: "Nothing slows your sign-in." },
  { cats: ["memory"], label: "Memory", clear: "Memory runs at its rated speed." },
  { cats: ["storage"], label: "Storage", clear: "Drives have room and suit the job." },
  { cats: ["power"], label: "Power", clear: "Power settings suit this PC." },
  { cats: ["bloat"], label: "Preinstalled apps", clear: "No removable preinstalled apps found." },
  { cats: ["drivers", "thermal"], label: "Other", clear: "Nothing else to report." },
];

const BUILTIN_PLANS: Record<string, string> = {
  "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c": "High performance",
  "381b4222-f694-41f0-9685-ff5bb260df2e": "Balanced",
  "a1841308-3541-4fab-bc81-f71556f20b4a": "Power saver",
};

export function planName(guid: string, machine: Machine | null): string {
  return machine?.power_plans.find((p) => p.guid === guid)?.name ?? BUILTIN_PLANS[guid] ?? "another plan";
}

const SETTING_NAMES: Record<string, string> = {
  HistoricalCaptureEnabled: "Game Bar background recording",
  AutoGameModeEnabled: "Game Mode",
  SilentInstalledAppsEnabled: "automatic installs of promoted apps",
  SystemPaneSuggestionsEnabled: "suggestions in Start",
  "SubscribedContent-338388Enabled": "suggested apps in Start",
  "SubscribedContent-338389Enabled": "tips and tricks notifications",
  "SubscribedContent-353694Enabled": "suggested content in Settings",
  "SubscribedContent-353696Enabled": "suggested content in Settings",
};

const MOUSE_NAMES: Record<string, string> = {
  MouseSpeed: "mouse acceleration",
  MouseThreshold1: "the first acceleration threshold",
  MouseThreshold2: "the second acceleration threshold",
};

/** "Microsoft.BingNews" -> "Bing News", same rule as the engine's titles. */
export function friendlyPackage(pkg: string): string {
  const last = pkg.split(".").pop() ?? pkg;
  return last.replace(/([a-z])([A-Z])/g, "$1 $2");
}

export function describeStep(s: FixStep, machine: Machine | null = null): string {
  switch (s.op) {
    case "disable_startup_item": return `Stop ${s.name} from starting with Windows`;
    case "restore_startup_item": return `Let ${s.name} start with Windows again`;
    case "set_power_plan": return `Switch the power plan to ${planName(s.guid, machine)}`;
    case "set_registry_dword":
      return SETTING_NAMES[s.name] ? `Turn ${s.value ? "on" : "off"} ${SETTING_NAMES[s.name]}` : `Set ${s.name} to ${s.value}`;
    case "set_registry_string":
      return MOUSE_NAMES[s.name] ? `${s.value === "0" ? "Turn off" : "Set"} ${MOUSE_NAMES[s.name]}${s.value === "0" ? "" : ` to ${s.value}`}` : `Set ${s.name} to ${s.value}`;
    case "delete_registry_value":
      return SETTING_NAMES[s.name] ? `Put ${SETTING_NAMES[s.name]} back to the Windows default` : `Remove the ${s.name} setting`;
    case "remove_appx": return `Uninstall ${friendlyPackage(s.package_name)} for your account`;
  }
}

export function describeImpact(i: Impact): string[] {
  const out: string[] = [];
  if (i.boot_ms > 0) out.push(`${(i.boot_ms / 1000).toFixed(1)} s faster startup`);
  if (i.idle_ram_mb > 0) out.push(`${i.idle_ram_mb} MB less RAM in use`);
  if (i.idle_cpu_pct > 0) out.push(`${i.idle_cpu_pct.toFixed(1)}% less idle CPU`);
  return out;
}

/** Display-only weight for lane bars: the engine's score, with a floor so every lane with findings shows. */
export function barWeight(f: Finding): number {
  return Math.max(0.15, f.score);
}

export const RISK_LABEL = { safe: "Low risk", caution: "Check first", expert: "Expert change" } as const;

export const STATUS_LABEL: Record<HistoryStatus, string> = {
  applied: "Applied",
  undone: "Undone",
  failed: "Failed, nothing changed",
  interrupted: "Stopped midway, may have applied",
};

export function when(unixTs: number): string {
  const d = new Date(unixTs * 1000);
  const today = new Date();
  const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  return d.toDateString() === today.toDateString() ? `Today ${time}` : `${d.toLocaleDateString()} ${time}`;
}
