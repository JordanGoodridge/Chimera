# Windows: pull the latest commit and run an optimized build that records
# a profile (docs/performance.md, "See a frame yourself").
# Usage (from the repo root):
#   .\scripts\profile.ps1 [lucker_party|combat_arms|mashup_demo] [options]
#     one second of spans, 20 s after start, into trace-<map>-<time>.json in
#     the repo root (open it at https://ui.perfetto.dev); `trace_capture 1`
#     in the console (`) records another second whenever you like.
#   .\scripts\profile.ps1 -tracy [lucker_party|...] [options]
#     streams to the Tracy profiler instead (start Tracy 0.14.x, Connect).
# The CS:S app without a name. Game options as for playtest.ps1.
$ErrorActionPreference = "Stop"
git pull --ff-only
$feature = "trace"
$rest = @($args)
if ($rest.Count -gt 0 -and $rest[0] -eq "-tracy") {
    $feature = "tracy"
    $rest = @($rest | Select-Object -Skip 1)
}
if ($feature -eq "trace") {
    if (-not $env:CHIMERA_TRACE_SECONDS) { $env:CHIMERA_TRACE_SECONDS = "1" }
    if (-not $env:CHIMERA_TRACE_AFTER) { $env:CHIMERA_TRACE_AFTER = "20" }
}
$apps = @("lucker_party", "combat_arms", "mashup_demo")
if ($rest.Count -gt 0 -and $apps -contains $rest[0]) {
    $app = $rest[0]
    $rest = @($rest | Select-Object -Skip 1)
    $package = if ($app -eq "combat_arms") { "combat_arms_app" } else { $app }
    cargo run --profile playtest -p $package --features $feature --bin $app -- @rest
} else {
    cargo run --profile playtest --features $feature -- @rest
}
