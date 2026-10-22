# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

"""Independent reference values for benchmarks/sagtension/crosscheck-drake.toml
and benchmarks/sagtension/nonlinear-criteria.toml.

The cable: a DRAKE-LIKE test cable. Its curves are taken from a
public-domain library's test data (source not stated there); they are test
input only, not data of CIGRE TB 324 or of any manufacturer.

Re-implements the composite non-linear cable model from its definition
(docs/theory/sagtension.md, "Non-linear stress-strain and creep") with
different numerics from the code:
- strain from tension by the Illinois (regula falsi) method, not bisection;
- the zero-tension strain (the length carrying the own weight) as the
  lowest zero-load strain of the components, found by Newton on each
  polynomial, not by bisection on the cable's tension;
- span length and mean tension by Simpson integration of the catenary;
- the section tension by the secant method on log H;
- the permanent set signed (e - P/EA_f), the final state
  min(EA_f (e - p), initial state), every component capped at zero
  compression;
- the own weight conserved: W per metre of the cable unstressed at the
  known temperature, the length L0 (1 + eps_w); a span of strain eps
  carries W (1 + eps_w) / (1 + eps) per metre (a fixed point), ice and wind
  as given;
- per case, the governing final state is the one with the lower H (the
  longer cable in that case), a tie (1e-9) to the load;
- tension criteria: for each, the largest stringing tension meeting it by
  bisection of fixed length; the governing one the smallest; then checked
  at that tension, and the loop repeated on any criterion above its limit.

Run: python3 -I benchmarks/sagtension/reference_crosscheck.py
(standard library only; about a minute).
"""

import math

PSI = 6894.757293168361  # Pa
A_TOTAL = (400.0 + 68.6442) * 1e-6  # m2: the curves are on the total area
T_CURVES = (70.0 - 32.0) * 5.0 / 9.0  # 21.111 C


def f2c(f):
    return (f - 32.0) * 5.0 / 9.0


# component: alpha, initial (psi per %^k), creep, final psi per %
COMPONENTS = [
    (2.304e-05, [-1213, 44308.1, -14004.4, -37618, 30676], [-544.8, 21426.8, -18842.2, 5495, 0], 64000.0),
    (1.152e-05, [-69.3, 38629, 3998.1, -45713, 27892], [47.1, 36211.3, 12201.4, -72392, 46338], 37000.0),
]

S = 365.76  # m, level
W = 15.965729813  # N/m own weight at the known state
H_KNOWN = 26689.329692  # N
T_KNOWN = f2c(60.0)
CREEP = (W, 0.0, f2c(60.0))  # load, wind, temperature
MAX = (36.865347248, 20.509298261, f2c(0.0))


def poly(c, e):
    """Load (N) of a curve at mechanical strain e (unit)."""
    p = e * 100.0
    return sum(a * p**k for k, a in enumerate(c)) * PSI * A_TOTAL


def dpoly(c, e):
    p = e * 100.0
    return sum(k * a * p ** (k - 1) for k, a in enumerate(c) if k) * PSI * A_TOTAL * 100.0


def final_stiffness(comp):
    return comp[3] * PSI * A_TOTAL * 100.0


def comp_load(comp, e, state, perm):
    alpha, initial, creep, _ = comp
    ea = final_stiffness(comp)
    if state == "initial":
        v = poly(initial, e) if e >= 0 else ea * e
    elif state == "creep":
        v = poly(creep, e) if e >= 0 else ea * e
    else:
        envelope = poly(initial, e) if e >= 0 else ea * e
        v = min(ea * (e - perm), envelope)
    return max(v, 0.0)  # no compression in either component


def tension(e, t, state, perms=(0.0, 0.0)):
    return sum(
        comp_load(c, e - c[0] * (t - T_CURVES), state, perms[k]) for k, c in enumerate(COMPONENTS)
    )


def strain(T, t, state, perms=(0.0, 0.0)):
    """Illinois method on a bracket where the tension is below and above T."""
    f = lambda e: tension(e, t, state, perms) - T
    a, b = -0.01, 0.03
    fa, fb = f(a), f(b)
    assert fa < 0 < fb, (T, fa, fb)
    side = 0
    c = a
    for _ in range(400):
        c = (a * fb - b * fa) / (fb - fa)
        fc = f(c)
        if abs(fc) < 1e-10 or abs(b - a) < 1e-17:
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


