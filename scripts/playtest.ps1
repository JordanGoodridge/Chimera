# Windows: pull the latest commit and run an optimized build.
# Usage (from the repo root): .\scripts\playtest.ps1 [game options, e.g. --movement chimera:noclip]
$ErrorActionPreference = "Stop"
git pull --ff-only
cargo run --profile playtest -- @args
