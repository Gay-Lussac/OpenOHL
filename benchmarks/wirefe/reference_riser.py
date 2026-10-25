# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

"""Independent reference values for benchmarks/wirefe/riser.toml: a vertical
riser (a jumper given by its tension, owner decision 2026-10-25).

Invented data: a wire of w = 15 N/m own weight and EA = 30 MN, dead-ended
between two points 30 m one above the other, tensioned to T = 5000 N at
its top in still air.

Still air, exact (a continuous elastic wire hanging straight): along the
unstressed length s from the top, T(s) = T - w s, so the stretched length
is L0 + (T L0 - w L0^2 / 2) / EA = 30 m, a quadratic in L0; the bottom's
tension is T - w L0.

In a wind of q = 10 N/m across it (per unstressed metre), with the same
L0: the continuous elastic string, integrated along s from the top with
RK4 (2000 steps): dF/ds = (q, -w) is the change of the tension vector
(F the force of the wire below on the part above, pulling), dx/ds =
(1 + |F|/EA) F/|F|, shot from the top's force (Fx, Fz) by Newton (its
Jacobian by central differences) so that the end lands at the bottom
point. The code solves a polygon of ~1 m bars with the loads at the
nodes: the reference is the continuum (the difference is the polygon's,
about (l/L)^2, below 1e-3 here).

Hand check of the wind: a taut string's middle deflects by
q L^2 / (8 T_mean); the bow stretches the wire by about 8 d^2 / 3L, which
raises its tension by EA 8 d^2 / 3L^2 (the length is kept), so
d = q L^2 / 8 (T0 + EA 8 d^2 / 3 L^2) with T0 the still-air mean,
4775 N: d = 0.160 m (the tension then 7050 N), against 0.236 m at the
still-air tension.

Run: python3 -I benchmarks/wirefe/reference_riser.py  (standard library only)
"""

import math

W, EA, T, H, Q = 15.0, 30e6, 5000.0, 30.0, 10.0


def cut_length():
    # L0 + (T L0 - w L0^2/2)/EA = H  ->  (w/2EA) L0^2 - (1 + T/EA) L0 + H = 0
    a, b, c = W / (2 * EA), -(1 + T / EA), H
    return (-b - math.sqrt(b * b - 4 * a * c)) / (2 * a)


def shoot(fx, fz, l0, q, steps=2000):
    """From the top (0, 0) down to s = l0 along the unstressed wire: the
    end point and the tension vector there.

    State: x, z and g, the wire's tension vector at s pointing down along
    the wire (from the top's g = (fx, fz)). A piece ds carries the load
    (q, -w) ds, so dg/ds = (-q, +w); it runs along g, stretched by 1 + |g|/EA.
    """
    def f(state):
        x, z, gx, gz = state
        g = math.hypot(gx, gz)
        k = 1 + g / EA
        return (k * gx / g, k * gz / g, -q, W)

    h = l0 / steps
    s = (0.0, 0.0, fx, fz)
    for _ in range(steps):
        k1 = f(s)
        k2 = f(tuple(a + h / 2 * b for a, b in zip(s, k1)))
        k3 = f(tuple(a + h / 2 * b for a, b in zip(s, k2)))
        k4 = f(tuple(a + h * b for a, b in zip(s, k3)))
        s = tuple(a + h / 6 * (b1 + 2 * b2 + 2 * b3 + b4) for a, b1, b2, b3, b4 in zip(s, k1, k2, k3, k4))
    return s


def windy(l0):
    # Unknown: the tension vector at the top (pointing down the wire).
    g = [0.0, -T]
    for _ in range(50):  # fixed iteration cap (exit clause)
        def res(v):
            x, z, _, _ = shoot(v[0], v[1], l0, Q)
            return [x - 0.0, z + H]
        r = res(g)
        if max(abs(v) for v in r) < 1e-12:
            break
        e = 1e-3
        jac = [[0.0, 0.0], [0.0, 0.0]]
        for j in range(2):
            up, dn = list(g), list(g)
            up[j] += e
            dn[j] -= e
            ru, rd = res(up), res(dn)
            for i in range(2):
                jac[i][j] = (ru[i] - rd[i]) / (2 * e)
        det = jac[0][0] * jac[1][1] - jac[0][1] * jac[1][0]
        d0 = (r[0] * jac[1][1] - r[1] * jac[0][1]) / det
        d1 = (jac[0][0] * r[1] - jac[1][0] * r[0]) / det
        g = [g[0] - d0, g[1] - d1]
    else:
        raise RuntimeError("Newton did not converge")
    # The middle: the point at mid-height (z = -H/2), by bisection on s.
    def at(s):
        return shoot(g[0], g[1], s, Q, steps=max(10, int(2000 * s / l0)))
    lo, hi = 0.0, l0
    for _ in range(60):
        mid = 0.5 * (lo + hi)
        if at(mid)[1] > -H / 2:
            lo = mid
        else:
            hi = mid
    x_mid = at(0.5 * (lo + hi))[0]
    _, _, gx, gz = shoot(g[0], g[1], l0, Q)
    return math.hypot(*g), math.hypot(gx, gz), x_mid


if __name__ == "__main__":
    l0 = cut_length()
    top, bottom, mid = windy(l0)
    print("riser-001")
    print(f"  cut_length_m               {l0:.10g}")
    print(f"  top_n                      {T:.10g}")
    print(f"  bottom_n                   {T - W * l0:.10g}")
    print(f"  wind_mid_deflection_m      {mid:.10g}")
    print(f"  wind_top_n                 {top:.10g}")
    print(f"  wind_bottom_n              {bottom:.10g}")
