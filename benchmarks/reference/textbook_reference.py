# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

"""Independent reference values for the textbook and CIGRE TB 324 cases.

Covers:
    benchmarks/sagtension/textbook-catenary.toml
    benchmarks/sagtension/textbook-change-of-state.toml
    benchmarks/sagtension/textbook-ruling-span.toml
    benchmarks/sagtension/textbook-point-load.toml
    benchmarks/sagtension/textbook-bimetallic.toml
    benchmarks/sagtension/cigre-tb324.toml
    benchmarks/weather/textbook-ice-wind.toml
    benchmarks/wirefe/textbook-elastic-catenary.toml
    benchmarks/wirefe/textbook-insulators.toml

Method (deliberately different from the Rust code, which uses closed-form
catenaries and Irvine's closed-form elastic element):

* A cable is described by its tension vector along the arc length s,
  F(s) = F0 - q s (q: load per unit length; equilibrium of the piece from
  the start to s), and its shape is the *integral* of its tangent:

      X(s) = integral_0^s  F/|F| (1 + |F|/EA) ds      (EA = inf: inextensible)

  evaluated by Gauss-Legendre quadrature (64 nodes per piece, checked
  against 96 nodes: the script prints the largest difference). The unknown
  start force F0 (and for an inextensible span its length) is found by
  Newton's method with the quadrature's own Jacobian, until the end lands
  on the far support. Sag, low point, mean tension and length come from
  the same integrals; no catenary formula is used.
* Point loads: the force vector jumps by the load where it acts; the arc
  lengths of the pieces are unknowns of the same Newton iteration.
* Change of state / sections: an outer root search (secant on log H) on the
  conserved unstressed length, with the cable model each calculation
  documents (multiplicative thermal form with the mean tension for the
  sag-tension change of state and section; the additive strain of the
  stress-strain model for the ruling-span and non-linear calculations).
* Finite-element line sections: free nodes (clamps, V tips) found by Newton
  on the nodal force balance (finite-difference Jacobian), each span the
  quadrature cable above in 3D, each string a bar (EA/l0 (l - l0),
  tension-only) with half its weight and half its wind at each end. The
  sections are first strung as the theory page says (each span at the
  stringing tension in still air, strings plumb and stretched by their
  load), which fixes each span's unstressed length; a weather case then
  scales it by exp(alpha (t - t_stringing)) and loads it per unit of that
  length: the own weight w exp(-alpha (t - t_stringing)) (the span keeps
  its weight, owner decision 2026-10-21), ice and wind as they are.
* The cable's own weight is conserved in every calculation: per metre of
  unstressed cable at the known (stringing) temperature; the closed-form
  and ruling-span models spread it over the stretched span (w / k per
  metre, k the stretch, a fixed point), see conserved_span().

Hand derivations confirming these values to the digits shown are written
out in each case file's comments.

Run:  python3 -I benchmarks/reference/textbook_reference.py
Needs only the Python standard library (math). Prints every expected value
with 10 significant digits, case by case.
"""

import math

G = 9.80665  # standard gravity, m/s^2


# ---------------------------------------------------------------- quadrature
def gauss_legendre(n):
    """Nodes and weights on [-1, 1] (Newton on the Legendre polynomial)."""
    nodes, weights = [], []
    for i in range(1, n + 1):
        x = math.cos(math.pi * (i - 0.25) / (n + 0.5))
        for _ in range(100):
            p0, p1 = 1.0, x
            for k in range(2, n + 1):
                p0, p1 = p1, ((2 * k - 1) * x * p1 - (k - 1) * p0) / k
            dp = n * (x * p1 - p0) / (x * x - 1)
            dx = p1 / dp
            x -= dx
            if abs(dx) < 1e-16:
                break
        nodes.append(x)
        weights.append(2 / ((1 - x * x) * dp * dp))
    return nodes, weights


GL = {n: gauss_legendre(n) for n in (64, 96)}
ORDER = [64]  # quadrature order in use (switched to 96 for the self-check)


def quad(f, a, b, pieces=8):
    """Integral of f (returning a list of floats) from a to b."""
    nodes, weights = GL[ORDER[0]]
    total = None
    h = (b - a) / pieces
    for p in range(pieces):
        lo = a + p * h
        for x, wt in zip(nodes, weights):
            v = f(lo + (x + 1) * h / 2)
            if total is None:
                total = [0.0] * len(v)
            for k, vk in enumerate(v):
                total[k] += vk * wt * h / 2
    return total


# ------------------------------------------------------------- linear algebra
def solve_linear(A, b):
    """Gaussian elimination with partial pivoting."""
    n = len(b)
    M = [row[:] + [b[i]] for i, row in enumerate(A)]
    for c in range(n):
        p = max(range(c, n), key=lambda r: abs(M[r][c]))
        M[c], M[p] = M[p], M[c]
        for r in range(c + 1, n):
            f = M[r][c] / M[c][c]
            for k in range(c, n + 1):
                M[r][k] -= f * M[c][k]
    x = [0.0] * n
    for r in range(n - 1, -1, -1):
        x[r] = (M[r][n] - sum(M[r][k] * x[k] for k in range(r + 1, n))) / M[r][r]
    return x


def secant(f, a, b, tol=1e-15, cap=200):
    """Root of f by the secant method from a, b (fixed iteration cap)."""
    fa, fb = f(a), f(b)
    for _ in range(cap):
        if fb == fa:
            break
        c = b - fb * (b - a) / (fb - fa)
        a, fa, b, fb = b, fb, c, f(c)
        if abs(b - a) <= tol * max(1.0, abs(b)):
            break
    return b


def bisect(f, a, b, cap=200):
    fa = f(a)
    for _ in range(cap):
        m = (a + b) / 2
        fm = f(m)
        if (fm < 0) == (fa < 0):
            a, fa = m, fm
        else:
            b = m
        if b - a <= 1e-15 * max(1.0, abs(a)):
            break
    return (a + b) / 2


# ------------------------------------------------- one cable piece, 2D or 3D
def norm(v):
    return math.sqrt(sum(x * x for x in v))


def cable_integrals(F0, q, L, EA):
    """Chord, its Jacobian w.r.t. F0, the integral of |F| over the piece.

    F(s) = F0 - q s; X = int F/|F| (1 + |F|/EA) ds.
    """
    n = len(F0)

    def integrand(s):
        F = [F0[k] - q[k] * s for k in range(n)]
        N = norm(F)
        u = [x / N for x in F]
        out = [u[k] + F[k] / EA for k in range(n)]
        for i in range(n):
            for j in range(n):
                out.append(((1.0 if i == j else 0.0) - u[i] * u[j]) / N + (1.0 if i == j else 0.0) / EA)
        out.append(N)
        return out

    v = quad(integrand, 0.0, L)
    X = v[:n]
    J = [v[n + i * n: n + (i + 1) * n] for i in range(n)]
    return X, J, v[-1]


def solve_piece(chord, L, q, EA, guess=None):
    """Start force F0 of an elastic piece of unstressed length L between two
    points `chord` apart under load q per unit (unstressed) length."""
    n = len(chord)
    if guess is None:
        d = norm(chord)
        slack = max(L - d, 1e-3 * d)
        sag = math.sqrt(3 * d * slack / 8)
        H = norm(q) * d * d / (8 * sag)
        guess = [H * chord[k] / d + q[k] * L / 2 for k in range(n)]
    F0 = list(guess)
    for _ in range(100):
        X, J, _ = cable_integrals(F0, q, L, EA)
        r = [X[k] - chord[k] for k in range(n)]
        if norm(r) < 1e-13 * max(1.0, norm(chord)):
            break
        dF = solve_linear(J, [-x for x in r])
        step = 1.0
        while True:  # damping: keep the start force from collapsing
            trial = [F0[k] + step * dF[k] for k in range(n)]
            if min(norm([trial[k] - q[k] * s for k in range(n)]) for s in (0.0, L / 2, L)) > 0:
                break
            step /= 2
        F0 = trial
    else:
        raise RuntimeError("piece did not converge")
    return F0


