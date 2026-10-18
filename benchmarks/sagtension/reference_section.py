# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

"""Independent reference values for benchmarks/sagtension/section.toml.

Tension section with equal horizontal tension in every span and a conserved
total unstressed length (see crates/ohl-sagtension/src/section.rs). Uses the
numerical integrals of reference_change_of_state.py and a secant root on
log(H) with a fixed iteration cap. Also prints the classic ruling span and
the tension obtained by treating the section as one level ruling span.

Run: python3 benchmarks/sagtension/reference_section.py  (standard library only)
"""

import math
import reference_change_of_state as cos

SPANS = [(220.0, 15.0), (310.0, -25.0), (380.0, 40.0), (260.0, -10.0)]  # (S, h) m
KNOWN = (28_000.0, 15.97, 15.0)  # H, load, temperature
TARGET = (15.97, 75.0)


def unstrained_total(H, w):
    total = 0.0
    for S, h in SPANS:
        L, integral = cos.length_and_tension_integral(S, h, H, w)
        total += L / (1 + integral / L / cos.EA)
    return total


def solve(L0, t0, w, t):
    target = L0 * math.exp(cos.ALPHA * (t - t0))
    f = lambda log_h: unstrained_total(math.exp(log_h), w) - target
    a, b = math.log(10_000.0), math.log(40_000.0)
    fa, fb = f(a), f(b)
    for _ in range(50):
        if fb == fa:
            break
        c = b - fb * (b - a) / (fb - fa)
        a, fa, b, fb = b, fb, c, f(c)
        if abs(b - a) < 1e-14:
            break
    return math.exp(b)


if __name__ == "__main__":
    cos.length_and_tension_integral.__defaults__ = (4_000,)
    H0, w0, t0 = KNOWN
    L0 = unstrained_total(H0, w0)
    H = solve(L0, t0, *TARGET)
    print(f"section horizontal tension  {H:.10g}")
    rs = math.sqrt(sum(S**3 for S, _ in SPANS) / sum(S for S, _ in SPANS))
    print(f"ruling span                 {rs:.10g}")
    L0_rs = cos.unstressed_length(rs, 0.0, H0, w0)
    H_rs = cos.solve(rs, 0.0, L0_rs, t0, *TARGET)["horizontal_tension"]
    print(f"ruling-span approximation   {H_rs:.10g}  ({(H_rs / H - 1) * 100:+.3f} %)")
