// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Series impedance per unit length between two wires above a homogeneous
//! earth, and their potential coefficient.
//!
//! The impedance of wire \\( i \\) with wire \\( j \\) (for \\( i = j \\)
//! the self impedance, its own resistance left out), Ω/m:
//!
//! \\[ Z_{ij} = j\frac{\omega\mu_0}{2\pi}\ln\frac{D'_{ij}}{d_{ij}} + \Delta Z_{ij}, \\]
//!
//! with \\( d_{ij} \\) the distance between the wires (the geometric mean
//! radius for \\( i = j \\)), \\( D'_{ij} \\) the distance from wire
//! \\( i \\) to the image of wire \\( j \\) below a perfectly conducting
//! earth (\\( 2h_i \\) for \\( i = j \\)), and \\( \Delta Z_{ij} \\) the
//! correction for the earth's finite conductivity, by one of the three
//! [`EarthReturn`] methods:
//!
//! - **Carson's series** (J. R. Carson, "Wave propagation in overhead wires
//!   with ground return", Bell System Technical Journal 5 (4), 1926,
//!   pp. 539–554): \\( \Delta Z = \frac{\omega\mu_0}{\pi}(P + jQ) \\) with
//!   \\( P, Q \\) the series in \\( a = 4\pi\sqrt{5}\cdot 10^{-4}\,
//!   D'\sqrt{f/\rho} \\) (\\( D' \\) in m, \\( \rho \\) in Ω·m) and the
//!   angle \\( \theta \\) of the image seen from the wire
//!   ([`carson_pq`]); for \\( a > 15 \\) its asymptotic expansion.
//! - **Complex depth** (C. Dubanton 1969; A. Deri, G. Tevan, A. Semlyen,
//!   A. Castanheira, "The complex ground return plane: a simplified model
//!   for homogeneous and multi-layer earth return", IEEE Trans. PAS-100
//!   (8), 1981, pp. 3686–3693): the earth replaced by a perfect conductor
//!   at the complex depth \\( p = \sqrt{\rho/(j\omega\mu_0)} \\),
//!   \\( Z_{ij} = j\frac{\omega\mu_0}{2\pi}\ln\frac{\sqrt{(h_i+h_j+2p)^2 +
//!   x_{ij}^2}}{d_{ij}} \\).
//! - **Carson's first terms** ("modified Carson"):
//!   \\( P = \pi/8 \\), \\( Q = (\tfrac12 - \gamma)/2 + \tfrac12\ln(2/a) \\)
//!   — the heights then cancel out of \\( Z_{ij} \\).
//!
//! The potential coefficients (perfectly conducting earth, air of
//! permittivity \\( \varepsilon_0 \\)), m/F:
//! \\( P_{ij} = \frac{1}{2\pi\varepsilon_0}\ln\frac{D'_{ij}}{d_{ij}} \\), the
//! wire's radius for \\( d_{ii} \\).

use std::f64::consts::{PI, SQRT_2};

use nalgebra::Complex;
use ohl_line::electrical::EarthReturn;

/// Permeability of vacuum, H/m (\\( 4\pi\cdot 10^{-7} \\); the SI value
/// since 2019 differs by about \\( 10^{-10} \\) relatively).
pub const MU0: f64 = 4e-7 * PI;

/// Permittivity of vacuum, F/m (CODATA 2018), taken for air.
pub const EPS0: f64 = 8.854_187_812_8e-12;

/// Euler's constant.
const EULER_GAMMA: f64 = std::f64::consts::EULER_GAMMA;

/// The argument below which Carson's series is summed, above which his
/// asymptotic expansion is taken.
pub const SERIES_LIMIT: f64 = 15.0;

/// Carson's parameter \\( a = 4\pi\sqrt{5}\cdot 10^{-4}\,D\sqrt{f/\rho} \\)
/// (\\( D \\) to the image, m; \\( f \\) Hz; \\( \rho \\) Ω·m), which is
/// \\( D\sqrt{\omega\mu_0/\rho} \\).
pub fn carson_parameter(distance_to_image: f64, frequency_hz: f64, rho_ohm_m: f64) -> f64 {
    distance_to_image * (2.0 * PI * frequency_hz * MU0 / rho_ohm_m).sqrt()
}

