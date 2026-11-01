<#
.SYNOPSIS
Drives a running `motioner preview` window through the phase 12 manual run and
captures screenshots.

.DESCRIPTION
Starts the release binary on a copy of hello.motion, clicks and drags with
posted mouse messages, types and presses shortcuts with SendKeys (the window is
focused for those), and captures screenshots through capture_window.ps1.

.EXAMPLE
powershell -ExecutionPolicy Bypass -File scripts/ui_drive.ps1
#>
param(
    [string]$File = 'out/edit/hello.motion',
    [string]$Prefix = 'out/ui/p08',
    [int]$Frame = 120,
    [string]$Size = '1600x900'
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $root 'target/release/motioner.exe'
if (-not (Test-Path $exe)) { throw "build first: $exe" }

$env:LIB = $null
Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class UiDrive {
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hWnd, uint msg, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hWnd, ref POINT p);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X; public int Y; }
}
'@
[void][UiDrive]::SetProcessDPIAware()

$WM_MOUSEMOVE = 0x0200
$WM_LBUTTONDOWN = 0x0201
$WM_LBUTTONUP = 0x0202

function MakeLParam([int]$x, [int]$y) {
    return [IntPtr](($y -shl 16) -bor ($x -band 0xFFFF))
}

function Move-To([IntPtr]$hwnd, [int]$x, [int]$y) {
    [void][UiDrive]::PostMessage($hwnd, $WM_MOUSEMOVE, [IntPtr]::Zero, (MakeLParam $x $y))
    # Keep the real cursor in sync so hover states match the posted clicks.
    $point = New-Object UiDrive+POINT
    $point.X = $x
    $point.Y = $y
    [void][UiDrive]::ClientToScreen($hwnd, [ref]$point)
    [void][UiDrive]::SetCursorPos($point.X, $point.Y)
}

function Wheel([IntPtr]$hwnd, [int]$x, [int]$y, [int]$notches) {
    Move-To $hwnd $x $y
    $point = New-Object UiDrive+POINT
    $point.X = $x
    $point.Y = $y
    [void][UiDrive]::ClientToScreen($hwnd, [ref]$point)
    $delta = $notches * 120
    $wParam = [IntPtr](($delta -band 0xFFFF) -shl 16)
    $lParam = [IntPtr](($point.Y -shl 16) -bor ($point.X -band 0xFFFF))
    [void][UiDrive]::PostMessage($hwnd, 0x020A, $wParam, $lParam)
    Start-Sleep -Milliseconds 250
}

function Click([IntPtr]$hwnd, [int]$x, [int]$y) {
    Move-To $hwnd $x $y
    Start-Sleep -Milliseconds 80
    [void][UiDrive]::PostMessage($hwnd, $WM_LBUTTONDOWN, [IntPtr]1, (MakeLParam $x $y))
    Start-Sleep -Milliseconds 100
    [void][UiDrive]::PostMessage($hwnd, $WM_LBUTTONUP, [IntPtr]0, (MakeLParam $x $y))
    Start-Sleep -Milliseconds 180
}

function Drag([IntPtr]$hwnd, [int]$x0, [int]$y0, [int]$x1, [int]$y1) {
    Move-To $hwnd $x0 $y0
    Start-Sleep -Milliseconds 120
    [void][UiDrive]::PostMessage($hwnd, $WM_LBUTTONDOWN, [IntPtr]1, (MakeLParam $x0 $y0))
    Start-Sleep -Milliseconds 180
    $steps = 10
    for ($i = 1; $i -le $steps; $i++) {
        $x = [int]($x0 + ($x1 - $x0) * $i / $steps)
        $y = [int]($y0 + ($y1 - $y0) * $i / $steps)
        [void][UiDrive]::PostMessage($hwnd, $WM_MOUSEMOVE, [IntPtr]1, (MakeLParam $x $y))
        Start-Sleep -Milliseconds 50
    }
    Start-Sleep -Milliseconds 150
    [void][UiDrive]::PostMessage($hwnd, $WM_LBUTTONUP, [IntPtr]0, (MakeLParam $x1 $y1))
    Start-Sleep -Milliseconds 400
}

function Send-Keys([IntPtr]$hwnd, [string]$keys) {
    [void][UiDrive]::SetForegroundWindow($hwnd)
    Start-Sleep -Milliseconds 150
    [System.Windows.Forms.SendKeys]::SendWait($keys)
    Start-Sleep -Milliseconds 300
}

function Shot([int]$processId, [string]$name) {
    & (Join-Path $PSScriptRoot 'capture_window.ps1') -ProcessId $processId -Out "$Prefix-$name.png" | Out-Null
    Write-Output "shot $Prefix-$name.png"
    Start-Sleep -Milliseconds 300
}

