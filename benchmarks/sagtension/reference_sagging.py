# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

"""Independent reference values for benchmarks/sagtension/sagging.toml.

The section model is reference_change_of_state.py's (numerical integrals,
the own weight conserved, a secant root on log H; EA = 34.64 MN, alpha =
18.9e-6 /K), solved for one H through the section. Then, with numerics
different from crates/ohl-sagtension/src/sagging.rs:
- stopwatch: the travel time of a wave, integral of ds / c with
  c = sqrt(T g / q), by 64-point Gauss-Legendre quadrature on 8 panels in
  the arc-length parameter u = asinh(y') (the code: Simpson in x);
- transit: the tangent point by Newton on the tangent condition (the code:
  bisection), the catenary in the textbook cosh form;
- level sag: C (cosh(S / 2C) - 1); inclined correction: the sag where the
  wire is parallel to the chord minus it;
- sag per kelvin: central difference of two solves at t -/+ 0.5 K;
- clipping offsets: the cumulative difference of each span's unstressed
  length L / k at the sagging and at the clipping temperature.

Run: python3 -I benchmarks/sagtension/reference_sagging.py  (standard library only)
"""

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import reference_change_of_state as cos  # noqa: E402

G = 9.80665
W = 15.97  # N/m own weight
KNOWN = (28_000.0, 15.0)  # H, temperature (C)


def gauss_legendre(n):
    """Nodes and weights on [-1, 1] by Newton on the Legendre polynomial."""
    nodes, weights = [], []
    for i in range(1, n + 1):
        x = math.cos(math.pi * (i - 0.25) / (n + 0.5))
        for _ in range(100):
            p0, p1 = 1.0, x
            for k in range(2, n + 1):
                p0, p1 = p1, ((2 * k - 1) * x * p1 - (k - 1) * p0) / k
            dp = n * (x * p1 - p0) / (x * x - 1)
            dx = p1 / dp
            x -= dx
            if abs(dx) < 1e-16:
                break
        nodes.append(x)
        weights.append(2 / ((1 - x * x) * dp * dp))
    return nodes, weights


GL = gauss_legendre(64)


def integrate(f, a, b, panels=8):
    nodes, weights = GL
    total = 0.0
    h = (b - a) / panels
    for p in range(panels):
        lo = a + p * h
        for x, w in zip(nodes, weights):
            total += w * f(lo + (x + 1) * h / 2) * h / 2
    return total


def total_unstressed(spans, H, thermal):
    return sum(L / k for L, _, _, k in (cos.conserved(S, h, H, W, 0.0, thermal) for S, h in spans))


def solve(spans, L0, t):
    thermal = math.exp(cos.ALPHA * (t - KNOWN[1]))
    f = lambda lh: total_unstressed(spans, math.exp(lh), thermal) - L0
    a, b = math.log(10_000.0), math.log(40_000.0)
    fa, fb = f(a), f(b)
    for _ in range(60):
        if fb == fa:
            break
        c = b - fb * (b - a) / (fb - fa)
        a, fa, b, fb = b, fb, c, f(c)
        if abs(b - a) < 1e-14:
            break
    H = math.exp(b)
    out = []
    for S, h in spans:
        L, _, q, k = cos.conserved(S, h, H, W, 0.0, thermal)
        out.append({"S": S, "h": h, "q": q, "L0": L / k})
    return H, out


def geometry(S, h, H, q):
    C, xm = cos.catenary(S, h, H, q)
    y = lambda x: C * (math.cosh((x - xm) / C) - math.cosh(xm / C))
    dy = lambda x: math.sinh((x - xm) / C)
    return C, xm, y, dy


def sag(S, h, H, q):
    C, xm, y, _ = geometry(S, h, H, q)
    x = xm + C * math.asinh(h / S)
    return h * x / S - y(x)


def stopwatch(S, h, H, q, waves):
    # In u = (x - xm)/C: ds = C cosh u du, T = H cosh u, dt = ds / sqrt(T g / q).
    C, xm, _, _ = geometry(S, h, H, q)
    f = lambda u: C * math.cosh(u) / math.sqrt(H * math.cosh(u) * G / q)
    return 2 * waves * integrate(f, -xm / C, (S - xm) / C)


def transit(S, h, H, q, xi, zi, xt):
    C, xm, y, dy = geometry(S, h, H, q)
    d2y = lambda x: math.cosh((x - xm) / C) / C
    f = lambda x: y(x) + dy(x) * (xi - x) - zi
    x = S / 2
    for _ in range(100):  # Newton: f' = y''(x) (xi - x)
        step = f(x) / (d2y(x) * (xi - x))
        x -= step
        if abs(step) < 1e-13:
            break
    slope = dy(x)
    return math.degrees(math.atan(slope)), x, zi + slope * (xt - xi)


def run(spans, t_sag, waves=1, transit_at=None, t_clip=None):
    L0 = total_unstressed(spans, KNOWN[0], 1.0)
    H, sol = solve(spans, L0, t_sag)
    out = {"h_n": H}
    _, cold = solve(spans, L0, t_sag - 0.5)
    _, warm = solve(spans, L0, t_sag + 0.5)
    Hc, _ = solve(spans, L0, t_sag - 0.5)
    Hw, _ = solve(spans, L0, t_sag + 0.5)
    for i, s in enumerate(sol):
        S, h, q = s["S"], s["h"], s["q"]
        C = H / q
        level = C * (math.cosh(S / (2 * C)) - 1)
        out[f"sag_{i}_m"] = sag(S, h, H, q)
        out[f"level_sag_{i}_m"] = level
        out[f"inclined_correction_{i}_m"] = out[f"sag_{i}_m"] - level
        out[f"sag_per_k_{i}_m"] = (sag(S, h, Hw, warm[i]["q"]) - sag(S, h, Hc, cold[i]["q"])) / 1.0
        out[f"stopwatch_{i}_s"] = stopwatch(S, h, H, q, waves)
        _, xm, _, _ = geometry(S, h, H, q)
        out[f"dyno_left_{i}_n"] = H * math.cosh(xm / C)
        out[f"dyno_right_{i}_n"] = H * math.cosh((S - xm) / C)
    if transit_at:
        k, xi, zi, xt = transit_at
        s = sol[k - 1]
        a, x, z = transit(s["S"], s["h"], H, s["q"], xi, zi, xt)
        out["transit_angle_deg"], out["transit_tangent_x_m"], out["transit_target_m"] = a, x, z
    if t_clip is not None:
        _, clip = solve(spans, L0, t_clip)
        acc = 0.0
        for i, (a, b) in enumerate(zip(sol, clip)):
            acc += a["L0"] - b["L0"]
            out[f"clip_offset_{i}_m"] = acc
    return out


CASES = {
    "sag-001": dict(spans=[(300.0, 0.0)], t_sag=15.0, waves=3, transit_at=(1, 0.0, -3.0, 300.0)),
    "sag-002": dict(spans=[(300.0, 50.0)], t_sag=15.0, waves=1, transit_at=(1, 0.0, -3.0, 300.0)),
    "sag-003": dict(spans=[(200.0, 30.0), (350.0, -10.0), (250.0, 20.0)], t_sag=15.0, waves=1, t_clip=35.0),
}

if __name__ == "__main__":
    cos.length_and_tension_integral.__defaults__ = (4_000,)
    for name, kw in CASES.items():
        print(name)
        for key, value in run(**kw).items():
            print(f"  {key:26s} {value:.10g}")
