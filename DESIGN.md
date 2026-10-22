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
   │  invoke("scan") / ("apply_fix", id)   ← the UI only ever sends a finding id
 Tauri shell (Rust)              unprivileged   holds last scan, resolves id → FixPlan
   ├─ forge-scan   collectors → Snapshot
   ├─ forge-core   rules(Snapshot) → Findings, journal, allowlist
   │  request: Apply{finding_id, step}
 forge-helper (Rust)             elevated, started on demand, exits when idle
      validate(step) against allowlist → execute → journal prior state
```

Why this split: the webview and the shell never hold admin rights. A compromised UI can only name a
finding from the last scan, and the helper only accepts a closed set of typed operations
(`FixStep`), never a shell string.

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
- No new frontend dependency without a bundle-size check. Currently React only (74 KB gzipped JS).
- The helper exits after about 30 s idle. It must never be left running.

**Biggest risk:** WebView2 runs as several `msedgewebview2.exe` processes that count toward real
memory. If measured idle memory misses the budget, fall back to a native UI (egui or WinUI). The
`forge-core` and `forge-scan` crates are UI-agnostic for exactly this reason.

Planned: EcoQoS for the app process while idle, and a self-profiling test that launches the app,
waits, and asserts on RSS and CPU.

## 4. Safety model

- **Allowlist:** `ipc::validate` permits only known power plans, a short list of registry values,
  and names without shell metacharacters. Anything else is rejected before it reaches the executor.
  Defender and other security settings are not on the list and never will be.
- **Journal:** each applied step is appended to `%ProgramData%\Forge\journal.jsonl` with the prior
  state. `journal::inverse` produces the undo step. Packaged-app removal is marked irreversible.
- **Advice-only findings:** firmware (XMP/EXPO) and hardware (HDD to SSD) never get an auto-fix.
- **Context rules:** no high-performance power plan on a laptop running on battery.
- **Planned:** a System Restore point before the first apply, and a watcher that notices when a
  Windows update reverts a change.

## 5. Rules and data

A rule is a pure function `(Snapshot, BloatDb) → Vec<Finding>` (`rules.rs`). To add one: write the
rule, register it in `default_rules()`, add a test in `tests/engine.rs`. Ranking is
`benefit × confidence × risk discount` (`Finding::score`).

`data/bloat.json` is **seed data, not curated** (`"curated": false`). Each entry is verdict `safe`,
`caution` or `keep`. `keep` items (security components, GPU and audio drivers) are never flagged.
Curating and testing this list on real machines is the long-term moat and ongoing cost.

## 6. UI concept

Telemetry bench. The machine reads as a spec sheet across the top. Findings fall into lanes (Startup,
Preinstalled apps, Power, Memory, Storage); each lane has a bar showing its share of estimated gain.
During a scan a single amber beam sweeps the lanes and results stream in. Selecting a finding opens
its detail: why, what Forge found, exactly what will change, whether it can be undone.

- Palette: petrol `#0A1A21`, panel `#10252E`, bone text `#E6EEF0`, amber `#FFB44D` for gain and
  action, teal `#3DDBC0` for safe and applied, violet `#A78BFA` for expert changes.
- Type: the Windows UI font (Segoe UI Variable) for speed and native feel; tabular numerals.
- Risk is encoded by shape as well as color (filled circle, ring, diamond).
- Copy: sentence case, plain verbs. One name per action ("Apply fix", then "Applied").
- Unverified: the UI has been type-checked and built but not yet viewed in a browser or WebView2.

## 6b. Not in the MVP on purpose

Registry cleaner (snake oil and a trust risk), driver auto-installer, always-on "boost" service,
AI chat. Per-game settings need a performance dataset first.

## 7. Status

| Area | State |
|---|---|
| Model, rules, engine, bloat DB, journal, allowlist | Built, 7 tests passing |
| Parsers (powercfg, startup state, SMBIOS memory speed) | Built, 6 tests passing |
| Helper protocol and safety checks | Built, 2 tests passing |
| UI | Built, type-checks, bundles. Sample-data mode for design work. Not yet viewed visually |
| Windows collectors (`forge-scan/src/win.rs`) | Written, **never compiled**. Needs first build on Windows |
| Helper Windows executor | Written, **never compiled**. Implements power plan and startup disable only |
| Tauri shell | Written, **never compiled**. Linux sandbox lacked GTK libs and the Windows Rust target |
| Not implemented | Appx removal, registry writes, undo from the UI, restore points, History screen, benchmarking, GPU driver checks, thermal checks, installer, code signing |

## 8. Next steps

1. Compile on Windows and smoke-test scan and apply on a real machine and a VM.
2. Measure idle memory and CPU of the packaged app. Decide WebView2 vs native against section 3.
3. Implement undo end to end (history screen, startup re-enable op).
4. Add the Prove step: boot time from the event log, idle RAM and CPU deltas, then PresentMon frametimes.
5. Replace the placeholder impact estimates with measured values and start curating the bloat DB.
