# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.
"""Writes synthetic-lidar.las: an invented LiDAR tile for synthetic-line.ohl.json.

Every point is made up, classified as the ASPRS LAS 1.4 standard classes
(libraries/lidar-classes/asprs-las-1.4.ohlc):

- 2, ground: the example's ground (make_synthetic_survey.py), every 2.5 m
  within 50 m of the routes, with a little noise;
- 3 and 5, low and high vegetation: three stands of trees near the line,
  placed so that the checks find something: tall trees beside span T1-T2
  that could fall onto the wires, a tree under span T4-T5 within the
  wires' clearance, a stand beside the branch;
- 6, building: one house beside span T4-T5;
- 14, wire conductor: points along the wires as `ohl line` solves them at
  15 °C, every 0.5 m, with 3 cm of noise (for `ohl lidar fit-wires`).

A LAS 1.2 file, point format 0, millimetre resolution; no randomness that
is not seeded, so the file is the same each time:

    cargo build -p ohl-cli
    python3 examples/synthetic-line/make_synthetic_lidar.py
    # OHL=path/to/ohl python3 … to use another program
"""

import csv
import math
import os
import random
import struct
import subprocess
import tempfile
from pathlib import Path

from make_synthetic_survey import ROAD_X, ground, line

HERE = Path(__file__).parent
OHL = os.environ.get("OHL", str(HERE.parent.parent / "target" / "debug" / "ohl"))
GROUND, LOW_VEG, HIGH_VEG, BUILDING, WIRE = 2, 3, 5, 6, 14

rng = random.Random(20261107)
points = []  # (x, y, z, class)

# The routes' centrelines: the main route and the branch.
pos = {s["id"]: s["position"] for s in line["structures"]}
routes = [["T1", "T2", "T3", "T4", "T5"], ["T3", "T6", "T7"]]
segments = [(pos[r[k]], pos[r[k + 1]]) for r in routes for k in range(len(r) - 1)]


def distance_to_routes(x, y):
    best = math.inf
    for a, b in segments:
        dx, dy = b[0] - a[0], b[1] - a[1]
        t = max(0.0, min(1.0, ((x - a[0]) * dx + (y - a[1]) * dy) / (dx * dx + dy * dy)))
        best = min(best, math.hypot(x - a[0] - t * dx, y - a[1] - t * dy))
    return best


# Ground, every 2.5 m within 50 m of the routes (the road's embankment kept off).
STEP = 2.5
for i in range(int(-80 / STEP), int(1320 / STEP)):
    for j in range(int(-80 / STEP), int(640 / STEP)):
        x, y = i * STEP + rng.uniform(-0.4, 0.4), j * STEP + rng.uniform(-0.4, 0.4)
        if distance_to_routes(x, y) > 50.0 or abs(x - ROAD_X) < 8.0:
            continue
        points.append((x, y, ground(x, y) + rng.gauss(0.0, 0.03), GROUND))


def tree(cx, cy, height, crown):
    """A tree: its crown's surface from 0.45 of its height up, points under it."""
    base = ground(cx, cy)
    for _ in range(int(60 * crown)):
        u, v = rng.uniform(-1, 1), rng.uniform(0, 2 * math.pi)
        r = crown * math.sqrt(1 - u * u)
        z = base + 0.45 * height + (u + 1) / 2 * 0.55 * height
        points.append((cx + r * math.cos(v), cy + r * math.sin(v), z, HIGH_VEG))
    for _ in range(25):
        a, d = rng.uniform(0, 2 * math.pi), rng.uniform(0, crown)
        x, y = cx + d * math.cos(a), cy + d * math.sin(a)
        points.append((x, y, ground(x, y) + rng.uniform(0.2, 1.5), LOW_VEG))


# Tall trees 18 m right of the axis of span T1-T2, 12 m from its nearest wire
# (some could fall onto it).
for k in range(6):
    tree(110 + 22 * k, -18 + rng.uniform(-1.5, 1.5), rng.uniform(22, 26), 3.5)
