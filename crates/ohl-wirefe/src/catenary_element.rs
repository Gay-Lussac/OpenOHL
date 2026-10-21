// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Exact elastic catenary element.
//!
//! One element models a whole cable span (no meshing): unstressed length
//! \\( L_0 \\), axial stiffness \\( EA \\) and a uniform load \\( \mathbf q \\)
//! per unit of **unstressed** length (mass is conserved when the cable
//! stretches). The cable lies in the plane containing the chord and
//! \\( \mathbf q \\); in that plane, with \\( z \\) opposite to the load and
//! \\( x \\) perpendicular to it, \\( w = |\mathbf q| \\), \\( W = w L_0 \\),
//! and \\( H \\), \\( V_0 \\) the tension components at node *i* (pointing
//! along the cable towards *j*), \\( V_1 = V_0 + W \\):
//!
//! \\[ x = \frac{H L_0}{EA} + \frac{H}{w}\left[\operatorname{asinh}\frac{V_1}{H} - \operatorname{asinh}\frac{V_0}{H}\right] \\]
//! \\[ z = \frac{V_0 L_0 + w L_0^2/2}{EA} + \frac{1}{w}\left[\sqrt{H^2 + V_1^2} - \sqrt{H^2 + V_0^2}\right] \\]
//!
//! (equilibrium \\( dT_z/ds_0 = w \\), strain \\( T/EA \\), integrated over the
//! unstressed length). Given the chord, \\( (H, V_0) \\) are found by Newton's
//! method on these two equations with exit clauses. The element then gives
//! the end forces and the 3×3 tangent stiffness; the out-of-plane stiffness is
//! the string stiffness \\( H / x \\).

use std::fmt;

use nalgebra::{Matrix2, Matrix3, Vector2, Vector3};
use ohl_core::budget::{Budget, Stop};

/// Errors of a single cable element.
#[derive(Debug, Clone, PartialEq)]
pub enum ElementError {
    /// The chord is (nearly) parallel to the load: the element has no
    /// horizontal extent in its own plane.
    ParallelToLoad,
    /// Invalid properties.
    Invalid(String),
    /// The element equations did not converge.
    NotConverged {
        /// Why the iteration stopped.
        stop: Stop,
        /// Remaining geometric mismatch, m.
        residual: f64,
    },
}

impl fmt::Display for ElementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ParallelToLoad => write!(
                f,
                "cable element chord is parallel to its load (vertical cable)"
            ),
            Self::Invalid(m) => write!(f, "invalid cable element: {m}"),
            Self::NotConverged { stop, residual } => {
                write!(
                    f,
                    "cable element did not converge: {stop}; geometric mismatch {residual:.3e} m"
                )
            }
        }
    }
}

impl std::error::Error for ElementError {}

/// Properties of an elastic catenary element.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CableProperties {
    /// Unstressed length, m.
    pub unstressed_length: f64,
    /// Axial stiffness, N.
    pub axial_stiffness: f64,
    /// Load per unit unstressed length (gravity, ice, wind), N/m, global axes.
    pub load: Vector3<f64>,
}

/// Solved state of an element for given end positions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElementState {
    /// Tension component perpendicular to the load (in the cable plane), N.
    pub horizontal: f64,
    /// Tension component along −load at node i (towards j), N.
    pub v0: f64,
    /// Force exerted by the cable on node i, N (global).
    pub force_i: Vector3<f64>,
    /// Force exerted by the cable on node j, N (global).
    pub force_j: Vector3<f64>,
    /// Tangent stiffness K (3×3): d(force_j)/d(x_j) = −K, d(force_j)/d(x_i) = K.
    pub stiffness: Matrix3<f64>,
    /// Tension at node i and node j, N.
    pub tension_i: f64,
    /// Tension at node j, N.
    pub tension_j: f64,
}

fn geometry(h: f64, v0: f64, l0: f64, ea: f64, w: f64) -> (Vector2<f64>, Matrix2<f64>) {
    let v1 = v0 + w * l0;
    let a0 = h.hypot(v0);
    let a1 = h.hypot(v1);
    let asinh_diff = (v1 / h).asinh() - (v0 / h).asinh();
    let x = h * l0 / ea + h / w * asinh_diff;
    let z = (v0 * l0 + w * l0 * l0 / 2.0) / ea + (a1 - a0) / w;
    let dx_dh = l0 / ea + asinh_diff / w - (v1 / a1 - v0 / a0) / w;
    let dx_dv = h / w * (1.0 / a1 - 1.0 / a0);
    let dz_dh = h / w * (1.0 / a1 - 1.0 / a0);
    let dz_dv = l0 / ea + (v1 / a1 - v0 / a0) / w;
    (Vector2::new(x, z), Matrix2::new(dx_dh, dx_dv, dz_dh, dz_dv))
}

