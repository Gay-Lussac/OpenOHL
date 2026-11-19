// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **A bent plate's bend** (the interface's `joint-check-bends`, 2026-11-19,
//! the owner's decision of that day): the plate's section along its bend
//! line — width w the bend's length, depth t the plate's thickness — under
//! what OpenOHL works out per case: the force P across the bend, the shear
//! V along it, the moment M about the bend line and M_in in the plate's
//! plane.
//!
//! - **Axial force and bending**: ASCE 10-97 3.13, Eq. 3.13-1, P/P_a +
//!   M/M_a + M_in/M_a,in ≤ 1, with P_a = F_y A on the gross section (3.10.1:
//!   no hole at the bend; in compression too — the bend is short across
//!   its section, the flats' own strut checks take their buckling) and the
//!   allowable moments F_y S of the gross section (3.14.1; 3.14.2 and
//!   3.14.3: F_b = F_y, a plate bent about either axis being laterally
//!   supported), S = w t²/6 and t w²/6 — the stresses at the extreme fibre
//!   added: the worst corner of the section.
//! - **With the shear along the bend**: the standard gives no rule for a
//!   plate's shear with its bending; the yield criterion of von Mises
//!   (1913), √(σ² + 3 τ²) ≤ F_y, σ the normal stress of Eq. 3.13-1 at the
//!   extreme fibre and τ = V/A taken there too (its average over the
//!   section, its largest of a thin plate in its own plane) — on the safe
//!   side: the peak normal stress and the shear at one point. It is Eq.
//!   3.13-1 itself where V = 0.
//!
//! The standard gives no least inside radius for a bent connection plate:
//! the radius is the fabricator's (said).

/// One bend as the interface gives it.
#[derive(Debug, Clone, PartialEq)]
pub struct Bend {
    /// The two flats, in words.
    pub part: String,
    /// Thickness, m.
    pub thickness: f64,
    /// Yield strength, Pa.
    pub fy: Option<f64>,
    /// The bend's length (the section's width), m.
    pub width: f64,
    /// Its inside radius, m.
    pub inner_radius: f64,
    /// A weld on either flat.
    pub welded: bool,
}

/// What acts on the section in one case: across (N, tension +), along
/// (N), about the bend line (N m), in the plate's plane (N m).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BendLoad {
    pub axial: f64,
    pub shear: f64,
    pub moment: f64,
    pub moment_in_plane: f64,
}

/// A bend's result: per case its usage and check, what governs, notes,
/// what is not on the safe side.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BendResult {
    pub usages: Vec<Option<(f64, String)>>,
    pub governs: String,
    pub notes: Vec<String>,
    pub not_safe: Vec<String>,
}

/// The bend `b` in every case (`strengths`: each case's strength factor).
pub fn check_bend(b: &Bend, loads: &[BendLoad], strengths: &[f64]) -> BendResult {
    let mut out = BendResult {
        notes: vec![format!(
            "inside radius {:.0} mm ({:.1} t): ASCE 10-97 gives no least bend radius for a \
             connection plate (the fabricator's)",
            b.inner_radius * 1000.0,
            b.inner_radius / b.thickness.max(1e-12)
        )],
        ..BendResult::default()
    };
    let Some(fy) = b.fy else {
        out.not_safe
            .push("the bend's bending not checked (no yield strength)".into());
        out.usages = vec![None; strengths.len()];
        return out;
    };
    let (w, t) = (b.width, b.thickness);
    if w <= 0.0 || t <= 0.0 {
        out.not_safe
            .push("the bend's bending not checked (its section not known)".into());
        out.usages = vec![None; strengths.len()];
        return out;
    }
    let a = w * t;
    let s = w * t * t / 6.0;
    let s_in = t * w * w / 6.0;
    let mut worst: Option<f64> = None;
    for (n, strength) in strengths.iter().enumerate() {
        let Some(l) = loads.get(n) else {
            out.usages.push(None);
            continue;
        };
        let sigma = l.axial.abs() / a + l.moment.abs() / s + l.moment_in_plane.abs() / s_in;
        let tau = l.shear.abs() / a;
        let eq = (sigma * sigma + 3.0 * tau * tau).sqrt();
        let r = fy * strength;
        let u = if r > 0.0 { eq / r } else { f64::INFINITY };
        let words = format!(
            "the bend of {} ({:.0} × {:.0} mm): P {:.1} kN, M {:.2} kN m, M in its plane {:.2} kN m, \
             V {:.1} kN — σ = P/A + M/S + M_in/S_in {:.0} MPa, τ = V/A {:.0} MPa, √(σ² + 3τ²) \
             {:.0} MPa of F_y {:.0} MPa (ASCE 10-97 3.13-1 with 3.10.1 and 3.14.1–3.14.3, the \
             gross elastic section; the shear by von Mises' criterion, no rule in the standard)",
            b.part,
            w * 1000.0,
            t * 1000.0,
            l.axial / 1000.0,
            l.moment / 1000.0,
            l.moment_in_plane / 1000.0,
            l.shear / 1000.0,
            sigma / 1e6,
            tau / 1e6,
            eq / 1e6,
            fy / 1e6
        );
        if worst.is_none_or(|x| crate::check::usage_order(u, x).is_gt()) {
            worst = Some(u);
            out.governs = words.clone();
        }
        out.usages.push(Some((u, words)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bend() -> Bend {
        Bend {
            part: "P2/P1".into(),
            thickness: 0.010,
            fy: Some(355e6),
            width: 0.2,
            inner_radius: 0.02,
            welded: false,
        }
    }

    /// By hand: 200 × 10 mm, F_y 355 MPa. P 100 kN alone: 100e3/2e-3 =
    /// 50 MPa, 14.08 %. M 0.5 kN m: S = 0.2 × 0.01² / 6 = 3.333e-6 m³,
    /// 150 MPa; with P: 200 MPa, 56.34 % (Eq. 3.13-1). V 100 kN alone:
    /// τ 50 MPa, √3 × 50 = 86.6 MPa, 24.40 %. A strength factor 0.9
    /// divides it.
    #[test]
    fn a_bend_by_hand() {
        let b = bend();
        let l = |axial, shear, moment| BendLoad {
            axial,
            shear,
            moment,
            moment_in_plane: 0.0,
        };
        let r = check_bend(
            &b,
            &[
                l(100e3, 0.0, 0.0),
                l(-100e3, 0.0, 500.0),
                l(0.0, 100e3, 0.0),
                l(100e3, 0.0, 500.0),
            ],
            &[1.0, 1.0, 1.0, 0.9],
        );
        let u: Vec<f64> = r.usages.iter().map(|x| x.as_ref().unwrap().0).collect();
        assert!((u[0] - 50.0 / 355.0).abs() < 1e-12, "{u:?}");
        assert!((u[1] - 200.0 / 355.0).abs() < 1e-9, "{u:?}");
        assert!((u[2] - 3f64.sqrt() * 50.0 / 355.0).abs() < 1e-9, "{u:?}");
        assert!((u[3] - 200.0 / 355.0 / 0.9).abs() < 1e-9, "{u:?}");
        assert!(r.governs.contains("3.13-1"), "{}", r.governs);
        assert!(r.not_safe.is_empty());
        // No yield strength: not checked, said.
        let r = check_bend(&Bend { fy: None, ..bend() }, &[l(1.0, 0.0, 0.0)], &[1.0]);
        assert!(r.usages[0].is_none() && !r.not_safe.is_empty());
    }
}
