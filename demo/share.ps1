# Windows: put the running demo on the internet for a while, and print a link.
#
# Cloudflare's free "quick tunnel" — no account, no card, no DNS. Read the
# warning it prints: this is a real public URL.

$ErrorActionPreference = 'Continue'

$WebPort = 8210
$BinDir = 'demo\.bin'
$Log = Join-Path $env:TEMP 'gym-tunnel.log'

Set-Location (Split-Path -Parent $PSScriptRoot)

function Write-Bold($t) { Write-Host $t -ForegroundColor White }
function Write-Warn($t) { Write-Host $t -ForegroundColor Yellow }
function Write-Fail($t) { Write-Host $t -ForegroundColor Red }

function Stop-Here($code) {
    Write-Host ''
    Read-Host 'Press Enter to close this window'
    exit $code
}

# ------------------------------------------------------- 1. is the demo up?
$up = $false
try {
    Invoke-WebRequest -Uri "http://localhost:$WebPort" -UseBasicParsing -TimeoutSec 3 | Out-Null
    $up = $true
} catch { }

if (-not $up) {
    Write-Host 'The demo is not running yet - starting it first.'
    Write-Host ''
    powershell -NoProfile -ExecutionPolicy Bypass -File 'demo\run.ps1'
}

# --------------------------------------------------------- 2. get cloudflared
$cf = $null
$onPath = Get-Command cloudflared -ErrorAction SilentlyContinue
if ($null -ne $onPath) {
    $cf = $onPath.Source
} elseif (Test-Path "$BinDir\cloudflared.exe") {
    $cf = (Resolve-Path "$BinDir\cloudflared.exe").Path
} else {
    Write-Host 'Fetching cloudflared (one-off, ~20 MB)...'
    New-Item -ItemType Directory -Force $BinDir | Out-Null
    $arch = if ([Environment]::Is64BitOperatingSystem) { 'amd64' } else { '386' }
    $url = "https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-windows-$arch.exe"
    try {
        Invoke-WebRequest -Uri $url -OutFile "$BinDir\cloudflared.exe" -UseBasicParsing
        $cf = (Resolve-Path "$BinDir\cloudflared.exe").Path
    } catch {
        Write-Fail 'Could not download cloudflared.'
        Write-Host 'Check your internet connection and try again.'
        Stop-Here 1
    }
}

# ------------------------------------------------------------ 3. open it up
Write-Host ''
Write-Warn 'About to put this demo on the public internet.'
Write-Host ''
Write-Host '  - Anyone with the link can open it. The address is random and'
Write-Host '    unguessable, but it is not password-protected.'
Write-Host '  - It carries the DEMO accounts, whose password is public knowledge.'
Write-Host '    Do not put anything real into it while it is shared.'
Write-Host '  - The link dies when you stop this - and a new one is different.'
Write-Host ''

if (Test-Path $Log) { Remove-Item $Log -Force }
$proc = Start-Process -FilePath $cf `
    -ArgumentList @('tunnel', '--url', "http://localhost:$WebPort", '--no-autoupdate') `
    -RedirectStandardError $Log -RedirectStandardOutput "$Log.out" `
    -NoNewWindow -PassThru

Write-Host 'Opening the tunnel' -NoNewline
$public = $null
foreach ($i in 1..40) {
    if (Test-Path $Log) {
        $m = Select-String -Path $Log -Pattern 'https://[a-z0-9-]+\.trycloudflare\.com' -ErrorAction SilentlyContinue |
             Select-Object -First 1
        if ($null -ne $m) { $public = $m.Matches[0].Value; break }
    }
    if ($proc.HasExited) { break }
    Write-Host '.' -NoNewline
    Start-Sleep -Seconds 1
}
Write-Host ''

if ([string]::IsNullOrWhiteSpace($public)) {
    Write-Fail 'Could not get a public address.'
    if (Test-Path $Log) { Get-Content $Log -Tail 15 }
    if (-not $proc.HasExited) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
    Stop-Here 1
}

# Prove it end to end. A link that 502s is worse than no link, because it gets
# sent anyway.
Write-Host 'Checking it answers' -NoNewline
$ok = $false
foreach ($i in 1..20) {
    try {
        Invoke-WebRequest -Uri "$public/health" -UseBasicParsing -TimeoutSec 8 | Out-Null
        $ok = $true
        break
    } catch {
        Write-Host '.' -NoNewline
        Start-Sleep -Seconds 2
    }
}
Write-Host ''

if (-not $ok) {
    Write-Warn 'The tunnel is up but the app did not answer through it yet.'
    Write-Host "Give it a few seconds and try $public in a browser."
}

Write-Host ''
Write-Bold 'Send them this link:'
Write-Host ''
Write-Bold "    $public"
Write-Host ''
Write-Host 'It works on anything with a browser - Windows, Mac, an iPhone.'
Write-Host 'On an iPhone, Safari then Share then Add to Home Screen makes it'
Write-Host 'open like a normal app, full screen.'
Write-Host ''
Write-Host 'Sign-in is a row of buttons; no typing needed. The password, if'
Write-Host 'they want to type one, is: demopassword'
Write-Host ''

$npx = Get-Command npx -ErrorAction SilentlyContinue
if ($null -ne $npx) {
    Write-Host 'Or point their phone camera at this:'
    Write-Host ''
    npx --yes qrcode-terminal $public
    Write-Host ''
}

Write-Bold 'Leave this window open. Press Ctrl+C when you are done sharing.'
try {
    Wait-Process -Id $proc.Id
} finally {
    if (-not $proc.HasExited) { Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue }
    Write-Host ''
    Write-Host 'Tunnel closed. The link no longer works.'
}