/// Default exit clauses for one element solve: Newton converges in a few
/// iterations from a good start (~1 µs each). The iteration limit (100,
/// ~100 µs) is what bounds the work; the time limit is only a backstop and
/// must stay far above it: a 20 ms limit fired after 4 iterations on a CI
/// Windows runner, where the OS paused the test thread while debug-build
/// tests ran in parallel — a false stop, not a convergence problem. The
/// enclosing structure solve has its own limit ([`crate::default_fe_budget`]).
pub fn default_element_budget() -> Budget {
    Budget::new()
        .max_iterations(100)
        .time_limit(std::time::Duration::from_secs(10))
}

impl CableProperties {
    /// Solves the element for the chord `d = x_j − x_i`.
    ///
    /// `guess` is a previous `(H, V0)` (e.g. from the last Newton step of the
    /// structure); without it a catenary-based estimate is used.
    pub fn solve(
        &self,
        d: Vector3<f64>,
        guess: Option<(f64, f64)>,
        budget: &Budget,
    ) -> Result<ElementState, ElementError> {
        let (l0, ea) = (self.unstressed_length, self.axial_stiffness);
        let w = self.load.norm();
        if !(l0 > 0.0 && ea > 0.0 && w > 0.0 && l0.is_finite() && ea.is_finite() && w.is_finite()) {
            return Err(ElementError::Invalid(format!(
                "L0 {l0}, EA {ea}, |q| {w} must be positive"
            )));
        }
        // Local frame: up = −q̂, e1 = horizontal part of the chord.
        let up = -self.load / w;
        let vz = d.dot(&up);
        let horizontal = d - up * vz;
        let hx = horizontal.norm();
        if hx <= 1e-9 * l0.max(d.norm()) {
            return Err(ElementError::ParallelToLoad);
        }
        let e1 = horizontal / hx;
        let e2 = up.cross(&e1);
        let target = Vector2::new(hx, vz);

        // Starting point: parabolic estimate of H from the cable slack.
        let (mut h, mut v0) = guess.filter(|(h, _)| *h > 0.0).unwrap_or_else(|| {
            let chord2 = hx * hx + vz * vz;
            let lambda = if l0 * l0 <= chord2 {
                0.2
            } else {
                (3.0 * ((l0 * l0 - vz * vz) / (hx * hx) - 1.0))
                    .sqrt()
                    .max(0.2)
            };
            let h = (w * hx / (2.0 * lambda)).abs().max(1e-6 * w * l0);
            (h, h * vz / hx - w * l0 / 2.0)
        });

        let mut run = budget.start();
        let (mut pos, mut flex) = geometry(h, v0, l0, ea, w);
        let mut residual = (target - pos).norm();
        // Relative tolerance, but never below the floating-point floor of
        // the position formulas: x = (H/w)·Δasinh carries rounding errors
        // of order ε·H/w (the catenary parameter, often 1–2 km). A short
        // piece (e.g. between a clamp and a marker ball, l0 ≈ 0.5 m) on such
        // a wire otherwise asks for less than the arithmetic gives and
        // stalls at its iteration limit (a real earth wire: 3e-12 m left).
        let floor = |h: f64| 64.0 * f64::EPSILON * (h / w + l0 + hx + vz.abs());
        let mut tol = (1e-12 * (hx + vz.abs() + l0)).max(floor(h));
        while residual > tol || residual.is_nan() {
            run.step()
                .map_err(|stop| ElementError::NotConverged { stop, residual })?;
            let step = flex.try_inverse().map(|inv| inv * (target - pos)).ok_or(
                ElementError::NotConverged {
                    stop: Stop::IterationLimit {
                        iterations: run.iterations(),
                        elapsed: run.elapsed(),
                    },
                    residual,
                },
            )?;
            // Damped update keeping H positive and reducing the mismatch.
            let mut factor = 1.0;
            loop {
                let (h_new, v_new) = (h + factor * step.x, v0 + factor * step.y);
                if h_new > 0.0 {
                    let (p, f) = geometry(h_new, v_new, l0, ea, w);
                    let r = (target - p).norm();
                    if r < residual || factor < 1e-6 {
                        (h, v0, pos, flex, residual) = (h_new, v_new, p, f, r);
                        tol = tol.max(floor(h));
                        break;
                    }
                }
                factor *= 0.5;
                if factor < 1e-6 {
                    // Cannot improve: take a tiny positive step on H.
                    h *= 1.5;
                    (pos, flex) = geometry(h, v0, l0, ea, w);
                    residual = (target - pos).norm();
                    break;
                }
            }
        }

        let v1 = v0 + w * l0;
        let force_i = e1 * h + up * v0;
        let force_j = -(e1 * h + up * v1);
        let k_in_plane = flex
            .try_inverse()
            .ok_or(ElementError::Invalid("singular flexibility".into()))?;
        // Stiffness in the local (e1, e2, up) basis, then rotated to global.
        let local = Matrix3::new(
            k_in_plane[(0, 0)],
            0.0,
            k_in_plane[(0, 1)],
            0.0,
            h / hx,
            0.0,
            k_in_plane[(1, 0)],
            0.0,
            k_in_plane[(1, 1)],
        );
        let r = Matrix3::from_columns(&[e1, e2, up]);
        Ok(ElementState {
            horizontal: h,
            v0,
            force_i,
            force_j,
            stiffness: r * local * r.transpose(),
            tension_i: h.hypot(v0),
            tension_j: h.hypot(v1),
        })
    }

