// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! ASCE 10-97's buckling lengths from the bracing, by hand, on invented
//! members: a leg braced on its two faces staggered (Figure 3.2), a
//! crossing diagonal with its partner pulling more or less than 20 % of
//! its compression (Appendix B, Example 7), a diagonal held by a
//! redundant member strong enough or not (C3.7.4), a member nothing
//! holds.

use std::collections::HashMap;

use super::*;
use crate::asce10::{capacity_worked, effective_slenderness};
use crate::bracing::{Piece, Point, Restraint};
use crate::model::{Condition, EndConditions, MemberDesign, MemberKind, Section, Source};

/// Both ends concentric (`false`) or eccentric, not restrained, given.
fn ends(eccentric: bool) -> EndConditions {
    let c = |v: bool| Condition {
        value: Some(v),
        source: Source::Member,
        why: None,
    };
    EndConditions {
        eccentric: [c(eccentric), c(eccentric)],
        restrained: [c(false), c(false)],
        connected: [false; 2],
    }
}

/// An angle section, f_y 355 MPa.
fn angle(shape: ohl_sections::Shape) -> Section {
    let p = ohl_sections::Section {
        id: "p".into(),
        name: String::new(),
        description: String::new(),
        shape,
        material: None,
        given: ohl_sections::Given::default(),
    };
    let q = p.properties();
    Section {
        area: q.area,
        iy: q.iy,
        iz: q.iz,
        j: q.it.unwrap_or(0.0),
        i_min: q.iv,
        wy: None,
        wz: None,
        e: 210e9,
        g: 81e9,
        yield_strength: Some(355e6),
        ultimate: None,
        tension: None,
        compression: None,
        moment_y: None,
        moment_z: None,
        profile: Some(p),
        properties: Some(q),
    }
}

fn equal() -> Section {
    angle(ohl_sections::Shape::EqualAngle {
        h: 0.1,
        t: 0.01,
        r: 0.012,
    })
}

fn member(id: &str, s: Section, length: f64, class: MemberClass, eccentric: bool) -> Member {
    Member {
        id: id.into(),
        kind: MemberKind::Truss,
        frame: false,
        design: MemberDesign {
            class: Some(class),
            ..MemberDesign::default()
        },
        section: s,
        length,
        k: 1.0,
        lk: None,
        capacity_tension: None,
        capacity_compression: None,
        joints: ["a".into(), "b".into()],
        conditions: ends(eccentric),
        onto: [None, None],
    }
}

fn r(member: &str, d: [f64; 2], through: bool) -> Restraint {
    Restraint {
        member: member.into(),
        direction: d,
        through,
    }
}

fn point(at: f64, joint: &str, support: bool, restraints: Vec<Restraint>) -> Point {
    Point {
        at,
        joint: joint.into(),
        support,
        restraints,
    }
}

fn piece(member: &str, start: f64, end: f64) -> Piece {
    Piece {
        member: member.into(),
        start,
        end,
        forward: true,
    }
}

fn radius(s: &Section, i: f64) -> f64 {
    (i / s.area).sqrt()
}

