// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The SIA 263 check by hand: the tests of the core's code
//! (`ohl_structures_fe::sia263`, `sia263_tests`, `plates`), moved with it
//! (2026-11-18) — the same data, the same values: an angle's buckling
//! about its minor axis with 0.9 L (5.4.1.6), the bending of its eccentric
//! connection (5.4.1.2, formula 50), a class 4 angle by (62)/(63), the
//! tension by (38), (39) and 6.2.4.4 (84), the bolts by Table 16, a
//! gusset plate and a fillet weld.

use crate::joints::{
    self, BoltGroup, PlateSteel, WeldInput, WeldKind, bolted, contact_width, weld,
};
use crate::model::{
    Bolts, Condition, ConnectedLeg, EndConditions, Grade, Member, MemberDesign, MemberKind,
    OntoLeg, Section, Source,
};
use crate::params::{GAMMA_M1, GAMMA_M2};
use crate::sia263::{self, Curve, Element, K_INTERNAL, K_OUTSTAND, chi, epsilon, eta, omega, rho};

const E: f64 = 210e9;

/// The conditions OpenOHL works out for a member with
/// `end_eccentricity = "both_ends"` and no connections.
fn both_eccentric(given: bool) -> EndConditions {
    let c = Condition {
        value: Some(given),
        source: Source::Member,
        why: None,
    };
    EndConditions {
        eccentric: [c.clone(), c],
        connected: [false; 2],
    }
}

fn angle(h: f64, t: f64, fy: f64, length: f64) -> (Member, ohl_sections::Properties) {
    let p = ohl_sections::Section {
        id: "L".into(),
        name: String::new(),
        description: String::new(),
        shape: ohl_sections::Shape::EqualAngle { h, t, r: 0.0 },
        material: None,
        given: ohl_sections::Given::default(),
    };
    let q = p.properties();
    let s = Section {
        area: q.area,
        iy: q.iy,
        iz: q.iz,
        j: q.it.unwrap_or(0.0),
        i_min: q.iv,
        wy: q.wel_y,
        wz: q.wel_z,
        e: E,
        g: E / 2.6,
        yield_strength: Some(fy),
        ultimate: Some(510e6),
        tension: None,
        compression: None,
        moment_y: None,
        moment_z: None,
        properties: Some(q.clone()),
        profile: Some(p),
    };
    let m = Member {
        id: "D".into(),
        kind: MemberKind::Truss,
        frame: false,
        design: MemberDesign {
            length_ratios: Some([1.0; 3]),
            connected_leg: Some(ConnectedLeg::Long),
            end_distance: Some(0.025),
            edge_distance: Some(0.030),
            bolts: Some(Bolts {
                diameter: 0.016,
                hole: None,
                count: 1,
                shear_planes: 1,
                shear_capacity: None,
                ultimate: Some(800e6),
                holes: Some(1),
                grade: Grade::parse("8.8"),
            }),
            ..MemberDesign::default()
        },
        section: s,
        length,
        k: 1.0,
        lk: None,
        capacity_tension: None,
        capacity_compression: None,
        joints: ["a".into(), "b".into()],
        conditions: both_eccentric(true),
        onto: [None, None],
    };
    (m, q)
}

