// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A pole described in the structure file checked as its `.olsp`; poles
//! by height and polygonal; the model of a structure standing on an
//! extension (its piece, its supports, a leg lowered).

use std::path::Path;

use ols_sections::SectionLibrary;
use ols_structures::{LibraryItem, StructureType};

use crate::pole::structure_pole;
use crate::{Loads, Model, Options, Pole, PoleLoad, check_pole, default_budget, solve};

fn example(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/synthetic-line/Structures")
        .join(name)
}

/// The example steel pole written in the structure file (`[pole]`, the
/// outline from the feet to 27 m) is the `.olsp` pole: the same values,
/// the same check, to the last digit, with wind and P-Delta.
#[test]
fn inline_pole_checks_as_its_olsp() {
    let olsp = Pole::load(&example("steel-pole-30m.olsp")).unwrap();
    let text = "id = \"type-05\"\nname = \"pole\"\nkind = \"pole\"\nversion = 3\n\
         [[point]]\nlabel = \"A1\"\npoint = [\"0 m\", \"0 m\", \"26 m\"]\n\
         [pole]\nembedment = \"3 m\"\nmaterial = \"steel\"\nwall = \"8 mm\"\n\
         elasticity = \"210 GPa\"\ndensity = \"7850 kg/m3\"\ndrag = 0.8\n\
         [[pole.outline]]\nz = \"0 m\"\ndiameter = \"700 mm\"\n\
         [[pole.outline]]\nz = \"27 m\"\ndiameter = \"300 mm\"\n\
         [[pole.capacity]]\nheight = \"0 m\"\nmoment = \"900 kN m\"\n\
         [[pole.capacity]]\nheight = \"13 m\"\nmoment = \"450 kN m\"\n\
         [[pole.capacity]]\nheight = \"27 m\"\nmoment = \"120 kN m\"\n";
    let t = StructureType::parse(Path::new("type-05.olss"), text).unwrap();
    let inline = structure_pole(&t, Path::new("."), &SectionLibrary::default())
        .unwrap()
        .unwrap();
    let same = Pole {
        id: olsp.id.clone(),
        name: olsp.name.clone(),
        source: olsp.source.clone(),
        ..inline.clone()
    };
    assert_eq!(same, olsp);
    let loads = [PoleLoad {
        point: t.point_local("A1", None).unwrap(),
        force: [6e3, 2e3, -9e3],
    }];
    for (wind, p_delta) in [([0.0, 0.0], false), ([-900.0, 0.0], true)] {
        let a = check_pole(&olsp, &loads, wind, 0.9, p_delta, &default_budget()).unwrap();
        let b = check_pole(&inline, &loads, wind, 0.9, p_delta, &default_budget()).unwrap();
        assert_eq!(a.usage, b.usage);
        assert_eq!(a.moment, b.moment);
        assert_eq!(a.amplification, b.amplification);
    }
    // Named by id next to the structure, or by file name: the same pole.
    let named = StructureType::parse(
        Path::new("t.olss"),
        "id = \"t\"\nname = \"t\"\nkind = \"pole\"\npole = \"steel-pole-30m\"\n",
    )
    .unwrap();
    let dir = example("");
    assert_eq!(
        structure_pole(&named, &dir, &SectionLibrary::default()).unwrap(),
        Ok(olsp.clone())
    );
    let by_file = StructureType::parse(
        Path::new("t.olss"),
        "id = \"t\"\nname = \"t\"\nkind = \"pole\"\npole = \"steel-pole-30m.olsp\"\n",
    )
    .unwrap();
    assert_eq!(
        structure_pole(&by_file, &dir, &SectionLibrary::default()).unwrap(),
        Ok(olsp)
    );
    // Not complete: said why.
    let outline_only = text.replace("material = \"steel\"\n", "");
    let t = StructureType::parse(Path::new("t.olss"), &outline_only).unwrap();
    let e = structure_pole(&t, Path::new("."), &SectionLibrary::default())
        .unwrap()
        .unwrap_err();
    assert!(e.contains("no material"), "{e}");
}

