# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

"""Independent reference values for benchmarks/clearance/*.toml.

Deliberately different from the Rust implementation:
- galloping ellipses: the distance between an ellipse K and the same
  ellipse moved by v is the distance from v to the difference body
  K - K = 2K (an ellipse twice the size about the origin); found by dense
  sampling of 2K (2e5 points), not by comparing two polygons;
- catenary sag: brute-force maximum of chord minus curve over 1e6 samples,
  not the "slope equals chord slope" formula.

Run: python3 benchmarks/clearance/reference_clearance.py  (standard library only)
"""

import math


def ellipse_gap(major, minor, tilt_deg, v):
    """Distance between an ellipse (full axes) and itself moved by v."""
    a, b = major, minor  # semi-axes of 2K
    s, c = math.sin(math.radians(tilt_deg)), math.cos(math.radians(tilt_deg))
    best = math.inf
    n = 200_000
    for k in range(n):
        t = 2 * math.pi * k / n
        A, B = a * math.cos(t), b * math.sin(t)
        x, y = A * s + B * c, A * c - B * s
        best = min(best, math.hypot(v[0] - x, v[1] - y))
    return best


def sag(c, s0, z0, span):
    z = lambda s: z0 + c * (math.cosh((s - s0) / c) - 1)
    za, zb = z(0.0), z(span)
    n = 1_000_000
    return max(za + (zb - za) * (k * span / n) / span - z(k * span / n) for k in range(n + 1))


if __name__ == "__main__":
    # galloping-001/002: A = 1.25 * 10 + 0.3 = 12.8 m, B = 0.4 A = 5.12 m at
    # mid-span (single loop); two wires 8 m apart horizontally.
    print("galloping-001 gap (tilt 0)", ellipse_gap(12.8, 5.12, 0.0, (8.0, 0.0)))
    print("galloping-002 gap (tilt 10)", ellipse_gap(12.8, 5.12, 10.0, (8.0, 0.0)))
    # galloping-003: two loops at a quarter of the span: full size; wires
    # 6 m apart vertically, A = 0.5 * 10 + 0.2 = 5.2, B = 2.6.
    print("galloping-003 gap (vertical)", ellipse_gap(5.2, 2.6, 0.0, (0.0, 6.0)))
    print("wirefit-001 sag", sag(1400.0, 180.0, 512.0, 300.0))