/// Carson's correction terms \\( (P, Q) \\) for the parameter `a` and the
/// angle `theta` (radians) between the vertical and the line from a wire
/// to the other's image.
///
/// For \\( a \le 15 \\) the series
///
/// \\[ P = \frac{\pi}{8} - b_1 a\cos\theta + b_2\left[(c_2 - \ln a)a^2\cos 2\theta + \theta a^2 \sin 2\theta\right] + b_3 a^3\cos 3\theta - d_4 a^4 \cos 4\theta - \dots \\]
/// \\[ Q = \frac{1}{2}\left(\frac12 - \gamma\right) + \frac12\ln\frac{2}{a} + b_1 a\cos\theta - d_2 a^2\cos 2\theta + b_3 a^3\cos 3\theta - b_4\left[(c_4 - \ln a)a^4\cos 4\theta + \theta a^4\sin 4\theta\right] + \dots \\]
///
/// with the pattern repeating every four terms,
/// \\( b_1 = \sqrt2/6 \\), \\( b_2 = 1/16 \\),
/// \\( |b_i| = |b_{i-2}|/(i(i+2)) \\), the sign of \\( b_i \\) + for
/// \\( i \\) = 1–4, − for 5–8, + for 9–12, …;
/// \\( c_2 = \frac54 - \gamma + \ln 2 \\) (1.3659315),
/// \\( c_i = c_{i-2} + 1/i + 1/(i+2) \\), \\( d_i = \frac{\pi}{4} b_i \\);
/// summed until the terms fall below \\( 10^{-17} \\). In double precision
/// it stays within \\( 10^{-10} \\) of the integral up to \\( a = 15 \\)
/// (cancellation grows beyond). For \\( a > 15 \\) the asymptotic
/// expansion (within \\( 10^{-6} \\) there, closer above)
///
/// \\[ P = \frac{\cos\theta}{\sqrt2\,a} - \frac{\cos 2\theta}{a^2} + \frac{\cos 3\theta}{\sqrt2\,a^3} + \frac{3\cos 5\theta}{\sqrt2\,a^5} - \frac{45\cos 7\theta}{\sqrt2\,a^7}, \\]
/// \\[ Q = \frac{\cos\theta}{\sqrt2\,a} - \frac{\cos 3\theta}{\sqrt2\,a^3} + \frac{3\cos 5\theta}{\sqrt2\,a^5} + \frac{45\cos 7\theta}{\sqrt2\,a^7}. \\]
///
/// Both are checked against a numerical integration of Carson's integral
/// (`benchmarks/electrical/reference_electrical.py`).
pub fn carson_pq(a: f64, theta: f64) -> (f64, f64) {
    if a > SERIES_LIMIT {
        let (c1, c2, c3, c5, c7) = (
            theta.cos(),
            (2.0 * theta).cos(),
            (3.0 * theta).cos(),
            (5.0 * theta).cos(),
            (7.0 * theta).cos(),
        );
        let p = c1 / (SQRT_2 * a) - c2 / a.powi(2)
            + c3 / (SQRT_2 * a.powi(3))
            + 3.0 * c5 / (SQRT_2 * a.powi(5))
            - 45.0 * c7 / (SQRT_2 * a.powi(7));
        let q = c1 / (SQRT_2 * a) - c3 / (SQRT_2 * a.powi(3))
            + 3.0 * c5 / (SQRT_2 * a.powi(5))
            + 45.0 * c7 / (SQRT_2 * a.powi(7));
        return (p, q);
    }
    let mut p = PI / 8.0;
    let mut q = 0.5 * (0.5 - EULER_GAMMA) + 0.5 * (2.0 / a).ln();
    if a <= 0.0 {
        return (p, q);
    }
    let ln_a = a.ln();
    // b_{i-2}, b_{i-1}; c of the last even index.
    let (mut b_prev2, mut b_prev1) = (0.0_f64, 0.0_f64);
    let mut c_even = 1.25 - EULER_GAMMA + 2f64.ln();
    let mut ai = 1.0;
    for i in 1..=200u32 {
        let fi = f64::from(i);
        ai *= a;
        let sign = if ((i - 1) / 4) % 2 == 0 { 1.0 } else { -1.0 };
        let b = match i {
            1 => SQRT_2 / 6.0,
            2 => 1.0 / 16.0,
            _ => sign * b_prev2.abs() / (fi * (fi + 2.0)),
        };
        if i % 2 == 0 && i > 2 {
            c_even += 1.0 / fi + 1.0 / (fi + 2.0);
        }
        let d = PI / 4.0 * b;
        let (cos, sin) = ((fi * theta).cos(), (fi * theta).sin());
        let log_term = (c_even - ln_a) * ai * cos + theta * ai * sin;
        let (dp, dq) = match i % 4 {
            1 => (-b * ai * cos, b * ai * cos),
            2 => (b * log_term, -d * ai * cos),
            3 => (b * ai * cos, b * ai * cos),
            _ => (-d * ai * cos, -b * log_term),
        };
        p += dp;
        q += dq;
        if i > 8 && (b.abs() * ai * (1.0 + (c_even - ln_a).abs() + theta.abs())) < 1e-17 {
            break;
        }
        b_prev2 = b_prev1;
        b_prev1 = b;
    }
    (p, q)
}

