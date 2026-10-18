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
                })
                .collect(),
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
