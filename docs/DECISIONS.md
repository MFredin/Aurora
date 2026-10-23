# Decisions (ADR log)

Add new entries at the bottom. Format: context, decision, consequences. Mark superseded ones instead of deleting.

## ADR-001. Rust core, Tauri shell, React UI

- **Context:** needs low overhead, native Windows APIs, and a modern-feeling UI.
- **Decision:** Rust for logic and system access; Tauri 2 for the window (uses the system WebView2); React + Vite for UI.
- **Consequences:** small binary, no bundled Chromium. WebView2 child processes still cost memory: measured in M1 (see `PERF-BUDGET.md`). Core crates are UI-agnostic so a native UI remains an option.

## ADR-002. Elevated helper, not an elevated app

- **Context:** most changes need admin; the webview must never run elevated.
- **Decision:** a separate `forge-helper` binary performs a closed set of typed operations, started on demand, exits when idle.
- **Consequences:** more moving parts (IPC, UAC launch). Dev uses stdio; production must use an ACL'd named pipe (M2).

## ADR-003. Typed `FixStep`s with an allowlist, no command strings

- **Context:** a tuner that can run arbitrary commands is a liability and an AV red flag.
- **Decision:** the helper only accepts enum variants that pass `ipc::validate`.
- **Consequences:** each new capability needs code, an allowlist entry, and tests. That friction is intended.

## ADR-004. SMBIOS table, not WMI, for RAM speed

- **Context:** configured vs rated DIMM speed identifies XMP/EXPO being off. WMI is heavy and slow to start.
- **Decision:** read the raw SMBIOS table (`GetSystemFirmwareTable('RSMB')`) and parse Type 17.
- **Consequences:** we own a small parser (tested with synthetic tables). Needs validation on real machines (extended speeds, unusual BIOSes): M1.

## ADR-005. No registry cleaner, no RAM booster, no driver installer

- **Context:** these are the features that eroded trust in existing cleaners.
- **Decision:** out of scope permanently, unless a measured benefit is demonstrated and the safety bar in `SAFETY.md` is met.

## ADR-006. Bloat DB is data, seeded and uncurated

- **Context:** curation is the long-term moat, and bad entries are the main user-harm risk.
- **Decision:** ship seed data flagged `curated: false`; releases require `curated: true` per entry with a test note (`BLOAT-DB.md`).

## ADR-007. Elevation: one UAC prompt per session, named pipe, 5-minute idle exit

- **Context:** stdio can't cross a UAC boundary, and a 30 s idle exit meant a new UAC prompt for almost every fix (open question 5).
- **Decision:** the app creates two single-instance, local-only, one-way named pipes (request and response; a shared duplex handle deadlocks because Windows serializes synchronous I/O per file object) with a random base name, launches `forge-helper.exe --pipe <name> --user-sid <sid>` with `ShellExecuteEx("runas")`, and accepts the connections only if the client PID on both is the process it launched. The helper exits after 5 minutes without a request or as soon as the app closes the pipe. Dev mode (`FORGE_HELPER_STDIO=1`) keeps unelevated stdio.
- **Consequences:** one prompt covers a batch of fixes. A standard user who elevates with another account's credentials still changes their own settings, because "HKCU" steps go to `HKEY_USERS\<sid>` of the app's user. The helper still validates every request, so a process that squats the pipe name gains nothing.

## ADR-008. Preinstalled apps are advice only in v0.1 (superseded by ADR-014)

- **Context:** removing packaged apps without PowerShell means driving the WinRT package manager from the elevated helper, and removal can't be undone.
- **Decision:** `RemoveAppx` is removed from `FixStep`. App findings point to Settings > Apps, which removes them cleanly and supports reinstall from the Store.
- **Consequences:** no irreversible operation exists in the helper at all. Revisit with the Windows package manager API and a curated DB (M5).

## ADR-009. Ranking advice-only findings

- **Context:** findings without an impact estimate all scored 0 and sank (M1 task 4), and the UI never re-sorted streamed results.
- **Decision:** each rule sets a `priority` used when there is no estimate: refresh rate 1.5, XMP 1.2, single channel 1.0, low disk 1.0, Game DVR 0.8, power plan 0.6, preinstalled apps 0.1. Score = max(estimate, priority) x confidence x risk discount. The engine stores `score` on each finding and the app sends the final order when the scan ends; the UI re-sorts on that.
- **Consequences:** priorities are judgement calls until the Prove step measures real effects. Change them in `rules.rs` with a test.

