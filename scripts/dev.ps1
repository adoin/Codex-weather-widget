#requires -Version 5.1

[CmdletBinding()]
param(
  [ValidateRange(100, 5000)]
  [int]$PollMilliseconds = 450,

  [ValidateRange(100, 5000)]
  [int]$DebounceMilliseconds = 650
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version 2.0

$script:RepoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$script:TargetExe = Join-Path $script:RepoRoot "target\debug\codex-calendar.exe"
$script:DevRunRoot = Join-Path $script:RepoRoot "target\dev-run"
$script:CurrentProcess = $null
$script:CurrentRunExe = $null
$script:RunCounter = 0

function Write-DevMessage {
  param(
    [Parameter(Mandatory = $true)][string]$Message,
    [ConsoleColor]$Color = [ConsoleColor]::DarkGray
  )
  Write-Host ("[dev] " + $Message) -ForegroundColor $Color
}
function Test-PathInsideRepo {
  param([Parameter(Mandatory = $true)][string]$Path)
  $resolved = [IO.Path]::GetFullPath($Path)
  $prefix = $script:RepoRoot.TrimEnd('\') + '\'
  return $resolved.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)
}

function Stop-ProcessGracefully {
  param([System.Diagnostics.Process]$Process)
  if ($null -eq $Process) { return }
  try {
    if ($Process.HasExited) { return }
    Stop-Process -Id $Process.Id -ErrorAction SilentlyContinue
    if (-not $Process.WaitForExit(3000)) {
      Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
      [void]$Process.WaitForExit(2000)
    }
  } catch { }
}

function Stop-StaleWorkspaceInstances {
  $stopped = 0
  Get-Process -Name "codex-calendar" -ErrorAction SilentlyContinue | ForEach-Object {
    try {
      $processPath = $_.Path
      if ($processPath -and (Test-PathInsideRepo -Path $processPath)) {
        Stop-Process -Id $_.Id -Force -ErrorAction Stop
        $stopped += 1
      }
    } catch { }
  }
  if ($stopped -gt 0) {
    Write-DevMessage "Stopped $stopped stale workspace instance(s)." Yellow
    Start-Sleep -Milliseconds 300
  }
}

function Test-ActionListenerInUse {
  try {
    $health = Invoke-RestMethod -UseBasicParsing -Uri "http://127.0.0.1:8787/health" -TimeoutSec 1
    return [bool]$health.ok
  } catch {
    return $false
  }
}

function Get-WatchedFiles {
  $files = New-Object System.Collections.Generic.List[System.IO.FileInfo]
  foreach ($relativeRoot in @("src", "assets")) {
    $root = Join-Path $script:RepoRoot $relativeRoot
    if (Test-Path -LiteralPath $root) {
      Get-ChildItem -LiteralPath $root -Recurse -File -ErrorAction SilentlyContinue | ForEach-Object {
        [void]$files.Add($_)
      }
    }
  }
  foreach ($name in @("Cargo.toml", "Cargo.lock", "build.rs")) {
    $path = Join-Path $script:RepoRoot $name
    if (Test-Path -LiteralPath $path) {
      [void]$files.Add((Get-Item -LiteralPath $path))
    }
  }
  return $files | Sort-Object FullName
}

function Get-SourceFingerprint {
  $metadata = Get-WatchedFiles | ForEach-Object {
    "{0}|{1}|{2}" -f $_.FullName, $_.Length, $_.LastWriteTimeUtc.Ticks
  }
  $text = [String]::Join("`n", @($metadata))
  $sha = [Security.Cryptography.SHA256]::Create()
  try {
    $bytes = [Text.Encoding]::UTF8.GetBytes($text)
    return [BitConverter]::ToString($sha.ComputeHash($bytes)).Replace("-", "")
  } finally {
    $sha.Dispose()
  }
}

function Invoke-DevelopmentBuild {
  Write-DevMessage "Building debug target..." Cyan
  Push-Location $script:RepoRoot
  try {
    & cargo build
    if ($LASTEXITCODE -ne 0) {
      Write-DevMessage "Build failed; the previous widget remains running." Red
      return $false
    }
  } finally {
    Pop-Location
  }
  if (-not (Test-Path -LiteralPath $script:TargetExe)) {
    Write-DevMessage "Build succeeded but the executable was not found." Red
    return $false
  }
  return $true
}

function Start-DevelopmentInstance {
  New-Item -ItemType Directory -Force -Path $script:DevRunRoot | Out-Null
  $script:RunCounter += 1
  $nextRunExe = Join-Path $script:DevRunRoot ("codex-calendar-dev-{0}-{1}.exe" -f $PID, $script:RunCounter)
  Copy-Item -LiteralPath $script:TargetExe -Destination $nextRunExe -Force

  $previousProcess = $script:CurrentProcess
  $previousRunExe = $script:CurrentRunExe
  Stop-ProcessGracefully -Process $previousProcess

  $script:CurrentProcess = Start-Process `
    -FilePath $nextRunExe `
    -WorkingDirectory $script:RepoRoot `
    -WindowStyle Hidden `
    -PassThru
  $script:CurrentRunExe = $nextRunExe

  if ($previousRunExe -and (Test-PathInsideRepo -Path $previousRunExe)) {
    Remove-Item -LiteralPath $previousRunExe -Force -ErrorAction SilentlyContinue
  }
  Write-DevMessage ("Started PID {0}. Watching for changes..." -f $script:CurrentProcess.Id) Green
}

function Remove-OldDevelopmentCopies {
  if (-not (Test-Path -LiteralPath $script:DevRunRoot)) { return }
  if (-not (Test-PathInsideRepo -Path $script:DevRunRoot)) {
    throw "Refusing to clean a development directory outside the repository."
  }
  Get-ChildItem -LiteralPath $script:DevRunRoot -File -Filter "codex-calendar-dev-*.exe" -ErrorAction SilentlyContinue |
    Remove-Item -Force -ErrorAction SilentlyContinue
}

Stop-StaleWorkspaceInstances
if (Test-ActionListenerInUse) {
  throw "Port 8787 is already served by another Codex Calendar installation. Exit that instance before starting the development environment."
}
Remove-OldDevelopmentCopies

try {
  if (-not (Invoke-DevelopmentBuild)) { exit 1 }
  Start-DevelopmentInstance

  $fingerprint = Get-SourceFingerprint
  $changePending = $false
  $lastChangeAt = [DateTime]::MinValue

  while ($true) {
    Start-Sleep -Milliseconds $PollMilliseconds
    $nextFingerprint = Get-SourceFingerprint
    if ($nextFingerprint -ne $fingerprint) {
      $fingerprint = $nextFingerprint
      $changePending = $true
      $lastChangeAt = [DateTime]::UtcNow
      Write-DevMessage "Change detected; waiting for writes to settle..." DarkCyan
    }

    if ($changePending -and ([DateTime]::UtcNow - $lastChangeAt).TotalMilliseconds -ge $DebounceMilliseconds) {
      $changePending = $false
      if (Invoke-DevelopmentBuild) {
        Start-DevelopmentInstance
      }
      $fingerprint = Get-SourceFingerprint
    }
  }
} finally {
  Write-DevMessage "Stopping development instance..." Yellow
  Stop-ProcessGracefully -Process $script:CurrentProcess
  if ($script:CurrentRunExe -and (Test-PathInsideRepo -Path $script:CurrentRunExe)) {
    Remove-Item -LiteralPath $script:CurrentRunExe -Force -ErrorAction SilentlyContinue
  }
}
