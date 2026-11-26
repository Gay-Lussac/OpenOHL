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

/// A member whose data gives no bolts or holes (O19 stage 5, the owner's
/// answer of 2026-11-23, built 2026-11-24): a bolt hole in each leg of an
/// angle until the bolts are known — the default 20 mm bolt's standard
/// hole 21.6 mm, punched 23.2 mm; an unequal angle as bolted by its short
/// leg (its long leg counted as the short one), 0.9 for one leg bolted:
/// L100 × 60 × 8 with no root radius, A = 8 (100 + 60 − 8) = 1216 mm²,
/// counted 1216 − 40 × 8 = 896 mm², net 896 − 2 × 23.2 × 8 = 524.8 mm²,
/// × 355 MPa × 0.9 = 167.6736 kN. Another end's bolt (M16, 17.5 mm hole):
/// its hole, 896 − 2 × 19.1 × 8 = 590.4 mm², 188.6328 kN. A section that
/// is not an angle: the gross section, said.
#[test]
fn holes_in_each_leg_without_bolts() {
    let (s, q) = profiled(ohl_sections::Shape::UnequalAngle {
        h: 0.1,
        b: 0.06,
        t: 0.008,
        r: 0.0,
    });
    assert!((q.area - 1216e-6).abs() < 1e-12);
    let mut b = member("b", s, 2.0, MemberDesign::default(), None);
    let c = capacity(&b, None);
    let (t, what) = c.tension.clone().unwrap();
    assert!((t - 524.8e-6 * 355e6 * 0.9).abs() < 1e-6, "{c:?}");
    assert_eq!(what, "net section");
    assert!(
        c.tension_assumed
            .iter()
            .any(|n| n.contains("21.6 mm hole in each leg (2 in the section; a 20 mm bolt")),
        "{c:?}"
    );
    assert!(
        c.tension_assumed
            .iter()
            .any(|n| n.starts_with("no bolts given: connection not checked")),
        "{c:?}"
    );
    // The other end's connection gives its bolts: their hole at this end.
    b.design.ends[1] = Some(crate::model::EndDesign {
        bolts: Some(crate::model::EndBolts {
            count: 2,
            diameter: Some(0.016),
            hole: Some(0.0175),
            ..Default::default()
        }),
        ..Default::default()
    });
    let c = capacity(&b, None);
    let (t, what) = c.tension.clone().unwrap();
    assert!((t - 590.4e-6 * 355e6 * 0.9).abs() < 1e-6, "{c:?}");
    assert_eq!(what, "net section at a", "{c:?}");
    // Not an angle: the gross section, as before.
    let mut s = elastic(10e-4, 50e-8);
    s.yield_strength = Some(355e6);
    let c = capacity(&member("g", s, 2.0, MemberDesign::default(), None), None);
    let (t, what) = c.tension.unwrap();
    assert!((t - 355e3).abs() < 1e-6);
    assert_eq!(what, "gross section, no holes");
}

