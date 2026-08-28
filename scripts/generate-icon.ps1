$ErrorActionPreference = "Stop"

$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$svgPath = Join-Path $repoRoot "logos\iterations\iteration-1-codex-sun-monogram.svg"
$iconDir = Join-Path $repoRoot "assets\icons"
$iconPath = Join-Path $iconDir "codex-weather-widget.ico"
$trayPngPath = Join-Path $iconDir "codex-weather-widget-32.png"
$previewPngPath = Join-Path $iconDir "codex-weather-widget-256.png"
$renderer = Join-Path $PSScriptRoot "render-icon.cjs"
$workDir = Join-Path ([IO.Path]::GetTempPath()) ("codex-weather-icon-{0}" -f [Guid]::NewGuid())
$sizes = @(256, 128, 64, 48, 32, 24, 16)

if (-not (Test-Path -LiteralPath $svgPath)) {
  throw "Logo source not found: $svgPath"
}

New-Item -ItemType Directory -Force -Path $iconDir | Out-Null
New-Item -ItemType Directory -Force -Path $workDir | Out-Null

try {
  & npm install --silent --no-save --prefix $workDir "@resvg/resvg-js@2.6.2"
  if ($LASTEXITCODE -ne 0) {
    throw "Unable to install the pinned SVG renderer"
  }

  $resvgModule = Join-Path $workDir "node_modules\@resvg\resvg-js"
  & node $renderer $svgPath $workDir $resvgModule ($sizes -join ",")
  if ($LASTEXITCODE -ne 0) {
    throw "SVG rendering failed"
  }

  $frames = foreach ($size in $sizes) {
    $path = Join-Path $workDir ("icon-{0}.png" -f $size)
    if (-not (Test-Path -LiteralPath $path)) {
      throw "Missing rendered icon frame: $path"
    }
    [pscustomobject]@{
      Size = $size
      Bytes = [IO.File]::ReadAllBytes($path)
    }
  }

  $stream = [IO.File]::Open($iconPath, [IO.FileMode]::Create, [IO.FileAccess]::Write)
  $writer = [IO.BinaryWriter]::new($stream)
  try {
    $writer.Write([UInt16]0)
    $writer.Write([UInt16]1)
    $writer.Write([UInt16]$frames.Count)

    $offset = 6 + (16 * $frames.Count)
    foreach ($frame in $frames) {
      $dimension = if ($frame.Size -eq 256) { [Byte]0 } else { [Byte]$frame.Size }
      $writer.Write($dimension)
      $writer.Write($dimension)
      $writer.Write([Byte]0)
      $writer.Write([Byte]0)
      $writer.Write([UInt16]1)
      $writer.Write([UInt16]32)
      $writer.Write([UInt32]$frame.Bytes.Length)
      $writer.Write([UInt32]$offset)
      $offset += $frame.Bytes.Length
    }

    foreach ($frame in $frames) {
      $writer.Write($frame.Bytes)
    }
  } finally {
    $writer.Dispose()
  }

  Copy-Item -LiteralPath (Join-Path $workDir "icon-32.png") -Destination $trayPngPath -Force
  Copy-Item -LiteralPath (Join-Path $workDir "icon-256.png") -Destination $previewPngPath -Force

  Write-Host "Generated $iconPath"
  Write-Host "Generated $trayPngPath"
  Write-Host "Generated $previewPngPath"
} finally {
  $resolvedTemp = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
  $resolvedWork = [IO.Path]::GetFullPath($workDir)
  if ($resolvedWork.StartsWith($resolvedTemp, [StringComparison]::OrdinalIgnoreCase) -and
      (Test-Path -LiteralPath $resolvedWork)) {
    Remove-Item -LiteralPath $resolvedWork -Recurse -Force
  }
}
