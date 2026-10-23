# Performance budget

Forge must not be the thing slowing the PC down. These are targets. M1 sets the real baseline; until then
every "current" cell below is unmeasured.

| Metric | Target | Current |
|---|---|---|
| Idle CPU (window open, nothing happening) | about 0% | unmeasured |
| Idle CPU (window minimized or hidden) | 0% | unmeasured |
| Idle memory, app + all WebView2 children (private working set) | Decide at M1 (suggested starting point: under 150 MB) | **442 MB working set across 8 processes** (first real run, Intel Core 5 320 laptop on battery, self-meter; working set overstates private memory). Over the suggested budget: see "The WebView2 question" |
| Cold start to interactive | under 1.5 s on a mid-range laptop | unmeasured |
| Full scan duration | under 5 s | unmeasured |
| Frame drops in a running game during a scan | none visible (frametime spikes within noise) | unmeasured |
| Installer size | small; WebView2 via bootstrapper, not bundled | unmeasured |
| Frontend bundle | JS under 100 KB gzipped, CSS under 10 KB gzipped | 76 KB / 2.2 KB |
| Helper idle | not running (exits 5 min after the last request or when the app closes) | by design |

## How to measure (Windows)

1. **Memory:** Task Manager > Details, add *Memory (private working set)*. Sum `forge-app.exe` and every `msedgewebview2.exe` that belongs to it. Record the sum, after 60 s idle.
2. **CPU:** Performance Monitor or `typeperf "\Process(forge-app)\% Processor Time" -si 5 -sc 12`, plus the same for each `msedgewebview2` process. Do it with the window focused and again minimized.
3. **Scan duration:** log `Instant::now()` around `collect()` + `engine::scan` and print it in dev builds.
4. **Scan interference:** run a game or a frametime logger (PresentMon) in the background, start a scan, compare 1% lows with and without.
5. **Cold start:** reboot-free: kill the app, start it, measure to first rendered frame (add a one-off timestamp log).
6. **Bundle:** `npm run build` output sizes (gzip).
7. Test on at least: a mid-range laptop on battery, and a high-end desktop. Record both.

## Budget rules

- No resident process when the window is closed. The helper exits when idle.
- Nothing polls while the window is hidden. The footer's self-meter polls every 2 s only when visible, and counts the app's whole process tree (WebView2 included).
- A new frontend dependency needs a size check and a reason. Prefer a few lines of code over a library.
- Animation: `transform` and `opacity` only; at most one looping animation; none under `prefers-reduced-motion`.
- Collectors don't use WMI, PowerShell or any child process (the power plan is now read from the registry). Adding one requires an ADR.
- Add EcoQoS (`SetProcessInformation` with `ProcessPowerThrottling`) to the app process while idle or hidden.

## The WebView2 question (decide in M1)

WebView2 is already on Windows 11, but it spawns several processes whose memory counts. If the measured
idle total exceeds the target and can't be trimmed (e.g. via `--disable-features`, a smaller renderer
footprint, or suspending the webview when hidden), propose a native UI (egui or WinUI 3) in an ADR.
`forge-core` and `forge-scan` are UI-agnostic so the switch is bounded to `src-tauri` and `ui`.

## Regression guard (add in M1/M2)

A script (PowerShell is fine here, it's test tooling, not shipped) that launches the packaged app,
waits 60 s, samples memory and CPU of the process tree, and fails if the budget is exceeded. Run in CI
on a Windows runner.