# ------------------------------------------------------ inextensible catenary
INF = float("inf")


def catenary_span(S, h, H, w):
    """Inextensible span from (0, 0) to (S, h): horizontal tension H, load w
    per unit length. Unknowns: start vertical force V0 (F = (H, V0 + w s)
    taken as the tangent force; F0 = (H, V0) with V0 < 0 going down) and the
    length L. Returns a dict of the quantities the cases check."""
    q = [0.0, -w]
    V0, L = -w * S / 2 + H * h / S, math.hypot(S, h) * 1.001
    for _ in range(100):
        X, J, _ = cable_integrals([H, V0], q, L, INF)
        r = [X[0] - S, X[1] - h]
        if norm(r) < 1e-12 * S:
            break
        F_end = [H, V0 + w * L]
        T_end = norm(F_end)
        # d/dV0 is J[.][1]; d/dL is the tangent at the end.
        A = [[J[0][1], F_end[0] / T_end], [J[1][1], F_end[1] / T_end]]
        dV, dL = solve_linear(A, [-r[0], -r[1]])
        V0, L = V0 + dV, L + dL
    else:
        raise RuntimeError("catenary did not converge")

    def point(s):
        """Position at arc length s (negative: on the curve extended beyond
        the left support, where the low point may lie)."""
        if s == 0:
            return [0.0, 0.0]
        if s > 0:
            return cable_integrals([H, V0], q, s, INF)[0]
        # integrate the tangent backwards over [s, 0]
        v = quad(lambda t: [H / norm([H, V0 + w * t]), (V0 + w * t) / norm([H, V0 + w * t])], s, 0.0)
        return [-v[0], -v[1]]

    _, _, T_int = cable_integrals([H, V0], q, L, INF)
    # Largest sag below the chord: where the slope equals the chord's.
    s_star = (H * h / S - V0) / w
    xs = point(s_star)
    sag = h / S * xs[0] - xs[1]
    # Sag at mid-span: the arc length where x = S/2 (Newton on s).
    s_mid = L / 2
    for _ in range(60):
        xm = point(s_mid)
        ds = (S / 2 - xm[0]) / (H / norm([H, V0 + w * s_mid]))
        s_mid += ds
        if abs(ds) < 1e-14 * S:
            break
    xm = point(s_mid)
    midspan_sag = h / 2 - xm[1]
    s_low = -V0 / w
    low = point(s_low)
    return {
        "V0": V0,
        "length": L,
        "sag": sag,
        "midspan_sag": midspan_sag,
        "tension_left": norm([H, V0]),
        "tension_right": norm([H, V0 + w * L]),
        "low_point_x": low[0],
        "low_point_z": low[1],
        "mean_tension": T_int / L,
        "vertical_left": -V0,  # upward reaction at the left support
        "vertical_right": V0 + w * L,
    }


def tension_for_sag(S, h, w, D):
    return bisect(lambda H: catenary_span(S, h, H, w)["sag"] - D, 1_000.0, 200_000.0)


# ------------------------------------------------------- change of state, RS
#
# The cable's own weight is conserved (owner decision 2026-10-21): it is per
# metre of unstressed cable at the known (stringing) state's temperature,
# so a span weighs w0 L0 in every state. The catenary takes its load per
# metre of stretched cable: the own weight w0 / k with k the span's length
# over that unstressed length; ice (w1 - w0 in the vertical) and wind stay
# per metre of the real cable. k depends on the mean tension of the very
# catenary it loads: a fixed point, iterated here until it no longer moves.
def conserved_span(S, h, H, own, ice, wind, k_of):
    """Inextensible span at H whose own weight `own` per unstressed metre is
    spread over its length (k_of(c): the stretch of catenary c). Returns the
    catenary dict and its load per metre."""
    k = 1.0
    for _ in range(100):
        w = math.hypot(own / k + ice, wind)
        c = catenary_span(S, h, H, w)
        k_new = k_of(c)
        if abs(k_new - k) <= 1e-15 * k:
            break
        k = k_new
    else:
        raise RuntimeError("stretch did not settle")
    return c, w, k_new


def split(w0, w1, wind1):
    """Own weight w0, the target's ice (vertical w1 beyond w0) and wind."""
    return w0, math.sqrt(w1 * w1 - wind1 * wind1) - w0, wind1


def cos_multiplicative(spans, EA, alpha, H0, w0, t0, w1, t1, wind1=0.0):
    """Equal-H section (one span: the single-span change of state): the
    code's linear model L = L0 exp(alpha dt) (1 + Tmean/EA) per span, the
    own weight w0 conserved. Returns H and each span's load per metre."""
    own, ice, wind = split(w0, w1, wind1)

    def unstressed(H, ice, wind, thermal):
        total, loads = 0.0, []
        for S, h in spans:
            c, w, k = conserved_span(S, h, H, own, ice, wind,
                                     lambda c: thermal * (1 + c["mean_tension"] / EA))
            total += c["length"] / k
            loads.append(w)
        return total, loads

    L0 = unstressed(H0, 0.0, 0.0, 1.0)[0]
    thermal = math.exp(alpha * (t1 - t0))
    lh = secant(lambda x: unstressed(math.exp(x), ice, wind, thermal)[0] - L0,
                math.log(H0), math.log(H0 * 0.9))
    return math.exp(lh), unstressed(math.exp(lh), ice, wind, thermal)[1]


def cos_additive(spans, strain, H0, w0, t0, w1, t1, wind1=0.0):
    """Equal-H section with the stress-strain model's additive strain:
    sum L_i / (1 + eps(Tmean_i, t)) conserved; strain(T, t) given. The own
    weight w0 is per metre of the unstressed cable at t0, the length
    L0 (1 + eps(0, t0)): span i carries w0 (1 + eps(0, t0)) / (1 + eps_i).
    Returns H and each span's load per metre."""
    own, ice, wind = split(w0, w1, wind1)
    m = strain(0.0, t0)

    def unstressed(H, ice, wind, t):
        total, loads = 0.0, []
        for S, h in spans:
            c, w, k = conserved_span(S, h, H, own, ice, wind,
                                     lambda c: (1 + strain(c["mean_tension"], t)) / (1 + m))
            total += c["length"] / (1 + strain(c["mean_tension"], t))
            loads.append(w)
        return total, loads

    L0 = unstressed(H0, 0.0, 0.0, t0)[0]
    lh = secant(lambda x: unstressed(math.exp(x), ice, wind, t1)[0] - L0, math.log(H0), math.log(H0 * 0.9))
    return math.exp(lh), unstressed(math.exp(lh), ice, wind, t1)[1]


def cos_unconserved(spans, EA, alpha, H0, w0, t0, w1, t1):
    """The model before 2026-10-21, for comparison only: the same load per
    metre of stretched cable in every state (the span heavier when hot)."""

    def unstressed(H, w):
        total = 0.0
        for S, h in spans:
            c = catenary_span(S, h, H, w)
            total += c["length"] / (1 + c["mean_tension"] / EA)
        return total

    L0 = unstressed(H0, w0) * math.exp(alpha * (t1 - t0))
    return math.exp(secant(lambda x: unstressed(math.exp(x), w1) - L0, math.log(H0), math.log(H0 * 0.9)))


def cos_tb324(S, EA, alpha, H0, w0, t0, w1, t1):
    """TB 324's linear elongation model (sec. 5.2, 6.1): the length at H is
    L_ref (1 + (H - H_ref)/EA + alpha dt), H the horizontal tension."""
    L_ref = catenary_span(S, 0.0, H0, w0)["length"]
    return secant(
        lambda H: catenary_span(S, 0.0, H, w1)["length"] - L_ref * (1 + (H - H0) / EA + alpha * (t1 - t0)),
        H0,
        H0 * 1.1,
    )


