# Windows: host Lucker Party's gun game for friends (needs no admin rights).
#
#   .\scripts\lucker_party_host.ps1                 you play on your own PC and host it (a listen server)
#   .\scripts\lucker_party_host.ps1 -Dedicated      only serve (no window; you can join it too)
#   .\scripts\lucker_party_host.ps1 -Players 6 -Bots 2 -Port 28930 -Name Host
#
# It pulls the latest version, prints your LAN address and the exact join
# command for your friends, asks your router to forward the port (UPnP) and
# starts the game. Players on your network join with the LAN address; players
# elsewhere need the public address (the "UPnP:" line below says it when your
# router agrees) and either UPnP or a UDP port forward to this PC. Windows
# asks once whether the game may use the network: allow it on private
# networks (and public ones if you host from a cafe or similar).
#
# -Players is the most humans (you included); -Bots the computer players kept
# in the game (they fill the party; 0 for friends only).
param(
    [int]$Port = 28930,
    [int]$Players = 4,
    [int]$Bots = 0,
    [string]$Name = $env:USERNAME,
    [switch]$Dedicated,
    [switch]$NoUpnp,
    [switch]$NoPull
)
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)
if (-not $Name) { $Name = "Host" }

if (-not $NoPull) {
    git pull --ff-only
}

# This PC's address on the network the router gives out (the one with a
# default gateway); the first non-loopback one as a fallback.
$lan = $null
try {
    $cfg = Get-NetIPConfiguration | Where-Object { $_.IPv4DefaultGateway -ne $null -and $_.IPv4Address -ne $null } | Select-Object -First 1
    if ($cfg) { $lan = $cfg.IPv4Address[0].IPAddress }
} catch { }
if (-not $lan) {
    $lan = [System.Net.Dns]::GetHostAddresses([System.Net.Dns]::GetHostName()) |
        Where-Object { $_.AddressFamily -eq 'InterNetwork' -and -not $_.IPAddressToString.StartsWith('127.') -and -not $_.IPAddressToString.StartsWith('169.254.') } |
        ForEach-Object { $_.IPAddressToString } | Select-Object -First 1
}
if (-not $lan) { $lan = "<your PC's LAN address: run ipconfig>" }

$join = "{0}:{1}" -f $lan, $Port
Write-Host ""
Write-Host "Lucker Party gun game: hosting on UDP port $Port, up to $Players players, $Bots bots."
Write-Host ""
Write-Host "Friends on your network (same router / Wi-Fi) join with:"
Write-Host "    .\scripts\lucker_party_join.ps1 $join"
Write-Host "  or, in a built checkout:"
Write-Host "    cargo run --profile playtest --bin lucker_party -- +connect $join"
Write-Host ""
Write-Host "Friends elsewhere use your public address instead of $lan (search 'what is my IP',"
Write-Host "or read it from the 'UPnP:' line the game prints), the same port ($Port), and one of:"
if ($NoUpnp) {
    Write-Host "  - a UDP port forward on your router: UDP $Port -> $lan (UPnP is off: -NoUpnp)"
} else {
    Write-Host "  - UPnP (asked of your router now; it says below whether it worked)"
    Write-Host "  - or a UDP port forward on your router: UDP $Port -> $lan"
}
Write-Host "  - or a VPN you share (Tailscale, ZeroTier): then use each other's VPN address."
Write-Host "Everyone needs Counter-Strike: Source and Combat Arms installed (and the Combat"
Write-Host "Arms keys in their chimera.local.toml): see README.md, 'Play the gun game with friends'."
Write-Host ""

$upnp = @()
if (-not $NoUpnp) { $upnp = @('+net_upnp', '1') }

if ($Dedicated) {
    cargo run --profile playtest --bin lucker_party_server -- -port $Port +maxplayers $Players +MaxPlayer $Bots +hostname "Lucker Party gun game" @upnp
} else {
    cargo run --profile playtest --bin lucker_party -- -port $Port +maxplayers $Players +MaxPlayer $Bots +name $Name @upnp
}
