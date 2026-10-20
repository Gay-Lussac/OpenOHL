# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

"""Independent reference values for benchmarks/wirefe/*.toml.

Elastic cable integrated along its UNSTRESSED length s0 with RK4 (not the
closed-form element used in Rust):
    T = sqrt(H^2 + Tz^2),  dx/ds0 = (H/T)(1 + T/EA),  dz/ds0 = (Tz/T)(1 + T/EA),
    dTz/ds0 = w,  Tz jumps by +P at a downward point load P.
Coordinates are in the load plane: z opposite to the load, x perpendicular.
End tensions found by 2D shooting (Newton with finite differences), fixed
iteration cap.

Run: python3 benchmarks/wirefe/reference_elastic_catenary.py  (standard library only)
"""

import math

EA = 34.64e6
N = 40_000  # RK4 steps over the cable


def integrate(H, V0, L0, w, point=None):
    """point = (s0_p, P): returns end (x, z), position at s0_p, Tz at end.

    The cable is integrated segment by segment so that a point load falls
    exactly on a segment boundary (an off-grid jump would add ~1e-5 error).
    """
    def f(Tz):
        T = math.hypot(H, Tz)
        return (H / T) * (1 + T / EA), (Tz / T) * (1 + T / EA)

    bounds = [0.0, L0] if point is None else [0.0, point[0], L0]
    x = z = 0.0
    Tz = V0
    at_point = None
    for a, b in zip(bounds, bounds[1:]):
        if a > 0.0:
            at_point = (x, z)
            Tz += point[1]
        h = (b - a) / N
        for _ in range(N):
            # Tz grows linearly: Simpson/RK4 on (x, z) with exact Tz(s).
            k1, k2, k4 = f(Tz), f(Tz + w * h / 2), f(Tz + w * h)
            x += h / 6 * (k1[0] + 4 * k2[0] + k4[0])
            z += h / 6 * (k1[1] + 4 * k2[1] + k4[1])
            Tz += w * h
    return x, z, Tz, at_point


def shoot(L0, w, target, point=None):
    H, V0 = 25_000.0, -w * L0 / 2
    for _ in range(30):  # fixed cap
        x, z, _, _ = integrate(H, V0, L0, w, point)
        r = (target[0] - x, target[1] - z)
        if math.hypot(*r) < 1e-10:
            break
        eh, ev = 1e-3, 1e-3
        xh, zh, _, _ = integrate(H + eh, V0, L0, w, point)
        xv, zv, _, _ = integrate(H, V0 + ev, L0, w, point)
        a, b = (xh - x) / eh, (xv - x) / ev
        c, d = (zh - z) / eh, (zv - z) / ev
        det = a * d - b * c
        H += (d * r[0] - b * r[1]) / det
        V0 += (-c * r[0] + a * r[1]) / det
    x, z, Tz_end, at_point = integrate(H, V0, L0, w, point)
    return {"H": H, "V0": V0, "T_i": math.hypot(H, V0), "T_j": math.hypot(H, Tz_end), "point": at_point}


if __name__ == "__main__":
    print("span 1: level 300 m, L0 = 300.4 m, w = 15.97 N/m")
    r = shoot(300.4, 15.97, (300.0, 0.0))
    print(f"  H {r['H']:.10g}  T_i {r['T_i']:.10g}  T_j {r['T_j']:.10g}")

    # Inclined span with wind: load (0, 8, -15.97) N/m, chord (450, 0, 35).
    # In the load plane: up = (0, -8, 15.97)/|q|, x = horizontal part of chord.
    q = (0.0, 8.0, -15.97)
    wq = math.sqrt(sum(c * c for c in q))
    up = tuple(-c / wq for c in q)
    d = (450.0, 0.0, 35.0)
    vz = sum(a * b for a, b in zip(d, up))
    hx = math.sqrt(sum((a - vz * u) ** 2 for a, u in zip(d, up)))
    print(f"span 2: inclined 450 m +35 m with wind, L0 = 452 m (|q| = {wq:.10g}, in-plane chord {hx:.10g}, {vz:.10g})")
    r = shoot(452.0, wq, (hx, vz))
    print(f"  H {r['H']:.10g}  T_i {r['T_i']:.10g}  T_j {r['T_j']:.10g}")

    print("span 3: level 300 m, L0 = 300.4 m, 2 kN point load at s0 = 120 m")
    r = shoot(300.4, 15.97, (300.0, 0.0), point=(120.0, 2_000.0))
    print(f"  H {r['H']:.10g}  T_i {r['T_i']:.10g}  T_j {r['T_j']:.10g}  load point x {r['point'][0]:.10g} z {r['point'][1]:.10g}")
