// Mirrors forge-core (serde, snake_case). Keep in sync with the Rust side.
export type Category = "startup" | "bloat" | "power" | "memory" | "storage" | "drivers" | "thermal" | "gaming" | "system";
export type Risk = "safe" | "caution" | "expert";
export type Confidence = "low" | "medium" | "high";

export interface Impact { boot_ms: number; idle_ram_mb: number; idle_cpu_pct: number }

export type StartupLocation = "hkcu_run" | "hklm_run" | "hklm_run32" | "startup_folder_user" | "startup_folder_common";

export type FixStep =
  | { op: "disable_startup_item"; name: string; location: StartupLocation }
  | { op: "restore_startup_item"; name: string; location: StartupLocation; prior: number[] | null }
  | { op: "set_power_plan"; guid: string }
  | { op: "set_power_mode"; guid: string }
  | { op: "set_registry_dword"; hive: string; key: string; name: string; value: number }
  | { op: "set_registry_string"; hive: string; key: string; name: string; value: string }
  | { op: "delete_registry_value"; hive: string; key: string; name: string }
  | { op: "remove_appx"; package_name: string };

export interface Finding {
  id: string;
  category: Category;
  title: string;
  why: string;
  evidence: string[];
  impact: Impact;
  risk: Risk;
  confidence: Confidence;
  fix: { steps: FixStep[]; reversible: boolean } | null;
  advice: string | null;
  priority: number;
  score: number;
}

export interface PowerPlan { guid: string; name: string }

export interface Display { name: string; width: number; height: number; current_hz: number; max_hz: number }

export interface StartupItem {
  name: string;
  command: string;
  location: StartupLocation;
  enabled: boolean;
  protected: boolean;
  note: string | null;
}

export interface ProcessUse { name: string; memory_mb: number }

export interface Machine {
  os: string;
  is_laptop: boolean;
  on_battery: boolean;
  cpu_name: string;
  cpu_threads: number;
  ram_total_mb: number;
  ram_configured_mts: number | null;
  ram_rated_mts: number | null;
  displays: Display[];
  gpus: string[];
  power_plans: PowerPlan[];
  ram_used_mb: number;
  startup: StartupItem[];
  temp_bytes: number | null;
  recycle_bin_bytes: number | null;
  top_memory: ProcessUse[];
  uptime_hours: number;
}

export interface Tally { files: number; bytes: number }

export type ScanEvent =
  | { event: "machine"; data: Machine }
  | { event: "finding"; data: Finding }
  | { event: "done"; data: { order: string[] } };

export interface SelfStats { memory_mb: number; cpu_pct: number; processes: number }

export interface ApplyResult { restore_point: string | null; warning: string | null }

export type HistoryStatus = "applied" | "failed" | "interrupted" | "undone";

export interface HistoryItem {
  id: number;
  unix_ts: number;
  finding_id: string;
  title: string;
  step: FixStep;
  status: HistoryStatus;
  undoable: boolean;
  group: number | null;
}
