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

/// Why the fallback of [`Rod::solve`] did not find the shape.
enum Fallback {
    /// The rod is too short to reach where its strings hold it (in words).
    Short(String),
    /// It did not converge.
    Failed(String),
}

/// A Newton solve that no step improves any more, or that runs out of
/// steps, is taken as converged within this many times the tolerance
/// (10⁻⁶ of the loads' moments: micrometres): next to a string just taut,
/// carrying next to nothing, it creeps at 100–150× (the first existing
/// line).
const STALL_TOLERANCE: f64 = 1e3;

/// How close the shapes with the strings as springs are solved, in times
/// the tolerance: they only start the rigid solve.
const SPRING_TOLERANCE: f64 = 1e4;

/// A rigid string: fixed top, length; tension only, or both ways
/// (`push`: it may take compression, e.g. a V leg designed to push).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RodString {
    pub top: V,
    pub length: f64,
    pub push: bool,
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
    /// The strings as springs of this stiffness, N/m, instead of rigid
    /// (0: rigid; the start [`Rod::solve`] falls back on).
    pub springs: f64,
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
    /// The forces the rod exerts on its ends A and B, N (its stiffness's
    /// moments at clamped ends left out).
    pub end_forces: [V; 2],
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
            springs: 0.0,
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

    /// A string as a spring ([`Rod::springs`]): the multiplier its force
    /// stands for (as a taut string's, force −2μℓ), from its stretch: for
    /// \( g = |r|^2 - ℓ^2 \) the energy \( k g^2 / 8ℓ^2 \) (\( ≈ k δ^2 / 2 \)),
    /// μ = −k g / 4ℓ²; none while a tension-only one is slack.
    fn spring_mu(&self, c: (usize, usize), nodes: &[V], d: &[V]) -> f64 {
        let s = self.string(c);
        if self.springs <= 0.0 {
            return 0.0;
        }
        let g = self.arm(c, nodes, d).norm_squared() - s.length * s.length;
        if s.push || g > 0.0 {
            -self.springs * g / (4.0 * s.length * s.length)
        } else {
            0.0
        }
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
        for c in self.strings() {
            let m = self.spring_mu(c, &nodes, d);
            let l = self.string(c).length;
            // k g² / 8ℓ² = 2 μ² ℓ² / k.
            if m != 0.0 {
                e += 2.0 * m * m * l * l / self.springs;
            }
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
        // Springs: as taut strings with the multiplier their stretch gives.
        for c in self.strings() {
            let m = self.spring_mu(c, &nodes, d);
            if m == 0.0 {
                continue;
            }
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
        // Springs: as a taut string with its multiplier, and that
        // multiplier's change with the stretch, (k/ℓ²) cᵢcₖ (r·eᵢ)(r·eₖ).
        for c in self.strings() {
            let m = self.spring_mu(c, &nodes, d);
            if m == 0.0 {
                continue;
            }
            let h = &self.holds[c.0];
            let r = self.arm(c, &nodes, d);
            let l = self.string(c).length;
            for i in 0..=h.piece {
                let ci = self.coef(h, i);
                for a in 0..2 {
                    for kk in 0..=h.piece {
                        let ck = self.coef(h, kk);
                        for b in 0..2 {
                            j[(2 * i + a, 2 * kk + b)] +=
                                -2.0 * m * ci * ck * e(i, a).dot(&e(kk, b))
                                    + self.springs / (l * l)
                                        * ci
                                        * ck
                                        * e(i, a).dot(&r)
                                        * e(kk, b).dot(&r);
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

    /// The forces the rod exerts on A and on B: at A all its loads (its
    /// own, the held weights, the strings) and what holds B (λ); at B,
    /// λ against it.
    fn end_forces(&self, d: &[V], lambda: V, mu: &[f64], active: &[(usize, usize)]) -> [V; 2] {
        let nodes = self.nodes(d);
        let mut a = self.q * self.pieces.iter().sum::<f64>() + lambda;
        for h in &self.holds {
            a += h.load;
        }
        for (&c, m) in active.iter().zip(mu) {
            a += self.arm(c, &nodes, d) * (2.0 * m);
        }
        [a, -lambda]
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
        self.in_local(|local| local.solve_steps(start, budget, false, 1.0))
    }

    /// The fallback where [`Rod::solve`] fails ([`Rod::through_springs`]):
    /// its error says in words when the rod is too short to reach where
    /// its strings hold it, and by about how much.
    pub(crate) fn solve_through_springs(
        &self,
        start: &[V],
        budget: &Budget,
    ) -> Result<RodShape, String> {
        self.in_local(|local| {
            local.through_springs(start, budget).map_err(|e| match e {
                Fallback::Short(why) | Fallback::Failed(why) => why,
            })
        })
    }

    /// `solve` run on the rod in coordinates from A, the shape it gives
    /// moved back.
    fn in_local(
        &self,
        solve: impl FnOnce(&Rod) -> Result<RodShape, String>,
    ) -> Result<RodShape, String> {
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
        let mut shape = solve(&local)?;
        for p in &mut shape.points {
            *p += self.a;
        }
        for h in &mut shape.holds {
            h.point += self.a;
        }
        Ok(shape)
    }

    /// The fallback of [`Rod::solve`] when pulling the strings to their
    /// lengths meets a taut stretch on the way (the rod locked on a branch
    /// where they cannot all reach: a held point pulled up beside a V that
    /// may push, swung the other way, on the first existing line): the
    /// strings as springs, from soft to stiff (stiffness 10⁻² to 10³ times
    /// the loads per metre, each solve starting from the last), let the rod
    /// find where they all hold it; then rigid from there, the strings
    /// stretched taut at once (they are within millimetres of their
    /// lengths). Failing that, stiffer still (10⁶: a millionth of a metre
    /// per the loads' newton): strings still stretched by more than a
    /// millimetre say that the rod cannot reach where they hold it — it is
    /// too short for them by about that ([`Fallback::Short`]).
    ///
    /// Expected cost: 7–8 Newton solves more (~1 ms each). Exit clauses: the
    /// budget's for each.
    fn through_springs(&self, start: &[V], budget: &Budget) -> Result<RodShape, Fallback> {
        let failed = Fallback::Failed;
        let force_scale = (self.q.norm() * self.length()
            + self.holds.iter().map(|h| h.load.norm()).sum::<f64>())
        .max(1.0);
        let springs = |d: &[V], k: f64| -> Result<Vec<V>, Fallback> {
            let soft = Rod {
                springs: k * force_scale,
                ..self.clone()
            };
            Ok(soft
                .newton_within(d, &[], budget, SPRING_TOLERANCE, f64::INFINITY)
                .map_err(failed)?
                .0)
        };
        let mut d = self
            .newton_within(start, &[], budget, SPRING_TOLERANCE, f64::INFINITY)
            .map_err(failed)?
            .0;
        for k in [1e-2, 1e-1, 1.0, 1e1, 1e2, 1e3] {
            d = springs(&d, k)?;
        }
        let e = match self.solve_steps(&d, budget, true, STALL_TOLERANCE) {
            Ok(shape) => return Ok(shape),
            Err(e) => e,
        };
        // With nearly rigid springs, how far each held point is from where
        // its strings would hold it (the least move putting the strings off
        // their lengths back on them, to first order): what the rod is
        // short by.
        for k in [1e4, 1e5, 1e6] {
            d = springs(&d, k)?;
        }
        let nodes = self.nodes(&d);
        let mut worst: Option<(f64, usize)> = None;
        for (h, hold) in self.holds.iter().enumerate() {
            let mut rows = Vec::new();
            let mut gaps = Vec::new();
            for (k, s) in hold.strings.iter().enumerate() {
                let arm = self.arm((h, k), &nodes, &d);
                let gap = arm.norm() - s.length;
                if s.push || gap > 0.0 {
                    rows.push(arm.normalize());
                    gaps.push(gap);
                }
            }
            if rows.is_empty() {
                continue;
            }
            let u = DMatrix::from_fn(rows.len(), 3, |i, j| rows[i][j]);
            let Ok(pinv) = u.clone().pseudo_inverse(1e-9) else {
                continue;
            };
            let short = (pinv * DVector::from_vec(gaps)).norm();
            if worst.is_none_or(|(w, _)| short > w) {
                worst = Some((short, h));
            }
        }
        match worst {
            Some((short, h)) if short > 1e-3 => {
                let hold = &self.holds[h];
                let along = self.pieces[..hold.piece].iter().sum::<f64>()
                    + self.pieces[hold.piece] * hold.offset;
                Err(Fallback::Short(format!(
                    "too short to reach where its strings hold it: they hold the point \
                     {along:.2} m along it about {:.0} mm beyond its reach{}",
                    short * 1e3,
                    if hold.strings.iter().any(|s| s.push) {
                        " (strings that may push)"
                    } else {
                        ""
                    }
                )))
            }
            _ => Err(failed(e)),
        }
    }

    /// The strings pulled to their lengths in steps ([`Rod::solve`]), from
    /// the shape without them; `warm`: from `start` as it is (the strings
    /// beyond their lengths there taut at once). Each Newton solve ends
    /// within the tolerance, or within `accept` times it where no step
    /// improves it or the budget's steps run out.
    fn solve_steps(
        &self,
        start: &[V],
        budget: &Budget,
        warm: bool,
        accept: f64,
    ) -> Result<RodShape, String> {
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
        let (mut d, mut lambda, mut mu) = if warm {
            (
                start.iter().map(|x| x.normalize()).collect(),
                V::zeros(),
                Vec::new(),
            )
        } else {
            self.newton_within(start, &[], budget, 1.0, accept)?
        };
        let mut active: Vec<(usize, usize)> = Vec::new();
        let force_scale =
            self.q.norm() * total + self.holds.iter().map(|h| h.load.norm()).sum::<f64>();
        for round in 0.. {
            let nodes = self.nodes(&d);
            // Taut strings that would push: let go (those that may push stay).
            let kept: Vec<(usize, usize)> = active
                .iter()
                .zip(&mu)
                .filter(|(c, m)| {
                    let s = self.string(**c);
                    s.push || -2.0 * **m * s.length >= -1e-9 * force_scale
                })
                .map(|(c, _)| *c)
                .collect();
            // Slack strings their points hang beyond: taut; those that may
            // push, at their length whichever way.
            let newly: Vec<((usize, usize), f64)> = all
                .iter()
                .filter(|c| !active.contains(c))
                .map(|&c| (c, self.arm(c, &nodes, &d).norm()))
                .filter(|&(c, far)| {
                    let s = self.string(c);
                    far > s.length * (1.0 + 1e-9) || (s.push && far < s.length * (1.0 - 1e-9))
                })
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
                let (dd, l, m) = self.newton_within(&d, &next, budget, 1.0, accept)?;
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
                    match rod.newton_within(&d, &next, budget, 1.0, accept) {
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
                        active.iter().position(|c| *c == (h, s)).map_or(0.0, |k| {
                            let f = -2.0 * mu[k] * x.strings[s].length;
                            if x.strings[s].push { f } else { f.max(0.0) }
                        })
                    })
                    .collect(),
            })
            .collect();
        Ok(RodShape {
            points: nodes,
            max_tension: self.max_tension(&d, lambda, &mu, &active),
            end_forces: self.end_forces(&d, lambda, &mu, &active),
            holds,
            directions: d,
        })
    }

    /// Newton from `start`, the strings in `active` taut, the others left
    /// out, to `within` times the tolerance; where no step
    /// improves it any more or the budget's steps run out, taken as it is
    /// within `accept` times (infinite: a rough start, wherever it stops).
    fn newton_within(
        &self,
        start: &[V],
        active: &[(usize, usize)],
        budget: &Budget,
        within: f64,
        accept: f64,
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
        let rough = accept.is_infinite();
        'newton: while merit > within || merit.is_nan() {
            if let Err(stop) = run.step() {
                if merit <= accept && merit.is_finite() {
                    break;
                }
                return Err(format!(
                    "rod did not converge: {stop}; residual {merit:.2e}× the tolerance"
                ));
            }
            let mut jac = self.jacobian(&d, lambda, &mu, active);
            let diag = (0..2 * n).map(|k| jac[(k, k)].abs()).fold(0.0, f64::max);
            for k in 0..2 * n {
                jac[(k, k)] += damping * diag.max(1e-12);
            }
            let Some(step) = jac
                .lu()
                .solve(&(-&r))
                .filter(|s| s.iter().all(|v| v.is_finite()))
            else {
                if rough && merit.is_finite() {
                    break;
                }
                return Err("rod: singular system".into());
            };
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
                        // Stalled near the tolerance.
                        if merit <= accept && merit.is_finite() {
                            break 'newton;
                        }
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
        RodString {
            top,
            length,
            push: false,
        }
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
            springs: 0.0,
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

    /// The strings as springs: the Jacobian of the residual with them.
    #[test]
    fn spring_jacobian_matches_finite_differences() {
        let rod = Rod {
            a: V::new(0.0, 0.0, 0.0),
            b: V::new(3.0, 0.5, -0.2),
            pieces: vec![0.7, 0.5, 0.6, 0.1, 0.8, 0.9],
            q: V::new(0.0, 4.0, -15.0),
            ei: 30.0,
            springs: 800.0,
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
        // The strings as springs: no multipliers of their own; one
        // stretched, one slack, one that may push (pushed).
        let active = Vec::new();
        let (lambda, mu) = (V::new(30.0, -5.0, 10.0), Vec::new());
        let nodes = rod.nodes(&d);
        let engaged = rod
            .strings()
            .into_iter()
            .filter(|c| rod.spring_mu(*c, &nodes, &d) != 0.0)
            .count();
        assert!(engaged >= 1, "no spring engaged");
        let j = rod.jacobian(&d, lambda, &mu, &active);
        let r0 = rod.residual(&d, lambda, &mu, &active);
        let h = 1e-6;
        let size = r0.len();
        assert_eq!(size, 2 * n + 3);
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
            springs: 0.0,
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

    /// An invented 17.1 m jumper in the wind, held by two I strings and,
    /// near its far end, by a V that may push whose tops are one above the
    /// other (it swings like a gate about them). The jumper hangs blown
    /// away from the strings; pulled up to their lengths in steps from
    /// there, it locks where the stretch to the V is straight (the V swung
    /// the wrong way), yet a shape holding every string exists.
    fn gate_jumper() -> Rod {
        let mut pieces = vec![6.5 / 16.0; 16];
        pieces.push(0.1);
        pieces.extend(vec![3.3 / 8.0; 8]);
        pieces.push(0.1);
        pieces.extend(vec![3.4 / 8.0; 8]);
        pieces.push(0.1);
        pieces.extend(vec![3.6 / 8.0; 8]);
        let push = |top: V, length: f64| RodString {
            top,
            length,
            push: true,
        };
        Rod {
            a: V::zeros(),
            b: V::new(14.0, 6.0, -1.5),
            pieces,
            q: V::new(8.0, -20.0, -22.0),
            ei: 1000.0,
            springs: 0.0,
            start: Some(V::new(0.9, 0.3, -0.3).normalize()),
            end: Some(V::new(0.6, 0.8, 0.0).normalize()),
            holds: vec![
                RodHold {
                    piece: 16,
                    offset: 0.5,
                    load: V::new(20.0, -30.0, -130.0),
                    strings: vec![string(V::new(4.0, 3.5, 0.6), 3.3)],
                },
                RodHold {
                    piece: 25,
                    offset: 0.5,
                    load: V::new(20.0, -30.0, -130.0),
                    strings: vec![string(V::new(7.0, 5.5, 0.6), 3.3)],
                },
                RodHold {
                    piece: 34,
                    offset: 0.5,
                    load: V::new(0.0, 0.0, -250.0),
                    strings: vec![
                        push(V::new(9.4, 3.0, 0.6), 5.0),
                        push(V::new(9.5, 3.5, -2.1), 3.5),
                    ],
                },
            ],
        }
    }

    /// The start `Stiff::solve` takes: the rod hanging straight-ish down
    /// the chord (here: a parabola below it, sagging with the load).
    fn sagging_start(rod: &Rod) -> Vec<V> {
        let total = rod.length();
        let mut run = 0.0;
        let mut nodes = vec![rod.a];
        let sag = rod.q.normalize() * 2.5;
        for l in &rod.pieces {
            run += l;
            let t = run / total;
            nodes.push(rod.a + (rod.b - rod.a) * t + sag * (4.0 * t * (1.0 - t)));
        }
        nodes.windows(2).map(|w| w[1] - w[0]).collect()
    }

    #[test]
    fn a_rod_locked_on_its_way_to_its_strings_is_found_through_springs() {
        let rod = gate_jumper();
        let start = sagging_start(&rod);
        // The strings pulled to their lengths in steps alone: locked.
        let locked = rod.solve(&start, &budget());
        assert!(locked.is_err(), "the steps alone solve it now: {locked:?}");
        let s = rod.solve_through_springs(&start, &budget()).unwrap();
        // Every string holds: the I strings at most their lengths (taut
        // ones pull), the V's legs at theirs.
        for (h, held) in rod.holds.iter().zip(&s.holds) {
            for (string, force) in h.strings.iter().zip(&held.forces) {
                let far = (held.point - string.top).norm();
                if string.push || *force > 0.0 {
                    assert!(
                        (far - string.length).abs() < 1e-6,
                        "{far} vs {}",
                        string.length
                    );
                } else {
                    assert!(far < string.length + 1e-9, "{far} vs {}", string.length);
                }
            }
        }
        // The V swung towards the I strings, against the wind (+y): the
        // stretches between the held points not straight.
        let v = s.holds[2].point;
        assert!(v.y > 5.0, "{v:?}");
        let along = |h: usize| {
            let x = &rod.holds[h];
            rod.pieces[..x.piece].iter().sum::<f64>() + rod.pieces[x.piece] * x.offset
        };
        let chord = (s.holds[2].point - s.holds[1].point).norm();
        assert!(chord < along(2) - along(1) - 0.01, "{chord}");
        // An equilibrium: the far end balances the loads (the rod's own,
        // the held points', the strings') with what holds A.
        let mut total = rod.q * rod.length();
        for h in &rod.holds {
            total += h.load;
        }
        for (h, held) in rod.holds.iter().zip(&s.holds) {
            for (string, force) in h.strings.iter().zip(&held.forces) {
                total += (string.top - held.point).normalize() * *force;
            }
        }
        let [at_a, at_b] = s.end_forces;
        assert!(
            (at_a + at_b - total).norm() < 1e-6 * total.norm(),
            "{at_a} {at_b} {total}"
        );
    }

    /// A 5 m rod between ends 4 m apart, its middle (2.5 m along it)
    /// pinned by three 1 m struts (strings that may push) at a point 1 cm,
    /// then 10 cm, farther than 2.5 m from A: no shape holds it (each strut
    /// alone could reach). Said so in words, with by about how much and
    /// where, instead of a residual.
    #[test]
    fn a_rod_too_short_for_its_strings_says_by_how_much() {
        for cut in [0.01, 0.1] {
            let dir = V::new(1.0, 0.0, -0.2).normalize();
            let pin = dir * (2.5 + cut);
            let strut = |x: V| RodString {
                top: pin + x.normalize(),
                length: 1.0,
                push: true,
            };
            let rod = Rod {
                holds: vec![RodHold {
                    piece: 24,
                    offset: 0.5,
                    load: V::new(0.0, 0.0, -20.0),
                    strings: vec![
                        strut(V::new(0.3, 0.0, 0.95)),
                        strut(V::new(0.3, 0.8, 0.52)),
                        strut(V::new(0.3, -0.8, 0.52)),
                    ],
                }],
                ..Rod::uniform(
                    V::zeros(),
                    V::new(4.0, 0.0, 0.0),
                    5.0,
                    49,
                    V::new(0.0, 3.0, -15.0),
                    20.0,
                )
            };
            let start = sagging_start(&rod);
            assert!(rod.solve(&start, &budget()).is_err());
            let e = rod.solve_through_springs(&start, &budget()).unwrap_err();
            assert!(
                e.starts_with("too short to reach where its strings hold it"),
                "{e}"
            );
            assert!(e.contains("the point 2.50 m along it about"), "{e}");
            let mm: f64 = e
                .split("about ")
                .nth(1)
                .and_then(|x| x.split(" mm").next())
                .and_then(|x| x.parse().ok())
                .unwrap_or_else(|| panic!("{e}"));
            assert!(
                mm > 0.8 * cut * 1e3 && mm < 1.2 * cut * 1e3,
                "{mm} mm for {cut} m: {e}"
            );
        }
    }
}
