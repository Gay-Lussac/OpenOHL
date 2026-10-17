// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Poles with levels (`[pole]`) and extension bodies and legs.

use std::path::Path;

use crate::{Detail, LibraryItem, PoleRef, StructureType};

fn parse(body: &str) -> Result<StructureType, String> {
    let text = format!(
        "version = 3\nid = \"P\"\nname = \"p\"\nkind = \"pole\"\nheight = \"20 m\"\n{body}"
    );
    StructureType::parse(Path::new("p.olss"), &text).map_err(|e| e.message)
}

const RINGS: &str = "[[pole.outline]]\nz = \"0 m\"\ndiameter = \"500 mm\"\n\
     [[pole.outline]]\nz = \"20 m\"\ndiameter = \"250 mm\"\n";

/// A `[pole]` with `fields` and the outline.
fn with(fields: &str) -> String {
    format!("[pole]\n{fields}{RINGS}")
}

/// Level 1 without an outline, 2 with diameters at two heights, 3 with
/// what the pole check needs; a named `.olsp` is level 3.
#[test]
fn pole_levels() {
    let t = parse("[pole]\nshape = \"round\"\n").unwrap();
    assert_eq!(t.detail(), Detail::Stick);
    assert_eq!(t.pole_detail(), Some(Detail::Stick));
    let one = parse("[pole]\n[[pole.outline]]\nz = \"0 m\"\ndiameter = \"500 mm\"\n").unwrap();
    assert_eq!(one.detail(), Detail::Stick);
    let t = parse(&with("")).unwrap();
    assert_eq!(t.detail(), Detail::Outline);
    let p = t.pole.as_ref().and_then(PoleRef::inline).unwrap();
    assert_eq!(p.diameter_at(10.0), Some(0.375));
    assert_eq!(p.diameter_at(-1.0), Some(0.5));
    assert_eq!(
        p.missing_for_design(),
        [
            "pole: no material",
            "pole: no strength (fibre stress, moment capacity or a capacity table)"
        ]
    );
    // Steel: its wall too.
    let steel = parse(&with(
        "material = \"steel\"\nmoment_capacity = \"300 kN m\"\n",
    ))
    .unwrap();
    assert_eq!(steel.detail(), Detail::Outline);
    assert_eq!(
        steel
            .pole
            .as_ref()
            .and_then(PoleRef::inline)
            .unwrap()
            .missing_for_design(),
        ["pole: no wall (a steel pole: its wall, or a section)"]
    );
    let full = parse(&with(
        "material = \"steel\"\nmoment_capacity = \"300 kN m\"\nwall = \"6 mm\"\n",
    ))
    .unwrap();
    assert_eq!(full.detail(), Detail::Design);
    // A wood pole is solid.
    let wood = parse(&with("material = \"wood\"\nfibre_stress = \"50 MPa\"\n")).unwrap();
    assert_eq!(wood.detail(), Detail::Design);
    // A section from the library stands for the wall.
    let section = parse(&with(
        "material = \"steel\"\nmoment_capacity = \"300 kN m\"\nsection = \"P-12\"\n",
    ))
    .unwrap();
    assert_eq!(section.detail(), Detail::Design);
    let named = parse("pole = \"steel-pole-30m\"\n").unwrap();
    assert_eq!(named.detail(), Detail::Design);
    assert_eq!(named.pole, Some(PoleRef::File("steel-pole-30m".into())));
    // No members: no finite-element model to build.
    assert!(!full.members_designed());
}