# ------------------------------------------------------------- point loads
def loaded_span(S, h, H, w, loads):
    """Inextensible span with point loads [(x, P)] (x horizontal from the
    left support). Unknowns: V0 and the arc length of each piece."""
    loads = sorted(loads)
    xs = [x for x, _ in loads] + [S]
    n = len(xs)

    def ends(V0, lengths):
        """End point of each piece, and the start force of each piece."""
        pts, starts = [], []
        F0 = [H, V0]
        p = [0.0, 0.0]
        for k, L in enumerate(lengths):
            starts.append(list(F0))
            X, _, _ = cable_integrals(F0, [0.0, -w], L, INF)
            p = [p[0] + X[0], p[1] + X[1]]
            pts.append(p)
            F0 = [H, F0[1] + w * L]
            if k < len(loads):
                F0[1] += loads[k][1]  # the load pulls down: the tangent force jumps up
        return pts, starts

    def residual(u):
        pts, _ = ends(u[0], u[1:])
        r = [pts[k][0] - xs[k] for k in range(n)]
        r.append(pts[-1][1] - h)
        return r

    total_load = w * math.hypot(S, h) + sum(P for _, P in loads)
    u = [-total_load / 2 + H * h / S] + [xs[0]] + [xs[k] - xs[k - 1] for k in range(1, n)]
    for _ in range(100):
        r = residual(u)
        if norm(r) < 1e-12 * S:
            break
        J = []
        for k in range(len(u)):
            v = list(u)
            dk = 1e-6 * max(1.0, abs(u[k]))
            v[k] += dk
            rv = residual(v)
            J.append([(rv[i] - r[i]) / dk for i in range(len(r))])
        A = [[J[k][i] for k in range(len(u))] for i in range(len(r))]
        du = solve_linear(A, [-x for x in r])
        u = [u[k] + du[k] for k in range(len(u))]
    else:
        raise RuntimeError("loaded span did not converge")
    V0, lengths = u[0], u[1:]
    _, starts = ends(V0, lengths)
    V_end = starts[-1][1] + w * lengths[-1]
    # Largest sag below the chord: per piece where its slope is the chord's,
    # and at the load points.
    best = 0.0
    p0 = [0.0, 0.0]
    for k, L in enumerate(lengths):
        F0 = starts[k]
        cand = [0.0, L]
        s_star = (H * h / S - F0[1]) / w
        if 0 < s_star < L:
            cand.append(s_star)
        for s in cand:
            X = cable_integrals(F0, [0.0, -w], s, INF)[0] if s > 0 else [0.0, 0.0]
            best = max(best, h / S * (p0[0] + X[0]) - (p0[1] + X[1]))
        X = cable_integrals(F0, [0.0, -w], L, INF)[0]
        p0 = [p0[0] + X[0], p0[1] + X[1]]
    # slope jump at each load: tangent of the slope after minus before
    jumps = []
    for k in range(len(loads)):
        before = (starts[k][1] + w * lengths[k]) / H
        after = starts[k + 1][1] / H
        jumps.append((after - before, before, after))
    return {
        "length": sum(lengths),
        "sag": best,
        "tension_left": norm([H, V0]),
        "tension_right": norm([H, V_end]),
        "jumps": jumps,
        "V_left": -V0,
        "V_right": V_end,
    }


# ------------------------------------------------------ elastic catenary 2D
def elastic_span(L0, EA, chord, q):
    """Elastic piece (any dimension): start force, end force, tensions."""
    F0 = solve_piece(chord, L0, q, EA)
    F1 = [F0[k] - q[k] * L0 for k in range(len(chord))]
    return F0, F1


# ------------------------------------------------------ weather loads
def ice_load(d, t, rho):
    """Ice weight per metre: rho g times the annulus area, by quadrature."""
    r0, r1 = d / 2, d / 2 + t
    area = quad(lambda r: [2 * math.pi * r], r0, r1)[0]
    return rho * G * area


def wire_load(mass, d, t=0.0, rho=913.0, q=0.0):
    wc = mass * G
    wi = ice_load(d, t, rho) if t > 0 else 0.0
    D = d + 2 * t
    wv, wh = wc + wi, q * D
    return {
        "vertical": wv,
        "horizontal": wh,
        "resultant": math.hypot(wv, wh),
        "swing_deg": math.degrees(math.atan2(wh, wv)),
        "iced_diameter_mm": D * 1000,
        "ice": wi,
    }


# ------------------------------------------------------ bimetallic strain
def bimetallic_strain(EAo, ao, EAc, ac, limit, t_ref=20.0):
    """Total strain eps(T, t) of a two-component linear cable whose outer
    strands carry at most `limit` N in compression (0: bird cage)."""

    def strain(T, t):
        dt = t - t_ref
        eps = (T + EAo * ao * dt + EAc * ac * dt) / (EAo + EAc)
        if EAo * (eps - ao * dt) >= -limit:
            return eps
        return (T + limit) / EAc + ac * dt  # outer strands at their limit

    return strain


# ------------------------------------------------------ finite-element section
class Bar:
    """A string: from node `a` to node `b`, unstressed length l0, axial
    stiffness k (N), weight W (half at each end), wind force vector at the
    whole string (half at each free end), tension-only."""

    def __init__(self, a, b, l0, k, W, wind=(0.0, 0.0, 0.0)):
        self.a, self.b, self.l0, self.k, self.W, self.wind = a, b, l0, k, W, wind

    def force(self, pos):
        d = [pos[self.b][i] - pos[self.a][i] for i in range(3)]
        L = norm(d)
        N = self.k / self.l0 * (L - self.l0)
        if N < 0:
            N = 0.0
        u = [x / L for x in d]
        return N, u


class Span:
    def __init__(self, a, b, L0, EA, q):
        self.a, self.b, self.L0, self.EA, self.q = a, b, L0, EA, q
        self.guess = None

    def forces(self, pos):
        chord = [pos[self.b][i] - pos[self.a][i] for i in range(3)]
        F0 = solve_piece(chord, self.L0, self.q, self.EA, self.guess)
        self.guess = F0
        F1 = [F0[k] - self.q[k] * self.L0 for k in range(3)]
        return F0, F1  # force on a: +F0; on b: -F1


RESIDUALS = []  # largest unbalanced force of each section solved, N


def equilibrium(nodes, free, spans, bars, extra=None):
    """Positions of the free nodes where every force balances (Newton with a
    finite-difference Jacobian). nodes: list of [x, y, z]; free: indices."""
    extra = extra or {}

    def residual(x):
        pos = [list(p) for p in nodes]
        for k, i in enumerate(free):
            pos[i] = x[3 * k: 3 * k + 3]
        f = {i: [0.0, 0.0, 0.0] for i in free}
        for sp in spans:
            F0, F1 = sp.forces(pos)
            if sp.a in f:
                f[sp.a] = [f[sp.a][k] + F0[k] for k in range(3)]
            if sp.b in f:
                f[sp.b] = [f[sp.b][k] - F1[k] for k in range(3)]
        for br in bars:
            N, u = br.force(pos)
            for end, sign in ((br.a, 1), (br.b, -1)):
                if end in f:
                    f[end] = [f[end][k] + sign * N * u[k] + br.wind[k] / 2 for k in range(3)]
                    f[end][2] -= br.W / 2
        for i, load in extra.items():
            f[i] = [f[i][k] + load[k] for k in range(3)]
        return [c for i in free for c in f[i]]

    x = [c for i in free for c in nodes[i]]
    scale = 1.0 + max(abs(c) for sp in spans for c in sp.q) * max(sp.L0 for sp in spans)
    best = None
    for _ in range(80):
        r = residual(x)
        base = norm(r)
        if best is not None and base >= best:
            break  # no further progress: at the floating-point floor
        best = base
        if base < 1e-12 * scale:
            break
        J = []
        for k in range(len(x)):
            v = list(x)
            v[k] += 1e-7
            rv = residual(v)
            J.append([(rv[i] - r[i]) / 1e-7 for i in range(len(r))])
        A = [[J[k][i] for k in range(len(x))] for i in range(len(r))]
        dx = solve_linear(A, [-c for c in r])
        step = 1.0
        while True:
            trial = [x[k] + step * dx[k] for k in range(len(x))]
            try:
                if norm(residual(trial)) < base or step < 1e-3:
                    break
            except (RuntimeError, ZeroDivisionError, ValueError):
                pass
            step /= 2
        x = trial
    if best > 1e-8 * scale + 1e-5:
        raise RuntimeError(f"section did not converge (unbalanced {best} N)")
    RESIDUALS.append(best)
    pos = [list(p) for p in nodes]
    for k, i in enumerate(free):
        pos[i] = x[3 * k: 3 * k + 3]
    return pos


