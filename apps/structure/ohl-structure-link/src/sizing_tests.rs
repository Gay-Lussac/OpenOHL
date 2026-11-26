// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The member sizer (O49 stage 1) on an invented plane truss, statically
//! determinate (its forces do not depend on the sections without its own
//! weight): A (0, 0, 0) and B (0, 0, 2 m) pinned, C (2 m, 0, 0), D (2 m,
//! 0, 2 m), E (4 m, 0, 0) held across the plane; 30 kN down at E. By hand:
//! AC 60 kN and CE 30 kN in compression (group `bottom`), BD 30 kN in
//! tension (`top`), CD 30 kN in compression, BC and DE 42.43 kN in tension
//! (`web`). The core's simple check (tension A f_y, compression the
//! smaller of A f_y and π² E I_v / L²), every member bolted with two 16 mm
//! bolts (the default minimums: edge 1.5 d = 24 mm — a 40 mm leg has 20 mm
//! to the toe at mid-leg: it does not take them).

use ohl_sections::SectionLibrary;
use ohl_structure_loads::{At, LoadRow, LoadTable, Source};
use ohl_structures::{LibraryItem, StructureType};

use crate::CaseToRun;
use crate::sizing::{self, Settings, SizingInput};

/// Invented equal angles, one steel (f_y 355 MPa, 7850 kg/m³).
pub(crate) const LIBRARY: &str = r#"format = "ohl-sections"
version = 1
name = "Sizing test angles (invented)"
default_material = "s355"

[[material]]
id = "s355"
name = "Steel (invented)"
elasticity = "210 GPa"
shear_modulus = "81 GPa"
density = "7850 kg/m3"
yield = "355 MPa"
ultimate = "510 MPa"

[[section]]
id = "L40x4"
shape = "equal_angle"
h = "40 mm"
t = "4 mm"
r = "5 mm"

[[section]]
id = "L45x4"
shape = "equal_angle"
h = "45 mm"
t = "4 mm"
r = "6 mm"

[[section]]
id = "L50x5"
shape = "equal_angle"
h = "50 mm"
t = "5 mm"
r = "6 mm"

[[section]]
id = "L60x6"
shape = "equal_angle"
h = "60 mm"
t = "6 mm"
r = "7 mm"

[[section]]
id = "L70x7"
shape = "equal_angle"
h = "70 mm"
t = "7 mm"
r = "8 mm"

[[section]]
id = "L80x8"
shape = "equal_angle"
h = "80 mm"
t = "8 mm"
r = "9 mm"

[[section]]
id = "L90x9"
shape = "equal_angle"
h = "90 mm"
t = "9 mm"
r = "10 mm"

[[section]]
id = "L100x10"
shape = "equal_angle"
h = "100 mm"
t = "10 mm"
r = "11 mm"

[[section]]
id = "T60"
shape = "circular_hollow"
d = "60 mm"
t = "4 mm"
"#;

/// The truss, every member an L80×8 with its design data's thickness and
/// w/t (as a tower program writes them).
pub(crate) const TRUSS: &str = r#"format = "ohl-structure"
version = 21
id = "T"
name = "sizing truss"
kind = "lattice"

[analysis]
nonlinear = false
self_weight = false
capacity = "simple"

[[node]]
id = "A"
point = ["0 m", "0 m", "0 m"]
support = "pinned"

[[node]]
id = "B"
point = ["0 m", "0 m", "2 m"]
support = "pinned"

[[node]]
id = "C"
point = ["2 m", "0 m", "0 m"]
support = ["y"]

[[node]]
id = "D"
point = ["2 m", "0 m", "2 m"]
support = ["y"]

[[node]]
id = "E"
point = ["4 m", "0 m", "0 m"]
support = ["y"]

[[member]]
id = "AC"
nodes = ["A", "C"]
section = "L80x8"
group = "bottom"
design = { thickness = "8 mm", width_thickness = 8.9, bolts = { diameter = "16 mm", count = 2 } }

[[member]]
id = "CE"
nodes = ["C", "E"]
section = "L80x8"
group = "bottom"
design = { bolts = { diameter = "16 mm", count = 2 } }

[[member]]
id = "BD"
nodes = ["B", "D"]
section = "L80x8"
group = "top"
design = { bolts = { diameter = "16 mm", count = 2 } }

[[member]]
id = "CD"
nodes = ["C", "D"]
section = "L80x8"
group = "web"
design = { bolts = { diameter = "16 mm", count = 2 } }