/// L 60×6, S 355, 2 m, both ends eccentric: N_K,Rd about v with L_K = 0.9 L
/// on curve c (class 3); the eccentricity N·e of the bolted leg's gauge in
/// formula (50); the bolt by (73) and bearing (76); tension by (38), (39),
/// (84).
#[test]
fn eccentric_single_angle_by_hand() {
    let (fy, fu) = (355e6, 510e6);
    let (m, q) = angle(0.060, 0.006, fy, 2.0);
    let (c, st) = sia263::capacity(&m, None);
    let st = st.expect("stability data");
    let lk_v = 0.9 * 2.0;
    let lb = lk_v / (q.iv / q.area).sqrt() / (std::f64::consts::PI * (E / fy).sqrt());
    let nk = chi(lb, 0.49) * fy * q.area / GAMMA_M1;
    assert!((st.n_k - nk).abs() < 1e-6 * nk, "{} {nk}", st.n_k);
    assert!(!st.class4);
    let a16 = std::f64::consts::PI * 0.016f64.powi(2) / 4.0;
    let shear = 0.6 * 800e6 * 0.6 * a16 / GAMMA_M2;
    let bearing = f64::min(
        0.85 * 0.025 / 0.018 * fu * 0.016 * 0.006,
        2.4 * fu * 0.016 * 0.006,
    ) / GAMMA_M2;
    let comp = c.compression.as_ref().unwrap();
    assert!(
        (comp.0 - nk.min(shear).min(bearing)).abs() < 1e-6,
        "{comp:?}"
    );
    let one = 0.9 * fu * (2.0 * 0.030 - 0.018) * 0.006 / GAMMA_M2;
    let net = 0.9 * fu * (q.area - 0.018 * 0.006) / GAMMA_M2;
    let gross = fy * q.area / GAMMA_M1;
    let t = c.tension.as_ref().unwrap();
    let want = one.min(net).min(gross).min(shear).min(bearing);
    assert!((t.0 - want).abs() < 1e-6, "{t:?} {want}");
    let (dy, dz) = (0.040 - q.centroid[0], -q.centroid[1]);
    let th = q.principal_angle;
    let (eu, ev) = (
        dy * th.cos() + dz * th.sin(),
        -dy * th.sin() + dz * th.cos(),
    );
    assert!(st.eccentric);
    assert!((st.axes[0].e - ev).abs() < 1e-12 && (st.axes[1].e - eu).abs() < 1e-12);
    assert!(
        c.compression_assumed
            .iter()
            .any(|a| a.contains("gauge the farthest Table 17 allows")),
        "{:?}",
        c.compression_assumed
    );
    let n = 20e3;
    let mut want = n / st.n_k;
    for ax in &st.axes {
        let m_ed = n * ax.e.abs();
        let mrd = match ax.lt {
            Some((s0, a)) => {
                let lbd = (fy / s0 * ax.w / ax.w_el).sqrt();
                sia263::chi_lt(lbd, a) * ax.m_rd
            }
            None => ax.m_rd,
        };
        want += m_ed / ((1.0 - n / ax.n_cr) * mrd);
    }
    let (u, words) = st.usage(-n, [[0.0; 2]; 2], 1.0);
    assert!((u - want).abs() < 1e-12, "{u} {want} {words}");
    assert!(u > n / st.n_k * 1.5, "the eccentricity matters: {u}");
    assert!(words.contains("formula 50"), "{words}");
    // Concentric (both legs bolted): no moment, buckling over L about v.
    let mut m2 = m.clone();
    m2.conditions = both_eccentric(false);
    m2.design.connected_leg = Some(ConnectedLeg::Both);
    let (_, st2) = sia263::capacity(&m2, None);
    let st2 = st2.unwrap();
    assert!(!st2.eccentric && st2.n_k < st.n_k);
}

/// L 100×6, S 355: class 4; A_eff from each leg's ρ; compression by
/// (62)/(63) with the bow of curve c (L/200).
#[test]
fn class_4_angle_by_hand() {
    let fy = 355e6;
    let (h, t) = (0.100, 0.006);
    let (m, q) = angle(h, t, fy, 2.0);
    let (c, st) = sia263::capacity(&m, None);
    let st = st.unwrap();
    assert!(st.class4, "{c:?}");
    let b = h - t / 2.0;
    let r = rho(b / t, 0.426, fy);
    assert!(r < 1.0);
    let a_eff = q.area - 2.0 * (1.0 - r) * b * t;
    assert!((st.a_eff - a_eff).abs() < 1e-12, "{} {a_eff}", st.a_eff);
    assert!(c.compression.unwrap().0 <= st.n_k);
    let n = 30e3;
    let (u, words) = st.usage(-n, [[0.0; 2]; 2], 1.0);
    let mut worst: f64 = 0.0;
    for k in 0..2 {
        let mut sigma = n / st.a_eff;
        for (a, ax) in st.axes.iter().enumerate() {
            let amp = if a == k {
                1.0 / (1.0 - n / ax.n_cr)
            } else {
                1.0
            };
            sigma += amp * n * (ax.e.abs() + ax.w0) / ax.w;
        }
        worst = worst.max(sigma / (fy / GAMMA_M1));
    }
    assert!((u - worst).abs() < 1e-12, "{u} {worst} {words}");
    assert!((st.axes[1].w0 - 2.0 / 200.0).abs() < 1e-12);
}

/// The angle bolted at its top end onto a leg of 4 mm (thinner than its
/// 6 mm): the bearing (76) on that leg, with this member's e_1, governs.
#[test]
fn bearing_on_the_thinner_leg_bolted_onto_by_hand() {
    let (mut m, _) = angle(0.060, 0.006, 355e6, 2.0);
    m.design.ends[1] = Some(crate::model::EndDesign {
        onto: Some(crate::model::Onto::Member),
        ..crate::model::EndDesign::default()
    });
    m.conditions.connected[1] = true;
    m.onto[1] = Some(OntoLeg {
        id: "G".into(),
        thickness: 0.004,
        fu: Some(510e6),
    });
    let (c, _) = sia263::capacity(&m, None);
    let d0 = sia263::standard_hole(0.016).unwrap();
    let fu = 510e6;
    let one = |t: f64| (0.85 * 0.025 / d0 * fu * 0.016 * t).min(2.4 * fu * 0.016 * t) / GAMMA_M2;
    let (t, what) = c.tension.clone().unwrap();
    assert!((t - one(0.004)).abs() < 1e-6, "{t} {what} {}", one(0.004));
    assert_eq!(what, "bearing at b");
}

