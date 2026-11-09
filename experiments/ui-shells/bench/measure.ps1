#requires -Version 5.1
<#
.SYNOPSIS
  Cold-start + idle-footprint measurement for one UI shell.

.DESCRIPTION
  Launches the shell, measures:
    * windowMs  - process start -> first non-zero MainWindowHandle
    * paintMs   - process start -> the UI's own first-contentful-paint marker,
                  reported over HTTP to bench/collector.mjs (wall-clock comparable,
                  because the page reports performance.timeOrigin + FCP).
    * idle memory after -IdleSeconds: main process and total process tree
      (WorkingSet64, inclusive of WebView2 / Chromium helpers).

  Every run is repeated -Runs times; the summary reports the median.
  Raw per-run records are written to bench/results/measure-<shell>.json.
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][ValidateSet('pwa', 'tauri', 'electron')][string]$Shell,
  [Parameter(Mandatory = $true)][string]$Exe,
  [string[]]$AppArgs = @(),
  [string]$ProcessFilter = '',
  [string]$ProfileDirRoot = '',
  [switch]$ReuseProfile,
  [switch]$Headless,
  [int]$Runs = 3,
  [int]$IdleSeconds = 8,
  [string]$MarksFile = '',
  [string]$WindowTitleLike = '*Layered*'
)

$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$resultsDir = Join-Path $here 'results'
New-Item -ItemType Directory -Force -Path $resultsDir | Out-Null
if (-not $MarksFile) { $MarksFile = Join-Path $resultsDir 'marks.jsonl' }

function Read-Marks {
  param([string]$Path)
  if (-not (Test-Path -LiteralPath $Path)) { return @() }
  return @(Get-Content -LiteralPath $Path -ErrorAction SilentlyContinue | Where-Object { $_.Trim() })
}

function Read-LatestMark {
  param([string]$Shell)
  $file = Join-Path $resultsDir ("latest-{0}.json" -f $Shell)
  if (-not (Test-Path -LiteralPath $file)) { return $null }
  try {
    return Get-Content -LiteralPath $file -Raw -ErrorAction Stop | ConvertFrom-Json
  } catch {
    return $null
  }
}

function Get-ProcessSnapshot {
  Get-CimInstance -ClassName Win32_Process | Select-Object ProcessId, ParentProcessId, Name, CommandLine
}

function Get-DescendantIds {
  param([int]$RootProcessId, $Snapshot)
  $children = @{}
  foreach ($p in $Snapshot) {
    $parentId = [int]$p.ParentProcessId
    if (-not $children.ContainsKey($parentId)) {
      $children[$parentId] = New-Object System.Collections.Generic.List[int]
    }
    $children[$parentId].Add([int]$p.ProcessId)
  }
  $seen = New-Object 'System.Collections.Generic.HashSet[int]'
  $queue = New-Object 'System.Collections.Generic.Queue[int]'
  $queue.Enqueue($RootProcessId)
  while ($queue.Count -gt 0) {
    $current = $queue.Dequeue()
    if (-not $seen.Add($current)) { continue }
    if ($children.ContainsKey($current)) {
      foreach ($childId in $children[$current]) { $queue.Enqueue($childId) }
    }
  }
  return @($seen)
}

function Get-FootprintIds {
  param([int]$RootProcessId, [string]$Filter)
  $snapshot = Get-ProcessSnapshot
  $ids = New-Object 'System.Collections.Generic.HashSet[int]'
  foreach ($processId in (Get-DescendantIds -RootProcessId $RootProcessId -Snapshot $snapshot)) {
    [void]$ids.Add($processId)
  }
  if ($Filter) {
    foreach ($p in $snapshot) {
      if ($p.Name -eq 'msedgewebview2.exe' -and $p.CommandLine -and $p.CommandLine -like "*$Filter*") {
        [void]$ids.Add([int]$p.ProcessId)
      }
    }
  }
  return @($ids)
}

