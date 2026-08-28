# Windows: started by start-demo.bat. Same job as demo/run.sh — check Docker,
# build, wait, open a browser — written so every failure says what to do next.

$ErrorActionPreference = 'Continue'

$WebPort = 8210
$ApiPort = 8211
$ComposeFile = 'docker-compose.demo.yml'

# Run from the project folder, whatever folder the shortcut was clicked from.
Set-Location (Split-Path -Parent $PSScriptRoot)

function Write-Bold($text) { Write-Host $text -ForegroundColor White }
function Write-Warn($text) { Write-Host $text -ForegroundColor Yellow }
function Write-Fail($text) { Write-Host $text -ForegroundColor Red }

function Stop-Here($code) {
    Write-Host ''
    Read-Host 'Press Enter to close this window'
    exit $code
}

Write-Host ''
Write-Bold 'Gym Platform - demo'
Write-Host 'This starts the app on your machine and opens it in your browser.'
Write-Host ''

# ------------------------------------------------------------ 1. Is Docker in?
$docker = Get-Command docker -ErrorAction SilentlyContinue
if ($null -eq $docker) {
    Write-Fail 'Docker is not installed.'
    Write-Host ''
    Write-Host 'The demo needs Docker Desktop. It is free, and it is the only thing'
    Write-Host 'you need to install - everything else is handled for you.'
    Write-Host ''
    Write-Host '  1. Download it from:  https://www.docker.com/products/docker-desktop/'
    Write-Host '  2. Install it and start it (you may be asked to restart your computer).'
    Write-Host '  3. Wait until the Docker whale icon stops animating.'
    Write-Host '  4. Run this file again.'
    Stop-Here 1
}

# ------------------------------------------------------- 2. Is Docker running?
docker info *> $null
if ($LASTEXITCODE -ne 0) {
    Write-Fail 'Docker is installed but not running.'
    Write-Host ''
    Write-Host 'Start Docker Desktop from the Start menu, wait for its whale icon to'
    Write-Host 'stop animating (that can take a minute), then run this file again.'
    Stop-Here 1
}

docker compose version *> $null
if ($LASTEXITCODE -ne 0) {
    Write-Fail 'This version of Docker is too old - it has no "compose" command.'
    Write-Host 'Update Docker Desktop and try again.'
    Stop-Here 1
}

# --------------------------------------------------------- 3. Are ports free?
# Skipped when our own demo is already up - it may hold its own ports.
$running = docker compose -f $ComposeFile ps -q 2>$null
if ([string]::IsNullOrWhiteSpace($running)) {
    foreach ($p in @($WebPort, $ApiPort)) {
        $busy = Get-NetTCPConnection -LocalPort $p -State Listen -ErrorAction SilentlyContinue
        if ($null -ne $busy) {
            Write-Warn "Something else on this computer is already using port $p."
            Write-Host 'The demo may fail to start. If it does, close the other program,'
            Write-Host "or change $p in $ComposeFile."
            Write-Host ''
        }
    }
}

# ------------------------------------------------------------------ 4. Build
# Built as its own step, separately from starting, so a compiler error and a
# crashed container do not produce the same unhelpful message.
Write-Bold 'Building...'
Write-Host 'The first run downloads and builds everything, which usually takes'
Write-Host '5-15 minutes. After that it starts in seconds.'
Write-Host ''

docker compose -f $ComposeFile build
if ($LASTEXITCODE -ne 0) {
    Write-Host ''
    Write-Fail 'The build did not finish.'
    Write-Host ''
    Write-Host 'The real reason is in the messages above this one - scroll up.'
    Write-Host ''
    Write-Host 'The two usual causes:'
    Write-Host '  - Docker has too little memory. Docker Desktop, Settings,'
    Write-Host '    Resources, set memory to at least 4 GB, Apply and Restart.'
    Write-Host '  - No internet connection. The build downloads as it goes.'
    Stop-Here 1
}

# ------------------------------------------------------------------ 5. Start
Write-Host ''
Write-Bold 'Starting...'
Write-Host ''

docker compose -f $ComposeFile up -d
if ($LASTEXITCODE -ne 0) {
    Write-Host ''
    Write-Fail 'The demo could not start.'
    Write-Host ''
    Write-Host 'To see what went wrong:'
    Write-Host "  docker compose -f $ComposeFile logs"
    Stop-Here 1
}

# ------------------------------------------------------- 6. Wait for the app
Write-Host ''
Write-Host 'Waiting for the app to come up' -NoNewline
$url = "http://localhost:$WebPort"
$ready = $false
foreach ($i in 1..90) {
    try {
        Invoke-WebRequest -Uri $url -UseBasicParsing -TimeoutSec 3 | Out-Null
        $ready = $true
        break
    } catch {
        Write-Host '.' -NoNewline
        Start-Sleep -Seconds 2
    }
}
Write-Host ''

if (-not $ready) {
    Write-Host ''
    Write-Fail 'The app did not answer in time.'
    Write-Host ''
    Write-Host "It may still be finishing. Try opening $url in your browser."
    Write-Host 'If that does not work, run:'
    Write-Host "  docker compose -f $ComposeFile logs"
    Stop-Here 1
}

# ---------------------------------------------------------- 7. Open a browser
Start-Process $url

Write-Host ''
Write-Bold "Ready - the app is at $url"
Write-Host ''
Write-Host 'If your browser did not open by itself, copy that address into it.'
Write-Host ''
Write-Bold 'Sign in with any of these (the sign-in screen lists them as buttons):'
Write-Host ''
Write-Host '  owner@demo.test       runs the gym - members, billing, everything'
Write-Host '  headcoach@demo.test   writes the training programmes'
Write-Host '  trainer@demo.test     coaches a handful of members'
Write-Host '  member@demo.test      trains here - the richest account to look at'
Write-Host '  multi@demo.test       one person, three gyms, a different role in each'
Write-Host ''
Write-Host '  The password for all of them is:  demopassword'
Write-Host ''
Write-Host 'To stop the demo later, run stop-demo.bat (next to this file).'
Stop-Here 0
