// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Two plates on one member end (O49 stage 1b and O44 L33, 2026-11-27)
//! end to end through the link and the bundled ASCE 10-97 plugin, and its
//! validation case `structsize-002` (`benchmarks/two-plates.toml` beside
//! this crate, in the benchmark format, run here): a bar whose end lies
//! between two plates has twice the one plate's bolt shear capacity and
//! half its usage, by hand; the second plate drawn on the other face of
//! the leg, its packing and its own bolts drawn and counted.

use std::path::Path;

use ohl_sections::SectionLibrary;
use ohl_structures::bolts::BoltLibrary;
use ohl_structures::{LibraryItem, StructureType};
use ohl_validation::{Calculation, Inputs, Outputs, Registry};

/// The case's structure and library.
fn inputs(i: &Inputs) -> Result<(String, SectionLibrary), String> {
    let model = i
        .get("model")
        .and_then(|v| v.as_str())
        .ok_or("no model")?
        .to_owned();
    let lib = SectionLibrary::parse(
        i.get("sections")
            .and_then(|v| v.as_str())
            .ok_or("no sections")?,
    )?;
    Ok((model, lib))
}

/// The same structure with one plate: the second plate's table cut off
/// (the case gives it last).
fn one_plate(text: &str) -> String {
    let at = text.find("[[plate]]\nid = \"G1b\"").unwrap_or(text.len());
    text[..at].to_owned()
}

/// D1's tension capacity (kN) and its usage in the run of the
/// structure's own load.
fn d1(text: &str, lib: &SectionLibrary) -> Result<(f64, f64, u32), String> {
    let model = ohl_structures_fe::Model::from_structure_type_with_joint_details(
        text,
        lib,
        &BoltLibrary::default(),
        None,
    )?;
    let planes = model.members[0].design.ends[1]
        .as_ref()
        .and_then(|e| e.planes.as_ref())
        .map_or(0, |p| p.0);
    let model = ohl_structure_link::plugin_check::find(model, None)?;
    let given = ohl_structure_link::plugin_check::capacities(&model)?;
    let cap = given
        .iter()
        .find(|g| g.id == "D1")
        .and_then(|g| g.capacities.iter().find(|c| c.0 == "tension"))
        .map(|c| c.1)
        .ok_or("no tension capacity")?;
    let t = StructureType::parse(Path::new("t.ohls"), text).map_err(|e| e.message)?;
    let (table, recipes) = ohl_structure_export::own_load_table(text, lib)?;
    let cases = ohl_structure_link::cases_to_run(&table, &recipes, None);
    let run = ohl_structure_link::analyse_structure(
        &ohl_structure_link::RunInput {
            structure: &t,
            text,
            dir: None,
            sections: lib,
            table: &table,
            recipes: &recipes,
            cases: &cases,
            extension: None,
            bolts: None,
            feet: None,
        },
        &ohl_structures_fe::default_budget(),
    )?;
    let usage = run
        .cases
        .iter()
        .filter_map(|c| c.fe.as_ref()?.as_ref().ok())
        .flat_map(|r| r.members.iter())
        .filter(|m| m.id == "D1")
        .filter_map(|m| m.usage)
        .fold(0.0, f64::max);
    Ok((cap / 1e3, usage, planes))
}

/// `structure.two_planes`: D1 with one plate and with two.
struct TwoPlanes;

impl Calculation for TwoPlanes {
    fn id(&self) -> &'static str {
        "structure.two_planes"
    }

    fn description(&self) -> &'static str {
        "OHL-Structure's member check (the bundled code plugin) of a member end on one plate and \
         between two plates, the joint details taken"
    }

    fn run(&self, i: &Inputs) -> Result<Outputs, String> {
        let (text, lib) = inputs(i)?;
        let (c1, u1, _) = d1(&one_plate(&text), &lib)?;
        let (c2, u2, planes) = d1(&text, &lib)?;
        let mut out = Outputs::new();
        out.insert("capacity_one".into(), c1);
        out.insert("capacity_two".into(), c2);
        out.insert("usage_one".into(), u1);
        out.insert("usage_two".into(), u2);
        out.insert("ratio".into(), u2 / u1);
        out.insert("planes_two".into(), f64::from(planes));
        Ok(out)
    }
}

