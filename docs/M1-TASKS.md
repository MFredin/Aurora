# M1 task list: Windows bring-up

> **Status (Oct 2026):** steps 1 and 3 are done in code: everything type-checks and is clippy clean
> for Windows, CI builds, tests and packages it on a Windows runner, the helper uses an elevated named
> pipe, and HKLM items work through UAC. Step 2 has `examples/dump_scan.rs`, Startup folders and
> 32-bit Run keys, NVMe detection via the storage stack, extended SMBIOS speeds, and a power plan read
> from the registry. What's left needs a real PC: compare the collectors with ground truth (step 2),
> view the UI in WebView2 (step 4), and measure the budget (step 5).

Work top to bottom. Commit after each numbered step. Stop and report if a step reveals a design
problem instead of working around it.

## Prerequisites on the Windows machine

Rust (stable, MSVC toolchain), Visual Studio Build Tools (C++), Node 20+, WebView2 runtime (present on
Windows 11), `cargo install tauri-cli --version "^2"`.

## 1. Make everything compile

```
cargo build --workspace
cargo test --workspace
cd src-tauri && cargo check
```

Known spots most likely to need fixes (written blind, never compiled):

- `crates/forge-scan/src/win.rs`
  - `winreg`: `RegValue.bytes` type (Vec vs Cow) in `read_startup`; `enum_values()` / `enum_keys()` item types.
  - `windows-sys`: `SetThreadPriority` arg types and `THREAD_MODE_BACKGROUND_BEGIN/END` casts; `GetSystemFirmwareTable` pointer types; `SYSTEM_POWER_STATUS` field names; needed feature flags in `Cargo.toml`.
- `crates/forge-helper/src/main.rs` (`exec_win`): `RegValue { bytes, vtype }` construction, `set_raw_value`.
- `src-tauri`: Tauri 2 `Channel` usage, `generate_context!`, capabilities file schema path, icon validity (`icons/icon.ico` is a generated placeholder: replace with a real multi-size ICO if the bundler complains).
- Workspace hygiene: crates were created with edition 2024 while the workspace says 2021. Pick one (prefer 2024 if the MSRV is fine) and use `[workspace.package]` inheritance.

Done when: `cargo test --workspace` green on Windows and `cargo tauri dev` opens a window.

## 2. Verify collectors against reality

Add a hidden dev command or an `examples/dump_scan.rs` in `forge-scan` that prints `collect()` as JSON.
Compare with ground truth on the test machine:

- CPU name and thread count (Task Manager)
- GPU list (Device Manager). Check laptops with iGPU + dGPU and VMs.
- RAM configured vs rated MT/s (CPU-Z SPD/Memory tabs). Handle `0xFFFF` extended-speed DIMMs (SMBIOS offset 0x54).
- Startup items: compare with Task Manager > Startup apps. Cover `HKLM\...\WOW6432Node\...\Run`, per-user and machine-wide Startup folders (currently missing), and enabled/disabled state.
- Power plan GUID and name (`powercfg /list`). Note localized plan names.
- Disk kind: `sysinfo` often reports NVMe as `Unknown`. Decide whether to query the storage device descriptor (`IOCTL_STORAGE_QUERY_PROPERTY`, seek-penalty) instead.
- Appx list from the Repository registry key vs `Get-AppxPackage` (one-off comparison only; don't ship PowerShell).

Write the discrepancies down in `docs/DECISIONS.md` and fix the collectors. Add parser tests for any
new formats you find.

## 3. First end-to-end fixes through the helper

1. Build the helper: `cargo build -p forge-helper`. Set `FORGE_HELPER` to its path.
2. In the app, scan, then apply a *disable startup item* fix. Check Task Manager shows it Disabled.
3. Apply the power plan fix. Check `powercfg /getactivescheme`.
4. Inspect `%ProgramData%\Forge\journal.jsonl`. Each entry must have the correct `prior`.
5. Failure paths: helper missing, helper crashes mid-call, journal directory not writable. The UI must show a clear error and never report "Applied" for something that failed. (Today `apply_fix` holds a mutex while blocking on the helper's stdout and runs on the async runtime: move it to `spawn_blocking` and add a timeout.)

Known gap to fix here: the startup-disable path currently runs unelevated for HKCU and will need
elevation for HKLM items. Decide how the dev stdio helper is elevated, or keep M1 to HKCU items and
document the limit.

## 4. UI in a real WebView2

- Look at every state: idle, scanning, done, applying, failed, no findings.
- Check at 820 px width, 125% / 150% DPI scaling, Windows high-contrast mode, keyboard-only use.
- Fix the zero-score ranking tie: findings with no estimated impact (XMP, low disk space, power plan) all score 0 and sink to the bottom. Either give advice-only findings a deliberate weight or sort by category priority first. Record the choice.
- Confirm the scan beam animation costs nothing noticeable (check GPU/CPU in Task Manager while scanning) and is absent under "reduce motion".

## 5. Baseline the performance budget

Follow `docs/PERF-BUDGET.md`, "How to measure". Record results in the table there, including the sum
across all `msedgewebview2.exe` children. Then answer the question: **does WebView2 fit the budget?**
If not, write an ADR proposing the native UI route before building more features.

## 6. Close out

- Update the status table in `DESIGN.md` (move verified items from "written" to "verified").
- Tick the boxes in `docs/ROADMAP.md` M1.
- Open questions that came up go in `docs/DECISIONS.md`.
