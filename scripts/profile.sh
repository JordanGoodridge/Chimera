#!/usr/bin/env sh
# Linux: pull the latest commit and run an optimized build that records a
# profile (docs/performance.md, "See a frame yourself").
# Usage (from the repo root):
#   scripts/profile.sh [lucker_party|combat_arms|mashup_demo] [options]
#     one second of spans, 20 s after start, into trace-<map>-<time>.json
#     (open it at https://ui.perfetto.dev); `trace_capture 1` in the
#     console records another second whenever you like.
#   scripts/profile.sh -tracy [lucker_party|...] [options]
#     streams to the Tracy profiler instead (start Tracy 0.14.x, Connect).
# The CS:S app without a name. Game options as for playtest.sh.
set -e
git pull --ff-only
feature=trace
if [ "$1" = "-tracy" ]; then
    feature=tracy
    shift
else
    export CHIMERA_TRACE_SECONDS="${CHIMERA_TRACE_SECONDS:-1}" CHIMERA_TRACE_AFTER="${CHIMERA_TRACE_AFTER:-20}"
fi
case "$1" in
    lucker_party|mashup_demo)
        app=$1
        shift
        exec cargo run --profile playtest -p "$app" --features "$feature" --bin "$app" -- "$@"
        ;;
    combat_arms)
        shift
        exec cargo run --profile playtest -p combat_arms_app --features "$feature" --bin combat_arms -- "$@"
        ;;
    *)
        exec cargo run --profile playtest --features "$feature" -- "$@"
        ;;
esac
