// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A jumper with bending stiffness: an inextensible elastic rod between two
//! fixed ends, optionally clamped in a direction at each end, optionally
//! held at one node by a rigid support string.
//!
//! - **Discretisation**: n straight pieces of equal length ℓ with unit
//!   directions d₀…dₙ₋₁; node m is at \\( A + ℓ \sum_{i<m} d_i \\).
//! - **Energy**: the distributed load q (weight, ice, wind; per metre) as
//!   a potential, bending \\( c (1 - d_i \cdot d_{i+1}) \\) at every inner
//!   node and \\( c (1 - d_0 \cdot e_A) \\), \\( c (1 - d_{n-1} \cdot e_B) \\)
//!   at a clamped end (e: the direction the clamp holds), with c = EI / ℓ
//!   (2c at a clamp: the first piece's direction is the tangent half a
//!   piece from it, as for a beam's clamped end).
//! - **Constraints**: the far end is at B (\\( ℓ \sum d_i = B - A \\)); a taut
//!   support string keeps its node at the string's length from its top.
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

/// A support string: top, length, the node it holds, the weight it puts
/// on that node (half the string's, N).
#[derive(Debug, Clone, Copy)]
pub(crate) struct RodSupport {
    pub top: V,
    pub length: f64,
    pub node: usize,
    pub weight: f64,
}

/// The rod to solve.
#[derive(Debug, Clone)]
pub(crate) struct Rod {
    /// First end.
    pub a: V,
    /// Second end.
    pub b: V,
    /// Total (unstressed) length, m.
    pub length: f64,
    /// Pieces.
    pub n: usize,
    /// Load per metre, N/m.
    pub q: V,
    /// Bending stiffness EI, N·m².
    pub ei: f64,
    /// Direction the rod leaves A in, when A is clamped.
    pub start: Option<V>,
    /// Direction the rod arrives at B in, when B is clamped.
    pub end: Option<V>,
    /// Support string.
    pub support: Option<RodSupport>,
}

/// The solved rod.
#[derive(Debug, Clone)]
pub(crate) struct RodShape {
    /// Nodes, A to B.
    pub points: Vec<V>,
    /// Largest tension, N.
    pub max_tension: f64,
    /// Support string clamp and force, N (taut strings only).
    pub support: Option<(V, f64)>,
    /// Piece directions (to start the next solve from).
    pub directions: Vec<V>,
}

fn basis(d: V) -> (V, V) {
    let helper = if d.x.abs() < 0.6 { V::x() } else { V::y() };
    let e1 = d.cross(&helper).normalize();
    (e1, d.cross(&e1))
}

impl Rod {
    fn piece(&self) -> f64 {
        self.length / self.n as f64
    }

    fn nodes(&self, d: &[V]) -> Vec<V> {
        let l = self.piece();
        let mut out = Vec::with_capacity(d.len() + 1);
        let mut x = self.a;
        out.push(x);
        for di in d {
            x += di * l;
            out.push(x);
        }
        out
    }

    /// Potential energy of the loads plus bending energy, J.
    fn energy(&self, d: &[V]) -> f64 {
        let (n, l) = (self.n, self.piece());
        let c = self.ei / l;
        let nodes = self.nodes(d);
        let mut e = 0.0;
        for i in 0..n {
            e -= self.q.dot(&(nodes[i] + nodes[i + 1])) * 0.5 * l;
        }
        for i in 0..n.saturating_sub(1) {
            e += c * (1.0 - d[i].dot(&d[i + 1]));
        }
        if let Some(a) = self.start {
            e += 2.0 * c * (1.0 - d[0].dot(&a));
        }
        if let Some(b) = self.end {
            e += 2.0 * c * (1.0 - d[n - 1].dot(&b));
        }
        if let Some(s) = &self.support {
            e += s.weight * nodes[s.node].z;
        }
        e
    }

