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

**Use `scripts/measure.ps1`.** It launches the app, waits 30 s, then records cold start (from
process start to the UI's first frame), private working set summed over the whole process tree, and
CPU, focused and again minimized:

```
powershell -ExecutionPolicy Bypass -File scripts\measure.ps1 -Exe "C:\Program Files\Forge\Forge.exe"
```

`-BrowserArgs` tries WebView2 variants without rebuilding (sets `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`).
CI runs it on every push to `main` (job **Measure**) for: current WebView2, a leaner WebView2
(one renderer, no background networking or extra UI features), WebView2 without GPU, and
`tools/native-probe`, an egui window of similar complexity, as the native baseline. Results are in
the run's summary page and the `forge-measurements` artifact. The runner has no GPU, so also run the
script on a real laptop and desktop before deciding.

Manual method (for reference):

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

## Measurements

### GitHub Windows runner, 2026-10-23 (CI run 39967116564)

AMD EPYC 7763, 4 logical cores, 16 GB, **no GPU** (software rendering). Private working set summed
over the process tree, after 30 s idle, sampled over 20 s.

| Setup | Cold start | Focused memory | Focused CPU | Minimized memory | Minimized CPU | Processes |
|---|---|---|---|---|---|---|
| WebView2 (as shipped) | 3109 ms (first launch ever on the machine) | 72.2 MB | 0.21% | 67.5 MB | 0.23% | 7 |
| WebView2, lean args | 559 ms | 65.1 MB | 0.18% | 61.3 MB | 0.33% | 7 |
| WebView2, no GPU | 600 ms | 65.1 MB | 0.16% | 61.1 MB | 0.37% | 7 |
| Native egui probe | 543 ms | 15.5 MB | 0% | 15.5 MB | 0% | 1 |

Reading:
- WebView2 is within the suggested 150 MB budget on private memory (about 65 to 72 MB, of which the
  app process is about 4 MB). The 442 MB the app showed on a real laptop is the in-app meter's
  working set, which counts shared pages; it is not what Task Manager's Memory column shows.
- Cold start is about 0.6 s once WebView2's data folder exists; the 3.1 s is the very first launch.
- Misses: minimized CPU is 0.2 to 0.4%, not 0%, so the webview keeps working while minimized; and
  the in-app meter overstates memory. Both are fixable without changing UI technology.
- The lean arguments save about 7 MB (10%); not worth it on their own.
- Native costs about 55 MB less and nothing at idle, at the price of rebuilding the UI in egui.

Still needed: the same script on a real laptop and desktop (real GPU, real drivers).

Follow-ups done (2026-10-23): the in-app meter now reports private working set (matches Task
Manager), re-lists processes only every tenth sample, and the shell marks WebView2 invisible while
the window is minimized, so the page stops polling and WebView2 can throttle. Decision: stay on
WebView2 (owner, 2026-10-23).

After those fixes (CI run 39972495959, same runner type): WebView2 as shipped 71.3 MB / 0.33% CPU
focused, 66.9 MB / **0.06% CPU minimized** (was 0.23%); first-ever launch 6.5 s, later launches about
0.6 s. Minimized CPU is down about 4x but not yet 0%; next step if it matters on real machines is
`ICoreWebView2_3::TrySuspend` while minimized.

## The WebView2 question (decide in M1)

WebView2 is already on Windows 11, but it spawns several processes whose memory counts. If the measured
idle total exceeds the target and can't be trimmed (e.g. via `--disable-features`, a smaller renderer
footprint, or suspending the webview when hidden), propose a native UI (egui or WinUI 3) in an ADR.
`forge-core` and `forge-scan` are UI-agnostic so the switch is bounded to `src-tauri` and `ui`.

## Regression guard (add in M1/M2)

A script (PowerShell is fine here, it's test tooling, not shipped) that launches the packaged app,
waits 60 s, samples memory and CPU of the process tree, and fails if the budget is exceeded. Run in CI
on a Windows runner.
