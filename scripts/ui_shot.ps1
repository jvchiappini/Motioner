<#
.SYNOPSIS
Opens `motioner preview` paused on a frame, captures the window and closes it.

.DESCRIPTION
Runs the debug (or -Release) binary with a fixed window size, waits for the
window to settle, captures its client area with capture_window.ps1 (no focus
needed) and stops the process. Screenshots go to out/ui/ (not committed).

.EXAMPLE
scripts/ui_shot.ps1 -Out out/ui/iter-01-layout.png
scripts/ui_shot.ps1 -Out out/ui/iter-02-720p.png -Size 1280x720 -Scale 1.25 -Select 3
scripts/ui_shot.ps1 -File out/ui/broken.motion -Out out/ui/iter-03-error.png
#>
param(
    [string]$File = 'examples/hello.motion',
    [Parameter(Mandatory = $true)][string]$Out,
    [int]$Frame = 120,
    [string]$Size = '1600x900',
    [string]$Scale = '',
    [int]$Select = -1,
    [double]$WaitSeconds = 4,
    [switch]$Release
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$profileDir = if ($Release) { 'release' } else { 'debug' }
$exe = Join-Path $root "target/$profileDir/motioner.exe"
if (-not (Test-Path $exe)) { throw "build first: $exe not found" }

$arguments = @('preview', $File, '--frame', "$Frame", '--paused', '--window-size', $Size)
if ($Scale -ne '') { $arguments += @('--ui-scale', $Scale) }
if ($Select -ge 0) { $arguments += @('--select', "$Select") }

$process = Start-Process -FilePath $exe -ArgumentList $arguments -WorkingDirectory $root -PassThru
try {
    Start-Sleep -Milliseconds ([int]($WaitSeconds * 1000))
    & (Join-Path $PSScriptRoot 'capture_window.ps1') -ProcessId $process.Id -Out $Out
} finally {
    if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force }
}
