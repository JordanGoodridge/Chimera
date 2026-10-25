#!/usr/bin/env sh
# Linux: pull the latest commit and run an optimized build.
# Usage (from the repo root): scripts/playtest.sh [game options]
# Combat Arms' skeleton: scripts/playtest.sh combat_arms [options]
# Lucker Party (the gun game on de_dust2): scripts/playtest.sh lucker_party [options]
# Ultimate Gun Game (every game's guns): scripts/playtest.sh ultimate_gun_game [options]
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
    ultimate_gun_game)
        shift
        exec cargo run --profile playtest --bin ultimate_gun_game -- "$@"
        ;;
    *)
        exec cargo run --profile playtest -- "$@"
        ;;
esac
