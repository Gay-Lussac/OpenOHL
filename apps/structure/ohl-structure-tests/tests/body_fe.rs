// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A body built by the structure application's generators, given a
//! section per member group and its feet supported, is a level 3 design
//! the core's finite-element model solves: X bracing with horizontals is
//! no mechanism, with crossarms and a peak on it too, and the reactions
//! carry the loads. The same with the options: redundants in every
//! bracing pattern (one and two levels), plan bracing, widths per panel,
//! several points on one arm — each solved without a stabilisation
//! refused (a load in a direction no member holds), with and without its
//! own weight.

use std::path::Path;

use ohl_structure_edit::{
    ArmShape, Bracing, Crossarm, Generator, Kind, Panel, Panels, Peak, Plan, Redundants, apply,
    edits, groups, rebuild_all, set_group_section,
};
use ohl_structures::{Detail, LibraryItem, StructureType};
use ohl_structures_fe::{Loads, Model, NodalLoad};
use toml_edit::DocumentMut;

fn read(doc: &DocumentMut) -> StructureType {
    StructureType::parse(Path::new("g.ohls"), &doc.to_string()).unwrap()
}

fn generator(id: &str, kind: Kind) -> Generator {
    Generator {
        id: id.into(),
        kind,
    }
}

/// A 30 m frustum, 8 m at the feet and 2 m at the waist, in panels of
/// `bracing` with horizontals.
fn body(bracing: Bracing) -> Panels {
    Panels {
        bottom: None,
        feet: [8.0, 8.0],
        waist: [2.0, 2.0],
        panels: [8.0, 15.0, 21.0, 26.0, 30.0]
            .into_iter()
            .map(|top| Panel {
                top,
                bracing,
                horizontal: true,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

/// The crossarms of a tower: the points, their shape, one arm per side.
struct Arms {
    points: Vec<[f64; 3]>,
    shape: ArmShape,
    one_arm: bool,
}

/// The body `panels`, optionally crossarms and a peak (4 m above the
/// top), sections per group (the legs heavier), the feet pinned.
fn build(panels: Panels, arms: Option<Arms>) -> DocumentMut {
    let mut doc: DocumentMut = "id = \"G\"\nname = \"generated\"\nkind = \"lattice\"\n"
        .parse()
        .unwrap();
    if let Some(a) = &arms {
        for at in &a.points {
            let t = read(&doc);
            edits::add_point(&mut doc, &t, *at);
        }
    }
    let t = read(&doc);
    apply(&mut doc, &t, &generator("body", Kind::Panels(panels))).unwrap();
    if let Some(a) = arms {
        let t = read(&doc);
        let labels: Vec<String> = t.points.iter().map(|p| p.label.clone()).collect();
        apply(
            &mut doc,
            &t,
            &generator(
                "arms",
                Kind::Crossarm(Crossarm {
                    points: labels,
                    shape: a.shape,
                    one_arm: a.one_arm,
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
        // Built again: the body knows the crossarms' levels now.
        let t = read(&doc);
        rebuild_all(&mut doc, &t).unwrap();
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
    edits::add_item(
        &mut doc,
        "section",
        "L40",
        &[("material", "S355"), ("area", "3 cm2")],
    );
    let t = read(&doc);
    for g in groups(&t) {
        let s = if g.name == "body-leg" {
            "L100"
        } else if g.name.contains("redundant") {
            "L40"
        } else {
            "L60"
        };
        set_group_section(&mut doc, &t, &g.name, Some(s));
    }
    let t = read(&doc);
    for n in t.nodes.iter().filter(|n| n.point[2].si().abs() < 1e-6) {
        edits::set_support(&mut doc, &n.id, Some("pinned"));
    }
    doc
}

/// A 30 m frustum, 8 m at the feet and 2 m at the waist, in X panels
/// with horizontals; optionally two triangular crossarms and a peak.
fn tower(arms: bool) -> DocumentMut {
    build(
        body(Bracing::X),
        arms.then(|| Arms {
            points: vec![[-7.0, 0.0, 26.0], [7.0, 0.0, 26.0]],
            shape: ArmShape::Triangular,
            one_arm: false,
        }),
    )
}

/// Solves the tower under a sideways and a vertical load at the outermost
/// joints of its top and at the joints carrying a point, with its own
/// weight times `weight`, with P-Delta (the default) and in first order:
/// never refused, and the reactions with what the stabilisation holds
/// balance the loads. Returns the joint directions held (no stiffness of
/// their own: the redundants' and plan bracing's joints across their
/// plane) and the share of the loads they take with P-Delta.
fn solve_with(doc: &DocumentMut, weight: f64) -> (usize, f64) {
    let t = read(doc);
    assert_eq!(t.detail(), Detail::Design);
    let mut model = Model::from_structure_type(&doc.to_string()).unwrap();
    let top = model
        .nodes
        .iter()
        .map(|n| n.point[2])
        .fold(f64::NEG_INFINITY, f64::max);
    let reach = |n: &ohl_structures_fe::Node| n.point[0].hypot(n.point[1]);
    let out = model
        .nodes
        .iter()
        .filter(|n| (n.point[2] - top).abs() < 1e-6)
        .map(reach)
        .fold(0.0, f64::max);
    let nodal: Vec<NodalLoad> = model
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| {
            ((n.point[2] - top).abs() < 1e-6 && reach(n) > out - 1e-6) || n.attachment.is_some()
        })
        .map(|(k, _)| NodalLoad {
            node: k,
            force: [10e3, 5e3, -20e3],
            moment: [0.0; 3],
        })
        .collect();
    let loads = Loads {
        nodal,
        self_weight: weight,
    };
    let applied: [f64; 3] = [0, 1, 2].map(|k| loads.nodal.iter().map(|l| l.force[k]).sum::<f64>());
    let size = applied.iter().map(|v| v * v).sum::<f64>().sqrt();
    let mut shares = Vec::new();
    for nonlinear in [true, false] {
        model.analysis.nonlinear = nonlinear;
        let (s, _) =
            ohl_structures_fe::analyse(&model, &loads, 1.0, &ohl_structures_fe::default_budget())
                .unwrap_or_else(|e| panic!("{e}"));
        let held: [f64; 3] = [0, 1, 2].map(|k| {
            s.stabilised
                .iter()
                .filter(|h| !h.at.rotation)
                .map(|h| h.force * h.at.direction[k])
                .sum::<f64>()
        });
        let sum: [f64; 3] =
            [0, 1, 2].map(|k| s.reactions.iter().map(|r| r.force[k]).sum::<f64>() + held[k]);
        for k in 0..2 {
            assert!(
                (sum[k] + applied[k]).abs() < 1e-3 * applied[k].abs().max(1.0),
                "{sum:?} {applied:?}"
            );
        }
        if weight == 0.0 {
            assert!(
                (sum[2] + applied[2]).abs() < 1e-3 * applied[2].abs(),
                "{sum:?} {applied:?}"
            );
        } else {
            // The weight is carried too: the vertical reactions are larger.
            assert!(sum[2] > -applied[2], "{sum:?} {applied:?}");
        }
        let share = held.iter().map(|v| v * v).sum::<f64>().sqrt() / size;
        if !nonlinear && weight == 0.0 {
            // In first order nothing loads a held direction (but
            // for the joints a millimetre off their plane, rounded).
            assert!(share < 1e-3, "{share}");
        }
        shares.push((s.stabilised.len(), share));
    }
    shares[0]
}

fn solve(doc: &DocumentMut) {
    let (held, _) = solve_with(doc, 0.0);
    assert_eq!(held, 0);
}

#[test]
fn x_panels_with_horizontals_solve() {
    solve(&tower(false));
}

#[test]
fn crossarms_and_a_peak_on_the_body_solve() {
    solve(&tower(true));
}

/// Redundants, one and two levels, in every bracing pattern: no
/// mechanism (each face stays triangulated); only the directions out of
/// a face of the redundants' own joints are held: in first order they
/// carry nothing (without the weight), with P-Delta a small share of the
/// sway (printed).
#[test]
fn redundants_in_every_pattern_solve() {
    for bracing in Bracing::ALL {
        for level in [Redundants::One, Redundants::Two] {
            let mut p = body(bracing);
            for panel in &mut p.panels {
                panel.redundants = level;
            }
            let doc = build(p, None);
            let (held, share) = solve_with(&doc, 0.0);
            assert!(held > 0);
            solve_with(&doc, 1.0);
            eprintln!(
                "{} {}: {held} direction(s) held, {:.2} % of the loads with P-Delta",
                bracing.key(),
                level.key(),
                100.0 * share
            );
        }
    }
}

/// Plan bracing, X and diamond, at every level; with widths per panel
/// (a waist at 21 m, straight above) and plan bracing where the slope
/// changes and at the crossarms' levels.
#[test]
fn plan_bracing_solves() {
    for plan in [Plan::X, Plan::Diamond] {
        let mut p = body(Bracing::X);
        for panel in &mut p.panels {
            panel.plan = plan;
            panel.horizontal = false;
        }
        solve_with(&build(p.clone(), None), 0.0);
        solve_with(&build(p, None), 1.0);
    }
    let mut p = body(Bracing::K);
    p.panels[2].width = Some([2.0, 2.0]);
    p.panels[1].redundants = Redundants::One;
    p.plan_auto = Plan::Diamond;
    let doc = build(
        p,
        Some(Arms {
            points: vec![[-6.0, 0.0, 26.0], [6.0, 0.0, 26.0]],
            shape: ArmShape::Triangular,
            one_arm: false,
        }),
    );
    let text = doc.to_string();
    // At the waist (21 m) and the arms' levels (26 m, 30 m).
    for level in [3, 4, 5] {
        assert!(
            text.contains(&format!("group = \"body-plan-{level}\"")),
            "{level}"
        );
    }
    assert!(!text.contains("group = \"body-plan-2\""));
    solve_with(&doc, 0.0);
    solve_with(&doc, 1.0);
}

/// Several points on one arm, loaded: an inner and an outer phase, and a
/// V string's two upper points, triangular and rectangular.
#[test]
fn several_points_on_one_arm_solve() {
    for shape in [ArmShape::Triangular, ArmShape::Rectangular] {
        for points in [
            vec![
                [-8.0, 0.0, 23.0],
                [-4.5, 0.0, 23.0],
                [4.5, 0.0, 23.0],
                [8.0, 0.0, 23.0],
            ],
            vec![
                [6.0, 0.0, 26.0],
                [4.0, 0.0, 26.0],
                [3.0, 0.0, 26.0],
                [-5.0, 0.0, 26.0],
            ],
        ] {
            let doc = build(
                body(Bracing::X),
                Some(Arms {
                    points,
                    shape,
                    one_arm: true,
                }),
            );
            solve_with(&doc, 0.0);
            solve_with(&doc, 1.0);
        }
    }
}