/// OpenOHL's default bolt sent with a member whose data gives no bolts
/// (2026-11-26, O19 stage 5, the owner's answer of 2026-11-25): its hole in
/// each leg in place of the 20 mm bolt's. L 40 × 4 with no root radius, A =
/// 4 (80 − 4) = 304 mm², fy 355 MPa; a 12 mm bolt with a 13.5 mm hole sent:
/// punched 15.1 mm, net 304 − 2 × 15.1 × 4 = 183.2 mm², one leg bolted × 0.9
/// × 355 MPa = 58.5324 kN (the 20 mm bolt: 304 − 2 × 23.2 × 4 = 118.4 mm²,
/// 37.8288 kN). A default without a hole: its standard hole (4.1), 12 + 1.6
/// + 1.6 = 15.2 mm. No diameter sent (the leg takes no library bolt): the
/// 20 mm bolt, OpenOHL's why said. The member's other end's bolt still
/// first.
#[test]
fn default_bolt_sent_for_holes_in_each_leg() {
    let (s, q) = profiled(ohl_sections::Shape::EqualAngle {
        h: 0.04,
        t: 0.004,
        r: 0.0,
    });
    assert!((q.area - 304e-6).abs() < 1e-12);
    let mut b = member("b", s, 1.5, MemberDesign::default(), None);
    let before = capacity(&b, None).tension.unwrap().0;
    assert!((before - 118.4e-6 * 355e6 * 0.9).abs() < 1e-6);
    b.design.default_bolt = Some(crate::model::DefaultBolt {
        diameter: Some(0.012),
        hole: Some(0.0135),
        why: "the largest bolt of the structure's library its 40 mm leg's usual gauge takes, M12"
            .into(),
    });
    let c = capacity(&b, None);
    let (t, what) = c.tension.clone().unwrap();
    assert!((t - 183.2e-6 * 355e6 * 0.9).abs() < 1e-6, "{c:?}");
    assert_eq!(what, "net section");
    assert!(
        c.tension_assumed.iter().any(|n| n.contains(
            "13.5 mm hole in each leg (2 in the section; OpenOHL's default bolt, the largest \
             bolt of the structure's library its 40 mm leg's usual gauge takes, M12)"
        )),
        "{c:?}"
    );
    // No hole sent: the standard hole.
    b.design.default_bolt.as_mut().unwrap().hole = None;
    let c = capacity(&b, None);
    let (t, _) = c.tension.clone().unwrap();
    assert!(
        (t - (304e-6 - 2.0 * 0.0152 * 0.004) * 355e6 * 0.9).abs() < 1e-6,
        "{c:?}"
    );
    assert!(
        c.tension_assumed
            .iter()
            .any(|n| n.contains("M12, its standard hole (4.1)")),
        "{c:?}"
    );
    // No diameter: the plugin's own 20 mm bolt, OpenOHL's why said.
    b.design.default_bolt = Some(crate::model::DefaultBolt {
        diameter: None,
        hole: None,
        why: "its 40 mm leg's usual gauge takes no bolt of the structure's library".into(),
    });
    let c = capacity(&b, None);
    assert!((c.tension.clone().unwrap().0 - before).abs() < 1e-6);
    assert!(
        c.tension_assumed.iter().any(|n| n.contains(
            "a 20 mm bolt, the program's default (not the code's), and its standard hole (4.1); \
             no default bolt from OpenOHL: its 40 mm leg's usual gauge takes no bolt"
        )),
        "{c:?}"
    );
    // The other end's own bolt comes first.
    b.design.default_bolt = Some(crate::model::DefaultBolt {
        diameter: Some(0.012),
        hole: Some(0.0135),
        why: String::new(),
    });
    b.design.ends[1] = Some(crate::model::EndDesign {
        bolts: Some(crate::model::EndBolts {
            count: 2,
            diameter: Some(0.016),
            hole: Some(0.0175),
            ..Default::default()
        }),
        ..Default::default()
    });
    let c = capacity(&b, None);
    assert!(
        c.tension_assumed
            .iter()
            .any(|n| n.contains("the member's other end's 16 mm bolt")),
        "{c:?}"
    );
}

/// A built-up member's bearing at member level (O19 stage 5, the owner's
/// answer of 2026-11-23, built 2026-11-24): two plies of 6 mm, 2 M18 in
/// double shear, f_u 370 MPa — one ply's thickness until the plate between
/// is defined, 2 × 1.5 × 370 MPa × 18 mm × 6 mm = 119.88 kN, said not on
/// the safe side; with the end bolted onto a plate of known thickness both
/// plies, 239.76 kN (the plate's own bearing in the joint checks).
#[test]
fn built_up_bearing_on_one_ply() {
    let mut s = elastic(20e-4, 50e-8);
    s.yield_strength = Some(355e6);
    s.ultimate = Some(370e6);
    let mut b = member(
        "b",
        s,
        2.0,
        MemberDesign {
            thickness: Some(0.006),
            plies: Some(2),
            connected_leg: Some(ConnectedLeg::Long),
            bolts: Some(Bolts {
                diameter: 0.018,
                hole: Some(0.020),
                count: 2,
                shear_planes: 2,
                shear_capacity: Some(1e6),
                ultimate: None,
                holes: Some(1),
                grade: None,
            }),
            ..MemberDesign::default()
        },
        None,
    );
    let c = capacity(&b, None);
    let (t, what) = c.tension.clone().unwrap();
    assert!((t - 119.88e3).abs() < 1e-6, "{c:?}");
    assert_eq!(what, "bearing");
    assert!(
        c.tension_assumed
            .iter()
            .any(|n| n.starts_with("bearing on the plate between the 2 plies not checked")),
        "{c:?}"
    );
    // Both ends onto a plate of known thickness: both plies.
    let plate = crate::model::EndDesign {
        onto: Some(crate::model::Onto::Plate),
        onto_thickness: Some(0.010),
        ..Default::default()
    };
    b.design.ends = [Some(plate.clone()), Some(plate)];
    let (t, what) = capacity(&b, None).tension.unwrap();
    assert!((t - 239.76e3).abs() < 1e-6, "{t}");
    assert_eq!(what, "bearing");
}

