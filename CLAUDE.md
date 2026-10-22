# Forge (working name): project guide for Claude Code

A Windows 11 desktop app that scans a PC, explains what slows it down, applies **reversible** fixes,
and measures the result. First persona: gamers. Read `DESIGN.md` for the full design and
`docs/ROADMAP.md` for what to build next.

## Commands

```
cargo test --workspace                 # core, scan, helper (runs on any OS)
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cd ui && npm install && npm run build  # tsc --noEmit + vite build
cd ui && npm run dev                   # browser dev mode, uses sample data (no Tauri needed)
cargo run -p forge-core --example dump_sample > ui/src/sample.json   # regenerate UI mock data

# Windows only
cargo build -p forge-helper
set FORGE_HELPER=%CD%\target\debug\forge-helper.exe
cd src-tauri && cargo tauri dev        # needs: cargo install tauri-cli --version "^2"
```

Run `/verify` (see `.claude/commands/verify.md`) before saying anything is done.

## Layout

| Path | Role |
|---|---|
| `crates/forge-core` | Model, rules, engine, bloat DB, journal, helper protocol + allowlist. Pure logic, no I/O. |
| `crates/forge-scan` | Collectors and parsers. Windows-only code is in `src/win.rs`; parsing lives in `parse.rs` / `smbios.rs` so it is testable anywhere. |
| `crates/forge-helper` | The only elevated process. Validate, execute, journal. |
| `src-tauri` | Tauri 2 shell. **Excluded from the Cargo workspace** on purpose (needs WebView2). |
| `ui` | React + Vite. Mirrors `forge-core::model` in `src/types.ts`. |

## Hard rules (do not break these without asking)

**Safety**
1. The UI sends only a finding **id**. The shell resolves the `FixPlan` from the last scan. Never add a command that accepts a step, path, registry key or command line from the UI.
2. The helper executes only typed `FixStep` variants that pass `ipc::validate`. No shell strings, no `cmd /c`, no PowerShell script text. New operation = new enum variant + allowlist entry + tests.
3. Never touch Windows Defender or other security settings. Never disable, exclude or tamper with them.
4. Every applied step writes a journal entry with the prior state. If a step can't be reversed, the fix plan says `reversible: false` and the UI shows it before the click.
5. Firmware and hardware findings (XMP/EXPO, HDD to SSD, BIOS) are **advice only**. No auto-fix.
6. Don't push a high-performance power plan on a laptop that's on battery.

**Performance (Forge must not be the bloat)**
7. No resident service, no startup entry by default. The helper starts on demand and exits when idle.
8. Collectors read the registry / firmware tables / cheap APIs. No WMI, no PowerShell, no new child processes (except `powercfg`). If WMI is truly needed, call it once per scan and justify it in `docs/DECISIONS.md`.
9. Scans run on a background-priority thread (`BackgroundGuard`).
10. UI: nothing polls while hidden; animate `transform`/`opacity` only; no web fonts; no new frontend dependency without a bundle-size check (`docs/PERF-BUDGET.md`).

**Honesty**
11. Estimates are labelled as estimates until measured. Don't claim a speed-up the Prove step hasn't measured.
12. Don't ship features that sell placebo: **no registry cleaner**, no "RAM booster", no driver auto-installer.

## Conventions

- Rust: stable toolchain, `cargo fmt`, clippy clean, no `unwrap()` outside tests and `main` setup. Errors as `Result<_, String>` at the Tauri boundary, typed errors (`thiserror`) inside crates.
- Keep Windows-specific code behind `#[cfg(windows)]` and as thin as possible; put logic in pure functions with tests.
- Rules (`rules.rs`) are pure: `(Snapshot, BloatDb) -> Vec<Finding>`. Every rule needs a test in `crates/forge-core/tests/engine.rs`, including a "does not fire" case.
- UI copy: sentence case, plain verbs, one name per action ("Apply fix" then "Applied"). Errors say what happened and what to do. No all-caps labels, no "word - fragment" labels.
- When the Rust model changes: update `ui/src/types.ts`, regenerate `ui/src/sample.json`, run the UI build.
- Commits: small and focused. Update the status table in `DESIGN.md` when something moves from "written" to "verified".

## Known unverified areas (first job: verify these on Windows)

`crates/forge-scan/src/win.rs`, the Windows executor in `crates/forge-helper/src/main.rs`, and all of `src-tauri/` were written **without a Windows compiler** and have never been built. Expect signature drift in `winreg` / `windows-sys`. The UI has never been viewed in a real browser or WebView2. See `docs/M1-TASKS.md`.

## Working style

- Prefer the smallest change that satisfies the acceptance criteria in `docs/ROADMAP.md`.
- Ask before: adding a dependency, adding a FixStep, changing the allowlist, changing the helper transport, or anything that writes to the registry.
- If a measurement misses the budget in `docs/PERF-BUDGET.md`, stop and report. Don't paper over it.
- Product name is a placeholder; don't rename things until it's decided (`docs/DECISIONS.md`, open question 1).
