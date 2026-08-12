# Mishra Miron API — build the frontend into the Go binary, then build the backend exe.
# Run via: npm run build     (from this folder)
#
# The frontend is embedded at compile time (-tags embed), so the order matters:
# frontend dist first, backend binary second.
param(
  [switch]$SkipFrontend,
  [switch]$SkipBackend,
  [switch]$Typecheck
)

$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $MyInvocation.MyCommand.Definition
$frontendDir = Join-Path $root 'frontend'
$backendDir = Join-Path $root 'backend'
$devtools = Join-Path $root '.devtools'
$serverExe = Join-Path $devtools 'sub2api-server.exe'

function Assert-Command($name, $hint) {
  if (-not (Get-Command $name -ErrorAction SilentlyContinue)) {
    throw "'$name' not found in PATH. $hint"
  }
}

Write-Host '==== Mishra Miron API - build ====' -ForegroundColor Yellow

if (-not $SkipFrontend) {
  Assert-Command 'pnpm' 'Install it with: npm install -g pnpm'

  if (-not (Test-Path (Join-Path $frontendDir 'node_modules'))) {
    Write-Host '[1/2] frontend: installing dependencies (first run) ...' -ForegroundColor Cyan
    Push-Location $frontendDir
    pnpm install
    $code = $LASTEXITCODE
    Pop-Location
    if ($code -ne 0) { throw "pnpm install failed (exit $code)" }
  }

  if ($Typecheck) {
    Write-Host '[1/2] frontend: vue-tsc typecheck (~20s, silent while running) ...' -ForegroundColor Cyan
    Push-Location $frontendDir
    & pnpm exec vue-tsc -b
    $code = $LASTEXITCODE
    Pop-Location
    if ($code -ne 0) { throw "frontend typecheck failed (exit $code)" }
  }

  # vite build only: the launcher stays fast. Use -Typecheck (or npm run
  # typecheck in frontend/) when you want vue-tsc to gate the build.
  Write-Host '[1/2] frontend: vite build -> backend/internal/web/dist (~25s) ...' -ForegroundColor Cyan
  Push-Location $frontendDir
  & pnpm exec vite build
  $code = $LASTEXITCODE
  Pop-Location
  if ($code -ne 0) { throw "frontend build failed (exit $code)" }
}
else {
  Write-Host '[1/2] frontend: skipped (-SkipFrontend)' -ForegroundColor DarkGray
}

if (-not $SkipBackend) {
  Assert-Command 'go' 'Install Go 1.25+ and reopen the terminal'

  # Windows cannot overwrite a running exe, so stop the old backend first.
  Get-Process sub2api-server -ErrorAction SilentlyContinue | ForEach-Object {
    Write-Host "       stopping running backend (PID $($_.Id)) ..." -ForegroundColor DarkGray
    Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
  }
  Start-Sleep -Milliseconds 500

  New-Item -ItemType Directory -Force -Path $devtools | Out-Null

  $version = 'dev'
  try {
    $described = & git -C $root describe --tags --always --dirty 2>$null
    if ($LASTEXITCODE -eq 0 -and $described) { $version = $described.Trim() }
  }
  catch { }

  Write-Host "[2/2] backend: go build -tags embed (version $version, ~30-60s first time) ..." -ForegroundColor Cyan
  Push-Location $backendDir
  & go build -tags embed -ldflags "-X main.Version=$version" -o $serverExe ./cmd/server
  $code = $LASTEXITCODE
  Pop-Location
  if ($code -ne 0) { throw "backend build failed (exit $code)" }
}
else {
  Write-Host '[2/2] backend: skipped (-SkipBackend)' -ForegroundColor DarkGray
}

Write-Host ''
Write-Host 'Build complete.' -ForegroundColor Green
Write-Host "  Backend binary : $serverExe"
Write-Host '  Embedded UI    : backend/internal/web/dist (served on http://localhost:8080)'