function Start-Editor {
    $arguments = @('preview', $File, '--frame', "$Frame", '--paused', '--window-size', $Size)
    $process = Start-Process -FilePath $exe -ArgumentList $arguments -WorkingDirectory $root -PassThru
    $deadline = (Get-Date).AddSeconds(25)
    while ((Get-Date) -lt $deadline) {
        $process.Refresh()
        if ($process.MainWindowHandle -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 150
    }
    if ($process.MainWindowHandle -eq [IntPtr]::Zero) { throw 'no window' }
    Start-Sleep -Milliseconds 3500
    $process.Refresh()
    return $process
}

# Client coordinates for a 1600x900 window at 125 % DPI, measured from earlier
# captures: layer rows are 36 px apart from y = 113, the title centre is
# (782, 287), the inspector Colour field is (1450, 515), the timeline rows
# start at y = 679 and frame 120 is x = 840.
$rowTitleY = 113
$rowCircle1Y = 149
$rowGroup1Y = 329
$rowRect1Y = 365
$eyeX = 300
$lockX = 276
$titleX = 782
$titleY = 287
$colourX = 1450
$colourY = 544
$positionYRow = 734
$easeX = 1450
$easeY = 559

New-Item -ItemType Directory -Force -Path (Join-Path $root 'out/ui') | Out-Null

$process = Start-Editor
$hwnd = $process.MainWindowHandle
[void][UiDrive]::SetForegroundWindow($hwnd)

# 1. Select the title: the canvas shows the gizmo and the property timeline.
Click $hwnd 100 $rowTitleY
Shot $process.Id '01-selection-gizmo'

# 2. Add keys at the playhead with K.
Send-Keys $hwnd 'k'
Start-Sleep -Milliseconds 500
Shot $process.Id '02-keys-added'

# 3. Move the title with the gizmo.
Drag $hwnd $titleX $titleY $titleX ($titleY + 40)
Shot $process.Id '03-moved'

# 4. Undo, then redo.
Send-Keys $hwnd '^z'
Start-Sleep -Milliseconds 400
Shot $process.Id '04-undo'
Send-Keys $hwnd '^y'
Start-Sleep -Milliseconds 400
Shot $process.Id '05-redo'

# 5. Change the text colour in the inspector (captured while editing). The
#    colour row sits below the fold, so the inspector is scrolled first.
Wheel $hwnd 1450 400 -2
Click $hwnd $colourX $colourY
Send-Keys $hwnd '#FF3366'
Shot $process.Id '06-inspector-editing'
Send-Keys $hwnd '{ENTER}'
Start-Sleep -Milliseconds 500

# 6. Select the opacity key at the playhead and retime it, then ease it.
#    The Keys section sits below the fold, so the inspector is scrolled first.
Click $hwnd 840 $positionYRow
Drag $hwnd 840 $positionYRow 980 $positionYRow
Wheel $hwnd 1450 500 -4
Start-Sleep -Milliseconds 300
Shot $process.Id '07a-keys-scrolled'
Click $hwnd $easeX $easeY
Start-Sleep -Milliseconds 400
Shot $process.Id '07-property-timeline'

# 7. Collapse the title, select Rect 1 and collapse it: the timeline shows the
#    visibility strips and the parent keys inherited by Rect 1.
Click $hwnd 24 $rowTitleY
Click $hwnd 100 $rowRect1Y
Click $hwnd 24 $rowRect1Y
Wheel $hwnd 800 780 -2
Click $hwnd 840 700
Shot $process.Id '08-collapsed-timeline'

# 8. Hide a circle and lock a spinner.
Click $hwnd $eyeX $rowCircle1Y
Click $hwnd $lockX $rowGroup1Y
Shot $process.Id '09-hidden-locked'

# 9. An external edit adds an override for a node that does not exist; the
#    editor reloads it and reports it in the layers panel.
$overrides = Join-Path $root 'out/edit/hello.overrides.motion'
Add-Content -LiteralPath $overrides -Value 'override("ghost-node", "opacity", 0.5)' -Encoding utf8
Start-Sleep -Milliseconds 1500
Shot $process.Id '10-orphans'
# Delete the orphan from its row (the trash action on the right).
Click $hwnd 300 505
Start-Sleep -Milliseconds 600
Shot $process.Id '10b-orphans-deleted'

# 10. Close and reopen: the overrides persist.
Send-Keys $hwnd '{ESC}'
Start-Sleep -Milliseconds 2500
$process = Start-Editor
$hwnd = $process.MainWindowHandle
[void][UiDrive]::SetForegroundWindow($hwnd)
Click $hwnd 100 $rowTitleY
Shot $process.Id '11-reopened'
Send-Keys $hwnd '{ESC}'
Start-Sleep -Milliseconds 1500
# 11. Bake: fold the overrides into the program and reopen.
Write-Output 'bake'
& $exe bake $File | Out-Host
$process = Start-Editor
$hwnd = $process.MainWindowHandle
[void][UiDrive]::SetForegroundWindow($hwnd)
Click $hwnd 100 $rowTitleY
Shot $process.Id '12-baked'
Send-Keys $hwnd '{ESC}'
Start-Sleep -Milliseconds 1000
Write-Output 'driver done'