/// Polygons, circumferences, and what is refused.
#[test]
fn pole_tables_are_checked() {
    let t = parse(
        "[pole]\nshape = \"polygon\"\nsides = 12\n\
         [[pole.outline]]\nz = \"0 m\"\ncircumference = \"1.2 m\"\n\
         [[pole.outline]]\nz = \"10 m\"\ndiameter = \"200 mm\"\n",
    )
    .unwrap();
    let p = t.pole.as_ref().and_then(PoleRef::inline).unwrap();
    // Across flats from the perimeter: 12 sides of 2 r tan 15°.
    let d = p.rings()[0].1;
    assert!((12.0 * d * (std::f64::consts::PI / 12.0).tan() - 1.2).abs() < 1e-12);
    assert_eq!(p.polygon_sides(), Some(12));
    for (bad, why) in [
        ("[pole]\nshape = \"polygon\"\n", "sides"),
        ("[pole]\nsides = 8\n", "only with"),
        (
            "[pole]\n[[pole.outline]]\nz = \"0 m\"\n",
            "diameter or its circumference",
        ),
        (
            "[pole]\n[[pole.outline]]\nz = \"5 m\"\ndiameter = \"1 m\"\n\
             [[pole.outline]]\nz = \"5 m\"\ndiameter = \"1 m\"\n",
            "rise",
        ),
        ("[pole]\nmaterial = \"glass\"\n", "glass"),
        ("[pole]\nwall = \"-1 mm\"\n", "above 0"),
        ("[pole]\nwal = \"6 mm\"\n", "wal"),
        ("pole = 3\n", "[pole] table"),
    ] {
        let e = parse(bad).unwrap_err();
        assert!(e.contains(why), "{bad}: {e}");
    }
}

/// A tower of four legs (feet at z = 0, supported), with an extension
/// "+2": a piece of four joints 2 m lower and its legs, and leg A 1 m
/// longer.
pub(crate) const TOWER: &str = r#"
[[node]]
id = "F1"
point = ["2 m", "2 m", "0 m"]
support = "pinned"
[[node]]
id = "F2"
point = ["-2 m", "2 m", "0 m"]
support = "pinned"
[[node]]
id = "F3"
point = ["-2 m", "-2 m", "0 m"]
support = "pinned"
[[node]]
id = "F4"
point = ["2 m", "-2 m", "0 m"]
support = "pinned"
[[node]]
id = "T"
point = ["0 m", "0 m", "10 m"]
[[member]]
id = "L1"
nodes = ["F1", "T"]
[[member]]
id = "L2"
nodes = ["F2", "T"]
[[member]]
id = "L3"
nodes = ["F3", "T"]
[[member]]
id = "L4"
nodes = ["F4", "T"]
[[member]]
id = "H1"
nodes = ["F1", "F2"]

[[extension]]
name = "+0"
dz = "0 m"

[[extension]]
name = "+2"
dz = "2 m"
[[extension.node]]
id = "E1"
point = ["2.4 m", "2.4 m", "-2 m"]
support = "pinned"
[[extension.node]]
id = "E2"
point = ["-2.4 m", "2.4 m", "-2 m"]
support = "pinned"
[[extension.node]]
id = "E3"
point = ["-2.4 m", "-2.4 m", "-2 m"]
support = "pinned"
[[extension.node]]
id = "E4"
point = ["2.4 m", "-2.4 m", "-2 m"]
support = "pinned"
[[extension.member]]
id = "EL1"
nodes = ["E1", "F1"]
[[extension.member]]
id = "EL2"
nodes = ["E2", "F2"]
[[extension.member]]
id = "EL3"
nodes = ["E3", "F3"]
[[extension.member]]
id = "EL4"
nodes = ["E4", "F4"]
[[extension.leg]]
leg = "A"
dz = "1 m"

[[extension]]
name = "legs"
dz = "0 m"
[[extension.leg]]
leg = "F3"
dz = "0.5 m"
"#;

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() < 1e-9)
}

