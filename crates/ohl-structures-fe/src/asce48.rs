// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The shaft of a steel pole by **ASCE/SEI 48-19** (*Design of Steel
//! Transmission Pole Structures*, American Society of Civil Engineers,
//! 2020), Section 5.2 (owner 2026-11-03: "Can you look in the code what
//! would apply for pole structures and start implementing it", the
//! standard given for the local documents). Built, in active development,
//! not approved yet.
//!
//! At a cross-section of a regular polygonal or round tube, under the
//! axial force P, the bending moment M, the shear V and the torsion T:
//!
//! - **regular polygons up to 16 sides** (5.2.3.2.1, Table 5-1): the
//!   compressive stress permitted F_a by the flats' w/t·√(F_y/E) and the
//!   bend angle (360°/n): ≥ 45° Eqs. 5.2-6/7 when the axial stress is at
//!   most 6.9 MPa, else 5.2-8/9; 30° to 45° Eqs. 5.2-8/9; 22.5° to 30°
//!   Eqs. 5.2-10/11 — beyond their upper limit no stress is permitted
//!   (5.2.3.2: tests needed);
//! - **round tubes and polygons of more than 16 sides** (5.2.3.2.4): F_a
//!   by D_o/t (Eqs. 5.2-13/14), F_b (Eqs. 5.2-15/16), the interaction
//!   f_a/F_a + f_b/F_b ≤ 1 (Eq. 5.2-12);
//! - **combined stress** (5.2.6, Eqs. 5.2-20/21): √((P/A + Mc/I)² + 3
//!   (VQ/Ib + Tc/J)²) ≤ F_a (polygon) or F_b (round) on the compression
//!   side, F_y on the tension side; shear alone ≤ 0.58 F_y (5.2.4).
//!
//! E is the standard's 200 GPa (5.2.1.2) in its limits.

/// The standard's modulus of elasticity in its limits (5.2.1.2), Pa.
pub const E: f64 = 200e9;

/// The compressive stress permitted on the extreme fibre of a regular
/// polygonal tube of `sides` (up to 16) with flats of `w_t`, Pa, `fy` in
/// Pa, `fa` the axial compressive stress (Table 5-1 for 8 sides and
/// fewer), with the equation. Err: beyond the equations' range (the
/// standard permits it only by tests).
pub fn polygon_allowable(
    sides: u32,
    w_t: f64,
    fy: f64,
    fa: f64,
) -> Result<(f64, &'static str), String> {
    let x = w_t * (fy / E).sqrt();
    let bend = 360.0 / f64::from(sides.max(3));
    // (limit for F_y, upper limit, factor, slope, equations)
    let (lo, hi, k, s, eqs) = if bend >= 45.0 && fa <= 6.9e6 {
        (1.53, 2.06, 1.42, 0.194, ["5.2-6", "5.2-7"])
    } else if bend >= 30.0 {
        (1.41, 2.20, 1.45, 0.220, ["5.2-8", "5.2-9"])
    } else if bend >= 22.5 {
        (1.26, 2.42, 1.42, 0.233, ["5.2-10", "5.2-11"])
    } else {
        return Err(format!(
            "{sides} sides: the round tube's rules apply (5.2.3.2.4)"
        ));
    };
    if x <= lo {
        Ok((fy, eqs[0]))
    } else if x <= hi {
        Ok((k * fy * (1.0 - s * x), eqs[1]))
    } else {
        Err(format!(
            "w/t {w_t:.1} (w/t·√(F_y/E) {x:.2}) beyond Eq. {}'s range {hi}: only by tests (5.2.3.2)",
            eqs[1]
        ))
    }
}

/// F_a and F_b of a round tube (or a polygon of more than 16 sides) of
/// outside diameter over wall `do_t`, Pa (Eqs. 5.2-13 to 5.2-16). Err:
/// D_o/t above 0.414 E/F_y.
pub fn round_allowables(do_t: f64, fy: f64) -> Result<(f64, f64), String> {
    let r = E / fy;
    if do_t > 0.414 * r {
        return Err(format!(
            "D/t {do_t:.0} above 0.414 E/F_y = {:.0}: only by tests (5.2.3.2)",
            0.414 * r
        ));
    }
    let fa = if do_t <= 0.131 * r {
        fy
    } else {
        0.75 * fy + 0.0328 * E / do_t
    };
    let fb = if do_t <= 0.207 * r {
        fy
    } else {
        0.70 * fy + 0.0621 * E / do_t
    };
    Ok((fa, fb))
}

