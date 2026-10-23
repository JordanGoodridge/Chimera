#!/usr/bin/env sh
# Linux: pull the latest commit and run an optimized build.
# Usage (from the repo root): scripts/playtest.sh [game options]
# Combat Arms' skeleton: scripts/playtest.sh combat_arms [options]
# Lucker Party (the gun game on de_dust2): scripts/playtest.sh lucker_party [options]
set -e
git pull --ff-only
case "$1" in
    combat_arms)
        shift
        exec cargo run --profile playtest --bin combat_arms -- "$@"
        ;;
    lucker_party)
        shift
        exec cargo run --profile playtest --bin lucker_party -- "$@"
        ;;
    *)
        exec cargo run --profile playtest -- "$@"
        ;;
esac
