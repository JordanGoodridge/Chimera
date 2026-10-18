"""The planted bomb's timing (specs/cs_source/objectives.md Q14, 2026-10-18).

    python3 tools/css_probe/bombtime.py plant [timer]
    python3 tools/css_probe/bombtime.py defuse <timer> <use tick> [kit]
    python3 tools/css_probe/bombtime.py think

`plant`: the second bot (a terrorist) plants at de_dust2's B with
`mp_c4timer` `timer` and the run bot idles; prints the plant's and the
explosion's server ticks (mashup_props' `snd` rows, `k=`) and their gap.
`defuse`: the same, and the run bot (a CT crouching 34 units east of the
bomb, looking down at it) holds +use from run tick `use tick` on; prints the
plant, the defuse's start and finish (`c4_disarm.wav` on channels 2 and 3)
and the explosion, in server ticks after the plant. `think` reads the
planted bomb's `m_nNextThinkTick` a few hundred times over RCON and
prints the distinct values (its think period).

The sounds a player makes carry its command time (`t=`, its tick base,
4 ticks ahead of the server's tick for these bots); the bomb's own think
runs on the server tick (`k=`). Needs mashup_props loaded with
`mashup_sndlog 1` and a second bot (`bot_add_t`).
"""

import os
import re
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(__file__))
import weapcmp as W  # noqa: E402
from propmeas import first_bot, log_since, log_size  # noqa: E402
from rcon import Rcon  # noqa: E402

SPOS = (-1478.0, 2768.0, 4.0)
IN_USE = 32
IN_DUCK = 4


def rows(pos):
    out = {}
    for ln in log_since(pos):
        m = re.match(r"t=([\d.]+) k=(\d+) snd \d+ \S+ ch=(\d+) .*weapons/c4/(c4_\w+)\.wav", ln)
        if not m:
            continue
        t, k, ch, wav = float(m[1]), int(m[2]), int(m[3]), m[4]
        key = {("c4_plant", 2): "plant", ("c4_disarm", 2): "defuse_start", ("c4_disarm", 3): "defused"}.get((wav, ch))
        if wav.startswith("c4_explode"):
            key = "explode"
        if key:
            out[key] = (k, t)
    return out


def move_planter(sh, pos, stop):
    """Once the bomb is planted, the planter steps off it (it stands on
    the bomb and its body would stop the defuser's use search)."""
    rc = Rcon()
    while not stop.is_set():
        if any("c4_plant.wav" in ln for ln in log_since(pos)):
            rc.exec(f"mashup_tp {sh} {SPOS[0]} {SPOS[1] + 150.0} {SPOS[2] + 10.0} 0 0 0")
            return
        time.sleep(0.05)


def plant_run(timer, inputs, kit=False):
    # Exploded bombs stay until a restart, and the use search could find one.
    W.rcon("mp_restartgame 1")
    time.sleep(2.5)
    W.rcon(f"mp_c4timer {timer}")
    W.rcon("mashup_health 1000 0")
    W.give("weapon_knife")
    if kit:
        # Bought at the CT spawn after the restart (`mashup_give
        # item_defuser` gave nothing that counted).
        print(W.rcon(f"mashup_botcmd {first_bot()} buy defuser"))
    out = W.rcon(f"mashup_shooter {SPOS[0]} {SPOS[1]} {SPOS[2]} weapon_c4 20 0 260")
    m = re.search(r"shooter (\d+)", out)
    sh = int(m.group(1)) if m else 2
    W.rcon(f"mashup_hp {sh} 1000 0 0")
    time.sleep(1.5)
    pos = log_size()
    vpos = (SPOS[0] + 34.0, SPOS[1], SPOS[2] + 3.3)
    pitch, yaw = W.angles_to((vpos[0], vpos[1], vpos[2] + 46.0), (SPOS[0], SPOS[1], SPOS[2]))
    stop = threading.Event()
    th = threading.Thread(target=move_planter, args=(sh, pos, stop))
    th.start()
    try:
        W.run("bombtime", [(b | IN_DUCK, 0.0, 0.0, pitch, yaw) for b in inputs], start=(*vpos, pitch, yaw), timeout=120)
    finally:
        stop.set()
        th.join()
    time.sleep(0.5)
    W.rcon("mashup_shooter -1")
    return rows(pos)


def report(r):
    if "plant" not in r:
        return f"no plant: {r}"
    p = r["plant"][0]
    parts = [f"plant k={p} t={r['plant'][1]:.3f}"]
    for key in ("defuse_start", "defused", "explode"):
        if key in r:
            k, t = r[key]
            parts.append(f"{key} +{k - p} (t {t - r['plant'][1]:.3f})")
    if "defuse_start" in r and "defused" in r:
        parts.append(f"defuse took {r['defused'][0] - r['defuse_start'][0]} ticks")
    return ", ".join(parts)


def main():
    cmd = sys.argv[1]
    if cmd == "plant":
        timer = int(sys.argv[2]) if len(sys.argv) > 2 else 10
        n = 260 + int(timer / 0.015) + 60
        print(report(plant_run(timer, [0] * n)))
    elif cmd == "defuse":
        timer, use = int(sys.argv[2]), int(sys.argv[3])
        kit = len(sys.argv) > 4 and sys.argv[4] == "kit"
        n = 260 + int(timer / 0.015) + 60
        print(report(plant_run(timer, [0] * use + [IN_USE] * (n - use), kit)))
    elif cmd == "think":
        e = re.search(r"^(\d+) planted_c4", W.rcon("mashup_find planted_c4"), re.M)
        seen = []
        for _ in range(300):
            v = W.rcon(f"mashup_getprop {e[1]} m_nNextThinkTick").split()[-1]
            if not seen or seen[-1] != v:
                seen.append(v)
        print(seen)


if __name__ == "__main__":
    main()
