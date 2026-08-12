# Mishra Miron API — build EVERYTHING, then start it with one command.
# Builds the frontend into the Go binary, then launches Redis, the Go backend,
# the kiro.rs sidecar, and the Vite dev server — each in its own terminal window.
# Run via: npm start                 (build + start)
#          npm start -- -SkipBuild   (start only, no rebuild)
#          npm start -- -NoFrontendDev  (skip the Vite dev server on :3000)
param(
  [switch]$SkipBuild,
  [switch]$NoFrontendDev
)

$root = Split-Path -Parent $MyInvocation.MyCommand.Definition
$devtools = Join-Path $root ".devtools"
$redisDir = Join-Path $devtools "redis"
$backendScript = Join-Path $devtools "start-backend.ps1"
$kiroScript = Join-Path $devtools "start-kiro-rs.ps1"
$frontendDir = Join-Path $root "frontend"
$buildScript = Join-Path $root "build-all.ps1"

function Start-Svc($title, $workdir, $command) {
  Write-Host "Starting $title ..." -ForegroundColor Cyan
  $inner = "`$host.UI.RawUI.WindowTitle='" + $title + "'; Set-Location -LiteralPath '" + $workdir + "'; " + $command
  Start-Process powershell -ArgumentList "-NoExit", "-ExecutionPolicy", "Bypass", "-Command", $inner | Out-Null
}

# 0) Build (frontend dist -> embedded Go binary). Runs inline so a failed build
#    stops the launch instead of silently starting a stale binary.
if ($SkipBuild) {
  Write-Host "==== Build skipped (-SkipBuild): using the existing binary ====" -ForegroundColor DarkYellow
}
else {
  & powershell -ExecutionPolicy Bypass -File $buildScript
  if ($LASTEXITCODE -ne 0) {
    Write-Host "Build failed - not starting services." -ForegroundColor Red
    exit 1
  }
}

Write-Host "==== Mishra Miron API - starting all services ====" -ForegroundColor Yellow

# 1) Redis
Start-Svc "Mishra Redis" $redisDir ".\redis-server.exe --port 6379"
Start-Sleep -Seconds 2

# 2) Go backend
Start-Svc "Mishra Backend" $root ("powershell -ExecutionPolicy Bypass -File '" + $backendScript + "'")
Start-Sleep -Seconds 2

# 3) kiro.rs sidecar (Kiro -> Anthropic API on :8990)
Start-Svc "Mishra kiro.rs" $root ("powershell -ExecutionPolicy Bypass -File '" + $kiroScript + "'")
Start-Sleep -Seconds 1

# 4) Frontend (Vite dev server on :3000) — optional, the built UI is on :8080
if ($NoFrontendDev) {
  Write-Host "Skipping Vite dev server (-NoFrontendDev); use http://localhost:8080" -ForegroundColor DarkGray
}
else {
  Start-Svc "Mishra Frontend" $frontendDir "pnpm dev"
}

Write-Host ""
Write-Host "All services launched in separate windows:" -ForegroundColor Green
Write-Host "  Website / Dashboard : http://localhost:8080  (freshly built UI)"
if (-not $NoFrontendDev) {
  Write-Host "  Frontend dev (HMR)  : http://localhost:3000"
}
Write-Host "  Backend API         : http://localhost:8080"
Write-Host "  kiro.rs admin       : http://localhost:8990/admin"
Write-Host "  Redis               : 127.0.0.1:6379"
Write-Host ""
Write-Host "To stop everything: npm run stop" -ForegroundColor DarkGray
