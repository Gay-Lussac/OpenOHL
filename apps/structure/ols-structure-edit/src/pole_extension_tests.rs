// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The pole's edits (outline rows, shape, design fields, a `.olsp`
//! brought in) and the extensions' (joints, members, the piece, legs):
//! each one undo step, comments kept, and what each level lacks.

use std::path::Path;

use ols_structures::{Detail, PoleRef, PoleShape};

use super::*;
use crate::extension_edits::{self as ext, PieceBracing};
use crate::pole::{self as pole_edit};

const POLE: &str = "# A pole carrying one wire (comment kept).\n\
     format = \"ols-structure\"\nversion = 2\nid = \"P\"\nname = \"p\"\nkind = \"pole\"\n\
     height = \"27 m\"\n\
     [[point]]\nlabel = \"A1\"\npoint = [\"0 m\", \"0 m\", \"26 m\"]\n";

fn rings(f: &StructureFile) -> Vec<(f64, f64)> {
    f.structure()
        .pole
        .as_ref()
        .and_then(PoleRef::inline)
        .map(|p| p.rings())
        .unwrap_or_default()
}

fn near(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
}

/// Level 1 → 2 by an outline, 2 → 3 by the design fields; each edit one
/// undo step; a bad row refused.
#[test]
fn a_pole_is_outlined_and_designed() {
    let mut f = StructureFile::parse(POLE, None).unwrap();
    assert_eq!(f.structure().detail(), Detail::Stick);
    let lacks = missing(f.structure(), 2);
    assert!(
        lacks[0].reason.starts_with("no member"),
        "{lacks:?} (no pole yet)"
    );
    f.edit("pole outline", pole_edit::add_outline_row).unwrap();
    assert!(near(rings(&f)[0], (0.0, 0.4)) && near(rings(&f)[1], (27.0, 0.2)));
    assert_eq!(f.structure().detail(), Detail::Outline);
    let text = f.text();
    assert!(text.starts_with("# A pole carrying one wire"), "{text}");
    assert!(text.contains("version = 3"), "{text}");
    // Level 3 lacks the material and the strength.
    let lacks = missing(f.structure(), 3);
    let pole_items: Vec<&str> = lacks
        .iter()
        .filter(|m| m.place == Place::Pole && m.need == Need::Level)
        .map(|m| m.reason.as_str())
        .collect();
    assert_eq!(pole_items.len(), 2, "{lacks:?}");
    // A row in the middle, then its diameter.
    let row = f.edit("pole row", pole_edit::add_outline_row).unwrap();
    assert_eq!(row, 1);
    assert!(near(rings(&f)[1], (13.5, 0.3)));
    f.edit("pole row", |doc, t| {
        pole_edit::set_outline_row(doc, t, 1, 12.0, 0.32)
    })
    .unwrap();
    assert!(near(rings(&f)[1], (12.0, 0.32)));
    let before = f.text();
    let e = f
        .edit("pole row", |doc, t| {
            pole_edit::set_outline_row(doc, t, 1, 30.0, 0.32)
        })
        .unwrap_err();
    assert!(e.contains("rise"), "{e}");
    assert_eq!(f.text(), before);
    // The design: steel, its wall, its capacity.
    for (k, v) in [
        ("material", "steel"),
        ("moment_capacity", "900 kN m"),
        ("wall", "8 mm"),
    ] {
        f.edit(&format!("pole {k}"), |doc, _| {
            pole_edit::set_field(doc, k, v)
        })
        .unwrap();
    }
    assert_eq!(f.structure().detail(), Detail::Design);
    assert!(missing(f.structure(), 3).is_empty());
    // One undo: the wall goes, level 2 again.
    assert_eq!(f.undo().as_deref(), Some("pole wall"));
    assert_eq!(f.structure().detail(), Detail::Outline);
    f.redo();
    assert!(pole_edit::set_field(&mut f.doc().clone(), "colour", "red").is_err());
    // Polygonal: 12 sides; the wireframe has a ring per height and a
    // line per corner.
    f.edit("pole shape", |doc, _| {
        pole_edit::set_shape(doc, PoleShape::Polygon, 12)
    });
    let p = f
        .structure()
        .pole
        .as_ref()
        .and_then(PoleRef::inline)
        .unwrap();
    assert_eq!(p.polygon_sides(), Some(12));
    assert_eq!(geometry::pole_wireframe(f.structure()).len(), 3 + 12);
    f.edit("pole row", |doc, _| pole_edit::remove_outline_row(doc, 1));
    assert_eq!(rings(&f).len(), 2);
    f.edit("no pole", |doc, _| pole_edit::remove_pole(doc));
    assert!(f.structure().pole.is_none());
}