/// χ_K at the curves' printed values; λ_E, ε, η, ω, ρ at their tables'.
#[test]
fn buckling_curves_by_hand() {
    for c in [Curve::A, Curve::B, Curve::C, Curve::D] {
        assert!((chi(0.2, c.alpha()) - 1.0).abs() < 1e-12);
    }
    let phi: f64 = 0.5 * (1.0 + 0.49 * 0.8 + 1.0);
    let by_hand = 1.0 / (phi + (phi * phi - 1.0).sqrt());
    assert!((chi(1.0, 0.49) - by_hand).abs() < 1e-12);
    assert!((by_hand - 0.540).abs() < 0.001, "{by_hand}");
    let le = std::f64::consts::PI * (210e9f64 / 235e6).sqrt();
    assert!((le - 93.9).abs() < 0.05, "{le}");
    assert!((epsilon(355e6) - 0.814).abs() < 0.001);
    assert!((eta(1.0) - 1.0).abs() < 1e-12);
    assert!((eta(0.0) - 1.75).abs() < 1e-12);
    assert!((eta(-0.5) - 2.35).abs() < 1e-12);
    assert!((eta(-1.0) - 2.35).abs() < 1e-12);
    assert!((omega(-1.0) - 0.4).abs() < 1e-12);
    assert!((omega(1.0) - 1.0).abs() < 1e-12);
    assert!((rho(16.0, K_OUTSTAND, 235e6) - 1.0).abs() < 1e-12);
    assert!(rho(17.0, K_OUTSTAND, 235e6) < 1.0 && rho(16.6, K_OUTSTAND, 235e6) == 1.0);
    assert!(rho(41.0, K_INTERNAL, 355e6) == 1.0 && rho(42.0, K_INTERNAL, 355e6) < 1.0);
}

/// Classes by Tables 5a and 5b.
#[test]
fn classes_by_the_tables() {
    let fy = 355e6;
    let e = epsilon(fy);
    assert_eq!(Element::Outstand(9.0 * e).class(fy), 1);
    assert_eq!(Element::Outstand(9.5 * e).class(fy), 2);
    assert_eq!(Element::Outstand(14.0 * e).class(fy), 3);
    assert_eq!(Element::Outstand(14.1 * e).class(fy), 4);
    assert_eq!(Element::Internal(42.0 * e).class(fy), 3);
    assert_eq!(Element::Tube(91.0 * e * e).class(fy), 4);
}

/// Table 17 by size; another size none; the shear factor by grade.
#[test]
fn bolt_table() {
    assert_eq!(sia263::bolt_minimums(0.020), Some([0.030, 0.045, 0.025]));
    assert_eq!(sia263::standard_hole(0.016), Some(0.018));
    assert_eq!(sia263::bolt_minimums(0.018), None);
    assert_eq!(sia263::shear_factor(Grade::parse("8.8")), (0.6, true));
    assert_eq!(sia263::shear_factor(Grade::parse("10.9")), (0.5, true));
    assert_eq!(sia263::shear_factor(None), (0.5, false));
}

fn group() -> BoltGroup {
    BoltGroup {
        count: 4,
        diameter: 0.016,
        hole: Some(0.018),
        lines: 2,
        per_line: 2,
        pitch: Some(0.050),
        span: 0.060,
        end: [Some(0.040), Some(0.200)],
        edge: Some(0.030),
        width: 0.120,
        width_words: "the bolts' 60 mm + 2 × 30 mm".into(),
    }
}

const STEEL: PlateSteel = PlateSteel {
    thickness: 0.010,
    fy: Some(355e6),
    fu: Some(510e6),
};

fn find<'a>(c: &'a joints::Checked, what: &str) -> &'a joints::PlateCheck {
    c.checks
        .iter()
        .find(|x| x.what.contains(what))
        .unwrap_or_else(|| panic!("no {what} in {c:?}"))
}

