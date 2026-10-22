# Kickoff prompt for Claude Code

Open Claude Code in the project root on the Windows 11 machine (Claude reads `CLAUDE.md` automatically),
then paste this as the first message.

---

Read `CLAUDE.md`, `DESIGN.md`, `docs/ROADMAP.md` and `docs/M1-TASKS.md`. We're starting milestone M1.

Work through `docs/M1-TASKS.md` in order. Before changing code, run `cargo build --workspace` and
`cargo test --workspace` and tell me what fails, grouped by crate. Then fix the compile errors in
`crates/forge-scan/src/win.rs`, the Windows executor in `crates/forge-helper`, and `src-tauri`, keeping
the Windows code as thin as possible and any logic in tested pure functions.

Rules for this session:
- Follow the hard rules in `CLAUDE.md`, especially the safety rules (UI sends only finding ids; helper executes only allowlisted typed steps).
- Don't add dependencies or new `FixStep` variants without asking me first.
- After each numbered step in `M1-TASKS.md`: run `/verify`, summarise what changed and what you verified on this machine vs what is still untested, and commit.
- If a measurement in `docs/PERF-BUDGET.md` misses its target, stop and report instead of working around it.
- Don't run destructive fixes on this machine. Ask me to use a VM snapshot for anything that removes apps or edits the registry beyond Run/StartupApproved.

Start with step 1.

---

## Useful follow-up prompts

- "Add `examples/dump_scan.rs` in forge-scan that prints `collect()` as JSON, then compare it against what Task Manager and CPU-Z show and list the discrepancies."
- "Implement `EnableStartupItem` as the inverse of `DisableStartupItem`, with allowlist validation and tests, and wire Undo into a History screen."
- "Measure idle memory and CPU of the packaged app per `docs/PERF-BUDGET.md` and fill in the table. Then give me a recommendation on WebView2 vs a native UI."
- "Review the helper and Tauri shell against `docs/SAFETY.md` and list anything that violates it."
