// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The member sizer (O49 stage 1) end to end through the link and the
//! bundled ASCE 10-97 plugin, and its validation case `structsize-001`
//! (`benchmarks/member-sizing.toml` beside this crate, in the benchmark
//! format, run here: the calculation is the structure application's, which
//! the core's `ohl validate` cannot reach): an invented plane truss of
//! three member groups whose sizer choice is proved the lightest passing
//! one by **brute force** — every combination of the library's six angles
//! for the three groups (6³ = 216 full runs of the structure), each kept
//! only when every member passes the code's check (usage ≤ 1 in the full
//! run), the code's limits (the plugin's `member-limits`) and the
//! joint-fit limits, the lightest kept compared with the sizer's.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ohl_sections::SectionLibrary;
use ohl_structure_link::plugin_check;
use ohl_structure_link::sizing::{self, Settings, SizingInput};
use ohl_structure_loads::{LoadCaseRecipe, LoadTable};
use ohl_structures::{LibraryItem, StructureType};
use ohl_validation::{Calculation, Inputs, Outputs, Registry};

/// The truss and its library of a case's inputs (`model`, `sections`).
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

/// The structure's own load table (its [[load]]s: the case "model").
fn own_table(text: &str, lib: &SectionLibrary) -> (LoadTable, Vec<LoadCaseRecipe>) {
    ohl_structure_export::own_load_table(text, lib).unwrap()
}

