#requires -Version 5.1
# SPDX-License-Identifier: MIT OR Apache-2.0
<#
.SYNOPSIS
  Build the desktop inputs: the `lkb` sidecar and the web SPA.

.DESCRIPTION
  Copies `target/<triple>/release/lkb[.exe]` to
  `desktop/src-tauri/binaries/lkb-<triple>[.exe]` (the name Tauri's
  `externalBin` expects) and `web/dist` to `desktop/src-tauri/resources/web`
  (bundled as a resource and served by `lkb serve` via LKB_WEB_DIR).

.PARAMETER Target
  Target triple to build for; defaults to the rustc host triple.

.PARAMETER SkipWeb
  Do not run `npm ci` / `npm run build` in web/.

.PARAMETER SkipRust
  Do not build the sidecar (only refresh the copied web assets).
#>
[CmdletBinding()]
param(
  [string]$Target = "",
  [switch]$SkipWeb,
  [switch]$SkipRust
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)

if (-not $Target) {
  $Target = (rustc -vV | Select-String '^host: ').ToString().Replace('host: ', '').Trim()
}
if (-not $Target) {
  throw "cannot determine the rustc host triple; pass -Target <triple>"
}
$exeSuffix = if ($Target -like "*windows*") { ".exe" } else { "" }

if (-not $SkipWeb) {
  Push-Location (Join-Path $root "web")
  try {
    npm ci --no-audit --no-fund
    npm run build
  }
  finally {
    Pop-Location
  }
}

$bin = ""
if (-not $SkipRust) {
  Push-Location $root
  try {
    cargo build --release --locked --target $Target
  }
  finally {
    Pop-Location
  }
  $bin = Join-Path $root "target/$Target/release/lkb$exeSuffix"
  if (-not (Test-Path $bin)) { throw "sidecar binary not found: $bin" }
}

$sidecarDir = Join-Path $root "desktop/src-tauri/binaries"
$webDir = Join-Path $root "desktop/src-tauri/resources/web"
New-Item -ItemType Directory -Force -Path $sidecarDir | Out-Null
if (Test-Path $webDir) { Remove-Item -Recurse -Force $webDir }
New-Item -ItemType Directory -Force -Path $webDir | Out-Null

if (-not $SkipRust) {
  $sidecar = Join-Path $sidecarDir "lkb-$Target$exeSuffix"
  Copy-Item $bin $sidecar -Force
  Write-Host "sidecar -> $sidecar"
}
Copy-Item (Join-Path $root "web/dist/*") $webDir -Recurse -Force
Write-Host "web     -> $webDir"