    /// Stretched (real) length of the solved element:
    /// \\( L = L_0 + \frac{1}{EA}\int_0^{L_0} T\,ds_0 \\) with
    /// \\( T = \sqrt{H^2 + V^2} \\), \\( V = V_0 + w s_0 \\):
    /// \\( \int T\,ds_0 = [G(V_1) - G(V_0)] / w \\),
    /// \\( G(V) = \tfrac12 (V T + H^2 \operatorname{asinh}(V/H)) \\).
    pub fn stretched_length(&self, state: &ElementState) -> f64 {
        let (l0, ea, w) = (
            self.unstressed_length,
            self.axial_stiffness,
            self.load.norm(),
        );
        let (h, v0) = (state.horizontal, state.v0);
        let v1 = v0 + w * l0;
        let g = |v: f64| 0.5 * (v * h.hypot(v) + h * h * (v / h).asinh());
        l0 + (g(v1) - g(v0)) / w / ea
    }

    /// Point of the cable at unstressed arc length `s0` from node i,
    /// relative to node i (global axes), for a solved element with chord `d`.
    pub fn point_at(&self, d: Vector3<f64>, state: &ElementState, s0: f64) -> Vector3<f64> {
        let (ea, w) = (self.axial_stiffness, self.load.norm());
        let up = -self.load / w;
        let horizontal = d - up * d.dot(&up);
        let e1 = horizontal / horizontal.norm();
        let (h, v0) = (state.horizontal, state.v0);
        let vs = v0 + w * s0;
        let x = h * s0 / ea + h / w * ((vs / h).asinh() - (v0 / h).asinh());
        let z = (v0 * s0 + w * s0 * s0 / 2.0) / ea + (h.hypot(vs) - h.hypot(v0)) / w;
        e1 * x + up * z
    }

    /// `n + 1` points along the cable (relative to node i), evenly spaced in
    /// unstressed arc length, both ends included.
    pub fn shape(&self, d: Vector3<f64>, state: &ElementState, n: usize) -> Vec<Vector3<f64>> {
        let n = n.max(1);
        (0..=n)
            .map(|k| self.point_at(d, state, self.unstressed_length * k as f64 / n as f64))
            .collect()
    }

