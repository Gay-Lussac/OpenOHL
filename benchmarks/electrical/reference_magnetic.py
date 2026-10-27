"""Independent reference values for benchmarks/electrical/magnetic.toml
(run: python3 -I).

1. Biot–Savart's law integrated numerically along each straight segment,

       dB = mu0 / (4 pi) * I dl x r / |r|^3,

   with scipy's adaptive quadrature (QUADPACK) per component — not the
   closed form the program uses. Currents are rms phasors; the components
   are summed as complex numbers, B is the root of the sum of their
   squared magnitudes, the ellipse's semi-major axis is found by sampling
   the cycle (3600 steps, refined by a golden-section search) — not by the
   program's closed form.
2. Infinite straight conductors: B = mu0 I / (2 pi r) per conductor,
   summed as phasors in their own frame (y across, z up).

Prints each case's expected outputs.
"""

import cmath
import math

from scipy.integrate import quad

MU0 = 4e-7 * math.pi


def segment(a, b, current, p):
    """Complex field (T) of current (A phasor) from a to b at p, by quadrature."""
    d = [b[k] - a[k] for k in range(3)]

    def integrand(s, k):
        q = [a[i] + s * d[i] for i in range(3)]
        r = [p[i] - q[i] for i in range(3)]
        n3 = math.sqrt(sum(x * x for x in r)) ** 3
        c = [d[1] * r[2] - d[2] * r[1], d[2] * r[0] - d[0] * r[2], d[0] * r[1] - d[1] * r[0]]
        return c[k] / n3

    out = []
    for k in range(3):
        v = quad(lambda s: integrand(s, k), 0.0, 1.0, limit=500, epsabs=0, epsrel=1e-12)[0]
        out.append(MU0 / (4 * math.pi) * v * current)
    return out


def major(field):
    """Largest |Re(B e^{jwt})| over the cycle, as an rms value."""

    def mag(t):
        return math.sqrt(sum((c * cmath.exp(1j * t)).real ** 2 for c in field))

    n = 3600
    best = max(range(n), key=lambda i: mag(2 * math.pi * i / n))
    lo, hi = 2 * math.pi * (best - 1) / n, 2 * math.pi * (best + 1) / n
    g = (math.sqrt(5) - 1) / 2
    for _ in range(100):
        m1, m2 = hi - g * (hi - lo), lo + g * (hi - lo)
        if mag(m1) > mag(m2):
            hi = m2
        else:
            lo = m1
    return mag(0.5 * (lo + hi))


def report(cid, field):
    b = math.sqrt(sum(abs(c) ** 2 for c in field))
    comps = "  ".join(f"{n}_ut = {abs(c) * 1e6:.10g}" for n, c in zip("xyz", field))
    print(f"{cid}: {comps}  b_ut = {b * 1e6:.10g}  b_major_ut = {major(field) * 1e6:.10g}")


def ph(i, deg):
    return cmath.rect(i, math.radians(deg))


def line2d(conductors, y, z):
    by = bz = 0
    for yk, zk, i in conductors:
        dy, dz = y - yk, z - zk
        r2 = dy * dy + dz * dz
        k = MU0 / (2 * math.pi) / r2
        by += -dz * k * i
        bz += dy * k * i
    return [by, bz]


SEGMENT_CASES = [
    # (id, [(a, b, current, angle)], point)
    ("electrical-032", [((0, 0, 0), (100, 0, 0), 500, 0)], (130, 5, 3)),
    ("electrical-033", [((0, 0, 10), (40, 30, 25), 800, 0)], (10, -8, 1)),
    (
        "electrical-034",
        [
            # A sagging wire as three pieces and a second phase beside it.
            ((0, 0, 30), (40, 0, 22), 1000, 0),
            ((40, 0, 22), (80, 0, 20), 1000, 0),
            ((80, 0, 20), (120, 0, 26), 1000, 0),
            ((0, 6, 30), (120, 6, 26), 1000, -120),
        ],
        (60, 15, 1),
    ),
]


def main():
    for cid, segs, p in SEGMENT_CASES:
        field = [0j, 0j, 0j]
        for a, b, i, deg in segs:
            f = segment(a, b, ph(i, deg), p)
            field = [field[k] + f[k] for k in range(3)]
        report(cid, field)
    # electrical-035: a balanced flat line, 2D, near the line.
    flat = [(-8, 20, ph(1000, 0)), (0, 20, ph(1000, -120)), (8, 20, ph(1000, 120))]
    f = line2d(flat, 15, 1)
    b = math.sqrt(sum(abs(c) ** 2 for c in f))
    print(f"electrical-035: b_ut = {b * 1e6:.10g}  b_major_ut = {major(f) * 1e6:.10g}")
    # electrical-036/037: the far field's closed form sqrt(3) mu0 I s / (2 pi d^2).
    for cid, d in [("electrical-036", 4000.0), ("electrical-037", 8000.0)]:
        far = math.sqrt(3) * MU0 * 1000 * 8 / (2 * math.pi * d * d)
        f = line2d(flat, d, 20)
        b = math.sqrt(sum(abs(c) ** 2 for c in f))
        print(f"{cid}: far field {far * 1e6:.10g} uT (exact sum {b * 1e6:.10g})")


if __name__ == "__main__":
    main()
