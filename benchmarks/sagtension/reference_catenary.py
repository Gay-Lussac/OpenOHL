# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

"""Independent reference values for benchmarks/sagtension/catenary.toml.

Deliberately different from the Rust implementation:
- textbook form y(x) = C [cosh((x - xm)/C) - cosh(xm/C)] (no product form);
- maximum sag found by brute-force sampling (1e6 points + local refinement),
  not by the "slope equals chord slope" formula;
- cable length by numerical integration (Simpson) of sqrt(1 + y'^2);
- support tensions from T = H + w (y - y_low).

Run: python3 benchmarks/sagtension/reference_catenary.py  (standard library only)
"""

import math

CASES = [
    # name, span S (m), rise h (m), horizontal tension H (N), weight w (N/m)
    ("level 300 m", 300.0, 0.0, 28_000.0, 15.97),
    ("inclined 450 m, +35 m", 450.0, 35.0, 22_000.0, 15.97),
    ("steep 200 m, +120 m", 200.0, 120.0, 10_000.0, 15.97),
]


def solve(S, h, H, w):
    C = H / w
    xm = S / 2 - C * math.asinh(h / (2 * C * math.sinh(S / (2 * C))))
    y = lambda x: C * (math.cosh((x - xm) / C) - math.cosh(xm / C))
    sag_at = lambda x: h * x / S - y(x)

    n = 1_000_000
    best = max(range(n + 1), key=lambda i: sag_at(S * i / n))
    lo, hi = S * max(best - 1, 0) / n, S * min(best + 1, n) / n
    for _ in range(200):  # golden-section refinement, fixed iteration count
        a, b = lo + (hi - lo) * 0.382, lo + (hi - lo) * 0.618
        if sag_at(a) > sag_at(b):
            hi = b
        else:
            lo = a
    x_sag = (lo + hi) / 2

    m = 200_000  # Simpson, even number of intervals
    f = lambda x: math.sqrt(1 + math.sinh((x - xm) / C) ** 2)
    step = S / m
    length = f(0) + f(S) + sum((4 if i % 2 else 2) * f(i * step) for i in range(1, m))
    length *= step / 3

    y_low = y(xm)
    return {
        "sag": sag_at(x_sag),
        "midspan_sag": sag_at(S / 2),
        "length": length,
        "tension_left": H + w * (y(0) - y_low),
        "tension_right": H + w * (y(S) - y_low),
        "low_point_x": xm,
    }


if __name__ == "__main__":
    for name, S, h, H, w in CASES:
        print(name)
        for key, value in solve(S, h, H, w).items():
            print(f"  {key:14s} {value:.10g}")
