# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

"""Independent reference values for benchmarks/wirefe/span_cases.toml.

A tension section of two level 300 m spans: dead-ends with their clamps at
(0, 27) and (600, 27) m, a suspension string in the middle hanging from
(300, 30) m (3 m, 800 N, axial stiffness 5e7 N, tension only, its weight
half on the clamp). Cable EA = 34.64 MN, alpha = 18.9e-6 /K, 1.6285 kg/m (g = 9.80665).

Stringing: 28 kN horizontal in both spans at 15 deg C, bare, the string
plumb and stretched by the clamp's load (fixed point, converged); this
fixes each span's unstressed length at 15 deg C.

A state: each span its own temperature and load per metre (its unstressed
length times exp(alpha (t - 15))). The cable's own weight is conserved
(owner decision 2026-10-21): per metre of that longer unstressed length it
is w exp(-alpha (t - 15)), so a span weighs w L0 hot or cold; ice is per
metre of the real cable and is not scaled. The clamp's position is found by Newton
on its two coordinates (finite differences, fixed cap) until the forces on
it balance: the two spans' end tensions, the string's pull and half its
weight. Each span is the closed-form elastic catenary (Irvine) between its
clamps, solved for its end forces by Newton (an implementation of its own,
not the Rust element's code):

    x(L) = H L / EA + (H / w) [asinh((V + w L) / H) - asinh(V / H)]
    z(L) = (V L + w L^2 / 2) / EA + (1 / w) [T(L) - T(0)]

with L the stretched-free length, V the vertical tension component at the
span's start (along the cable), T = sqrt(H^2 + Tz^2).

Run: python3 benchmarks/wirefe/reference_span_cases.py  (standard library only)
"""

import math

EA = 34.64e6
ALPHA = 18.9e-6
W = 1.6285 * 9.80665
H_STRING = 28_000.0
T_STRING = 15.0
ATTACH = (300.0, 30.0)
STRING = (3.0, 800.0, 5e7)  # length, weight, stiffness
ENDS = ((0.0, 27.0), (600.0, 27.0))


def ends(H, V, L, w):
    """Chord (dx, dz) of a span with horizontal tension H, start vertical V."""
    Te = math.hypot(H, V + w * L)
    T0 = math.hypot(H, V)
    dx = H * L / EA + H / w * (math.asinh((V + w * L) / H) - math.asinh(V / H))
    dz = (V * L + w * L * L / 2) / EA + (Te - T0) / w
    return dx, dz


def newton2(f, x, scale, cap=60):
    """Solves f(x) = 0 in two unknowns (finite differences, fixed cap)."""
    for _ in range(cap):
        r = f(x)
        if math.hypot(*r) < 1e-11:
            break
        J = []
        for k in range(2):
            y = list(x)
            y[k] += scale[k]
            ry = f(y)
            J.append(((ry[0] - r[0]) / scale[k], (ry[1] - r[1]) / scale[k]))
        a, c = J[0]
        b, d = J[1]
        det = a * d - b * c
        x = [x[0] - (d * r[0] - b * r[1]) / det, x[1] - (-c * r[0] + a * r[1]) / det]
    return x


def span_forces(L, w, chord):
    """(H, V at start, Tz at end) of a span of length L under w on chord."""
    H, V = newton2(
        lambda p: [ends(p[0], p[1], L, w)[0] - chord[0], ends(p[0], p[1], L, w)[1] - chord[1]],
        [25_000.0, -w * L / 2],
        [1e-3, 1e-3],
    )
    return H, V, V + w * L


def length_for(H, chord, w):
    """Unstressed length and start vertical giving H on chord."""
    L, V = newton2(
        lambda p: [ends(H, p[1], p[0], w)[0] - chord[0], ends(H, p[1], p[0], w)[1] - chord[1]],
        [math.hypot(*chord), -w * math.hypot(*chord) / 2],
        [1e-6, 1e-3],
    )
    return L


def strung(w):
    """Unstressed lengths at stringing (clamp plumb, string stretched)."""
    length, weight, k = STRING
    cz = ATTACH[1] - length
    for _ in range(20):  # converges in 3
        clamp = (ATTACH[0], cz)
        lengths = [
            length_for(H_STRING, (clamp[0] - ENDS[0][0], clamp[1] - ENDS[0][1]), w),
            length_for(H_STRING, (ENDS[1][0] - clamp[0], ENDS[1][1] - clamp[1]), w),
        ]
        _, _, tz0 = span_forces(lengths[0], w, (clamp[0] - ENDS[0][0], clamp[1] - ENDS[0][1]))
        _, v1, _ = span_forces(lengths[1], w, (ENDS[1][0] - clamp[0], ENDS[1][1] - clamp[1]))
        force = tz0 - v1 + weight / 2
        cz = ATTACH[1] - length * (1 + force / k)
    return lengths


def state(lengths, spans):
    """spans = [(temperature, own weight, ice)] per span, N/m: H of each,
    clamp, swing, string force."""
    length, weight, k = STRING
    L = [l * math.exp(ALPHA * (t - T_STRING)) for l, (t, _, _) in zip(lengths, spans)]
    w = [own * math.exp(-ALPHA * (t - T_STRING)) + ice for t, own, ice in spans]

    def parts(c):
        h0, _, tz0 = span_forces(L[0], w[0], (c[0] - ENDS[0][0], c[1] - ENDS[0][1]))
        h1, v1, _ = span_forces(L[1], w[1], (ENDS[1][0] - c[0], ENDS[1][1] - c[1]))
        ax, az = ATTACH[0] - c[0], ATTACH[1] - c[1]
        d = math.hypot(ax, az)
        t = max(0.0, k * (d / length - 1))
        return h0, tz0, h1, v1, t, ax / d, az / d

    def residual(c):
        h0, tz0, h1, v1, t, ux, uz = parts(c)
        return [-h0 + h1 + t * ux, -tz0 + v1 + t * uz - weight / 2]

    c = newton2(residual, [ATTACH[0], ATTACH[1] - length], [1e-6, 1e-6])
    h0, _, h1, _, t, _, _ = parts(c)
    swing = math.degrees(math.atan2(c[0] - ATTACH[0], ATTACH[1] - c[1]))
    return {"H0": h0, "H1": h1, "clamp_x": c[0], "clamp_z": c[1], "swing": swing, "string": t}


if __name__ == "__main__":
    # The validation cases (1.6285 kg/m), then the unit test of
    # ohl_wirefe::section (15.97 N/m, the same geometry).
    for w, label in ((W, "span_cases.toml"), (15.97, "ohl_wirefe::section test, 15.97 N/m")):
        lengths = strung(w)
        print(f"{label}: unstressed lengths at 15 C: {lengths[0]:.10f} {lengths[1]:.10f}")
        cases = {
            "spancases-001 both bare at -5 C": [(-5.0, w, 0.0), (-5.0, w, 0.0)],
            "spancases-002 span 0 iced (+10 N/m) at -5 C, span 1 bare at -5 C": [(-5.0, w, 10.0), (-5.0, w, 0.0)],
            "spancases-003 span 0 at 60 C, span 1 at 15 C (bare)": [(60.0, w, 0.0), (15.0, w, 0.0)],
        }
        for name, spans in cases.items():
            r = state(lengths, spans)
            print(name)
            for key, value in r.items():
                print(f"  {key:8s} {value:.10g}")