/// A cross-section of a hollow shaft.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shaft {
    /// Sides of a regular polygon (none: round).
    pub sides: Option<u32>,
    /// Outside diameter (across flats for a polygon), m.
    pub d: f64,
    /// Wall, m.
    pub t: f64,
    /// Inside bend radius, m (none: not known — sharp corners, the flats
    /// at their widest, on the safe side; above 4t taken as 4t).
    pub bend_radius: Option<f64>,
}

impl Shaft {
    fn solid(&self, d: f64) -> (f64, f64) {
        match self.sides {
            Some(n) => {
                let (r, tan) = (d / 2.0, (std::f64::consts::PI / f64::from(n)).tan());
                let a = f64::from(n) * r * r * tan;
                (a, a * r * r * (3.0 + tan * tan) / 12.0)
            }
            None => (
                std::f64::consts::PI / 4.0 * d * d,
                std::f64::consts::PI / 64.0 * d.powi(4),
            ),
        }
    }

    /// Area, m², and second moment, m⁴ (a regular polygon's is the same
    /// about every axis).
    pub fn area_inertia(&self) -> (f64, f64) {
        let (ao, io) = self.solid(self.d);
        let (ai, ii) = self.solid(self.d - 2.0 * self.t);
        (ao - ai, io - ii)
    }

    /// The farthest fibre from the axis, m (a polygon's corner).
    pub fn reach(&self) -> f64 {
        match self.sides {
            Some(n) => self.d / 2.0 / (std::f64::consts::PI / f64::from(n)).cos(),
            None => self.d / 2.0,
        }
    }

    /// The flat width over the wall (5.2.3.2.1): the inside face's side
    /// less the bends (the inside bend radius, at most 4t).
    pub fn w_t(&self) -> f64 {
        let n = f64::from(self.sides.unwrap_or(16));
        let half = std::f64::consts::PI / n;
        let side = (self.d - 2.0 * self.t) * half.tan();
        let r = self.bend_radius.unwrap_or(0.0).min(4.0 * self.t);
        // Each bend turns by 2π/n: its arc takes r·tan(π/n) off each end.
        ((side - 2.0 * r * half.tan()) / self.t).max(0.0)
    }
}