    /// Exact (L1) penalty merit: energy plus ρ times the constraints'
    /// violation (far end, taut string).
    fn merit(&self, d: &[V], rho: f64, taut: bool) -> f64 {
        let nodes = self.nodes(d);
        let mut m = self.energy(d) + rho * (nodes[self.n] - self.b).norm();
        if taut && let Some(s) = &self.support {
            let r = (nodes[s.node] - s.top).norm_squared() - s.length * s.length;
            m += rho * r.abs() / (2.0 * s.length);
        }
        m
    }

    /// ∂L/∂dᵢ for multipliers λ (end) and μ (string).
    fn lagrangian_gradient(&self, d: &[V], lambda: V, mu: f64, taut: bool) -> Vec<V> {
        let (n, l) = (self.n, self.piece());
        let c = self.ei / l;
        let nodes = self.nodes(d);
        let mut g: Vec<V> = (0..n)
            .map(|i| -self.q * (l * l * ((n - i) as f64 - 0.5)) - lambda * l)
            .collect();
        for i in 0..n.saturating_sub(1) {
            g[i] -= d[i + 1] * c;
            g[i + 1] -= d[i] * c;
        }
        if let Some(e) = self.start {
            g[0] -= e * (2.0 * c);
        }
        if let Some(e) = self.end {
            g[n - 1] -= e * (2.0 * c);
        }
        if let Some(s) = &self.support {
            let w = V::new(0.0, 0.0, -s.weight);
            let r = nodes[s.node] - s.top;
            for gi in g.iter_mut().take(s.node) {
                *gi -= w * l;
                if taut {
                    *gi -= r * (2.0 * mu * l);
                }
            }
        }
        g
    }

    /// Residual: tangential gradients (2 per piece), end gap (3), string
    /// (1 when taut).
    fn residual(&self, d: &[V], lambda: V, mu: f64, taut: bool) -> DVector<f64> {
        let n = self.n;
        let g = self.lagrangian_gradient(d, lambda, mu, taut);
        let size = 2 * n + 3 + usize::from(taut);
        let mut r = DVector::zeros(size);
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
        if taut && let Some(s) = &self.support {
            r[2 * n + 3] = -((nodes[s.node] - s.top).norm_squared() - s.length * s.length);
        }
        r
    }

    /// The residual's Jacobian (symmetric).
    fn jacobian(&self, d: &[V], lambda: V, mu: f64, taut: bool) -> DMatrix<f64> {
        let (n, l) = (self.n, self.piece());
        let c = self.ei / l;
        let g = self.lagrangian_gradient(d, lambda, mu, taut);
        let size = 2 * n + 3 + usize::from(taut);
        let mut j = DMatrix::zeros(size, size);
        let bases: Vec<(V, V)> = d.iter().map(|x| basis(*x)).collect();
        let e = |i: usize, a: usize| if a == 0 { bases[i].0 } else { bases[i].1 };
        for i in 0..n {
            // Rotating dᵢ: its second derivative is −dᵢ.
            let curvature = d[i].dot(&g[i]);
            for a in 0..2 {
                j[(2 * i + a, 2 * i + a)] -= curvature;
                // The far end: ∂(ℓ Σ d)/∂uᵢ = ℓ eᵢ.
                for k in 0..3 {
                    j[(2 * i + a, 2 * n + k)] -= l * e(i, a)[k];
                    j[(2 * n + k, 2 * i + a)] -= l * e(i, a)[k];
                }
            }
            // Bending between neighbours: −c I.
            if i + 1 < n {
                for a in 0..2 {
                    for b in 0..2 {
                        let v = -c * e(i, a).dot(&e(i + 1, b));
                        j[(2 * i + a, 2 * (i + 1) + b)] += v;
                        j[(2 * (i + 1) + b, 2 * i + a)] += v;
                    }
                }
            }
        }
        if taut && let Some(s) = &self.support {
            let r = self.nodes(d)[s.node] - s.top;
            let m = 2 * n + 3;
            for i in 0..s.node {
                for a in 0..2 {
                    let v = -2.0 * l * e(i, a).dot(&r);
                    j[(2 * i + a, m)] += v;
                    j[(m, 2 * i + a)] += v;
                    // −μ |r|²: ∂²/∂dᵢ∂dₖ = 2 ℓ² I for i, k before the node.
                    for k in 0..s.node {
                        for b in 0..2 {
                            j[(2 * i + a, 2 * k + b)] -= 2.0 * mu * l * l * e(i, a).dot(&e(k, b));
                        }
                    }
                }
            }
        }
        j
    }

