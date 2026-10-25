# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

"""Independent reference values for sag-004 (benchmarks/sagtension/sagging.toml):
a section in the blocks over a frictionless sheave (owner decision 2026-10-25).

Two inclined spans A-B-C. The section's unstressed length L0 is that of
every span at the stringing tension H0 at the stringing temperature (the
given tension stays "H in the ruling span"). Over the sheave at B the
wire's tension is equal on both sides, so the spans' horizontal tensions
H1, H2 solve the two equations

    T_right,1(H1) = T_left,2(H2)          (equal tension at the sheave)
    L0_1(H1) + L0_2(H2) = L0              (the length conserved)

where, on a catenary of load q per metre of wire, T = H cosh((x - xm)/C)
= H + q (z - z_low) at an end (C = H/q). The spans are those of
reference_change_of_state.py (numerical integrals, the own weight
conserved: q = w / k with k = L / L0). Numerics different from the code
(which propagates H span by span from the first span's and takes the
length as the residual of a bracketed secant): Newton on (log H1, log H2)
together, its Jacobian by central differences.

Hand check (the parabola): z_B - z_low = h/2 + D + H h^2/(2 q S^2) for the
span ending at B rising h (from z = h x/S - q x (S-x)/2H, its vertex at
x_m = S/2 - H h/(q S)). For span 1 (300 m, +40 m) and span 2 (250 m,
-30 m, so B is 30 m above C) at about 28 kN, q = 15.96 N/m:
    span 1: D = 6.41 m, H h^2/(2 q S^2) = 28000 x 1600 / (2 x 15.96 x 90000)
            = 15.59 m; z_B - z_low,1 = 20 + 6.41 + 15.59 = 42.0 m;
    span 2, mirrored (its back end B is the high one, h = 30 m):
            D = 4.45 m, 28000 x 900 / (2 x 15.96 x 62500) = 12.63 m;
            z_B - z_low,2 = 15 + 4.45 + 12.63 = 32.1 m;
    so H2 - H1 = q (42.0 - 32.1) = 158 N (exact below: 157.4 N).

Run: python3 -I benchmarks/sagtension/reference_sheaves.py  (standard library only)
"""

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import reference_change_of_state as cos  # noqa: E402
import reference_sagging as rs  # noqa: E402

W = rs.W
H0, T0 = rs.KNOWN
SPANS = [(300.0, 40.0), (250.0, -30.0)]


def span(S, h, H, thermal):
    """The span at H: its unstressed length (at T0), load per metre, and
    the wire's tension at its two ends."""
    L, _, q, k = cos.conserved(S, h, H, W, 0.0, thermal)
    C, xm = cos.catenary(S, h, H, q)
    return L / k, q, H * math.cosh(xm / C), H * math.cosh((S - xm) / C)


def solve(L0, t):
    thermal = math.exp(cos.ALPHA * (t - T0))

    def residual(x):
        h1, h2 = math.exp(x[0]), math.exp(x[1])
        l1, _, _, t1 = span(*SPANS[0], h1, thermal)
        l2, _, t2, _ = span(*SPANS[1], h2, thermal)
        return [(t1 - t2) / H0, (l1 + l2 - L0) / L0]

    x = [math.log(H0), math.log(H0)]
    for _ in range(50):  # fixed iteration cap (exit clause)
        r = residual(x)
        if max(abs(v) for v in r) < 1e-15:
            break
        e = 1e-7
        jac = [[0.0, 0.0], [0.0, 0.0]]
        for j in range(2):
            up = list(x)
            dn = list(x)
            up[j] += e
            dn[j] -= e
            ru, rd = residual(up), residual(dn)
            for i in range(2):
                jac[i][j] = (ru[i] - rd[i]) / (2 * e)
        det = jac[0][0] * jac[1][1] - jac[0][1] * jac[1][0]
        dx0 = (r[0] * jac[1][1] - r[1] * jac[0][1]) / det
        dx1 = (jac[0][0] * r[1] - jac[1][0] * r[0]) / det
        x = [x[0] - dx0, x[1] - dx1]
    else:
        raise RuntimeError("Newton did not converge")
    hs = [math.exp(v) for v in x]
    out = []
    for (S, h), H in zip(SPANS, hs):
        l0, q, tl, tr = span(S, h, H, thermal)
        out.append({"H": H, "L0": l0, "q": q, "tl": tl, "tr": tr, "S": S, "h": h})
    return out


def run(t_sag=15.0, t_clip=35.0):
    L0 = sum(rs.total_unstressed([s], H0, 1.0) for s in SPANS)
    sol = solve(L0, t_sag)
    out = {}
    for i, s in enumerate(sol):
        out[f"h_{i}_n"] = s["H"]
        out[f"sag_{i}_m"] = rs.sag(s["S"], s["h"], s["H"], s["q"])
        out[f"dyno_left_{i}_n"] = s["tl"]
        out[f"dyno_right_{i}_n"] = s["tr"]
    clip = solve(L0, t_clip)
    acc = 0.0
    for i, (a, b) in enumerate(zip(sol, clip)):
        acc += a["L0"] - b["L0"]
        out[f"clip_offset_{i}_m"] = acc
    out["h_difference_n"] = sol[1]["H"] - sol[0]["H"]
    return out


if __name__ == "__main__":
    cos.length_and_tension_integral.__defaults__ = (4_000,)
    print("sag-004")
    for key, value in run().items():
        print(f"  {key:26s} {value:.10g}")