/// The force per bolt of the minimum distances without an analysis (O44
/// L16's question 4, the owner's answer of 2026-11-24): the smaller of the
/// bolt's design shear capacity and its bearing cap 1.5 f_u d t (4.4, f_u
/// the lower of the part's and the bolt's). M20 in a 5 mm leg, f_u 490
/// MPa, the bolt's capacity 94 kN a plane: bearing 1.5 × 490 × 20 × 5 =
/// 73.5 kN governs; end 1.2 × 73.5 kN / (490 MPa × 5 mm) = 36 mm (= 1.8
/// d; 1.3 d 26 mm), rolled edge 0.85 × 36 = 30.6 mm, sheared 32.1875 mm.
/// A 12 mm leg: bearing 176.4 kN, the shear 94 kN governs.
#[test]
fn force_without_analysis_by_hand() {
    use crate::asce10::{bearing_cap, bolt_minimums, bolt_shear, force_without_analysis};
    let shear = bolt_shear(0.020, 1, Some(94e3), None, None, None);
    let bearing = bearing_cap(0.020, 0.005, Some(490e6), Some(800e6));
    assert!((bearing.unwrap() - 73.5e3).abs() < 1e-6);
    let (p, basis) = force_without_analysis(shear, bearing);
    assert!((p.unwrap() - 73.5e3).abs() < 1e-6);
    assert!(basis.starts_with("the bolt's bearing cap in the part, ASCE 10-97 4.4"));
    let [end, _, edge] = bolt_minimums(0.020, 0.005, 490e6, p, false, false);
    assert!((end - 0.036).abs() < 1e-12, "{end}");
    assert!((edge - 0.0306).abs() < 1e-12, "{edge}");
    let [_, _, sheared] = bolt_minimums(0.020, 0.005, 490e6, p, false, true);
    assert!((sheared - (0.0306 + 0.0015875)).abs() < 1e-12);
    // The bolt's f_u lower than the part's: the bolt's.
    let low = bearing_cap(0.020, 0.005, Some(490e6), Some(400e6)).unwrap();
    assert!((low - 60e3).abs() < 1e-6);
    // A thick leg: the shear capacity governs.
    let (p, basis) = force_without_analysis(shear, bearing_cap(0.020, 0.012, Some(490e6), None));
    assert!((p.unwrap() - 94e3).abs() < 1e-6);
    assert!(basis.starts_with("the bolt's design shear capacity"));
    assert_eq!(force_without_analysis(None, None), (None, ""));
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
    let u0 = member_usage(&a, &m, &plain, None, false, 1.0, None, None);
    let u1 = member_usage(&a, &m, &bent, None, false, 1.0, None, None);
    assert!(
        (u1.usage.unwrap() - u0.usage.unwrap() - 0.1).abs() < 1e-12,
        "{u0:?} {u1:?}"
    );
    assert!(u1.governs.contains("Mz 0.2 kN m of 2.0"), "{u1:?}");
    assert!(!u0.governs.contains("Mz"), "{u0:?}");
}

/// 2026-11-25 (C3.7.4: the bolt pattern's centroid beyond the connected
/// leg's centre line is not normal framing eccentricity; before, checked
/// as if it were, said NOT ON THE SAFE SIDE in the theory). An invented
/// L 80 × 8 (no root radius), 2.5 m, bolted by one leg at both ends: one
/// gauge line at 35 mm from the heel (within the 40 mm centre line) adds
/// nothing; at 55 mm, Δe = 15 mm beyond it bends the angle by P·Δe about
/// its principal axes (Δe/√2 along each for an equal angle), the stresses
/// at both axes' farthest fibres added, × 1/(1 − P/P_e) with P_e =
/// π² E A/(KL/r)², over F_y — added to the compression's usage.
#[test]
fn bolts_beyond_the_legs_centre_line_bend_the_angle() {
    use crate::check::member_usage;
    use crate::model::{Analysis, EndDesign, MemberForces};
    let (s, q) = profiled(ohl_sections::Shape::EqualAngle {
        h: 0.08,
        t: 0.008,
        r: 0.0,
    });
    let end = |g: f64| EndDesign {
        leg: Some(ConnectedLeg::Long),
        gauges: vec![g],
        ..EndDesign::default()
    };
    let with = |g: f64| {
        member(
            "D",
            s.clone(),
            2.5,
            MemberDesign {
                connected_leg: Some(ConnectedLeg::Long),
                ends: [Some(end(g)), Some(end(g))],
                ..MemberDesign::default()
            },
            Some(Ends::BothEnds),
        )
    };
    let a = Analysis {
        single_angle_moments: false,
        no_compression: false,
        redundant_force: None,
    };
    let f = MemberForces {
        axial: -40e3,
        end_forces: [0.0; 12],
        slack: false,
        bowed: false,
        length: 2.5,
        span_load: [0.0; 3],
        lumped_load: [0.0; 3],
    };
    let within = with(0.035);
    let c = capacity(&within, None);
    assert!(c.beyond_centre.is_none(), "{c:?}");
    let u_within = member_usage(&a, &within, &f, None, false, 1.0, None, None);
    let beyond = with(0.055);
    let c = capacity(&beyond, None);
    let b = c.beyond_centre.clone().expect("beyond the centre line");
    assert!((b.e - 0.015).abs() < 1e-12, "{b:?}");
    // By hand.
    let th = q.principal_angle;
    let (eu, ev) = (0.015 * th.cos(), -0.015 * th.sin());
    let per_n = ev.abs() / q.wel_u.unwrap() + eu.abs() / q.wel_v.unwrap();
    let kl_r = c.kl_r.unwrap();
    let pe = std::f64::consts::PI.powi(2) * E * q.area / (kl_r * kl_r);
    assert!((b.euler - pe).abs() < 1e-6 * pe, "{b:?} {pe}");
    let bend = 40e3 * per_n / ((1.0 - 40e3 / pe) * 355e6);
    let u = member_usage(&a, &beyond, &f, None, false, 1.0, None, None);
    let (cap, _) = c.compression.clone().unwrap();
    assert!(
        (u.usage.unwrap() - (40e3 / cap + bend)).abs() < 1e-9,
        "{u:?} {bend}"
    );
    assert!(
        (u.usage.unwrap() - u_within.usage.unwrap() - bend).abs() < 1e-9,
        "{u:?} {u_within:?}"
    );
    assert!(
        u.governs
            .contains("beyond the connected leg's centre line at a 15 mm"),
        "{u:?}"
    );
    // What it leaves out, said (never a pass).
    assert!(
        u.governs.contains("not on the safe side")
            && u.governs
                .contains("lateral-torsional buckling of the angle"),
        "{u:?}"
    );
    assert!(
        !u_within.governs.contains("lateral-torsional"),
        "{u_within:?}"
    );
    // Tension: unchanged (the curves are compression's).
    let t = MemberForces { axial: 40e3, ..f };
    assert_eq!(
        member_usage(&a, &beyond, &t, None, false, 1.0, None, None).usage,
        member_usage(&a, &within, &t, None, false, 1.0, None, None).usage
    );
}

