# Forge: design

## 1. What it does

Scan, explain, fix, prove. Forge reads the machine, ranks what is worth changing, shows exactly what
each change does, applies it reversibly, and (from v0.2) measures the before and after.

Principles that decide arguments:
1. **Never be the bloat.** Forge's own cost is a feature, and the UI shows it.
2. **Explain or don't change.** Every finding has a plain reason and its raw evidence.
3. **Reversible by default.** If a step can't be undone, the UI says so before the click.
4. **Honest about hardware.** If the fix is a BIOS setting or an SSD, say that. Don't fake a software fix.

## 2. Architecture

```
 UI (React, WebView2)            unprivileged
   │  invoke("scan") / ("apply_fix", finding id) / ("undo", journal id)   ← ids only
 Tauri shell (Rust)              unprivileged   holds last scan, resolves id → FixPlan
   ├─ forge-scan   collectors → Snapshot
   ├─ forge-core   rules(Snapshot) → Findings, journal reader, allowlist
   │  named pipe (random name, one instance, client PID checked), UAC-launched
 forge-helper (Rust)             elevated, started on first apply, exits after 5 min idle
      validate(step) → re-check live system → journal intent → apply → journal outcome
      undo(journal id) → inverse computed from its own journal → same path
```

Why this split: the webview and the shell never hold admin rights. A compromised UI can only name a
finding from the last scan or a journal entry, and the helper only accepts a closed set of typed
operations (`FixStep`), never a shell string. See ADR-002, ADR-007.

## 3. Performance budget

These are targets. The first milestone is to measure them and fail CI when they regress.

| Budget | Target | How |
|---|---|---|
| Idle CPU | ~0% | No resident service, no startup entry by default, no timers while hidden |
| Idle memory | Measure first (see risk below) | Rust backend, system WebView2 runtime, no bundled browser |
| Scan impact | Must not hitch a running game | Scan thread runs in Windows background mode (low CPU, I/O and memory priority) |
| Scan time | A few seconds | Registry reads and one SMBIOS call. No WMI, no PowerShell, no process spawns except `powercfg` |
| Installer | Small | Tauri with `opt-level="s"`, LTO, stripped. WebView2 bootstrapper instead of a bundled runtime |
| UI | 60 fps on integrated graphics | Animations use transform and opacity only. One looping animation (scan beam), removed under reduced motion. No web fonts: system UI font |

Rules for contributors:
- Collectors read, they never shell out to PowerShell or WMI. If WMI is unavoidable, call it once per scan.
- Nothing polls while the window is hidden (see `Overhead.tsx`).
- No new frontend dependency without a bundle-size check. Currently React only (76 KB gzipped JS).
- The helper exits after 5 minutes idle or when the app closes (ADR-007). It must never be left running.

**Biggest risk:** WebView2 runs as several `msedgewebview2.exe` processes that count toward real
memory. If measured idle memory misses the budget, fall back to a native UI (egui or WinUI). The
`forge-core` and `forge-scan` crates are UI-agnostic for exactly this reason.

Planned: EcoQoS for the app process while idle, and a self-profiling test that launches the app,
waits, and asserts on RSS and CPU.

## 4. Safety model

- **Allowlist:** `ipc::validate` permits power plan GUIDs, one Game Bar registry value with values
  0/1, and startup item names without shell metacharacters that don't match a `keep` entry. The
  helper also re-reads the item's live command line and refuses protected components. Defender and
  other security settings are not on the list and never will be.
- **Journal (write-ahead):** `%ProgramData%\Forge\journal.jsonl`, in a folder only administrators
  can write. An intent with the prior state is written before every change; no intent, no change.
  `journal::inverse` produces the undo step. History shows applied, failed, interrupted and undone.
- **Restore point:** requested once per session before the first change (Windows may reuse one
  from the last 24 hours, or System Restore may be off; the UI says which).
