// Mirrors forge-core::model (serde, snake_case). Keep in sync with the Rust side.
export type Category = "startup" | "bloat" | "power" | "memory" | "storage" | "drivers" | "thermal" | "gaming";
export type Risk = "safe" | "caution" | "expert";
export type Confidence = "low" | "medium" | "high";

export interface Impact { boot_ms: number; idle_ram_mb: number; idle_cpu_pct: number }

export type FixStep =
  | { op: "disable_startup_item"; name: string; location: string }
  | { op: "set_power_plan"; guid: string }
  | { op: "remove_appx"; package_name: string }
  | { op: "set_registry_dword"; hive: string; key: string; name: string; value: number };

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
}

export interface Machine {
  os: string;
  cpu_name: string;
  cpu_threads: number;
  ram_total_mb: number;
  gpus: string[];
}

export type ScanEvent =
  | { event: "machine"; data: Machine }
  | { event: "finding"; data: Finding }
  | { event: "done"; data: { count: number } };

export interface SelfStats { rss_mb: number; cpu_pct: number }
