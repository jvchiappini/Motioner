<#
.SYNOPSIS
Opens `motioner preview`, optionally drives it with `ctl` commands, writes a
screenshot and closes it.

.DESCRIPTION
Without -Commands: runs the debug (or -Release) binary paused on -Frame with
the in-app screenshot hook (`--screenshot`) and exits after -Frames frames.

With -Commands: starts the editor with a private control endpoint, sends each
command through `motioner ctl` (the same commands a person's clicks become,
see docs/agents.md), captures with `screenshot(...)` and sends `quit()`.
No focus, no mouse coordinates. Screenshots go to out/ui/ (not committed).

.EXAMPLE
scripts/ui_shot.ps1 -Out out/ui/iter-01-layout.png
scripts/ui_shot.ps1 -Out out/ui/iter-02-720p.png -Size 1280x720 -Scale 1.25 -Select 3
scripts/ui_shot.ps1 -Out out/ui/keys.png -Commands 'select("@288")','select_keys([("@288", "pos.x", 0.8)])'
#>
param(
    [string]$File = 'examples/hello.motion',
    [Parameter(Mandatory = $true)][string]$Out,
    [int]$Frame = 120,
    [string]$Size = '1600x900',
    [string]$Scale = '',
    [int]$Select = -1,
    [int]$Frames = 20,
    [string[]]$Commands = @(),
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

Push-Location $root
try {
    if ($Commands.Count -eq 0) {
        & $exe @arguments --exit-after-frames "$Frames" --screenshot $Out
        if ($LASTEXITCODE -ne 0) { throw "motioner preview failed with exit code $LASTEXITCODE" }
        return
    }
    $endpoint = Join-Path ([System.IO.Path]::GetTempPath()) "motioner-shot-$PID.txt"
    $process = Start-Process -FilePath $exe -ArgumentList ($arguments + @('--control-file', $endpoint)) -PassThru
    try {
        $deadline = (Get-Date).AddSeconds(60)
        while (-not (Test-Path $endpoint)) {
            if ($process.HasExited) { throw 'the editor exited before opening its control endpoint' }
            if ((Get-Date) -gt $deadline) { throw 'the control endpoint never appeared' }
            Start-Sleep -Milliseconds 100
        }
        # Windows PowerShell 5.1 drops unescaped double quotes when it passes
        # arguments to native programs.
        $quote = { param($text) $text -replace '"', '\"' }
        foreach ($command in $Commands) {
            & $exe ctl --endpoint $endpoint (& $quote $command)
        }
        $target = $Out -replace '\\', '/'
        & $exe ctl --endpoint $endpoint (& $quote "screenshot(`"$target`")")
        & $exe ctl --endpoint $endpoint 'quit()' | Out-Null
        $process.WaitForExit(20000) | Out-Null
    } finally {
        if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force }
    }
} finally {
    Pop-Location
}
