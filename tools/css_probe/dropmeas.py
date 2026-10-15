"""Dropping a weapon (specs/cs_source/weapons.md M20): the probe bot drops an
AK-47 standing, looking down/up, and running; prints launch and slide.

python3 tools/css_probe/dropmeas.py
"""
import os, sys, time, threading, re, math
sys.path.insert(0, os.path.dirname(__file__))
import weapcmp as W
from rcon import Rcon
uid = int(re.search(r'#\s+(\d+)\s+"\w+"\s+BOT', W.rcon("status")).group(1))
START = (-2000.0, -872.0, 131.1)
for label, fwd, pitch in [("stand pitch 0", 0.0, 0.0), ("stand look down 45", 0.0, 45.0), ("stand look up 45", 0.0, -45.0), ("run 250", 400.0, 0.0)]:
    W.rcon("mashup_health 100 0"); W.give("weapon_ak47", 30, 90); time.sleep(1.5)
    W.rcon("mashup_track 0")
    ins = [(8 if fwd else 0, fwd, 0.0, pitch, 0.0, 1)] + [(8 if fwd else 0, fwd, 0.0, pitch, 0.0)] * 400
    def dropper():
        time.sleep(1.6); Rcon().exec(f"mashup_botcmd {uid} drop")
    th = threading.Thread(target=dropper); th.start()
    r = W.run("drop", ins, start=(*START, pitch, 0.0), timeout=60)
    th.join(); W.rcon("mashup_track -1")
    T = [list(map(float, p[1:8])) + [p[8]] for p in r.extra if p[0] == "T"]
    # the drop: first row whose owner is -1
    idx = next((i for i, t in enumerate(T) if t[7] == "-1"), None)
    if idx is None: print(label, "no drop seen"); continue
    t0 = T[idx]; bot = r.ticks[int(t0[0])]
    eye = (bot[1], bot[2], bot[3] + 64)
    v = t0[4:7]
    print(f"{label}: drop tick {int(t0[0])}, start offset from eye ({t0[1]-eye[0]:.1f},{t0[2]-eye[1]:.1f},{t0[3]-eye[2]:.1f}), vel ({v[0]:.1f},{v[1]:.1f},{v[2]:.1f}) |v|={math.sqrt(sum(x*x for x in v)):.1f}, bot vel ({bot[4]:.1f},{bot[5]:.1f})")
    seq = T[idx:idx+8]
    vs = [((b[1]-a[1])/0.015, (b[2]-a[2])/0.015, (b[3]-a[3])/0.015) for a, b in zip(seq, seq[1:])]
    print("   first velocities (u/s):", [tuple(round(c, 1) for c in v) for v in vs[:5]])
    still = next((t for a, t in zip(T[idx:], T[idx+1:]) if abs(t[1]-a[1]) + abs(t[2]-a[2]) + abs(t[3]-a[3]) < 0.01 and t[0] > T[idx][0] + 5), None)
    last = T[-1]
    print(f"   still from tick {int(still[0]) if still else None} ({(still[0]-T[idx][0])*0.015 if still else 0:.2f} s), slid {math.hypot(last[1]-t0[1], last[2]-t0[2]):.1f} units from the drop point, final z {last[3]:.1f}")
