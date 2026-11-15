# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

"""Independent reference values for benchmarks/wirefe/flexible_support.toml.

A level 300 m span between wire points fixed at (0, 27) and (300, 27) m,
the cable of span_cases.toml (EA = 34.64 MN, alpha = 18.9e-6 /K,
1.6285 kg/m with g = 9.80665 at its data sheet's 20 deg C; per metre of
unstressed cable at the stringing temperature, 15 deg C,
w exp(-alpha (15 - 20))), strung at 28 kN horizontal at 15 deg C. The
second support stands on a spring of stiffness k along the span
(calculation level 3: the structure's flexibility 1/k along the line,
nothing across or vertically), measured from the structure as strung: at
the stringing state the spring carries the stringing pull, 28 kN.

Stringing: the unstressed length L0 at 15 deg C from the level elastic
catenary (Irvine)

    L = (2 H / w) asinh(w L0 / (2 H)) + H L0 / EA

with L = 300 m and H = 28 kN (bisection).

A case at temperature t in still air: the unstressed length
L0 exp(alpha (t - 15)), the same total weight (w scaled by the inverse).
The support moves along the line by u = (F - F_ref) / k with F = -H the
wire's pull on it (towards the span) and F_ref = -28 kN: the chord is
300 + u. H solves L(H) = 300 - (H - 28000) / k (bisection; L(H) grows with
H, the right side falls: one root). Outputs: H and u.

Run: python3 -I benchmarks/wirefe/reference_flexible_support.py  (standard library only)
"""

import math

EA = 34.64e6
ALPHA = 18.9e-6
W_DATA_SHEET = 1.6285 * 9.80665  # at 20 deg C
W15 = W_DATA_SHEET * math.exp(-ALPHA * (15.0 - 20.0))
SPAN = 300.0
H0 = 28_000.0


def chord(h, w, l0):
    """The level elastic catenary's chord for horizontal tension h."""
    return 2.0 * h / w * math.asinh(w * l0 / (2.0 * h)) + h * l0 / EA


def bisect(f, lo, hi):
    """The root of an increasing f between lo and hi."""
    for _ in range(300):
        mid = 0.5 * (lo + hi)
        if f(mid) > 0.0:
            hi = mid
        else:
            lo = mid
    return 0.5 * (lo + hi)


# The unstressed length at stringing (at a fixed H and weight per
# unstressed metre the chord grows with L0).
L0_15 = bisect(lambda l0: chord(H0, W15, l0) - SPAN, SPAN * 0.99, SPAN * 1.01)


def case(t, k):
    l0 = L0_15 * math.exp(ALPHA * (t - 15.0))
    w = W15 * L0_15 / l0
    h = bisect(lambda h: chord(h, w, l0) - (SPAN - (h - H0) / k), 100.0, 1e6)
    return h, -(h - H0) / k


if __name__ == "__main__":
    print(f"L0 at 15 deg C: {L0_15:.10f} m")
    for t, k in [(-20.0, 1e5), (50.0, 1e5), (-20.0, 7e3), (15.0, 1e5)]:
        h, u = case(t, k)
        print(f"t = {t:6.1f} deg C, k = {k:9.0f} N/m: H = {h:.9f} N, u = {u:.12f} m")
