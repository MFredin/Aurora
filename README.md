# Forge (working name)

A Windows tuner that scans a PC, explains what is slowing it down, applies reversible fixes, and
measures the result. First persona: gamers on Windows 11.

## Layout

| Path | What it is |
|---|---|
| `crates/forge-core` | Data model, rules, bloat database, undo journal, helper protocol and allowlist. Pure Rust, no I/O, fully unit-tested. |
| `crates/forge-scan` | Collectors (registry, SMBIOS, power plan, disks) and the app's self-meter. Parsing is cross-platform and tested. |
| `crates/forge-helper` | The only elevated process. Validates every request against an allowlist, applies it, writes the undo journal. |
| `src-tauri` | Tauri 2 shell. Streams scan results to the UI, looks up fixes by finding id. |
| `ui` | React + Vite UI. Runs in a plain browser with engine-generated sample data for design work. |
| `DESIGN.md` | Architecture, performance budget, safety model, UI concept, status. |
| `CLAUDE.md` | Project guide and hard rules for Claude Code (read automatically). |
| `docs/` | `ROADMAP.md`, `M1-TASKS.md`, `SAFETY.md`, `PERF-BUDGET.md`, `DECISIONS.md`, `BLOAT-DB.md`, `KICKOFF.md` (first prompt). |
| `.claude/commands/verify.md` | `/verify` slash command. |

## Try it

Any OS (Rust and Node 20+ installed):

```
cargo test --workspace            # 15 tests across core, scan and helper
cd ui && npm install && npm run dev   # UI with sample data at http://localhost:5173
```

On Windows 11, full app (needs Rust, Node, the WebView2 runtime and Visual Studio Build Tools):

```
cargo install tauri-cli --version "^2"
cd ui && npm install && cd ..
cargo build -p forge-helper
set FORGE_HELPER=%CD%\target\debug\forge-helper.exe
cd src-tauri && cargo tauri dev
```

## Status

See the table at the end of `DESIGN.md`. In short: the engine, rules, safety layer and UI are built and
tested. The Windows-only collectors, the Tauri shell and the helper's Windows executor were written
without a Windows toolchain and need a first compile and smoke test on Windows.