/// A gusset plate by hand: bearing (76), net shear (82), block shear (83),
/// towards the side edge, (38), (39); a short pitch; Table 17's distances.
#[test]
fn plate_by_hand() {
    let c = bolted(&group(), &STEEL, 200e3);
    let one = 0.85 * 40.0 / 18.0 * 510e6 * 0.016 * 0.010 / 1.25;
    assert!((find(&c, "(76)").resistance - 4.0 * one).abs() < 1.0);
    let r82 = 0.9 * 510e6 / (1.25 * 3f64.sqrt()) * 1260e-6;
    assert!((find(&c, "(82)").resistance - r82).abs() < 1.0);
    let r83 = (0.9 * 510e6 * 420e-6 + 355e6 / 3f64.sqrt() * 1800e-6) / 1.25;
    assert!((find(&c, "(83)").resistance - r83).abs() < 1.0);
    let side = (0.9 * 510e6 * 630e-6 + 355e6 / 3f64.sqrt() * 900e-6) / 1.25;
    assert!((find(&c, "towards its side edge").resistance - side).abs() < 1.0);
    assert!((find(&c, "(38)").resistance - 355e6 * 1200e-6 / 1.05).abs() < 1.0);
    assert!((find(&c, "(39)").resistance - 0.9 * 510e6 * 840e-6 / 1.25).abs() < 1.0);
    let short = BoltGroup {
        pitch: Some(0.030),
        ..group()
    };
    let c = bolted(&short, &STEEL, 200e3);
    let one = 0.85 * 21.0 / 18.0 * 510e6 * 0.016 * 0.010 / 1.25;
    assert!((find(&c, "(76)").resistance - 4.0 * one).abs() < 1.0);
    let c = bolted(&group(), &STEEL, 200e3);
    assert!((find(&c, "e_1").demand - 0.025).abs() < 1e-12);
    assert!((find(&c, "e_2").demand - 0.020).abs() < 1e-12);
    // In compression: no net section, buckling said.
    let c = bolted(&group(), &STEEL, -200e3);
    assert!(!c.checks.iter().any(|x| x.what.contains("(39)")));
    assert!(
        c.notes
            .iter()
            .any(|n| n.contains("plate buckling not checked"))
    );
}

/// A fillet weld by 6.3.2.2: a = 6 mm, 220 mm, plates at 90°, S355.
#[test]
fn a_fillet_weld() {
    let w = WeldInput {
        kind: WeldKind::Fillet,
        size: Some(0.006),
        length: 0.220,
        length_given: true,
        angle: 90.0,
        fy: Some(355e6),
        thickness: [0.010, 0.010],
    };
    let c = weld(&w, 100e3);
    let r87 = 0.006 * 0.220 * 0.6 * 510e6 / 1.25;
    assert!((find(&c, "(87)").resistance - r87).abs() < 1.0);
    let s = 0.006 * 2f64.sqrt();
    let r88 = s * 0.220 * 0.8 * 355e6 / 1.25;
    assert!((find(&c, "(88)").resistance - r88).abs() < 1e-6);
    let (u, g) = c.governing(1.0).unwrap();
    assert!(g.what.contains("(87)") && (u - 100e3 / r87).abs() < 1e-12);
    assert!((contact_width(0.006, 120.0) - 0.006 / 30f64.to_radians().cos()).abs() < 1e-12);
    let none = WeldInput {
        size: None,
        ..w.clone()
    };
    assert!(weld(&none, 1e3).checks.is_empty());
    let full = WeldInput {
        kind: WeldKind::FullPenetration,
        ..w.clone()
    };
    assert!(weld(&full, 1e3).checks.is_empty());
    let thin = WeldInput {
        thickness: [0.006, 0.010],
        ..w
    };
    assert!(weld(&thin, 1e3).notes.iter().any(|n| n.contains("6.3.6.3")));
}