[[member]]
id = "BC"
nodes = ["B", "C"]
section = "L80x8"
group = "web"
design = { bolts = { diameter = "16 mm", count = 2 } }

[[member]]
id = "DE"
nodes = ["D", "E"]
section = "L80x8"
group = "web"
design = { bolts = { diameter = "16 mm", count = 2 } }
"#;

pub(crate) fn library() -> SectionLibrary {
    SectionLibrary::parse(LIBRARY).unwrap()
}

/// 30 kN down at E.
pub(crate) fn table(force: f64) -> LoadTable {
    LoadTable {
        rows: vec![LoadRow {
            case: "down".into(),
            load: None,
            at: At::Joint,
            label: "E".into(),
            force: [0.0, 0.0, force],
            source: Source::Typed,
            evaluated: None,
            missing: None,
            note: String::new(),
            area: None,
            moment: [0.0; 3],
        }],
    }
}

pub(crate) fn cases() -> Vec<CaseToRun> {
    vec![CaseToRun {
        name: "down".into(),
        strength: 1.0,
        self_weight: 1.0,
    }]
}

fn size(text: &str, settings: &Settings) -> sizing::Sizing {
    let t = StructureType::parse(std::path::Path::new("t.ohls"), text).unwrap();
    let lib = library();
    let table = table(30e3);
    let cases = cases();
    sizing::size(
        &SizingInput {
            structure: &t,
            text,
            dir: None,
            sections: &lib,
            table: &table,
            recipes: &[],
            cases: &cases,
            settings,
        },
        &ohl_core::budget::Budget::new(),
    )
    .unwrap()
}

/// The usage of each member of `text` (one full run), by id.
fn usage_of(text: &str) -> std::collections::HashMap<String, f64> {
    let t = StructureType::parse(std::path::Path::new("t.ohls"), text).unwrap();
    let lib = library();
    let table = table(30e3);
    let cases = cases();
    let run = crate::analyse_structure(
        &crate::RunInput {
            structure: &t,
            text,
            dir: None,
            sections: &lib,
            table: &table,
            recipes: &[],
            cases: &cases,
            extension: None,
            bolts: None,
            feet: None,
        },
        &ohl_structures_fe::default_budget(),
    )
    .unwrap();
    let Some(Ok(r)) = &run.cases[0].fe else {
        panic!("not solved")
    };
    r.members
        .iter()
        .map(|m| (m.id.clone(), m.usage.unwrap()))
        .collect()
}

/// The three groups by their `group` (one section each).
#[test]
fn groups_by_group_and_section() {
    let t = StructureType::parse(std::path::Path::new("t.ohls"), TRUSS).unwrap();
    let model = crate::model_of(&t, TRUSS, None, &library()).unwrap();
    let g = sizing::sizing_groups(&t, &model, &[]);
    let names: Vec<&str> = g.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(names, ["bottom", "top", "web"]);
    assert_eq!(g[2].members, ["CD", "BC", "DE"]);
    // Split: each member alone, named with its group.
    let g = sizing::sizing_groups(&t, &model, &["web".into()]);
    assert_eq!(g.len(), 5);
    assert!(g.iter().any(|g| g.name == "web · BC" && g.members == ["BC"]));
    // Two sections in one group: two groups.
    let two = TRUSS.replacen(
        "id = \"CE\"\nnodes = [\"C\", \"E\"]\nsection = \"L80x8\"",
        "id = \"CE\"\nnodes = [\"C\", \"E\"]\nsection = \"L90x9\"",
        1,
    );
    let t = StructureType::parse(std::path::Path::new("t.ohls"), &two).unwrap();
    let g = sizing::sizing_groups(&t, &model, &[]);
    assert!(g.iter().any(|g| g.name == "bottom · L80x8" && g.members == ["AC"]));
    assert!(g.iter().any(|g| g.name == "bottom · L90x9" && g.members == ["CE"]));
}