/// The usage of a shaft cross-section by ASCE/SEI 48-19 5.2, with what
/// governs: axial force `p` (compression positive), N; bending moment
/// `m`, N·m; shear `v`, N; torsion `t`, N·m; `fy`, Pa; `strength` the load
/// case's strength factor on the permitted stresses. Shear: VQ/Ib = 2V/A
/// of a thin tube, torsion Tc/J = T/(2 A_m t) (Bredt, A_m inside the
/// wall's midline).
pub fn usage(
    s: &Shaft,
    p: f64,
    m: f64,
    v: f64,
    torsion: f64,
    fy: f64,
    strength: f64,
) -> (f64, String) {
    let (a, i) = s.area_inertia();
    let fb = m.abs() * s.reach() / i;
    let fa = p / a;
    let dm = s.d - s.t;
    let am = match s.sides {
        Some(n) => f64::from(n) * (dm / 2.0).powi(2) * (std::f64::consts::PI / f64::from(n)).tan(),
        None => std::f64::consts::PI / 4.0 * dm * dm,
    };
    let tau = 2.0 * v.abs() / a + torsion.abs() / (2.0 * am * s.t);
    let mpa = |x: f64| x / 1e6;
    let combined = |sigma: f64| (sigma * sigma + 3.0 * tau * tau).sqrt();
    let compression = combined(fa.max(0.0) + fb);
    let tension = combined((-fa).max(0.0) + fb);
    let shear = tau / (strength * 0.58 * fy);
    let polygon = s.sides.filter(|n| *n <= 16);
    let mut worst = (
        tension / (strength * fy),
        format!(
            "tension side {:.0} MPa of F_y {:.0} MPa (5.2-20/21)",
            mpa(tension),
            mpa(fy)
        ),
    );
    let mut take = |u: f64, w: String| {
        if u > worst.0 || u.is_nan() {
            worst = (u, w);
        }
    };
    take(
        shear,
        format!("shear {:.0} MPa of 0.58 F_y (5.2-17)", mpa(tau)),
    );
    match polygon {
        Some(n) => match polygon_allowable(n, s.w_t(), fy, fa.max(0.0)) {
            Ok((fa_perm, eq)) => take(
                compression / (strength * fa_perm),
                format!(
                    "compression side {:.0} MPa of F_a {:.0} MPa (Eq. {eq}, w/t {:.1}; 5.2-20)",
                    mpa(compression),
                    mpa(fa_perm),
                    s.w_t()
                ),
            ),
            Err(why) => take(f64::INFINITY, why),
        },
        None => match round_allowables(s.d / s.t, fy) {
            Ok((fa_perm, fb_perm)) => {
                take(
                    fa.max(0.0) / (strength * fa_perm) + fb / (strength * fb_perm),
                    format!(
                        "f_a/F_a + f_b/F_b: {:.0}/{:.0} + {:.0}/{:.0} MPa (D/t {:.0}; 5.2-12)",
                        mpa(fa.max(0.0)),
                        mpa(fa_perm),
                        mpa(fb),
                        mpa(fb_perm),
                        s.d / s.t
                    ),
                );
                take(
                    compression / (strength * fb_perm),
                    format!(
                        "compression side {:.0} MPa of F_b {:.0} MPa (5.2-21)",
                        mpa(compression),
                        mpa(fb_perm)
                    ),
                );
            }
            Err(why) => take(f64::INFINITY, why),
        },
    }
    worst
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The permitted stresses by hand (ASCE/SEI 48-19 5.2.3.2), F_y 345
    /// MPa, E 200 GPa (√(F_y/E) = 0.041533):
    /// - 12 sides, w/t 30: x = 1.246 ≤ 1.41 → F_y; w/t 40: x = 1.661 →
    ///   1.45 × 345 × (1 − 0.220 × 1.661) = 317.4 MPa (5.2-9);
    /// - 8 sides, w/t 40, axial 5 MPa: 1.42 × 345 × (1 − 0.194 × 1.661) =
    ///   332.0 MPa (5.2-7); axial 10 MPa: Eqs. 5.2-8/9 as 12 sides;
    /// - w/t 60 on 12 sides: x = 2.49 beyond 2.20, refused;
    /// - round D/t 100: 0.131 E/F_y = 75.9 < 100 ≤ 240 → F_a = 0.75 ×
    ///   345 plus 0.0328 × 200000/100, 324.35 MPa; 0.207 E/F_y = 120 ≥
    ///   100 → F_b = F_y.
    #[test]
    fn permitted_stresses_by_hand() {
        let x = |w_t: f64| w_t * (345e6f64 / 200e9).sqrt();
        assert_eq!(polygon_allowable(12, 30.0, 345e6, 0.0).unwrap().0, 345e6);
        let (f, eq) = polygon_allowable(12, 40.0, 345e6, 0.0).unwrap();
        assert!((f - 1.45 * 345e6 * (1.0 - 0.220 * x(40.0))).abs() < 1e-3);
        assert_eq!(eq, "5.2-9");
        let (f, eq) = polygon_allowable(8, 40.0, 345e6, 5e6).unwrap();
        assert!((f - 1.42 * 345e6 * (1.0 - 0.194 * x(40.0))).abs() < 1e-3);
        assert_eq!(eq, "5.2-7");
        assert_eq!(polygon_allowable(8, 40.0, 345e6, 10e6).unwrap().1, "5.2-9");
        assert!(polygon_allowable(12, 60.0, 345e6, 0.0).is_err());
        let (fa, fb) = round_allowables(100.0, 345e6).unwrap();
        assert!((fa - (0.75 * 345e6 + 0.0328 * 200e9 / 100.0)).abs() < 1e-3);
        assert_eq!(fb, 345e6);
        assert!(round_allowables(300.0, 345e6).is_err());
    }

    /// A 12-sided shaft (500 mm across flats, 8 mm wall, sharp corners)
    /// under bending only: the compression side Mc/I against F_y (w/t
    /// 16.5: x 0.69 ≤ 1.41), and a bend radius shortens the flats.
    #[test]
    fn shaft_usage_by_hand() {
        let s = Shaft {
            sides: Some(12),
            d: 0.5,
            t: 0.008,
            bend_radius: None,
        };
        let (_, i) = s.area_inertia();
        let m = 100e3;
        let (u, w) = usage(&s, 0.0, m, 0.0, 0.0, 345e6, 1.0);
        assert!((u - m * s.reach() / i / 345e6).abs() < 1e-12, "{w}");
        let half = std::f64::consts::PI / 12.0;
        assert!((s.w_t() - 0.484 * half.tan() / 0.008).abs() < 1e-9);
        let bent = Shaft {
            bend_radius: Some(0.05),
            ..s
        };
        assert!((bent.w_t() - (0.484 - 2.0 * 0.032) * half.tan() / 0.008).abs() < 1e-9);
    }
}