/// The limits of `member-limits` (2026-11-26, O49) by hand on an L50×5
/// (no root radius), 2 m between its joints, every length ratio 1:
/// a leg's L/r against 150 (3.4), the same check's value; w/t (50 − 5)/5
/// = 9 against 25 (3.7.1); its thickness 5 mm against the suggested 3 mm
/// (3.3). A redundant's KL/r against 250, a member without a class
/// against 200 (other members, said), a tension-only member: the
/// thickness only.
#[test]
fn member_limits_by_hand() {
    use crate::limits::{
        ANGLE_W_T, LEG_SLENDERNESS, MEMBER_THICKNESS, OTHER_SLENDERNESS, REDUNDANT_SLENDERNESS,
        member_limits,
    };
    let (s, q) = profiled(ohl_sections::Shape::EqualAngle {
        h: 0.05,
        t: 0.005,
        r: 0.0,
    });
    let design = |class| MemberDesign {
        class,
        ..MemberDesign::default()
    };
    let mut m = member("m", s, 2.0, design(Some(MemberClass::Leg)), None);
    let c = capacity(&m, None);
    let l = member_limits(&m, &c);
    assert_eq!(l.len(), 3, "{l:?}");
    let r_v = (q.iv / q.area).sqrt();
    assert!(l[0].value >= 2.0 / r_v - 1e-9, "{l:?}");
    assert_eq!(Some(l[0].value), c.l_r);
    assert_eq!(l[0].maximum, Some(LEG_SLENDERNESS));
    assert!(l[0].compression);
    assert_eq!(l[0].clause, "ASCE 10-97 3.4 (leg members)");
    assert!((l[1].value - 9.0).abs() < 1e-12);
    assert_eq!(l[1].maximum, Some(ANGLE_W_T));
    assert_eq!(l[1].clause, "ASCE 10-97 3.7.1");
    assert!((l[2].value - 0.005).abs() < 1e-12);
    assert_eq!(l[2].minimum, Some(MEMBER_THICKNESS));
    assert!(!l[2].compression);
    m.design.class = Some(MemberClass::Redundant);
    let c = capacity(&m, None);
    let l = member_limits(&m, &c);
    assert_eq!(Some(l[0].value), c.kl_r);
    assert_eq!(l[0].maximum, Some(REDUNDANT_SLENDERNESS));
    m.design.class = None;
    let l = member_limits(&m, &capacity(&m, None));
    assert_eq!(l[0].maximum, Some(OTHER_SLENDERNESS));
    assert!(l[0].clause.contains("without a class taken so"), "{l:?}");
    m.kind = MemberKind::Tension;
    let l = member_limits(&m, &capacity(&m, None));
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].minimum, Some(MEMBER_THICKNESS));
}