/// The validation case, run as `ohl validate` runs the benchmark set.
#[test]
fn validation_case_structsize_002() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("benchmarks");
    let cases = ohl_validation::load_cases(&root).unwrap();
    let mut registry = Registry::new();
    registry.register(Box::new(TwoPlanes));
    let mut seen = false;
    for c in cases.into_iter().filter(|c| c.case.id == "structsize-002") {
        seen = true;
        let r = ohl_validation::run_case(&registry, c);
        assert!(r.passed(), "{:#?}", r.outcome);
    }
    assert!(seen);
}

/// The second plate in the joint details: on the other face of D1's leg
/// (its face the leg's thickness off the first's, through away from the
/// leg), D1's bolts made longer by its thickness, its packing and its own
/// bolts drawn as solids and counted in the bill of materials (the plate,
/// the packing, the bolts).
#[test]
fn the_second_plate_is_drawn_and_counted() {
    let case = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("benchmarks/two-plates.toml"),
    )
    .unwrap();
    let v: toml::Table = case.parse().unwrap();
    let c = &v["case"].as_array().unwrap()[0];
    let text = c["inputs"]["model"].as_str().unwrap();
    let lib = SectionLibrary::parse(c["inputs"]["sections"].as_str().unwrap()).unwrap();
    let bolts = BoltLibrary::default();
    let one = ohl_structure_link::joint_details(&one_plate(text), &lib, &bolts).unwrap();
    let d = ohl_structure_link::joint_details(text, &lib, &bolts).unwrap();
    let j = d.joint("N2").unwrap();
    let a = j.arrivals.iter().find(|a| a.member == "D1").unwrap();
    let a1 = one
        .joint("N2")
        .unwrap()
        .arrivals
        .iter()
        .find(|a| a.member == "D1")
        .unwrap();
    assert!(
        (a.grip[0] - a1.grip[0] - 0.010).abs() < 1e-12,
        "{:?}",
        a.grip
    );
    // Nothing runs into anything: D1's bolts pass through the second plate,
    // its packing and its own bolts lie clear of D1.
    let words: Vec<String> = j.clashes.iter().map(|c| c.words()).collect();
    assert!(words.is_empty(), "{words:#?}");
    let g2 = j.plates.iter().find(|p| p.id == "G1b").unwrap();
    let q = g2.paired.as_ref().expect("paired");
    assert_eq!(q.bolts.len(), 2);
    assert!(q.held);
    assert!((q.packing - 0.008).abs() < 1e-12);
    assert!(g2.outline.len() >= 3);
    assert!(g2.members.contains(&"D1".to_owned()));
    // On the other face: its outline off D1's bolts by the leg's thickness
    // along their axis.
    let axis = a.bolt_axis;
    let off: f64 = (0..3)
        .map(|k| (g2.outline[0][k] - a.bolt_points[0][k]) * axis[k])
        .sum();
    assert!((off - 0.008).abs() < 1e-9, "{off}");
    assert!(
        g2.notes
            .iter()
            .any(|n| n.contains("second plate of D1") && n.contains("NOT ON THE SAFE SIDE")),
        "{:?}",
        g2.notes
    );
    let plates = ohl_structure_link::plate_solids(&d);
    assert!(plates.iter().filter(|p| p.id == "G1b").count() >= 2);
    let solids = ohl_structure_link::bolt_solids(&d);
    let before = ohl_structure_link::bolt_solids(&one);
    assert_eq!(solids.len(), before.len() + 2);
    let bom = ohl_structure_link::structure_quantities(text, &lib, &bolts).unwrap();
    let ids: Vec<&str> = bom.parts.iter().map(|p| p.id.as_str()).collect();
    assert!(ids.contains(&"G1b"), "{ids:?}");
    assert!(ids.contains(&"G1b packing"), "{ids:?}");
}
