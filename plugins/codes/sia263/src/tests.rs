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

/// A member whose data gives no bolts or holes (O19 stage 5, the owner's
/// answer of 2026-11-23, built 2026-11-24): a bolt hole in each leg of an
/// angle in the net section (39) until the bolts are known — the default
/// 20 mm bolt's hole of Table 17, 22 mm: L 60×6 (no root radius), A = 6
/// (120 − 6) = 684 mm², net 684 − 2 × 22 × 6 = 420 mm², 0.9 × 510 MPa ×
/// 420 mm² / 1.25 = 154.224 kN (the gross (38) 355 × 684 / 1.05 = 231.257
/// kN).
#[test]
fn holes_in_each_leg_without_bolts() {
    let (mut m, q) = angle(0.060, 0.006, 355e6, 2.0);
    assert!((q.area - 684e-6).abs() < 1e-12);
    m.design.bolts = None;
    let (c, _) = sia263::capacity(&m, None);
    let (t, what) = c.tension.clone().unwrap();
    assert!((t - 0.9 * 510e6 * 420e-6 / GAMMA_M2).abs() < 1e-6, "{c:?}");
    assert_eq!(what, "net section (39)");
    assert!(
        c.tension_assumed
            .iter()
            .any(|n| n.contains("22.0 mm hole in each leg (2 in the section; a 20 mm bolt")),
        "{c:?}"
    );
}

/// OpenOHL's default bolt sent with a member whose data gives no bolts
/// (2026-11-26, O19 stage 5): its hole in each leg in the net section (39)
/// in place of the 20 mm bolt's. L 60×6 (no root radius), A = 684 mm²; a
/// 16 mm bolt sent without a hole: Table 17's (`standard_hole`), net 684 −
/// 2 × d0 × 6, 0.9 f_u A_net / γ_M2; with a 17.5 mm hole sent, that hole.
/// No diameter sent: the 20 mm bolt, OpenOHL's why said.
#[test]
fn default_bolt_sent_for_holes_in_each_leg() {
    let (mut m, _) = angle(0.060, 0.006, 355e6, 2.0);
    m.design.bolts = None;
    let net = |hole: f64| 0.9 * 510e6 * (684e-6 - 2.0 * hole * 0.006) / GAMMA_M2;
    m.design.default_bolt = Some(crate::model::DefaultBolt {
        diameter: Some(0.016),
        hole: None,
        why: "the structure's most used bolt, 16 mm".into(),
    });
    let d0 = sia263::standard_hole(0.016).unwrap();
    let (c, _) = sia263::capacity(&m, None);
    let (t, what) = c.tension.clone().unwrap();
    assert!((t - net(d0)).abs() < 1e-6, "{c:?}");
    assert_eq!(what, "net section (39)");
    assert!(
        c.tension_assumed.iter().any(|n| n.contains(
            "OpenOHL's default bolt, the structure's most used bolt, 16 mm, its hole Table 17's"
        )),
        "{c:?}"
    );
    m.design.default_bolt.as_mut().unwrap().hole = Some(0.0175);
    let (c, _) = sia263::capacity(&m, None);
    assert!((c.tension.unwrap().0 - net(0.0175)).abs() < 1e-6);
    m.design.default_bolt = Some(crate::model::DefaultBolt {
        diameter: None,
        hole: None,
        why: "no bolt library".into(),
    });
    let (c, _) = sia263::capacity(&m, None);
    assert!(
        (c.tension.clone().unwrap().0 - net(0.022)).abs() < 1e-6,
        "{c:?}"
    );
    assert!(
        c.tension_assumed
            .iter()
            .any(|n| n.contains("(Table 17); no default bolt from OpenOHL: no bolt library")),
        "{c:?}"
    );
}