/// A named `.olsp` brought into the file: two heights from the feet, the
/// same fields; level 3 before and after.
#[test]
fn a_pole_file_is_brought_in() {
    let text = POLE.replace("height", "pole = \"steel-pole-30m\"\nheight");
    let mut f = StructureFile::parse(&text, None).unwrap();
    assert_eq!(f.structure().detail(), Detail::Design);
    let e = f.edit("pole row", pole_edit::add_outline_row).unwrap_err();
    assert!(e.contains("bring it into this file"), "{e}");
    let olsp = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../examples/synthetic-line/Structures/steel-pole-30m.olsp"),
    )
    .unwrap();
    f.edit("bring the pole in", |doc, t| {
        pole_edit::bring_in(doc, t, &olsp)
    })
    .unwrap();
    assert!(near(rings(&f)[0], (0.0, 0.7)) && near(rings(&f)[1], (27.0, 0.3)));
    let p = f
        .structure()
        .pole
        .as_ref()
        .and_then(PoleRef::inline)
        .unwrap();
    assert_eq!(p.embedment.map(|e| e.si()), Some(3.0));
    assert_eq!(p.capacity.len(), 3);
    assert_eq!(p.drag, Some(0.8));
    assert_eq!(f.structure().detail(), Detail::Design);
    assert_eq!(f.undo().as_deref(), Some("bring the pole in"));
    assert!(matches!(f.structure().pole, Some(PoleRef::File(_))));
}

/// A four-legged body on supported feet (z = 0).
const TOWER: &str = "format = \"ols-structure\"\nversion = 3\nid = \"T\"\nname = \"t\"\n\
     kind = \"lattice\"\n\
     [[node]]\nid = \"F1\"\npoint = [\"2 m\", \"2 m\", \"0 m\"]\nsupport = \"pinned\"\n\
     [[node]]\nid = \"F2\"\npoint = [\"-2 m\", \"2 m\", \"0 m\"]\nsupport = \"pinned\"\n\
     [[node]]\nid = \"F3\"\npoint = [\"-2 m\", \"-2 m\", \"0 m\"]\nsupport = \"pinned\"\n\
     [[node]]\nid = \"F4\"\npoint = [\"2 m\", \"-2 m\", \"0 m\"]\nsupport = \"pinned\"\n\
     [[node]]\nid = \"T1\"\npoint = [\"1 m\", \"1 m\", \"10 m\"]\n\
     [[node]]\nid = \"T2\"\npoint = [\"-1 m\", \"1 m\", \"10 m\"]\n\
     [[node]]\nid = \"T3\"\npoint = [\"-1 m\", \"-1 m\", \"10 m\"]\n\
     [[node]]\nid = \"T4\"\npoint = [\"1 m\", \"-1 m\", \"10 m\"]\n\
     [[member]]\nid = \"L1\"\nnodes = [\"F1\", \"T1\"]\nsection = \"S\"\n\
     [[member]]\nid = \"L2\"\nnodes = [\"F2\", \"T2\"]\nsection = \"S\"\n\
     [[member]]\nid = \"L3\"\nnodes = [\"F3\", \"T3\"]\nsection = \"S\"\n\
     [[member]]\nid = \"L4\"\nnodes = [\"F4\", \"T4\"]\nsection = \"S\"\n\
     [[extension]]\nname = \"+0\"\ndz = \"0 m\"\n";

