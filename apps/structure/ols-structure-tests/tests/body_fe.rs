// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A body built by the structure application's generators, given a
//! section per member group and its feet supported, is a level 3 design
//! the core's finite-element model solves: X bracing with horizontals is
//! no mechanism, with crossarms and a peak on it too, and the reactions
//! carry the loads.

use std::path::Path;

use ols_structure_edit::{
    ArmShape, Bracing, Crossarm, Generator, Kind, Panel, Panels, Peak, apply, edits, groups,
    set_group_section,
};
use ols_structures::{Detail, LibraryItem, StructureType};
use ols_structures_fe::{Loads, Model, NodalLoad};
use toml_edit::DocumentMut;

fn read(doc: &DocumentMut) -> StructureType {
    StructureType::parse(Path::new("g.olss"), &doc.to_string()).unwrap()
}

fn generator(id: &str, kind: Kind) -> Generator {
    Generator {
        id: id.into(),
        kind,
    }
}

/// A 30 m frustum, 8 m at the feet and 2 m at the waist, in X panels
/// with horizontals; optionally two triangular crossarms and a peak.
fn tower(arms: bool) -> DocumentMut {
    let mut doc: DocumentMut = "id = \"G\"\nname = \"generated\"\nkind = \"lattice\"\n"
        .parse()
        .unwrap();
    let panels = generator(
        "body",
        Kind::Panels(Panels {
            bottom: None,
            feet: [8.0, 8.0],
            waist: [2.0, 2.0],
            panels: [8.0, 15.0, 21.0, 26.0, 30.0]
                .into_iter()
                .map(|top| Panel {
                    top,
                    bracing: Bracing::X,
                    horizontal: true,
                })
                .collect(),
        }),
    );
    let t = read(&doc);
    apply(&mut doc, &t, &panels).unwrap();
    if arms {
        for x in [-7.0, 7.0] {
            let t = read(&doc);
            edits::add_point(&mut doc, &t, [x, 0.0, 26.0]);
        }
        let t = read(&doc);
        let labels: Vec<String> = t.points.iter().map(|p| p.label.clone()).collect();
        apply(
            &mut doc,
            &t,
            &generator(
                "arms",
                Kind::Crossarm(Crossarm {
                    points: labels,
                    shape: ArmShape::Triangular,
                }),
            ),
        )
        .unwrap();
        let t = read(&doc);
        apply(
            &mut doc,
            &t,
            &generator(
                "peak",
                Kind::Peak(Peak {
                    points: Vec::new(),
                    height: 4.0,
                }),
            ),
        )
        .unwrap();
    }
    edits::add_item(
        &mut doc,
        "material",
        "S355",
        &[("elasticity", "210 GPa"), ("density", "7850 kg/m3")],
    );
    edits::add_item(
        &mut doc,
        "section",
        "L100",
        &[("material", "S355"), ("area", "19 cm2")],
    );
    edits::add_item(
        &mut doc,
        "section",
        "L60",
        &[("material", "S355"), ("area", "7 cm2")],
    );
    let t = read(&doc);
    for g in groups(&t) {
        let s = if g.name == "body-leg" { "L100" } else { "L60" };
        set_group_section(&mut doc, &t, &g.name, Some(s));
    }
    let t = read(&doc);
    for n in t.nodes.iter().filter(|n| n.point[2].si().abs() < 1e-6) {
        edits::set_support(&mut doc, &n.id, Some("pinned"));
    }
    doc
}

/// Solves the tower under a sideways and a vertical load at its top
/// joints; the reactions balance them.
fn solve(doc: &DocumentMut) {
    let t = read(doc);
    assert_eq!(t.detail(), Detail::Design);
    let model = Model::from_structure_type(&doc.to_string()).unwrap();
    let top = t
        .nodes
        .iter()
        .map(|n| n.point[2].si())
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
    let (s, _) =
        ols_structures_fe::analyse(&model, &loads, 1.0, &ols_structures_fe::default_budget())
            .unwrap_or_else(|e| panic!("{e}"));
    let reactions: [f64; 3] =
        [0, 1, 2].map(|k| s.reactions.iter().map(|r| r.force[k]).sum::<f64>());
    for k in 0..3 {
        assert!(
            (reactions[k] + total[k]).abs() < 1e-3 * total[k].abs().max(1.0),
            "{reactions:?} {total:?}"
        );
    }
}

#[test]
fn x_panels_with_horizontals_solve() {
    solve(&tower(false));
}

#[test]
fn crossarms_and_a_peak_on_the_body_solve() {
    solve(&tower(true));
}