function Measure-Footprint {
  param([int]$RootProcessId, [string]$Filter)
  $ids = Get-FootprintIds -RootProcessId $RootProcessId -Filter $Filter
  $rows = @()
  $totalBytes = [int64]0
  $totalPrivate = [int64]0
  foreach ($processId in $ids) {
    $proc = Get-Process -Id $processId -ErrorAction SilentlyContinue
    if (-not $proc) { continue }
    $totalBytes += [int64]$proc.WorkingSet64
    $totalPrivate += [int64]$proc.PrivateMemorySize64
    $rows += [pscustomobject]@{
      Name           = $proc.ProcessName
      Id             = $processId
      WorkingSetMB   = [math]::Round($proc.WorkingSet64 / 1MB, 1)
      PrivateMB      = [math]::Round($proc.PrivateMemorySize64 / 1MB, 1)
    }
  }
  $main = Get-Process -Id $RootProcessId -ErrorAction SilentlyContinue
  return [pscustomobject]@{
    ProcessCount       = @($rows).Count
    MainWorkingSetMB   = if ($main) { [math]::Round($main.WorkingSet64 / 1MB, 1) } else { $null }
    TotalWorkingSetMB  = [math]::Round($totalBytes / 1MB, 1)
    TotalPrivateMB     = [math]::Round($totalPrivate / 1MB, 1)
    Processes          = @($rows)
  }
}

function Stop-Tree {
  param([int]$RootProcessId, [string]$Filter)
  try {
    $ids = @(Get-FootprintIds -RootProcessId $RootProcessId -Filter $Filter)
  } catch {
    $ids = @($RootProcessId)
  }
  $arr = @($ids)
  [array]::Reverse($arr)
  foreach ($processId in $arr) {
    Stop-Process -Id $processId -Force -ErrorAction SilentlyContinue
  }
}

function Get-Median {
  param([double[]]$Values)
  $sorted = @($Values | Sort-Object)
  if ($sorted.Count -eq 0) { return $null }
  $mid = [int][math]::Floor($sorted.Count / 2)
  if ($sorted.Count % 2 -eq 1) { return [math]::Round([double]$sorted[$mid], 1) }
  return [math]::Round(([double]$sorted[$mid - 1] + [double]$sorted[$mid]) / 2, 1)
}

$records = @()

if ($ReuseProfile -and $ProfileDirRoot) {
  # One profile for the whole series: models a PWA that is already installed,
  # so the service worker and HTTP cache carry over between runs.
  if (Test-Path -LiteralPath $ProfileDirRoot) {
    Remove-Item -LiteralPath $ProfileDirRoot -Recurse -Force -ErrorAction SilentlyContinue
  }
  New-Item -ItemType Directory -Force -Path $ProfileDirRoot | Out-Null
}

