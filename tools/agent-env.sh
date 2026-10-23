# shellcheck shell=bash
# Source this in every agent shell before building or testing:
#
#   source tools/agent-env.sh
#   agent_run cargo test -p chimera_world --features dev
#
# What it does (measured: docs/OBSERVABILITY.md "Several agents at once"):
#  * Joins one shared GNU-make-style jobserver FIFO (default: one token per
#    CPU thread) via MAKEFLAGS. cargo and rustc of every agent on the box then
#    draw compile jobs from the same pool, so 4 simultaneous builds use ~16
#    rustc processes instead of ~48. This is the control that matters.
#  * agent_run runs a command inside the user slice agents.slice
#    (CPUWeight=50, MemoryHigh=24G), so agents yield to the desktop and
#    anything interactive instead of starving it.
# The FIFO only holds its tokens while some process has it open, so a small
# holder process (`sleep infinity`) is started once, race-free under flock.
# Override: AGENT_JOBS (token count), AGENT_JS_DIR (where the FIFO lives),
# AGENT_NO_SLICE=1 (skip the slice).

_agent_env_init() {
  # Non-login shells (agent tool sandboxes) often lack these; the user bus needs them.
  [ -z "$XDG_RUNTIME_DIR" ] && [ -d "/run/user/$(id -u)" ] && export XDG_RUNTIME_DIR="/run/user/$(id -u)"
  local dir="${AGENT_JS_DIR:-${XDG_RUNTIME_DIR:-/tmp}/agent-jobserver}"
  local jobs="${AGENT_JOBS:-$(nproc)}"
  local fifo="$dir/jobserver.fifo" pidf="$dir/holder.pid" pid
  mkdir -p "$dir" || return 1
  (
    flock 5
    pid=$(cat "$pidf" 2>/dev/null)
    if [ -z "$pid" ] || [ ! -d "/proc/$pid" ] || [ ! -p "$fifo" ]; then
      rm -f "$fifo"
      mkfifo "$fifo" || exit 1
      # The holder keeps the FIFO open read-write so the tokens survive
      # between cargo runs. Its fds exclude the lock fd (5).
      setsid bash -c '
        exec 9<>"$1"
        printf "%*s" "$2" "" | tr " " "+" >&9
        echo $$ >"$3"
        exec sleep infinity' _ "$fifo" "$jobs" "$pidf" 5>&- >/dev/null 2>&1 &
      for _ in $(seq 1 50); do [ "$(cat "$pidf" 2>/dev/null)" ] && [ -d "/proc/$(cat "$pidf")" ] && break; sleep 0.1; done
    fi
  ) 5>"$dir/lock" || return 1
  export MAKEFLAGS="--jobserver-auth=fifo:$fifo -j$jobs"
  export AGENT_ENV=1
}

# Run a command in agents.slice (falls back to running it directly).
agent_run() {
  if [ -n "$AGENT_NO_SLICE" ] || ! command -v systemd-run >/dev/null 2>&1 \
     || ! systemctl --user show -p Version --value >/dev/null 2>&1; then
    "$@"; return
  fi
  # Slice limits are runtime-only (gone at reboot); setting them is idempotent.
  if [ "$(systemctl --user show agents.slice -p CPUWeight --value 2>/dev/null)" != 50 ]; then
    systemctl --user set-property --runtime agents.slice MemoryHigh=24G CPUWeight=50 2>/dev/null
  fi
  systemd-run --user --scope --quiet --slice=agents.slice -- "$@"
}

_agent_env_init || echo "agent-env: could not set up the jobserver; builds run unthrottled" >&2
