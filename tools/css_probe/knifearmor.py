"""The knife against an armoured player (2026-10-17; specs/cs_source/
weapons.md, specs/source/prop_damage.md: knife hits are bullet damage).
Needs mashup_props and two bots on opposite teams (`bot_add_t`), `bot_stop 1`.

    python3 tools/css_probe/knifearmor.py <1 slash | 2 stab> <armor> [helmet] [aim pitch]

The first bot (client 1, the victim) stands on de_dust2's CT spawn with
100 health and that armour; the second (userid from `status`) knifes it
from 40 units in front once. Prints the `hurt` row (health and armour
taken), the damage hook rows and every sound emitted around the hit.
"""

import os
import re
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
from propmeas import SPOT, log_since, log_size, rcon  # noqa: E402


def attacker_userid():
    bots = re.findall(r"#\s+(\d+)\s+\"[^\"]+\"\s+BOT", rcon("status"))
    return int(bots[1])


def knife(buttons, armor, helmet=0, pitch=10.0, dist=40.0, victim=1):
    x, y, z = SPOT
    rcon(f"mashup_hp {victim} 100 {armor} {helmet}")
    rcon(f"mashup_tp {victim} {x} {y} {z + 0.05} 0 0 0")
    rcon("mashup_sndlog 1")
    pos = log_size()
    hold = os.environ.get('HOLD', '1')
    rcon(f"mashup_fire {attacker_userid()} {x} {y + dist} {z + 0.05} {pitch} -90 {os.environ.get("WPN", "weapon_knife")} {buttons} 100 {hold}")
    time.sleep(4.5)
    rcon("mashup_sndlog 0")
    rows = log_since(pos)
    return [r for r in rows if " hurt " in r or " dmg " in r or " post " in r
            or (" snd " in r and ("player" in r or "knife" in r or "kevlar" in r.lower() or "flesh" in r))]


if __name__ == "__main__":
    b = int(sys.argv[1])
    armor = int(sys.argv[2])
    helmet = int(sys.argv[3]) if len(sys.argv) > 3 else 0
    pitch = float(sys.argv[4]) if len(sys.argv) > 4 else 10.0
    print("\n".join(knife(b, armor, helmet, pitch)))