/// Each group's choice passes, and the next lighter section of its family
/// fails its check or its joint fit: the lightest passing. The 40 mm legs
/// pass the check of BD (30 kN tension: 307 mm² × 355 MPa = 109 kN) but
/// not the joint fit (20 mm to the toe, 24 mm needed): L45×4 instead,
/// said with the limits' effect.
#[test]
fn the_lightest_passing_section_per_group() {
    let s = size(TRUSS, &Settings::default());
    assert_eq!(s.settled, sizing::Settled::Settled);
    let by = |n: &str| s.groups.iter().find(|g| g.group.name == n).unwrap();
    let top = by("top");
    assert_eq!(top.to, "L45x4", "{:#?}", s.groups);
    assert_eq!(top.without_fit.as_deref(), Some("L40x4"));
    assert!(s.fit_effect.contains("kept 1 group(s) heavier"), "{}", s.fit_effect);
    let order = ["L40x4", "L45x4", "L50x5", "L60x6", "L70x7", "L80x8", "L90x9", "L100x10"];
    for g in &s.groups {
        assert!(g.changed(), "{g:?}");
        assert!(g.usage[1].unwrap() <= 1.0, "{g:?}");
        assert!(!g.over_target);
        assert!(g.mass[1] < g.mass[0]);
        assert_eq!(g.fit[1], 0, "{g:?}");
        let at = order.iter().position(|x| *x == g.to).unwrap();
        if at > 1 {
            // The next lighter fails the check (no joint-fit problem from
            // 45 mm legs up).
            let lighter = sizing::apply(
                TRUSS,
                &sizing::Sizing {
                    groups: vec![sizing::GroupChange {
                        to: order[at - 1].into(),
                        ..g.clone()
                    }],
                    ..s.clone()
                },
                &[g.group.name.clone()],
            )
            .unwrap();
            let u = usage_of(&lighter);
            assert!(
                g.group.members.iter().any(|m| u[m] > 1.0),
                "{}: {} passes: {u:?}",
                g.group.name,
                order[at - 1]
            );
        }
    }
    assert!(s.mass[1] < s.mass[0]);
    assert!(s.over_target.is_empty());
    // The core's simple check: NOT ON THE SAFE SIDE, said; no code limits.
    assert!(s.notes.iter().any(|n| n.starts_with("NOT ON THE SAFE SIDE: sized by the core's simple check")));
    assert!(s.notes.iter().any(|n| n.contains("limits on the members' sizes") && n.contains("not checked")));
    // AC's design thickness and w/t described the L80×8: removed, said.
    assert!(by("bottom").removed.iter().any(|r| r.contains("thickness")));
}

/// A lower target: heavier choices, every usage within it; the joint-fit
/// limits off: the 40 mm leg for BD, said.
#[test]
fn settings_change_the_choice() {
    let tight = size(
        TRUSS,
        &Settings {
            target: 0.5,
            ..Settings::default()
        },
    );
    let normal = size(TRUSS, &Settings::default());
    assert!(tight.mass[1] > normal.mass[1]);
    for g in &tight.groups {
        assert!(g.usage[1].unwrap() <= 0.5, "{g:?}");
    }
    let loose = size(
        TRUSS,
        &Settings {
            joint_fit: false,
            ..Settings::default()
        },
    );
    let top = loose.groups.iter().find(|g| g.group.name == "top").unwrap();
    assert_eq!(top.to, "L40x4");
    assert_eq!(top.fit[1], 2, "{top:?}");
    assert!(loose.notes.iter().any(|n| n.starts_with("joint-fit limits off")));
    // Kept: left as it is.
    let kept = size(
        TRUSS,
        &Settings {
            keep: vec!["web".into()],
            ..Settings::default()
        },
    );
    let web = kept.groups.iter().find(|g| g.group.name == "web").unwrap();
    assert!(!web.changed() && web.not_sized);
    assert_eq!(web.why, "left as it is (asked)");
}

/// With the members' own weight the forces change with the sections: the
/// loop takes rounds and settles; the final run's usage is the full run's.
#[test]
fn the_loop_settles_with_self_weight() {
    let heavy = TRUSS.replace("self_weight = false", "self_weight = true");
    let s = size(&heavy, &Settings::default());
    assert_eq!(s.settled, sizing::Settled::Settled);
    assert!(s.rounds.len() >= 2, "{:?}", s.rounds);
    let chosen = sizing::apply(
        &heavy,
        &s,
        &s.groups.iter().map(|g| g.group.name.clone()).collect::<Vec<_>>(),
    )
    .unwrap();
    assert_eq!(chosen, s.text);
    let t = StructureType::parse(std::path::Path::new("t.ohls"), &chosen).unwrap();
    assert!(t.members.iter().all(|m| m.section.as_deref() != Some("L80x8")));
}

