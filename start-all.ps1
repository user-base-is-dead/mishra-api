# Mishra Miron API — start EVERYTHING with one command.
# Launches Redis, Go backend, kiro.rs sidecar, and the frontend,
# each in its own terminal window. Run via: npm start   (from this folder)

$root = Split-Path -Parent $MyInvocation.MyCommand.Definition
$devtools = Join-Path $root ".devtools"
$redisDir = Join-Path $devtools "redis"
$backendScript = Join-Path $devtools "start-backend.ps1"
$kiroScript = Join-Path $devtools "start-kiro-rs.ps1"
$frontendDir = Join-Path $root "frontend"

function Start-Svc($title, $workdir, $command) {
  Write-Host "Starting $title ..." -ForegroundColor Cyan
  $inner = "`$host.UI.RawUI.WindowTitle='" + $title + "'; Set-Location -LiteralPath '" + $workdir + "'; " + $command
  Start-Process powershell -ArgumentList "-NoExit", "-ExecutionPolicy", "Bypass", "-Command", $inner | Out-Null
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

# 4) Frontend (Vite dev server on :3000)
Start-Svc "Mishra Frontend" $frontendDir "pnpm dev"

Write-Host ""
Write-Host "All services launched in separate windows:" -ForegroundColor Green
Write-Host "  Website / Dashboard : http://localhost:3000"
Write-Host "  Backend API         : http://localhost:8080"
Write-Host "  kiro.rs admin       : http://localhost:8990/admin"
Write-Host "  Redis               : 127.0.0.1:6379"
Write-Host ""
Write-Host "To stop everything: npm run stop" -ForegroundColor DarkGray
