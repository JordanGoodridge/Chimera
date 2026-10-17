"""HE self damage (specs/cs_source/grenades.md "Self damage", 2026-10-17):
the second bot throws an HE at the run bot's feet; prints the detonation
point and, for each player hurt, its horizontal distance from it, the
damage before armour (mashup_props' `dmg`/`post` rows) and the `hurt` row.

    python3 tools/css_probe/hemeas.py '{"vpos": [250, 2384, -126.2], "spos": [150, 2384, -126.2], "aimz": 0, "ff": 0, "reps": 2}'

Needs mashup_props loaded, two bots (`bot_add_t` or `bot_add_ct` for a
teammate) and `mashup_dmghook player` (again after a changelevel or a new
bot). Keys: vpos (run bot), spos (thrower), aimz (aim height over the run
bot's feet), ff (mp_friendlyfire), t_armor, v_armor, reps. Leaves
mp_friendlyfire as the last run set it: restore 0 afterwards.
"""

import json
import math
import os
import re
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
import weapcmp as W  # noqa: E402
from propmeas import log_since, log_size  # noqa: E402


def throw(vpos, spos, aimz, ff, t_armor=0, v_armor=0, hold=1, tick=20, n=170):
    W.rcon(f"mp_friendlyfire {ff}")
    W.rcon(f"mashup_health 1000 {v_armor}")
    W.give("weapon_knife")
    out = W.rcon(f"mashup_shooter {spos[0]} {spos[1]} {spos[2]} weapon_hegrenade {tick} {aimz} {hold}")
    m = re.search(r"shooter (\d+)", out)
    sh = int(m.group(1)) if m else 2
    W.rcon(f"mashup_hp {sh} 1000 {t_armor} 0")
    time.sleep(1.5)
    pos = log_size()
    r = W.run("he", [(0, 0.0, 0.0, 0.0, 0.0)] * n, start=(*vpos, 0.0, 180.0))
    time.sleep(0.3)
    rows = [x for x in log_since(pos) if " dmg " in x or " post " in x or " hurt " in x]
    return sh, [e for e in r.extra if e[0] == "G"], rows


def summary(a, sh, g, rows):
    if not g:
        return "no detonation"
    b = [float(x) for x in g[0][3:6]]
    out = [f"ff={a['ff']} B=({b[0]:.1f},{b[1]:.1f},{b[2]:.1f})"]
    dmg = {}
    for row in rows:
        p = row.split()
        if p[2] in ("dmg", "post"):
            dmg.setdefault(int(p[3]), []).append(p[6])
    for v in sorted(dmg):
        o = a["spos"] if v == sh else a["vpos"]
        horiz = math.hypot(o[0] - b[0], o[1] - b[1])
        hurt = [x.split(" ", 3)[3] for x in rows if x.split()[2] == "hurt" and int(x.split()[3]) == v]
        who = "thrower" if v == sh else "run bot"
        out.append(f"  {who} horiz={horiz:.1f} dz={o[2] - b[2]:.1f} {'/'.join(dmg[v])} {hurt[0] if hurt else 'no hurt'}")
    return "\n".join(out)


if __name__ == "__main__":
    a = json.loads(sys.argv[1])
    reps = a.pop("reps", 1)
    for _ in range(reps):
        print(summary(a, *throw(**a)))
