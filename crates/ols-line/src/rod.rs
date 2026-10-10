// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A jumper with bending stiffness: an inextensible elastic rod between two
//! fixed ends, optionally clamped in a direction at each end, optionally
//! held at points along it by rigid, tension-only strings.
//!
//! - **Discretisation**: n straight pieces of lengths ℓ₀…ℓₙ₋₁ with unit
//!   directions d₀…dₙ₋₁; node m is at \\( A + \sum_{i<m} ℓ_i d_i \\). A
//!   support clamp is one piece of its own length: rigid and straight, so
//!   the rod leaves it in the direction it enters.
//! - **Energy**: the distributed load q (weight, ice, wind; per metre) as
//!   a potential, bending \\( c_i (1 - d_i \cdot d_{i+1}) \\) at every inner
//!   node with \\( c_i = EI / \bar ℓ \\) (\\( \bar ℓ \\) the mean of the two
//!   pieces), and \\( 2 EI / ℓ (1 - d \cdot e) \\) at a clamped end (e: the
//!   direction the clamp holds; the end piece's direction is the tangent
//!   half a piece from it, as for a beam's clamped end); weights hanging at
//!   held points.
//! - **Constraints**: the far end is at B (\\( \sum ℓ_i d_i = B - A \\)); a
//!   taut string keeps its held point at the string's length from its top.
//!   A held point is on a piece, a share of its length from its start (a
//!   node: 0; a clamp's middle: ½), so the strings turn a clamp too.
//! - **Strings** (active set): a string is left out while its held point
//!   is nearer its top than its length, and let go when its force would
//!   push; one going taut is shortened to its length in steps.
//! - **Solution**: Newton on the Lagrangian's stationarity, each direction
//!   rotated by a 2-vector in its normal plane (lengths stay exact, so
//!   there is no fake stretch of short stiff pieces), Levenberg–Marquardt
//!   damping and a backtracking line search.
//!
//! The cable's stretch is left out: a jumper's tension (tens to hundreds
//! of newtons) stretches it by ~10⁻⁶.

use nalgebra::{DMatrix, DVector, Vector3};
use ols_core::budget::Budget;

type V = Vector3<f64>;

/// A rigid, tension-only string: fixed top, length.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RodString {
    pub top: V,
    pub length: f64,
}

/// A point of the rod held by strings: on piece `piece`, `offset` of its
/// length from the piece's start; `load` acts there (N: the weight
/// hanging there, the wind on its strings).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RodHold {
    pub piece: usize,
    pub offset: f64,
    pub load: V,
    pub strings: Vec<RodString>,
}

/// The rod to solve.
#[derive(Debug, Clone)]
pub(crate) struct Rod {
    /// First end.
    pub a: V,
    /// Second end.
    pub b: V,
    /// Pieces' (unstressed) lengths, A to B, m.
    pub pieces: Vec<f64>,
    /// Load per metre, N/m.
    pub q: V,
    /// Bending stiffness EI, N·m².
    pub ei: f64,
    /// Direction the rod leaves A in, when A is clamped.
    pub start: Option<V>,
    /// Direction the rod arrives at B in, when B is clamped.
    pub end: Option<V>,
    /// Points held by strings.
    pub holds: Vec<RodHold>,
}

/// A solved held point.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HoldShape {
    /// Where it is.
    pub point: V,
    /// Its piece's direction.
    pub direction: V,
    /// Its strings' forces, N (0: slack).
    pub forces: Vec<f64>,
}

/// The solved rod.
#[derive(Debug, Clone)]
pub(crate) struct RodShape {
    /// Nodes, A to B.
    pub points: Vec<V>,
    /// Largest tension, N.
    pub max_tension: f64,
    /// The held points, as the rod's.
    pub holds: Vec<HoldShape>,
    /// Piece directions (to start the next solve from).
    pub directions: Vec<V>,
}

fn basis(d: V) -> (V, V) {
    let helper = if d.x.abs() < 0.6 { V::x() } else { V::y() };
    let e1 = d.cross(&helper).normalize();
    (e1, d.cross(&e1))
}

impl Rod {
    /// A rod of `n` equal pieces.
    #[cfg(test)]
    pub(crate) fn uniform(a: V, b: V, length: f64, n: usize, q: V, ei: f64) -> Rod {
        Rod {
            a,
            b,
            pieces: vec![length / n as f64; n],
            q,
            ei,
            start: None,
            end: None,
            holds: Vec::new(),
        }
    }

    fn n(&self) -> usize {
        self.pieces.len()
    }

    /// Total length, m.
    fn length(&self) -> f64 {
        self.pieces.iter().sum()
    }

    /// Mean piece length, m.
    fn mean_piece(&self) -> f64 {
        self.length() / self.n() as f64
    }

    /// Every string, as (hold, string) in order.
    fn strings(&self) -> Vec<(usize, usize)> {
        self.holds
            .iter()
            .enumerate()
            .flat_map(|(h, x)| (0..x.strings.len()).map(move |s| (h, s)))
            .collect()
    }

    fn string(&self, (h, s): (usize, usize)) -> RodString {
        self.holds[h].strings[s]
    }

