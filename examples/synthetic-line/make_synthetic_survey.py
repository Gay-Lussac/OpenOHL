# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
"""Writes synthetic-survey.csv: invented ground for synthetic-line.ols.json.

Smooth ground through the structure bases (Gaussian radial basis
interpolation), an 8 m hill under span T2-T3 of the main route (so a hot
wire comes close to the ground there), and a road embankment across span
T4-T5 surveyed as a breakline. A 20 m grid plus the road points; no
randomness, so the file is reproducible:

    python3 examples/synthetic-line/make_synthetic_survey.py
"""

import json
import math
from pathlib import Path

HERE = Path(__file__).parent
line = json.loads((HERE / "synthetic-line.ols.json").read_text())
bases = [s["position"] for s in line["structures"]]

# Gaussian RBF through the bases around their mean height.
RADIUS = 400.0
mean = sum(b[2] for b in bases) / len(bases)


def kernel(a, b):
    return math.exp(-((a[0] - b[0]) ** 2 + (a[1] - b[1]) ** 2) / RADIUS**2)


n = len(bases)
matrix = [[kernel(bases[i], bases[j]) for j in range(n)] + [bases[i][2] - mean] for i in range(n)]
for c in range(n):  # Gauss-Jordan elimination with partial pivoting
    pivot = max(range(c, n), key=lambda r: abs(matrix[r][c]))
    matrix[c], matrix[pivot] = matrix[pivot], matrix[c]
    for r in range(n):
        if r != c:
            f = matrix[r][c] / matrix[c][c]
            matrix[r] = [x - f * y for x, y in zip(matrix[r], matrix[c])]
weights = [matrix[i][n] / matrix[i][i] for i in range(n)]


def ground(x, y):
    z = mean + sum(w * kernel((x, y), b) for w, b in zip(weights, bases))
    # Hill under span T2-T3 (main route along y = 0 there), 8 m high.
    z += 8.0 * math.exp(-((x - 470.0) ** 2 + (y - 10.0) ** 2) / 70.0**2)
    return z


# Road across span T4-T5: a 2 m embankment, 8 m wide at the top, along
# x = 1090 (its slopes run to the grid points 10 m either side).
ROAD_X, HALF_WIDTH, BANK = 1090.0, 4.0, 2.0
rows = ["x,y,z,code,line"]
for i in range(-5, 67):
    for j in range(-8, 32):
        x, y = i * 20.0, j * 20.0
        rows.append(f"{x:.2f},{y:.2f},{ground(x, y):.3f},GND,")
for side, dx in (("left", -HALF_WIDTH), ("right", HALF_WIDTH)):
    for j in range(-8, 32):
        x, y = ROAD_X + dx, j * 20.0
        rows.append(f"{x:.2f},{y:.2f},{ground(x, y) + BANK:.3f},ROAD,road-{side}")
(HERE / "synthetic-survey.csv").write_text("\n".join(rows) + "\n")
print(f"wrote {len(rows) - 1} points")
