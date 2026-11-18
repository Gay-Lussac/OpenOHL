// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The ASCE 10-97 check by hand: the tests of the core's code
//! (`ohl_structures_fe::asce10`, its own tests), moved with it
//! (2026-11-18) — the same data, the same values: a crossing diagonal and
//! the assumptions named, the standard's Examples 15 and 16, torsional
//! buckling by profile, the connection rules by hand, the six curves,
//! local buckling and the column curve. The plate checks' test moved with
//! them is in `joints`.

use crate::asce10::{
    capacity, column_stress, effective_slenderness, local_buckling_stress,
    torsional_flexural_radius, torsional_radius,
};
use crate::model::{
    Bolts, Condition, ConnectedLeg, EndConditions, Ends, Member, MemberClass, MemberDesign,
    MemberKind, Section, Source,
};

const E: f64 = 210e9;

/// One condition at both ends as OpenOHL works it out for a member
/// without connections: the member's flag when given (its "one end" the
/// first), else assumed (`assumed`).
fn resolve(given: Option<Ends>, assumed: bool) -> [Condition; 2] {
    let c = |value: bool, source: Source| Condition {
        value: Some(value),
        source,
        why: None,
    };
    match given {
        Some(Ends::None) => [c(false, Source::Member), c(false, Source::Member)],
        Some(Ends::BothEnds) => [c(true, Source::Member), c(true, Source::Member)],
        Some(Ends::OneEnd) => [
            Condition {
                value: None,
                source: Source::Member,
                why: None,
            },
            Condition {
                value: None,
                source: Source::Member,
                why: None,
            },
        ],
        None => [c(assumed, Source::Assumed), c(assumed, Source::Assumed)],
    }
}

/// A truss member `id` of `section`, `length` long between joints "a" and
/// "b", with `design` and the member's own end flags.
fn member(
    id: &str,
    section: Section,
    length: f64,
    design: MemberDesign,
    eccentricity: Option<Ends>,
) -> Member {
    Member {
        id: id.into(),
        kind: MemberKind::Truss,
        frame: false,
        design,
        section,
        length,
        k: 1.0,
        lk: None,
        capacity_tension: None,
        capacity_compression: None,
        joints: ["a".into(), "b".into()],
        conditions: EndConditions {
            eccentric: resolve(eccentricity, true),
            restrained: resolve(None, false),
            connected: [false; 2],
        },
        onto: [None, None],
    }
}

/// A section of the given values without a profile (the core's
/// `Section::elastic`): torsion constant `iy + iz`, minor second moment
/// the smaller.
fn elastic(area: f64, i: f64) -> Section {
    Section {
        area,
        iy: i,
        iz: i,
        j: 2.0 * i,
        i_min: i,
        wy: None,
        wz: None,
        e: E,
        g: E / 2.6,
        yield_strength: None,
        ultimate: None,
        tension: None,
        compression: None,
        moment_y: None,
        moment_z: None,
        profile: None,
        properties: None,
    }
}

/// A section of a library profile, its properties worked out.
fn profiled(shape: ohl_sections::Shape) -> (Section, ohl_sections::Properties) {
    let p = ohl_sections::Section {
        id: "p".into(),
        name: String::new(),
        description: String::new(),
        shape,
        material: None,
        given: ohl_sections::Given::default(),
    };
    let q = p.properties();
    let mut s = elastic(q.area, q.iy);
    s.iz = q.iz;
    s.i_min = q.iv;
    s.yield_strength = Some(355e6);
    s.properties = Some(q.clone());
    s.profile = Some(p);
    (s, q)
}