def string_at_stringing(top, chord_loads, l0, k, W):
    """Clamp of a plumb suspension string stretched by its load (fixed
    point): chord_loads(z) -> vertical load on the clamp from the spans."""
    z = top[2] - l0
    for _ in range(100):
        N = chord_loads(z) + W / 2
        z_new = top[2] - l0 * (1 + N / k)
        if abs(z_new - z) < 1e-13:
            break
        z = z_new
    return z


def span_stringing_length(a, b, H, w, EA):
    """Unstressed length of a span between a and b (3D, in a vertical
    plane) with horizontal tension H in still air: unknowns L0 and the start
    vertical force, by Newton on the 2D chord."""
    S = math.hypot(b[0] - a[0], b[1] - a[1])
    h = b[2] - a[2]
    V0, L = -w * S / 2 + H * h / S, catenary_span(S, h, H, w)["length"]
    for _ in range(100):
        X, J, _ = cable_integrals([H, V0], [0.0, -w], L, EA)
        r = [X[0] - S, X[1] - h]
        if norm(r) < 1e-13 * S:
            break
        F1 = [H, V0 + w * L]
        T1 = norm(F1)
        A = [[J[0][1], F1[0] / T1 + F1[0] / EA], [J[1][1], F1[1] / T1 + F1[1] / EA]]
        dV, dL = solve_linear(A, [-r[0], -r[1]])
        V0, L = V0 + dV, L + dL
    return L, -V0, V0 + w * L  # length, upward reaction at a, at b


def span_load_vector(mass, d, case, a, b, weight_scale=1.0):
    """Load per unit unstressed length on a span from a to b: weight (and
    ice) down, wind across the span to its left (wind from the right). The
    own weight times `weight_scale` (the cable as cut over the element's
    unstressed length: the span keeps its weight)."""
    wl = wire_load(mass * weight_scale, d, case.get("ice", 0.0), case.get("rho", 913.0), case.get("q", 0.0))
    dx, dy = b[0] - a[0], b[1] - a[1]
    hn = math.hypot(dx, dy)
    left = [-dy / hn, dx / hn, 0.0]
    return [left[k] * wl["horizontal"] for k in range(3)][:2] + [-wl["vertical"]]


def mid_point(sp, pos, at=0.5):
    """Point of the span at half the horizontal clamp distance."""
    a, b = pos[sp.a], pos[sp.b]
    F0 = solve_piece([b[i] - a[i] for i in range(3)], sp.L0, sp.q, sp.EA, sp.guess)
    horizontal = math.hypot(b[0] - a[0], b[1] - a[1])
    u = [(b[0] - a[0]) / horizontal, (b[1] - a[1]) / horizontal]

    def along(s):
        X = cable_integrals(F0, sp.q, s, sp.EA)[0] if s > 0 else [0.0, 0.0, 0.0]
        return X, X[0] * u[0] + X[1] * u[1]

    s = sp.L0 * at
    for _ in range(60):
        X, x = along(s)
        F = [F0[k] - sp.q[k] * s for k in range(3)]
        N = norm(F)
        dxds = (F[0] * u[0] + F[1] * u[1]) * (1 / N + 1 / sp.EA)
        ds = (horizontal * at - x) / dxds
        s += ds
        if abs(ds) < 1e-13:
            break
    X, _ = along(s)
    return [a[k] + X[k] for k in range(3)], F0


def max_tension(sp, pos):
    a, b = pos[sp.a], pos[sp.b]
    F0 = solve_piece([b[i] - a[i] for i in range(3)], sp.L0, sp.q, sp.EA, sp.guess)
    F1 = [F0[k] - sp.q[k] * sp.L0 for k in range(3)]
    return max(norm(F0), norm(F1))


def horizontal_tension(sp, pos):
    """Tension component perpendicular to the load (the solver's H)."""
    a, b = pos[sp.a], pos[sp.b]
    F0 = solve_piece([b[i] - a[i] for i in range(3)], sp.L0, sp.q, sp.EA, sp.guess)
    qn = norm(sp.q)
    along_q = sum(F0[k] * sp.q[k] for k in range(3)) / qn
    return math.sqrt(max(norm(F0) ** 2 - along_q**2, 0.0))


# ================================================================== CASES
def fmt(x):
    return f"{x:.10g}"


def show(case, values):
    print(f"{case}")
    for k, v in values.items():
        print(f"    {k:<26} {fmt(v)}")


def textbook_catenary():
    print("== textbook-catenary ==")
    for name, S, h, H, w in [
        ("tbcat-001 level 400 m, C = 2000 m", 400.0, 0.0, 20_000.0, 10.0),
        ("tbcat-002 level 1000 m, C = 1000 m", 1000.0, 0.0, 10_000.0, 10.0),
        ("tbcat-003 inclined 400 m, +20 m", 400.0, 20.0, 20_000.0, 10.0),
        ("tbcat-004 inclined 300 m, +60 m, uplift", 300.0, 60.0, 10_000.0, 10.0),
    ]:
        c = catenary_span(S, h, H, w)
        show(name, {k: c[k] for k in ("sag", "midspan_sag", "length", "tension_left", "tension_right", "low_point_x", "vertical_left")})
        if h == 0:
            D_par = w * S * S / (8 * H)
            print(f"    parabola sag {fmt(D_par)}  error {fmt(D_par - c['sag'])} m"
                  f"  length {fmt(S + 8 * D_par**2 / (3 * S))}")
    c = catenary_span(400.0, 0.0, 20_000.0, 10.0)
    show("tbcat-005 tension for the sag of tbcat-001", {"sag_in": c["sag"], "H": tension_for_sag(400.0, 0.0, 10.0, c["sag"])})


CABLE = {"EA": 30e6, "alpha": 20e-6, "mass": 1.6, "d": 0.028}


def textbook_cos():
    print("== textbook-change-of-state ==")
    w0 = CABLE["mass"] * G
    EA, al = CABLE["EA"], CABLE["alpha"]
    ice = wire_load(CABLE["mass"], CABLE["d"], 0.012)
    wind = wire_load(CABLE["mass"], CABLE["d"], q=600.0)
    for name, spans, w1, wind1, t1 in [
        ("tbcos-001 level 300 m, 10 -> 70 degC", [(300.0, 0.0)], w0, 0.0, 70.0),
        ("tbcos-002 level 300 m, 12 mm glaze ice at -5 degC", [(300.0, 0.0)], ice["vertical"], 0.0, -5.0),
        ("tbcos-003 level 300 m, 600 Pa wind at 10 degC", [(300.0, 0.0)], wind["resultant"],
         wind["horizontal"], 10.0),
        ("tbcos-004 level 300 m, 150 degC", [(300.0, 0.0)], w0, 0.0, 150.0),
        ("tbcos-005 inclined 300 m +50 m, 10 -> 80 degC", [(300.0, 50.0)], w0, 0.0, 80.0),
    ]:
        H, loads = cos_multiplicative(spans, EA, al, 25_000.0, w0, 10.0, w1, t1, wind1)
        S, h = spans[0]
        c = catenary_span(S, h, H, loads[0])
        # The hand check's cubic with the loads per stretched metre (the own
        # weight over 1 + H/EA when strung, the solution's load after).
        Hp = parabolic_cos(S, EA, al, 25_000.0, w0 / (1 + 25_000.0 / EA), 10.0, loads[0], t1)
        old = cos_unconserved(spans, EA, al, 25_000.0, w0, 10.0, w1, t1)
        show(name, {"load": w1, "load_per_stretched_m": loads[0], "horizontal_tension": H, "sag": c["sag"],
                    "length": c["length"], "tension_left": c["tension_left"],
                    "tension_right": c["tension_right"], "parabolic_cubic_H": Hp,
                    "before_2026_10_21_H": old, "change_pct": (H / old - 1) * 100})


