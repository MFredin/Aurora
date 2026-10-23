# Forge (working name)

A Windows 11 tuner that scans a PC, explains what is slowing it down, applies reversible fixes, and
keeps a History you can undo from. First persona: gamers on Windows 11.

## Install on Windows

1. Open the repository's **Actions** tab, pick the latest green **Build** run on `main`, and download
   the **forge-windows-installer** artifact (a zip containing `Forge_0.1.0_x64-setup.exe`).
   Tagged versions (`v*`) also publish the installer on the **Releases** page.
2. Run the installer. It installs for all users under `C:\Program Files\Forge`, so Windows asks for
   permission once.
3. The installer isn't code-signed yet, so SmartScreen shows "Windows protected your PC". Choose
   **More info**, then **Run anyway**.

Using it: **Scan this PC**, pick a finding, read exactly what will change, then **Apply fix**. The
first fix in a session shows one Windows permission prompt and asks Windows for a restore point.
Every change is listed under **History** with **Undo**. Uninstall from Settings > Apps.

## What it checks

| Area | Check | Fix |
|---|---|---|
| Gaming | Monitor running below its highest refresh rate | Advice (Settings path) |
| Gaming | Game Bar background recording on | Turn off; undoable |
| Gaming | Game Mode turned off | Turn on; undoable |
| Gaming | Mouse acceleration on | Turn off; undoable |
| Startup | Known heavy or unnecessary startup items (Run keys incl. 32-bit, Startup folders) | Disable, same as Task Manager; undoable |
| Memory | RAM slower than rated (XMP/EXPO off) | Advice only (BIOS) |
| Memory | Single memory module on a desktop | Advice only (hardware) |
| Storage | System drive under 10% free; Windows on a hard drive | Advice only |
| Power | Balanced plan on a desktop or plugged-in laptop (skipped on dual-CCD X3D CPUs, on battery, and where High performance doesn't exist) | Switch plan; undoable |
| Preinstalled apps | Known removable apps (only those the bloat DB marks safe) | Uninstall for your account; not undoable, reinstall from the Store |
| Preinstalled apps | Windows installing promoted apps and showing suggestions | Turn off; undoable |

It never touches Defender, firewall, UAC, Windows Update, services, drivers or firmware
(`docs/SAFETY.md`).

## Layout

| Path | What it is |
|---|---|
| `crates/forge-core` | Data model, rules, bloat database, write-ahead undo journal, helper protocol and allowlist. Pure Rust, no I/O beyond the journal file. |
| `crates/forge-scan` | Collectors (registry, SMBIOS, displays, storage, power plans) and the app's self-meter. Parsing is cross-platform and tested. |
| `crates/forge-helper` | The only elevated process. Validates every request, journals before changing anything, applies, supports undo and restore points. |
| `src-tauri` | Tauri 2 shell. Launches the helper through UAC over a private named pipe, streams scan results, looks up fixes by finding id. |
| `ui` | React + Vite UI. Runs in a plain browser with engine-generated sample data for design work. |
| `.github/workflows/build.yml` | CI: checks on Linux; tests, clippy and the NSIS installer on Windows. |
| `DESIGN.md`, `CLAUDE.md`, `docs/` | Design, project rules, roadmap, safety model, decisions, perf budget. |

## Develop

Any OS (Rust and Node 20+):

```
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cd ui && npm install && npm run dev        # UI with sample data at http://localhost:5173
```

The Windows code can be type-checked from Linux or macOS:

```
rustup target add x86_64-pc-windows-msvc
cargo clippy --workspace --all-targets --target x86_64-pc-windows-msvc -- -D warnings
```

On Windows 11, the full app (needs Rust MSVC toolchain, Node, Visual Studio Build Tools; WebView2 is
built in):

```
cd ui && npm install && npm run build && cd ..
cargo build -p forge-helper
mkdir src-tauri\binaries
copy target\debug\forge-helper.exe src-tauri\binaries\forge-helper-x86_64-pc-windows-msvc.exe

rem Dev mode: helper runs unelevated over stdio, journal in %LOCALAPPDATA%\Forge
set FORGE_HELPER_STDIO=1
set FORGE_HELPER=%CD%\target\debug\forge-helper.exe
start npm --prefix ui run dev
node ui\node_modules\@tauri-apps\cli\tauri.js dev

rem Installer, same as CI
cargo build --release -p forge-helper
copy target\release\forge-helper.exe src-tauri\binaries\forge-helper-x86_64-pc-windows-msvc.exe
node ui\node_modules\@tauri-apps\cli\tauri.js build
```

`cargo run -p forge-scan --example dump_scan` prints what the collectors see on the current machine.

## Status

See the table at the end of `DESIGN.md`. In short: everything is built, unit-tested, and the Windows
code type-checks with clippy clean; CI compiles, tests and packages it on a Windows runner. It has
not yet been run by a person on a real Windows PC, so treat the first install as a test drive and
check History after applying fixes.