def zero_strain(t):
    """Lowest total strain at which a component starts to carry load
    (initial state): Newton on each polynomial from zero."""
    out = []
    for c in COMPONENTS:
        e = 0.0
        for _ in range(100):
            step = poly(c[1], e) / dpoly(c[1], e)
            e -= step
            if abs(step) < 1e-18:
                break
        out.append(max(e, 0.0) + c[0] * (t - T_CURVES))
    return min(out)


EPS_W = zero_strain(T_KNOWN)


def simpson(f, a, b, n=2000):
    h = (b - a) / n
    return (f(a) + f(b) + sum((4 if i % 2 else 2) * f(a + i * h) for i in range(1, n))) * h / 3


def span(H, q):
    """Length and mean tension of the level span at H under q N/m."""
    C = H / q
    L = simpson(lambda x: math.cosh((x - S / 2) / C), 0.0, S)
    I = simpson(lambda x: H * math.cosh((x - S / 2) / C) ** 2, 0.0, S)
    return L, I / L


def sag(H, q):
    C = H / q
    return C * (math.cosh(S / (2 * C)) - 1)


def load_per_metre(load, wind, k):
    vertical = math.sqrt(load * load - wind * wind)
    ice = max(vertical - W, 0.0)
    return math.hypot(ice + W / k, wind)


def loaded(H, case, state, perms=(0.0, 0.0)):
    load, wind, t = case
    k = 1.0
    for _ in range(200):
        q = load_per_metre(load, wind, k)
        L, Tm = span(H, q)
        e = strain(Tm, t, state, perms)
        k_new = (1 + e) / (1 + EPS_W)
        if abs(k_new - k) <= 1e-15 * k:
            break
        k = k_new
    return L, Tm, e, q


def unstrained(H, case, state, perms=(0.0, 0.0)):
    L, _, e, _ = loaded(H, case, state, perms)
    return L / (1 + e)


def solve(L0, case, state, perms=(0.0, 0.0), guess=25_000.0):
    f = lambda lh: unstrained(math.exp(lh), case, state, perms) - L0
    a, b = math.log(guess), math.log(guess * 1.05)
    fa, fb = f(a), f(b)
    for _ in range(80):
        if fb == fa:
            break
        c = b - fb * (b - a) / (fb - fa)
        a, fa, b, fb = b, fb, c, f(c)
        if abs(b - a) < 1e-14:
            break
    return math.exp(b)


def permanent(T, t, state):
    e = strain(T, t, state)
    out = []
    for c in COMPONENTS:
        em = e - c[0] * (t - T_CURVES)
        out.append(em - comp_load(c, em, state, 0.0) / final_stiffness(c))
    return tuple(out)


def strung(h_known):
    L0 = unstrained(h_known, (W, 0.0, T_KNOWN), "initial")
    hc = solve(L0, CREEP, "creep", guess=h_known)
    perm_creep = permanent(loaded(hc, CREEP, "creep")[1], CREEP[2], "creep")
    hl = solve(L0, MAX, "initial", guess=2 * h_known)
    perm_load = permanent(loaded(hl, MAX, "initial")[1], MAX[2], "initial")
    return L0, perm_creep, perm_load


def case_results(h_known, case):
    L0, pc, pl = strung(h_known)
    hi = solve(L0, case, "initial", guess=h_known)
    hc = solve(L0, case, "final", pc, guess=hi)
    hl = solve(L0, case, "final", pl, guess=hi)
    gov = 0 if hc < hl * (1 - 1e-9) else 1
    return {
        "h_initial_n": hi,
        "h_final_creep_n": hc,
        "h_final_load_n": hl,
        "h_final_n": hc if gov == 0 else hl,
        "final_cause": float(gov),
    }


