// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! SIA 263's buckling lengths from the bracing, by hand, on invented
//! members: a leg braced on its two faces staggered (5.4.1.3 read
//! strictly), a crossing diagonal (no hold out of the plane), a diagonal
//! held by a lateral support strong enough or not (4.2.3.7), a member
//! giving its own lengths.

use std::collections::HashMap;

use super::*;
use crate::bracing::{Piece, Point, Restraint};
use crate::model::{
    Condition, EndConditions, MemberClass, MemberDesign, MemberKind, Section, Source,
};
use crate::sia263::capacity_worked;

fn member(id: &str, s: Section, length: f64, eccentric: bool) -> Member {
    let c = Condition {
        value: Some(eccentric),
        source: Source::Member,
        why: None,
    };
    Member {
        id: id.into(),
        kind: MemberKind::Truss,
        frame: false,
        design: MemberDesign {
            class: Some(MemberClass::Bracing),
            ..MemberDesign::default()
        },
        section: s,
        length,
        k: 1.0,
        lk: None,
        capacity_tension: None,
        capacity_compression: None,
        joints: ["a".into(), "b".into()],
        conditions: EndConditions {
            eccentric: [c.clone(), c],
            connected: [false; 2],
        },
        onto: [None, None],
    }
}

/// An equal angle L 100×10, S 355.
fn equal() -> Section {
    let p = ohl_sections::Section {
        id: "L".into(),
        name: String::new(),
        description: String::new(),
        shape: ohl_sections::Shape::EqualAngle {
            h: 0.1,
            t: 0.01,
            r: 0.012,
        },
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
        wy: q.wel_y,
        wz: q.wel_z,
        e: 210e9,
        g: 81e9,
        yield_strength: Some(355e6),
        ultimate: Some(510e6),
        tension: None,
        compression: None,
        moment_y: None,
        moment_z: None,
        properties: Some(q),
        profile: Some(p),
    }
}

/// A section of the given values without a profile (y and z alike).
fn plain(area: f64, i: f64) -> Section {
    Section {
        area,
        iy: i,
        iz: i,
        j: 2.0 * i,
        i_min: i,
        wy: None,
        wz: None,
        e: 210e9,
        g: 81e9,
        yield_strength: Some(355e6),
        ultimate: Some(510e6),
        tension: None,
        compression: None,
        moment_y: None,
        moment_z: None,
        profile: None,
        properties: None,
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

const NO_CASE: Context = Context {
    resistance: None,
    axial: None,
    strength: 1.0,
};

/// A leg of four 1 m pieces held at its foot and top in both faces and on
/// its faces staggered (along y at 1 m and 3 m, along z at 2 m): no joint
/// between is held along u or v (each held along a leg of the angle, 45°
/// off): u and v over the whole 4 m (5.4.1.3 read strictly, the longer
/// length); λ_K = 4 m / i_v (concentric: no 0.9).
#[test]
fn a_leg_with_staggered_redundants() {
    let s = equal();
    let m = member("L2", s.clone(), 1.0, false);
    let b = Bracing {
        run: (0..4)
            .map(|k| piece(&format!("L{}", k + 1), k as f64, k as f64 + 1.0))
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
    let w = worked(&m, &b, &NO_CASE);
    assert_eq!((w.u.length, w.v.length), (4.0, 4.0));
    assert_eq!((w.y.length, w.z.length), (2.0, 2.0));
    let (c, _) = capacity_worked(&m, None, Some(&w));
    let lambda = 4.0 / (s.i_min / s.area).sqrt();
    assert!((c.kl_r.unwrap() - lambda).abs() < 1e-9, "{c:?}");
    let governs = &c.compression.unwrap().1;
    assert!(governs.contains("about v") && governs.contains("from the bracing"));
}

/// A crossing diagonal (5 m, eccentric) crossed at its middle by its
/// partner pulling hard: held there along the plane only (no rule holds
/// it out of the plane: Table 13 not taken), and not along u or v — v
/// over the whole 5 m, × 0.9 (5.4.1.6).
#[test]
fn a_crossing_diagonal_pair() {
    let s = equal();
    let d = member("D1", s.clone(), 5.0, true);
    let b = Bracing {
        run: vec![piece("D1", 0.0, 5.0)],
        length: 5.0,
        points: vec![
            point(0.0, "A", true, vec![]),
            point(2.5, "", false, vec![r("D2", [1.0, 0.0], true)]),
            point(5.0, "D", true, vec![]),
        ],
    };
    let axial = HashMap::from([("D1".to_owned(), -100e3), ("D2".to_owned(), 80e3)]);
    let res = HashMap::from([("D2".to_owned(), 1e6)]);
    let w = worked(
        &d,
        &b,
        &Context {
            resistance: Some(&res),
            axial: Some(&axial),
            strength: 1.0,
        },
    );
    assert_eq!((w.y.length, w.z.length, w.v.length), (5.0, 2.5, 5.0));
    let (c, _) = capacity_worked(&d, None, Some(&w));
    let lambda = 0.9 * 5.0 / (s.i_min / s.area).sqrt();
    assert!((c.kl_r.unwrap() - lambda).abs() < 1e-9, "{c:?}");
}

/// A diagonal of two 1.5 m pieces (y and z alike, no profile) held at its
/// middle along local y by a lateral support R: about z (the displacement
/// along y) over 1.5 m while R resists 1 % of the run's largest
/// compression (4.2.3.7) — 100 kN needs 1 kN; at 1 000 kN (10 kN needed)
/// R's 2 kN do not count: 3 m, said. No load case: every member counts.
#[test]
fn a_diagonal_held_by_a_lateral_support() {
    let d = member("D1a", plain(10e-4, 50e-8), 1.5, false);
    let b = Bracing {
        run: vec![piece("D1a", 0.0, 1.5), piece("D1b", 1.5, 3.0)],
        length: 3.0,
        points: vec![
            point(0.0, "A", true, vec![]),
            point(1.5, "M", false, vec![r("R", [1.0, 0.0], false)]),
            point(3.0, "D", true, vec![]),
        ],
    };
    let res = HashMap::from([("R".to_owned(), 2e3)]);
    for (n, z) in [(-100e3, 1.5), (-1000e3, 3.0)] {
        let axial = HashMap::from([("D1a".to_owned(), n), ("D1b".to_owned(), n / 2.0)]);
        let w = worked(
            &d,
            &b,
            &Context {
                resistance: Some(&res),
                axial: Some(&axial),
                strength: 1.0,
            },
        );
        assert_eq!(w.z.length, z, "{n}: {w:?}");
        assert_eq!(w.y.length, 3.0);
        assert_eq!(
            w.notes.iter().any(|x| x.contains("4.2.3.7")),
            z == 3.0,
            "{w:?}"
        );
    }
    assert_eq!(worked(&d, &b, &NO_CASE).z.length, 1.5);
}

/// A member giving its own lengths keeps them: none worked out.
#[test]
fn given_lengths_win() {
    let mut m = member("M", plain(10e-4, 50e-8), 2.0, false);
    m.design.length_ratios = Some([0.5; 3]);
    let b = Bracing {
        run: vec![piece("M", 0.0, 2.0)],
        length: 2.0,
        points: vec![point(0.0, "A", true, vec![]), point(2.0, "B", true, vec![])],
    };
    let set = HashMap::from([("M".to_owned(), b)]);
    let list = [m];
    assert!(
        Lengths::new(&list, &set)
            .worked(&list[0], None, 1.0)
            .is_none()
    );
}
