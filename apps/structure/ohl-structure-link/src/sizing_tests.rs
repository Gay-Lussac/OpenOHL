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
//! to the toe at mid-leg and a 45 mm one 22.5 mm: neither takes them).

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
    assert!(
        g.iter()
            .any(|g| g.name == "web · BC" && g.members == ["BC"])
    );
    // Two sections in one group: two groups.
    let two = TRUSS.replacen(
        "id = \"CE\"\nnodes = [\"C\", \"E\"]\nsection = \"L80x8\"",
        "id = \"CE\"\nnodes = [\"C\", \"E\"]\nsection = \"L90x9\"",
        1,
    );
    let t = StructureType::parse(std::path::Path::new("t.ohls"), &two).unwrap();
    let g = sizing::sizing_groups(&t, &model, &[]);
    assert!(
        g.iter()
            .any(|g| g.name == "bottom · L80x8" && g.members == ["AC"])
    );
    assert!(
        g.iter()
            .any(|g| g.name == "bottom · L90x9" && g.members == ["CE"])
    );
}

/// Each group's choice passes, and the next lighter section of its family
/// fails its check or its joint fit: the lightest passing. The 40 mm legs
/// pass the check of BD (30 kN tension: 307 mm² × 355 MPa = 109 kN) but
/// not the joint fit (20 mm to the toe, 24 mm needed; the 45 mm legs 22.5
/// mm): L50×5 instead, said with the limits' effect.
#[test]
fn the_lightest_passing_section_per_group() {
    let s = size(TRUSS, &Settings::default());
    assert_eq!(s.settled, sizing::Settled::Settled);
    let by = |n: &str| s.groups.iter().find(|g| g.group.name == n).unwrap();
    let top = by("top");
    assert_eq!(top.to, "L50x5", "{:#?}", s.groups);
    assert_eq!(top.without_fit.as_deref(), Some("L40x4"));
    assert!(
        s.fit_effect.contains("kept 1 group(s) heavier"),
        "{}",
        s.fit_effect
    );
    let order = [
        "L40x4", "L45x4", "L50x5", "L60x6", "L70x7", "L80x8", "L90x9", "L100x10",
    ];
    for g in &s.groups {
        assert!(g.changed(), "{g:?}");
        assert!(g.usage[1].unwrap() <= 1.0, "{g:?}");
        assert!(!g.over_target);
        assert!(g.mass[1] < g.mass[0]);
        assert_eq!(g.fit[1], 0, "{g:?}");
        let at = order.iter().position(|x| *x == g.to).unwrap();
        if at > 0 && g.without_fit.is_none() {
            // The next lighter fails the check (no joint-fit problem from
            // 50 mm legs up).
            let lighter = sizing::apply(
                TRUSS,
                &sizing::Sizing {
                    groups: vec![sizing::GroupChange {
                        to: order[at - 1].into(),
                        ..g.clone()
                    }],
                    ..s.clone()
                },
                std::slice::from_ref(&g.group.name),
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
    assert!(
        s.notes
            .iter()
            .any(|n| n.starts_with("NOT ON THE SAFE SIDE: sized by the core's simple check"))
    );
    assert!(
        s.notes
            .iter()
            .any(|n| n.contains("limits on the members' sizes") && n.contains("not checked"))
    );
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
    assert!(
        loose
            .notes
            .iter()
            .any(|n| n.starts_with("joint-fit limits off"))
    );
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
        &s.groups
            .iter()
            .map(|g| g.group.name.clone())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert_eq!(chosen, s.text);
    let t = StructureType::parse(std::path::Path::new("t.ohls"), &chosen).unwrap();
    assert!(
        t.members
            .iter()
            .all(|m| m.section.as_deref() != Some("L80x8"))
    );
}

/// Apply takes only the groups ticked (one edit), and refuses a file whose
/// member is gone.
#[test]
fn apply_takes_the_ticked_groups_only() {
    let s = size(TRUSS, &Settings::default());
    let text = sizing::apply(TRUSS, &s, &["top".into()]).unwrap();
    let t = StructureType::parse(std::path::Path::new("t.ohls"), &text).unwrap();
    for m in &t.members {
        let want = if m.id == "BD" { "L50x5" } else { "L80x8" };
        assert_eq!(m.section.as_deref(), Some(want), "{}", m.id);
    }
    // AC kept: its design data untouched.
    assert!(text.contains("width_thickness = 8.9"));
    let all: Vec<String> = s.groups.iter().map(|g| g.group.name.clone()).collect();
    let text = sizing::apply(TRUSS, &s, &all).unwrap();
    assert!(!text.contains("width_thickness"));
    assert!(text.contains("bolts = { diameter = \"16 mm\", count = 2 }"));
    let gone = TRUSS.replace("id = \"BD\"", "id = \"BX\"");
    assert!(
        sizing::apply(&gone, &s, &["top".into()])
            .unwrap_err()
            .contains("BD")
    );
}

/// The joint fit by hand: a 40 mm leg with a 16 mm bolt — 20 mm to the
/// toe, below the default 1.5 d = 24 mm; a 16 mm bolt's hole in an L40×4
/// is clear of its root (4 + 5 + 8 = 17 mm ≤ 20 mm). AC goes on in line
/// through C as CE: of one section, one part — its end at C holds no bolts
/// (only A's checked); of two sections, a splice — both ends checked. A
/// member with two bolts at an end needs 2 × 24 + 48 = 96 mm there: a 2 m
/// member is long enough, a 0.08 m one not (96 mm with one end, 192 mm
/// with both).
#[test]
fn joint_fit_by_hand() {
    let fit = |text: &str| {
        let t = StructureType::parse(std::path::Path::new("t.ohls"), text).unwrap();
        let model = crate::model_of(&t, text, None, &library()).unwrap();
        sizing::joint_fit(&model, &[0], &[0.0; 6], &sizing::FitRules::default())
    };
    let small = TRUSS.replace("L80x8", "L40x4");
    let (p, unknown) = fit(&small);
    assert_eq!(unknown, 0);
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(
        p[0].words
            .contains("AC at A: 20 mm to the toe at mid-leg, below the code's edge distance 24 mm"),
        "{p:?}"
    );
    let spliced = small.replacen(
        "id = \"CE\"\nnodes = [\"C\", \"E\"]\nsection = \"L40x4\"",
        "id = \"CE\"\nnodes = [\"C\", \"E\"]\nsection = \"L45x4\"",
        1,
    );
    let (p, _) = fit(&spliced);
    assert_eq!(p.len(), 2, "{p:?}");
    let short = |text: &str| {
        text.replace(
            "point = [\"2 m\", \"0 m\", \"0 m\"]",
            "point = [\"0.08 m\", \"0 m\", \"0 m\"]",
        )
    };
    let length = |p: &[sizing::FitProblem], need: &str| {
        p.iter()
            .any(|x| x.rule == sizing::FitRule::Length && x.words.contains(need))
    };
    let (p, _) = fit(&short(&small));
    assert!(length(&p, "need 96 mm"), "{p:?}");
    let (p, _) = fit(&short(&spliced));
    assert!(length(&p, "need 192 mm"), "{p:?}");
}

/// A section not in the library: its group left as it is, said.
#[test]
fn a_section_outside_the_library_is_left() {
    let own = TRUSS.replacen(
        "id = \"BD\"\nnodes = [\"B\", \"D\"]\nsection = \"L80x8\"",
        "id = \"BD\"\nnodes = [\"B\", \"D\"]\nsection = \"own\"",
        1,
    ) + "\n[[section]]\nid = \"own\"\narea = \"10 cm2\"\niy = \"50 cm4\"\niz = \"50 cm4\"\ni_min = \"20 cm4\"\nmaterial = \"m\"\n[[material]]\nid = \"m\"\nelasticity = \"210 GPa\"\nyield = \"355 MPa\"\n";
    let s = size(&own, &Settings::default());
    let top = s.groups.iter().find(|g| g.group.name == "top").unwrap();
    assert!(top.not_sized && !top.changed());
    assert!(
        top.why.contains("not in the project's section library"),
        "{}",
        top.why
    );
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
    assert!(
        lines[0].starts_with("3 of 3 group(s) changed"),
        "{}",
        lines[0]
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("* top (1 member(s)): L80x8 → L50x5")),
        "{lines:#?}"
    );
}

/// The detailing practice's plate thicknesses (O44 L29) as joint-fit
/// limits: the shop makes plates up to 5 mm only, so a member with 6 mm
/// legs or more cannot go onto a plate. The bottom chord needs an L60×6 at
/// least for its check (AC: 60 kN in compression over 2 m; an L50×5 buckles
/// at about 24 kN): no section of its family both passes and fits — the
/// lightest passing taken, its problems said; the top chord (an L50×5)
/// fits as before.
#[test]
fn the_practice_plates_limit_the_choice() {
    let text = format!("{TRUSS}\n[layout_practice.plates]\nthicknesses = [\"4 mm\", \"5 mm\"]\n");
    let s = size(&text, &Settings::default());
    let free = size(TRUSS, &Settings::default());
    let by =
        |s: &sizing::Sizing, n: &str| s.groups.iter().find(|g| g.group.name == n).unwrap().clone();
    let bottom = by(&s, "bottom");
    assert_eq!(bottom.basis, sizing::Basis::FitNotMet, "{bottom:?}");
    assert_eq!(bottom.to, by(&free, "bottom").to);
    assert!(bottom.fit[1] > 0, "{bottom:?}");
    assert_eq!(by(&s, "top").to, "L50x5");
    assert!(
        s.notes
            .iter()
            .any(|n| n.contains("legs thicker than every plate the detailing practice lists")),
        "{:?}",
        s.notes
    );
    assert!(
        s.notes
            .iter()
            .any(|n| n.contains("joint fit by the detailing practice of this structure file's own")),
        "{:?}",
        s.notes
    );
    // Without the practice: the default, said.
    assert!(
        free.notes
            .iter()
            .any(|n| n == "joint fit by the default detailing practice"),
        "{:?}",
        free.notes
    );
}

/// The practice's bolts per end for ends without bolts of their own: per
/// member class, else its picked bolt's count (the layout's own values).
#[test]
fn the_practice_bolt_counts() {
    let t: toml::Table = "[bolts]\ncount = 3\n[classes.bracing.bolts]\ncount = 4\n"
        .parse()
        .unwrap();
    let p = crate::practice::in_force(Some(&t), None).unwrap();
    let r = sizing::FitRules::of(p.as_ref(), None);
    assert_eq!(r.picked_count(Some("bracing")), 4);
    assert_eq!(r.picked_count(Some("leg")), 3);
    assert_eq!(r.picked_count(None), 3);
    assert!(r.has_rules());
    let d = sizing::FitRules::default();
    assert_eq!(d.picked_count(Some("bracing")), 2);
    assert!(!d.has_rules());
}

/// Capacities written by hand describe their section: the group is left
/// as it is, said; a structure analysed from a model file of its own and a
/// target above 100 % are refused.
#[test]
fn what_the_sizer_leaves_and_refuses() {
    let hand = TRUSS.replace(
        "id = \"BD\"\nnodes = [\"B\", \"D\"]",
        "id = \"BD\"\nnodes = [\"B\", \"D\"]\ncapacity_tension = \"100 kN\"",
    );
    let s = size(&hand, &Settings::default());
    let top = s.groups.iter().find(|g| g.group.name == "top").unwrap();
    assert!(top.not_sized && !top.changed(), "{top:?}");
    assert!(
        top.why.contains("capacities written by hand"),
        "{}",
        top.why
    );
    let run = |text: &str, settings: &Settings| {
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
    };
    let over = run(
        TRUSS,
        &Settings {
            target: 1.2,
            ..Settings::default()
        },
    )
    .unwrap_err();
    assert!(over.contains("at most 100 %"), "{over}");
    let own = TRUSS.replace(
        "kind = \"lattice\"",
        "kind = \"lattice\"\nmodel = \"t.ohlfe\"",
    );
    let e = run(&own, &Settings::default()).unwrap_err();
    assert!(e.contains("finite-element model file (t.ohlfe)"), "{e}");
}

/// O49 stage 1b (2026-11-27): the bolts govern. The truss checked by the
/// bundled ASCE 10-97 plugin, its bolts given 10 kN per plane: AC's 60 kN
/// needs six (20 kN with its two), more than any section gives — the
/// group is not made heavier for it, its bolts are proposed (more of its
/// own: no bolt library beside it), within the joint-fit limits; the run
/// with them passes; Apply writes them with the sections; a group not
/// ticked keeps its bolts.
#[test]
fn the_bolts_govern() {
    let text = TRUSS
        .replace("capacity = \"simple\"", "capacity = \"asce10-97\"")
        .replace(
            "bolts = { diameter = \"16 mm\", count = 2 }",
            "bolts = { diameter = \"16 mm\", count = 2, shear_capacity = \"10 kN\" }",
        );
    let s = size(&text, &Settings::default());
    let bottom = s.groups.iter().find(|g| g.group.name == "bottom").unwrap();
    let b = bottom.bolts.first().expect("bolts proposed");
    assert!(b.pick.count >= 6, "{b:?}");
    assert_eq!(b.from, "2 × 16 mm");
    assert!(
        bottom
            .bolts_govern
            .as_ref()
            .is_some_and(|w| w.starts_with("the bolts govern (bolt shear")),
        "{bottom:?}"
    );
    assert!(s.over_target.is_empty(), "{:?}", s.over_target);
    assert!(
        s.notes
            .iter()
            .any(|n| n.contains("group(s) where the bolts govern")),
        "{:?}",
        s.notes
    );
    assert!(
        sizing::summary(&s)
            .iter()
            .any(|l| l.starts_with("    bolts 2 × 16 mm → ")),
        "{:?}",
        sizing::summary(&s)
    );
    let applied = sizing::apply(&text, &s, std::slice::from_ref(&bottom.group.name)).unwrap();
    let t = StructureType::parse(std::path::Path::new("t.ohls"), &applied).unwrap();
    let count = |id: &str| {
        let m = crate::model_of(&t, &applied, None, &library()).unwrap();
        m.members
            .iter()
            .find(|m| m.id == id)
            .and_then(|m| m.design.bolts)
            .map(|b| b.count)
    };
    assert_eq!(count("AC"), Some(b.pick.count));
    assert_eq!(count("BD"), Some(2));
}

/// O49 stage 1b: a larger bolt of the library where more of the group's
/// own do not fit; the count by the per-plane capacities.
#[test]
fn a_larger_bolt_by_its_capacity() {
    use crate::sizing_bolts::{BoltPick, count_for};
    let m16 = BoltPick {
        count: 6,
        diameter: 0.016,
        hole: None,
        shear_capacity: Some(60e3),
        bolt: None,
        grade: None,
    };
    let m20 = BoltPick {
        count: 2,
        diameter: 0.020,
        hole: Some(0.022),
        shear_capacity: Some(94e3),
        bolt: Some("M20".into()),
        grade: Some("8.8".into()),
    };
    assert_eq!(count_for(&m16, 6, &m20), 4);
    let m20_bare = BoltPick {
        shear_capacity: None,
        ..m20.clone()
    };
    // By the areas: 6 × (16/20)² = 3.84.
    assert_eq!(count_for(&m16, 6, &m20_bare), 4);
    assert_eq!(m20.words(), "2 × 20 mm (M20)");
}

/// O49 stage 1c (the owner's answer of 2026-11-27): a proposal per bolt
/// size within a group. The bottom group's AC bolted by 16 mm, CE by 20 mm,
/// both at 10 kN per plane (the ASCE 10-97 plugin): each size its own
/// proposal, more of its own bolts; stage 1b gave none (the sizes differ).
#[test]
fn a_proposal_per_bolt_size_within_a_group() {
    let text = TRUSS
        .replace("capacity = \"simple\"", "capacity = \"asce10-97\"")
        .replace(
            "bolts = { diameter = \"16 mm\", count = 2 }",
            "bolts = { diameter = \"16 mm\", count = 2, shear_capacity = \"10 kN\" }",
        )
        .replacen(
            "design = { bolts = { diameter = \"16 mm\", count = 2, shear_capacity = \"10 kN\" } }",
            "design = { bolts = { diameter = \"20 mm\", count = 2, shear_capacity = \"10 kN\" } }",
            1,
        );
    let s = size(&text, &Settings::default());
    let bottom = s.groups.iter().find(|g| g.group.name == "bottom").unwrap();
    assert_eq!(bottom.bolts.len(), 2, "{:?}", bottom.bolts);
    let by = |d: &str| {
        bottom
            .bolts
            .iter()
            .find(|b| b.from.contains(d))
            .unwrap_or_else(|| panic!("{d}: {:?}", bottom.bolts))
    };
    assert_eq!(by("16 mm").members, vec!["AC".to_owned()]);
    assert_eq!(by("20 mm").members, vec!["CE".to_owned()]);
    assert!(by("16 mm").pick.count >= 6 && by("16 mm").pick.diameter == 0.016);
    assert!(s.over_target.is_empty(), "{:?}", s.over_target);
    // No larger bolt was asked: every joint keeps its sizes.
    assert_eq!(s.joint_sizes.taken_along, 0);
    assert_eq!(s.joint_sizes.mixed[0], s.joint_sizes.mixed[1]);
    let applied = sizing::apply(&text, &s, std::slice::from_ref(&bottom.group.name)).unwrap();
    assert!(applied.contains("diameter = \"20 mm\""), "{applied}");
}

/// O49 stage 1c (the owner's answer of 2026-11-27): the bolts govern and no
/// bolt proposal is possible (the bottom members' ends laid out, their
/// bolts given there): the group keeps its current section, said — not the
/// lightest of equally failing sections (stage 1's "least usage").
#[test]
fn bolts_governing_without_a_proposal_keep_the_section() {
    let mut text = TRUSS
        .replace("capacity = \"simple\"", "capacity = \"asce10-97\"")
        .replace(
            "bolts = { diameter = \"16 mm\", count = 2 }",
            "bolts = { diameter = \"16 mm\", count = 2, shear_capacity = \"1 kN\" }",
        );
    text = text.replacen(
        "kind = \"lattice\"",
        "kind = \"lattice\"\njoint_details = true",
        1,
    );
    for (m, j) in [
        ("AC", "A"),
        ("AC", "C"),
        ("CE", "C"),
        ("CE", "E"),
        ("BD", "B"),
        ("BD", "D"),
        ("CD", "C"),
        ("CD", "D"),
        ("BC", "B"),
        ("BC", "C"),
        ("DE", "D"),
        ("DE", "E"),
    ] {
        text.push_str(&format!(
            "\n[[connection]]\nmember = \"{m}\"\njoint = \"{j}\"\nbolts = {{ count = 2, \
             diameter = \"16 mm\" }}\n"
        ));
    }
    let t = StructureType::parse(std::path::Path::new("t.ohls"), &text).unwrap();
    assert_eq!(t.joint_details_not_applied(), None);
    let s = size(&text, &Settings::default());
    let bottom = s.groups.iter().find(|g| g.group.name == "bottom").unwrap();
    assert_eq!(bottom.basis, sizing::Basis::BoltsKept, "{bottom:?}");
    assert_eq!(bottom.to, bottom.from, "{bottom:?}");
    assert!(
        bottom.why.contains("current section kept"),
        "{}",
        bottom.why
    );
    assert!(bottom.bolts.is_empty());
    assert!(
        bottom
            .bolts_govern
            .as_ref()
            .is_some_and(|w| w.contains("no bolt proposal")),
        "{bottom:?}"
    );
}

/// O49 stage 1c (the owner, 2026-11-27): one bolt size per joint. AC goes
/// from 16 mm to 20 mm: the 16 mm bolts at its joints go with it, and on
/// through the joints those reach that had 16 mm alone (the whole truss);
/// CD kept (its leg does not take it): its joint C stops it — said, and
/// nothing is taken through C.
#[test]
fn one_bolt_size_per_joint() {
    use crate::sizing_bolts::{BoltPick, JointSizes, same_size, size_key};
    let t = StructureType::parse(std::path::Path::new("t.ohls"), TRUSS).unwrap();
    let model = crate::model_of(&t, TRUSS, None, &library()).unwrap();
    let place = |id: &str| model.members.iter().position(|m| m.id == id).unwrap();
    let joints = JointSizes::of(&model, &t, &Default::default());
    let m20 = BoltPick {
        count: 2,
        diameter: 0.020,
        hole: None,
        shear_capacity: None,
        bolt: None,
        grade: None,
    };
    let old = size_key(0.016, None, None);
    let none = vec![None; model.members.len()];
    let (taken, stops) = same_size(
        &model,
        &joints,
        &[place("AC")],
        &old,
        &m20,
        &none,
        &Default::default(),
    );
    let mut ids: Vec<&str> = taken
        .iter()
        .map(|(k, _)| model.members[*k].id.as_str())
        .collect();
    ids.sort();
    assert_eq!(ids, ["BC", "BD", "CD", "CE", "DE"], "{stops:?}");
    assert!(stops.is_empty(), "{stops:?}");
    let frozen: std::collections::HashMap<usize, String> =
        [(place("CD"), "its leg does not take it".to_owned())].into();
    let (taken, stops) = same_size(&model, &joints, &[place("AC")], &old, &m20, &none, &frozen);
    assert!(taken.is_empty(), "{taken:?}");
    assert!(
        stops
            .iter()
            .any(|(j, w)| j == "C" && w.contains("CD keeps its 16 mm")),
        "{stops:?}"
    );
    let (single, mixed) = joints.count(&model, &none);
    assert_eq!((single, mixed.len()), (5, 0));
}
