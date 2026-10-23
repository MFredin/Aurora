<#
.SYNOPSIS
  Measures Forge's own cost: cold start, idle memory and idle CPU for the app and every process it
  starts (the WebView2 browser, renderer, GPU and utility processes), focused and minimized.

.DESCRIPTION
  Test tooling, not shipped (docs/PERF-BUDGET.md). Memory is the private working set, the
  "Memory" column in Task Manager's Details tab, summed over the process tree. CPU is processor time
  used during the sample window, as a share of the whole machine (all cores = 100%).

.EXAMPLE
  # Installed app, on your own PC (PowerShell, no admin needed):
  powershell -ExecutionPolicy Bypass -File .\measure.ps1 -Exe "C:\Program Files\Forge\Forge.exe"

.EXAMPLE
  # A WebView2 variant without changing the app:
  .\measure.ps1 -Exe .\forge-app.exe -Label "low-memory" -BrowserArgs "--renderer-process-limit=1"
#>
param(
  [Parameter(Mandatory = $true)][string]$Exe,
  [string]$Label = "default",
  [string]$BrowserArgs = "",
  [int]$SettleSeconds = 30,
  [int]$SampleSeconds = 20,
  [string]$Out = ""
)

$ErrorActionPreference = "Stop"
$Exe = (Resolve-Path $Exe).Path
$cores = [Environment]::ProcessorCount

Add-Type -Namespace Forge -Name Win -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool ShowWindow(System.IntPtr hWnd, int nCmdShow);
'@

function Get-Tree([int]$RootPid) {
  $all = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process |
    Where-Object { $_.IDProcess -ne 0 -and $_.Name -ne "_Total" -and $_.Name -ne "Idle" }
  $ids = New-Object System.Collections.Generic.HashSet[int]
  [void]$ids.Add($RootPid)
  do {
    $before = $ids.Count
    foreach ($p in $all) { if ($ids.Contains([int]$p.CreatingProcessID)) { [void]$ids.Add([int]$p.IDProcess) } }
  } while ($ids.Count -ne $before)
  $all | Where-Object { $ids.Contains([int]$_.IDProcess) }
}

function Measure-State([int]$RootPid, [string]$State) {
  $tree = @(Get-Tree $RootPid)
  $procs = foreach ($t in $tree) { Get-Process -Id $t.IDProcess -ErrorAction SilentlyContinue }
  $cpu0 = ($procs | ForEach-Object { $_.TotalProcessorTime.TotalMilliseconds } | Measure-Object -Sum).Sum
  $sw = [Diagnostics.Stopwatch]::StartNew()
  Start-Sleep -Seconds $SampleSeconds
  $elapsed = $sw.Elapsed.TotalMilliseconds
  $procs | ForEach-Object { $_.Refresh() }
  $cpu1 = ($procs | Where-Object { -not $_.HasExited } | ForEach-Object { $_.TotalProcessorTime.TotalMilliseconds } | Measure-Object -Sum).Sum
  $tree = @(Get-Tree $RootPid)
  [pscustomobject]@{
    state        = $State
    processes    = $tree.Count
    private_mb   = [math]::Round((($tree | Measure-Object WorkingSetPrivate -Sum).Sum) / 1MB, 1)
    cpu_pct      = [math]::Round([math]::Max(0, $cpu1 - $cpu0) / ($elapsed * $cores) * 100, 2)
    breakdown    = ($tree | Group-Object { $_.Name -replace '#\d+$', '' } | ForEach-Object {
                     "{0} x{1}: {2} MB" -f $_.Name, $_.Count, [math]::Round((($_.Group | Measure-Object WorkingSetPrivate -Sum).Sum) / 1MB, 1)
                   }) -join "; "
  }
}

$name = [IO.Path]::GetFileNameWithoutExtension($Exe)
Get-Process -Name $name -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 2

$perfLog = Join-Path $env:TEMP ("forge-perf-{0}.txt" -f [guid]::NewGuid())
$env:FORGE_PERF_LOG = $perfLog
if ($BrowserArgs) { $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = $BrowserArgs } else { Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS -ErrorAction SilentlyContinue }

$proc = Start-Process -FilePath $Exe -PassThru
$coldMs = $null
for ($i = 0; $i -lt 300 -and -not (Test-Path $perfLog); $i++) { Start-Sleep -Milliseconds 100 }
if (Test-Path $perfLog) { $coldMs = [int](Get-Content $perfLog -Raw) }

Start-Sleep -Seconds $SettleSeconds
$focused = Measure-State $proc.Id "focused"

$proc.Refresh()
if ($proc.MainWindowHandle -ne [IntPtr]::Zero) { [void][Forge.Win]::ShowWindow($proc.MainWindowHandle, 6) }  # SW_MINIMIZE
Start-Sleep -Seconds 10
$minimized = Measure-State $proc.Id "minimized"

foreach ($p in (Get-Tree $proc.Id)) { Stop-Process -Id $p.IDProcess -Force -ErrorAction SilentlyContinue }
Remove-Item $perfLog -ErrorAction SilentlyContinue
Remove-Item Env:FORGE_PERF_LOG, Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS -ErrorAction SilentlyContinue

$result = [pscustomobject]@{
  label         = $Label
  exe           = $Exe
  browser_args  = $BrowserArgs
  cold_start_ms = $coldMs
  machine       = "{0}, {1} logical cores, {2} GB" -f (Get-CimInstance Win32_Processor | Select-Object -First 1).Name.Trim(), $cores, [math]::Round((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory / 1GB)
  focused       = $focused
  minimized     = $minimized
}

"{0,-14} cold start {1,6} ms | focused {2,7} MB {3,6}% CPU ({4} procs) | minimized {5,7} MB {6,6}% CPU" -f `
  $Label, $coldMs, $focused.private_mb, $focused.cpu_pct, $focused.processes, $minimized.private_mb, $minimized.cpu_pct
"  focused:   " + $focused.breakdown
"  minimized: " + $minimized.breakdown
if ($Out) { $result | ConvertTo-Json -Depth 4 | Set-Content -Path $Out -Encoding utf8 }
