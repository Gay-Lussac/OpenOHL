# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

"""Independent reference values for benchmarks/clearance/*.toml.

Deliberately different from the Rust implementation:
- galloping ellipses: the distance between an ellipse K and the same
  ellipse moved by v is the distance from v to the difference body
  K - K = 2K (an ellipse twice the size about the origin); found by dense
  sampling of 2K (2e5 points), not by comparing two polygons;
- catenary sag: brute-force maximum of chord minus curve over 1e6 samples,
  not the "slope equals chord slope" formula;
- clearances to the surface: the catenary from cosh, the polyline's
  bound from the chords' sagittas, checked against the true largest
  distance of the catenary from the polyline (dense sampling).

OpenOHL compares the galloping ellipses as circumscribed 72-gons: its gaps
are at most the true gaps printed here, and above them less
A (1/cos 2.5 deg - 1); where the closest points are polygon vertices (side
by side or one above the other, untilted) the gap is exactly
gap - (extent of both ellipses) (1/cos 2.5 deg - 1), printed too.

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


def surface(span, c, h, d, t, offset, pieces=64, step=0.5):
    """Clearances of a level catenary span (constant c, clamps h above
    flat ground) to the surface of a conductor of diameter d with radial
    ice t, sampled as `pieces` even in x."""
    r = d / 2 + t
    sag_ = c * (math.cosh(span / (2 * c)) - 1)
    z = lambda x: h - sag_ + c * (math.cosh((x - span / 2) / c) - 1)
    pts = [(span * k / pieces, z(span * k / pieces)) for k in range(pieces + 1)]
    chords = [math.dist(pts[k], pts[k + 1]) for k in range(pieces)]
    bound = max(c - math.sqrt(c * c - l * l / 4) for l in chords)
    true = 0.0
    for k in range(pieces):
        (x0, z0), (x1, z1) = pts[k], pts[k + 1]
        for j in range(1, 400):
            f = j / 400
            true = max(true, z0 + (z1 - z0) * f - z(x0 + (x1 - x0) * f))
    low = h - sag_
    l = chords[pieces // 2]
    s = l / math.ceil(l / step)
    return {
        "sag": sag_,
        "surface_radius": r,
        "bound": bound,
        "true_distance_from_polyline": true,
        "ground_vertical": low - r - bound,
        "ground_radial": math.sqrt(low * low - (s / 2) ** 2) - r - bound,
        "point_radial": offset - r - bound,
    }


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
    out = 1 / math.cos(math.radians(2.5))
    print("galloping-001 circumscribed (vertices)", 8.0 - 5.12 * out)
    print("galloping-003 circumscribed (vertices)", 6.0 - 5.2 * out)
    # galloping-004: an eighth of the span, sin(pi/8) of the size.
    b4 = 0.4 * 12.8 * math.sin(math.pi / 8)
    print("galloping-004 gap", ellipse_gap(12.8 * math.sin(math.pi / 8), b4, 0.0, (8.0, 0.0)))
    print("galloping-004 circumscribed (vertices)", 8.0 - b4 * out)
    print("wirefit-001 sag", sag(1400.0, 180.0, 512.0, 300.0))
    print("clearance-surface-001", surface(300.0, 1500.0, 20.0, 0.02814, 0.010, 5.0))