/// `wall = "solid"` is the solid pole a `.olsp` without a wall always was:
/// the same pole and the same check to the last digit, in a `.olsp` file
/// and in `[pole]`; solid wins over a section's wall (its material still
/// gives the elasticity and density).
#[test]
fn a_solid_wall_checks_as_no_wall() {
    let olsp = |wall: &str| {
        Pole::parse(&format!(
            "format = \"ols-pole\"\nversion = 1\nid = \"w\"\nmaterial = \"wood\"\n\
             length = \"14 m\"\nembedment = \"2 m\"\ntop_diameter = \"190 mm\"\n\
             ground_diameter = \"290 mm\"\nfibre_stress = \"55 MPa\"\n\
             elasticity = \"11 GPa\"\ndensity = \"650 kg/m3\"\n{wall}"
        ))
    };
    let none = olsp("").unwrap();
    let solid = olsp("wall = \"solid\"\n").unwrap();
    assert_eq!(none.wall, None);
    assert_eq!(solid, none);
    let e = olsp("wall = \"hollow\"\n").unwrap_err();
    assert!(e.contains("wall: \"hollow\""), "{e}");
    let structure = |extra: &str| {
        format!(
            "id = \"w\"\nname = \"w\"\nkind = \"pole\"\nversion = 3\n\
             [[point]]\nlabel = \"A1\"\npoint = [\"0 m\", \"0 m\", \"11.5 m\"]\n\
             [pole]\nembedment = \"2 m\"\nmaterial = \"wood\"\nwall = \"solid\"\n\
             fibre_stress = \"55 MPa\"\nelasticity = \"11 GPa\"\ndensity = \"650 kg/m3\"\n{extra}\
             [[pole.outline]]\nz = \"0 m\"\ndiameter = \"290 mm\"\n\
             [[pole.outline]]\nz = \"12 m\"\ndiameter = \"190 mm\"\n"
        )
    };
    let t = StructureType::parse(Path::new("w.olss"), &structure("")).unwrap();
    let inline = structure_pole(&t, Path::new("."), &SectionLibrary::default())
        .unwrap()
        .unwrap();
    assert_eq!(
        Pole {
            name: none.name.clone(),
            ..inline.clone()
        },
        none
    );
    let loads = [PoleLoad {
        point: t.point_local("A1", None).unwrap(),
        force: [3e3, 1e3, -5e3],
    }];
    for (wind, p_delta) in [([0.0, 0.0], false), ([-700.0, 0.0], true)] {
        let a = check_pole(&none, &loads, wind, 0.9, p_delta, &default_budget()).unwrap();
        let b = check_pole(&solid, &loads, wind, 0.9, p_delta, &default_budget()).unwrap();
        let c = check_pole(&inline, &loads, wind, 0.9, p_delta, &default_budget()).unwrap();
        for r in [&b, &c] {
            assert_eq!(a.usage, r.usage);
            assert_eq!(a.moment, r.moment);
            assert_eq!(a.amplification, r.amplification);
        }
    }
    // Solid with a section: solid, the section's material for the rest.
    let lib = SectionLibrary::parse(
        "[[material]]\nid = \"m\"\nelasticity = \"12 GPa\"\ndensity = \"700 kg/m3\"\n\
         [[section]]\nid = \"C\"\nshape = \"circular_hollow\"\nmaterial = \"m\"\n\
         d = \"290 mm\"\nt = \"20 mm\"\n",
    )
    .unwrap();
    let text = structure("section = \"C\"\n")
        .replace("elasticity = \"11 GPa\"\ndensity = \"650 kg/m3\"\n", "");
    let t = StructureType::parse(Path::new("w.olss"), &text).unwrap();
    let p = structure_pole(&t, Path::new("."), &lib).unwrap().unwrap();
    assert_eq!((p.wall, p.elasticity, p.density), (None, Some(12e9), 700.0));
}

/// Three heights: the diameter by height; a polygonal pole's section
/// as the section library computes a polygonal hollow section; its wall
/// from a section of the library.
#[test]
fn poles_by_height_and_polygonal() {
    let lib = SectionLibrary::parse(
        "[[material]]\nid = \"steel\"\nelasticity = \"200 GPa\"\ndensity = \"7800 kg/m3\"\n\
         [[section]]\nid = \"P12\"\nshape = \"polygonal_hollow\"\nmaterial = \"steel\"\n\
         sides = 12\nd = \"500 mm\"\nt = \"6 mm\"\n\
         [[section]]\nid = \"P8\"\nshape = \"polygonal_hollow\"\nmaterial = \"steel\"\n\
         sides = 8\nd = \"500 mm\"\nt = \"6 mm\"\n",
    )
    .unwrap();
    let text = |section: &str| {
        format!(
            "id = \"p\"\nname = \"p\"\nkind = \"pole\"\n\
             [pole]\nshape = \"polygon\"\nsides = 12\nmaterial = \"steel\"\n\
             moment_capacity = \"500 kN m\"\nsection = \"{section}\"\n\
             [[pole.outline]]\nz = \"0 m\"\ndiameter = \"600 mm\"\n\
             [[pole.outline]]\nz = \"10 m\"\ndiameter = \"500 mm\"\n\
             [[pole.outline]]\nz = \"20 m\"\ndiameter = \"200 mm\"\n"
        )
    };
    let t = StructureType::parse(Path::new("p.olss"), &text("P12")).unwrap();
    let p = structure_pole(&t, Path::new("."), &lib).unwrap().unwrap();
    assert_eq!(p.sides, Some(12));
    assert_eq!(
        (p.wall, p.elasticity, p.density),
        (Some(0.006), Some(200e9), 7800.0)
    );
    assert!((p.diameter_at(5.0) - 0.55).abs() < 1e-12);
    assert!((p.diameter_at(15.0) - 0.35).abs() < 1e-12);
    assert_eq!(p.diameter_at(25.0), 0.2);
    // The same section as the library's at 500 mm.
    let s = lib.section("P12").unwrap().properties();
    assert!(
        (p.area(0.5) - s.area).abs() < 1e-9 * s.area,
        "{} {}",
        p.area(0.5),
        s.area
    );
    assert!(
        (p.second_moment(0.5) - s.iy).abs() < 1e-9 * s.iy,
        "{} {}",
        p.second_moment(0.5),
        s.iy
    );
    // A section of another shape is refused.
    let t = StructureType::parse(Path::new("p.olss"), &text("P8")).unwrap();
    let e = structure_pole(&t, Path::new("."), &lib)
        .unwrap()
        .unwrap_err();
    assert!(
        e.contains("not a hollow section of the pole's shape"),
        "{e}"
    );
    let t = StructureType::parse(Path::new("p.olss"), &text("P99")).unwrap();
    assert!(structure_pole(&t, Path::new("."), &lib).unwrap().is_err());
}

