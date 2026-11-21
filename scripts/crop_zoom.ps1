<#
.SYNOPSIS
Crops a region of a PNG and enlarges it with nearest-neighbour sampling, to
inspect icons and text rendering pixel by pixel.

.EXAMPLE
scripts/crop_zoom.ps1 -In out/ui/iter-01.png -Out out/ui/zoom.png -X 670 -Y 580 -W 260 -H 44 -Factor 4
#>
param(
    [Parameter(Mandatory = $true)][string]$In,
    [Parameter(Mandatory = $true)][string]$Out,
    [Parameter(Mandatory = $true)][int]$X,
    [Parameter(Mandatory = $true)][int]$Y,
    [Parameter(Mandatory = $true)][int]$W,
    [Parameter(Mandatory = $true)][int]$H,
    [int]$Factor = 4
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$source = [System.Drawing.Bitmap]::FromFile((Resolve-Path $In))
$target = New-Object System.Drawing.Bitmap ($W * $Factor), ($H * $Factor)
$g = [System.Drawing.Graphics]::FromImage($target)
$g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::NearestNeighbor
$g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::Half
$g.DrawImage($source, (New-Object System.Drawing.Rectangle 0, 0, ($W * $Factor), ($H * $Factor)), (New-Object System.Drawing.Rectangle $X, $Y, $W, $H), [System.Drawing.GraphicsUnit]::Pixel)
$g.Dispose()
$source.Dispose()
$full = [System.IO.Path]::GetFullPath((Join-Path (Get-Location) $Out))
$target.Save($full, [System.Drawing.Imaging.ImageFormat]::Png)
$target.Dispose()
Write-Output "zoomed -> $full"
