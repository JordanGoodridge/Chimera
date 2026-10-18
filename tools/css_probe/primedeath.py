"""Dying with a primed grenade while moving (specs/cs_source/grenades.md
Q17, 2026-10-18).

    python3 tools/css_probe/primedeath.py [weapon] [x y z yaw]

The run bot draws the grenade, runs forward holding attack (the pin out)
and is killed (`mashup_hurt <index> -1 1000 0`, its own RCON connection;
`kill` has a cooldown after a respawn)
about run tick 172 (the pin is out from about 165); prints the run bot's last logged velocity and
each projectile's origin and velocity just after (`mashup_find`), twice a
few ticks apart. Default start: de_aztec's open ground at (-2690, -440,
-223), running +x.
"""

import os
import re
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(__file__))
import weapcmp as W  # noqa: E402
from rcon import Rcon  # noqa: E402

OUT = os.path.join(W.SERVER, "mashup", "primedeath.out")
# Seconds from the run command to the kill: about run tick 172.
KILL_AFTER = float(os.environ.get("KILL_AFTER", "2.6"))


def killer(index, after, proj, stop, seen):
    """Kills the run bot `after` seconds after the run's start (the run's
    output is written at its end, so it can't be watched)."""
    rc = Rcon()
    t_end = time.time() + after
    while time.time() < t_end:
        if stop.is_set():
            return
        time.sleep(0.002)
    rc.exec(f"mashup_hurt {index} -1 1000 0")
    for _ in range(3):
        seen.append(rc.exec(f"mashup_find {proj}"))
        time.sleep(0.05)


def main():
    weapon = sys.argv[1] if len(sys.argv) > 1 else "weapon_hegrenade"
    x, y, z, yaw = (float(v) for v in sys.argv[2:6]) if len(sys.argv) > 5 else (-2690.0, -440.0, -223.0, 0.0)
    proj = weapon.replace("weapon_", "") + "_projectile"
    W.rcon("mashup_health 100 0")
    W.give(weapon)
    # The run bot's index (`mashup_info`'s first line).
    uid = int(W.rcon("mashup_info").split()[1])
    if os.path.exists(OUT):
        os.remove(OUT)
    stop = threading.Event()
    seen = []
    th = threading.Thread(target=killer, args=(uid, KILL_AFTER, proj, stop, seen))
    th.start()
    try:
        r = W.run("primedeath", [(0, 0, 0, 0, yaw, 4)] * 100 + [(1 | 8, 400, 0, 0, yaw, 4)] * 100, start=(x, y, z, 0, yaw),
                  timeout=30)
    except RuntimeError as e:
        r = None
        print(e)
    finally:
        stop.set()
        th.join()
    rows = [ln for ln in open(OUT).read().splitlines() if re.match(r"^\d+ ", ln)]
    print("last rows:", rows[-2:])
    for s in seen:
        print(s.strip())


if __name__ == "__main__":
    main()
