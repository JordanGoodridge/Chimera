# Windows: pull the latest commit and run an optimized build.
# Usage (from the repo root): .\scripts\playtest.ps1 [game options, e.g. --movement chimera:noclip]
# Combat Arms' skeleton instead of CS:S: .\scripts\playtest.ps1 combat_arms [options]
# Lucker Party (the gun game on de_dust2): .\scripts\playtest.ps1 lucker_party [options]
#   e.g. .\scripts\playtest.ps1 lucker_party +connect 192.168.1.20:28930
#   (playing with friends: scripts\lucker_party_host.ps1 and lucker_party_join.ps1)
$ErrorActionPreference = "Stop"
git pull --ff-only
if ($args.Count -gt 0 -and $args[0] -eq "combat_arms") {
    $rest = @($args | Select-Object -Skip 1)
    cargo run --profile playtest --bin combat_arms -- @rest
} elseif ($args.Count -gt 0 -and $args[0] -eq "lucker_party") {
    $rest = @($args | Select-Object -Skip 1)
    cargo run --profile playtest --bin lucker_party -- @rest
} else {
    cargo run --profile playtest -- @args
}
