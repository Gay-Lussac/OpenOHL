# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

"""Independent reference values for benchmarks/sagtension/loaded.toml.

Spans with additional vertical loads, solved as an ODE instead of the
closed-form segments used in Rust:
    y' = p,  p' = (w(x)/H) sqrt(1 + p^2),  p jumps by P/H at a point load,
    s' = sqrt(1 + p^2),  I' = H (1 + p^2)   (I = integral of T ds)
integrated with RK4 (fixed step), shooting on the start slope with the
secant method; change of state with a secant on log(H). Fixed iteration caps.
The cable's own weight w_base is conserved (owner decision 2026-10-21): per
metre of the span's length it is w_base / k, k = exp(alpha dt)(1 + Tmean/EA)
(a fixed point); markers and partial ice as given.

Run: python3 benchmarks/sagtension/reference_loaded.py  (standard library only)
"""

import math

EA, ALPHA = 34.64e6, 18.9e-6
STEPS = 30_000


def integrate(S, H, w_base, points, dist, p0):
    """Returns end elevation, length, tension integral, max sag info, p at ends."""
    def w_at(x):
        return w_base + sum(wd for a, b, wd in dist if a <= x <= b)

    breaks = sorted({0.0, S, *(x for x, _ in points), *(v for a, b, _ in dist for v in (a, b))})
    y, p, s, I = 0.0, p0, 0.0, 0.0
    samples = [(0.0, 0.0)]
    for x0, x1 in zip(breaks, breaks[1:]):
        if x0 > 0.0:
            p += sum(P for x, P in points if abs(x - x0) < 1e-9) / H
        n = max(2, int(STEPS * (x1 - x0) / S))
        h = (x1 - x0) / n
        w = w_at(0.5 * (x0 + x1))
        f = lambda st: (st[1], w / H * math.sqrt(1 + st[1] ** 2), math.sqrt(1 + st[1] ** 2), H * (1 + st[1] ** 2))
        st = (y, p, s, I)
        for i in range(n):
            k1 = f(st)
            k2 = f(tuple(a + h / 2 * b for a, b in zip(st, k1)))
            k3 = f(tuple(a + h / 2 * b for a, b in zip(st, k2)))
            k4 = f(tuple(a + h * b for a, b in zip(st, k3)))
            st = tuple(a + h / 6 * (b1 + 2 * b2 + 2 * b3 + b4) for a, b1, b2, b3, b4 in zip(st, k1, k2, k3, k4))
            samples.append((x0 + (i + 1) * h, st[0]))
        y, p, s, I = st
    return y, s, I, samples, p


def shape(S, h, H, w_base, points, dist):
    a, b = -0.5, 0.5
    fa = integrate(S, H, w_base, points, dist, a)[0] - h
    fb = integrate(S, H, w_base, points, dist, b)[0] - h
    for _ in range(40):
        if fb == fa:
            break
        c = b - fb * (b - a) / (fb - fa)
        a, fa = b, fb
        b, fb = c, integrate(S, H, w_base, points, dist, c)[0] - h
        if abs(b - a) < 1e-15:
            break
    y, s, I, samples, p_end = integrate(S, H, w_base, points, dist, b)
    sag = max(h * x / S - yy for x, yy in samples)
    return {"length": s, "integral": I, "sag": sag,
            "tension_left": H * math.sqrt(1 + b * b), "tension_right": H * math.sqrt(1 + p_end * p_end)}


def unstrained(S, h, H, w_base, points, dist, thermal=1.0):
    """Unstressed length at the reference temperature, the own weight w_base
    spread over the span's length."""
    k = 1.0
    for _ in range(100):
        r = shape(S, h, H, w_base / k, points, dist)
        k_new = thermal * (1 + r["integral"] / r["length"] / EA)
        if abs(k_new - k) <= 1e-15 * k:
            break
        k = k_new
    return r["length"] / k_new


def change_of_state(S, h, L0, t0, w_base, points, dist, t):
    thermal = math.exp(ALPHA * (t - t0))
    f = lambda lh: unstrained(S, h, math.exp(lh), w_base, points, dist, thermal) - L0
    a, b = math.log(15_000.0), math.log(40_000.0)
    fa, fb = f(a), f(b)
    for _ in range(40):
        if fb == fa:
            break
        c = b - fb * (b - a) / (fb - fa)
        a, fa, b, fb = b, fb, c, f(c)
        if abs(b - a) < 1e-13:
            break
    return math.exp(b)


if __name__ == "__main__":
    print("shape: level 300 m, H 28 kN, w 15.97, 2 kN at 100 m")
    for k, v in shape(300.0, 0.0, 28_000.0, 15.97, [(100.0, 2_000.0)], []).items():
        if k != "integral":
            print(f"  {k:14s} {v:.10g}")
    print("change of state: inclined 350 m +20 m, 28 kN @ 15 C bare ->")
    print("  -5 C with 3 markers 70 N at 87.5/175/262.5 m and 10 N/m ice on 0..150 m")
    L0 = unstrained(350.0, 20.0, 28_000.0, 15.97, [], [])
    H = change_of_state(350.0, 20.0, L0, 15.0, 15.97, [(87.5, 70.0), (175.0, 70.0), (262.5, 70.0)], [(0.0, 150.0, 10.0)], -5.0)
    print(f"  horizontal_tension {H:.10g}")