- **One irreversible operation:** app removal, limited to apps the DB marks safe, labelled before the click, never batched (ADR-014).
- **Advice-only findings:** firmware (XMP/EXPO), hardware (SSD, second DIMM) and display settings.
- **Context rules:** no High performance plan on a laptop on battery, on dual-CCD X3D CPUs, or where
  the plan doesn't exist.
- **Install:** per-machine, so the elevated helper binary is admin-writable only (ADR-012).

## 5. Rules and data

A rule is a pure function `(Snapshot, BloatDb) → Vec<Finding>` (`rules.rs`). To add one: write the
rule, register it in `default_rules()`, add a test in `tests/engine.rs`. Ranking is
`benefit × confidence × risk discount` (`Finding::score`).

`data/bloat.json` is **seed data, not curated** (`"curated": false`). Each entry is verdict `safe`,
`caution` or `keep`. `keep` items (security components, GPU and audio drivers) are never flagged.
Curating and testing this list on real machines is the long-term moat and ongoing cost.

## 6. UI concept

Telemetry bench. The machine reads as a spec sheet across the top. Findings fall into lanes (Gaming, Startup,
Memory, Storage, Power, Preinstalled apps); each lane has a bar showing its share of estimated gain.
During a scan a single amber beam sweeps the lanes and results stream in. Selecting a finding opens
its detail: why, what Forge found, exactly what will change, whether it can be undone.

- Palette: petrol `#0A1A21`, panel `#10252E`, bone text `#E6EEF0`, amber `#FFB44D` for gain and
  action, teal `#3DDBC0` for safe and applied, violet `#A78BFA` for expert changes.
- Type: the Windows UI font (Segoe UI Variable) for speed and native feel; tabular numerals.
- Risk is encoded by shape as well as color (filled circle, ring, diamond).
- Copy: sentence case, plain verbs. One name per action ("Apply fix", then "Applied").
- Viewed in Chromium in every state; not yet in WebView2 at 125%/150% scaling or high contrast.

## 6b. Not in the MVP on purpose

Registry cleaner (snake oil and a trust risk), driver auto-installer, always-on "boost" service,
AI chat. Per-game settings need a performance dataset first.

## 7. Status

"Verified here" means unit tests plus a Windows-target type-check and clippy from Linux. "CI" means
compiled, tested and packaged on a GitHub Windows runner. Nobody has run it on a real PC yet.

| Area | State |
|---|---|
| Model, rules (11), engine, bloat DB, journal, allowlist | Built, 31 tests |
| Parsers (power names, StartupApproved, SMBIOS speeds/count/chassis, GPUs, refresh, sizes) | Built, 13 tests |
| Helper protocol, write-ahead journal, undo, failure paths | Built, 7 tests with a fake executor |
| Windows collectors (`forge-scan/src/win.rs`) | Type-checks and clippy clean for Windows; CI runs `dump_scan` on a runner. Not compared against a real PC yet |
| Helper Windows executor, journal ACL, restore point | Type-checks and clippy clean for Windows. Not run elevated yet |
| Tauri shell, named pipe + UAC launch | Type-checks and clippy clean for Windows. Not run yet |
| UI | Built and viewed in Chromium in every state (idle, scanning, results, advice, applied, history, 820 px). Not viewed in WebView2 |
| Installer | NSIS per-machine, built in CI. Unsigned. Installed and run on a real laptop: scan, UAC, startup disable and the restore-point message work |
| Not implemented | Prove step (benchmarks), driver/thermal checks, per-game profiles, Windows Update revert detection, EcoQoS, code signing, auto-update |

## 8. Next steps

1. Install the CI build on a real Windows 11 PC and a VM: scan, compare with `dump_scan`, apply and
   undo each fix, check `%ProgramData%\Forge\journal.jsonl`.
2. Measure idle memory and CPU of the installed app (`PERF-BUDGET.md`). Decide WebView2 vs native.
3. Add the Prove step: boot time from the event log, idle RAM and CPU deltas, then PresentMon frametimes.
4. Replace the placeholder startup estimates with measured values and start curating the bloat DB.
5. Code signing, then a public build under the final product name.