# A tree under span T4-T5, 6 m off the axis, 15 m high: under a wire some 19 m
# up, within its 5 m clearance.
ax = (1180 - pos["T4"][0]) / (pos["T5"][0] - pos["T4"][0])
axis_y = pos["T4"][1] + ax * (pos["T5"][1] - pos["T4"][1])
tree(1180, axis_y + 6, 15.0, 3.0)
# A stand beside the branch, 18-24 m east of it.
for k in range(4):
    tree(660 + rng.uniform(-2, 4), 380 + 25 * k, rng.uniform(14, 18), 3.0)

# A house beside span T4-T5, 26 m off the axis: 12 × 8 m, 7 m high, a flat roof.
hx0, hy0, hw, hd, hh = 1000.0, 63.0 + 26.0, 12.0, 8.0, 7.0
hz = ground(hx0 + hw / 2, hy0 + hd / 2)
for i in range(int(hw / 0.5) + 1):
    for j in range(int(hd / 0.5) + 1):
        points.append((hx0 + 0.5 * i, hy0 + 0.5 * j, hz + hh + rng.gauss(0, 0.02), BUILDING))
for k in range(int(2 * (hw + hd) / 0.5)):
    s = 0.5 * k
    x, y = (hx0 + s, hy0) if s < hw else (hx0 + hw, hy0 + s - hw) if s < hw + hd else \
        (hx0 + hw - (s - hw - hd), hy0 + hd) if s < 2 * hw + hd else (hx0, hy0 + hd - (s - 2 * hw - hd))
    for z in range(1, int(hh)):
        points.append((x, y, hz + z, BUILDING))

# The wires as solved at 15 °C, every 0.5 m along them.
with tempfile.TemporaryDirectory() as tmp:
    out = Path(tmp) / "wires.csv"
    subprocess.run([OHL, "line", str(HERE / "synthetic-line.ohl.json"), "--temperature", "15",
                    "--csv", str(out)], check=True, capture_output=True)
    rows = list(csv.DictReader(out.open()))
by_span = {}
for r in rows:
    by_span.setdefault((r["section"], r["span"]), []).append((float(r["x"]), float(r["y"]), float(r["z"])))
for pts in by_span.values():
    for a, b in zip(pts, pts[1:]):
        n = max(1, int(math.dist(a, b) / 0.5))
        for k in range(n):
            t = k / n
            points.append(tuple(a[i] + t * (b[i] - a[i]) + rng.gauss(0, 0.03) for i in range(3)) + (WIRE,))

# LAS 1.2, point format 0.
SCALE = 0.001
xs, ys, zs = ([p[i] for p in points] for i in range(3))
offset = (math.floor(min(xs)), math.floor(min(ys)), math.floor(min(zs)))
header = struct.pack(
    "<4sHH16sBB32s32sHHHIIBHI5I12d",
    b"LASF", 0, 0, bytes(16), 1, 2,
    b"OpenOHL synthetic example".ljust(32, b"\0"), b"make_synthetic_lidar.py".ljust(32, b"\0"),
    311, 2026, 227, 227, 0, 0, 20, len(points), len(points), 0, 0, 0, 0,
    SCALE, SCALE, SCALE, *offset,
    max(xs), min(xs), max(ys), min(ys), max(zs), min(zs),
)
assert len(header) == 227
with open(HERE / "synthetic-lidar.las", "wb") as f:
    f.write(header)
    for x, y, z, c in points:
        f.write(struct.pack("<iiiHBBbBH",
                            round((x - offset[0]) / SCALE), round((y - offset[1]) / SCALE),
                            round((z - offset[2]) / SCALE), 0, 0b001001, c, 0, 0, 0))
counts = {}
for p in points:
    counts[p[3]] = counts.get(p[3], 0) + 1
print(f"wrote {len(points)} points: " + ", ".join(f"class {c}: {n}" for c, n in sorted(counts.items())))