/// The external impedance per unit length between a wire at (`xi`,
/// `hi`) and one at (`xj`, `hj`) (across the line, height above the
/// earth; m) whose distance is `d` (the geometric mean radius for a wire
/// with itself), Ω/m: everything but the wire's own resistance.
pub fn impedance(
    method: EarthReturn,
    [xi, hi]: [f64; 2],
    [xj, hj]: [f64; 2],
    d: f64,
    frequency_hz: f64,
    rho_ohm_m: f64,
) -> Complex<f64> {
    let omega = 2.0 * PI * frequency_hz;
    let x = xi - xj;
    let k = omega * MU0 / (2.0 * PI);
    match method {
        EarthReturn::ComplexDepth => {
            let p = (Complex::new(rho_ohm_m, 0.0) / Complex::new(0.0, omega * MU0)).sqrt();
            let h = Complex::new(hi + hj, 0.0) + p * 2.0;
            let far = (h * h + Complex::new(x * x, 0.0)).sqrt();
            Complex::new(0.0, k) * (far / d).ln()
        }
        EarthReturn::CarsonSeries | EarthReturn::CarsonFirstTerms => {
            let image = (hi + hj).hypot(x);
            let a = carson_parameter(image, frequency_hz, rho_ohm_m);
            let (p, q) = match method {
                EarthReturn::CarsonSeries => carson_pq(a, x.abs().atan2(hi + hj)),
                _ => (PI / 8.0, 0.5 * (0.5 - EULER_GAMMA) + 0.5 * (2.0 / a).ln()),
            };
            Complex::new(0.0, k * (image / d).ln()) + Complex::new(p, q) * (omega * MU0 / PI)
        }
    }
}

/// The potential coefficient between a wire at (`xi`, `hi`) and one at
/// (`xj`, `hj`) whose distance is `d` (the wire's radius for a wire with
/// itself), m/F: a perfectly conducting earth, air of permittivity
/// [`EPS0`].
pub fn potential_coefficient([xi, hi]: [f64; 2], [xj, hj]: [f64; 2], d: f64) -> f64 {
    let image = (hi + hj).hypot(xi - xj);
    (image / d).ln() / (2.0 * PI * EPS0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first terms of the series as an open source prints them
    /// (Vismor, "Carson's Equations", eqs. 14–15, to the fourth power):
    /// for a small `a` the summed series agrees with them to the fifth
    /// power of `a`.
    #[test]
    fn series_agrees_with_its_first_terms() {
        let (a, t) = (0.3_f64, 0.4_f64);
        let k = 1.0 / (3.0 * SQRT_2);
        let p4 = PI / 8.0 - k * a * t.cos()
            + a * a * (2.0 * t).cos() * (0.6728 + (2.0 / a).ln()) / 16.0
            + a * a * t * (2.0 * t).sin() / 16.0
            + a.powi(3) * (3.0 * t).cos() / (45.0 * SQRT_2)
            - a.powi(4) * PI * (4.0 * t).cos() / 1536.0;
        let q4 = -0.0386 + 0.5 * (2.0 / a).ln() + k * a * t.cos()
            - PI * a * a * (2.0 * t).cos() / 64.0
            + a.powi(3) * (3.0 * t).cos() / (45.0 * SQRT_2)
            - a.powi(4) * t * (4.0 * t).sin() / 384.0
            - a.powi(4) * (4.0 * t).cos() * (1.0895 + (2.0 / a).ln()) / 384.0;
        let (p, q) = carson_pq(a, t);
        // Rounded constants (0.6728, 0.0386, 1.0895) and the fifth power.
        assert!((p - p4).abs() < 2e-4, "{p} {p4}");
        assert!((q - q4).abs() < 2e-4, "{q} {q4}");
    }

    /// The series and the asymptotic expansion meet at a = 15 (to the
    /// accuracy of the asymptotic expansion there).
    #[test]
    fn series_meets_asymptote() {
        for t in [0.0, 0.3, 0.8] {
            let below = carson_pq(SERIES_LIMIT, t);
            let above = carson_pq(SERIES_LIMIT * (1.0 + 1e-12), t);
            assert!(
                (below.0 - above.0).abs() < 1e-5 * below.0.abs(),
                "{t}: {below:?} {above:?}"
            );
            assert!(
                (below.1 - above.1).abs() < 1e-5 * below.1.abs(),
                "{t}: {below:?} {above:?}"
            );
        }
    }

    /// The first terms' impedance does not depend on the heights:
    /// \\( j\frac{\omega\mu_0}{2\pi}\ln(D_e/d) + \omega\mu_0/8 \\) with
    /// \\( D_e = 2 e^{(1/2-\gamma)}/\sqrt{\omega\mu_0/\rho} \\).
    #[test]
    fn first_terms_independent_of_height() {
        let a = impedance(
            EarthReturn::CarsonFirstTerms,
            [0.0, 10.0],
            [3.0, 12.0],
            3.6,
            50.0,
            100.0,
        );
        let b = impedance(
            EarthReturn::CarsonFirstTerms,
            [0.0, 30.0],
            [3.0, 32.0],
            3.6,
            50.0,
            100.0,
        );
        assert!((a - b).norm() < 1e-15);
        let omega = 2.0 * PI * 50.0;
        assert!((a.re - omega * MU0 / 8.0).abs() < 1e-15);
    }
}
