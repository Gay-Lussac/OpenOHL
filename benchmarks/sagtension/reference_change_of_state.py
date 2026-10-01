# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

"""Independent reference values for benchmarks/sagtension/change_of_state.toml.

Model (see crates/ols-sagtension/src/change_of_state.rs):
    L = L0 * exp(alpha (t - t0)) * (1 + Tavg / EA),  Tavg = (1/L) * integral of T ds

Deliberately different from the Rust implementation:
- the tension integral is computed numerically (Simpson on T(x) ds/dx), not
  with the closed form;
- the catenary uses the textbook cosh-difference form;
- the root is found with the secant method on log(H), not bisection.

Run: python3 benchmarks/sagtension/reference_change_of_state.py  (standard library only)
"""

import math

EA = 34.64e6      # N, Drake-like composite axial stiffness
ALPHA = 18.9e-6   # 1/K


def catenary(S, h, H, w):
    C = H / w
    xm = S / 2 - C * math.asinh(h / (2 * C * math.sinh(S / (2 * C))))
    return C, xm


def length_and_tension_integral(S, h, H, w, n=20_000):
    C, xm = catenary(S, h, H, w)
    ds = lambda x: math.cosh((x - xm) / C)          # ds/dx
    t_ds = lambda x: H * math.cosh((x - xm) / C) ** 2  # T ds/dx
    step = S / n
    def simpson(f):
        total = f(0) + f(S) + sum((4 if i % 2 else 2) * f(i * step) for i in range(1, n))
        return total * step / 3
    return simpson(ds), simpson(t_ds)


def unstressed_length(S, h, H, w):
    L, integral = length_and_tension_integral(S, h, H, w)
    return L / (1 + integral / L / EA)


def solve(S, h, L0, t0, w, t):
    def mismatch(log_h):
        H = math.exp(log_h)
        L, integral = length_and_tension_integral(S, h, H, w)
        return L - L0 * math.exp(ALPHA * (t - t0)) * (1 + integral / L / EA)

    a, b = math.log(10_000.0), math.log(40_000.0)
    fa, fb = mismatch(a), mismatch(b)
    for _ in range(50):  # fixed iteration cap (exit clause)
        if fb == fa:
            break
        c = b - fb * (b - a) / (fb - fa)
        a, fa, b, fb = b, fb, c, mismatch(c)
        if abs(b - a) < 1e-14:
            break
    H = math.exp(b)
    C, xm = catenary(S, h, H, w)
    y = lambda x: C * (math.cosh((x - xm) / C) - math.cosh(xm / C))
    x_sag = xm + C * math.asinh(h / S)
    L, _ = length_and_tension_integral(S, h, H, w)
    return {
        "horizontal_tension": H,
        "sag": h * x_sag / S - y(x_sag),
        "length": L,
        "tension_left": H * math.cosh(xm / C),
        "tension_right": H * math.cosh((S - xm) / C),
    }


CASES = [
    # name, S, h, known (H, w, t), new (w, t)
    ("level 300 m: 28 kN @ 15 C bare -> 75 C bare", 300.0, 0.0, (28_000.0, 15.97, 15.0), (15.97, 75.0)),
    ("level 300 m: 28 kN @ 15 C bare -> -20 C, 30 N/m", 300.0, 0.0, (28_000.0, 15.97, 15.0), (30.0, -20.0)),
    ("inclined 450 m +35 m: 22 kN @ 15 C -> 100 C", 450.0, 35.0, (22_000.0, 15.97, 15.0), (15.97, 100.0)),
]

if __name__ == "__main__":
    for name, S, h, (H0, w0, t0), (w, t) in CASES:
        L0 = unstressed_length(S, h, H0, w0)
        print(name)
        for key, value in solve(S, h, L0, t0, w, t).items():
            print(f"  {key:20s} {value:.10g}")