/// A crossing diagonal braced at mid-length (length ratios 0.5): with
/// its partner in tension its L/r is half its length over r_v; with
/// the partner compressed (or no partner) at least its whole length
/// over its smaller geometric radius, said in its assumptions. Each
/// missing datum is named; given data are not.
#[test]
fn crossing_diagonal_and_assumptions() {
    let mut s = elastic(10e-4, 50e-8);
    s.i_min = 20e-8;
    s.j = 100e-8;
    s.yield_strength = Some(355e6);
    let l = 18f64.sqrt();
    let d1 = member(
        "d1",
        s.clone(),
        l,
        MemberDesign {
            class: Some(MemberClass::Crossing),
            length_ratios: Some([0.5; 3]),
            ..MemberDesign::default()
        },
        Some(Ends::BothEnds),
    );
    let (ry, rv) = ((50e-8f64 / 10e-4).sqrt(), (20e-8f64 / 10e-4).sqrt());
    let braced = capacity(&d1, Some(true));
    let lr = 0.5 * l / rv;
    assert!(
        (braced.kl_r.unwrap() - effective_slenderness(lr, Ends::BothEnds, Ends::None)).abs() < 1e-9
    );
    assert!(
        !braced
            .compression_assumed
            .iter()
            .any(|a| a.contains("crossing"))
    );
    let free = capacity(&d1, Some(false));
    let lr = l / ry;
    assert!(
        (free.kl_r.unwrap() - effective_slenderness(lr, Ends::BothEnds, Ends::None)).abs() < 1e-9
    );
    assert!(
        free.compression_assumed
            .iter()
            .any(|a| a.contains("not in tension")),
        "{free:?}"
    );
    assert!(free.compression.unwrap().0 < braced.compression.unwrap().0);
    // The partner without data: every assumption named.
    let mut d2 = member("d2", s, l, MemberDesign::default(), None);
    let bare = capacity(&d2, None);
    for a in ["length ratios 1", "both ends eccentric", "no bolts given"] {
        assert!(
            bare.compression_assumed.iter().any(|x| x.contains(a)),
            "{a}: {bare:?}"
        );
    }
    assert_eq!(bare.tension.as_ref().unwrap().1, "gross section, no holes");
    // Bolts counted 0: the holes only — the net section, no bolt check.
    d2.design.bolts = Some(Bolts {
        diameter: 0.016,
        hole: Some(0.0175),
        count: 0,
        shear_planes: 1,
        shear_capacity: Some(60e3),
        ultimate: None,
        holes: Some(1),
        grade: None,
    });
    d2.design.thickness = Some(0.006);
    d2.design.connected_leg = Some(ConnectedLeg::Both);
    let holes = capacity(&d2, None);
    // The given hole taken as punched: + 1.6 mm (3.10.1).
    let net = (10e-4 - (0.0175 + 0.0016) * 0.006) * 355e6;
    assert!(
        (holes.tension.as_ref().unwrap().0 - net).abs() < 1e-6,
        "{holes:?}"
    );
    assert!(
        holes
            .tension_assumed
            .iter()
            .any(|a| a.contains("no end bolts"))
    );
    // No yield strength: no capacity (the check falls back).
    let mut d1 = d1;
    d1.section.yield_strength = None;
    assert!(capacity(&d1, None).compression.is_none());
}

/// ASCE 10-97 Appendix B, Examples 15 and 16 (60° angles, inches and
/// ksi as printed): r_t 1.33 in, r_tf 1.21 in, KL/r_tf 88.8 and F_a
/// 26.7 ksi; with the mixed ends of Example 16 (r_t / K_t, K_t 0.5,
/// taking r_t 0.663 in as printed) K'/r_tf 0.878.
#[test]
fn standard_examples_15_and_16() {
    let rt = torsional_radius(3.68, 0.646, 172.0, 9.0 * 12.0);
    assert!((rt - 1.33).abs() < 0.005, "{rt}");
    let rps = (172.0f64 / 7.75).sqrt();
    let rtf = torsional_flexural_radius(rt, 2.40, 3.58, rps);
    assert!((rtf - 1.21).abs() < 0.01, "{rtf}");
    let kl_r = 9.0 * 12.0 / rtf;
    assert!((kl_r - 88.8).abs() < 0.2, "{kl_r}");
    let fa = column_stress(35.2, 29_000.0, kl_r);
    assert!((fa - 26.7).abs() < 0.05, "{fa}");
    let k = 1.0 / torsional_flexural_radius(0.663 / 0.5, 1.84, 2.71, 3.57);
    assert!((k - 0.878).abs() < 0.001, "{k}");
}

