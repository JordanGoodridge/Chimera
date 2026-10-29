#!/usr/bin/env bash
# Which code takes the frame's time (Linux): run a game build with line
# tables, sample its CPU with `perf record` for a while, and write a top-N
# table, a flame graph and bars with `profsum` (docs/performance.md,
# "Measuring"). Rerun with --baseline <earlier out>/top.json after a fix
# for before, after and delta per function.
#
# Usage (from the repo root; build first, with frame pointers: perf's
# DWARF unwinding lost most stacks of the game's main thread here):
#   CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS="-Cforce-frame-pointers=yes" \
#     cargo build --profile profiling -p lucker_party --bin lucker_party
#   cargo build --profile playtest --features tools -p chimera_tools --bin profsum):
#   tools/perf-profile.sh [options] -- <game args>
#     --bin <name>       the binary in target/profiling (default lucker_party)
#     --after <s>        seconds from start to the first sample (default 30)
#     --seconds <s>      how long to sample (default 20)
#     --freq <hz>        samples a second (default 499)
#     --call-graph <m>   perf's unwinding (default fp; dwarf,65528 for a
#                        build without frame pointers: slower, loses samples)
#     --out <dir>        where everything goes (default target/perf-profile)
#     --baseline <json>  an earlier run's top.json: before/after columns
#     --title <text>     the pictures' title
#   e.g. tools/perf-profile.sh --out target/pp/dust2 -- +minigame originals:gun_game_dust2 +MaxPlayer 4
#
# Needs `perf` (PERF=<path> for one outside PATH; perf_event_paranoid 2
# is enough: user space of our own process; perf starts the game), and a display for a windowed
# game (or pass the game's --bench/--views options). The game runs with
# vsync off, no frame cap and `chimera_perf_log 1`; its log (game.log)
# gives the frame count the ms-per-frame columns divide by. Also writes
# perf.data and perf.txt (perf script). Windows has no perf: there, take
# a span trace (scripts\profile.ps1) and run `tracesum <trace> --folded
# f.txt` then `profsum f.txt --unit us --frames N --out <dir>`.
set -euo pipefail

bin=lucker_party after=30 seconds=20 freq=499 callgraph=fp out=target/perf-profile baseline="" title=""
while [ $# -gt 0 ]; do
    case "$1" in
        --bin) bin=$2; shift 2 ;;
        --after) after=$2; shift 2 ;;
        --seconds) seconds=$2; shift 2 ;;
        --freq) freq=$2; shift 2 ;;
        --call-graph) callgraph=$2; shift 2 ;;
        --out) out=$2; shift 2 ;;
        --baseline) baseline=$2; shift 2 ;;
        --title) title=$2; shift 2 ;;
        --) shift; break ;;
        *) echo "unknown option $1 (game args go after --)" >&2; exit 2 ;;
    esac
done
perf=${PERF:-perf}
game=target/profiling/$bin
profsum=${PROFSUM:-target/playtest/profsum}
for f in "$game" "$profsum"; do
    [ -x "$f" ] || { echo "$f: not built (see the usage at the top of $0)" >&2; exit 1; }
done
command -v "$perf" >/dev/null || { echo "perf not found (PERF=<path>)" >&2; exit 1; }
mkdir -p "$out"
log=$out/game.log

# perf starts the game (so it sees every mapping from the start) and
# samples from --after on (-D); attaching to a running game lost the main
# thread's mappings.
"$perf" record -F "$freq" -e cpu-clock:u --call-graph "$callgraph" -D "$((after * 1000))" \
    -o "$out/perf.data" -- "$game" "$@" +mat_vsync 0 +fps_max 0 +chimera_perf_log 1 \
    >"$log" 2>&1 &
perf_pid=$!
trap 'kill -INT "$perf_pid" 2>/dev/null || true' EXIT
sleep "$after"
before=$(grep -c 'chimera_perf:' "$log" || true)
sleep "$seconds"
lines=$(grep 'chimera_perf:' "$log" | tail -n +"$((before + 1))" || true)
game_pid=$(pgrep -P "$perf_pid" | head -1 || true)
[ -n "$game_pid" ] || { echo "the game quit early; see $log" >&2; exit 1; }
kill "$game_pid" 2>/dev/null || true
wait "$perf_pid" 2>/dev/null || true
trap - EXIT
# Frames sampled: the per-second readouts' fps over the window.
frames=$(printf '%s\n' "$lines" | sed -n 's/.*chimera_perf: \([0-9.]*\) fps.*/\1/p' | awk '{s+=$1} END {printf "%d", s}')
[ "${frames:-0}" -gt 0 ] || { echo "no chimera_perf readouts in $log while sampling" >&2; exit 1; }
printf '%s\n' "$lines" >"$out/perf-readouts.txt"
echo "sampled ${seconds}s, about $frames frames; folding (perf script takes a while)"
"$perf" script --no-inline -i "$out/perf.data" >"$out/perf.txt" 2>/dev/null
args=(--frames "$frames" --freq "$freq" --out "$out" --title "${title:-$bin ${*}}")
[ -n "$baseline" ] && args+=(--baseline "$baseline")
"$profsum" "$out/perf.txt" "${args[@]}" | tee "$out/summary.txt"
