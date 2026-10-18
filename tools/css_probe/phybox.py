"""A model's collision hull bounds from its .phy in the server install
(Source axes, inches; specs/cs_source/physics_props.md 1). Reads the user's
own install at run time; nothing is extracted.

    python3 tools/css_probe/phybox.py models/props_junk/plasticcrate01a.mdl ...

Prints, per solid, the AABB of the points its convex pieces use, and the
text section's mass and surfaceprop.
"""

import os
import re
import struct
import sys

sys.path.insert(0, os.path.dirname(__file__))
import vpkls  # noqa: E402

DS = os.path.expanduser("~/games/css-ds")
M = 39.37007874  # inches a metre
_dirs = {}


def read(rel):
    rel = rel.lower()
    for vdir in (DS + "/cstrike/cstrike_pak_dir.vpk", DS + "/hl2/hl2_misc_dir.vpk"):
        if vdir not in _dirs:
            _dirs[vdir] = vpkls.read_dir(vdir)
        if rel in _dirs[vdir]:
            return vpkls.cat(vdir, _dirs[vdir], rel)
    raise FileNotFoundError(rel)


def solids(data):
    hsize, _, n, _ = struct.unpack_from("<4i", data, 0)
    off = hsize
    out = []
    for _ in range(n):
        size = struct.unpack_from("<i", data, off)[0]
        s = off + 4 + 28 if data[off + 4:off + 8] == b"VPHY" else off + 4
        root = s + struct.unpack_from("<i", data, s + 32)[0]
        pts = set()
        base = [None]

        def walk(node):
            right, piece = struct.unpack_from("<2i", data, node)
            if right == 0:
                lp = node + piece
                base[0] = lp + struct.unpack_from("<i", data, lp)[0]
                t = struct.unpack_from("<h", data, lp + 12)[0]
                for i in range(t):
                    words = struct.unpack_from("<4I", data, lp + 16 + 16 * i)
                    for w in words[1:]:
                        pts.add(w & 0xFFFF)
            else:
                walk(node + 28)
                walk(node + right)

        walk(root)
        src = []
        for i in pts:
            x, y, z = struct.unpack_from("<3f", data, base[0] + 16 * i)
            src.append((x * M, z * M, -y * M))
        lo = tuple(min(p[k] for p in src) for k in range(3))
        hi = tuple(max(p[k] for p in src) for k in range(3))
        out.append((lo, hi))
        off += 4 + size
    text = data[off:].split(b"\0")[0].decode(errors="replace")
    return out, text


def main():
    for model in sys.argv[1:]:
        boxes, text = solids(read(re.sub(r"\.mdl$", ".phy", model)))
        mass = re.findall(r'"mass"\s*"([^"]+)"', text)
        prop = re.findall(r'"surfaceprop"\s*"([^"]+)"', text)
        for (lo, hi) in boxes:
            print(model, "min %.3f %.3f %.3f max %.3f %.3f %.3f" % (lo + hi), "mass", mass[:1], "surfaceprop",
                  prop[:1])


if __name__ == "__main__":
    main()