/// Torsional buckling by the profile: a short-legged I section braced
/// about both axes at mid-length but free to twist over its length
/// governs by L/r_t (3.9.4, said); a channel by L/r_tf (3.9.5); an
/// angle and a tube are not touched; a `general` profile is said not
/// checked.
#[test]
fn torsional_buckling_by_profile() {
    use ohl_sections::Shape;
    let with = |shape: Shape| {
        let (mut s, q) = profiled(shape);
        s.j = q.it.unwrap_or(q.iy + q.iz);
        let b = member(
            "b",
            s,
            4.0,
            MemberDesign {
                length_ratios: Some([0.25; 3]),
                ..MemberDesign::default()
            },
            Some(Ends::None),
        );
        (capacity(&b, None), q)
    };
    let i = Shape::ISection {
        h: 0.3,
        b: 0.3,
        tw: 0.006,
        tf: 0.006,
        r: 0.0,
    };
    let (c, q) = with(i.clone());
    let ips = q.iy + q.iz;
    let rt = torsional_radius(q.warping.unwrap(), q.it.unwrap(), ips, 4.0);
    assert!((c.kl_r.unwrap() - 4.0 / rt).abs() < 1e-9, "{c:?}");
    assert!(
        c.compression
            .as_ref()
            .unwrap()
            .1
            .contains("torsional, 3.9.4")
    );
    assert!(c.compression_assumed.iter().any(|a| a.contains("K_t 1")));
    let (c, _) = with(Shape::Channel {
        h: 0.3,
        b: 0.1,
        tw: 0.006,
        tf: 0.006,
        r: 0.0,
    });
    assert!(
        c.compression
            .as_ref()
            .unwrap()
            .1
            .contains("torsional-flexural, 3.9.5"),
        "{c:?}"
    );
    for shape in [
        Shape::EqualAngle {
            h: 0.1,
            t: 0.006,
            r: 0.0,
        },
        Shape::CircularHollow { d: 0.1, t: 0.003 },
    ] {
        let (c, q) = with(shape);
        assert!(!c.compression.as_ref().unwrap().1.contains("torsional"));
        assert!(
            !c.compression_assumed
                .iter()
                .any(|a| a.contains("torsional"))
        );
        let rv = (q.iv / q.area).sqrt();
        assert!((c.kl_r.unwrap() - 0.25 * 4.0 / rv).abs() < 1e-6, "{c:?}");
    }
    let (c, _) = with(Shape::General { h: 0.1, b: 0.1 });
    assert!(
        c.compression_assumed
            .iter()
            .any(|a| a.contains("torsional buckling not checked"))
    );
}

/// The connection rules of ASCE 10-97 by hand, on an unequal angle
/// 100 × 60 × 8 mm (no root radius; f_y 355, f_u 510 MPa) bolted by
/// its short leg with three M16 bolts (pitch 50 mm, end distance 20
/// mm, 25 mm to the toe, no hole given):
/// - the hole: standard (16 + 1.6) taken as punched, + 1.6: 19.2 mm;
/// - the area: the long leg counted as 60 mm (3.10.2): A − 40 × 8 mm²;
/// - net section: (1216 − 320 − 19.2 × 8) mm² × 355 × 0.9 = 237.2 kN;
/// - block shear (Eq. 3.10-1): A_v = 8 (20 + 2 × 50 − 2.5 × 19.2) =
///   576 mm², A_t = 8 (25 − 9.6) = 123.2 mm², P = 0.6 × 576 × 510 +
///   123.2 × 355 = 219.99 kN;
/// - bearing on the bolt's f_u when lower (4.4): 3 × 1.5 × 300 × 16 ×
///   8 = 172.8 kN;
/// - bolt shear from f_ub alone on 0.6 of the gross area: 3 × 0.62 ×
///   300 × 0.6 × 201.06 = 67.31 kN;
/// - a leg of w/t above 25 said (3.7.1).
#[test]
fn connection_rules_by_hand() {
    let (mut s, q) = profiled(ohl_sections::Shape::UnequalAngle {
        h: 0.1,
        b: 0.06,
        t: 0.008,
        r: 0.0,
    });
    s.ultimate = Some(510e6);
    let mut b = member(
        "b",
        s,
        2.0,
        MemberDesign {
            connected_leg: Some(ConnectedLeg::Short),
            end_distance: Some(0.02),
            spacing: Some(0.05),
            edge_distance: Some(0.025),
            bolts: Some(Bolts {
                diameter: 0.016,
                hole: None,
                count: 3,
                shear_planes: 1,
                shear_capacity: None,
                ultimate: Some(300e6),
                holes: Some(1),
                grade: None,
            }),
            ..MemberDesign::default()
        },
        None,
    );
    let c = capacity(&b, None);
    let gross = std::f64::consts::PI * 0.016f64.powi(2) / 4.0;
    let shear = 3.0 * 0.62 * 300e6 * 0.6 * gross;
    let (t, what) = c.tension.clone().unwrap();
    assert!((t - shear).abs() < 1e-6, "{c:?}");
    assert_eq!(what, "bolt shear");
    // The bolts strong: bearing on the bolt's f_u, then block shear.
    b.design.bolts.as_mut().unwrap().shear_capacity = Some(1e6);
    let (t, what) = capacity(&b, None).tension.unwrap();
    assert!((t - 3.0 * 1.5 * 300e6 * 0.016 * 0.008).abs() < 1e-6, "{t}");
    assert_eq!(what, "bearing");
    b.design.bolts.as_mut().unwrap().ultimate = Some(800e6);
    let c = capacity(&b, None);
    let (t, what) = c.tension.clone().unwrap();
    let block = 0.6 * 576e-6 * 510e6 + 123.2e-6 * 355e6;
    assert!((t - block).abs() < 1e-3, "{c:?}");
    assert_eq!(what, "block shear");
    // Without the block shear's data: the net section, short leg.
    b.design.end_distance = None;
    let c = capacity(&b, None);
    let net = (q.area - 0.04 * 0.008 - 0.0192 * 0.008) * 355e6 * 0.9;
    let (t, what) = c.tension.clone().unwrap();
    assert!((t - net).abs() < 1e-3, "{c:?}");
    assert_eq!(what, "net section");
    for n in [
        "standard hole",
        "holes punched",
        "block shear not checked (no end distance)",
    ] {
        assert!(
            c.tension_assumed.iter().any(|a| a.contains(n)),
            "{n}: {c:?}"
        );
    }
    // w/t: (100 − 8)/8 = 11.5 within 25; a 4 mm leg, 23, too; 3 mm, 31.
    assert!(c.limit.is_none());
    b.design.width_thickness = Some(31.0);
    assert!(
        capacity(&b, None)
            .limit
            .unwrap()
            .contains("w/t 31.0 above 25 (3.7.1)")
    );
}

