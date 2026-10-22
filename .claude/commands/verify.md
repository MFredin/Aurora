---
description: Run every check that can run on this machine and report what was and wasn't verified
---

Run these in order and report results honestly. Stop at the first failure and fix it before continuing.

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`
4. `cd ui && npm run build` (type-check + bundle). Note the gzipped JS and CSS sizes and compare with `docs/PERF-BUDGET.md`.
5. Windows code: on Windows run `cd src-tauri && cargo clippy -- -D warnings`; elsewhere run
   `cargo clippy --workspace --all-targets --target x86_64-pc-windows-msvc -- -D warnings` and the same in `src-tauri`
   (needs `rustup target add x86_64-pc-windows-msvc` and a placeholder `src-tauri/binaries/forge-helper-x86_64-pc-windows-msvc.exe`).
6. If the Rust model changed: confirm `ui/src/types.ts` matches and `ui/src/sample.json` was regenerated with
   `cargo run -p forge-core --example dump_sample`.
7. Check the hard rules in `CLAUDE.md` against the diff: no new command accepts steps/paths/keys from the UI, no new child processes or WMI/PowerShell in collectors, no allowlist change without tests, no Defender/security changes.

Finish with a short report: what passed, what failed, and what was **not** verified (for example "Windows-only code not compiled on this OS", "UI not viewed in WebView2", "no manual test on a VM").