/// A frame of four legs to an apex (feet fixed at z = 0) and extension
/// `+2`: four fixed joints 2 m lower and a member each to a foot; leg
/// extension `A1` lowers the joint under the first foot by 1 m more.
const TOWER: &str = r#"
id = "T"
name = "tower"
kind = "lattice"
version = 3
[analysis]
nonlinear = false
self_weight = false
[[material]]
id = "steel"
elasticity = "210 GPa"
[[section]]
id = "s"
material = "steel"
area = "2000 mm2"
iy = "500 cm4"
iz = "500 cm4"
[[node]]
id = "F1"
point = ["2 m", "2 m", "0 m"]
support = "fixed"
[[node]]
id = "F2"
point = ["-2 m", "2 m", "0 m"]
support = "fixed"
[[node]]
id = "F3"
point = ["-2 m", "-2 m", "0 m"]
support = "fixed"
[[node]]
id = "F4"
point = ["2 m", "-2 m", "0 m"]
support = "fixed"
[[node]]
id = "T"
point = ["0 m", "0 m", "10 m"]
[[member]]
id = "L1"
nodes = ["F1", "T"]
section = "s"
kind = "frame"
[[member]]
id = "L2"
nodes = ["F2", "T"]
section = "s"
kind = "frame"
[[member]]
id = "L3"
nodes = ["F3", "T"]
section = "s"
kind = "frame"
[[member]]
id = "L4"
nodes = ["F4", "T"]
section = "s"
kind = "frame"
[[point]]
label = "A1"
node = "T"

[[extension]]
name = "+0"
dz = "0 m"

[[extension]]
name = "+2"
dz = "2 m"
[[extension.node]]
id = "E1"
point = ["2.4 m", "2.4 m", "-2 m"]
support = "fixed"
[[extension.node]]
id = "E2"
point = ["-2.4 m", "2.4 m", "-2 m"]
support = "fixed"
[[extension.node]]
id = "E3"
point = ["-2.4 m", "-2.4 m", "-2 m"]
support = "fixed"
[[extension.node]]
id = "E4"
point = ["2.4 m", "-2.4 m", "-2 m"]
support = "fixed"
[[extension.member]]
id = "EL1"
nodes = ["E1", "F1"]
section = "s"
kind = "frame"
group = "x-leg"
[[extension.member]]
id = "EL2"
nodes = ["E2", "F2"]
section = "s"
kind = "frame"
[[extension.member]]
id = "EL3"
nodes = ["E3", "F3"]
section = "s"
kind = "frame"
[[extension.member]]
id = "EL4"
nodes = ["E4", "F4"]
section = "s"
kind = "frame"

[[extension]]
name = "+2A1"
dz = "2 m"
[[extension.node]]
id = "G1"
point = ["2.4 m", "2.4 m", "-2 m"]
support = "fixed"
[[extension.node]]
id = "G2"
point = ["-2.4 m", "2.4 m", "-2 m"]
support = "fixed"
[[extension.node]]
id = "G3"
point = ["-2.4 m", "-2.4 m", "-2 m"]
support = "fixed"
[[extension.node]]
id = "G4"
point = ["2.4 m", "-2.4 m", "-2 m"]
support = "fixed"
[[extension.member]]
id = "GL1"
nodes = ["G1", "F1"]
section = "s"
kind = "frame"
[[extension.member]]
id = "GL2"
nodes = ["G2", "F2"]
section = "s"
kind = "frame"
[[extension.member]]
id = "GL3"
nodes = ["G3", "F3"]
section = "s"
kind = "frame"
[[extension.member]]
id = "GL4"
nodes = ["G4", "F4"]
section = "s"
kind = "frame"
[[extension.leg]]
leg = "A"
dz = "1 m"
"#;