## ADR-010. Power plan rule guards

- **Context:** High performance is not always better. On dual-CCD Ryzen X3D CPUs AMD's V-Cache driver needs Balanced to park the non-cache CCD in games; on Modern Standby laptops the High performance plan often doesn't exist; the gaming benefit is unmeasured.
- **Decision:** the rule skips dual-CCD X3D parts (`power::is_dual_ccd_x3d`), PCs without the High performance plan, and laptops on battery; confidence is Low and the copy says the effect is often small. Undo may restore any plan GUID (OEM/custom plans), so the allowlist accepts any well-formed GUID; activating an existing plan can't weaken security.

## ADR-011. Write-ahead journal in an admin-only folder

- **Context:** the journal was written after the change (a crash lost it), a journal failure was reported as "rejected" though the change had happened, and `%ProgramData%\Forge` could be pre-created by a normal user to plant entries for undo.
- **Decision:** append an `intent` record with the prior state before changing anything, then an `outcome`. No intent, no change. History shows interrupted entries and still offers undo. The helper creates `%ProgramData%\Forge` with an explicit protected DACL (SYSTEM and Administrators full, Users read) and refuses an existing folder not owned by SYSTEM or Administrators. Undo requests carry only a journal id; the helper derives the inverse from its own journal.

## ADR-012. Per-machine install

- **Context:** the helper runs elevated. Installed per-user, any process running as the user could replace `forge-helper.exe` and be elevated at the next UAC prompt.
- **Decision:** the NSIS installer is per-machine (`C:\Program Files\Forge`), writable only by administrators.

## ADR-013. Things Forge will not recommend

- **Memory integrity (HVCI/VBS)** costs some FPS but is a security control. Forge doesn't flag it or suggest turning it off (`SAFETY.md` never list).
- **Anti-cheat:** M4 per-game priority, affinity and CPU-set changes must be checked against Vanguard, EasyAntiCheat and BattlEye before shipping; they can be read as tampering. Default to off for games with kernel anti-cheat until verified.
- **Resizable BAR** is a firmware setting with no reliable read path without vendor APIs; left out until there is one.

## ADR-014. Remove bloatware and change settings from the app

- **Context:** the owner wants Forge to act, not just advise: remove preinstalled apps and apply settings fixes.
- **Decision:** `RemoveAppx { package_name }` uninstalls an app for the signed-in user through the Windows package manager (WinRT `PackageManager`, via the `windows` crate already in the dependency tree). The helper allows it only for apps the bloat DB marks `safe` (never `keep`, never unknown apps), only when the elevated account is the app's user, and journals the full package name first. It is the one irreversible step: the plan says `reversible: false`, the button reads "Remove app", it is never in the low-risk batch, and History says to reinstall from the Store. Settings fixes are typed registry writes on an explicit per-user allowlist with allowed values: Game Mode, mouse acceleration (three text values, digits only), Game Bar background recording, and six Windows suggestion / promoted-app switches. All are undoable.
- **Consequences:** Windows feature updates can reinstall some inbox apps; turning off promoted-app installs reduces that. Curating the app list (BLOAT-DB.md) now matters more, because entries cause real removals.

## Open questions

1. **Product name.** "Forge" is a placeholder; the repository is named Aurora. Needs a name, domain and trademark check before any public build.
2. **Open-core or closed.** Open scanner + DB as a trust signal, paid fix engine/profiles? Decide before M5.
3. **Monetization and licensing.** Free tier scope, Pro features, license enforcement without being intrusive.
4. ~~**WebView2 vs native UI.**~~ Stay on WebView2: about 65 to 72 MB private memory and 0.6 s cold start on the CI runner, within budget (PERF-BUDGET.md). Revisit if real-GPU machines exceed 150 MB.
5. ~~**Elevation UX.**~~ Decided in ADR-007: one prompt per session (5-minute idle).
6. **Windows 10.** Support, or Windows 11 only? (Windows 10 is past end of support; check user base before spending effort.)
7. **ARM64 Windows.** Build target in CI?