    /// Bending coefficient at the inner node after piece i: EI over the
    /// mean of the two pieces.
    fn bend(&self, i: usize) -> f64 {
        2.0 * self.ei / (self.pieces[i] + self.pieces[i + 1])
    }

    /// How far a held point moves per unit of piece i's direction.
    fn coef(&self, hold: &RodHold, i: usize) -> f64 {
        if i < hold.piece {
            self.pieces[i]
        } else if i == hold.piece {
            self.pieces[i] * hold.offset
        } else {
            0.0
        }
    }

    /// The lengths after each piece.
    fn tails(&self) -> Vec<f64> {
        let mut t = vec![0.0; self.n()];
        for i in (0..self.n().saturating_sub(1)).rev() {
            t[i] = t[i + 1] + self.pieces[i + 1];
        }
        t
    }

    fn nodes(&self, d: &[V]) -> Vec<V> {
        let mut out = Vec::with_capacity(d.len() + 1);
        let mut x = self.a;
        out.push(x);
        for (di, l) in d.iter().zip(&self.pieces) {
            x += di * *l;
            out.push(x);
        }
        out
    }

    fn held(&self, hold: &RodHold, nodes: &[V], d: &[V]) -> V {
        nodes[hold.piece] + d[hold.piece] * (self.pieces[hold.piece] * hold.offset)
    }

    /// From a string's top to its held point.
    fn arm(&self, c: (usize, usize), nodes: &[V], d: &[V]) -> V {
        self.held(&self.holds[c.0], nodes, d) - self.string(c).top
    }

    /// Potential energy of the loads plus bending energy, J.
    fn energy(&self, d: &[V]) -> f64 {
        let n = self.n();
        let nodes = self.nodes(d);
        let mut e = 0.0;
        for i in 0..n {
            e -= self.q.dot(&(nodes[i] + nodes[i + 1])) * 0.5 * self.pieces[i];
        }
        for i in 0..n.saturating_sub(1) {
            e += self.bend(i) * (1.0 - d[i].dot(&d[i + 1]));
        }
        if let Some(a) = self.start {
            e += 2.0 * self.ei / self.pieces[0] * (1.0 - d[0].dot(&a));
        }
        if let Some(b) = self.end {
            e += 2.0 * self.ei / self.pieces[n - 1] * (1.0 - d[n - 1].dot(&b));
        }
        for h in &self.holds {
            e -= h.load.dot(&self.held(h, &nodes, d));
        }
        e
    }

    /// Exact (L1) penalty merit: energy plus ρ times the constraints'
    /// violation (far end, taut strings).
    fn merit(&self, d: &[V], rho: f64, active: &[(usize, usize)]) -> f64 {
        let nodes = self.nodes(d);
        let mut m = self.energy(d) + rho * (nodes[self.n()] - self.b).norm();
        for &c in active {
            let s = self.string(c);
            let r = self.arm(c, &nodes, d).norm_squared() - s.length * s.length;
            m += rho * r.abs() / (2.0 * s.length);
        }
        m
    }

    /// ∂L/∂dᵢ for multipliers λ (end) and μ (strings in `active`, in
    /// order).
    fn lagrangian_gradient(
        &self,
        d: &[V],
        lambda: V,
        mu: &[f64],
        active: &[(usize, usize)],
    ) -> Vec<V> {
        let n = self.n();
        let nodes = self.nodes(d);
        let tails = self.tails();
        let mut g: Vec<V> = (0..n)
            .map(|i| {
                let l = self.pieces[i];
                -self.q * (l * (0.5 * l + tails[i])) - lambda * l
            })
            .collect();
        for i in 0..n.saturating_sub(1) {
            let c = self.bend(i);
            g[i] -= d[i + 1] * c;
            g[i + 1] -= d[i] * c;
        }
        if let Some(e) = self.start {
            g[0] -= e * (2.0 * self.ei / self.pieces[0]);
        }
        if let Some(e) = self.end {
            g[n - 1] -= e * (2.0 * self.ei / self.pieces[n - 1]);
        }
        for h in &self.holds {
            let w = h.load;
            for (i, gi) in g.iter_mut().enumerate().take(h.piece + 1) {
                *gi -= w * self.coef(h, i);
            }
        }
        for (&c, m) in active.iter().zip(mu) {
            let h = &self.holds[c.0];
            let r = self.arm(c, &nodes, d);
            for (i, gi) in g.iter_mut().enumerate().take(h.piece + 1) {
                *gi -= r * (2.0 * m * self.coef(h, i));
            }
        }
        g
    }

    /// Residual: tangential gradients (2 per piece), end gap (3), strings
    /// (1 per taut string).
    fn residual(&self, d: &[V], lambda: V, mu: &[f64], active: &[(usize, usize)]) -> DVector<f64> {
        let n = self.n();
        let g = self.lagrangian_gradient(d, lambda, mu, active);
        let mut r = DVector::zeros(2 * n + 3 + active.len());
        for i in 0..n {
            let (e1, e2) = basis(d[i]);
            r[2 * i] = e1.dot(&g[i]);
            r[2 * i + 1] = e2.dot(&g[i]);
        }
        let nodes = self.nodes(d);
        let gap = nodes[n] - self.b;
        for k in 0..3 {
            r[2 * n + k] = -gap[k];
        }
        for (k, &c) in active.iter().enumerate() {
            let s = self.string(c);
            r[2 * n + 3 + k] = -(self.arm(c, &nodes, d).norm_squared() - s.length * s.length);
        }
        r
    }

