# Roadmap

Persona for v0.x: **gamers on Windows 11** (desktop first, laptops later). Thesis: Scan, Explain, Fix, Prove.
Each milestone lists acceptance criteria. Done means every box is true and `/verify` passes.

## M1. Windows bring-up and first end-to-end (start here)

Goal: the scaffold compiles and runs on a real Windows 11 machine, and one fix works end to end.

- [ ] Whole workspace and `src-tauri` build on Windows. Signature drift in `win.rs`, helper executor and Tauri shell fixed.
- [ ] `cargo tauri dev` shows real data from this machine (CPU, GPUs, RAM, startup items, power plan, disks).
- [ ] RAM configured vs rated speed comes from SMBIOS and matches CPU-Z / Task Manager on a test machine.
- [ ] Apply "Disable startup item" from the UI through the helper. Verify in Task Manager > Startup apps that it shows as Disabled.
- [ ] Apply "Switch to High performance" through the helper and verify with `powercfg /getactivescheme`.
- [ ] Journal entry written for each apply at `%ProgramData%\Forge\journal.jsonl`.
- [ ] First perf baseline recorded in `docs/PERF-BUDGET.md` (idle RSS incl. WebView2 children, idle CPU, cold start, scan time).
- [ ] Decision recorded: stay on WebView2, or move to a native UI (see PERF-BUDGET).
- [ ] UI viewed in WebView2 and fixed where the design doesn't hold up (layout, contrast, focus rings, 820px min width).

## M2. Undo, History, restore points

- [ ] `EnableStartupItem` op (inverse of disable) added with allowlist + tests.
- [ ] Undo works for every reversible step from a History screen. Irreversible entries are shown as such.
- [ ] System Restore point created before the first apply in a session (skip with a clear message if disabled by policy).
- [ ] Failed or partial applies leave the journal consistent (test with an executor that fails mid-plan).
- [ ] Helper transport moves from stdio to an ACL'd named pipe with UAC-elevated launch and ~30 s idle exit.

## M3. Prove

- [ ] Before/after for idle RAM, idle CPU, and boot time (Windows event log boot performance data).
- [ ] Results shown per fix and as a session summary the user can copy or save as an image.
- [ ] Placeholder estimates in `rules.rs` replaced by measured values or removed. "No measurable change" is a valid, displayed outcome.
- [ ] Measurement itself stays inside the perf budget.

## M4. Gaming layer

- [ ] PresentMon-based frametime capture (average, 1% low) with a repeatable benchmark flow.
- [ ] Per-game profiles: process priority, affinity/CPU sets (X3D dual-CCD case), background process rules. Auto-apply on launch, auto-revert on exit.
- [ ] Detection of Windows Update reverting a tweak, with a one-click re-apply.
- [ ] GPU/driver age check that links to the vendor's official download. No bundled driver installer.
- [ ] Thermal throttling detection (read-only, advice only).

## M5. Data and release

- [ ] Bloat DB curated: every entry tested on real hardware or VM, `curated: true`, with a source note. See `docs/BLOAT-DB.md`.
- [ ] Installer (NSIS), code signing, AV vendor false-positive submissions, auto-update strategy.
- [ ] Opt-in anonymous result sharing design (what is sent, how it's reviewed). Decision on open-core vs closed.
- [ ] Laptop and battery profiles; OEM-specific rules (ASUS, Dell, HP, Lenovo).

## Explicitly out of scope

Registry cleaner. "RAM booster" or memory trimming. Driver auto-installer. Always-on background optimizer.
Cloud accounts. AI chat in the UI (AI may help *write* explanations and curate the DB offline; it
doesn't run in the app's hot path).