/// A leg of four 1 m pieces, held at its foot and top in both faces and
/// on its faces staggered: along y at 1 m and 3 m, along z at 2 m. Its
/// second piece: v over 1 m (every point holds an angle's v, Figure 3.2
/// (a)); its geometric axes over 2 m each (y held at 2 m, z at 1 m and 3
/// m), × 1.2 (staggered, Figure 3.2 (b), (c)): KL/r = 2.4 m / r_x
/// (concentric, Eq. 3.7-4), larger than 1 m / r_v. Held in both faces 4 m
/// apart, two panels of 2 m: Figure 3.2's condition (every four panels)
/// met; a leg held in both faces only 10 m apart, five panels: v between
/// them (10 m, the longer length), said.
#[test]
fn a_leg_with_staggered_redundants() {
    let s = equal();
    let ids = ["L1", "L2", "L3", "L4"];
    let legs: Vec<Member> = ids
        .iter()
        .map(|id| member(id, s.clone(), 1.0, MemberClass::Leg, false))
        .collect();
    let b = Bracing {
        run: (0..4)
            .map(|k| piece(ids[k], k as f64, k as f64 + 1.0))
            .collect(),
        length: 4.0,
        points: vec![
            point(0.0, "N0", true, vec![]),
            point(1.0, "N1", false, vec![r("R1", [1.0, 0.0], false)]),
            point(2.0, "N2", false, vec![r("R2", [0.0, 1.0], false)]),
            point(3.0, "N3", false, vec![r("R3", [1.0, 0.0], false)]),
            point(
                4.0,
                "N4",
                false,
                vec![r("T1", [1.0, 0.0], false), r("T2", [0.0, 1.0], false)],
            ),
        ],
    };
    let by: HashMap<String, &Member> = legs.iter().map(|m| (m.id.clone(), m)).collect();
    let cx = Context {
        members: &by,
        adequate: None,
        axial: None,
    };
    let w = worked(&legs[1], &b, &cx);
    assert_eq!(w.axes[2].span.length, 1.0);
    assert_eq!(w.axes[0].span.length, 2.0);
    assert_eq!(w.axes[1].span.length, 2.0);
    assert_eq!(w.axes[0].factor, STAGGER);
    assert!(w.notes.iter().any(|n| n.contains("staggered")), "{w:?}");
    let c = capacity_worked(&legs[1], None, Some(&w));
    let geometric = 2.4 / radius(&s, s.iy);
    assert!(geometric > 1.0 / radius(&s, s.i_min));
    assert!((c.kl_r.unwrap() - geometric).abs() < 1e-9, "{c:?}");
    assert!(c.compression.unwrap().1.contains("from the bracing"));
    // Held in both faces only at the foot and at 10 m, the faces
    // alternating every metre (panels of 2 m): more than four panels,
    // v between the points held in both faces.
    let mut b6 = b.clone();
    b6.length = 10.0;
    b6.run = (0..10)
        .map(|k| piece(&format!("L{}", k + 1), k as f64, k as f64 + 1.0))
        .collect();
    b6.points = (0..=10)
        .map(|k| match k {
            0 | 10 => point(k as f64, &format!("N{k}"), true, vec![]),
            _ => point(
                k as f64,
                &format!("N{k}"),
                false,
                vec![r(
                    "R",
                    if k % 2 == 1 { [1.0, 0.0] } else { [0.0, 1.0] },
                    false,
                )],
            ),
        })
        .collect();
    let w6 = worked(&legs[1], &b6, &cx);
    assert_eq!(w6.axes[2].span.length, 10.0);
    assert_eq!(w6.axes[0].factor, 1.0);
    assert!(
        w6.notes.iter().any(|n| n.contains("within 4 panels")),
        "{w6:?}"
    );
}

/// A crossing diagonal (5 m, eccentric at both ends) crossed at its
/// middle by its partner away from a joint, held there along the plane
/// (local y). The partner pulling 30 % of its compression: held every
/// way, every axis over 2.5 m. Pulling 10 % (or pushed): v over 2.5 m
/// with one end eccentric and the crossover concentric (Eq. 3.7-6), the
/// axis out of the plane over 5 m with both ends eccentric (Eq. 3.7-7) —
/// Appendix B, Example 7. An unequal angle bolted by its long leg: not
/// held out of the plane at the crossing even pulled (Example 7's note).
#[test]
fn a_crossing_diagonal_pair() {
    let s = equal();
    let d = member("D1", s.clone(), 5.0, MemberClass::Crossing, true);
    let b = Bracing {
        run: vec![piece("D1", 0.0, 5.0)],
        length: 5.0,
        points: vec![
            point(0.0, "A", true, vec![]),
            point(2.5, "", false, vec![r("D2", [1.0, 0.0], true)]),
            point(5.0, "D", true, vec![]),
        ],
    };
    let by: HashMap<String, &Member> = HashMap::from([("D1".to_owned(), &d)]);
    let axial =
        |partner: f64| HashMap::from([("D1".to_owned(), -100e3), ("D2".to_owned(), partner)]);
    let pulled = axial(30e3);
    let held = worked(
        &d,
        &b,
        &Context {
            members: &by,
            adequate: None,
            axial: Some(&pulled),
        },
    );
    assert!(held.axes.iter().all(|a| a.span.length == 2.5), "{held:?}");
    for partner in [10e3, -20e3] {
        let a = axial(partner);
        let w = worked(
            &d,
            &b,
            &Context {
                members: &by,
                adequate: None,
                axial: Some(&a),
            },
        );
        // y: the displacement along z, out of the plane.
        assert_eq!(w.axes[0].span.length, 5.0);
        assert_eq!(w.axes[1].span.length, 2.5);
        assert_eq!(w.axes[2].span.length, 2.5);
        assert_eq!(w.axes[2].span.between[1], "the crossing");
        let c = capacity_worked(&d, None, Some(&w));
        let v = effective_slenderness(2.5 / radius(&s, s.i_min), Ends::OneEnd, Ends::None);
        let out = effective_slenderness(5.0 / radius(&s, s.iy), Ends::BothEnds, Ends::None);
        assert!((c.kl_r.unwrap() - v.max(out)).abs() < 1e-9, "{c:?}");
    }
    // An unequal angle bolted by its long leg.
    let mut u = member(
        "D1",
        angle(ohl_sections::Shape::UnequalAngle {
            h: 0.1,
            b: 0.065,
            t: 0.008,
            r: 0.01,
        }),
        5.0,
        MemberClass::Crossing,
        true,
    );
    u.design.connected_leg = Some(ConnectedLeg::Long);
    let w = worked(
        &u,
        &b,
        &Context {
            members: &by,
            adequate: None,
            axial: Some(&pulled),
        },
    );
    assert_eq!(w.axes[0].span.length, 5.0);
    assert!(w.notes.iter().any(|n| n.contains("long leg")));
}