/// A folder where the bundled plugin is found (the tests run in the
/// crate's folder, inside the source tree).
fn dir() -> PathBuf {
    let d = std::env::temp_dir().join(format!("ohl-member-sizing-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The sizer's answer for `text`.
fn sized(text: &str, lib: &SectionLibrary) -> Result<sizing::Sizing, String> {
    let t = StructureType::parse(Path::new("t.ohls"), text).map_err(|e| e.message)?;
    let (table, recipes) = own_table(text, lib);
    let cases = ohl_structure_link::cases_to_run(&table, &recipes, None);
    let d = dir();
    sizing::size(
        &SizingInput {
            structure: &t,
            text,
            dir: Some(&d),
            sections: lib,
            table: &table,
            recipes: &recipes,
            cases: &cases,
            settings: &Settings::default(),
        },
        &ohl_core::budget::Budget::new(),
    )
}

/// Whether the structure of `text` passes everything the sizer keeps to:
/// every member's usage ≤ 1 in a full run, the code's limits (compression
/// ones on members compressed), no joint-fit problem.
fn passes(text: &str, lib: &SectionLibrary) -> Result<bool, String> {
    let t = StructureType::parse(Path::new("t.ohls"), text).map_err(|e| e.message)?;
    let (table, recipes) = own_table(text, lib);
    let cases = ohl_structure_link::cases_to_run(&table, &recipes, None);
    let d = dir();
    let run = ohl_structure_link::analyse_structure(
        &ohl_structure_link::RunInput {
            structure: &t,
            text,
            dir: Some(&d),
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
    let mut axial: BTreeMap<String, f64> = BTreeMap::new();
    let mut compressed: BTreeMap<String, bool> = BTreeMap::new();
    for (c, k) in run.cases.iter().zip(&cases) {
        let Some(Ok(r)) = &c.fe else {
            return Err(format!("{}: not solved", c.name));
        };
        for m in &r.members {
            if m.usage.is_none_or(|u| u > 1.0) {
                return Ok(false);
            }
            let a = axial.entry(m.id.clone()).or_default();
            *a = a.max(m.axial.abs() / k.strength);
            *compressed.entry(m.id.clone()).or_default() |= m.axial < 0.0;
        }
    }
    let bolts = ohl_structures::bolts::BoltLibrary::default();
    let model = ohl_structures_fe::Model::from_structure_type_with_bolts(text, lib, &bolts, None)?;
    let model = plugin_check::find(model, Some(&d))?;
    if let plugin_check::limits::Limits::Given { limits, .. } =
        plugin_check::limits::limits(&model)?
    {
        for l in limits {
            let id = &model.members[l.member].id;
            if (!l.compression || compressed[id]) && l.broken() {
                return Ok(false);
            }
        }
    }
    let ks: Vec<usize> = (0..model.members.len()).collect();
    let forces: Vec<f64> = model.members.iter().map(|m| axial[&m.id]).collect();
    let (fit, _) = sizing::joint_fit(
        &model,
        &ks,
        &forces,
        &sizing::FitRules::for_structure(text, Some(&d)),
    );
    Ok(fit.is_empty())
}

/// The truss with the groups' sections `choice` (bottom, top, web).
fn with(text: &str, choice: [&str; 3]) -> String {
    let mut doc: toml_edit::DocumentMut = text.parse().unwrap();
    for m in doc["member"].as_array_of_tables_mut().unwrap().iter_mut() {
        let g = m["group"].as_str().unwrap().to_owned();
        let s = match g.as_str() {
            "bottom" => choice[0],
            "top" => choice[1],
            _ => choice[2],
        };
        m["section"] = toml_edit::value(s);
    }
    doc.to_string()
}

/// The mass of the truss's members of `text`, kg (profile mass × length).
fn mass(text: &str, lib: &SectionLibrary) -> f64 {
    let t = StructureType::parse(Path::new("t.ohls"), text).unwrap();
    let at = |id: &str| {
        let n = t.nodes.iter().find(|n| n.id == id).unwrap();
        n.point.map(|v| v.si())
    };
    t.members
        .iter()
        .map(|m| {
            let (a, b) = (at(&m.nodes[0]), at(&m.nodes[1]));
            let l = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
            let s = lib.section(m.section.as_deref().unwrap()).unwrap();
            s.properties().area * lib.material_of(s).unwrap().density * l
        })
        .sum()
}

/// `structure.size` for the validation case: the sizer's choice and the
/// brute force's.
struct SizeCalculation;

impl Calculation for SizeCalculation {
    fn id(&self) -> &'static str {
        "structure.size"
    }

    fn description(&self) -> &'static str {
        "OHL-Structure's member sizer on a structure (its own [[load]]s), with a brute force over \
         every combination of the library's sections for its groups (bottom, top, web)"
    }

    fn run(&self, i: &Inputs) -> Result<Outputs, String> {
        let (text, lib) = inputs(i)?;
        let s = sized(&text, &lib)?;
        let mut out = Outputs::new();
        out.insert("mass_total".into(), s.mass[1]);
        for g in &s.groups {
            out.insert(format!("mass_{}", g.group.name), g.mass[1]);
        }
        out.insert("over_target".into(), s.over_target.len() as f64);
        // Brute force: every combination, the lightest passing.
        let ids: Vec<String> = lib.sections.iter().map(|x| x.id.clone()).collect();
        let mut best: Option<(f64, [String; 3])> = None;
        for a in &ids {
            for b in &ids {
                for c in &ids {
                    let t = with(&text, [a, b, c]);
                    let m = mass(&t, &lib);
                    if best.as_ref().is_some_and(|x| x.0 <= m) {
                        continue;
                    }
                    if passes(&t, &lib)? {
                        best = Some((m, [a.clone(), b.clone(), c.clone()]));
                    }
                }
            }
        }
        let (m, choice) = best.ok_or("no combination passes")?;
        out.insert("brute_mass_total".into(), m);
        let chosen: Vec<String> = ["bottom", "top", "web"]
            .iter()
            .map(|n| {
                s.groups
                    .iter()
                    .find(|g| g.group.name == *n)
                    .map(|g| g.to.clone())
                    .unwrap_or_default()
            })
            .collect();
        out.insert("same_choice".into(), f64::from(u8::from(chosen == choice)));
        Ok(out)
    }
}

/// The validation case, run as `ohl validate` runs the benchmark set.
#[test]
fn validation_case_structsize_001() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("benchmarks");
    let cases = ohl_validation::load_cases(&root).unwrap();
    assert!(cases.iter().any(|c| c.case.id == "structsize-001"));
    let mut registry = Registry::new();
    registry.register(Box::new(SizeCalculation));
    for c in cases {
        let r = ohl_validation::run_case(&registry, c);
        assert!(r.passed(), "{:#?}", r.outcome);
    }
}

/// The sizer through the bundled plugin: no member above the target, the
/// code's limits checked (no NOT ON THE SAFE SIDE note), the proposal's
/// text the structure with every change.
#[test]
fn the_bundled_code_sizes_the_truss() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("benchmarks/member-sizing.toml");
    let cases =
        ohl_validation::parse_cases(&root, &std::fs::read_to_string(&root).unwrap()).unwrap();
    let (text, lib) = inputs(&cases[0].case.inputs).unwrap();
    let s = sized(&text, &lib).unwrap();
    assert!(s.over_target.is_empty(), "{:?}", s.over_target);
    assert!(
        !s.notes
            .iter()
            .any(|n| n.starts_with("NOT ON THE SAFE SIDE")),
        "{:?}",
        s.notes
    );
    assert!(s.mass[1] < s.mass[0]);
    assert!(passes(&s.text, &lib).unwrap());
    // The CLI's lines.
    let lines = sizing::summary(&s);
    assert!(lines[0].contains("group(s) changed"), "{lines:?}");
}