CASES = [
    ("xdrake-001", (W, 0.0, f2c(60.0))),
    ("xdrake-002", (W, 0.0, f2c(32.0))),
    ("xdrake-003", (W, 0.0, f2c(120.0))),
    ("xdrake-004", (W, 0.0, f2c(167.0))),
    ("xdrake-005", (W, 0.0, f2c(212.0))),
    ("xdrake-006", (W, 0.0, f2c(250.0))),
    ("xdrake-007", MAX),
    ("xdrake-008", (62.259841953, 30.238566886, f2c(0.0))),
    ("xdrake-009", (30.633682647, 0.0, f2c(0.0))),
]

# Tension criteria (nonlinear-criteria.toml): label, case, condition,
# limit kind, value. RTS 140118.981 N.
RTS = 140118.981


def measured(h_known, case, condition, kind):
    L0, pc, pl = strung(h_known)
    state, perms = {
        "initial": ("initial", (0.0, 0.0)),
        "creep": ("final", pc),
        "load": ("final", pl),
    }[condition]
    H = solve(L0, case, state, perms, guess=h_known)
    if kind == "horizontal":
        return H
    if kind == "support":
        _, _, _, q = loaded(H, case, state, perms)
        return H * math.cosh(S / (2 * H / q))
    if kind == "catenary_constant":
        _, _, _, q = loaded(H, case, state, perms)
        load, wind, _ = case
        return H / load  # H over the case's load per metre, as given
    raise ValueError(kind)


def governing(criteria):
    """Largest stringing tension meeting every criterion; returns (h, index,
    utilisations)."""

    def h_max(c):
        _, case, cond, kind, limit = c
        lo, hi = 5_000.0, 80_000.0
        assert measured(lo, case, cond, kind) < limit < measured(hi, case, cond, kind)
        for _ in range(48):  # fixed: (75 kN / 2^48) far below 1e-9 relative
            mid = 0.5 * (lo + hi)
            if measured(mid, case, cond, kind) > limit:
                hi = mid
            else:
                lo = mid
        return lo

    best = min((h_max(c), i) for i, c in enumerate(criteria))
    h, index = best
    for _ in range(10):
        util = [measured(h, c[1], c[2], c[3]) / c[4] for c in criteria]
        worst = max(range(len(util)), key=lambda i: util[i])
        if util[worst] <= 1 + 1e-9:
            return h, index, util
        h, index = h_max(criteria[worst]), worst
    raise RuntimeError("no fixed point")


EVERYDAY = (W, 0.0, f2c(60.0))
CRITERIA_CASES = {
    # one criterion each: the stringing tension that meets it
    "nlcrit-001": [("H initial at 60 F <= 26689.329692 N", EVERYDAY, "initial", "horizontal", 26689.329692)],
    "nlcrit-002": [("H after creep at 60 F <= 18 % RTS", EVERYDAY, "creep", "horizontal", 0.18 * RTS)],
    "nlcrit-003": [("support after load at the heavy case <= 40 % RTS", MAX, "load", "support", 0.40 * RTS)],
    "nlcrit-004": [("support initial at the heavy case <= 40 % RTS", MAX, "initial", "support", 0.40 * RTS)],
    "nlcrit-005": [
        ("H initial at 60 F <= 20 % RTS", EVERYDAY, "initial", "horizontal", 0.20 * RTS),
        ("H after creep at 60 F <= 15 % RTS", EVERYDAY, "creep", "horizontal", 0.15 * RTS),
        ("H/w after load at 60 F <= 1400 m", EVERYDAY, "load", "catenary_constant", 1400.0),
        ("support initial at the heavy case <= 50 % RTS", MAX, "initial", "support", 0.50 * RTS),
    ],
}

if __name__ == "__main__":
    print(f"eps_w = {EPS_W:.6e}")
    for cid, case in CASES:
        r = case_results(H_KNOWN, case)
        print(cid, " ".join(f"{k}={v:.10g}" for k, v in r.items()))
    for cid, criteria in CRITERIA_CASES.items():
        h, index, util = governing(criteria)
        print(cid, f"reference_tension_n={h:.10g} governing_index={index}",
              " ".join(f"utilisation_{i}={u:.10g}" for i, u in enumerate(util)))