/// The six curves at L/r 100 and 150 (hand values).
#[test]
fn effective_slenderness_curves() {
    assert_eq!(effective_slenderness(100.0, Ends::None, Ends::None), 100.0);
    assert_eq!(
        effective_slenderness(100.0, Ends::OneEnd, Ends::None),
        105.0
    );
    assert_eq!(
        effective_slenderness(100.0, Ends::BothEnds, Ends::None),
        110.0
    );
    assert_eq!(
        effective_slenderness(150.0, Ends::BothEnds, Ends::None),
        150.0
    );
    assert!((effective_slenderness(150.0, Ends::None, Ends::OneEnd) - 142.9).abs() < 1e-9);
    assert!((effective_slenderness(150.0, Ends::None, Ends::BothEnds) - 138.45).abs() < 1e-9);
}

/// Local buckling for f_y 355 MPa: (w/t)_lim = 11.12, the linear
/// range to 20.06, then elastic.
#[test]
fn local_buckling() {
    let fy = 355e6;
    assert_eq!(local_buckling_stress(fy, E, 10.0), fy);
    let lim = 209.6 / 355f64.sqrt();
    let f = local_buckling_stress(fy, E, 14.0);
    assert!((f - (1.677 - 0.677 * 14.0 / lim) * fy).abs() < 1.0);
    let f = local_buckling_stress(fy, E, 25.0);
    assert!((f - 0.0332 * std::f64::consts::PI.powi(2) * E / 625.0).abs() < 1.0);
}

/// The column curve: C_c for f_y 355 MPa is 108.06; at KL/r 80
/// F_a = (1 − ½(80/107.4)²) f_y; at 150 the Euler stress.
#[test]
fn column_curve() {
    let fy = 355e6;
    let cc = std::f64::consts::PI * (2.0 * E / fy).sqrt();
    assert!((cc - 108.06).abs() < 0.01, "{cc}");
    let f = column_stress(fy, E, 80.0);
    assert!((f - (1.0 - 0.5 * (80.0 / cc).powi(2)) * fy).abs() < 1.0);
    let f = column_stress(fy, E, 150.0);
    assert!((f - std::f64::consts::PI.powi(2) * E / 22500.0).abs() < 1.0);
}