/// Apply takes only the groups ticked (one edit), and refuses a file whose
/// member is gone.
#[test]
fn apply_takes_the_ticked_groups_only() {
    let s = size(TRUSS, &Settings::default());
    let text = sizing::apply(TRUSS, &s, &["top".into()]).unwrap();
    let t = StructureType::parse(std::path::Path::new("t.ohls"), &text).unwrap();
    for m in &t.members {
        let want = if m.id == "BD" { "L45x4" } else { "L80x8" };
        assert_eq!(m.section.as_deref(), Some(want), "{}", m.id);
    }
    // AC kept: its design data untouched.
    assert!(text.contains("width_thickness = 8.9"));
    let all: Vec<String> = s.groups.iter().map(|g| g.group.name.clone()).collect();
    let text = sizing::apply(TRUSS, &s, &all).unwrap();
    assert!(!text.contains("width_thickness"));
    assert!(text.contains("bolts = { diameter = \"16 mm\", count = 2 }"));
    let gone = TRUSS.replace("id = \"BD\"", "id = \"BX\"");
    assert!(sizing::apply(&gone, &s, &["top".into()]).unwrap_err().contains("BD"));
}

/// The joint fit by hand: a 40 mm leg with a 16 mm bolt — 20 mm to the
/// toe, below the default 1.5 d = 24 mm; a 16 mm bolt's hole in an L40×4
/// is clear of its root (4 + 5 + 8 = 17 mm ≤ 20 mm). A 2 m member with
/// two ends of one bolt needs 4 × 24 mm: long enough; a 0.08 m one not.
#[test]
fn joint_fit_by_hand() {
    let small = TRUSS.replace("L80x8", "L40x4");
    let t = StructureType::parse(std::path::Path::new("t.ohls"), &small).unwrap();
    let model = crate::model_of(&t, &small, None, &library()).unwrap();
    let (p, unknown) = sizing::joint_fit(&model, &[0], &[0.0; 6]);
    assert_eq!(unknown, 0);
    assert_eq!(p.len(), 2, "{p:?}");
    assert!(p[0].words.contains("20 mm to the toe at mid-leg, below the code's edge distance 24 mm"), "{p:?}");
    let short = small.replace("point = [\"2 m\", \"0 m\", \"0 m\"]", "point = [\"0.08 m\", \"0 m\", \"0 m\"]");
    let t = StructureType::parse(std::path::Path::new("t.ohls"), &short).unwrap();
    let model = crate::model_of(&t, &short, None, &library()).unwrap();
    let (p, _) = sizing::joint_fit(&model, &[0], &[0.0; 6]);
    assert!(p.iter().any(|x| x.rule == sizing::FitRule::Length && x.words.contains("need 96 mm")), "{p:?}");
}

/// A section not in the library: its group left as it is, said.
#[test]
fn a_section_outside_the_library_is_left() {
    let own = TRUSS.replace("group = \"top\"", "group = \"top\"").replacen(
        "id = \"BD\"\nnodes = [\"B\", \"D\"]\nsection = \"L80x8\"",
        "id = \"BD\"\nnodes = [\"B\", \"D\"]\nsection = \"own\"",
        1,
    ) + "\n[[section]]\nid = \"own\"\narea = \"10 cm2\"\niy = \"50 cm4\"\niz = \"50 cm4\"\ni_min = \"20 cm4\"\nmaterial = \"m\"\n[[material]]\nid = \"m\"\nelasticity = \"210 GPa\"\nyield = \"355 MPa\"\n";
    let s = size(&own, &Settings::default());
    let top = s.groups.iter().find(|g| g.group.name == "top").unwrap();
    assert!(top.not_sized && !top.changed());
    assert!(top.why.contains("not in the project's section library"), "{}", top.why);
}

/// The heavier of a cycle per group.
#[test]
fn a_cycle_takes_the_heavier() {
    let lib = library();
    let h = sizing::heavier(
        &lib,
        &[
            vec!["L50x5".into(), "L70x7".into()],
            vec!["L60x6".into(), "L45x4".into()],
        ],
    );
    assert_eq!(h, ["L60x6", "L70x7"]);
}

/// The summary names each change and every member above the target.
#[test]
fn summary_in_lines() {
    let s = size(TRUSS, &Settings::default());
    let lines = sizing::summary(&s);
    assert!(lines[0].starts_with("3 of 3 group(s) changed"), "{}", lines[0]);
    assert!(lines.iter().any(|l| l.starts_with("* top (1 member(s)): L80x8 → L45x4")), "{lines:#?}");
}