fn solved(m: &Model) -> crate::Solution {
    let loads = Loads {
        nodal: m.point_loads("A1", [10e3, 4e3, -20e3]).unwrap(),
        self_weight: 0.0,
    };
    let options = Options {
        nonlinear: false,
        ..Options::default()
    };
    solve(m, &loads, &options, &default_budget()).unwrap()
}

fn sum(s: &crate::Solution) -> [f64; 3] {
    s.reactions.iter().fold([0.0; 3], |a, r| {
        [a[0] + r.force[0], a[1] + r.force[1], a[2] + r.force[2]]
    })
}

/// The extension's members join the model; the feet stand on them; the
/// reactions balance the loads; a plain extension leaves the model alone.
#[test]
fn extension_bodies_in_the_model() {
    let lib = SectionLibrary::default();
    let base = Model::from_structure_type_extended(TOWER, &lib, None).unwrap();
    assert_eq!((base.nodes.len(), base.members.len()), (5, 4));
    assert_eq!(
        Model::from_structure_type_extended(TOWER, &lib, Some("+0")).unwrap(),
        base
    );
    let m = Model::from_structure_type_extended(TOWER, &lib, Some("+2")).unwrap();
    assert_eq!((m.nodes.len(), m.members.len()), (9, 8));
    let supported: Vec<&str> = m
        .nodes
        .iter()
        .filter(|n| n.fixed.iter().any(|s| *s))
        .map(|n| n.id.as_str())
        .collect();
    assert_eq!(supported, ["E1", "E2", "E3", "E4"]);
    let s = solved(&m);
    let total = sum(&s);
    for (k, load) in [10e3, 4e3, -20e3].into_iter().enumerate() {
        assert!((total[k] + load).abs() < 1e-6 * 20e3, "{total:?}");
    }
    assert!(
        s.reactions
            .iter()
            .all(|r| m.nodes[r.node].id.starts_with('E'))
    );
    // The same loads on the body alone: the same total.
    let b = sum(&solved(&base));
    for k in 0..3 {
        assert!((b[k] - total[k]).abs() < 1e-6 * 20e3);
    }
    assert!(Model::from_structure_type_extended(TOWER, &lib, Some("+9")).is_err());
}

/// A leg extension lowers its support joint along its leg member: G1 at
/// (2.6, 2.6, −3) m, the leg 1.5 times as long; the model still balances.
#[test]
fn a_leg_extension_lowers_its_support() {
    let lib = SectionLibrary::default();
    let m = Model::from_structure_type_extended(TOWER, &lib, Some("+2A1")).unwrap();
    let g1 = m.nodes.iter().find(|n| n.id == "G1").unwrap();
    for (k, v) in [2.6, 2.6, -3.0].into_iter().enumerate() {
        assert!((g1.point[k] - v).abs() < 1e-9, "{:?}", g1.point);
    }
    let g2 = m.nodes.iter().find(|n| n.id == "G2").unwrap();
    assert_eq!(g2.point, [-2.4, 2.4, -2.0]);
    let leg = m.members.iter().find(|x| x.id == "GL1").unwrap();
    let (a, b) = (m.nodes[leg.i].point, m.nodes[leg.j].point);
    let len = (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f64>().sqrt();
    let plain = (0.4f64 * 0.4 * 2.0 + 4.0).sqrt();
    assert!((len - 1.5 * plain).abs() < 1e-9, "{len}");
    let s = solved(&m);
    let total = sum(&s);
    assert!((total[2] - 20e3).abs() < 1e-6 * 20e3, "{total:?}");
    // The lowered support takes a reaction.
    let r = s
        .reactions
        .iter()
        .find(|r| m.nodes[r.node].id == "G1")
        .unwrap();
    assert!(r.force[2].abs() > 1.0, "{r:?}");
}

/// A model file has no extensions; a structure file names one it has.
#[test]
fn extensions_of_model_files_are_refused() {
    let dir = std::env::temp_dir().join(format!("ols-fe-ext-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("t.olss");
    std::fs::write(&path, TOWER).unwrap();
    let lib = SectionLibrary::default();
    assert_eq!(
        Model::load_extended(&path, &lib, Some("+2"))
            .unwrap()
            .nodes
            .len(),
        9
    );
    let e = Model::load_extended(&example("type-02-model.olsfe"), &lib, Some("+2")).unwrap_err();
    assert!(e.contains("no extension"), "{e}");
    std::fs::remove_dir_all(&dir).ok();
}