    /// The residual's Jacobian (symmetric).
    fn jacobian(&self, d: &[V], lambda: V, mu: &[f64], active: &[(usize, usize)]) -> DMatrix<f64> {
        let n = self.n();
        let g = self.lagrangian_gradient(d, lambda, mu, active);
        let size = 2 * n + 3 + active.len();
        let mut j = DMatrix::zeros(size, size);
        let bases: Vec<(V, V)> = d.iter().map(|x| basis(*x)).collect();
        let e = |i: usize, a: usize| if a == 0 { bases[i].0 } else { bases[i].1 };
        for i in 0..n {
            let l = self.pieces[i];
            // Rotating dᵢ: its second derivative is −dᵢ.
            let curvature = d[i].dot(&g[i]);
            for a in 0..2 {
                j[(2 * i + a, 2 * i + a)] -= curvature;
                // The far end: ∂(Σ ℓ d)/∂uᵢ = ℓᵢ eᵢ.
                for k in 0..3 {
                    j[(2 * i + a, 2 * n + k)] -= l * e(i, a)[k];
                    j[(2 * n + k, 2 * i + a)] -= l * e(i, a)[k];
                }
            }
            // Bending between neighbours: −c I.
            if i + 1 < n {
                let c = self.bend(i);
                for a in 0..2 {
                    for b in 0..2 {
                        let v = -c * e(i, a).dot(&e(i + 1, b));
                        j[(2 * i + a, 2 * (i + 1) + b)] += v;
                        j[(2 * (i + 1) + b, 2 * i + a)] += v;
                    }
                }
            }
        }
        let nodes = self.nodes(d);
        for (k, (&c, &m)) in active.iter().zip(mu).enumerate() {
            let h = &self.holds[c.0];
            let r = self.arm(c, &nodes, d);
            let col = 2 * n + 3 + k;
            for i in 0..=h.piece {
                let ci = self.coef(h, i);
                for a in 0..2 {
                    let v = -2.0 * ci * e(i, a).dot(&r);
                    j[(2 * i + a, col)] += v;
                    j[(col, 2 * i + a)] += v;
                    // −μ |r|²: ∂²/∂dᵢ∂dₖ = 2 cᵢ cₖ I.
                    for kk in 0..=h.piece {
                        let ck = self.coef(h, kk);
                        for b in 0..2 {
                            j[(2 * i + a, 2 * kk + b)] -=
                                2.0 * m * ci * ck * e(i, a).dot(&e(kk, b));
                        }
                    }
                }
            }
        }
        j
    }

    /// The end force λ and string multipliers μ that best balance the
    /// pieces at directions `d` (least squares on the tangential residual,
    /// which is affine in them): after a damped step, the multipliers of
    /// the Newton step can be far off and spoil the next Jacobian.
    fn multipliers(&self, d: &[V], active: &[(usize, usize)]) -> (V, Vec<f64>) {
        let n = self.n();
        let zeros = vec![0.0; active.len()];
        let base = self
            .residual(d, V::zeros(), &zeros, active)
            .rows(0, 2 * n)
            .into_owned();
        let k = 3 + active.len();
        let mut a = DMatrix::zeros(2 * n, k);
        for c in 0..k {
            let (mut l, mut m) = (V::zeros(), zeros.clone());
            if c < 3 {
                l[c] = 1.0;
            } else {
                m[c - 3] = 1.0;
            }
            let col = self.residual(d, l, &m, active).rows(0, 2 * n) - &base;
            a.set_column(c, &col);
        }
        let ata = a.transpose() * &a;
        let atb = a.transpose() * -base;
        match ata.lu().solve(&atb) {
            Some(x) if x.iter().all(|v| v.is_finite()) => (
                V::new(x[0], x[1], x[2]),
                x.iter().skip(3).copied().collect(),
            ),
            _ => (V::zeros(), zeros),
        }
    }

    /// The largest tension along the rod (at the pieces' middles).
    fn max_tension(&self, d: &[V], lambda: V, mu: &[f64], active: &[(usize, usize)]) -> f64 {
        let nodes = self.nodes(d);
        let mut max: f64 = 0.0;
        for (i, (l, tail)) in self.pieces.iter().zip(self.tails()).enumerate() {
            let l = *l;
            let mut f = self.q * (0.5 * l + tail) + lambda;
            for h in &self.holds {
                f += h.load * (self.coef(h, i) / l);
            }
            for (&c, m) in active.iter().zip(mu) {
                let h = &self.holds[c.0];
                f += self.arm(c, &nodes, d) * (2.0 * m * self.coef(h, i) / l);
            }
            max = max.max(f.norm());
        }
        max
    }

