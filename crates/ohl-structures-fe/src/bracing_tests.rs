// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The runs and bracing points of invented members.

use super::*;

const HEAD: &str = "[analysis]\nnonlinear = false\ncapacity = \"simple\"\n\
    [[material]]\nid = \"steel\"\nelasticity = \"210 GPa\"\nyield = \"355 MPa\"\n\
    [[section]]\nid = \"s\"\nmaterial = \"steel\"\narea = \"10 cm2\"\ni_min = \"20 cm4\"\n";

/// A model of `nodes` (id, x, y, z in m, supported) and `members` (id,
/// first, second, extra lines).
fn model(nodes: &[(&str, f64, f64, f64, bool)], members: &[(&str, &str, &str, &str)]) -> Model {
    let mut t = HEAD.to_owned();
    for (id, x, y, z, fixed) in nodes {
        t.push_str(&format!(
            "[[node]]\nid = \"{id}\"\npoint = [\"{x} m\", \"{y} m\", \"{z} m\"]\n{}",
            if *fixed { "support = \"pinned\"\n" } else { "" }
        ));
    }
    for (id, a, b, extra) in members {
        t.push_str(&format!(
            "[[member]]\nid = \"{id}\"\nnodes = [\"{a}\", \"{b}\"]\nsection = \"s\"\n{extra}\n"
        ));
    }
    Model::parse(&t).unwrap_or_else(|e| panic!("{e}"))
}

fn index(m: &Model, id: &str) -> usize {
    m.members.iter().position(|x| x.id == id).unwrap()
}

/// A leg of four pieces (L1–L4, 1 m each, up z) with redundants
/// staggered on its two faces: along x at 1 m and 3 m, along y at 2 m;
/// the foot held, the top braced in both faces. One run of 4 m; its
/// points at 0, 1, 2, 3, 4 m; each intermediate point held along one
/// direction only, x or y.
#[test]
fn a_leg_with_staggered_redundants() {
    let m = model(
        &[
            ("N0", 0.0, 0.0, 0.0, true),
            ("N1", 0.0, 0.0, 1.0, false),
            ("N2", 0.0, 0.0, 2.0, false),
            ("N3", 0.0, 0.0, 3.0, false),
            ("N4", 0.0, 0.0, 4.0, false),
            ("X1", 2.0, 0.0, 1.0, true),
            ("Y2", 0.0, 2.0, 2.0, true),
            ("X3", 2.0, 0.0, 3.0, true),
            ("X4", 2.0, 0.0, 4.0, true),
            ("Y4", 0.0, 2.0, 4.0, true),
        ],
        &[
            ("L1", "N0", "N1", ""),
            ("L2", "N1", "N2", ""),
            ("L3", "N2", "N3", ""),
            ("L4", "N3", "N4", ""),
            ("R1", "N1", "X1", ""),
            ("R2", "N2", "Y2", ""),
            ("R3", "N3", "X3", ""),
            ("T1", "N4", "X4", ""),
            ("T2", "N4", "Y4", ""),
        ],
    );
    let b = bracing(&m);
    let (run, span) = b.of(index(&m, "L2")).unwrap();
    assert_eq!(
        run.members,
        ["L1", "L2", "L3", "L4"].map(|id| index(&m, id)).to_vec()
    );
    assert!((run.length - 4.0).abs() < 1e-12);
    assert_eq!(span, (1.0, 2.0));
    let ats: Vec<f64> = run.points.iter().map(|p| p.at).collect();
    assert_eq!(ats, vec![0.0, 1.0, 2.0, 3.0, 4.0]);
    assert!(run.points[0].support);
    let dirs = |p: &Point| -> Vec<[i32; 3]> {
        p.restraints
            .iter()
            .map(|r| r.direction.map(|x| x.round() as i32))
            .collect()
    };
    assert_eq!(dirs(&run.points[1]), vec![[1, 0, 0]]);
    assert_eq!(dirs(&run.points[2]), vec![[0, 1, 0]]);
    assert_eq!(dirs(&run.points[3]), vec![[1, 0, 0]]);
    assert_eq!(dirs(&run.points[4]), vec![[1, 0, 0], [0, 1, 0]]);
    assert!(
        run.points
            .iter()
            .all(|p| p.restraints.iter().all(|r| !r.through))
    );
    // The redundants are runs of their own, held at the leg along z.
    let (r1, _) = b.of(index(&m, "R1")).unwrap();
    assert_eq!(r1.members, vec![index(&m, "R1")]);
    assert_eq!(r1.points.len(), 2);
    assert_eq!(dirs(&r1.points[0]), vec![[0, 0, -1], [0, 0, 1]]);
    assert!(r1.points[1].support);
}

