# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

"""Independent reference values for benchmarks/spotting/flat.toml.

Optimum spotting on level ground with one dead-end type (at the ends) and
one cheaper tangent type of the same height, no span limits but the ground
clearance. By hand, without any search:

- the wire hangs from both supports at height h; its sag at mid-span is
  C (cosh(L / 2C) - 1) with C = H / w, so the longest span keeping the
  clearance c is L_max = 2 C acosh(1 + (h - c) / C);
- structures stand on a grid of step s, so the longest usable span is
  L_grid = s floor(L_max / s);
- the fewest spans over a route of length D is n = ceil(D / L_grid); every
  inner structure is a tangent, so the least cost is
  2 * dead_end + (n - 1) * tangent.

Run: python3 benchmarks/spotting/reference_spotting.py  (standard library only)
"""

import math

CASES = [
    # name, D (m), h (m), clearance (m), H (N), w (N/m), step (m), dead-end, tangent
    ("2000 m route", 2000.0, 20.0, 5.0, 21_000.0, 15.0, 50.0, 10.0, 5.0),
    ("2150 m route", 2150.0, 20.0, 5.0, 21_000.0, 15.0, 50.0, 10.0, 5.0),
    ("3000 m route, 25 m step, taller", 3000.0, 28.0, 7.0, 25_000.0, 16.0, 25.0, 60.0, 25.0),
]

for name, D, h, c, H, w, s, de, tan in CASES:
    C = H / w
    l_max = 2 * C * math.acosh(1 + (h - c) / C)
    l_grid = s * math.floor(l_max / s)
    n = math.ceil(D / l_grid)
    cost = 2 * de + (n - 1) * tan
    print(f"{name}: L_max = {l_max:.4f} m, L_grid = {l_grid:.0f} m, "
          f"structures = {n + 1}, cost = {cost:.1f}, longest span = {l_grid:.0f} m")