    /// The end force λ and string multiplier μ that best balance the
    /// pieces at directions `d` (least squares on the tangential residual,
    /// which is affine in them): after a damped step, the multipliers of
    /// the Newton step can be far off and spoil the next Jacobian.
    fn multipliers(&self, d: &[V], taut: bool) -> (V, f64) {
        let n = self.n;
        let base = self
            .residual(d, V::zeros(), 0.0, taut)
            .rows(0, 2 * n)
            .into_owned();
        let k = 3 + usize::from(taut);
        let mut a = DMatrix::zeros(2 * n, k);
        for c in 0..k {
            let (mut l, mut m) = (V::zeros(), 0.0);
            if c < 3 {
                l[c] = 1.0;
            } else {
                m = 1.0;
            }
            let col = self.residual(d, l, m, taut).rows(0, 2 * n) - &base;
            a.set_column(c, &col);
        }
        let ata = a.transpose() * &a;
        let atb = a.transpose() * -base;
        match ata.lu().solve(&atb) {
            Some(x) if x.iter().all(|v| v.is_finite()) => {
                (V::new(x[0], x[1], x[2]), if taut { x[3] } else { 0.0 })
            }
            _ => (V::zeros(), 0.0),
        }
    }

    /// Tensions along the rod and the string's force.
    fn forces(&self, d: &[V], lambda: V, mu: f64, taut: bool) -> (f64, Option<(V, f64)>) {
        let (n, l) = (self.n, self.piece());
        let nodes = self.nodes(d);
        let mut max: f64 = 0.0;
        for i in 0..n {
            let mut f = self.q * (l * ((n - i) as f64 - 0.5)) + lambda;
            if let Some(s) = &self.support
                && i < s.node
            {
                f += V::new(0.0, 0.0, -s.weight);
                if taut {
                    f += (nodes[s.node] - s.top) * (2.0 * mu);
                }
            }
            max = max.max(f.norm());
        }
        let support = self
            .support
            .filter(|_| taut)
            .map(|s| (nodes[s.node], 2.0 * mu.abs() * s.length));
        (max, support)
    }

    /// Solves from the directions `start` (n of them). A support string
    /// is first left out; if the rod then hangs farther from its top than
    /// its length, the string is shortened to its length in steps, each
    /// solve starting from the last (a string pulling the rod up from far
    /// in one go did not converge in testing).
    ///
    /// Expected cost: 5–30 Newton iterations on a dense system of 2n + 3
    /// unknowns (n ≈ 24: ~50 µs each in release), a few times over with a
    /// string. Exit clauses: the budget's iteration and time limits for
    /// each solve, string steps no smaller than 1/64 of the way; an error
    /// names the residual.
    pub(crate) fn solve(&self, start: &[V], budget: &Budget) -> Result<RodShape, String> {
        // In coordinates from A: in project coordinates (~10⁶ m) rounding
        // swamps the energy differences and the tolerances (28 of a real
        // line's jumpers stalled at 1–1000× the tolerance).
        let local = Rod {
            a: V::zeros(),
            b: self.b - self.a,
            support: self.support.map(|s| RodSupport {
                top: s.top - self.a,
                ..s
            }),
            ..self.clone()
        };
        let mut shape = local.solve_local(start, budget)?;
        for p in &mut shape.points {
            *p += self.a;
        }
        if let Some((clamp, _)) = &mut shape.support {
            *clamp += self.a;
        }
        Ok(shape)
    }