/// Two diagonals crossing at mid-length without a joint, each the other's
/// crossing partner: a point at the middle of each, held along the other
/// (passing through). Without the partner given: no point.
#[test]
fn a_crossing_pair_without_a_joint() {
    let nodes = [
        ("A", 0.0, 0.0, 0.0, true),
        ("B", 4.0, 0.0, 0.0, true),
        ("C", 0.0, 0.0, 4.0, true),
        ("D", 4.0, 0.0, 4.0, true),
    ];
    let m = model(
        &nodes,
        &[
            (
                "D1",
                "A",
                "D",
                "design = { class = \"crossing\", crossing = \"D2\" }",
            ),
            (
                "D2",
                "B",
                "C",
                "design = { class = \"crossing\", crossing = \"D1\" }",
            ),
        ],
    );
    let b = bracing(&m);
    let (run, span) = b.of(0).unwrap();
    let l = 32f64.sqrt();
    assert!((span.1 - l).abs() < 1e-12);
    assert_eq!(run.points.len(), 3);
    let mid = &run.points[1];
    assert!((mid.at - l / 2.0).abs() < 1e-9);
    assert_eq!(mid.joint, None);
    assert_eq!(mid.restraints.len(), 1);
    let r = &mid.restraints[0];
    assert_eq!(r.member, Some(1));
    assert!(r.through);
    // Square to D1 in the plane: along ±(1, 0, -1)/√2.
    assert!((r.direction[0].abs() - 0.5f64.sqrt()).abs() < 1e-9);
    assert!(r.direction[1].abs() < 1e-12);
    let plain = model(&nodes, &[("D1", "A", "D", ""), ("D2", "B", "C", "")]);
    assert_eq!(bracing(&plain).of(0).unwrap().0.points.len(), 2);
}

/// Diagonals split at their crossing joint: each a run of two halves;
/// at the crossing the other diagonal's halves pass through. A tension-
/// only member ending at a joint holds nothing; passing through, it does.
#[test]
fn a_crossing_at_a_joint_and_tension_only_members() {
    let nodes = [
        ("A", 0.0, 0.0, 0.0, true),
        ("B", 4.0, 0.0, 0.0, true),
        ("C", 0.0, 0.0, 4.0, true),
        ("D", 4.0, 0.0, 4.0, true),
        ("M", 2.0, 0.0, 2.0, false),
        ("E", 2.0, 3.0, 2.0, true),
    ];
    let members = |kind: &'static str| {
        [
            ("D1a", "A", "M", ""),
            ("D1b", "M", "D", ""),
            ("D2a", "B", "M", kind),
            ("D2b", "M", "C", kind),
            ("H", "M", "E", "kind = \"tension\""),
        ]
    };
    let m = model(&nodes, &members(""));
    let b = bracing(&m);
    let (run, _) = b.of(index(&m, "D1b")).unwrap();
    assert_eq!(run.members, vec![index(&m, "D1a"), index(&m, "D1b")]);
    let mid = &run.points[1];
    assert_eq!(mid.joint, Some(4));
    // The other diagonal's halves, through; the tension-only H not.
    let by: Vec<(Option<usize>, bool)> = mid
        .restraints
        .iter()
        .map(|r| (r.member, r.through))
        .collect();
    assert_eq!(
        by,
        vec![
            (Some(index(&m, "D2a")), true),
            (Some(index(&m, "D2b")), true)
        ]
    );
    // The other diagonal tension-only: still holding (passing through).
    let t = model(&nodes, &members("kind = \"tension\""));
    let bt = bracing(&t);
    let (run, _) = bt.of(index(&t, "D1a")).unwrap();
    assert_eq!(run.points[1].restraints.len(), 2);
    assert!(run.points[1].restraints.iter().all(|r| r.through));
}

/// A support fixed along some axes only holds the joint along each of
/// them square to the run (none along the run itself).
#[test]
fn a_support_fixed_along_some_axes() {
    let mut t = HEAD.to_owned();
    t.push_str(
        "[[node]]\nid = \"a\"\npoint = [\"0 m\", \"0 m\", \"0 m\"]\nsupport = \"pinned\"\n\
         [[node]]\nid = \"b\"\npoint = [\"1.5 m\", \"0 m\", \"0 m\"]\nsupport = [\"x\", \"y\"]\n\
         [[member]]\nid = \"m\"\nnodes = [\"a\", \"b\"]\nsection = \"s\"\n",
    );
    let m = Model::parse(&t).unwrap();
    let b = bracing(&m);
    let (run, _) = b.of(0).unwrap();
    let end = &run.points[1];
    assert!(!end.support);
    assert_eq!(end.restraints.len(), 1);
    assert_eq!(end.restraints[0].member, None);
    assert!((end.restraints[0].direction[1].abs() - 1.0).abs() < 1e-12);
}

/// A member not design-checked and a cable hold nothing; a member nearly
/// in line (within 10°) neither — it continues the run instead.
#[test]
fn what_does_not_hold() {
    let m = model(
        &[
            ("A", 0.0, 0.0, 0.0, true),
            ("B", 0.0, 0.0, 1.0, false),
            ("C", 0.0, 0.0, 2.0, true),
            ("E", 1.0, 0.0, 1.0, true),
            ("F", 0.0, 1.0, 1.0, true),
        ],
        &[
            ("P1", "A", "B", ""),
            ("P2", "B", "C", ""),
            ("Q", "B", "E", "check = false"),
            ("W", "B", "F", "kind = \"cable\""),
        ],
    );
    let b = bracing(&m);
    let (run, _) = b.of(0).unwrap();
    assert_eq!(run.members, vec![0, 1]);
    assert!(run.points[1].restraints.is_empty());
    assert_eq!(b.run_of[index(&m, "Q")], None);
    assert_eq!(b.run_of[index(&m, "W")], None);
}
