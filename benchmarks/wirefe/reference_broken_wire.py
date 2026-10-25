# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

"""Independent reference values for benchmarks/wirefe/broken_wire.toml and
benchmarks/wirefe/local_wind.toml.

1. A broken wire (roadmap O9). The tension section of span_cases.toml: two
level 300 m spans, dead-ends with their clamps fixed at (0, 27) and
(600, 27) m, a suspension string in the middle hanging from (300, 30) m
(3 m, 800 N, axial stiffness 5e7 N, tension only, its weight half on each
end). Cable EA = 34.64 MN, alpha = 18.9e-6 /K, 1.6285 kg/m (g = 9.80665) as
its data sheet gives it at 20 deg C; per metre of unstressed cable at the
stringing temperature, 15 deg C, w exp(-alpha (15 - 20)).

Stringing: 28 kN horizontal in both spans at 15 deg C, bare, the string
plumb and stretched by the clamp's load; this fixes each span's unstressed
length at 15 deg C (equal by symmetry: a section in the blocks rolled in
still air keeps them so).

The wire of span 2 breaks: span 2 carries nothing; span 1 keeps its
unstressed length (times exp(alpha (t - 15)), its weight per metre times
exp(-alpha (t - 15)): the same weight). The clamp is free, held by the
string alone: its position is found by Newton on its two coordinates until
span 1's end force, the string's pull and half the string's weight balance
(the moment about the string's top, by hand: tan(phi) = H / (V + W/2),
V the wire's vertical pull at the clamp). Span 1 is the closed-form
elastic catenary (Irvine) between its clamps, solved for its end forces by
Newton (an implementation of its own, not the Rust element's code):

    x(L) = H L / EA + (H / w) [asinh((V + w L) / H) - asinh(V / H)]
    z(L) = (V L + w L^2 / 2) / EA + (1 / w) [T(L) - T(0)]

The load on the middle structure (the string's top): the string's pull
towards the clamp and half its weight.

2. A local wind (roadmap O15). One level 300 m span between wire points
fixed at (0, 27) and (300, 27) m, clipped in with an unstressed length of
300.5 m at 20 deg C (the same cable, its data sheet weight w at 20 deg C),
in wind p from the right at 20 deg C with the span's local wind factor f:
the wind per metre of the stretched wire is f p d (d = 28 mm), per unit
unstressed length f p d k with the span's stretch
k = 1 + (1 / (EA L)) int T ds (a fixed point). The load is constant along
the span in a plane tilted by atan(f p d k / w) from the vertical: the
elastic catenary in that plane with the resultant load
sqrt(w^2 + (f p d k)^2), its chord level and across the load. Outputs: H
(the tension across the load), the blowout angle, the sag at mid-span in
the plane (the mid-point's distance off the chord).

Run: python3 -I benchmarks/wirefe/reference_broken_wire.py  (standard library only)
"""

import math

EA = 34.64e6
ALPHA = 18.9e-6
W_DATA_SHEET = 1.6285 * 9.80665  # at 20 deg C
W = W_DATA_SHEET * math.exp(-ALPHA * (15.0 - 20.0))  # at stringing, 15 deg C
H_STRING = 28_000.0
T_STRING = 15.0
ATTACH = (300.0, 30.0)
STRING = (3.0, 800.0, 5e7)  # length, weight, stiffness
ENDS = ((0.0, 27.0), (600.0, 27.0))
DIAMETER = 0.028


def ends(H, V, L, w):
    """Chord (dx, dz) of a span with horizontal tension H, start vertical V."""
    Te = math.hypot(H, V + w * L)
    T0 = math.hypot(H, V)
    dx = H * L / EA + H / w * (math.asinh((V + w * L) / H) - math.asinh(V / H))
    dz = (V * L + w * L * L / 2) / EA + (Te - T0) / w
    return dx, dz


def newton2(f, x, scale, cap=200, positive=False, tol=1e-11):
    """Solves f(x) = 0 in two unknowns (finite differences, fixed cap); the
    step halved while it does not lower the residual (or, `positive`, while
    it would make x[0] not positive)."""
    for _ in range(cap):
        r = f(x)
        size = math.hypot(*r)
        if size < tol:
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
        step = [(d * r[0] - b * r[1]) / det, (-c * r[0] + a * r[1]) / det]
        t = 1.0
        for _ in range(60):
            y = [x[0] - t * step[0], x[1] - t * step[1]]
            if not (positive and y[0] <= 0.0):
                try:
                    if math.hypot(*f(y)) < size:
                        break
                except (ValueError, ZeroDivisionError, OverflowError):
                    pass
            t /= 2
        x = y
    else:
        raise RuntimeError("Newton did not converge")
    return x