    fn solve_local(&self, start: &[V], budget: &Budget) -> Result<RodShape, String> {
        let n = self.n;
        if start.len() != n || n < 2 {
            return Err(format!(
                "rod: {} start directions for {n} pieces",
                start.len()
            ));
        }
        if let Some(s) = &self.support {
            // The string's node is s.node pieces from A: out of reach when
            // the string's top is farther from either end than that plus
            // the string's length.
            let along = self.piece() * s.node as f64;
            if (s.top - self.a).norm() > along + s.length
                || (s.top - self.b).norm() > self.length - along + s.length
            {
                return Err(format!(
                    "the support string cannot reach the jumper's point {along:.2} m along it"
                ));
            }
        }
        let (mut d, mut lambda, mut mu) = self.newton(start, false, budget)?;
        let mut taut = false;
        if let Some(s) = self.support {
            let far = (self.nodes(&d)[s.node] - s.top).norm();
            if far > s.length {
                taut = true;
                let (mut done, mut step) = (0.0_f64, 0.25_f64);
                while done < 1.0 {
                    let target = (done + step).min(1.0);
                    let mut rod = self.clone();
                    if let Some(r) = &mut rod.support {
                        r.length = far + (s.length - far) * target;
                    }
                    match rod.newton(&d, true, budget) {
                        Ok(next) => {
                            (d, lambda, mu) = next;
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
            }
        }
        let (max_tension, support) = self.forces(&d, lambda, mu, taut);
        Ok(RodShape {
            points: self.nodes(&d),
            max_tension,
            support,
            directions: d,
        })
    }

    /// Newton from `start`, the string held (`taut`) or left out.
    fn newton(&self, start: &[V], taut: bool, budget: &Budget) -> Result<(Vec<V>, V, f64), String> {
        let n = self.n;
        let mut d: Vec<V> = start.iter().map(|x| x.normalize()).collect();
        let l = self.piece();
        let force_scale = self.q.norm() * self.length + self.support.map_or(0.0, |s| s.weight);
        let moment_tol = 1e-9 * (force_scale * l + self.ei / l).max(1e-9);
        let gap_tol = 1e-10 * self.length;
        let scaled = |r: &DVector<f64>| -> f64 {
            let mut worst: f64 = 0.0;
            for (k, v) in r.iter().enumerate() {
                let tol = if k < 2 * n { moment_tol } else { gap_tol };
                worst = worst.max(v.abs() / tol);
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
        let mut rho: f64 = 0.0;
        let trace = std::env::var_os("OLS_ROD_TRACE").is_some();
        let mut run = budget.start();
        {
            let (mut lambda, mut mu) = self.multipliers(&d, taut);
            let mut r = self.residual(&d, lambda, mu, taut);
            let mut merit = scaled(&r);
            let mut damping = 1e-6;
            while merit > 1.0 {
                run.step().map_err(|stop| {
                    format!("rod did not converge: {stop}; residual {merit:.2e}× the tolerance")
                })?;
                let mut jac = self.jacobian(&d, lambda, mu, taut);
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
                    let tl =
                        lambda + V::new(step[2 * n], step[2 * n + 1], step[2 * n + 2]) * factor;
                    let tm = if taut {
                        mu + step[2 * n + 3] * factor
                    } else {
                        mu
                    };
                    let tr = self.residual(&trial, tl, tm, taut);
                    rho = rho.max(
                        2.0 * (tl.norm() + 2.0 * tm.abs() * self.support.map_or(0.0, |s| s.length))
                            + 1.0,
                    );
                    let (now, then) = (self.merit(&d, rho, taut), self.merit(&trial, rho, taut));
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
                            (lambda, mu) = self.multipliers(&d, taut);
                            r = self.residual(&d, lambda, mu, taut);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn budget() -> Budget {
        Budget::new().max_iterations(200)
    }

    /// Without bending stiffness the rod hangs as the inextensible
    /// catenary: z = a cosh(x / a) through both ends.
    #[test]
    fn a_hinged_rod_hangs_as_a_catenary() {
        // a = 2: half span 1.5, length 2 a sinh(1.5 / a).
        let a = 2.0_f64;
        let half = 1.5;
        let length = 2.0 * a * (half / a).sinh();
        let rod = Rod {
            a: V::new(-half, 0.0, 0.0),
            b: V::new(half, 0.0, 0.0),
            length,
            n: 60,
            q: V::new(0.0, 0.0, -10.0),
            ei: 0.0,
            start: None,
            end: None,
            support: None,
        };
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
            a: V::new(0.0, 0.0, 0.0),
            b: V::new(4.0, 0.0, 0.0),
            length: 5.0,
            n: 30,
            q,
            ei,
            start: (ei > 0.0).then(|| V::new(1.0, 0.0, -0.5).normalize()),
            end: (ei > 0.0).then(|| V::new(1.0, 0.0, 0.5).normalize()),
            support: None,
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

    /// The Jacobian matches finite differences of the residual (with a
    /// taut string).
    #[test]
    fn jacobian_matches_finite_differences() {
        let rod = Rod {
            a: V::new(0.0, 0.0, 0.0),
            b: V::new(3.0, 0.5, -0.2),
            length: 3.6,
            n: 6,
            q: V::new(0.0, 4.0, -15.0),
            ei: 30.0,
            start: Some(V::new(1.0, 0.1, 0.2).normalize()),
            end: Some(V::new(1.0, -0.1, -0.2).normalize()),
            support: Some(RodSupport {
                top: V::new(1.5, 0.0, 1.0),
                length: 1.5,
                node: 3,
                weight: 20.0,
            }),
        };
        let d: Vec<V> = (0..6)
            .map(|i| V::new(1.0, 0.1 * i as f64, -0.3 + 0.1 * i as f64).normalize())
            .collect();
        let (lambda, mu) = (V::new(30.0, -5.0, 10.0), 7.0);
        let j = rod.jacobian(&d, lambda, mu, true);
        let r0 = rod.residual(&d, lambda, mu, true);
        let h = 1e-6;
        let size = r0.len();
        for col in 0..size {
            let (mut dd, mut l, mut m) = (d.clone(), lambda, mu);
            if col < 12 {
                let i = col / 2;
                let (e1, e2) = basis(d[i]);
                let v = if col % 2 == 0 { e1 } else { e2 } * h;
                dd[i] = (d[i] * v.norm().cos() + v.normalize() * v.norm().sin()).normalize();
            } else if col < 15 {
                l[col - 12] += h;
            } else {
                m += h;
            }
            let r1 = rod.residual(&dd, l, m, true);
            for row in 0..size {
                let fd = (r1[row] - r0[row]) / h;
                // Rows of the rotations: the basis turns with dᵢ, which
                // only matters away from equilibrium (a twist term).
                let tol = 1e-3 * (1.0 + fd.abs())
                    + if row < 12 && col < 12 && row / 2 == col / 2 {
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
                a: V::new(0.0, 0.0, 0.0),
                b: V::new(4.0, 0.0, 0.0),
                length: 5.0,
                n: 30,
                q: V::new(0.0, 0.0, -15.0),
                ei,
                start: (ei > 0.0).then(|| V::new(1.0, 0.0, -0.5).normalize()),
                end: (ei > 0.0).then(|| V::new(1.0, 0.0, 0.5).normalize()),
                support: Some(RodSupport {
                    top: V::new(2.0, 0.0, 0.5),
                    length: 1.0,
                    node: 15,
                    weight: 20.0,
                }),
            };
            let start: Vec<V> = (0..30)
                .map(|i| V::new(1.0, 0.0, if i < 15 { -0.6 } else { 0.6 }))
                .collect();
            let s = rod.solve(&start, &budget()).unwrap();
            let (clamp, force) = s.support.unwrap();
            assert!(((clamp - V::new(2.0, 0.0, 0.5)).norm() - 1.0).abs() < 1e-6);
            // About the weight of rod and string (the rod pulls down on it).
            assert!(force > 40.0 && force < 300.0, "{force}");
        }
    }
}
