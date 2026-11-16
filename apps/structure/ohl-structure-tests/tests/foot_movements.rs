// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A placed structure's measured foot movements in OHL-Structure (owner
//! 2026-11-16): the line-values file carries them (version 11, read back
//! the same; an older file has none); the run solves the structure on its
//! moved feet — a portal frame on two fixed feet, one settled, is
//! strained: each member's usage the larger of with and without them,
//! said.

use std::path::Path;

use ohl_structure_link::{RunInput, SectionAreas, analyse_structure, cases_to_run};
use ohl_structure_loads::{
    LineValues, PlacedFeet, PlacedFoot, PlacedValues, StructureLoadCases, evaluate_with_areas,
};
use ohl_structures::{LibraryItem, StructureType};
use ohl_structures_fe::default_budget;

/// A 6 m by 6 m portal of tubes on two fixed feet, a pull across at the
/// top.
const PORTAL: &str = r#"
format = "ohl-structure"
version = 3
id = "portal"
name = "portal"
kind = "other"

[[point]]
label = "A1"
node = "T1"

[[node]]
id = "F1"
point = ["0 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "F2"
point = ["6 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "T1"
point = ["0 m", "0 m", "6 m"]

[[node]]
id = "T2"
point = ["6 m", "0 m", "6 m"]

[[member]]
id = "c1"
nodes = ["F1", "T1"]
section = "CHS"
kind = "frame"

[[member]]
id = "c2"
nodes = ["F2", "T2"]
section = "CHS"
kind = "frame"

[[member]]
id = "b"
nodes = ["T1", "T2"]
section = "CHS"
kind = "frame"

[[load_case]]
name = "pull"
self_weight = 0

[[load_case.load]]
value = "1 kN"
direction = "x"
points = ["A1"]
"#;

const TUBE: &str = "[[material]]\nid = \"steel\"\nelasticity = \"210 GPa\"\n\
    density = \"7850 kg/m3\"\nyield = \"355 MPa\"\n\
    [[section]]\nid = \"CHS\"\nshape = \"circular_hollow\"\nmaterial = \"steel\"\n\
    d = \"200 mm\"\nt = \"10 mm\"\n";

fn feet() -> PlacedFeet {
    PlacedFeet {
        source: "survey".into(),
        date: "2026-05-12".into(),
        feet: vec![PlacedFoot {
            joint: "F2".into(),
            translation: [0.0, 0.0, -0.02],
            tilt_deg: [0.0, 0.0],
        }],
    }
}

#[test]
fn the_line_values_carry_the_feet() {
    let mut v = PlacedValues::pending("T2", "line", "portal", "");
    assert!(v.needed_version() < 11);
    v.feet = Some(feet());
    let v = v.at_needed_version();
    assert_eq!(v.version, 11);
    let back = PlacedValues::parse(&v.to_json()).unwrap();
    assert_eq!(back.feet, v.feet);
    assert!(v.to_json().contains("\"feet\""));
    assert_eq!(
        feet().feet[0].describe(),
        "F2: across 0.0, ahead 0.0, up -20.0 mm"
    );
}

#[test]
fn the_run_stands_on_the_moved_feet() {
    let lib = ohl_sections::SectionLibrary::parse(TUBE).unwrap();
    let t = StructureType::parse(Path::new("s.ohls"), PORTAL).unwrap();
    let recipes = StructureLoadCases::from_structure(&t).unwrap();
    let areas = SectionAreas::new(&t, PORTAL, None, &lib);
    let table = evaluate_with_areas(&recipes, &t, &LineValues::default(), &areas);
    let cases = cases_to_run(&table, &recipes, None);
    let f = feet();
    let run = |feet: Option<&PlacedFeet>| {
        analyse_structure(
            &RunInput {
                structure: &t,
                text: PORTAL,
                dir: None,
                sections: &lib,
                table: &table,
                recipes: &recipes,
                cases: &cases,
                extension: None,
                bolts: None,
                feet,
            },
            &default_budget(),
        )
        .unwrap()
    };
    let plain = run(None);
    let moved = run(Some(&f));
    let a = plain.cases[0].fe.as_ref().unwrap().as_ref().unwrap();
    let b = moved.cases[0].fe.as_ref().unwrap().as_ref().unwrap();
    assert!(
        moved
            .notes
            .iter()
            .any(|n| n.contains("foot movements from the line")),
        "{:?}",
        moved.notes
    );
    assert!(
        b.notes.iter().any(|n| n.contains("on its moved feet")),
        "{:?}",
        b.notes
    );
    // Strained: the beam bends (the columns turning with it: less than
    // a fixed-ended beam's 12EIδ/L³), the settled foot's reaction changes.
    let ra = a.reactions.iter().find(|r| r.joint == "F2").unwrap();
    let rb = b.reactions.iter().find(|r| r.joint == "F2").unwrap();
    assert!((ra.force[2] - rb.force[2]).abs() > 500.0, "{ra:?} {rb:?}");
    // Each member's usage never below its usage without.
    let mut higher = false;
    for (x, y) in a.members.iter().zip(&b.members) {
        let (Some(u), Some(w)) = (x.usage, y.usage) else {
            continue;
        };
        assert!(w >= u - 1e-12, "{}: {w} below {u}", x.id);
        higher |= w > u + 1e-9;
    }
    assert!(higher);
}
