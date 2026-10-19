// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The body tools: generators (counts and positions worked out by hand
//! for a small frustum), rebuild, symmetry, groups and the levels.

use std::path::Path;

use ohl_structures::{Detail, LibraryItem, StructureType};
use toml_edit::DocumentMut;

use super::*;
use crate::body::{set_symmetry, symmetry_option};

const EMPTY: &str =
    "format = \"ohl-structure\"\nversion = 2\nid = \"G\"\nname = \"g\"\nkind = \"lattice\"\n";

fn read(doc: &DocumentMut) -> StructureType {
    let text = doc.to_string();
    let t =
        StructureType::parse(Path::new("g.ohls"), &text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    body::check(&t).unwrap();
    t
}

/// A frustum 6 m wide at the feet (z = 0), 2 m at the waist, panels
/// topped at `tops`.
fn frustum(tops: &[(f64, Bracing, bool)]) -> Generator {
    Generator {
        id: "body".into(),
        kind: Kind::Panels(Panels {
            bottom: Some(0.0),
            feet: [6.0, 6.0],
            waist: [2.0, 2.0],
            panels: tops
                .iter()
                .map(|&(top, bracing, horizontal)| Panel {
                    top,
                    bracing,
                    horizontal,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }),
    }
}

fn joint(b: &Body, id: &str) -> [f64; 3] {
    b.joints
        .iter()
        .find(|j| j.id == id)
        .unwrap_or_else(|| panic!("no joint {id}"))
        .at
}

fn joined(b: &Body, a: &str, c: &str) -> bool {
    b.members
        .iter()
        .any(|m| (m.nodes[0] == a && m.nodes[1] == c) || (m.nodes[0] == c && m.nodes[1] == a))
}

fn applied(doc: &mut DocumentMut, g: &Generator) -> Report {
    let t = read(doc);
    apply(doc, &t, g).unwrap()
}

/// Two X panels with horizontals: 12 corners; 8 legs, 8 horizontals,
/// 16 diagonals. The width shrinks linearly: 6 m, 4 m, 2 m.
#[test]
fn x_panels_with_horizontals() {
    let t = read(&EMPTY.parse().unwrap());
    let g = frustum(&[(5.0, Bracing::X, true), (10.0, Bracing::X, true)]);
    let b = generate(&t, &g).unwrap();
    assert_eq!((b.joints.len(), b.members.len()), (12, 32));
    assert_eq!(joint(&b, "body-L0-1"), [3.0, 3.0, 0.0]);
    assert_eq!(joint(&b, "body-L1-2"), [-2.0, 2.0, 5.0]);
    assert_eq!(joint(&b, "body-L2-3"), [-1.0, -1.0, 10.0]);
    assert_eq!(joint(&b, "body-L2-4"), [1.0, -1.0, 10.0]);
    let count = |g: &str| {
        b.members
            .iter()
            .filter(|m| m.group.as_deref() == Some(g))
            .count()
    };
    assert_eq!(count("body-leg"), 8);
    assert_eq!(
        (count("body-horizontal-1"), count("body-horizontal-2")),
        (4, 4)
    );
    assert_eq!((count("body-bracing-1"), count("body-bracing-2")), (8, 8));
    // The x+ face of panel 1: both diagonals, no horizontal at the feet.
    assert!(joined(&b, "body-L0-4", "body-L1-1") && joined(&b, "body-L0-1", "body-L1-4"));
    assert!(!joined(&b, "body-L0-4", "body-L0-1"));
}

/// Single diagonals, no horizontals: 8 legs, one diagonal per face and
/// panel; Warren alternates its direction from panel to panel.
#[test]
fn single_and_warren() {
    let t = read(&EMPTY.parse().unwrap());
    let b = generate(
        &t,
        &frustum(&[
            (5.0, Bracing::Single, false),
            (10.0, Bracing::Single, false),
        ]),
    )
    .unwrap();
    assert_eq!((b.joints.len(), b.members.len()), (12, 16));
    assert!(joined(&b, "body-L0-4", "body-L1-1") && joined(&b, "body-L1-4", "body-L2-1"));
    let w = generate(
        &t,
        &frustum(&[
            (5.0, Bracing::Warren, false),
            (10.0, Bracing::Warren, false),
        ]),
    )
    .unwrap();
    assert_eq!((w.joints.len(), w.members.len()), (12, 16));
    // Up to the right, then back up to the left: a zig-zag.
    assert!(joined(&w, "body-L0-4", "body-L1-1") && joined(&w, "body-L1-1", "body-L2-4"));
    assert!(!joined(&w, "body-L1-4", "body-L2-1"));
}

/// K: a joint in the middle of each top edge, the horizontal split
/// there; diamond: joints in the middle of the four edges, legs and both
/// horizontals split.
#[test]
fn k_and_diamond() {
    let t = read(&EMPTY.parse().unwrap());
    let k = generate(&t, &frustum(&[(4.0, Bracing::K, false)])).unwrap();
    // 8 corners and 4 middles; 4 legs, 8 half horizontals, 8 diagonals.
    assert_eq!((k.joints.len(), k.members.len()), (12, 20));
    assert_eq!(joint(&k, "body-L1-F1"), [1.0, 0.0, 4.0]);
    assert!(joined(&k, "body-L0-4", "body-L1-F1") && joined(&k, "body-L0-1", "body-L1-F1"));
    assert!(joined(&k, "body-L1-4", "body-L1-F1") && !joined(&k, "body-L1-4", "body-L1-1"));
    let d = generate(&t, &frustum(&[(4.0, Bracing::Diamond, true)])).unwrap();
    // 8 corners, 4 leg middles, 4 + 4 edge middles; 8 half legs,
    // 8 + 8 half horizontals, 16 diamond members.
    assert_eq!((d.joints.len(), d.members.len()), (20, 40));
    assert_eq!(joint(&d, "body-P1-1"), [2.0, 2.0, 2.0]);
    assert_eq!(joint(&d, "body-L0-F2"), [0.0, 3.0, 0.0]);
    assert!(joined(&d, "body-L0-F1", "body-P1-1") && joined(&d, "body-P1-1", "body-L1-F1"));
}

/// Crossarms from the body face on the point's side; the point goes on
/// the tip joint. A peak on the top.
#[test]
fn crossarms_and_peak() {
    let mut doc: DocumentMut = EMPTY.parse().unwrap();
    applied(
        &mut doc,
        &frustum(&[(5.0, Bracing::X, true), (10.0, Bracing::X, true)]),
    );
    let t = read(&doc);
    edits::add_point(&mut doc, &t, [6.0, 0.0, 7.5]);
    let t = read(&doc);
    let arm = Generator {
        id: "arm".into(),
        kind: Kind::Crossarm(Crossarm {
            points: vec!["A1".into()],
            shape: ArmShape::Triangular,
            ..Default::default()
        }),
    };
    let b = generate(&t, &arm).unwrap();
    // From the x+ face at 5 m (x = 2) and 10 m (x = 1) to the tip.
    assert_eq!(b.joints.len(), 1);
    assert_eq!(joint(&b, "arm-A1"), [6.0, 0.0, 7.5]);
    for root in ["body-L1-1", "body-L1-4", "body-L2-1", "body-L2-4"] {
        assert!(joined(&b, root, "arm-A1"), "{root}");
    }
    assert_eq!(b.members.len(), 4);
    let rect = Generator {
        id: "arm".into(),
        kind: Kind::Crossarm(Crossarm {
            points: vec!["A1".into()],
            shape: ArmShape::Rectangular,
            ..Default::default()
        }),
    };
    let r = generate(&t, &rect).unwrap();
    assert_eq!((r.joints.len(), r.members.len()), (2, 7));
    assert_eq!(joint(&r, "arm-A1-top"), [6.0, 0.0, 12.5]);
    applied(&mut doc, &arm);
    let t = read(&doc);
    let a1 = t.points.iter().find(|p| p.label == "A1").unwrap();
    assert_eq!(a1.node.as_deref(), Some("arm-A1"));
    assert_eq!(t.point_local("A1", None), Some([6.0, 0.0, 7.5]));
    // Built again: the same.
    let again = rebuild(&mut doc, &t, "arm").unwrap();
    assert_eq!((again.joints, again.members, again.removed), (1, 4, 0));
    let t = read(&doc);
    let peak = Generator {
        id: "peak".into(),
        kind: Kind::Peak(Peak {
            points: Vec::new(),
            height: 3.0,
        }),
    };
    let p = generate(&t, &peak).unwrap();
    assert_eq!(joint(&p, "peak-tip"), [0.0, 0.0, 13.0]);
    assert_eq!(p.members.len(), 4);
    assert!(joined(&p, "body-L2-3", "peak-tip"));
}

/// A rectangular arm on a point at the top of the body: its other tip one
/// level down (not above the body), and a peak still finds the body's top.
#[test]
fn a_rectangular_arm_at_the_top_and_a_peak() {
    let mut doc: DocumentMut = EMPTY.parse().unwrap();
    applied(
        &mut doc,
        &frustum(&[(5.0, Bracing::X, true), (10.0, Bracing::X, true)]),
    );
    let t = read(&doc);
    edits::add_point(&mut doc, &t, [6.0, 0.0, 10.0]);
    let t = read(&doc);
    let rect = Generator {
        id: "arm".into(),
        kind: Kind::Crossarm(Crossarm {
            points: vec!["A1".into()],
            shape: ArmShape::Rectangular,
            ..Default::default()
        }),
    };
    let r = generate(&t, &rect).unwrap();
    assert_eq!((r.joints.len(), r.members.len()), (2, 7));
    assert_eq!(joint(&r, "arm-A1-bottom"), [6.0, 0.0, 5.0]);
    for root in ["body-L2-1", "body-L2-4"] {
        assert!(joined(&r, root, "arm-A1"), "{root}");
    }
    for root in ["body-L1-1", "body-L1-4"] {
        assert!(joined(&r, root, "arm-A1-bottom"), "{root}");
    }
    applied(&mut doc, &rect);
    let t = read(&doc);
    let peak = Generator {
        id: "peak".into(),
        kind: Kind::Peak(Peak {
            points: Vec::new(),
            height: 3.0,
        }),
    };
    let p = generate(&t, &peak).unwrap();
    assert_eq!(joint(&p, "peak-tip"), [0.0, 0.0, 13.0]);
    assert_eq!(p.members.len(), 4);
}

/// A face copied to the opposite one: what is on it and not there yet
/// (a hand-made redundant), mirrored.
#[test]
fn a_face_is_copied() {
    let mut doc: DocumentMut = EMPTY.parse().unwrap();
    applied(
        &mut doc,
        &frustum(&[(5.0, Bracing::X, true), (10.0, Bracing::X, true)]),
    );
    let t = read(&doc);
    // A joint in the middle of the 5 m horizontal of the back face (y−),
    // braced to the feet.
    let mid = edits::add_joint(&mut doc, Some(&t), [0.0, -2.0, 5.0]);
    for foot in ["body-L0-3", "body-L0-4"] {
        let t = read(&doc);
        edits::add_member(&mut doc, &t, &mid, foot).unwrap();
    }
    let copy = Generator {
        id: "front".into(),
        kind: Kind::CopyFace(CopyFace {
            body: "body".into(),
            face: Face::YMinus,
        }),
    };
    let r = applied(&mut doc, &copy);
    assert_eq!((r.joints, r.members), (1, 2));
    let t = read(&doc);
    let n = t
        .nodes
        .iter()
        .find(|n| n.id == format!("front-{mid}"))
        .unwrap();
    assert_eq!(n.point.map(|v| v.si()), [0.0, 2.0, 5.0]);
}

/// A rebuild replaces what the generator made, keeps what was set on it
/// and what was made by hand; a joint it no longer makes stays while a
/// hand-made member uses it, and is reported.
#[test]
fn rebuild_keeps_hand_edits() {
    let mut doc: DocumentMut = EMPTY.parse().unwrap();
    applied(
        &mut doc,
        &frustum(&[(5.0, Bracing::X, true), (10.0, Bracing::X, true)]),
    );
    let t = read(&doc);
    edits::add_member(&mut doc, &t, "body-L2-1", "body-L0-3").unwrap();
    let t = read(&doc);
    let hand = t.members.last().unwrap().id.clone();
    assert!(t.members.last().unwrap().made_by.is_none());
    // A section on a generated member.
    edits::add_item(
        &mut doc,
        "section",
        "L50",
        &[("material", "S235"), ("area", "5 cm2")],
    );
    edits::set_member_section(&mut doc, "body-M1", Some("L50"));
    // One panel now: the 10 m level goes.
    let r = applied(&mut doc, &frustum(&[(10.0, Bracing::X, true)]));
    let t = read(&doc);
    assert_eq!(
        r.kept,
        vec![format!("joint body-L2-1 (member {hand} uses it)")]
    );
    assert!(t.members.iter().any(|m| m.id == hand));
    let kept = t.nodes.iter().find(|n| n.id == "body-L2-1").unwrap();
    assert!(kept.made_by.is_none());
    // The other joints of 10 m went: 8 corners and the one kept.
    assert_eq!(t.nodes.len(), 9);
    assert!(t.nodes.iter().all(|n| n.id != "body-L2-2"));
    // 4 legs, 4 horizontals, 8 diagonals, and the hand-made one.
    assert_eq!(t.members.len(), 17);
    let m1 = t.members.iter().find(|m| m.id == "body-M1").unwrap();
    assert_eq!(m1.section.as_deref(), Some("L50"));
    // Version 3 written.
    assert!(doc.to_string().contains("version = 3"));
    // The generator removed: what it made goes, but the joints the
    // hand-made member uses (now made by hand).
    let r = remove_generator(&mut doc, &t, "body");
    let t = read(&doc);
    assert_eq!(t.nodes.len(), 2);
    assert_eq!(t.members.len(), 1);
    assert!(t.generators.is_empty());
    assert_eq!(r.kept.len(), 1, "{:?}", r.kept);
}

/// With the symmetry on, a joint or member added, moved or deleted takes
/// its mirror images; a joint on a mirror plane stays on it.
#[test]
fn symmetry_mirrors_edits() {
    let mut doc: DocumentMut = EMPTY.parse().unwrap();
    set_symmetry(&mut doc, Some(Mirror::Xy));
    let t = read(&doc);
    assert_eq!(symmetry_option(&t), Some(Mirror::Xy));
    let ids = symmetry::add_joint(&mut doc, &t, [3.0, 4.0, 10.0]);
    assert_eq!(ids.len(), 4);
    let t = read(&doc);
    let centre = symmetry::add_joint(&mut doc, &t, [0.0, 0.0, 10.0]);
    assert_eq!(centre.len(), 1);
    let t = read(&doc);
    let members = symmetry::add_member(&mut doc, &t, &ids[0], &centre[0]).unwrap();
    assert_eq!(members.len(), 4);
    let t = read(&doc);
    symmetry::move_to(
        &mut doc,
        &t,
        &Handle::Joint(ids[0].clone()),
        [5.0, 4.0, 11.0],
    )
    .unwrap();
    let t = read(&doc);
    let at = |id: &str| {
        t.nodes
            .iter()
            .find(|n| n.id == id)
            .unwrap()
            .point
            .map(|v| v.si())
    };
    let mut images: Vec<[f64; 3]> = ids.iter().map(|i| at(i)).collect();
    images.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(
        images,
        vec![
            [-5.0, -4.0, 11.0],
            [-5.0, 4.0, 11.0],
            [5.0, -4.0, 11.0],
            [5.0, 4.0, 11.0]
        ]
    );
    // The centre joint stays on both planes.
    symmetry::move_to(
        &mut doc,
        &t,
        &Handle::Joint(centre[0].clone()),
        [0.3, -0.2, 12.0],
    )
    .unwrap();
    let t = read(&doc);
    assert_eq!(
        t.nodes
            .iter()
            .find(|n| n.id == centre[0])
            .unwrap()
            .point
            .map(|v| v.si()),
        [0.0, 0.0, 12.0]
    );
    // A member deleted takes its images; a joint its images and members.
    let r = symmetry::removal(&t, &Handle::Member(members[0].clone()));
    assert_eq!(r.members.len(), 4);
    let r = symmetry::removal(&t, &Handle::Joint(ids[1].clone()));
    assert_eq!((r.joints.len(), r.members.len()), (4, 4));
    edits::remove(&mut doc, &r);
    let t = read(&doc);
    assert_eq!((t.nodes.len(), t.members.len()), (1, 0));
    // Make symmetric: a half drawn by hand gets its other half.
    let mut doc: DocumentMut = EMPTY.parse().unwrap();
    let t = read(&doc);
    let a = edits::add_joint(&mut doc, Some(&t), [2.0, 0.0, 0.0]);
    let t = read(&doc);
    let b = edits::add_joint(&mut doc, Some(&t), [1.0, 0.0, 10.0]);
    let t = read(&doc);
    edits::add_member(&mut doc, &t, &a, &b).unwrap();
    set_symmetry(&mut doc, Some(Mirror::X));
    let t = read(&doc);
    assert_eq!(symmetry::make_symmetric(&mut doc, &t), (2, 1));
    let t = read(&doc);
    assert_eq!((t.nodes.len(), t.members.len()), (4, 2));
    assert_eq!(symmetry::make_symmetric(&mut doc, &t), (0, 0));
    set_symmetry(&mut doc, None);
    assert!(read(&doc).symmetry.is_empty());
}

/// A generated body is level 2; a section per group and the feet
/// supported make it level 3.
#[test]
fn groups_take_the_body_to_level_3() {
    let mut doc: DocumentMut = EMPTY.parse().unwrap();
    applied(
        &mut doc,
        &frustum(&[(5.0, Bracing::X, true), (10.0, Bracing::X, true)]),
    );
    let t = read(&doc);
    assert_eq!(t.detail(), Detail::Outline);
    let names: Vec<String> = groups(&t).into_iter().map(|g| g.name).collect();
    assert_eq!(
        names,
        [
            "body-bracing-1",
            "body-bracing-2",
            "body-horizontal-1",
            "body-horizontal-2",
            "body-leg"
        ]
    );
    edits::add_item(&mut doc, "material", "S235", &[("elasticity", "210 GPa")]);
    edits::add_item(
        &mut doc,
        "section",
        "L80",
        &[("material", "S235"), ("area", "12 cm2")],
    );
    edits::add_item(
        &mut doc,
        "section",
        "L50",
        &[("material", "S235"), ("area", "5 cm2")],
    );
    for g in &names {
        let t = read(&doc);
        let s = if g == "body-leg" { "L80" } else { "L50" };
        set_group_section(&mut doc, &t, g, Some(s));
    }
    let t = read(&doc);
    assert_eq!(t.detail(), Detail::Outline);
    for n in t.nodes.iter().filter(|n| n.point[2].si() == 0.0) {
        edits::set_support(&mut doc, &n.id, Some("pinned"));
    }
    let t = read(&doc);
    assert_eq!(t.detail(), Detail::Design);
    let leg = groups(&t)
        .into_iter()
        .find(|g| g.name == "body-leg")
        .unwrap();
    assert_eq!(
        (leg.members.len(), leg.section.as_deref()),
        (8, Some("L80"))
    );
    // A member put in a group takes its section.
    let t = read(&doc);
    edits::add_member(&mut doc, &t, "body-L0-1", "body-L1-3").unwrap();
    let t = read(&doc);
    let new = t.members.last().unwrap().id.clone();
    edits::set_member_section(&mut doc, &new, None);
    let t = read(&doc);
    set_group(
        &mut doc,
        &t,
        std::slice::from_ref(&new),
        Some("body-bracing-1"),
    );
    let t = read(&doc);
    assert_eq!(t.members.last().unwrap().section.as_deref(), Some("L50"));
}

/// The generator records read back as written; typos are refused.
#[test]
fn generator_records_round_trip() {
    let mut doc: DocumentMut = EMPTY.parse().unwrap();
    let g = frustum(&[(5.0, Bracing::K, false), (10.0, Bracing::Diamond, true)]);
    applied(&mut doc, &g);
    let arm = Generator {
        id: "arm".into(),
        kind: Kind::Crossarm(Crossarm {
            points: vec!["A1".into()],
            shape: ArmShape::Rectangular,
            ..Default::default()
        }),
    };
    body::set_generator(&mut doc, &arm);
    let t = read(&doc);
    assert_eq!(body::generators(&t).unwrap(), vec![g, arm]);
    let text = doc.to_string();
    assert!(
        text.contains("panel = [\n  { top = \"5 m\", bracing = \"k\", horizontal = false },"),
        "{text}"
    );
    for bad in [
        "[[generator]]\nid = \"x\"\nkind = \"tower\"\n",
        "[[generator]]\nid = \"x\"\nkind = \"peak\"\nheigth = \"3 m\"\n",
        "[[generator]]\nid = \"x\"\nkind = \"panels\"\nfeet = \"6 m\"\nwaist = \"2 m\"\npanel = [{ top = \"5 m\", bracing = \"z\" }]\n",
        "[[symmetry]]\nmirror = \"z\"\n",
        "[[symmetry]]\nmirror = \"x\"\ngroup = [\"a\"]\n",
    ] {
        let t = StructureType::parse(Path::new("g.ohls"), &format!("{EMPTY}{bad}")).unwrap();
        assert!(body::check(&t).is_err(), "{bad}");
        assert!(
            StructureFile::parse(&format!("{EMPTY}{bad}"), None).is_err(),
            "{bad}"
        );
    }
}

/// `frustum` with options on panel `k` (1-based).
fn with(mut g: Generator, k: usize, set: impl Fn(&mut Panel)) -> Generator {
    if let Kind::Panels(p) = &mut g.kind {
        set(&mut p.panels[k - 1]);
    }
    g
}

fn count(b: &Body, group: &str) -> usize {
    b.members
        .iter()
        .filter(|m| m.group.as_deref() == Some(group))
        .count()
}

/// X with one level of redundants: a joint at each face's crossing (the
/// diagonals cut there), braced to the legs' middles (the legs cut
/// there). One panel, 6 m → 2 m over 4 m: on the x+ face the diagonals
/// (3, −3, 0)–(1, 1, 4) and (3, 3, 0)–(1, −1, 4) cross at y = 0, a
/// quarter of the way down from the top: (1.5, 0, 3).
#[test]
fn x_with_one_level_of_redundants() {
    let t = read(&EMPTY.parse().unwrap());
    let g = with(frustum(&[(4.0, Bracing::X, true)]), 1, |p| {
        p.redundants = Redundants::One;
    });
    let b = generate(&t, &g).unwrap();
    // 8 corners, 4 leg middles, 4 crossings; 8 half legs, 4 horizontals,
    // 16 half diagonals, 8 redundants.
    assert_eq!((b.joints.len(), b.members.len()), (16, 36));
    assert_eq!(joint(&b, "body-R1-F1-C"), [1.5, 0.0, 3.0]);
    assert_eq!(joint(&b, "body-P1-4"), [2.0, -2.0, 2.0]);
    assert!(joined(&b, "body-L0-4", "body-R1-F1-C") && joined(&b, "body-R1-F1-C", "body-L1-1"));
    assert!(!joined(&b, "body-L0-4", "body-L1-1"));
    assert!(joined(&b, "body-R1-F1-C", "body-P1-4") && joined(&b, "body-R1-F1-C", "body-P1-1"));
    assert_eq!(count(&b, "body-redundant-1"), 8);
    assert_eq!(count(&b, "body-leg"), 8);
    assert_eq!(count(&b, "body-bracing-1"), 16);
    // The pieces are named after the member cut: the others keep their
    // names (the first leg is body-M1 without redundants).
    assert!(b.members.iter().any(|m| m.id == "body-M1-1"));
    assert!(b.members.iter().any(|m| m.id == "body-M1-2"));
    assert!(b.members.iter().all(|m| m.id != "body-M1"));
}

/// Two levels: single diagonals, K and diamond, counted by hand.
#[test]
fn two_levels_of_redundants() {
    let t = read(&EMPTY.parse().unwrap());
    let two = |bracing: Bracing, horizontal: bool, level: Redundants| {
        let g = with(frustum(&[(4.0, bracing, horizontal)]), 1, |p| {
            p.redundants = level;
        });
        let b = generate(&t, &g).unwrap();
        (
            b.joints.len(),
            b.members.len(),
            count(&b, "body-redundant-1"),
        )
    };
    // Single, two levels: per face the middle and the halves' middles (3
    // joints), the diagonal in 4 pieces, 6 redundants (the middle to both
    // legs' middles, a secondary diagonal in each quadrilateral, the
    // halves' middles to the legs' middles); legs in 2 pieces.
    assert_eq!(two(Bracing::Single, false, Redundants::Two), (24, 48, 24));
    // Single, one level: 1 joint, 2 pieces and 4 redundants per face.
    assert_eq!(two(Bracing::Single, false, Redundants::One), (16, 32, 16));
    // K, one level: the diagonals' middles (2 per face) braced to the leg
    // middle and the top corner on their side.
    assert_eq!(two(Bracing::K, false, Redundants::One), (24, 48, 16));
    // K, two levels: and the 4 halves' middles per face.
    assert_eq!(two(Bracing::K, false, Redundants::Two), (40, 80, 32));
    // Diamond, one level: each side's middle braced to the corner it
    // faces (4 per face).
    assert_eq!(two(Bracing::Diamond, true, Redundants::One), (36, 72, 16));
    // X, two levels: the crossing and the 4 halves' middles per face.
    assert_eq!(two(Bracing::X, true, Redundants::Two), (32, 68, 24));
}

/// Plan bracing: X across the plan (no joint at the centre), a diamond
/// between the horizontals' middles (cut there) with a cross to a joint
/// at the centre; at a level without
/// horizontals they are written too.
#[test]
fn plan_bracing() {
    let t = read(&EMPTY.parse().unwrap());
    let two = [(5.0, Bracing::X, true), (10.0, Bracing::X, true)];
    let x = generate(&t, &with(frustum(&two), 1, |p| p.plan = Plan::X)).unwrap();
    assert_eq!((x.joints.len(), x.members.len()), (12, 34));
    assert!(joined(&x, "body-L1-1", "body-L1-3") && joined(&x, "body-L1-2", "body-L1-4"));
    assert_eq!(count(&x, "body-plan-1"), 2);
    let d = generate(&t, &with(frustum(&two), 1, |p| p.plan = Plan::Diamond)).unwrap();
    assert_eq!((d.joints.len(), d.members.len()), (17, 44));
    assert_eq!(joint(&d, "body-L1-F2"), [0.0, 2.0, 5.0]);
    assert!(joined(&d, "body-L1-F1", "body-L1-F2") && joined(&d, "body-L1-F4", "body-L1-F1"));
    assert_eq!(count(&d, "body-horizontal-1"), 8);
    let bare = [(5.0, Bracing::X, false), (10.0, Bracing::X, true)];
    let f = generate(&t, &with(frustum(&bare), 1, |p| p.plan = Plan::X)).unwrap();
    assert_eq!(f.members.len(), 34);
    assert_eq!(count(&f, "body-horizontal-1"), 4);
    assert!(f.members.iter().any(|m| m.id == "body-H1-E1"));
}

/// A width per panel: the slope changes there (6 m → 3 m over the first
/// 4 m, then 3 m → 2 m over 8 m: 2.5 m at 8 m). `plan_auto` puts plan
/// bracing there and at the levels a crossarm starts from (a point at
/// x = 6 m, z = 9 m: the 8 m and 12 m levels).
#[test]
fn widths_per_panel_and_plan_bracing_where_needed() {
    let mut doc: DocumentMut = EMPTY.parse().unwrap();
    let mut g = with(
        frustum(&[
            (4.0, Bracing::X, true),
            (8.0, Bracing::X, true),
            (12.0, Bracing::X, true),
        ]),
        1,
        |p| p.width = Some([3.0, 3.0]),
    );
    let b = generate(&read(&doc), &g).unwrap();
    assert_eq!(joint(&b, "body-L1-1"), [1.5, 1.5, 4.0]);
    assert_eq!(joint(&b, "body-L2-1"), [1.25, 1.25, 8.0]);
    assert_eq!(count(&b, "body-plan-1"), 0);
    if let Kind::Panels(p) = &mut g.kind {
        p.plan_auto = Plan::X;
    }
    let b = generate(&read(&doc), &g).unwrap();
    assert_eq!(count(&b, "body-plan-1"), 2);
    assert_eq!(count(&b, "body-plan-2") + count(&b, "body-plan-3"), 0);
    let t = read(&doc);
    edits::add_point(&mut doc, &t, [6.0, 0.0, 9.0]);
    let arm = Generator {
        id: "arm".into(),
        kind: Kind::Crossarm(Crossarm {
            points: vec!["A1".into()],
            ..Default::default()
        }),
    };
    body::set_generator(&mut doc, &arm);
    let b = generate(&read(&doc), &g).unwrap();
    for (level, n) in [(1, 2), (2, 2), (3, 2)] {
        assert_eq!(count(&b, &format!("body-plan-{level}")), n, "{level}");
    }
    // A panel's own choice goes first.
    let g = with(g, 2, |p| p.plan = Plan::Diamond);
    let b = generate(&read(&doc), &g).unwrap();
    assert_eq!(count(&b, "body-plan-2"), 8);
    // Written only when on, read back.
    applied(&mut doc, &g);
    let t = read(&doc);
    assert_eq!(body::generators(&t).unwrap()[1], g);
    let text = doc.to_string();
    assert!(text.contains("width = \"3 m\""), "{text}");
    assert!(text.contains("plan_auto = \"x\""), "{text}");
    assert!(text.contains("plan = \"diamond\""), "{text}");
    assert!(!text.contains("redundants"), "{text}");
    // The last panel's width is the waist.
    let bad = with(g, 3, |p| p.width = Some([2.0, 2.0]));
    assert!(bad.check().is_err());
}

/// Turning redundants on cuts members in pieces: the pieces take what was
/// set on the member (its section); turned off again, the member takes
/// the first piece's.
#[test]
fn pieces_keep_what_was_set() {
    let mut doc: DocumentMut = EMPTY.parse().unwrap();
    let g = frustum(&[(4.0, Bracing::X, true)]);
    applied(&mut doc, &g);
    edits::add_item(
        &mut doc,
        "section",
        "L90",
        &[("material", "S235"), ("area", "15 cm2")],
    );
    edits::set_member_section(&mut doc, "body-M1", Some("L90"));
    let on = with(g.clone(), 1, |p| p.redundants = Redundants::One);
    applied(&mut doc, &on);
    let t = read(&doc);
    let section = |t: &StructureType, id: &str| {
        t.members
            .iter()
            .find(|m| m.id == id)
            .unwrap_or_else(|| panic!("no member {id}"))
            .section
            .clone()
    };
    assert_eq!(section(&t, "body-M1-1").as_deref(), Some("L90"));
    assert_eq!(section(&t, "body-M1-2").as_deref(), Some("L90"));
    assert_eq!(section(&t, "body-M2-1"), None);
    applied(&mut doc, &g);
    let t = read(&doc);
    assert_eq!(section(&t, "body-M1").as_deref(), Some("L90"));
    assert_eq!(t.members.len(), 16);
}

/// Several points on one arm. A body 6 m → 2 m over 10 m; points at
/// x = 6 m and 4 m, both at 7.5 m: one arm from the 5 m level (x = 2,
/// y = ±2) and the 10 m level (x = 1, y = ±1). Triangular: the chords
/// meet at the outer point; at x = 4 m the bottom chords are half-way
/// (4, ±1, 6.25), the top chords at 3/5 (4, ±0.4, 8.5).
#[test]
fn several_points_on_one_arm() {
    let mut doc: DocumentMut = EMPTY.parse().unwrap();
    applied(
        &mut doc,
        &frustum(&[(5.0, Bracing::X, true), (10.0, Bracing::X, true)]),
    );
    for at in [[6.0, 0.0, 7.5], [4.0, 0.0, 7.5]] {
        let t = read(&doc);
        edits::add_point(&mut doc, &t, at);
    }
    let t = read(&doc);
    let arm = |shape: ArmShape, one_arm: bool| Generator {
        id: "arm".into(),
        kind: Kind::Crossarm(Crossarm {
            points: vec!["A1".into(), "A2".into()],
            shape,
            one_arm,
        }),
    };
    // Without the option: an arm to each point.
    let each = generate(&t, &arm(ArmShape::Triangular, false)).unwrap();
    assert_eq!((each.joints.len(), each.members.len()), (2, 8));
    let b = generate(&t, &arm(ArmShape::Triangular, true)).unwrap();
    assert_eq!((b.joints.len(), b.members.len()), (6, 19));
    assert_eq!(joint(&b, "arm-A2-bottom-1"), [4.0, -1.0, 6.25]);
    assert_eq!(joint(&b, "arm-A2-top-2"), [4.0, 0.4, 8.5]);
    // The bottom chord in two pieces through the section.
    assert!(joined(&b, "body-L1-1", "arm-A2-bottom-2") && joined(&b, "arm-A2-bottom-2", "arm-A1"));
    assert!(!joined(&b, "body-L1-1", "arm-A1"));
    // The inner point braced to its section and back to the roots.
    for j in [
        "arm-A2-bottom-1",
        "arm-A2-bottom-2",
        "arm-A2-top-1",
        "arm-A2-top-2",
        "body-L1-1",
        "body-L1-4",
    ] {
        assert!(joined(&b, "arm-A2", j), "{j}");
    }
    assert_eq!(b.points.len(), 2);
    // Rectangular: the other chord to a tip 5 m above the outer point.
    let r = generate(&t, &arm(ArmShape::Rectangular, true)).unwrap();
    assert_eq!((r.joints.len(), r.members.len()), (7, 22));
    assert_eq!(joint(&r, "arm-A2-top-1"), [4.0, -0.4, 11.5]);
    assert!(joined(&r, "arm-A1", "arm-A1-top"));
    // Applied: both points on the arm's joints; read back.
    applied(&mut doc, &arm(ArmShape::Triangular, true));
    let t = read(&doc);
    assert_eq!(t.point_local("A2", None), Some([4.0, 0.0, 7.5]));
    assert!(doc.to_string().contains("one_arm = true"));
    // Two points as far out: refused.
    let t = read(&doc);
    edits::add_point(&mut doc, &t, [6.0, 0.5, 7.5]);
    let t = read(&doc);
    let mut three = arm(ArmShape::Triangular, true);
    if let Kind::Crossarm(c) = &mut three.kind {
        c.points.push("A3".into());
    }
    assert!(generate(&t, &three).is_err());
}

/// The options raise the file to version 6 (`body::OPTIONS_VERSION`),
/// and only they: a body without them stays at the body version 3, a
/// file without a version stays without; each option counts.
#[test]
fn options_raise_the_version() {
    let version = |doc: &DocumentMut| doc.get("version").and_then(|v| v.as_integer());
    let plain = frustum(&[(5.0, Bracing::X, true), (10.0, Bracing::X, true)]);
    let mut doc: DocumentMut = EMPTY.parse().unwrap();
    applied(&mut doc, &plain);
    assert_eq!(version(&doc), Some(body::BODY_VERSION));
    let redundants = with(plain.clone(), 1, |p| p.redundants = Redundants::One);
    applied(&mut doc, &redundants);
    assert_eq!(version(&doc), Some(body::OPTIONS_VERSION));
    assert_eq!(read(&doc).version, Some(6));
    // Written back without them: the version stays (it is never lowered).
    applied(&mut doc, &plain);
    assert_eq!(version(&doc), Some(body::OPTIONS_VERSION));
    let mut unversioned: DocumentMut = EMPTY.replace("version = 2\n", "").parse().unwrap();
    applied(&mut unversioned, &redundants);
    assert_eq!(version(&unversioned), None);
    let mut auto = plain.clone();
    if let Kind::Panels(p) = &mut auto.kind {
        p.plan_auto = Plan::X;
    }
    let arm = Generator {
        id: "arm".into(),
        kind: Kind::Crossarm(Crossarm {
            points: vec!["A1".into()],
            one_arm: true,
            ..Default::default()
        }),
    };
    for g in [
        &redundants,
        &with(plain.clone(), 1, |p| p.plan = Plan::Diamond),
        &with(plain.clone(), 1, |p| p.width = Some([4.0, 4.0])),
        &auto,
        &arm,
    ] {
        assert!(g.uses_options(), "{g:?}");
    }
    assert!(!plain.uses_options());
}