    /// Unstressed arc length from node i at which the cable reaches the
    /// fraction `fraction` (0–1) of the chord's horizontal (in-plane) extent.
    /// Monotone in `s0`, found by bisection (60 halvings, fixed).
    pub fn arc_length_at_fraction(
        &self,
        d: Vector3<f64>,
        state: &ElementState,
        fraction: f64,
    ) -> f64 {
        let w = self.load.norm();
        let up = -self.load / w;
        let target = (d - up * d.dot(&up)).norm() * fraction.clamp(0.0, 1.0);
        let (ea, h, v0) = (self.axial_stiffness, state.horizontal, state.v0);
        let x_at = |s0: f64| h * s0 / ea + h / w * (((v0 + w * s0) / h).asinh() - (v0 / h).asinh());
        let (mut lo, mut hi) = (0.0, self.unstressed_length);
        for _ in 0..60 {
            let mid = 0.5 * (lo + hi);
            if x_at(mid) < target {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        0.5 * (lo + hi)
    }

    /// Unstressed length that gives horizontal tension `h` for the chord `d`
    /// (used to "sag" a span at stringing). The chord length grows strictly
    /// as H grows for fixed geometry, so bisection on L0 is safe.
    pub fn unstressed_length_for_tension(
        d: Vector3<f64>,
        h: f64,
        axial_stiffness: f64,
        load: Vector3<f64>,
        budget: &Budget,
    ) -> Result<f64, ElementError> {
        let chord = d.norm();
        let element_budget = default_element_budget();
        let horizontal_for = |l0: f64| -> Result<f64, ElementError> {
            CableProperties {
                unstressed_length: l0,
                axial_stiffness,
                load,
            }
            .solve(d, None, &element_budget)
            .map(|s| s.horizontal)
        };
        // Longer cable = lower tension. Bracket and bisect.
        let mut run = budget.start();
        let mut lo = chord * 0.9;
        let mut hi = chord * 1.1;
        while horizontal_for(lo)? < h {
            run.step().map_err(|stop| ElementError::NotConverged {
                stop,
                residual: hi - lo,
            })?;
            lo = chord - 2.0 * (chord - lo);
        }
        while horizontal_for(hi)? > h {
            run.step().map_err(|stop| ElementError::NotConverged {
                stop,
                residual: hi - lo,
            })?;
            hi = chord + 2.0 * (hi - chord);
        }
        // Bracketed secant on H(L0) − h (Illinois): superlinear, never
        // leaves the bracket.
        let (mut f_lo, mut f_hi) = (horizontal_for(lo)? - h, horizontal_for(hi)? - h);
        let mut side = 0;
        let mut l0 = 0.5 * (lo + hi);
        while hi - lo > 1e-13 * chord {
            run.step().map_err(|stop| ElementError::NotConverged {
                stop,
                residual: hi - lo,
            })?;
            l0 = (lo * f_hi - hi * f_lo) / (f_hi - f_lo);
            if !(lo < l0 && l0 < hi) {
                l0 = 0.5 * (lo + hi);
            }
            let f = horizontal_for(l0)? - h;
            if f.abs() <= 1e-12 * h {
                break;
            }
            if f > 0.0 {
                (lo, f_lo) = (l0, f);
                if side == 1 {
                    f_hi /= 2.0;
                }
                side = 1;
            } else {
                (hi, f_hi) = (l0, f);
                if side == -1 {
                    f_lo /= 2.0;
                }
                side = -1;
            }
        }
        Ok(l0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stretched length's closed form against a midpoint sum of
    /// 1 + T(s)/EA along the unstressed length (200 000 pieces).
    #[test]
    fn stretched_length_is_the_integral_of_the_strain() {
        let props = CableProperties {
            unstressed_length: 306.5,
            axial_stiffness: 30e6,
            load: Vector3::new(0.0, 0.0, -15.69064),
        };
        let d = Vector3::new(300.0, 0.0, 60.0);
        let state = props.solve(d, None, &default_element_budget()).unwrap();
        let (h, v0, w, l0) = (state.horizontal, state.v0, 15.69064, 306.5);
        let n = 200_000;
        let ds = l0 / n as f64;
        let sum: f64 = (0..n)
            .map(|i| {
                let v = v0 + w * (i as f64 + 0.5) * ds;
                (1.0 + h.hypot(v) / 30e6) * ds
            })
            .sum();
        let closed = props.stretched_length(&state);
        assert!((closed - sum).abs() < 1e-9, "{closed} vs {sum}");
        // About L0 (1 + Tmean / EA): Tmean near 19 kN.
        assert!(closed > l0 * (1.0 + 18e3 / 30e6) && closed < l0 * (1.0 + 21e3 / 30e6));
    }

    /// Short pieces of tight wires (catenary parameter H/w of 1–2 km)
    /// converge to the exact answer (end points from the element's own
    /// formulas). The stall that led to the tolerance floor (a 0.45 m piece
    /// next to a marker ball on a real earth wire, 3e-12 m left at the
    /// iteration limit) arose inside a global iteration and is not
    /// reproduced by this isolated check.
    #[test]
    fn short_pieces_of_tight_wires_converge() {
        let (ea, w) = (2.0e7, 10.0);
        let mut worst: f64 = 0.0;
        for k in 0..200 {
            let h = 8_000.0 + 60.0 * k as f64; // H/w 800–2 000 m
            let v0 = -0.3 * h + 7.0 * k as f64;
            let l0 = 0.2 + 0.013 * k as f64;
            let (pos, _) = geometry(h, v0, l0, ea, w);
            let props = CableProperties {
                unstressed_length: l0,
                axial_stiffness: ea,
                load: Vector3::new(0.0, 0.0, -w),
            };
            let state = props
                .solve(
                    Vector3::new(pos.x, 0.0, pos.y),
                    None,
                    &default_element_budget(),
                )
                .unwrap_or_else(|e| panic!("H {h}, l0 {l0}: {e}"));
            worst = worst.max((state.horizontal - h).abs() / h);
        }
        assert!(worst < 1e-6, "{worst}");
    }

    fn close(a: f64, b: f64, tol: f64) {
        assert!(((a - b) / b).abs() <= tol, "actual {a}, expected {b}");
    }

    const W: f64 = 15.97;
    const EA: f64 = 34.64e6;

    fn gravity() -> Vector3<f64> {
        Vector3::new(0.0, 0.0, -W)
    }

    #[test]
    fn equilibrium_and_geometry_round_trip() {
        let props = CableProperties {
            unstressed_length: 300.5,
            axial_stiffness: EA,
            load: gravity(),
        };
        let d = Vector3::new(300.0, 0.0, 10.0);
        let s = props.solve(d, None, &default_element_budget()).unwrap();
        // The cable pulls its end nodes down by its own weight: f_i + f_j = q L0.
        let total = s.force_i + s.force_j - props.load * props.unstressed_length;
        assert!(total.norm() < 1e-6, "{total}");
        // Re-evaluating the closed form at (H, V0) gives back the chord.
        let (pos, _) = geometry(s.horizontal, s.v0, 300.5, EA, W);
        assert!((pos - Vector2::new(300.0, 10.0)).norm() < 1e-9);
    }

    #[test]
    fn stiffness_matches_finite_differences() {
        let props = CableProperties {
            unstressed_length: 410.0,
            axial_stiffness: EA,
            load: Vector3::new(4.0, -3.0, -W),
        };
        let d = Vector3::new(380.0, 120.0, -25.0);
        let budget = default_element_budget();
        let base = props.solve(d, None, &budget).unwrap();
        for axis in 0..3 {
            let mut dd = d;
            let eps = 1e-4;
            dd[axis] += eps;
            let moved = props
                .solve(dd, Some((base.horizontal, base.v0)), &budget)
                .unwrap();
            let numeric = (moved.force_j - base.force_j) / eps;
            // d(force_j)/d(x_j) = −K
            let analytic = -base.stiffness.column(axis);
            for k in 0..3 {
                assert!(
                    (numeric[k] - analytic[k]).abs() <= 1e-4 * base.stiffness.norm(),
                    "axis {axis} comp {k}: numeric {} analytic {}",
                    numeric[k],
                    analytic[k]
                );
            }
        }
    }

    #[test]
    fn inverse_finds_unstressed_length_for_tension() {
        let d = Vector3::new(300.0, 0.0, 0.0);
        let budget = Budget::new().max_iterations(200);
        let l0 =
            CableProperties::unstressed_length_for_tension(d, 28_000.0, EA, gravity(), &budget)
                .unwrap();
        let s = CableProperties {
            unstressed_length: l0,
            axial_stiffness: EA,
            load: gravity(),
        }
        .solve(d, None, &default_element_budget())
        .unwrap();
        close(s.horizontal, 28_000.0, 1e-9);
    }

    #[test]
    fn shape_ends_on_both_nodes() {
        let props = CableProperties {
            unstressed_length: 452.0,
            axial_stiffness: EA,
            load: Vector3::new(0.0, 6.0, -W),
        };
        let d = Vector3::new(450.0, 3.0, 35.0);
        let s = props.solve(d, None, &default_element_budget()).unwrap();
        let shape = props.shape(d, &s, 40);
        assert!(shape[0].norm() < 1e-12);
        assert!((shape[40] - d).norm() < 1e-8, "{}", shape[40] - d);
        let s_half = props.arc_length_at_fraction(d, &s, 0.5);
        assert!(s_half > 0.0 && s_half < 452.0);
    }

    #[test]
    fn rejects_vertical_cable_and_reports_exit() {
        let props = CableProperties {
            unstressed_length: 10.0,
            axial_stiffness: EA,
            load: gravity(),
        };
        assert_eq!(
            props.solve(
                Vector3::new(0.0, 0.0, -9.0),
                None,
                &default_element_budget()
            ),
            Err(ElementError::ParallelToLoad)
        );
        let error = props
            .solve(
                Vector3::new(9.0, 0.0, 0.0),
                Some((1.0, -50.0)),
                &Budget::new().max_iterations(1),
            )
            .unwrap_err();
        assert!(error.to_string().contains("did not converge"), "{error}");
    }
}
