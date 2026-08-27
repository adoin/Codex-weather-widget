$ErrorActionPreference = "Stop"
$installRoot = if ($env:CODEX_CALENDAR_INSTALL_DIR) {
  [IO.Path]::GetFullPath($env:CODEX_CALENDAR_INSTALL_DIR)
} else {
  Join-Path $env:LOCALAPPDATA "Programs\CodexCalendar"
}
$executable = Join-Path $installRoot "codex-calendar.exe"
$config = Join-Path $env:APPDATA "CodexCalendar\config.json"
$port = 8787

if (Test-Path -LiteralPath $config) {
  try {
    $configured = [int](Get-Content -LiteralPath $config -Raw | ConvertFrom-Json).port
    if ($configured -ge 1024 -and $configured -le 65535) { $port = $configured }
  } catch { }
}

$baseUrl = "http://127.0.0.1:$port"
function Test-CodexCalendar {
  try { return [bool](Invoke-RestMethod -Uri "$baseUrl/health" -TimeoutSec 1).ok }
  catch { return $false }
}

if (Test-CodexCalendar) {
  Invoke-RestMethod -Method Post -Uri "$baseUrl/panel/toggle" -TimeoutSec 3 | ConvertTo-Json -Compress
  exit 0
}

if (-not (Test-Path -LiteralPath $executable)) {
  throw "Codex Calendar is not installed. Run scripts/install-dev.ps1 from the repository."
}
Start-Process -FilePath $executable -WorkingDirectory $installRoot -WindowStyle Hidden
$deadline = (Get-Date).AddSeconds(15)
do { Start-Sleep -Milliseconds 300 } until ((Test-CodexCalendar) -or (Get-Date) -ge $deadline)
if (-not (Test-CodexCalendar)) { throw "Codex Calendar did not start within 15 seconds." }
Invoke-RestMethod -Method Post -Uri "$baseUrl/panel/show" -TimeoutSec 3 | ConvertTo-Json -Compress
