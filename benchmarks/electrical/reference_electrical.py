"""Independent reference values for benchmarks/electrical (run: python3 -I).

1. Carson's earth return term by numerical integration of Carson's
   integral (J. R. Carson, Bell System Technical Journal 5 (4), 1926):

       dZ = j (omega mu0 / pi) * integral_0^inf
            exp(-(hi + hj) l) cos(x l) / (l + sqrt(l^2 + j omega mu0 / rho)) dl

   evaluated with scipy's adaptive quadrature (QUADPACK) — a different
   method from the program's series and asymptotic expansion.
2. The complex depth term (Deri et al., IEEE Trans. PAS-100, 1981):
   j omega mu0 / (2 pi) * ln(sqrt((hi + hj + 2p)^2 + x^2) / D'),
   p = sqrt(rho / (j omega mu0)), in complex arithmetic of its own.
3. The symmetrical components (Fortescue) of the published phase
   impedance matrix of Kersting's Example 4.1 as T. Overbye's ECEN 667
   lecture 4 (2019, slide 16) prints it, and the ideally transposed values
   from its means — the program computes them from its own matrix.

Prints each case's expected outputs.
"""

import cmath
import math

from scipy.integrate import quad

MU0 = 4e-7 * math.pi


def carson_integral(hi, hj, x, f, rho):
    """Carson's correction dZ, ohm/m (complex), by quadrature."""
    w = 2 * math.pi * f
    m = w * MU0 / rho
    H = hi + hj

    def g(l):
        return math.exp(-H * l) * math.cos(x * l) / (l + cmath.sqrt(l * l + 1j * m))

    upper = 60.0 / H  # exp(-60): nothing left beyond
    re = quad(lambda l: g(l).real, 0, upper, limit=500, epsabs=0, epsrel=1e-12)[0]
    im = quad(lambda l: g(l).imag, 0, upper, limit=500, epsabs=0, epsrel=1e-12)[0]
    return 1j * (w * MU0 / math.pi) * complex(re, im)


def complex_depth(hi, hj, x, f, rho):
    """The complex depth's term beyond the perfect earth's, ohm/m."""
    w = 2 * math.pi * f
    p = cmath.sqrt(rho / (1j * w * MU0))
    far = cmath.sqrt((hi + hj + 2 * p) ** 2 + x * x)
    near = math.hypot(hi + hj, x)
    return 1j * w * MU0 / (2 * math.pi) * cmath.log(far / near)


# (id, hi, hj, x, f, rho): the cases of earth-return.toml.
CARSON_CASES = [
    ("electrical-010", 20.0, 20.0, 0.0, 50.0, 100.0),
    ("electrical-011", 20.0, 26.0, 7.5, 50.0, 100.0),
    ("electrical-012", 12.0, 30.0, 40.0, 50.0, 10.0),
    ("electrical-013", 25.0, 25.0, 0.0, 16.7, 1000.0),
    ("electrical-014", 30.0, 30.0, 0.0, 50.0, 0.5),
    ("electrical-015", 15.0, 18.0, 60.0, 5000.0, 20.0),
    ("electrical-016", 40.0, 35.0, 120.0, 50000.0, 10.0),
]

COMPLEX_DEPTH_CASES = [
    ("electrical-020", 20.0, 26.0, 7.5, 50.0, 100.0),
    ("electrical-021", 15.0, 18.0, 60.0, 5000.0, 20.0),
]

# Kersting Example 4.1, Zp in ohm/mile as printed (slide 16).
ZP = [
    [complex(0.4576, 1.0780), complex(0.1560, 0.5017), complex(0.1535, 0.3849)],
    [complex(0.1560, 0.5017), complex(0.4666, 1.0482), complex(0.1580, 0.4236)],
    [complex(0.1535, 0.3849), complex(0.1580, 0.4236), complex(0.4615, 1.0651)],
]


def sequence(z):
    a = cmath.exp(2j * math.pi / 3)
    A = [[1, 1, 1], [1, a * a, a], [1, a, a * a]]
    Ai = [[x.conjugate() / 3 for x in row] for row in A]

    def mul(p, q):
        return [[sum(p[i][k] * q[k][j] for k in range(3)) for j in range(3)] for i in range(3)]

    return mul(mul(Ai, z), A)


def main():
    for cid, hi, hj, x, f, rho in CARSON_CASES:
        dz = carson_integral(hi, hj, x, f, rho)
        a = math.hypot(hi + hj, x) * math.sqrt(2 * math.pi * f * MU0 / rho)
        print(f"{cid}: a = {a:.6g}  dr_ohm_per_km = {dz.real * 1e3:.9g}  dx_ohm_per_km = {dz.imag * 1e3:.9g}")
    for cid, hi, hj, x, f, rho in COMPLEX_DEPTH_CASES:
        dz = complex_depth(hi, hj, x, f, rho)
        exact = carson_integral(hi, hj, x, f, rho)
        print(
            f"{cid}: dr_ohm_per_km = {dz.real * 1e3:.9g}  dx_ohm_per_km = {dz.imag * 1e3:.9g}"
            f"  (Carson's integral: {exact.real * 1e3:.6g}, {exact.imag * 1e3:.6g})"
        )
    s = sequence(ZP)
    zs = sum(ZP[i][i] for i in range(3)) / 3
    zm = sum(ZP[i][j] for i in range(3) for j in range(3) if i != j) / 6
    print("electrical-003 (sequence of the published Zp, ohm/mile):")
    print(f"  z1_r = {(zs - zm).real:.6f}  z1_x = {(zs - zm).imag:.6f}")
    print(f"  z0_r = {(zs + 2 * zm).real:.6f}  z0_x = {(zs + 2 * zm).imag:.6f}")
    for k, n in [(1, "z1_built"), (2, "z2_built"), (0, "z0_built")]:
        print(f"  {n}_r = {s[k][k].real:.6f}  {n}_x = {s[k][k].imag:.6f}")


if __name__ == "__main__":
    main()
