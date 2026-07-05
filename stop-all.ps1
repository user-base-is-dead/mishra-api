# Stop all Mishra Miron API services started by start-all.ps1
Write-Host "Stopping all Mishra services..." -ForegroundColor Yellow
Get-Process redis-server -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-Process sub2api-server -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Get-Process kiro-rs -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
# Frontend: kill node processes serving vite on port 3000
$c = Get-NetTCPConnection -LocalPort 3000 -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1
if ($c) { Stop-Process -Id $c.OwningProcess -Force -ErrorAction SilentlyContinue }
Write-Host "Done. (Note: the individual service windows may need to be closed manually.)" -ForegroundColor Green