/// The extension's joints and members join the body; the feet stand on
/// it (their supports dropped); leg A slides 1 m lower along its leg.
#[test]
fn extension_bodies_and_legs() {
    let t = parse(TOWER).unwrap();
    let plain = t.extended(Some("+0")).unwrap();
    assert_eq!((plain.nodes.len(), plain.members.len()), (5, 5));
    assert!(plain.unsupported.is_empty() && plain.lowered.is_empty());
    let x = t.extended(Some("+2")).unwrap();
    assert_eq!((x.nodes.len(), x.members.len()), (9, 9));
    assert_eq!(x.unsupported, ["F1", "F2", "F3", "F4"]);
    assert!(x.added.contains("E1") && x.added.contains("EL4"));
    assert_eq!(x.lowered, [("E1".to_owned(), "A".to_owned())]);
    let at = |id: &str| {
        x.nodes
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.point.map(|v| v.si()))
            .unwrap()
    };
    // Along E1 → F1 produced: (0.4, 0.4, −2) per 2 m down.
    assert!(near(at("E1"), [2.6, 2.6, -3.0]), "{:?}", at("E1"));
    assert!(near(at("E3"), [-2.4, -2.4, -2.0]));
    // A leg by joint id, no body: F3 slides along L3 (to T).
    let y = t.extended(Some("legs")).unwrap();
    let f3 = y.nodes.iter().find(|n| n.id == "F3").unwrap();
    assert!(near(f3.point.map(|v| v.si()), [-2.1, -2.1, -0.5]));
    assert!(t.extended(Some("+9")).is_err());
    // Drawn with the piece: everything rises by 2 m, the piece from the
    // ground (leg A 1 m below it).
    let lines = t.stick(Some("+2")).unwrap();
    assert_eq!(lines.len(), 9);
    assert!(lines.iter().any(|l| near(l[0], [2.6, 2.6, -1.0])));
    assert!(lines.iter().any(|l| near(l[1], [0.0, 0.0, 12.0])));
    // Without the extension: as written.
    assert_eq!(t.stick(None).unwrap().len(), 5);
    // Members designed: the extension's members need sections too.
    let designed = TOWER.replace("nodes = [", "section = \"S\"\nnodes = [");
    assert!(parse(&designed).unwrap().members_designed());
    let bare = designed.replacen("section = \"S\"\nnodes = [\"E1\"", "nodes = [\"E1\"", 1);
    let t = parse(&bare).unwrap();
    assert_eq!(
        t.missing_for_design(),
        ["extension +2: 1 member(s) without a section"]
    );
    assert_eq!(t.detail(), Detail::Outline);
}

/// What is refused in an extension.
#[test]
fn extensions_are_checked() {
    for (bad, why) in [
        (
            "[[extension]]\nname = \"x\"\ndz = \"2 m\"\n[[extension.node]]\nid = \"E\"\npoint = [\"0 m\", \"0 m\", \"1 m\"]\n",
            "below the feet",
        ),
        (
            "[[extension]]\nname = \"x\"\ndz = \"2 m\"\n[[extension.node]]\nid = \"E\"\npoint = [\"0 m\", \"0 m\", \"-3 m\"]\n",
            "below the feet",
        ),
        (
            "[[node]]\nid = \"E\"\npoint = [\"0 m\", \"0 m\", \"0 m\"]\n\
             [[extension]]\nname = \"x\"\ndz = \"2 m\"\n[[extension.node]]\nid = \"E\"\npoint = [\"0 m\", \"0 m\", \"-1 m\"]\n",
            "given twice",
        ),
        (
            "[[extension]]\nname = \"x\"\ndz = \"2 m\"\n[[extension.member]]\nid = \"M\"\nnodes = [\"a\", \"b\"]\n",
            "no joint",
        ),
        (
            "[[extension]]\nname = \"x\"\ndz = \"0 m\"\n[[extension.node]]\nid = \"E\"\npoint = [\"0 m\", \"0 m\", \"0 m\"]\n",
            "dz above 0",
        ),
        (
            "[[extension]]\nname = \"x\"\ndz = \"1 m\"\n[[extension.leg]]\nleg = \"Q\"\ndz = \"1 m\"\n",
            "A, B, C, D",
        ),
        (
            "[[extension]]\nname = \"x\"\ndz = \"1 m\"\n[[extension.leg]]\nleg = \"A\"\ndz = \"0 m\"\n",
            "dz above 0",
        ),
        (
            "[[extension]]\nname = \"x\"\ndz = \"1 m\"\n[[extension.leg]]\nleg = \"A\"\ndz = \"1 m\"\n[[extension.leg]]\nleg = \"A\"\ndz = \"2 m\"\n",
            "given twice",
        ),
    ] {
        let e = parse(bad).unwrap_err();
        assert!(e.contains(why), "{bad}: {e}");
    }
    // Two equally low joints in a quarter: the leg names its joint.
    let t = parse(
        "[[node]]\nid = \"a\"\npoint = [\"1 m\", \"1 m\", \"0 m\"]\n\
         [[node]]\nid = \"b\"\npoint = [\"2 m\", \"1 m\", \"0 m\"]\n\
         [[extension]]\nname = \"x\"\ndz = \"1 m\"\n[[extension.leg]]\nleg = \"A\"\ndz = \"1 m\"\n",
    )
    .unwrap();
    let e = t.extended(Some("x")).unwrap_err();
    assert!(e.contains("name the joint"), "{e}");
}