/// The piece built in one go under the feet, by hand, legs; each one
/// undo step; what level 3 lists.
#[test]
fn an_extension_gets_a_body_and_legs() {
    let mut f = StructureFile::parse(TOWER, None).unwrap();
    assert_eq!(f.structure().detail(), Detail::Design);
    // An extension without members: advice at level 3.
    let advice: Vec<String> = missing(f.structure(), 3)
        .into_iter()
        .filter(|m| m.place == Place::Extensions)
        .map(|m| m.reason)
        .collect();
    assert_eq!(
        advice,
        ["extension +0 has no members: the FE model leaves it out"]
    );
    f.edit("add extension", |doc, _| {
        edits::set_extension(doc, "+2", 2.0)
    });
    let e = f
        .edit("piece", |doc, t| {
            ext::build_piece(doc, t, "+0", PieceBracing::X, false)
        })
        .unwrap_err();
    assert!(e.contains("dz above 0"), "{e}");
    let piece = f
        .edit("piece", |doc, t| {
            ext::build_piece(doc, t, "+2", PieceBracing::X, true)
        })
        .unwrap();
    assert_eq!((piece.joints, piece.members), (4, 4 + 8 + 4));
    let t = f.structure().clone();
    let e2 = t.extension("+2").unwrap();
    assert_eq!(e2.nodes.len(), 4);
    // Under F1, along its leg (to T1: 1 m out per 10 m down), 2 m lower.
    let j = e2
        .nodes
        .iter()
        .find(|n| {
            let p = n.point.map(|v| v.si());
            (p[0] - 2.2).abs() < 1e-9 && (p[1] - 2.2).abs() < 1e-9 && (p[2] + 2.0).abs() < 1e-9
        })
        .expect("a joint under F1");
    assert_eq!(j.support.as_ref().and_then(|v| v.as_str()), Some("pinned"));
    assert!(e2.members.iter().all(|m| m.section.as_deref() == Some("S")));
    assert_eq!(t.detail(), Detail::Design);
    let x = t.extended(Some("+2")).unwrap();
    assert_eq!(x.unsupported.len(), 4);
    // By hand: a joint (refused above the feet), a member, removed again.
    let e = f
        .edit("joint", |doc, t| {
            ext::add_joint(doc, t, "+2", [0.0, 0.0, 1.0])
        })
        .unwrap_err();
    assert!(e.contains("below the feet"), "{e}");
    let id = f
        .edit("joint", |doc, t| {
            ext::add_joint(doc, t, "+2", [0.0, 3.0, -1.0])
        })
        .unwrap();
    assert_eq!(id, "X2-J5");
    let m = f
        .edit("member", |doc, t| ext::add_member(doc, t, "+2", &id, "F1"))
        .unwrap();
    assert_eq!(m, "X2-M17");
    assert!(
        f.edit("member", |doc, t| ext::add_member(doc, t, "+2", "F1", "F2"))
            .is_err()
    );
    // No support on the new joint: it is not a support, still level 3.
    assert_eq!(f.structure().detail(), Detail::Design);
    let gone = f.edit("delete joint", |doc, t| {
        ext::remove_joint(doc, t, "+2", &id)
    });
    assert_eq!(gone, ["X2-M17"]);
    // A member without a section: level 3 lacks it.
    f.edit("section", |doc, _| {
        ext::set_field(doc, "+2", "member", "X2-M1", "section", None)
    })
    .unwrap();
    assert_eq!(f.structure().detail(), Detail::Outline);
    assert!(
        missing(f.structure(), 3)
            .iter()
            .any(|m| m.reason == "extension +2: member X2-M1 has no section")
    );
    f.undo();
    // Leg A one metre longer, then no longer.
    f.edit("leg", |doc, _| ext::set_leg(doc, "+2", "A", Some(1.0)))
        .unwrap();
    let t = f.structure().clone();
    assert_eq!(t.extension("+2").unwrap().legs.len(), 1);
    let x = t.extended(Some("+2")).unwrap();
    assert_eq!(x.lowered.len(), 1);
    let g = geometry_with(&t, &Default::default(), Some("+2"));
    assert_eq!(g.joints.len(), 12);
    assert_eq!(g.extension.len(), 4 + 16);
    f.edit("leg", |doc, _| ext::set_leg(doc, "+2", "A", None))
        .unwrap();
    assert!(!f.text().contains("[[extension.leg]]"));
    assert_eq!(f.undo().as_deref(), Some("leg"));
    assert!(f.text().contains("[[extension.leg]]"));
}