def tension_integral(H, V, L, w):
    """int_0^L T ds for T = sqrt(H^2 + (V + w s)^2)."""
    g = lambda v: 0.5 * (v * math.hypot(H, v) + H * H * math.asinh(v / H))
    return (g(V + w * L) - g(V)) / w


def span_forces(L, w, chord, guess=20_000.0):
    """(H, V at start, Tz at end) of a span of unstressed length L under w
    per unstressed metre on chord (dx, dz), Newton from H = guess."""
    H, V = newton2(
        lambda p: [ends(p[0], p[1], L, w)[0] - chord[0], ends(p[0], p[1], L, w)[1] - chord[1]],
        [guess, -w * L / 2],
        [1e-3, 1e-3],
        positive=True,
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


def broken(lengths, t):
    """Span 2 broken at temperature t (still air): span 1's H, the clamp,
    the string's swing (negative: towards span 1) and force, the load on
    the string's top along the line and down."""
    length, weight, k = STRING
    L = lengths[0] * math.exp(ALPHA * (t - T_STRING))
    w = W * math.exp(-ALPHA * (t - T_STRING))

    def parts(c):
        h0, _, tz0 = span_forces(L, w, (c[0] - ENDS[0][0], c[1] - ENDS[0][1]), 10_000.0)
        ax, az = ATTACH[0] - c[0], ATTACH[1] - c[1]
        d = math.hypot(ax, az)
        tension = max(0.0, k * (d / length - 1))
        return h0, tz0, tension, ax / d, az / d

    def residual(c):
        h0, tz0, tension, ux, uz = parts(c)
        return [-h0 + tension * ux, -tz0 + tension * uz - weight / 2]

    # From the clamp swung well back (from plumb, Newton would first throw
    # it where the wire's whole pull takes it).
    # (The residual in N, of span forces each solved to 1e-11 m.)
    c = newton2(residual, [ATTACH[0] - 2.8, ATTACH[1] - 0.9], [1e-6, 1e-6], tol=1e-7)
    h0, tz0, tension, ux, uz = parts(c)
    swing = math.degrees(math.atan2(c[0] - ATTACH[0], ATTACH[1] - c[1]))
    # The hand check: the moment about the top.
    hand = math.degrees(math.atan(h0 / (tz0 + weight / 2)))
    return {
        "H": h0,
        "clamp_x": c[0],
        "clamp_z": c[1],
        "swing": swing,
        "swing_by_hand": -hand,
        "string": tension,
        "along": -tension * ux,
        "down": tension * uz + weight / 2,
    }


def local_wind(p, f):
    """One clipped span (300.5 m at 20 deg C, fixed ends 300 m apart,
    level) in wind p from the right with local factor f."""
    L, w = 300.5, W_DATA_SHEET
    q = f * p * DIAMETER
    k = 1.0
    for _ in range(200):
        r = math.hypot(w, q * k)
        H, V, _ = span_forces(L, r, (300.0, 0.0))
        k_new = 1 + tension_integral(H, V, L, r) / EA / L
        if abs(k_new - k) <= 1e-15:
            break
        k = k_new
    else:
        raise RuntimeError("stretch did not settle")
    r = math.hypot(w, q * k)
    H, V, _ = span_forces(L, r, (300.0, 0.0))
    _, dz = ends(H, V, L / 2, r)
    return {
        "H": H,
        "blowout": math.degrees(math.atan2(q * k, w)),
        "sag": -dz,
        "stretch": k,
    }


if __name__ == "__main__":
    lengths = strung(W)
    print(f"unstressed lengths at 15 C: {lengths[0]:.10f} {lengths[1]:.10f}")
    for name, t in (("brokenwire-001 span 2 broken at 15 C", 15.0),
                    ("brokenwire-002 span 2 broken at -5 C", -5.0)):
        r = broken(lengths, t)
        print(name)
        for key, value in r.items():
            print(f"  {key:14s} {value:.12g}")
    print("brokenwire-003 span 1 broken at 15 C: the mirror image of 001 "
          "(clamp_x = 600 - x, swing and along with the sign changed)")
    for name, p, f in (("localwind-001 factor 1", 500.0, 1.0),
                       ("localwind-002 factor 1.5", 500.0, 1.5),
                       ("localwind-003 factor 0.6", 500.0, 0.6),
                       ("localwind-004 factor 0 (no wind on the span)", 500.0, 0.0)):
        r = local_wind(p, f)
        print(name)
        for key, value in r.items():
            print(f"  {key:14s} {value:.12g}")