/// A fillet weld with the moments of its force (invented values, by
/// hand; SIA 263 6.3.1.7, 6.3.2.1): a = 6 mm, L = 200 mm, plates at 90°,
/// S355; F = 60 kN, its moment about the weld's middle M = 2 kN m,
/// about the seam T = 0.1 kN m. At the most loaded end, per unit length:
/// F/L = 300 kN/m, 6M/L² = 6 × 2 / 0.04 = 300 kN/m, on the throat
/// 6T/(L a) = 0.6 / (0.2 × 0.006) = 500 kN/m — 1 100 kN/m, so (87) takes
/// 1 100 × 0.2 = 220 kN against a L 0.6 f_uE / γ_M2 = 0.006 × 0.2 × 306
/// / 1.25 = 293.76 kN: 74.9 %. On the contact s = 6√2 = 8.485 mm:
/// 6T/(L s) = 353.6 kN/m, 953.6 kN/m × 0.2 = 190.7 kN against s L 0.8
/// f_y / γ_M2 = 385.6 kN. Without moments: the force alone (the check of
/// before). T said not on the safe side (6.3.6.5).
#[test]
fn a_fillet_weld_with_its_moments() {
    let w = WeldInput {
        kind: WeldKind::Fillet,
        size: Some(0.006),
        length: 0.200,
        length_given: true,
        angle: 90.0,
        fy: Some(355e6),
        thickness: [0.010, 0.010],
    };
    let x = joints::Extra {
        in_plane: 2e3,
        out_of_plane: 100.0,
        ..Default::default()
    };
    let c = joints::weld_with(&w, 60e3, &x);
    let t = find(&c, "(87)");
    assert!((t.demand - 220e3).abs() < 1e-6, "{}", t.demand);
    assert!((t.resistance - 293.76e3).abs() < 1e-6);
    let s = 0.006 * 2f64.sqrt();
    let c88 = find(&c, "(88)");
    let q = 300e3 + 300e3 + 6.0 * 100.0 / (0.2 * s);
    assert!((c88.demand - q * 0.2).abs() < 1e-6, "{}", c88.demand);
    assert!((c88.resistance - s * 0.2 * 0.8 * 355e6 / 1.25).abs() < 1e-6);
    let (u, g) = c.governing(1.0).unwrap();
    assert!(g.what.contains("(87)") && (u - 220.0 / 293.76).abs() < 1e-12);
    assert!(c.not_safe.iter().any(|n| n.contains("6.3.6.5")));
    // Without moments: the force alone, nothing said not safe.
    let c = joints::weld_with(&w, 60e3, &joints::Extra::default());
    assert!((find(&c, "(87)").demand - 60e3).abs() < 1e-9);
    assert!(c.not_safe.is_empty());
    assert_eq!(c, weld(&w, 60e3));
    // The in-plane moment alone: (300 + 300) × 0.2 = 120 kN.
    let m = joints::Extra {
        in_plane: 2e3,
        ..Default::default()
    };
    let c = joints::weld_with(&w, 60e3, &m);
    assert!((find(&c, "(87)").demand - 120e3).abs() < 1e-6);
    assert!(c.not_safe.is_empty());
}

/// The parameter file's γ_M1 changes the members' design values, and the
/// standard's values give the core's numbers.
#[test]
fn parameters_change_the_design_values() {
    let (m, _) = angle(0.060, 0.006, 355e6, 2.0);
    crate::params::set(crate::params::Params::default());
    let (c, _) = sia263::capacity(&m, None);
    let gross = c.tension.unwrap();
    let p = crate::params::parse("[resistance]\ngamma_m1 = 1.1\ngamma_m2 = 1.5\n").unwrap();
    crate::params::set(p);
    let (c2, _) = sia263::capacity(&m, None);
    crate::params::set(crate::params::Params::default());
    assert!(c2.tension.unwrap().0 < gross.0);
}

/// A shift where only bars meet (2026-11-18): OpenOHL puts its moment N·e
/// in a truss's end forces as a uniform moment; SIA 263's plugin then
/// checks the truss with it as a frame (its stability check, formulas
/// 50, 62, 63, with the moment). An L 60×6 S355, 2 m, 20 kN compression,
/// 0.3 kN m about each axis: its usage larger than without, which is the
/// axial check alone (the connection's eccentricity of 5.4.1.2 as
/// before).
#[test]
fn a_truss_with_the_moment_of_its_shift() {
    use crate::check::{member_usage, shifted};
    use crate::model::{Analysis, MemberForces};
    let (m, _) = angle(0.060, 0.006, 355e6, 2.0);
    crate::params::set(crate::params::Params::default());
    let plain = MemberForces {
        axial: -20e3,
        end_forces: [0.0; 12],
        slack: false,
        bowed: false,
        length: 2.0,
        span_load: [0.0; 3],
        lumped_load: [0.0; 3],
    };
    let mut bent = plain.clone();
    for (i, j) in [(4, 10), (5, 11)] {
        bent.end_forces[i] = -300.0;
        bent.end_forces[j] = 300.0;
    }
    assert!(shifted(&m, &bent) && !shifted(&m, &plain));
    let a = Analysis {
        single_angle_moments: false,
        no_compression: false,
        redundant_force: None,
    };
    let u0 = member_usage(&a, &m, &plain, None, false, 1.0, None);
    let u1 = member_usage(&a, &m, &bent, None, false, 1.0, None);
    assert!(
        u1.usage.unwrap() > u0.usage.unwrap() + 0.01,
        "{u0:?} {u1:?}"
    );
}
