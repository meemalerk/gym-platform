# Windows: stops the demo. Leaves the data alone, so starting it again is
# instant and everything is where you left it.

$ErrorActionPreference = 'Continue'
Set-Location (Split-Path -Parent $PSScriptRoot)

Write-Host ''
Write-Host 'Stopping the demo...'
docker compose -f docker-compose.demo.yml down

Write-Host ''
Write-Host 'Stopped. Run start-demo.bat whenever you want it back -'
Write-Host 'your data is still there and it will start in seconds.'
Write-Host ''
Write-Host 'To erase the demo data as well:'
Write-Host '  docker compose -f docker-compose.demo.yml down -v'
Write-Host ''
Read-Host 'Press Enter to close this window'
