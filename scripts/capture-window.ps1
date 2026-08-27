param(
    [Parameter(Mandatory = $true)]
    [int]$TargetProcessId,

    [Parameter(Mandatory = $true)]
    [string]$OutputPath
)

$ErrorActionPreference = 'Stop'

Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;

public static class WidgetWindowCapture {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern bool EnumChildWindows(IntPtr parent, EnumWindowsProc callback, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);

    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);

    [DllImport("user32.dll")]
    public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);

    [DllImport("user32.dll")]
    public static extern int GetWindowRgnBox(IntPtr hWnd, out RECT rect);

    [DllImport("user32.dll")]
    public static extern int GetWindowRgn(IntPtr hWnd, IntPtr region);

    [DllImport("gdi32.dll")]
    public static extern IntPtr CreateRectRgn(int left, int top, int right, int bottom);

    [DllImport("gdi32.dll")]
    public static extern bool PtInRegion(IntPtr region, int x, int y);

    [DllImport("gdi32.dll")]
    public static extern bool DeleteObject(IntPtr handle);

    [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW")]
    public static extern IntPtr GetWindowLongPtr(IntPtr hWnd, int index);

}
'@

$targetHandle = [IntPtr]::Zero
$largestArea = 0
$bestWidgetScore = [int]::MaxValue
$callback = [WidgetWindowCapture+EnumWindowsProc]{
    param([IntPtr]$windowHandle, [IntPtr]$unused)
    $windowProcessId = [uint32]0
    [void][WidgetWindowCapture]::GetWindowThreadProcessId($windowHandle, [ref]$windowProcessId)
    if ($windowProcessId -eq $TargetProcessId) {
        $candidateRect = New-Object WidgetWindowCapture+RECT
        if ([WidgetWindowCapture]::GetWindowRect($windowHandle, [ref]$candidateRect)) {
            $candidateWidth = $candidateRect.Right - $candidateRect.Left
            $candidateHeight = $candidateRect.Bottom - $candidateRect.Top
            $candidateArea = $candidateWidth * $candidateHeight
            $looksLikeWidget = $candidateWidth -ge 300 -and $candidateWidth -le 700 -and $candidateHeight -ge 350 -and $candidateHeight -le 800
            $widgetScore = [Math]::Abs($candidateWidth - 392) + [Math]::Abs($candidateHeight - 466)
            if ($looksLikeWidget -and $widgetScore -lt $script:bestWidgetScore) {
                $script:bestWidgetScore = $widgetScore
                $script:largestArea = $candidateArea
                $script:targetHandle = $windowHandle
            }
            elseif ($script:bestWidgetScore -eq [int]::MaxValue -and $candidateArea -gt $script:largestArea) {
                $script:largestArea = $candidateArea
                $script:targetHandle = $windowHandle
            }
        }
    }
    return $true
}
$topLevelHandles = [System.Collections.Generic.List[IntPtr]]::new()
$collectTopLevel = [WidgetWindowCapture+EnumWindowsProc]{
    param([IntPtr]$windowHandle, [IntPtr]$unused)
    $script:topLevelHandles.Add($windowHandle)
    return $true
}
[void][WidgetWindowCapture]::EnumWindows($collectTopLevel, [IntPtr]::Zero)
foreach ($topLevelHandle in $topLevelHandles) {
    [void]$callback.Invoke($topLevelHandle, [IntPtr]::Zero)
    [void][WidgetWindowCapture]::EnumChildWindows($topLevelHandle, $callback, [IntPtr]::Zero)
}

if ($targetHandle -eq [IntPtr]::Zero) {
    throw "No visible top-level window found for process $TargetProcessId"
}

$windowRect = New-Object WidgetWindowCapture+RECT
if (-not [WidgetWindowCapture]::GetWindowRect($targetHandle, [ref]$windowRect)) {
    throw "GetWindowRect failed for process $TargetProcessId"
}

$width = $windowRect.Right - $windowRect.Left
$height = $windowRect.Bottom - $windowRect.Top
if ($width -le 0 -or $height -le 0) {
    throw "Invalid window bounds ${width}x${height}"
}

$destination = [System.IO.Path]::GetFullPath($OutputPath)
$destinationDirectory = [System.IO.Path]::GetDirectoryName($destination)
[System.IO.Directory]::CreateDirectory($destinationDirectory) | Out-Null

$bitmap = [System.Drawing.Bitmap]::new(
    $width,
    $height,
    [System.Drawing.Imaging.PixelFormat]::Format32bppArgb
)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$deviceContext = $graphics.GetHdc()
try {
    $captured = [WidgetWindowCapture]::PrintWindow($targetHandle, $deviceContext, 2)
}
finally {
    $graphics.ReleaseHdc($deviceContext)
}

if (-not $captured) {
    $graphics.Dispose()
    $bitmap.Dispose()
    throw "PrintWindow failed for process $TargetProcessId; screen capture fallback is intentionally disabled"
}

$graphics.Dispose()
$bitmap.Save($destination, [System.Drawing.Imaging.ImageFormat]::Png)
$bitmap.Dispose()

$regionRect = New-Object WidgetWindowCapture+RECT
$regionKind = [WidgetWindowCapture]::GetWindowRgnBox($targetHandle, [ref]$regionRect)
$regionHandle = [WidgetWindowCapture]::CreateRectRgn(0, 0, 0, 0)
$regionCenterRows = $null
if ($regionHandle -ne [IntPtr]::Zero) {
    try {
        [void][WidgetWindowCapture]::GetWindowRgn($targetHandle, $regionHandle)
        $centerX = [int]($width / 2)
        $regionCenterRows = (0..3 | ForEach-Object {
            "y${_}=" + [WidgetWindowCapture]::PtInRegion($regionHandle, $centerX, $_)
        }) -join ','
    }
    finally {
        [void][WidgetWindowCapture]::DeleteObject($regionHandle)
    }
}
$windowStyle = [WidgetWindowCapture]::GetWindowLongPtr($targetHandle, -16).ToInt64()
$extendedWindowStyle = [WidgetWindowCapture]::GetWindowLongPtr($targetHandle, -20).ToInt64()

[pscustomobject]@{
    ProcessId = $TargetProcessId
    Handle = ('0x{0:X}' -f $targetHandle.ToInt64())
    WindowBounds = "${width}x${height} at $($windowRect.Left),$($windowRect.Top)"
    RegionKind = $regionKind
    RegionBounds = "$($regionRect.Left),$($regionRect.Top)-$($regionRect.Right),$($regionRect.Bottom)"
    RegionCenterRows = $regionCenterRows
    WindowStyle = ('0x{0:X8}' -f $windowStyle)
    ExtendedWindowStyle = ('0x{0:X8}' -f $extendedWindowStyle)
    CaptureMethod = 'PrintWindow'
    OutputPath = $destination
} | ConvertTo-Json