for ($run = 1; $run -le $Runs; $run++) {
  $runArgs = @($AppArgs)
  if ($ProfileDirRoot) {
    if ($ReuseProfile) {
      $profileDir = $ProfileDirRoot
    } else {
      $profileDir = Join-Path $ProfileDirRoot ("run{0}" -f $run)
      if (Test-Path -LiteralPath $profileDir) { Remove-Item -LiteralPath $profileDir -Recurse -Force -ErrorAction SilentlyContinue }
      New-Item -ItemType Directory -Force -Path $profileDir | Out-Null
    }
    $runArgs += "--user-data-dir=$profileDir"
  }

  $baseline = (Read-Marks -Path $MarksFile).Count
  $errFile = Join-Path $resultsDir ("{0}-run{1}.stderr.log" -f $Shell, $run)
  $outFile = Join-Path $resultsDir ("{0}-run{1}.stdout.log" -f $Shell, $run)

  $started = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
  $startParams = @{ FilePath = $Exe; PassThru = $true }
  if ($runArgs.Count -gt 0) { $startParams['ArgumentList'] = $runArgs }
  try {
    $proc = Start-Process @startParams -RedirectStandardError $errFile -RedirectStandardOutput $outFile
  } catch {
    Write-Warning "redirect failed ($($_.Exception.Message)); starting without redirect"
    $proc = Start-Process @startParams
  }
  $procId = $proc.Id
  $treeRoot = $procId
  $relaunched = $false

  $windowMs = $null
  if ($Headless) {
    # No window may exist (Electron offscreen / headless browser): poll only to
    # confirm the process is alive, never to wait for a visible window.
    Start-Sleep -Milliseconds 120
  } else {
    $deadline = $started + 60000
    while ([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() -lt $deadline) {
      $live = Get-Process -Id $procId -ErrorAction SilentlyContinue
      if ($live -and $live.MainWindowHandle -ne [IntPtr]::Zero) {
        $windowMs = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() - $started
        break
      }
      if (-not $live) {
        $titled = Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowTitle -like $WindowTitleLike } | Select-Object -First 1
        if ($titled) {
          $treeRoot = $titled.Id
          $relaunched = $true
          $windowMs = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() - $started
        }
        break
      }
      Start-Sleep -Milliseconds 10
    }
  }

  $mark = $null
  $deadline = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() + 60000
  while ([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() -lt $deadline) {
    $candidate = Read-LatestMark -Shell $Shell
    if ($candidate -and [double]$candidate.received -ge [double]$started) {
      $mark = $candidate
      break
    }
    Start-Sleep -Milliseconds 20
  }

  $paintMs = $null
  $inPageFcpMs = $null
  if ($mark) {
    $paintMs = [double]$mark.t - [double]$started
    $inPageFcpMs = [double]$mark.fcp
  } else {
    $tail = (Read-Marks -Path $MarksFile) | Select-Object -Last 2
    Write-Warning ("no paint mark for run {0}; baseline={1} marks_tail={2}" -f $run, $baseline, ($tail -join ' | '))
  }

  Start-Sleep -Seconds $IdleSeconds
  $footprint = Measure-Footprint -RootProcessId $treeRoot -Filter $ProcessFilter

  $record = [pscustomobject]@{
    shell            = $Shell
    headless         = [bool]$Headless
    run              = $run
    startEpochMs     = $started
    windowMs         = $windowMs
    paintMs          = if ($paintMs -ne $null) { [math]::Round($paintMs, 1) } else { $null }
    inPageFcpMs      = if ($inPageFcpMs -ne $null) { [math]::Round($inPageFcpMs, 1) } else { $null }
    processCount     = $footprint.ProcessCount
    mainWorkingSetMB = $footprint.MainWorkingSetMB
    totalWorkingSetMB = $footprint.TotalWorkingSetMB
    totalPrivateMB = $footprint.TotalPrivateMB
    relaunched       = $relaunched
    processes        = $footprint.Processes
  }
  $records += $record

  Write-Host ("[{0}] run {1}/{2}: window={3} ms, paint={4} ms, mem main={5} MB, tree={6} MB / {7} procs" -f `
      $Shell, $run, $Runs, $windowMs, $record.paintMs, $record.mainWorkingSetMB, $record.totalWorkingSetMB, $record.processCount)

  Stop-Tree -RootProcessId $treeRoot -Filter $ProcessFilter
  Start-Sleep -Milliseconds 1500
}

$paintValues = @($records | Where-Object { $_.paintMs -ne $null } | ForEach-Object { [double]$_.paintMs })
$windowValues = @($records | Where-Object { $_.windowMs -ne $null } | ForEach-Object { [double]$_.windowMs })
$memoryValues = @($records | ForEach-Object { [double]$_.totalWorkingSetMB })
$privateValues = @($records | ForEach-Object { [double]$_.totalPrivateMB })

$summary = [pscustomobject]@{
  shell                = $Shell
  exe                  = $Exe
  headless             = [bool]$Headless
  args                 = @($AppArgs)
  runs                 = $records.Count
  paintMsMedian        = Get-Median -Values $paintValues
  paintMsAll           = @($paintValues | ForEach-Object { [math]::Round($_, 1) })
  windowMsMedian       = Get-Median -Values $windowValues
  windowMsAll          = @($windowValues | ForEach-Object { [math]::Round($_, 1) })
  totalWorkingSetMBMedian = Get-Median -Values $memoryValues
  totalWorkingSetMBAll = @($memoryValues)
  totalPrivateMBMedian = Get-Median -Values $privateValues
  totalPrivateMBAll = @($privateValues)
  capturedAt           = (Get-Date).ToString('o')
}

$payload = [pscustomobject]@{ summary = $summary; records = @($records) }
$outPath = Join-Path $resultsDir ("measure-{0}.json" -f $Shell)
$payload | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $outPath -Encoding UTF8

Write-Host ''
Write-Host ("== {0} summary ==" -f $Shell)
Write-Host ("cold start (paint)  median: {0} ms   all: {1}" -f $summary.paintMsMedian, ($summary.paintMsAll -join ', '))
Write-Host ("window visible       median: {0} ms   all: {1}" -f $summary.windowMsMedian, ($summary.windowMsAll -join ', '))
Write-Host ("idle working set     median: {0} MB   all: {1}" -f $summary.totalWorkingSetMBMedian, ($summary.totalWorkingSetMBAll -join ', '))
Write-Host ("idle private bytes   median: {0} MB   all: {1}" -f $summary.totalPrivateMBMedian, ($summary.totalPrivateMBAll -join ', '))
Write-Host ("raw: {0}" -f $outPath)
