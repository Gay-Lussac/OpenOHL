// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The structure application's pole and extension edits with the core:
//! the example pole brought into its structure file is checked by the
//! core's pole check with the `.ohlp` numbers; a generated body standing
//! on an extension piece built by the application (and a leg made
//! longer) is solved by the core's model, the reactions on the piece.

use std::path::Path;

use ols_structure_edit::extension_edits::{PieceBracing, build_piece, set_leg};
use ols_structure_edit::{
    Bracing, Generator, Kind, Panel, Panels, apply, edits, groups, pole as pole_edit,
    set_group_section,
};
use ols_structures::{Detail, LibraryItem, StructureType};
use ols_structures_fe::pole::structure_pole;
use ols_structures_fe::{Loads, Model, NodalLoad, Pole, PoleLoad, check_pole, default_budget};
use toml_edit::DocumentMut;

fn structures() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/synthetic-line/Structures")
}

fn read(doc: &DocumentMut) -> StructureType {
    StructureType::parse(Path::new("g.ohls"), &doc.to_string()).unwrap()
}

#[test]
fn the_example_pole_brought_in_checks_the_same() {
    let dir = structures();
    let text = std::fs::read_to_string(dir.join("type-05.ohls")).unwrap();
    let named = StructureType::parse(&dir.join("type-05.ohls"), &text).unwrap();
    let lib = ols_sections::SectionLibrary::default();
    let olsp = structure_pole(&named, &dir, &lib).unwrap().unwrap();
    assert_eq!(olsp, Pole::load(&dir.join("steel-pole-30m.ohlp")).unwrap());
    let mut doc: DocumentMut = text.parse().unwrap();
    let pole_text = std::fs::read_to_string(dir.join("steel-pole-30m.ohlp")).unwrap();
    pole_edit::bring_in(&mut doc, &named, &pole_text).unwrap();
    let t = read(&doc);
    assert_eq!(t.detail(), Detail::Design);
    let inline = structure_pole(&t, Path::new("."), &lib).unwrap().unwrap();
    let loads = [PoleLoad {
        point: t.point_local("A1", None).unwrap(),
        force: [5e3, 1e3, -8e3],
    }];
    for wind in [[0.0, 0.0], [-800.0, 0.0]] {
        let a = check_pole(&olsp, &loads, wind, 1.0, true, &default_budget()).unwrap();
        let b = check_pole(&inline, &loads, wind, 1.0, true, &default_budget()).unwrap();
        assert_eq!((a.usage, a.moment, a.height), (b.usage, b.moment, b.height));
    }
}

/// A 20 m frustum in X panels with horizontals, a section per group, the
/// feet pinned.
fn tower() -> DocumentMut {
    let mut doc: DocumentMut = "id = \"G\"\nname = \"generated\"\nkind = \"lattice\"\n"
        .parse()
        .unwrap();
    let t = read(&doc);
    apply(
        &mut doc,
        &t,
        &Generator {
            id: "body".into(),
            kind: Kind::Panels(Panels {
                bottom: None,
                feet: [6.0, 6.0],
                waist: [2.0, 2.0],
                panels: [7.0, 14.0, 20.0]
                    .into_iter()
                    .map(|top| Panel {
                        top,
                        bracing: Bracing::X,
                        horizontal: true,
                    })
                    .collect(),
            }),
        },
    )
    .unwrap();
    edits::add_item(
        &mut doc,
        "material",
        "S355",
        &[("elasticity", "210 GPa"), ("density", "7850 kg/m3")],
    );
    edits::add_item(
        &mut doc,
        "section",
        "L80",
        &[("material", "S355"), ("area", "12 cm2")],
    );
    let t = read(&doc);
    for g in groups(&t) {
        set_group_section(&mut doc, &t, &g.name, Some("L80"));
    }
    let t = read(&doc);
    for n in t.nodes.iter().filter(|n| n.point[2].si().abs() < 1e-6) {
        edits::set_support(&mut doc, &n.id, Some("pinned"));
    }
    doc
}

/// Solved with a sideways and a vertical load at the top joints: the
/// reactions balance them; the supported joints' ids.
fn solve(doc: &DocumentMut, extension: Option<&str>) -> (Model, Vec<String>) {
    let lib = ols_sections::SectionLibrary::default();
    let model = Model::from_structure_type_extended(&doc.to_string(), &lib, extension).unwrap();
    let top = model
        .nodes
        .iter()
        .map(|n| n.point[2])
        .fold(f64::NEG_INFINITY, f64::max);
    let nodal: Vec<NodalLoad> = model
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| (n.point[2] - top).abs() < 1e-6)
        .map(|(k, _)| NodalLoad {
            node: k,
            force: [10e3, 5e3, -20e3],
            moment: [0.0; 3],
        })
        .collect();
    let total: [f64; 3] = [0, 1, 2].map(|k| nodal.iter().map(|l| l.force[k]).sum::<f64>());
    let loads = Loads {
        nodal,
        self_weight: 0.0,
    };
    let (s, _) = ols_structures_fe::analyse(&model, &loads, 1.0, &default_budget())
        .unwrap_or_else(|e| panic!("{e}"));
    let reactions: [f64; 3] =
        [0, 1, 2].map(|k| s.reactions.iter().map(|r| r.force[k]).sum::<f64>());
    for k in 0..3 {
        assert!(
            (reactions[k] + total[k]).abs() < 1e-3 * total[k].abs().max(1.0),
            "{reactions:?} {total:?}"
        );
    }
    let supported = s
        .reactions
        .iter()
        .map(|r| model.nodes[r.node].id.clone())
        .collect();
    (model, supported)
}

#[test]
fn a_body_on_its_extension_piece_solves() {
    let mut doc = tower();
    let (base, feet) = solve(&doc, None);
    assert_eq!(feet.len(), 4);
    edits::set_extension(&mut doc, "+3", 3.0);
    let t = read(&doc);
    let piece = build_piece(&mut doc, &t, "+3", PieceBracing::X, true).unwrap();
    assert_eq!((piece.joints, piece.members), (4, 16));
    let t = read(&doc);
    assert_eq!(t.detail(), Detail::Design);
    let (m, supported) = solve(&doc, Some("+3"));
    assert_eq!(m.nodes.len(), base.nodes.len() + 4);
    assert_eq!(m.members.len(), base.members.len() + 16);
    assert!(
        supported.iter().all(|id| id.starts_with("X3-J")),
        "{supported:?}"
    );
    // Leg A one metre longer: its support 4 m under the feet.
    set_leg(&mut doc, "+3", "A", Some(1.0)).unwrap();
    let (m, supported) = solve(&doc, Some("+3"));
    assert_eq!(supported.len(), 4);
    let lowest = m
        .nodes
        .iter()
        .map(|n| n.point[2])
        .fold(f64::INFINITY, f64::min);
    assert!((lowest + 4.0).abs() < 1e-9, "{lowest}");
}
