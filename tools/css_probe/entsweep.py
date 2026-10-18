"""Sweep the server install's maps' entity lumps (2026-10-18; open
questions answered by what maps place: sounds.md Q10, ropes, viewcontrol
Q5). Reads the user's own install at run time; nothing is extracted.

    python3 tools/css_probe/entsweep.py class <classname> [key ...]   # each one's keys
    python3 tools/css_probe/entsweep.py count                         # classes per map
    python3 tools/css_probe/entsweep.py outputs                       # output separators

Maps: `~/games/css-ds/cstrike/maps/*.bsp` (and any given with MAPS=dir).
"""

import glob
import os
import re
import struct
import sys

DIRS = [os.path.expanduser("~/games/css-ds/cstrike/maps")] + [d for d in os.environ.get("MAPS", "").split(":") if d]


def maps():
    for d in DIRS:
        yield from sorted(glob.glob(os.path.join(d, "*.bsp")))


def entities(path):
    with open(path, "rb") as fh:
        head = fh.read(8 + 16)
        off, length = struct.unpack_from("<2i", head, 8)
        fh.seek(off)
        text = fh.read(length).decode("latin-1")
    out = []
    for block in re.findall(r"\{(.*?)\}", text, re.S):
        kv = re.findall(r'"([^"]*)"\s+"([^"]*)"', block)
        out.append(kv)
    return out


def main():
    what = sys.argv[1]
    if what == "class":
        cls = sys.argv[2]
        keys = sys.argv[3:]
        for m in maps():
            for kv in entities(m):
                d = dict((k.lower(), v) for k, v in kv)
                if d.get("classname") == cls:
                    shown = {k: d.get(k) for k in keys} if keys else d
                    print(os.path.basename(m), shown)
    elif what == "count":
        for m in maps():
            n = {}
            for kv in entities(m):
                c = dict(kv).get("classname", "?")
                n[c] = n.get(c, 0) + 1
            print(os.path.basename(m), sorted(n.items()))
    elif what == "outputs":
        for m in maps():
            comma = esc = 0
            for kv in entities(m):
                for k, v in kv:
                    if k.lower().startswith("on") or k.lower().startswith("out"):
                        if "\x1b" in v:
                            esc += 1
                        elif v.count(",") >= 4:
                            comma += 1
            print(os.path.basename(m), "comma", comma, "escape", esc)


if __name__ == "__main__":
    main()