def parabolic_cos(S, EA, alpha, H1, w1, t1, w2, t2):
    """The classical parabolic change-of-state cubic (level span):
    H2^2 [H2 - H1 + EA w1^2 S^2 / (24 H1^2) + EA alpha dt] = EA w2^2 S^2 / 24."""
    K = H1 - EA * w1**2 * S**2 / (24 * H1**2) - EA * alpha * (t2 - t1)
    M = EA * w2**2 * S**2 / 24
    return secant(lambda H: H * H * (H - K) - M, H1, H1 * 1.1)


def ruling_span_cases():
    print("== textbook-ruling-span ==")
    w0 = CABLE["mass"] * G
    EA, al = CABLE["EA"], CABLE["alpha"]
    spans = [(200.0, 0.0), (350.0, 0.0), (250.0, 0.0)]
    RS = math.sqrt(sum(S**3 for S, _ in spans) / sum(S for S, _ in spans))
    H_exact = cos_multiplicative(spans, EA, al, 25_000.0, w0, 10.0, w0, 80.0)[0]
    H_rs, loads = cos_multiplicative([(RS, 0.0)], EA, al, 25_000.0, w0, 10.0, w0, 80.0)
    H_par = parabolic_cos(RS, EA, al, 25_000.0, w0 / (1 + 25_000.0 / EA), 10.0, loads[0], 80.0)
    show("tbrs-001 section 200/350/250 m, 10 -> 80 degC (sagtension.section)",
         {"horizontal_tension": H_exact, "ruling_span": RS, "ruling_span_H": H_rs,
          "ruling_span_error_pct": (H_rs / H_exact - 1) * 100, "parabolic_ruling_span_H": H_par})
    for t in (80.0, -20.0):
        fe = fe_section_rs_case(spans, t)
        exact = cos_multiplicative(spans, EA, al, 25_000.0, w0, 10.0, w0, t)[0]
        for i in range(3):
            fe[f"span_{i}_vs_exact_pct"] = (fe[f"h_{i}_n"] / exact - 1) * 100
        fe["exact_equal_h_n"] = exact
        show(f"tbrs FE section with 3 m strings at {t} degC", fe)


def suspension_section(spans_x, top_z, wire_z, l0, k, W, mass, d, EA, alpha, H, t_s, case, wind_area=0.0):
    """Level section along x: dead-end clamps fixed at the ends (height
    wire_z), suspension strings at the inner points from top_z. Strung at H
    in still air at t_s; then solved in `case` (dict: t, q, ice, rho)."""
    w = mass * G
    xs = [0.0]
    for S in spans_x:
        xs.append(xs[-1] + S)
    n = len(xs)
    # Stringing: clamps plumb under the tops, stretched by their load.
    clamp_z = [wire_z] + [top_z - l0] * (n - 2) + [wire_z]
    for _ in range(50):
        old = list(clamp_z)
        for i in range(1, n - 1):
            def load(z, i=i):
                _, _, up_b = span_stringing_length([xs[i - 1], 0, clamp_z[i - 1]], [xs[i], 0, z], H, w, EA)
                _, up_a, _ = span_stringing_length([xs[i], 0, z], [xs[i + 1], 0, clamp_z[i + 1]], H, w, EA)
                return up_b + up_a
            clamp_z[i] = string_at_stringing([xs[i], 0, top_z], load, l0, k, W)
        if max(abs(a - b) for a, b in zip(old, clamp_z)) < 1e-13:
            break
    nodes = [[xs[i], 0.0, clamp_z[i]] for i in range(n)] + [[xs[i], 0.0, top_z] for i in range(1, n - 1)]
    lengths = [span_stringing_length(nodes[i], nodes[i + 1], H, w, EA)[0] for i in range(n - 1)]
    thermal = math.exp(alpha * (case["t"] - t_s))
    spans = []
    for i in range(n - 1):
        # Per unit of the thermally longer unstressed length: the own
        # weight w / thermal (conserved), ice and wind as they are.
        q = span_load_vector(mass, d, case, nodes[i], nodes[i + 1], weight_scale=1 / thermal)
        spans.append(Span(i, i + 1, lengths[i] * thermal, EA, q))
    bars = []
    p = case.get("q", 0.0)
    for j, i in enumerate(range(1, n - 1)):
        bars.append(Bar(n + j, i, l0, k, W, wind=(0.0, p * wind_area, 0.0)))
    free = list(range(1, n - 1))
    strung = [list(p) for p in nodes[:n]]
    start = [list(p) for p in nodes]
    if p > 0:
        # Start the clamps swung by the pendulum estimate (Newton from a
        # plumb string has no stiffness across the line yet).
        wl = wire_load(mass, d, case.get("ice", 0.0), case.get("rho", 913.0), p)
        for i in free:
            S = (spans_x[i - 1] + spans_x[i]) / 2
            phi = math.atan2(wl["horizontal"] * S + p * wind_area / 2, wl["vertical"] * S + W / 2)
            r = top_z - nodes[i][2]
            start[i] = [nodes[i][0], r * math.sin(phi), top_z - r * math.cos(phi)]
    pos = equilibrium(start, free, spans, bars)
    return pos, spans, bars, lengths, strung


def fe_section_rs_case(spans, t):
    mass, d, EA, alpha = CABLE["mass"], CABLE["d"], CABLE["EA"], CABLE["alpha"]
    pos, sps, bars, lengths, strung = suspension_section(
        [S for S, _ in spans], 33.0, 30.0, 3.0, 5e7, 600.0, mass, d, EA, alpha, 25_000.0, 10.0, {"t": t})
    out = {}
    for i, sp in enumerate(sps):
        out[f"h_{i}_n"] = horizontal_tension(sp, pos)
    for i in range(1, len(spans)):
        out[f"clamp_x_{i}_m"] = pos[i][0]
        out[f"clamp_z_{i}_m"] = pos[i][2]
    for j, br in enumerate(bars):
        N, u = br.force(pos)
        out[f"string_{j + 1}_0_n"] = N
        out[f"string_{j + 1}_0_along_deg"] = math.degrees(math.atan2(u[0], -u[2]))
    out["rs_line_h_n"] = rs_line(strung, mass, EA, alpha, 25_000.0, 10.0, t)
    return out


def rs_line(strung, mass, EA, alpha, H, t_s, t):
    """The line's ruling-span method: one H, each span a catenary between
    its clamps held where they hang in still air at stringing, the cable's
    linear stress-strain model (additive: eps = T/EA + alpha (t - 20 degC),
    20 degC the curves' reference temperature)."""
    w = mass * G
    spans = [(math.hypot(b[0] - a[0], b[1] - a[1]), b[2] - a[2]) for a, b in zip(strung, strung[1:])]
    strain = lambda T, tt: T / EA + alpha * (tt - 20.0)
    return cos_additive(spans, strain, H, w, t_s, w, t)[0]


class SpanH:
    """A span held at a horizontal tension H in still air (the stringing
    state): its forces on its end nodes for any clamp positions."""

    def __init__(self, a, b, H, w, EA):
        self.a, self.b, self.H, self.w, self.EA = a, b, H, w, EA
        self.q = [0.0, 0.0, -w]
        self.L0 = 1.0

    def forces(self, pos):
        a, b = pos[self.a], pos[self.b]
        L0, up_a, up_b = span_stringing_length(a, b, self.H, self.w, self.EA)
        self.L0 = L0
        hn = math.hypot(b[0] - a[0], b[1] - a[1])
        d = [(b[0] - a[0]) / hn, (b[1] - a[1]) / hn]
        F0 = [self.H * d[0], self.H * d[1], -up_a]
        F1 = [self.H * d[0], self.H * d[1], up_b]
        return F0, F1


