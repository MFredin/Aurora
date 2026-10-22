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

## Open questions

1. **Product name.** "Forge" is a placeholder. Needs a name, domain and trademark check before any public build.
2. **Open-core or closed.** Open scanner + DB as a trust signal, paid fix engine/profiles? Decide before M5.
3. **Monetization and licensing.** Free tier scope, Pro features, license enforcement without being intrusive.
4. **WebView2 vs native UI.** Decided by the M1 measurements.
5. **Elevation UX.** One UAC prompt per session vs per apply. Prefer per session with a visible "helper running" indicator.
6. **Windows 10.** Support, or Windows 11 only? (Windows 10 is past end of support; check user base before spending effort.)
7. **ARM64 Windows.** Build target in CI?
