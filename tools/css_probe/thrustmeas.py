"""An offset phys_thruster's turn (specs/source/physics_brushes.md Q3,
2026-10-18).

    python3 tools/css_probe/thrustmeas.py <force> <dy> <dz> [flags] [secs]

With `sv_gravity 0` an oil drum (`oildrum001`, 30 kg, its origin at the
base, its centre 22.5 units up) floats at (320, 1540, 300) on de_dust2 and
a thruster pushing along +x sits `dy` to the side of the drum's origin and
`dz` above it (flags 5: Start On, Apply Torque). A turn about the vertical
axis through the drum leaves its origin where it is; a tip (a torque about
a horizontal axis) swings the origin round the centre. Prints the drum's
origin over time (`mashup_htrack`), then puts gravity back.
"""

import os
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
import physmeas as P  # noqa: E402

X, Y = 320.0, 1540.0
Z = float(os.environ.get("Z", "300"))


def main():
    force, dy, dz = float(sys.argv[1]), float(sys.argv[2]), float(sys.argv[3])
    flags = int(sys.argv[4]) if len(sys.argv) > 4 else 5
    secs = float(sys.argv[5]) if len(sys.argv) > 5 else 2.0
    P.rcon("sv_gravity 0")
    try:
        drum = P.spawn("models/props_c17/oildrum001.mdl", X, Y, Z, 0, 0, "prop_physics", "targetname", "tdrum")
        time.sleep(0.3)
        th = P.spawn("-", X, Y + dy, Z + dz, 0, 0, "phys_thruster", "attach1", "tdrum", "force", str(force),
                     "spawnflags", str(flags))
        rows, _ = P.track(drum, secs)
        P.rcon(f"mashup_input {th} Kill")
        P.rcon(f"mashup_input {drum} Kill")
    finally:
        P.rcon("sv_gravity 800")
    t0 = rows[0][0]
    for r in rows[:: max(1, len(rows) // 25)]:
        print("%.3f %.3f %.3f %.3f" % (r[0] - t0, r[1] - X, r[2] - Y, r[3] - Z))


if __name__ == "__main__":
    main()