def dead_end_span(S, z, mass, d, EA, alpha, H, t_s, case, stretch=0.0, conserved=True):
    """One span between fixed wire points (0, 0, z) and (S, 0, z), strung
    at H in still air at t_s; solved in `case`. `stretch`: a permanent
    stretch of the unstressed length (linear creep model). `conserved`:
    the own weight kept (False: the model before 2026-10-21, the weight per
    unit of the longer length, for comparison only)."""
    a, b = [0.0, 0.0, z], [S, 0.0, z]
    L0 = span_stringing_length(a, b, H, mass * G, EA)[0]
    k = (1 + stretch) * math.exp(alpha * (case["t"] - t_s))
    q = span_load_vector(mass, d, case, a, b, weight_scale=1 / k if conserved else 1.0)
    sp = Span(0, 1, L0 * k, EA, q)
    pos = [a, b]
    sp.forces(pos)
    mid, _ = mid_point(sp, pos)
    return {
        "L0": L0,
        "h_0_n": horizontal_tension(sp, pos),
        "max_tension_0_n": max_tension(sp, pos),
        "mid_y_0_m": mid[1],
        "mid_z_0_m": mid[2],
        "sag_0_m": z - mid[2],
        "blowout_deg": math.degrees(math.atan2(mid[1], z - mid[2])),
    }


def insulator_cases():
    print("== textbook-insulators ==")
    mass, d, EA, alpha = CABLE["mass"], CABLE["d"], CABLE["EA"], CABLE["alpha"]
    show("tbins-001 dead-end span 300 m, 500 Pa",
         dead_end_span(300.0, 30.0, mass, d, EA, alpha, 25_000.0, 10.0, {"t": 10.0, "q": 500.0}))
    wl = wire_load(mass, d, q=500.0)
    print(f"    wire load: vertical {fmt(wl['vertical'])} horizontal {fmt(wl['horizontal'])}"
          f" angle {fmt(wl['swing_deg'])}")
    # I-string: two spans of 300 m, string 3 m / 800 N / 0.4 m2.
    pos, sps, bars, lengths, strung = suspension_section(
        [300.0, 300.0], 33.0, 30.0, 3.0, 5e7, 800.0, mass, d, EA, alpha, 25_000.0, 10.0,
        {"t": 10.0, "q": 500.0}, wind_area=0.4)
    N, u = bars[0].force(pos)
    out = {"h_0_n": horizontal_tension(sps[0], pos), "h_1_n": horizontal_tension(sps[1], pos),
           "string_1_0_n": N, "string_1_0_across_deg": math.degrees(math.atan2(-u[1], -u[2])),
           "clamp_y_1_m": pos[1][1], "clamp_z_1_m": pos[1][2], "L0": lengths[0]}
    show("tbins-002 I-string 3 m, 800 N, 0.4 m2 in 500 Pa", out)
    # The classical pendulum estimate (no chord skew), for the hand check.
    sp_load = wl["horizontal"] * lengths[0]
    sp_w = wl["vertical"] * lengths[0]
    phi0 = math.degrees(math.atan2(sp_load + 500.0 * 0.4 / 2, sp_w + 800.0 / 2))
    print(f"    pendulum estimate (no chord skew): {fmt(phi0)} deg")
    for q, name in ((0.0, "tbins-003 V-string still air"), (300.0, "tbins-004 V-string 300 Pa"),
                    (1000.0, "tbins-005 V-string 1000 Pa")):
        show(name, v_section(q))


def v_section(q):
    """Two spans of 300 m from fixed dead-ends at (0, 0, 30), (600, 0, 30);
    a V at x = 300: legs from (300, +-2.5, 33) to the tip, unstressed
    length the drawn distance to (300, 0, 30), 400 N each, EA 5e7 N,
    tension only. Strung at 25 kN, 10 degC; solved at 10 degC with wind q."""
    mass, d, EA = CABLE["mass"], CABLE["d"], CABLE["EA"]
    w = mass * G
    l0 = math.sqrt(2.5**2 + 3.0**2)
    k, W = 5e7, 400.0
    # Stringing: the tip settles under the spans (symmetric: z only).
    tip_z = 30.0
    for _ in range(100):
        _, _, up_b = span_stringing_length([0, 0, 30.0], [300, 0, tip_z], 25_000.0, w, EA)
        V = 2 * up_b + 2 * W / 2

        def f(z, V=V):
            ll = math.hypot(2.5, 33 - z)
            return 2 * k / l0 * (ll - l0) * (33 - z) / ll - V

        new = bisect(f, 29.9, 30.0)
        if abs(new - tip_z) < 1e-14:
            break
        tip_z = new
    nodes = [[0.0, 0.0, 30.0], [300.0, 0.0, tip_z], [600.0, 0.0, 30.0], [300.0, 2.5, 33.0], [300.0, -2.5, 33.0]]
    lengths = [span_stringing_length(nodes[0], nodes[1], 25_000.0, w, EA)[0],
               span_stringing_length(nodes[1], nodes[2], 25_000.0, w, EA)[0]]
    case = {"t": 10.0, "q": q}
    spans = [Span(0, 1, lengths[0], EA, span_load_vector(mass, d, case, nodes[0], nodes[1])),
             Span(1, 2, lengths[1], EA, span_load_vector(mass, d, case, nodes[1], nodes[2]))]
    bars = [Bar(3, 1, l0, k, W), Bar(4, 1, l0, k, W)]
    start = [list(p) for p in nodes]
    T, V = q * d * lengths[0], w * lengths[0] + W
    if T / V > 2.5 / 3.0:
        # The leg towards the wind goes slack: start the tip on the other
        # leg's arc, along the load (Newton has no stiffness there yet).
        psi = math.atan2(T, V)
        start[1] = [300.0, -2.5 + l0 * math.sin(psi), 33.0 - l0 * math.cos(psi)]
    pos = equilibrium(start, [1], spans, bars)
    out = {"h_0_n": horizontal_tension(spans[0], pos), "clamp_y_1_m": pos[1][1], "clamp_z_1_m": pos[1][2],
           "tip_z_strung": tip_z, "L0": lengths[0]}
    for j, br in enumerate(bars):
        N, u = br.force(pos)
        out[f"string_1_{j}_n"] = N
        out[f"string_1_{j}_slack"] = 1.0 if N == 0 else 0.0
    # The spans' pull on the tip (for the hand check).
    _, F1a = spans[0].forces(pos)
    F0b, _ = spans[1].forces(pos)
    out["tip_load_y_n"] = -F1a[1] + F0b[1]
    out["tip_load_z_n"] = -F1a[2] + F0b[2]
    return out


def strain_strings_case():
    """TB 324 sec. 2.5: 300 m level span of Drake (15.97 N/m) at H = 28 kN
    between 2 m dead-end strings of 100 kg each (EA 5e7 N, the line's
    default), attachment points at (0, 0, 30) and (300, 0, 30)."""
    w, H, EA = 15.97, 28_000.0, 55e9 * 402.8e-6 + 190e9 * 65.8e-6
    W, l0, k = 100 * G, 2.0, 5e7
    nodes = [[2.0, 0.0, 29.8], [298.0, 0.0, 29.8], [0.0, 0.0, 30.0], [300.0, 0.0, 30.0]]
    spans = [SpanH(0, 1, H, w, EA)]
    bars = [Bar(2, 0, l0, k, W), Bar(3, 1, l0, k, W)]
    pos = equilibrium(nodes, [0, 1], spans, bars)
    spans[0].forces(pos)
    sp = Span(0, 1, spans[0].L0, EA, [0.0, 0.0, -w])
    mid, _ = mid_point(sp, pos)
    return {"clamp_x_0_m": pos[0][0], "clamp_z_0_m": pos[0][2], "low_z_0_m": mid[2],
            "sag_from_attachments_m": 30.0 - mid[2], "string_0_0_n": bars[0].force(pos)[0]}