    /// Solves from the directions `start` (one per piece), in coordinates
    /// from A (in project coordinates, ~10⁶ m, rounding swamps the energy
    /// differences and the tolerances: 28 of a real line's jumpers stalled
    /// at 1–1000× the tolerance).
    ///
    /// The strings are first left out. A string whose held point then
    /// hangs farther from its top than its length goes taut: it is
    /// shortened to its length in steps, each solve starting from the last
    /// (a string pulling the rod up from far in one go did not converge in
    /// testing). A taut string that would push is let go. This repeats
    /// until no string changes.
    ///
    /// Expected cost: 5–30 Newton iterations on a dense system of 2n + 3
    /// (+ taut strings) unknowns (n ≈ 24: ~50 µs each in release), a few
    /// times over with strings. Exit clauses: the budget's iteration and
    /// time limits for each solve, string steps no smaller than 1/64 of the
    /// way, 8 rounds of strings going taut or slack; an error names the
    /// residual.
    pub(crate) fn solve(&self, start: &[V], budget: &Budget) -> Result<RodShape, String> {
        let local = Rod {
            a: V::zeros(),
            b: self.b - self.a,
            holds: self
                .holds
                .iter()
                .map(|h| RodHold {
                    strings: h
                        .strings
                        .iter()
                        .map(|s| RodString {
                            top: s.top - self.a,
                            ..*s
                        })
                        .collect(),
                    ..h.clone()
                })
                .collect(),
            ..self.clone()
        };
        let mut shape = local.solve_local(start, budget)?;
        for p in &mut shape.points {
            *p += self.a;
        }
        for h in &mut shape.holds {
            h.point += self.a;
        }
        Ok(shape)
    }

    fn solve_local(&self, start: &[V], budget: &Budget) -> Result<RodShape, String> {
        let n = self.n();
        if start.len() != n || n < 2 {
            return Err(format!(
                "rod: {} start directions for {n} pieces",
                start.len()
            ));
        }
        let total = self.length();
        for h in &self.holds {
            // The held point is `along` from A: out of reach when a
            // string's top is farther from either end than that plus the
            // string's length.
            let along: f64 =
                self.pieces[..h.piece].iter().sum::<f64>() + self.pieces[h.piece] * h.offset;
            for s in &h.strings {
                if (s.top - self.a).norm() > along + s.length
                    || (s.top - self.b).norm() > total - along + s.length
                {
                    return Err(format!(
                        "the support string cannot reach the jumper's point {along:.2} m along it"
                    ));
                }
            }
        }
        let all = self.strings();
        let (mut d, mut lambda, mut mu) = self.newton(start, &[], budget)?;
        let mut active: Vec<(usize, usize)> = Vec::new();
        let force_scale =
            self.q.norm() * total + self.holds.iter().map(|h| h.load.norm()).sum::<f64>();
        for round in 0.. {
            let nodes = self.nodes(&d);
            // Taut strings that would push: let go.
            let kept: Vec<(usize, usize)> = active
                .iter()
                .zip(&mu)
                .filter(|(c, m)| -2.0 * **m * self.string(**c).length >= -1e-9 * force_scale)
                .map(|(c, _)| *c)
                .collect();
            // Slack strings their points hang beyond: taut.
            let newly: Vec<((usize, usize), f64)> = all
                .iter()
                .filter(|c| !active.contains(c))
                .map(|&c| (c, self.arm(c, &nodes, &d).norm()))
                .filter(|&(c, far)| far > self.string(c).length * (1.0 + 1e-9))
                .collect();
            if kept.len() == active.len() && newly.is_empty() {
                break;
            }
            if round == 8 {
                return Err(
                    "the support strings do not settle (taut and slack in turn 8 times)".into(),
                );
            }
            let mut next = kept;
            next.extend(newly.iter().map(|(c, _)| *c));
            next.sort_unstable();
            if newly.is_empty() {
                let (dd, l, m) = self.newton(&d, &next, budget)?;
                (d, lambda, mu) = (dd, l, m);
            } else {
                let (mut done, mut step) = (0.0_f64, 0.25_f64);
                let mut m = Vec::new();
                while done < 1.0 {
                    let target = (done + step).min(1.0);
                    let mut rod = self.clone();
                    for &(c, far) in &newly {
                        let s = &mut rod.holds[c.0].strings[c.1];
                        s.length = far + (s.length - far) * target;
                    }
                    match rod.newton(&d, &next, budget) {
                        Ok(r) => {
                            (d, lambda, m) = r;
                            done = target;
                            step = (step * 2.0).min(0.5);
                        }
                        Err(e) => {
                            step /= 2.0;
                            if step < 1.0 / 64.0 {
                                return Err(format!("support string: {e}"));
                            }
                        }
                    }
                }
                mu = m;
            }
            active = next;
        }
        let nodes = self.nodes(&d);
        let holds = self
            .holds
            .iter()
            .enumerate()
            .map(|(h, x)| HoldShape {
                point: self.held(x, &nodes, &d),
                direction: d[x.piece],
                forces: (0..x.strings.len())
                    .map(|s| {
                        active
                            .iter()
                            .position(|c| *c == (h, s))
                            .map_or(0.0, |k| (-2.0 * mu[k] * x.strings[s].length).max(0.0))
                    })
                    .collect(),
            })
            .collect();
        Ok(RodShape {
            points: nodes,
            max_tension: self.max_tension(&d, lambda, &mu, &active),
            holds,
            directions: d,
        })
    }

