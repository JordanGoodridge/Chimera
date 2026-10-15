"""Recoil of the automatics (specs/cs_source/weapons.md M3, 2026-10-15 rows):
full-clip holds standing, crouched and side-running; prints a, b, c, e and the caps.

python3 tools/css_probe/recoilall.py out.json
"""
import os, sys, json, statistics
sys.path.insert(0, os.path.dirname(__file__))
import weapmeas as M
from weapcmp import IN_ATTACK, IN_DUCK, run
GUNS = ["weapon_famas", "weapon_galil", "weapon_sg552", "weapon_aug", "weapon_mp5navy", "weapon_p90", "weapon_ump45", "weapon_mac10", "weapon_tmp", "weapon_m249"]
CLIP = {"weapon_m249": 100, "weapon_p90": 50, "weapon_famas": 25, "weapon_galil": 35, "weapon_ump45": 25}
def hold(w, state, yaw=270.0):
    n = CLIP.get(w, 30)
    M.give(w, n, 0)
    b = IN_DUCK if state == "crouched" else 0
    side = 400.0 if state == "moving" else 0.0
    ins = M.idle(90, 0, yaw, buttons=b) + [(b | IN_ATTACK, 0.0, side, 0, yaw)] * (n * 8) + M.idle(120, 0, yaw, buttons=b)
    return run("rec", ins, start=(M.SPAWN[0], M.SPAWN[1], M.SPAWN[2], 0, yaw))
def fit(k):
    k = [x for x in k if x[0] >= 1]
    if not k: return None
    a = k[0][1]; c = abs(k[0][2])
    pcap = min(x[3] for x in k); ycap = max(abs(x[4]) for x in k)
    bs, es = [], []
    prev_p, prev_y = 0.0, 0.0
    for x in k[1:]:
        if abs(x[3]) < abs(pcap) - 0.01 and x[0] >= 2:
            bs.append(round((x[1] - a) / x[0], 4))
        if abs(x[4]) < ycap - 0.01 and x[0] >= 2:
            es.append(round((abs(x[2]) - c) / x[0], 4))
    flips = sum(1 for i in range(1, len(k)) if k[i][2] * k[i-1][2] < 0)
    mode = lambda v: max(set(v), key=v.count) if v else None
    return dict(a=round(a, 4), b=mode(bs[:6]), c=round(c, 4), e=mode(es[:8]), pitch_cap=round(pcap, 3),
                yaw_cap=round(ycap, 3), shots=len(k), side_flips=flips)
out = {}
for w in GUNS:
    for st in ("standing", "crouched", "moving"):
        try:
            r = hold(w, st)
            k = M.kicks(r)
            f = fit(k); f["decay_err"] = round(M.decay_err(r), 6); f["kicks"] = [x[:5] for x in k[:8]]
            out[f"{w} {st}"] = f
            print(w, st, {x: f[x] for x in f if x != 'kicks'}, flush=True)
        except Exception as e:
            print(w, st, "ERROR", e, flush=True)
json.dump(out, open(sys.argv[1], "w"), indent=1)
