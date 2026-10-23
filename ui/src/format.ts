import type { Finding, FixStep, HistoryStatus, Impact, Machine } from "./types";

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

export type Kind = "app" | "setting" | "advice";

/** How a finding is acted on: removed (app), switched (setting) or explained (advice). */
export function kindOf(f: Finding): Kind {
  if (!f.fix) return "advice";
  return f.fix.steps.some((s) => s.op === "remove_appx") ? "app" : "setting";
}

/** First sentence of a finding's reason, for one-line row subtitles. */
export function firstSentence(text: string): string {
  const sentences = text.match(/[^.!?]+[.!?]+(\s|$)/g) ?? [text];
  const first = sentences[0].trim();
  // "Your choice." alone says nothing; take the next sentence too.
  return first.length < 30 && sentences[1] ? `${first} ${sentences[1].trim()}` : first;
}

/**
 * The switch a setting row shows: the setting's current state, and the words for each position.
 * Applying the fix moves the switch to the other position.
 */
export function switchFor(f: Finding): { on: boolean; onText: string; offText: string } {
  if (f.id === "gaming:game-mode") return { on: false, onText: "On", offText: "Off" };
  if (f.id === "power:plan") return { on: true, onText: "Balanced", offText: "High performance" };
  return { on: true, onText: "On", offText: "Off" };
}

/** Short row titles for settings, phrased as the thing the switch controls. */
export function settingTitle(f: Finding): string {
  if (f.id.startsWith("startup:")) return f.title;
  const titles: Record<string, string> = {
    "gaming:dvr": "Game Bar background recording",
    "gaming:game-mode": "Game Mode",
    "gaming:mouse-accel": "Mouse acceleration",
    "bloat:suggestions": "Promoted apps and suggestions",
    "power:plan": "Power plan",
  };
  return titles[f.id] ?? f.title;
}
