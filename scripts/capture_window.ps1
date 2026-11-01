<#
.SYNOPSIS
Captures the client area of a process's main window to a PNG, without focus.

.DESCRIPTION
Uses Win32 PrintWindow with PW_CLIENTONLY | PW_RENDERFULLCONTENT (1 | 2), which
asks DWM for the composed window content, so GPU (wgpu/DX12/Vulkan) frames are
captured even when the window is behind other windows. A capture that is
entirely black is reported as an error (exit 2) so callers notice when the
compositor returned nothing.

.EXAMPLE
scripts/capture_window.ps1 -ProcessId 1234 -Out out/ui/iter-01.png
#>
param(
    [Parameter(Mandatory = $true)][int]$ProcessId,
    [Parameter(Mandatory = $true)][string]$Out,
    [int]$TimeoutSeconds = 20
)

$ErrorActionPreference = 'Stop'

Add-Type -AssemblyName System.Drawing
# A stale entry in LIB (for example an uninstalled CUDA toolkit) makes the C#
# compiler behind Add-Type fail with "warning as error"; it needs no LIB.
$env:LIB = $null
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class MotionerCapture {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
}
'@

# Physical pixels: without this, GetClientRect is virtualised on scaled displays.
[void][MotionerCapture]::SetProcessDPIAware()

$deadline = (Get-Date).AddSeconds($TimeoutSeconds)
$hwnd = [IntPtr]::Zero
while ((Get-Date) -lt $deadline) {
    $process = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
    if ($null -eq $process) { throw "process $ProcessId is not running" }
    $process.Refresh()
    if ($process.MainWindowHandle -ne [IntPtr]::Zero) { $hwnd = $process.MainWindowHandle; break }
    Start-Sleep -Milliseconds 100
}
if ($hwnd -eq [IntPtr]::Zero) { throw "process $ProcessId has no main window after $TimeoutSeconds s" }

$rect = New-Object MotionerCapture+RECT
[void][MotionerCapture]::GetClientRect($hwnd, [ref]$rect)
$width = $rect.Right - $rect.Left
$height = $rect.Bottom - $rect.Top
if ($width -le 0 -or $height -le 0) { throw "window client area is empty (${width}x${height})" }

$bitmap = New-Object System.Drawing.Bitmap $width, $height
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$hdc = $graphics.GetHdc()
$ok = [MotionerCapture]::PrintWindow($hwnd, $hdc, 3)
$graphics.ReleaseHdc($hdc)
$graphics.Dispose()
if (-not $ok) { $bitmap.Dispose(); throw 'PrintWindow failed' }

# Reject an all-black capture (the compositor gave us nothing).
$lit = $false
for ($y = 0; $y -lt $height -and -not $lit; $y += [Math]::Max(1, [int]($height / 16))) {
    for ($x = 0; $x -lt $width; $x += [Math]::Max(1, [int]($width / 16))) {
        $p = $bitmap.GetPixel($x, $y)
        if (($p.R + $p.G + $p.B) -gt 0) { $lit = $true; break }
    }
}

$full = [System.IO.Path]::GetFullPath((Join-Path (Get-Location) $Out))
$dir = Split-Path -Parent $full
if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Force $dir | Out-Null }
$bitmap.Save($full, [System.Drawing.Imaging.ImageFormat]::Png)
$bitmap.Dispose()
Write-Output "captured ${width}x${height} -> $full"
if (-not $lit) { Write-Error 'capture is entirely black'; exit 2 }