/// ASCE 10-97 4.5 by hand (the core's `bolt_rules` test, moved): M16
/// bolts in a 6 mm leg of f_u 510 MPa carrying P = 30 kN each: 1.2 P/(f_u
/// t) = 11.76 mm, 1.3 d = 20.8 mm, t + d/2 = 14 mm → e_min 20.8 mm; s_min =
/// 11.76 + 9.6 = 21.36 mm; sheared edge 0.85 × 20.8 + 1.5875 = 19.27 mm,
/// rolled 17.68 mm. At 60 kN the force term governs the end: 23.53 mm.
/// Redundant: the larger of 14 and 19.2 mm.
#[test]
fn asce10_minimums_by_hand() {
    use crate::asce10::bolt_minimums;
    let m = bolt_minimums(0.016, 0.006, 510e6, Some(30e3), false, true);
    assert!((m[0] - 0.0208).abs() < 1e-9, "{m:?}");
    assert!((m[1] - (1.2 * 30e3 / (510e6 * 0.006) + 0.0096)).abs() < 1e-9);
    assert!((m[2] - (0.85 * 0.0208 + 0.0015875)).abs() < 1e-9);
    let rolled = bolt_minimums(0.016, 0.006, 510e6, Some(30e3), false, false);
    assert!((rolled[2] - 0.85 * 0.0208).abs() < 1e-9);
    let heavy = bolt_minimums(0.016, 0.006, 510e6, Some(60e3), false, true);
    assert!((heavy[0] - 1.2 * 60e3 / (510e6 * 0.006)).abs() < 1e-9);
    let red = bolt_minimums(0.016, 0.006, 510e6, Some(60e3), true, true);
    assert!((red[0] - 0.0192).abs() < 1e-9);
}

/// The bolt's design shear capacity, the force basis of the minimum
/// distances without an analysis (third round, 2026-11-18; the core's
/// `bolt_shear_per_bolt` moved): 4.3.2's 0.62 f_u on 0.6 of the gross area
/// times the planes, bit for bit as the core worked it out; a given
/// capacity over it, the shank's when the threads are out; none without
/// either.
#[test]
fn asce10_bolt_shear_by_hand() {
    use crate::asce10::bolt_shear;
    let gross = std::f64::consts::PI * 0.016f64.powi(2) / 4.0;
    let v = bolt_shear(0.016, 2, None, None, None, Some(800e6)).unwrap();
    assert_eq!(v.to_bits(), ((0.62 * 800e6 * 0.6 * gross) * 2.0).to_bits());
    assert_eq!(
        bolt_shear(0.016, 1, Some(60e3), None, None, Some(800e6)),
        Some(60e3)
    );
    assert_eq!(
        bolt_shear(0.016, 1, Some(60e3), Some(75e3), Some(false), None),
        Some(75e3)
    );
    assert_eq!(
        bolt_shear(0.016, 1, Some(60e3), Some(75e3), Some(true), None),
        Some(60e3)
    );
    assert_eq!(bolt_shear(0.016, 1, None, None, None, None), None);
}

/// A shift where only bars meet (2026-11-18), by hand: OpenOHL puts its
/// moment N·e in a truss's end forces as a uniform moment; ASCE 10-97's
/// plugin then checks the truss with it as a frame — the linear sum N /
/// N_c + M_y / M_y,c + M_z / M_z,c (the frames' rule). Invented: 1 000
/// mm², f_y 355 MPa, M_z,c = 2 kN m; 10 kN tension and 0.2 kN m about z
/// (−200 N m at its first end, +200 at its second): the moment adds
/// 0.2 / 2 = 10 % to its axial usage; without it the axial check alone.
#[test]
fn a_truss_with_the_moment_of_its_shift() {
    use crate::check::{member_usage, shifted};
    use crate::model::{Analysis, MemberForces};
    let mut s = elastic(1000e-6, 2e-6);
    s.yield_strength = Some(355e6);
    s.moment_y = Some(2e3);
    s.moment_z = Some(2e3);
    let m = member("T", s, 3.0, MemberDesign::default(), None);
    let plain = MemberForces {
        axial: 10e3,
        end_forces: [0.0; 12],
        slack: false,
        bowed: false,
        length: 3.0,
        span_load: [0.0; 3],
        lumped_load: [0.0; 3],
    };
    let mut bent = plain.clone();
    bent.end_forces[5] = -200.0;
    bent.end_forces[11] = 200.0;
    assert!(shifted(&m, &bent) && !shifted(&m, &plain));
    let a = Analysis {
        single_angle_moments: false,
        no_compression: false,
        redundant_force: None,
    };
    let u0 = member_usage(&a, &m, &plain, None, false, 1.0, None);
    let u1 = member_usage(&a, &m, &bent, None, false, 1.0, None);
    assert!(
        (u1.usage.unwrap() - u0.usage.unwrap() - 0.1).abs() < 1e-12,
        "{u0:?} {u1:?}"
    );
    assert!(u1.governs.contains("Mz 0.2 kN m of 2.0"), "{u1:?}");
    assert!(!u0.governs.contains("Mz"), "{u0:?}");
}
