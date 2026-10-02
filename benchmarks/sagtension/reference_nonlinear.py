# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

"""Independent reference values for benchmarks/sagtension/nonlinear.toml.

Re-implements the composite non-linear cable model from its definition
(crates/ols-cables/src/mechanics.rs docs) with different numerics:
- strain from tension by the Illinois (regula falsi) method, not bisection;
- span length and mean tension by Simpson integration of the textbook
  catenary, not closed forms;
- section tension by the secant method on log H.

Cable: the SYNTHETIC test cable (invented curves, not a real conductor):
outer 400 mm², alpha 23e-6, initial 300 e - 150 e^2 MPa (e in %), creep
200 e - 100 e^2, final 550 MPa/%, no compression; core 65 mm², alpha 11.5e-6,
initial 1800 e - 100 e^2, final 1900 MPa/%. Curves at 20 C.

Run: python3 benchmarks/sagtension/reference_nonlinear.py  (standard library only)
"""

import math

T_REF = 20.0
W = 1.6 * 9.80665  # N/m
COMPONENTS = [
    # area m², alpha, initial [MPa per %^k], creep, final MPa/%, compression limited
    (400e-6, 23.0e-6, [0.0, 300.0, -150.0], [0.0, 200.0, -100.0], 550.0, True),
    (65e-6, 11.5e-6, [0.0, 1800.0, -100.0], None, 1900.0, False),
]


def load(comp, e, state, perm=None):
    area, _, initial, creep, final, limited = comp
    p = e * 100.0  # percent
    poly = lambda c: sum(a * p**k for k, a in enumerate(c)) * 1e6 * area
    line = lambda e0: final * 1e6 * area * (e - e0) * 100.0
    if state == "initial":
        v = poly(initial) if e >= 0 else line(0.0)
    elif state == "creep":
        v = poly(creep or initial) if e >= 0 else line(0.0)
    else:
        v = min(line(perm), poly(initial)) if e >= 0 else line(perm)
    return max(v, 0.0) if limited else v


def tension(e, t, state, perms=(0.0, 0.0)):
    return sum(load(c, e - c[1] * (t - T_REF), state, perms[k]) for k, c in enumerate(COMPONENTS))


def strain(T, t, state, perms=(0.0, 0.0)):
    f = lambda e: tension(e, t, state, perms) - T
    a, b = -0.01, 0.02
    fa, fb = f(a), f(b)
    side = 0
    for _ in range(300):  # Illinois method, fixed cap
        c = (a * fb - b * fa) / (fb - fa)
        fc = f(c)
        if abs(fc) < 1e-9 or abs(b - a) < 1e-16:
            return c
        if fc * fb > 0:
            b, fb = c, fc
            if side == -1:
                fa /= 2
            side = -1
        else:
            a, fa = c, fc
            if side == 1:
                fb /= 2
            side = 1
    return c


def permanent(T, t, state):
    e = strain(T, t, state)
    perms = []
    for c in COMPONENTS:
        em = e - c[1] * (t - T_REF)
        perms.append(max(em - load(c, em, state) / (c[4] * 1e6 * c[0] * 100.0), 0.0))
    return tuple(perms)


def span(S, H, w, n=4000):
    """Length and mean tension of a level span (Simpson)."""
    C = H / w
    xm = S / 2
    f_len = lambda x: math.cosh((x - xm) / C)
    f_t = lambda x: H * math.cosh((x - xm) / C) ** 2
    h = S / n
    simpson = lambda f: (f(0) + f(S) + sum((4 if i % 2 else 2) * f(i * h) for i in range(1, n))) * h / 3
    L = simpson(f_len)
    return L, simpson(f_t) / L


def sag(S, H, w):
    C = H / w
    return C * (math.cosh(S / (2 * C)) - 1)


def unstrained(S, H, w, t, state, perms=(0.0, 0.0)):
    L, Tm = span(S, H, w)
    return L / (1 + strain(Tm, t, state, perms))


def solve(S, L0, w, t, state, perms=(0.0, 0.0)):
    f = lambda lh: unstrained(S, math.exp(lh), w, t, state, perms) - L0
    a, b = math.log(10_000.0), math.log(40_000.0)
    fa, fb = f(a), f(b)
    for _ in range(60):
        if fb == fa:
            break
        c = b - fb * (b - a) / (fb - fa)
        a, fa, b, fb = b, fb, c, f(c)
        if abs(b - a) < 1e-13:
            break
    return math.exp(b)


if __name__ == "__main__":
    S = 300.0
    L0 = unstrained(S, 25_000.0, W, 15.0, "initial")
    # Final condition: after creep at 15 C bare vs after max load -10 C, 30 N/m.
    h_creep = solve(S, L0, W, 15.0, "creep")
    perm_creep = permanent(span(S, h_creep, W)[1], 15.0, "creep")
    h_load = solve(S, L0, 30.0, -10.0, "initial")
    perm_load = permanent(span(S, h_load, 30.0)[1], -10.0, "initial")
    e_creep, e_load = strain(0.0, T_REF, "final", perm_creep), strain(0.0, T_REF, "final", perm_load)
    perms, cause = (perm_creep, "creep") if e_creep >= e_load else (perm_load, "load")
    print(f"final governed by {cause}: permanent {perms}")
    for label, w, t in [("75 C bare", W, 75.0), ("-10 C, 30 N/m", 30.0, -10.0)]:
        hi = solve(S, L0, w, t, "initial")
        hf = solve(S, L0, w, t, "final", perms)
        print(f"{label}: H initial {hi:.10g}  H final {hf:.10g}  sag initial {sag(S, hi, w):.10g}  sag final {sag(S, hf, w):.10g}")
