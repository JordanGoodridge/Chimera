"""Round restart keep-list (specs/source/entity_io.md question 1): on every
stock map, list the entities (the mashup_ents plugin) before and after
`mp_restartgame 1`; an unchanged reference is an entity kept.

python3 tools/css_probe/roundkeep.py out.json
"""
import os, sys, time, collections, json
sys.path.insert(0, os.path.dirname(__file__))
from rcon import Rcon
d = os.path.expanduser("~/games/css-ds/cstrike/mashup/")
def rc(c):
    for k in range(30):
        try: return Rcon().exec(c)
        except Exception: time.sleep(2)
def load(n):
    out = {}
    for l in open(d + n):
        p = l.rstrip("\n").split(" ", 3)
        out[int(p[0])] = p[1]
    return out
kept, recreated = collections.defaultdict(set), collections.defaultdict(set)
maps = "cs_assault cs_compound cs_havana cs_italy cs_militia cs_office de_aztec de_cbble de_chateau de_dust de_inferno de_nuke de_piranesi de_port de_prodigy de_tides de_train de_dust2".split()
for m in maps:
    rc(f"changelevel {m}"); time.sleep(12)
    for f in ("e0", "e1"): 
        try: os.remove(d + f + ".txt")
        except FileNotFoundError: pass
    rc("mashup_ents mashup/e0.txt"); rc("mp_restartgame 1"); time.sleep(3.5); rc("mashup_ents mashup/e1.txt"); time.sleep(0.5)
    a, b = load("e0.txt"), load("e1.txt")
    for r, c in a.items():
        (kept if r in b else recreated)[c].add(m)
    print(m, len(a), len(b), flush=True)
both = sorted(set(kept) & set(recreated))
print("KEPT:", sorted(set(kept) - set(recreated)))
print("RECREATED:", sorted(set(recreated) - set(kept)))
print("BOTH (some kept, some not):", {c: (sorted(kept[c]), sorted(recreated[c])) for c in both})
json.dump({"kept": {k: sorted(v) for k, v in kept.items()}, "recreated": {k: sorted(v) for k, v in recreated.items()}}, open(sys.argv[1], "w"), indent=1)