def creep_cases():
    print("== creep as an equivalent temperature ==")
    mass, d, EA, alpha = CABLE["mass"], CABLE["d"], CABLE["EA"], CABLE["alpha"]
    shift = 25.0
    fe = dead_end_span(300.0, 30.0, mass, d, EA, alpha, 25_000.0, 10.0, {"t": 10.0}, stretch=alpha * shift)
    warm = dead_end_span(300.0, 30.0, mass, d, EA, alpha, 25_000.0, 10.0, {"t": 10.0 + shift})
    show("tbcreep-001 creep-fe at 10 degC (25 degC shift)",
         {"h_0_n": fe["h_0_n"], "sag_0_m": fe["sag_0_m"], "initial_at_35_h": warm["h_0_n"],
          "initial_at_35_sag": warm["sag_0_m"]})
    # RS: the strung span (clamps at 30 m), the linear model's additive
    # strain, the unstressed length times (1 + stretch). The own weight is
    # per metre of the cable unstressed at stringing (10 degC), the length
    # L0 (1 + eps(0, 10)); the stretch adds length, not weight: per metre of
    # stretched cable w (1 + eps(0, 10)) / ((1 + stretch)(1 + eps)) on the
    # stretched zero-strain length.
    w = mass * G
    stretch = alpha * shift

    def strain(T, tt):
        return T / EA + alpha * (tt - 20.0)

    m = strain(0.0, 10.0)

    def span(H, s):
        k_of = lambda c: (1 + s) * (1 + strain(c["mean_tension"], 10.0)) / (1 + m)
        c, _, k = conserved_span(300.0, 0.0, H, w, 0.0, 0.0, k_of)
        return c, c["length"] / (1 + strain(c["mean_tension"], 10.0))

    L0 = span(25_000.0, 0.0)[1] * (1 + stretch)
    H = math.exp(secant(lambda x: span(math.exp(x), stretch)[1] - L0, math.log(25_000.0), math.log(22_000.0)))
    show("tbcreep-002 creep-rs at 10 degC", {"h_0_n": H, "sag_0_m": span(H, stretch)[0]["sag"],
                                             "fe_minus_rs_n": fe["h_0_n"] - H})


def heated_span_cases():
    """tbheat-001...003: a level 400 m span between fixed wire points at
    30 m, strung at 25 kN at 10 degC, at 150 degC: the closed-form change of
    state, the finite elements and the line's ruling-span method. The span
    weighs w L0 in each (L0 as strung at 10 degC): for a level span each
    support carries half of it, 2 sqrt(Tmax^2 - H^2) = w L0."""
    print("== heated span, total weight constant ==")
    mass, d, EA, alpha = CABLE["mass"], CABLE["d"], CABLE["EA"], CABLE["alpha"]
    w = mass * G
    H_cf, loads = cos_multiplicative([(400.0, 0.0)], EA, alpha, 25_000.0, w, 10.0, w, 150.0)
    c = catenary_span(400.0, 0.0, H_cf, loads[0])
    L0_cf = conserved_span(400.0, 0.0, 25_000.0, w, 0.0, 0.0, lambda c: 1 + c["mean_tension"] / EA)
    show("tbheat-001 closed-form change of state (sagtension.change_of_state)",
         {"horizontal_tension": H_cf, "sag": c["sag"], "length": c["length"], "tension_left": c["tension_left"],
          "total_weight": 2 * math.sqrt(c["tension_left"] ** 2 - H_cf**2),
          "w_L0": w * L0_cf[0]["length"] / L0_cf[2],
          "before_2026_10_21_H": cos_unconserved([(400.0, 0.0)], EA, alpha, 25_000.0, w, 10.0, w, 150.0)})
    fe = dead_end_span(400.0, 30.0, mass, d, EA, alpha, 25_000.0, 10.0, {"t": 150.0})
    old = dead_end_span(400.0, 30.0, mass, d, EA, alpha, 25_000.0, 10.0, {"t": 150.0}, conserved=False)
    fe["total_weight"] = 2 * math.sqrt(fe["max_tension_0_n"] ** 2 - fe["h_0_n"] ** 2)
    fe["w_L0"] = w * fe["L0"]
    fe["before_2026_10_21_h"] = old["h_0_n"]
    fe["before_2026_10_21_sag"] = old["sag_0_m"]
    fe["before_2026_10_21_total_weight"] = 2 * math.sqrt(old["max_tension_0_n"] ** 2 - old["h_0_n"] ** 2)
    show("tbheat-002 finite elements (line.section, initial-fe)", fe)
    strain = lambda T, tt: T / EA + alpha * (tt - 20.0)
    H_rs, loads = cos_additive([(400.0, 0.0)], strain, 25_000.0, w, 10.0, w, 150.0)
    c = catenary_span(400.0, 0.0, H_rs, loads[0])
    show("tbheat-003 the line's ruling-span method (line.section, initial-rs)",
         {"h_0_n": H_rs, "sag_0_m": c["sag"], "max_tension_0_n": c["tension_left"],
          "total_weight": 2 * math.sqrt(c["tension_left"] ** 2 - H_rs**2),
          "rs_minus_fe_n": H_rs - fe["h_0_n"]})


def irvine(H, V0, L0, EA, w):
    """Irvine's closed-form elastic catenary (for the hand check only):
    V0 the start force's vertical component (negative: going down)."""
    V1 = V0 + w * L0
    x = H * L0 / EA + H / w * (math.asinh(V1 / H) - math.asinh(V0 / H))
    z = (V0 * L0 + w * L0**2 / 2) / EA + (math.hypot(H, V1) - math.hypot(H, V0)) / w
    return x, z


def elastic_catenary_cases():
    print("== textbook-elastic-catenary ==")
    EA, w = CABLE["EA"], CABLE["mass"] * G
    for name, chord, L0 in [
        ("tbirv-001 level 400 m, L0 = 400.5 m", (400.0, 0.0), 400.5),
        ("tbirv-002 inclined 300 m +60 m, L0 = 306.5 m", (300.0, 60.0), 306.5),
        ("tbirv-003 taut 50 m, L0 = 49.98 m", (50.0, 0.0), 49.98),
    ]:
        F0, F1 = elastic_span(L0, EA, list(chord), [0.0, -w])
        H = F0[0]
        x, z = irvine(H, F0[1], L0, EA, w)
        show(name, {"horizontal_n": H, "tension_i_n": norm(F0), "tension_j_n": norm(F1), "V_i_n": -F0[1],
                    "irvine_residual_x": x - chord[0], "irvine_residual_z": z - chord[1]})


def point_load_cases():
    print("== textbook-point-load ==")
    for name, S, h, H, w, loads in [
        ("tbpl-001 level 300 m, 2 kN at mid-span", 300.0, 0.0, 20_000.0, 10.0, [(150.0, 2000.0)]),
        ("tbpl-002 inclined 300 m +30 m, 1.5 kN at 100 m", 300.0, 30.0, 20_000.0, 10.0, [(100.0, 1500.0)]),
    ]:
        r = loaded_span(S, h, H, w, loads)
        show(name, {"length": r["length"], "sag": r["sag"], "tension_left": r["tension_left"],
                    "tension_right": r["tension_right"], "slope_jump": r["jumps"][0][0], "slope_before": r["jumps"][0][1], "slope_after": r["jumps"][0][2],
                    "V_left": r["V_left"], "V_right": r["V_right"]})


