$ErrorActionPreference = "Stop"

Add-Type -AssemblyName System.Drawing

$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$iconDir = Join-Path $repoRoot "assets\icons"
$iconPath = Join-Path $iconDir "codex-weather-widget.ico"
$pngPath = Join-Path ([IO.Path]::GetTempPath()) ("codex-weather-widget-{0}.png" -f [Guid]::NewGuid())
New-Item -ItemType Directory -Force -Path $iconDir | Out-Null

function New-RoundedRectanglePath([System.Drawing.RectangleF]$rectangle, [float]$radius) {
  $diameter = $radius * 2
  $path = [System.Drawing.Drawing2D.GraphicsPath]::new()
  $path.AddArc($rectangle.X, $rectangle.Y, $diameter, $diameter, 180, 90)
  $path.AddArc($rectangle.Right - $diameter, $rectangle.Y, $diameter, $diameter, 270, 90)
  $path.AddArc($rectangle.Right - $diameter, $rectangle.Bottom - $diameter, $diameter, $diameter, 0, 90)
  $path.AddArc($rectangle.X, $rectangle.Bottom - $diameter, $diameter, $diameter, 90, 90)
  $path.CloseFigure()
  return $path
}

try {
  $bitmap = [System.Drawing.Bitmap]::new(256, 256, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
  $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
  $graphics.Clear([System.Drawing.Color]::Transparent)

  $background = New-RoundedRectanglePath ([System.Drawing.RectangleF]::new(12, 12, 232, 232)) 52
  $backgroundBrush = [System.Drawing.Drawing2D.LinearGradientBrush]::new(
    [System.Drawing.PointF]::new(28, 24),
    [System.Drawing.PointF]::new(224, 232),
    [System.Drawing.Color]::FromArgb(255, 28, 58, 72),
    [System.Drawing.Color]::FromArgb(255, 63, 89, 111)
  )
  $graphics.FillPath($backgroundBrush, $background)
  $graphics.DrawPath([System.Drawing.Pen]::new([System.Drawing.Color]::FromArgb(160, 218, 238, 239), 4), $background)

  $sunBrush = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(255, 255, 203, 73))
  $graphics.FillEllipse($sunBrush, 145, 48, 58, 58)

  $cloudBrush = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(255, 224, 242, 245))
  $graphics.FillEllipse($cloudBrush, 52, 104, 82, 76)
  $graphics.FillEllipse($cloudBrush, 100, 83, 94, 99)
  $graphics.FillEllipse($cloudBrush, 150, 111, 58, 68)
  $cloudBody = New-RoundedRectanglePath ([System.Drawing.RectangleF]::new(51, 132, 158, 57)) 25
  $graphics.FillPath($cloudBrush, $cloudBody)

  $rainPen = [System.Drawing.Pen]::new([System.Drawing.Color]::FromArgb(255, 95, 190, 244), 10)
  $rainPen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
  $rainPen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
  $graphics.DrawLine($rainPen, 86, 199, 77, 218)
  $graphics.DrawLine($rainPen, 132, 199, 123, 218)
  $graphics.DrawLine($rainPen, 178, 199, 169, 218)

  $bitmap.Save($pngPath, [System.Drawing.Imaging.ImageFormat]::Png)
  $png = [IO.File]::ReadAllBytes($pngPath)
  $stream = [IO.File]::Open($iconPath, [IO.FileMode]::Create, [IO.FileAccess]::Write)
  $writer = [IO.BinaryWriter]::new($stream)
  $writer.Write([UInt16]0)
  $writer.Write([UInt16]1)
  $writer.Write([UInt16]1)
  $writer.Write([Byte]0)
  $writer.Write([Byte]0)
  $writer.Write([Byte]0)
  $writer.Write([Byte]0)
  $writer.Write([UInt16]1)
  $writer.Write([UInt16]32)
  $writer.Write([UInt32]$png.Length)
  $writer.Write([UInt32]22)
  $writer.Write($png)
  $writer.Dispose()

  Write-Host "Generated $iconPath"
} finally {
  if ($graphics) { $graphics.Dispose() }
  if ($bitmap) { $bitmap.Dispose() }
  if (Test-Path -LiteralPath $pngPath) { Remove-Item -LiteralPath $pngPath -Force }
}
