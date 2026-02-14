param(
  [int]$Port = 8000
)
$docs = Join-Path $PSScriptRoot 'docs'
if (-not (Test-Path $docs)) { Write-Error 'docs/ folder not found'; exit 1 }
try {
  python -V > $null 2>&1
  Write-Host "Serving docs via Python on http://localhost:$Port"
  Push-Location $docs
  python -m http.server $Port
  Pop-Location
  exit 0
} catch {
  Write-Host 'Python not available — trying npx http-server'
}
try {
  npx http-server $docs -p $Port
} catch {
  Write-Error 'No python or npx available. Install one or use VS Code Live Server.'
  exit 1
}