/// A built-up member's bearing at member level (O19 stage 5, the owner's
/// answer of 2026-11-23, built 2026-11-24): L 60×6 counted as two plies,
/// the M16's bearing (76) on one ply's 6 mm until the plate between is
/// defined (said not on the safe side), on both with the ends bolted onto
/// a plate of known thickness.
#[test]
fn built_up_bearing_on_one_ply() {
    let (mut m, _) = angle(0.060, 0.006, 355e6, 2.0);
    m.design.plies = Some(2);
    m.design.bolts.as_mut().unwrap().shear_capacity = Some(1e6);
    let d0 = sia263::standard_hole(0.016).unwrap();
    let fu = 510e6;
    let one = (0.85 * 0.025 / d0 * fu * 0.016 * 0.006).min(2.4 * fu * 0.016 * 0.006) / GAMMA_M2;
    let (c, _) = sia263::capacity(&m, None);
    let (t, what) = c.tension.clone().unwrap();
    assert!((t - one).abs() < 1e-6, "{c:?}");
    assert_eq!(what, "bearing");
    assert!(
        c.tension_assumed
            .iter()
            .any(|n| n.starts_with("bearing on the plate between the 2 plies not checked")),
        "{c:?}"
    );
    let plate = crate::model::EndDesign {
        onto: Some(crate::model::Onto::Plate),
        onto_thickness: Some(0.010),
        ..Default::default()
    };
    m.design.ends = [Some(plate.clone()), Some(plate)];
    let (c, _) = sia263::capacity(&m, None);
    let (t, _) = c.tension.unwrap();
    assert!((t - 2.0 * one).abs() < 1e-6 || t < 2.0 * one, "{t}");
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

/// A weld with its sides (2026-11-25, roadmap O44 J7b; invented values, by
/// hand): a = 5 mm, L = 200 mm given, plates of 10 and 12 mm at 90°,
/// S355; F = 60 kN, M = 1 kN m about the weld's middle, T = 0.3 kN m about
/// the seam. (87) per unit length a 0.6 f_uE / γ_M2 = 0.005 × 244.8 =
/// 1 224 kN/m. **One fillet** (sides 1, L = 200 mm): F/L 300 + 6M/L² 150
/// + 6T/(L a) 1 800 = 2 250 kN/m, 183.8 % — its root opened, 6.3.6.5 said
/// (avoid), nothing not safe. **Two fillets** (sides 2, each 100 mm,
/// e = the thinner plate's 10 mm): F/2l 300 + 6M/(2 l²) 300 + T/(e l) 300
/// = 900 kN/m, (87) 180 kN of 244.8 kN, 73.53 %; (88) s = 7.071 mm,
/// 0.8 f_y / γ_M2 = 227.2 N/mm²: 900 / 1 606.5 = 56.02 %. **Not given**:
/// both, the worse (one fillet's) governing, said. Two fillets with the
/// seam's length (not given, 200 mm each): 150 + 75 + 150 = 375 kN/m.
/// Without moments two fillets of L/2 carry what one of L does.
#[test]
fn a_welds_sides() {
    use crate::joints::{Extra, weld_sides};
    let w = WeldInput {
        kind: WeldKind::Fillet,
        size: Some(0.005),
        length: 0.200,
        length_given: true,
        angle: 90.0,
        fy: Some(355e6),
        thickness: [0.010, 0.012],
    };
    let x = Extra {
        in_plane: 1e3,
        out_of_plane: 300.0,
        ..Default::default()
    };
    let per = 0.005 * 0.6 * 510e6 / 1.25;
    // One fillet: as weld_with, the moment about the seam said avoided.
    let one = weld_sides(&w, Some(1), 60e3, &x);
    let (u, g) = one.governing(1.0).unwrap();
    assert!(
        g.what.contains("(87)") && g.what.contains("one side"),
        "{g:?}"
    );
    assert!((u - 2250e3 / per).abs() < 1e-12, "{u}");
    assert!((u - 1.838_235_294_117_647).abs() < 1e-12);
    assert!(one.not_safe.is_empty(), "{one:?}");
    assert!(
        one.notes.iter().any(|n| n.contains("6.3.6.5")
            && n.contains("root")
            && n.contains("engineering model")),
        "{:?}",
        one.notes
    );
    let old = joints::weld_with(&w, 60e3, &x);
    assert!(
        (old.governing(1.0).unwrap().0 - u).abs() < 1e-12,
        "the same model"
    );
    // Two fillets: the couple across the thinner plate.
    let two = weld_sides(&w, Some(2), 60e3, &x);
    let t87 = find(&two, "(87)");
    assert!((t87.demand - 180e3).abs() < 1e-6, "{}", t87.demand);
    assert!((t87.resistance - 244.8e3).abs() < 1e-6);
    let t88 = find(&two, "(88)");
    let s = 0.005 * 2f64.sqrt();
    assert!((t88.resistance - s * 0.2 * 0.8 * 355e6 / 1.25).abs() < 1e-6);
    let (u2, g2) = two.governing(1.0).unwrap();
    assert!(
        g2.what.contains("2 × a 5 mm × 100 mm") && g2.what.contains("(87)"),
        "{g2:?}"
    );
    assert!((u2 - 900e3 / per).abs() < 1e-12 && (u2 - 0.735_294_117_647_058_8).abs() < 1e-12);
    assert!(two.not_safe.is_empty());
    assert!(
        two.notes
            .iter()
            .any(|n| n.contains("couple") && n.contains("10 mm")),
        "{:?}",
        two.notes
    );
    assert!(!two.notes.iter().any(|n| n.contains("6.3.6.5")));
    // Not given: the worse of the two, said.
    let both = weld_sides(&w, None, 60e3, &x);
    let (ub, gb) = both.governing(1.0).unwrap();
    assert!(
        (ub - u).abs() < 1e-12 && gb.what.starts_with("one fillet: "),
        "{gb:?}"
    );
    assert_eq!(both.checks.len(), 4);
    assert!(both.notes.iter().any(|n| n.contains("sides not given")));
    // The seam's length (not given): each fillet that long.
    let seam = WeldInput {
        length_given: false,
        ..w.clone()
    };
    let c = weld_sides(&seam, Some(2), 60e3, &x);
    assert!((find(&c, "(87)").demand - 375e3 * 0.4).abs() < 1e-6);
    assert!(
        c.notes
            .iter()
            .any(|n| n.contains("each fillet the seam's length"))
    );
    // Without moments two fillets of L/2 carry what one fillet of L does.
    let plain = Extra::default();
    let a = weld_sides(&w, Some(1), 60e3, &plain)
        .governing(1.0)
        .unwrap()
        .0;
    let b = weld_sides(&w, Some(2), 60e3, &plain)
        .governing(1.0)
        .unwrap()
        .0;
    assert!((a - b).abs() < 1e-12 && (a - 60e3 / (per * 0.2)).abs() < 1e-12);
    // 6.3.6.2 per fillet: two of 30 mm each below 40 mm.
    let short = WeldInput {
        length: 0.060,
        ..w.clone()
    };
    assert!(
        weld_sides(&short, Some(2), 1e3, &plain)
            .notes
            .iter()
            .any(|n| n.contains("30 mm below 40 mm"))
    );
    // Neither one nor two: not checked; full penetration as before.
    assert!(weld_sides(&w, Some(3), 1e3, &plain).checks.is_empty());
    let full = WeldInput {
        kind: WeldKind::FullPenetration,
        ..w
    };
    assert!(weld_sides(&full, Some(2), 1e3, &plain).checks.is_empty());
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
    let u0 = member_usage(&a, &m, &plain, None, false, 1.0, None, None);
    let u1 = member_usage(&a, &m, &bent, None, false, 1.0, None, None);
    assert!(
        u1.usage.unwrap() > u0.usage.unwrap() + 0.01,
        "{u0:?} {u1:?}"
    );
}

/// A gusset plate in compression as a strut (2026-11-18; an assumption:
/// no rule in SIA 263), by hand: the plate of `plate_by_hand`, 200 kN
/// compression, free length 150 mm. Whitmore's width 60 + 2 × 50 × tan
/// 30° = 117.7 mm, i = 10/√12 = 2.887 mm, λ_K = 2 × 150 / 2.887 = 103.9,
/// λ_E = π √(210 000/355) = 76.4, λ̄_K = 1.360; curve c (α 0.49): Φ =
/// ½ (1 + 0.49 × 1.160 + 1.850) = 1.709, χ = 1 / (1.709 + √(1.709² −
/// 1.850)) = 0.364; N_K,Rd = 0.364 × 355 × 1 177 / 1.05 = 145.0 kN.
#[test]
fn a_plate_in_compression_as_a_strut() {
    crate::params::set(crate::params::Params::default());
    let c = joints::bolted_with(
        &group(),
        &STEEL,
        -200e3,
        &joints::Extra::default(),
        Some(0.150),
    );
    let b = 0.060 + 2.0 * 0.050 * 30f64.to_radians().tan();
    let lb =
        2.0 * 0.150 / (0.010 / 12f64.sqrt()) / (std::f64::consts::PI * (210e9 / 355e6f64).sqrt());
    let phi = 0.5 * (1.0 + 0.49 * (lb - 0.2) + lb * lb);
    let chi = 1.0 / (phi + (phi * phi - lb * lb).sqrt());
    let k = find(&c, "plate buckling as a strut");
    assert!(
        (k.resistance - chi * 355e6 * b * 0.010 / 1.05).abs() < 1e-6,
        "{k:?}"
    );
    assert!((k.resistance - 145.0e3).abs() < 0.3e3, "{k:?}");
    assert!(
        !c.notes
            .iter()
            .any(|n| n.contains("plate buckling not checked"))
    );
    let c = joints::bolted_with(&group(), &STEEL, -200e3, &joints::Extra::default(), None);
    assert!(
        c.not_safe
            .iter()
            .any(|n| n.contains("free length not known"))
    );
}

/// A frame member's end shear and moment in the plate (2026-11-18), by
/// hand: the plate of `plate_by_hand`, 100 kN tension, 10 kN shear in its
/// plane, 1 kN m about its normal: k = 4 r_max / Σ r² = 25.61 /m (the
/// bolts ±25 mm, ±30 mm), bearing under 100 + 10 + 25.61 = 135.61 kN at
/// the nearer of the end (40 mm) and edge (30 mm) distances: (76) with
/// e_1 = 30 mm, 4 × 0.85 × 30/18 × 510 × 16 × 10 / 1.25 = 369.9 kN; the
/// other checks under 160 kN (net section (39) 160 of 308.4 kN).
#[test]
fn a_frame_members_shear_and_moment_in_the_plate() {
    crate::params::set(crate::params::Params::default());
    let x = joints::Extra {
        shear: 10e3,
        in_plane: 1e3,
        ..Default::default()
    };
    let c = joints::bolted_with(&group(), &STEEL, 100e3, &x, Some(0.150));
    let k = joints::moment_factor(&group());
    let b = find(&c, "(76)");
    assert!((b.demand - (110e3 + 1e3 * k)).abs() < 1e-6, "{b:?}");
    let one = 0.85 * 30.0 / 18.0 * 510e6 * 0.016 * 0.010 / 1.25;
    assert!((b.resistance - 4.0 * one).abs() < 1.0, "{b:?}");
    assert!((b.resistance - 369.9e3).abs() < 0.1e3, "{b:?}");
    let net = find(&c, "(39)");
    assert!((net.demand - 160e3).abs() < 1e-6, "{net:?}");
    assert!(c.checks.iter().any(|p| p.what.contains("as a strut")));
    let plain = joints::bolted_with(&group(), &STEEL, 100e3, &joints::Extra::default(), None);
    assert_eq!(plain, bolted(&group(), &STEEL, 100e3));
}