def bimetallic_cases():
    print("== textbook-bimetallic ==")
    EAo, ao = 55e9 * 402.8e-6, 23e-6
    EAc, ac = 190e9 * 65.8e-6, 11.5e-6
    w = 1.6285 * G
    strain = bimetallic_strain(EAo, ao, EAc, ac, 0.0)

    def state(t):
        H, loads = cos_additive([(300.0, 0.0)], strain, 28_000.0, w, 20.0, w, t)
        c = catenary_span(300.0, 0.0, H, loads[0])
        dt = t - 20.0
        eps = (c["mean_tension"] + EAo * ao * dt + EAc * ac * dt) / (EAo + EAc)
        return H, c, EAo * (eps - ao * dt)

    for t in (60.0, 150.0):
        H, c, outer = state(t)
        show(f"tbbim at {t} degC", {"h_initial_n": H, "sag_initial_m": c["sag"],
                                    "outer_load_n (both in tension)": outer})
    knee = bisect(lambda t: state(t)[2], 60.0, 150.0)
    print(f"    knee temperature {fmt(knee)} degC")


def weather_cases():
    print("== textbook-ice-wind ==")
    for name, args in [
        ("tbwx-001 12 mm glaze ice", dict(t=0.012)),
        ("tbwx-002 25 mm rime 500 kg/m3 + 300 Pa", dict(t=0.025, rho=500.0, q=300.0)),
        ("tbwx-003 bare, 600 Pa", dict(q=600.0)),
    ]:
        show(name, wire_load(CABLE["mass"], CABLE["d"], **args))


def tb324_cases():
    print("== cigre-tb324 ==")
    c = catenary_span(300.0, 0.0, 28_000.0, 15.97)
    show("tb324-001 sec. 2.1-2.4 Drake 300 m, 28 kN, 15.97 N/m",
         {k: c[k] for k in ("sag", "length", "tension_left", "vertical_left")})
    print(f"    parabola: {fmt(15.97 * 300**2 / (8 * 28_000))}  slack {fmt(c['length'] - 300)}")
    show("tb324-002 sec. 2.3 tension for sag 7.241 m", {"H": tension_for_sag(300.0, 0.0, 15.97, 7.241)})
    for h in range(5, 45, 5):
        c = catenary_span(300.0, float(h), 28_000.0, 15.96)
        show(f"tb324 Table 2 h = {h} m", {"Xr (low_point_x)": c["low_point_x"], "Tr (left)": c["tension_left"],
                                          "Tl (right)": c["tension_right"], "Vr": c["vertical_left"],
                                          "Vl": c["vertical_right"]})
    show("tb324 sec. 2.5 dead-end strings 2 m, 100 kg", strain_strings_case())
    EA = 55e9 * 402.8e-6 + 190e9 * 65.8e-6
    show("tb324 sec. 5.2.3/5.2.4 Drake composite",
         {"E_GPa": EA / 468.6e-6 / 1e9, "alpha": (55e9 * 402.8e-6 * 23e-6 + 190e9 * 65.8e-6 * 11.5e-6) / EA,
          "alpha_with_E_74": 23e-6 * (55 / 74) * (402.8 / 468.6) + 11.5e-6 * (190 / 74) * (65.8 / 468.6)})
    show("tb324 sec. 5.2.1 A1-37 at 12 kN",
         {"strain": 12_000 / (58.9e9 * 402.9e-6), "on 300 m": 300 * 12_000 / (58.9e9 * 402.9e-6)})
    wl = wire_load(10.89 / G, 0.0261, q=430.0)
    show("tb324 sec. 6.1 430 Pa on 26.1 mm", {k: wl[k] for k in ("vertical", "horizontal", "resultant")})
    show("tb324 sec. 6.1 initial sag", {"sag": catenary_span(300.0, 0.0, 15_330.0, 10.89)["sag"]})
    EA = 58.9e9 * 403e-6
    # The 15.6 N/m resultant: the own weight 10.89 N/m and the wind (its
    # horizontal part, the rest).
    wind = math.sqrt(15.6**2 - 10.89**2)
    Hm, loads = cos_multiplicative([(300.0, 0.0)], EA, 23e-6, 15_330.0, 10.89, 15.0, 15.6, 15.0, wind)
    Ht = cos_tb324(300.0, EA, 23e-6, 15_330.0, 10.89, 15.0, 15.6, 15.0)
    Hu = cos_unconserved([(300.0, 0.0)], EA, 23e-6, 15_330.0, 10.89, 15.0, 15.6, 15.0)
    show("tb324 sec. 6.1 Arbutus LE, 15.6 N/m",
         {"wind_n_per_m": wind, "H_code_model": Hm, "H_tb324_model": Ht,
          "code_minus_tb324_n": Hm - Ht, "H_code_before_2026_10_21": Hu,
          "sag_code": catenary_span(300.0, 0.0, Hm, loads[0])["sag"],
          "strain_increase": (Ht - 15_330.0) / EA})


def tb324_not_used():
    """The numbers behind the TB 324 examples not turned into cases
    (docs/validation/textbook.md)."""
    print("== cigre-tb324, examples not used ==")
    EA = 55e9 * 402.8e-6 + 190e9 * 65.8e-6
    alpha = (55e9 * 402.8e-6 * 23e-6 + 190e9 * 65.8e-6 * 11.5e-6) / EA
    # Table 3 (sec. 3.1): 250 m and 350 m dead-end spans strung at 35 kN,
    # 15 degC, at 100 degC — linear elastic with the brochure's Drake data.
    show("Table 3, linear Drake (printed 21177 N and 24670 N)",
         {f"{S} m": cos_multiplicative([(float(S), 0.0)], EA, alpha, 35_000.0, 15.97, 15.0, 15.97, 100.0)[0]
          for S in (250, 350)})
    # Sec. 6.2: SPE, strung at 48 MPa (22.49 kN) at 15 degC, 600 µstrain
    # of plastic elongation, then 15 degC and 100 degC.
    H0, w = 48e6 * 468.6e-6, 15.97
    out = {}
    # The own weight conserved: per metre of the cable unstressed at
    # 15 degC; the plastic elongation adds length, not weight.
    strung = conserved_span(300.0, 0.0, H0, w, 0.0, 0.0, lambda c: 1 + c["mean_tension"] / EA)
    u = strung[0]["length"] / strung[2] * (1 + 600e-6)
    for t in (15.0, 100.0):
        k_t = (1 + 600e-6) * math.exp(alpha * (t - 15.0))

        def at(H, k_t=k_t):
            c, _, k = conserved_span(300.0, 0.0, H, w, 0.0, 0.0, lambda c: k_t * (1 + c["mean_tension"] / EA))
            return c, c["length"] / k * (1 + 600e-6)

        H = math.exp(secant(lambda x: at(math.exp(x))[1] - u, math.log(H0 * 0.9), math.log(H0 * 0.8)))
        out[f"H at {t} degC (printed {'20.1' if t == 15 else '15.9'} kN)"] = H
        out[f"sag at {t} degC (printed {'8.95' if t == 15 else '11.3'} m)"] = at(H)[0]["sag"]
    show("Sec. 6.2 SPE, Drake 300 m", out)


def self_check():
    """The largest relative change of a few results from 64 to 96 nodes."""

    def sample():
        return [catenary_span(1000.0, 0.0, 10_000.0, 10.0)["sag"],
                catenary_span(300.0, 60.0, 10_000.0, 10.0)["low_point_x"],
                elastic_span(306.5, 30e6, [300.0, 60.0], [0.0, -15.69064])[0][0],
                loaded_span(300.0, 30.0, 20_000.0, 10.0, [(100.0, 1500.0)])["sag"]]

    a = sample()
    ORDER[0] = 96
    b = sample()
    ORDER[0] = 64
    print(f"quadrature self-check (64 vs 96 nodes): largest relative change "
          f"{max(abs(x / y - 1) for x, y in zip(a, b)):.1e}")


def main():
    self_check()
    textbook_catenary()
    textbook_cos()
    creep_cases()
    heated_span_cases()
    ruling_span_cases()
    point_load_cases()
    bimetallic_cases()
    weather_cases()
    elastic_catenary_cases()
    insulator_cases()
    tb324_cases()
    tb324_not_used()
    print(f"largest unbalanced nodal force of the sections solved: {max(RESIDUALS):.1e} N")


if __name__ == "__main__":
    main()
