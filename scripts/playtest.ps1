# Windows: pull the latest commit and run an optimized build.
# Usage (from the repo root): .\scripts\playtest.ps1 [game options, e.g. --movement chimera:noclip]
# Combat Arms' skeleton instead of CS:S: .\scripts\playtest.ps1 combat_arms [options]
$ErrorActionPreference = "Stop"
git pull --ff-only
if ($args.Count -gt 0 -and $args[0] -eq "combat_arms") {
    $rest = @($args | Select-Object -Skip 1)
    cargo run --profile playtest --bin combat_arms -- @rest
} else {
    cargo run --profile playtest -- @args
}