/// A diagonal of two 1.5 m pieces held at its middle joint by a redundant
/// member along the plane: v over 1.5 m. The redundant's own KL/r above
/// 250: it does not count (C3.7.4), v over the whole 3 m, said.
#[test]
fn a_diagonal_held_by_a_redundant_member() {
    let s = equal();
    let mut pieces = vec![
        member("D1a", s.clone(), 1.5, MemberClass::Bracing, true),
        member("D1b", s.clone(), 1.5, MemberClass::Bracing, true),
    ];
    let b = Bracing {
        run: vec![piece("D1a", 0.0, 1.5), piece("D1b", 1.5, 3.0)],
        length: 3.0,
        points: vec![
            point(0.0, "A", true, vec![]),
            point(1.5, "M", false, vec![r("R", [1.0, 0.0], false)]),
            point(3.0, "D", true, vec![]),
        ],
    };
    let bracing: HashMap<String, Bracing> =
        HashMap::from([("D1a".to_owned(), b.clone()), ("D1b".to_owned(), b)]);
    for (thin, v) in [(false, 1.5), (true, 3.0)] {
        // The redundant gives its own lengths: a stocky or a slender one.
        let mut red = member(
            "R",
            angle(ohl_sections::Shape::EqualAngle {
                h: if thin { 0.02 } else { 0.06 },
                t: 0.003,
                r: 0.0,
            }),
            if thin { 3.0 } else { 1.0 },
            MemberClass::Redundant,
            true,
        );
        red.design.length_ratios = Some([1.0; 3]);
        pieces.truncate(2);
        pieces.push(red);
        let l = Lengths::new(&pieces, &bracing);
        let w = l.worked(&pieces[0], None).unwrap();
        assert_eq!(w.axes[2].span.length, v, "thin {thin}: {w:?}");
        assert_eq!(
            w.notes.iter().any(|n| n.contains("not counted as bracing")),
            thin
        );
    }
}

/// A member nothing holds (no member at either end, no support): twice
/// its length, said not checked (a mechanism unless the joints' bending
/// holds it). Held every way at one end only: a cantilever, twice. A
/// member giving its own lengths: none worked out.
#[test]
fn nothing_holds_it_and_given_lengths() {
    let s = equal();
    let m = member("M", s.clone(), 2.0, MemberClass::Bracing, false);
    let by: HashMap<String, &Member> = HashMap::from([("M".to_owned(), &m)]);
    let cx = Context {
        members: &by,
        adequate: None,
        axial: None,
    };
    let free = Bracing {
        run: vec![piece("M", 0.0, 2.0)],
        length: 2.0,
        points: vec![
            point(0.0, "A", false, vec![]),
            point(2.0, "B", false, vec![]),
        ],
    };
    let w = worked(&m, &free, &cx);
    assert!(w.axes.iter().all(|a| a.span.length == 4.0 && a.span.unheld));
    assert!(w.notes.iter().any(|n| n.contains("not checked")));
    let mut one = free.clone();
    one.points[0].support = true;
    let w = worked(&m, &one, &cx);
    assert!(
        w.axes
            .iter()
            .all(|a| a.span.length == 4.0 && a.span.free && !a.span.unheld)
    );
    let mut g = m.clone();
    g.design.length_ratios = Some([0.5; 3]);
    let set: HashMap<String, Bracing> = HashMap::from([("M".to_owned(), free)]);
    let list = [g];
    assert!(Lengths::new(&list, &set).worked(&list[0], None).is_none());
}
