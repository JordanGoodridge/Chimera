# Windows: join a friend's Lucker Party gun game (needs no admin rights).
#
#   .\scripts\lucker_party_join.ps1 192.168.1.20          port 28930, the host script's default
#   .\scripts\lucker_party_join.ps1 203.0.113.7:28931 -Name Guest
#
# It pulls the latest version (the host's and yours must match: the game says
# so if they don't), builds, and joins. You need Counter-Strike: Source and
# Combat Arms installed, and the Combat Arms keys in chimera.local.toml
# (README.md, 'Play the gun game with friends'); the game says exactly what
# is missing. Windows asks once whether the game may use the network: allow it.
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string]$Address,
    [string]$Name = $env:USERNAME,
    [switch]$NoPull
)
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)
if (-not $Name) { $Name = "Player" }

# ip or ip:port; the party's default port when none.
if ($Address -notmatch '^[^:\s]+(:\d{1,5})?$') {
    Write-Host "Not an address: '$Address'. Use the host's IP, e.g. 192.168.1.20 or 192.168.1.20:28930."
    exit 2
}
if ($Address -notmatch ':') {
    $Address = "${Address}:28930"
}

if (-not $NoPull) {
    git pull --ff-only
}

Write-Host ""
Write-Host "Joining $Address as $Name ..."
Write-Host "If it times out: the host's game must be running, the address and port right, and for a"
Write-Host "friend outside your network the host needs UPnP or a UDP port forward (or a shared VPN)."
Write-Host ""
cargo run --profile playtest --bin lucker_party -- +name $Name +connect $Address
