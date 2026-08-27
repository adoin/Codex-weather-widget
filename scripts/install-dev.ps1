$ErrorActionPreference = "Stop"
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$installRoot = Join-Path $env:LOCALAPPDATA "Programs\CodexCalendar"
$skillRoot = Join-Path $env:USERPROFILE ".codex\skills\quota"
$startupRoot = [Environment]::GetFolderPath("Startup")

Push-Location $repoRoot
try {
  cargo build --release --locked
  New-Item -ItemType Directory -Force -Path $installRoot | Out-Null
  Copy-Item -LiteralPath (Join-Path $repoRoot "target\release\codex-calendar.exe") -Destination (Join-Path $installRoot "codex-calendar.exe") -Force
  New-Item -ItemType Directory -Force -Path $skillRoot | Out-Null
  Copy-Item -LiteralPath (Join-Path $repoRoot "codex-plugin\quota-calendar\skills\quota\SKILL.md") -Destination (Join-Path $skillRoot "SKILL.md") -Force
  Copy-Item -LiteralPath (Join-Path $repoRoot "codex-plugin\quota-calendar\skills\quota\scripts") -Destination $skillRoot -Recurse -Force

  $shell = New-Object -ComObject WScript.Shell
  $shortcut = $shell.CreateShortcut((Join-Path $startupRoot "Codex Calendar.lnk"))
  $shortcut.TargetPath = Join-Path $installRoot "codex-calendar.exe"
  $shortcut.WorkingDirectory = $installRoot
  $shortcut.WindowStyle = 7
  $shortcut.Save()

  Start-Process -FilePath (Join-Path $installRoot "codex-calendar.exe") -WorkingDirectory $installRoot -WindowStyle Hidden
  Write-Host "Codex Calendar installed to $installRoot"
  Write-Host "Restart Codex once, then type /quota in a new task."
} finally {
  Pop-Location
}