    /// Newton from `start`, the strings in `active` taut, the others left
    /// out.
    fn newton(
        &self,
        start: &[V],
        active: &[(usize, usize)],
        budget: &Budget,
    ) -> Result<(Vec<V>, V, Vec<f64>), String> {
        let n = self.n();
        let mut d: Vec<V> = start.iter().map(|x| x.normalize()).collect();
        let l = self.mean_piece();
        let force_scale =
            self.q.norm() * self.length() + self.holds.iter().map(|h| h.load.norm()).sum::<f64>();
        let moment_tol = 1e-9 * (force_scale * l + self.ei / l).max(1e-9);
        let gap_tol = 1e-10 * self.length();
        let scaled = |r: &DVector<f64>| -> f64 {
            let mut worst: f64 = 0.0;
            for (k, v) in r.iter().enumerate() {
                let tol = if k < 2 * n { moment_tol } else { gap_tol };
                let r = v.abs() / tol;
                worst = if r.is_nan() {
                    f64::INFINITY
                } else {
                    worst.max(r)
                };
            }
            worst
        };
        // The line search lowers the energy plus ρ times the constraints'
        // violation, ρ above the constraint forces: with damping (a
        // positive definite Hessian) every step is a descent direction.
        // Near the solution, where energy differences are below rounding,
        // a step lowering the residual's squares is taken too.
        let squares = |r: &DVector<f64>| -> f64 {
            r.iter()
                .enumerate()
                .map(|(k, v)| (v / if k < 2 * n { moment_tol } else { gap_tol }).powi(2))
                .sum()
        };
        let lengths: Vec<f64> = active.iter().map(|&c| self.string(c).length).collect();
        let mut rho: f64 = 0.0;
        let trace = std::env::var_os("OLS_ROD_TRACE").is_some();
        let mut run = budget.start();
        let (mut lambda, mut mu) = self.multipliers(&d, active);
        let mut r = self.residual(&d, lambda, &mu, active);
        let mut merit = scaled(&r);
        let mut damping = 1e-6;
        while merit > 1.0 || merit.is_nan() {
            run.step().map_err(|stop| {
                format!("rod did not converge: {stop}; residual {merit:.2e}× the tolerance")
            })?;
            let mut jac = self.jacobian(&d, lambda, &mu, active);
            let diag = (0..2 * n).map(|k| jac[(k, k)].abs()).fold(0.0, f64::max);
            for k in 0..2 * n {
                jac[(k, k)] += damping * diag.max(1e-12);
            }
            let step = jac
                .lu()
                .solve(&(-&r))
                .filter(|s| s.iter().all(|v| v.is_finite()))
                .ok_or("rod: singular system")?;
            // At most 0.3 rad of rotation per piece in one step.
            let largest = (0..n)
                .map(|i| step[2 * i].hypot(step[2 * i + 1]))
                .fold(0.0, f64::max);
            let mut factor = if largest > 0.3 { 0.3 / largest } else { 1.0 };
            loop {
                let trial: Vec<V> = (0..n)
                    .map(|i| {
                        let (e1, e2) = basis(d[i]);
                        let v = e1 * step[2 * i] * factor + e2 * step[2 * i + 1] * factor;
                        let angle = v.norm();
                        if angle == 0.0 {
                            d[i]
                        } else {
                            (d[i] * angle.cos() + v / angle * angle.sin()).normalize()
                        }
                    })
                    .collect();
                let tl = lambda + V::new(step[2 * n], step[2 * n + 1], step[2 * n + 2]) * factor;
                let tm: Vec<f64> = mu
                    .iter()
                    .enumerate()
                    .map(|(k, m)| m + step[2 * n + 3 + k] * factor)
                    .collect();
                let tr = self.residual(&trial, tl, &tm, active);
                let pull: f64 = tm
                    .iter()
                    .zip(&lengths)
                    .map(|(m, l)| 2.0 * m.abs() * l)
                    .sum();
                rho = rho.max(2.0 * (tl.norm() + pull) + 1.0);
                let (now, then) = (self.merit(&d, rho, active), self.merit(&trial, rho, active));
                if trace {
                    eprintln!(
                        "rod {}: merit {merit:.3e} → {:.3e}, factor {factor}, damping {damping:.1e}",
                        run.iterations(),
                        scaled(&tr)
                    );
                }
                let lower = then < now - 1e-13 * now.abs().max(1e-9);
                let level = (then - now).abs() <= 1e-13 * now.abs().max(1e-9);
                if lower || (level && squares(&tr) < squares(&r)) {
                    let tmerit = scaled(&tr);
                    damping = if factor == 1.0 {
                        (damping * 0.1).max(1e-12)
                    } else {
                        damping * 10.0
                    };
                    (d, lambda, mu, r, merit) = (trial, tl, tm, tr, tmerit);
                    if factor < 1.0 {
                        // A cut step's multipliers are not Newton's.
                        (lambda, mu) = self.multipliers(&d, active);
                        r = self.residual(&d, lambda, &mu, active);
                        merit = scaled(&r);
                    }
                    break;
                }
                factor *= 0.5;
                if factor < 1e-4 {
                    // No progress along this step: more damping (a
                    // shorter, steeper step) next time.
                    damping *= 100.0;
                    if damping > 1e8 {
                        return Err(format!(
                            "rod: no step lowers the residual ({merit:.2e}× the tolerance)"
                        ));
                    }
                    break;
                }
            }
        }
        Ok((d, lambda, mu))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn budget() -> Budget {
        Budget::new().max_iterations(200)
    }

    fn string(top: V, length: f64) -> RodString {
        RodString { top, length }
    }

    /// Without bending stiffness the rod hangs as the inextensible
    /// catenary: z = a cosh(x / a) through both ends.
    #[test]
    fn a_hinged_rod_hangs_as_a_catenary() {
        // a = 2: half span 1.5, length 2 a sinh(1.5 / a).
        let a = 2.0_f64;
        let half = 1.5;
        let length = 2.0 * a * (half / a).sinh();
        let rod = Rod::uniform(
            V::new(-half, 0.0, 0.0),
            V::new(half, 0.0, 0.0),
            length,
            60,
            V::new(0.0, 0.0, -10.0),
            0.0,
        );
        let start: Vec<V> = (0..60)
            .map(|i| V::new(1.0, 0.0, if i < 30 { -0.5 } else { 0.5 }))
            .collect();
        let s = rod.solve(&start, &budget()).unwrap();
        let low = s.points.iter().map(|p| p.z).fold(f64::INFINITY, f64::min);
        let exact = a - a * (half / a).cosh();
        assert!((low - exact).abs() < 0.01, "{low} vs {exact}");
        // Tension at the ends: w a cosh(x / a).
        let t = 10.0 * a * (half / a).cosh();
        assert!(
            (s.max_tension / t - 1.0).abs() < 0.02,
            "{} vs {t}",
            s.max_tension
        );
    }

    /// Side wind moves a jumper whose clamps point it downwards (jumper
    /// terminals angled down) less than a hinged one, and less the stiffer
    /// it is; it leaves its ends in the clamps' directions. (With clamps
    /// rising along the spans the bend at the clamps eases as the jumper
    /// swings sideways, and it swings further than a hinged one: physics,
    /// not a fault.)
    #[test]
    fn clamped_ends_hold_the_rod_against_the_wind() {
        let rod = |ei: f64, q: V| Rod {
            start: (ei > 0.0).then(|| V::new(1.0, 0.0, -0.5).normalize()),
            end: (ei > 0.0).then(|| V::new(1.0, 0.0, 0.5).normalize()),
            ..Rod::uniform(V::zeros(), V::new(4.0, 0.0, 0.0), 5.0, 30, q, ei)
        };
        let start: Vec<V> = (0..30)
            .map(|i| V::new(1.0, 0.0, if i < 15 { -0.6 } else { 0.6 }))
            .collect();
        let (still, windy) = (V::new(0.0, 0.0, -15.0), V::new(0.0, 10.0, -15.0));
        // How far the wind moves the middle of the jumper (from the still
        // shape, as the wind rises).
        let moved = |ei: f64| {
            let a = rod(ei, still).solve(&start, &budget()).unwrap();
            let b = rod(ei, windy).solve(&a.directions, &budget()).unwrap();
            (b.points[15] - a.points[15]).norm()
        };
        let (hinged, clamped, stiff) = (moved(0.0), moved(20.0), moved(200.0));
        assert!(clamped < 0.9 * hinged, "{clamped} vs {hinged}");
        assert!(stiff < 0.7 * clamped, "{stiff} vs {clamped}");
        let s = rod(20.0, windy).solve(&start, &budget()).unwrap();
        let angle = s.directions[0].angle(&V::new(1.0, 0.0, -0.5)).to_degrees();
        assert!(angle < 10.0, "{angle}");
    }

    /// The Jacobian matches finite differences of the residual: pieces of
    /// different lengths, a string at a node and a V at a clamp's middle,
    /// all taut.
    #[test]
    fn jacobian_matches_finite_differences() {
        let rod = Rod {
            a: V::new(0.0, 0.0, 0.0),
            b: V::new(3.0, 0.5, -0.2),
            pieces: vec![0.7, 0.5, 0.6, 0.1, 0.8, 0.9],
            q: V::new(0.0, 4.0, -15.0),
            ei: 30.0,
            start: Some(V::new(1.0, 0.1, 0.2).normalize()),
            end: Some(V::new(1.0, -0.1, -0.2).normalize()),
            holds: vec![
                RodHold {
                    piece: 2,
                    offset: 0.0,
                    load: V::new(0.0, 0.0, -20.0),
                    strings: vec![string(V::new(1.5, 0.0, 1.0), 1.5)],
                },
                RodHold {
                    piece: 3,
                    offset: 0.5,
                    load: V::new(0.0, 0.0, -35.0),
                    strings: vec![
                        string(V::new(2.0, -1.0, 1.0), 1.8),
                        string(V::new(2.0, 1.0, 1.2), 1.6),
                    ],
                },
            ],
        };
        let n = rod.n();
        let d: Vec<V> = (0..n)
            .map(|i| V::new(1.0, 0.1 * i as f64, -0.3 + 0.1 * i as f64).normalize())
            .collect();
        let active = rod.strings();
        let (lambda, mu) = (V::new(30.0, -5.0, 10.0), vec![7.0, -3.0, -5.0]);
        let j = rod.jacobian(&d, lambda, &mu, &active);
        let r0 = rod.residual(&d, lambda, &mu, &active);
        let h = 1e-6;
        let size = r0.len();
        assert_eq!(size, 2 * n + 3 + 3);
        for col in 0..size {
            let (mut dd, mut l, mut m) = (d.clone(), lambda, mu.clone());
            if col < 2 * n {
                let i = col / 2;
                let (e1, e2) = basis(d[i]);
                let v = if col % 2 == 0 { e1 } else { e2 } * h;
                dd[i] = (d[i] * v.norm().cos() + v.normalize() * v.norm().sin()).normalize();
            } else if col < 2 * n + 3 {
                l[col - 2 * n] += h;
            } else {
                m[col - 2 * n - 3] += h;
            }
            let r1 = rod.residual(&dd, l, &m, &active);
            for row in 0..size {
                let fd = (r1[row] - r0[row]) / h;
                // Rows of the rotations: the basis turns with dᵢ, which
                // only matters away from equilibrium (a twist term).
                let tol = 1e-3 * (1.0 + fd.abs())
                    + if row < 2 * n && col < 2 * n && row / 2 == col / 2 {
                        1e3
                    } else {
                        0.0
                    };
                assert!(
                    (j[(row, col)] - fd).abs() < tol,
                    "J[{row},{col}] = {} vs {fd}",
                    j[(row, col)]
                );
            }
        }
    }

    /// A support string over the middle lifts it to the string's length
    /// from the top and carries part of the weight.
    #[test]
    fn a_support_string_holds_the_rod() {
        for ei in [0.0, 20.0] {
            let rod = Rod {
                start: (ei > 0.0).then(|| V::new(1.0, 0.0, -0.5).normalize()),
                end: (ei > 0.0).then(|| V::new(1.0, 0.0, 0.5).normalize()),
                holds: vec![RodHold {
                    piece: 15,
                    offset: 0.0,
                    load: V::new(0.0, 0.0, -20.0),
                    strings: vec![string(V::new(2.0, 0.0, 0.5), 1.0)],
                }],
                ..Rod::uniform(
                    V::zeros(),
                    V::new(4.0, 0.0, 0.0),
                    5.0,
                    30,
                    V::new(0.0, 0.0, -15.0),
                    ei,
                )
            };
            let start: Vec<V> = (0..30)
                .map(|i| V::new(1.0, 0.0, if i < 15 { -0.6 } else { 0.6 }))
                .collect();
            let s = rod.solve(&start, &budget()).unwrap();
            let (clamp, force) = (s.holds[0].point, s.holds[0].forces[0]);
            assert!(((clamp - V::new(2.0, 0.0, 0.5)).norm() - 1.0).abs() < 1e-6);
            // About the weight of rod and string (the rod pulls down on it).
            assert!(force > 40.0 && force < 300.0, "{force}");
        }
    }

    /// A rod of 4 m between ends 3 m apart at the same height: 0.1 m
    /// pieces, the 20th (1.9–2.0 m from A) a clamp held at its middle by
    /// `strings`, 30 N hanging there.
    fn clamped_rod(ei: f64, q: V, strings: Vec<RodString>) -> (Rod, Vec<V>) {
        let rod = Rod {
            holds: vec![RodHold {
                piece: 19,
                offset: 0.5,
                load: V::new(0.0, 0.0, -30.0),
                strings,
            }],
            ..Rod::uniform(V::zeros(), V::new(3.0, 0.0, 0.0), 3.9, 39, q, ei)
        };
        let start = (0..39)
            .map(|i| V::new(1.0, 0.0, if i < 19 { -0.6 } else { 0.6 }))
            .collect();
        (rod, start)
    }

    /// An I-string over the clamp's middle in still air: plumb, carrying
    /// the weight hanging on it (the clamp's, and the rod's less what its
    /// ends carry); the rod is straight through the clamp, with or without
    /// bending stiffness.
    #[test]
    fn an_i_string_holds_a_clamp_plumb() {
        for ei in [0.0, 50.0] {
            let w = 15.0;
            let (rod, start) = clamped_rod(
                ei,
                V::new(0.0, 0.0, -w),
                vec![string(V::new(1.5, 0.0, 1.0), 1.5)],
            );
            let started = std::time::Instant::now();
            let s = rod.solve(&start, &budget()).unwrap();
            println!("EI {ei}: {:.1?}", started.elapsed());
            let h = &s.holds[0];
            // At the string's length below its top, plumb.
            let arm = h.point - V::new(1.5, 0.0, 1.0);
            assert!((arm.norm() - 1.5).abs() < 1e-9, "{}", arm.norm());
            assert!(arm.xy().norm() < 1e-9, "{arm:?}");
            // Straight through: the clamp's ends and middle on one line,
            // the direction in the direction out.
            let (p0, p1) = (s.points[19], s.points[20]);
            assert!(((p1 - p0).normalize() - h.direction).norm() < 1e-9);
            assert!(((p0 + p1) * 0.5 - h.point).norm() < 1e-12);
            // Symmetric: the clamp level.
            assert!(h.direction.z.abs() < 1e-6, "{:?}", h.direction);
            // The string carries the weight hanging on it: the clamp's,
            // and the rod's less what its ends carry. Hinged, the tension
            // at the end pieces' middles is along them (the largest, at
            // the ends, the highest points): each end carries its vertical
            // part and half its piece. (Stiff, the rod lifted in its middle
            // pulls its hinged ends down: the string carries more.)
            let total = w * 3.9 + 30.0;
            assert!(h.forces[0] > 30.0, "{}", h.forces[0]);
            if ei == 0.0 {
                assert!(h.forces[0] > 30.0 && h.forces[0] < total, "{}", h.forces[0]);
                let slope = (s.points[1] - s.points[0]).normalize().z.abs();
                let ends = 2.0 * (s.max_tension * slope + 0.5 * w * 0.1);
                assert!(
                    (h.forces[0] + ends - total).abs() < 1e-6 * total,
                    "string {} + ends {ends} vs {total}",
                    h.forces[0]
                );
            }
        }
    }

    /// Wind across a V-held clamp: the clamp swings with the wind, the
    /// leg on the wind side holds, the leg to leeward goes slack (it
    /// would push); in still air both hold. No string pushes.
    #[test]
    fn a_v_held_clamp_lets_a_leg_go_slack_in_the_wind() {
        let leg = 2.0_f64.sqrt();
        let legs = vec![
            string(V::new(1.5, -1.0, 1.0), leg),
            string(V::new(1.5, 1.0, 1.0), leg),
        ];
        let (still, start) = clamped_rod(50.0, V::new(0.0, 0.0, -15.0), legs.clone());
        let s = still.solve(&start, &budget()).unwrap();
        let h = &s.holds[0];
        assert!(h.forces.iter().all(|f| *f > 1.0), "{:?}", h.forces);
        assert!(
            (h.point - V::new(1.5, 0.0, 0.0)).norm() < 1e-3,
            "{:?}",
            h.point
        );
        // The wind towards +y: the clamp swings about the leg from −y; the
        // leg from +y is slack.
        let (windy, _) = clamped_rod(50.0, V::new(0.0, 25.0, -15.0), legs);
        let w = windy.solve(&s.directions, &budget()).unwrap();
        let h = &w.holds[0];
        assert!(h.forces[0] > 1.0 && h.forces[1] == 0.0, "{:?}", h.forces);
        assert!(h.point.y > 0.05, "{:?}", h.point);
        assert!(((h.point - V::new(1.5, -1.0, 1.0)).norm() - leg).abs() < 1e-9);
        assert!((h.point - V::new(1.5, 1.0, 1.0)).norm() < leg);
        // Straight through it.
        assert!(((w.points[20] - w.points[19]).normalize() - h.direction).norm() < 1e-9);
    }

    /// Two clamps of 0.08 m, each on an I-string, in a side wind, without
    /// bending stiffness: both strings taut at their lengths, swung with
    /// the wind, the rod straight through each clamp.
    #[test]
    fn two_clamps_hold_the_rod() {
        let mut pieces = vec![0.1; 9];
        pieces.push(0.08);
        pieces.extend(vec![0.1; 19]);
        pieces.push(0.08);
        pieces.extend(vec![0.1; 9]);
        let rod = Rod {
            a: V::zeros(),
            b: V::new(3.4, 0.0, 0.0),
            pieces,
            q: V::new(0.0, 3.0, -15.0),
            ei: 0.0,
            start: None,
            end: None,
            holds: [(9, 0.94), (29, 2.96)]
                .into_iter()
                .map(|(piece, x)| RodHold {
                    piece,
                    offset: 0.5,
                    load: V::new(0.0, 0.0, -10.0),
                    strings: vec![string(V::new(x, 0.0, 0.8), 1.0)],
                })
                .collect(),
        };
        let start: Vec<V> = (0..39)
            .map(|i| V::new(1.0, 0.0, if i < 19 { -0.6 } else { 0.6 }))
            .collect();
        let started = std::time::Instant::now();
        let s = rod.solve(&start, &budget()).unwrap();
        println!("two clamps: {:.1?}", started.elapsed());
        for (h, (piece, x)) in s.holds.iter().zip([(9, 0.94), (29, 2.96)]) {
            assert!(h.forces[0] > 1.0, "{:?}", h.forces);
            assert!(((h.point - V::new(x, 0.0, 0.8)).norm() - 1.0).abs() < 1e-9);
            let through = (s.points[piece + 1] - s.points[piece]).normalize();
            assert!((through - h.direction).norm() < 1e-12);
            // The wind swings it sideways.
            assert!(h.point.y > 0.01, "{:?}", h.point);
        }
    }
}
