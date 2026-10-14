"""Being hit (specs/cs_source/weapons.md M18): a second bot fires one shot at
the run's bot running at 250 with the knife; prints the hitgroup, damage,
speed modifier and view punch after the hit.

python3 tools/css_probe/hitmeas.py weapon_ak47 30 [armor]
(aim height above the victim's origin: 12 legs, 30 stomach, 45 chest, 66 head)
"""

import os
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
import weapcmp as W  # noqa: E402

START = (352.0, 2384.0, -126.24)  # de_dust2, CT spawn floor; clear along +x
SHOOTER = (150.0, 2384.0, -126.24)


def shot(weapon, aimz, armor=0, tick=30):
    W.rcon(f"mashup_health 500 {armor}")
    W.give("weapon_knife")
    W.rcon(f"mashup_shooter {SHOOTER[0]} {SHOOTER[1]} {SHOOTER[2]} {weapon} {tick} {aimz}")
    time.sleep(1.5)
    r = W.run("hit", [(8, 400.0, 0.0, 0.0, 0.0)] * (tick + 20), start=(*START, 0.0, 0.0))
    fired = [e[2][1] for e in r.events if e[1] == "weapon_fire"]
    if not fired or "weapon_" + fired[0] != weapon:
        return {"error": f"fired {fired}"}
    hurt = [e for e in r.events if e[1] == "player_hurt"]
    if not hurt:
        return {"error": "no hit"}
    e = hurt[-1][2]
    t = hurt[0][0]
    g = lambda k: e[e.index(k) + 1]  # noqa: E731
    return {
        "hitgroup": int(g("hitgroup")),
        "damage": int(g("dmg_health")),
        "armor_damage": int(g("dmg_armor")),
        "hits": len(hurt),
        "velmod": r.w[t]["velmod"],
        "punch": r.w[t]["punch"],
    }


if __name__ == "__main__":
    if W.rcon("status").count("BOT") < 2:
        W.rcon("bot_join_team t")
        W.rcon("bot_add_t")
        time.sleep(3)
    print(shot(sys.argv[1], float(sys.argv[2]), int(sys.argv[3]) if len(sys.argv) > 3 else 0))
