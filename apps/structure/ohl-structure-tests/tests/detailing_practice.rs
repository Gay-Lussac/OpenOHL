// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The joint layout's detailing practice (O44 L24) run through the link as
//! OHL-Structure runs it, on an invented K-brace (two diagonals meeting a
//! leg at N2, two horizontals at its foot, the example sections; diagonal
//! D1 turned so that its leg lies off the flange): the default practice
//! given as the structure file's own `[layout_practice]` lays the tower out exactly as no practice
//! does — the same plans, the same file but for the record's practice
//! table, the same estimated eccentricities; a shop that makes neither
//! bent nor welded plates gets no plate in a member's own plane and never
//! more ends laid out, the end left so said.

use std::path::Path;

use ohl_structure_link::eccentricities::{EstimateAsk, estimate};
use ohl_structure_link::{
    BoltArea, DetailingPractice, Fabrication, LayoutRecord, LayoutRun, LayoutSettings,
    PracticeSource, RunAsk, in_force, lay_out_whole, with_practice,
};
use ohl_structures::{LibraryItem, StructureType};

/// The K-brace, D1 turned by `roll` degrees.
fn tower(roll: f64) -> String {
    format!(
        r#"id = "K"
name = "k"
kind = "lattice"

[[node]]
id = "N1"
point = ["0 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "N2"
point = ["0 m", "0 m", "3 m"]

[[node]]
id = "N3"
point = ["0 m", "0 m", "6 m"]
support = "fixed"

[[node]]
id = "N4"
point = ["2.5 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "N5"
point = ["2.5 m", "0 m", "6 m"]
support = "fixed"

[[node]]
id = "N6"
point = ["5 m", "0 m", "0 m"]
support = "fixed"

[[member]]
id = "L1"
nodes = ["N1", "N2"]
section = "EA-61x7"
orientation = "x+y+"
design = {{ class = "leg" }}

[[member]]
id = "L2"
nodes = ["N2", "N3"]
section = "EA-61x7"
orientation = "x+y+"
design = {{ class = "leg" }}

[[member]]
id = "D1"
nodes = ["N4", "N2"]
section = "EA-61x7"
orientation = "y-z+"
roll = "{roll} deg"
design = {{ class = "bracing" }}

[[member]]
id = "D2"
nodes = ["N5", "N2"]
section = "EA-61x7"
orientation = "z-y-"
design = {{ class = "bracing" }}

[[member]]
id = "H0"
nodes = ["N1", "N4"]
section = "EA-61x7"
design = {{ class = "bracing" }}

[[member]]
id = "H1"
nodes = ["N4", "N6"]
section = "EA-61x7"
design = {{ class = "bracing" }}
"#
    )
}

fn library() -> ohl_sections::SectionLibrary {
    ohl_sections::SectionLibrary::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/sections/example-sections.ohlsec"),
    )
    .unwrap()
}

/// The structure's own practice `p` written as its `[layout_practice]`
/// table (structure file version 27), as a user's file holds it.
fn with_own(text: &str, p: &DetailingPractice) -> String {
    let own = toml::to_string(&p.to_table()).unwrap();
    let mut doc: toml_edit::DocumentMut = text.parse().unwrap();
    let t: toml_edit::DocumentMut = own.parse().unwrap();
    let mut table = t.as_table().clone();
    table.set_implicit(false);
    doc.insert("layout_practice", toml_edit::Item::Table(table));
    doc.to_string()
}

/// The run as OHL-Structure makes it: the window's settings with the
/// structure's own practice (none: the default).
fn run(text: &str) -> LayoutRun {
    let t = StructureType::parse(Path::new("k.ohls"), text).unwrap();
    let own = in_force(t.layout_practice.as_ref(), None).unwrap();
    let s = with_practice(
        LayoutSettings {
            bolt: Some(BoltArea::picked(None, 0.016, None)),
            ..Default::default()
        },
        own.as_ref(),
    );
    let lib = library();
    let bolts = Default::default();
    lay_out_whole(
        text,
        &RunAsk {
            library: &lib,
            bolts: &bolts,
            settings: &s,
            preset_fields: Default::default(),
            joints: None,
            replace_hand: false,
        },
    )
    .unwrap()
}

/// The file without its `[layout_practice]` table.
fn without_practice(text: &str) -> String {
    let mut doc: toml_edit::DocumentMut = text.parse().unwrap();
    doc.remove("layout_practice");
    doc.to_string()
}

#[test]
fn the_default_practice_lays_the_tower_out_as_before() {
    for roll in [0.0, 10.0, 30.0] {
        let plain = tower(roll);
        let given = with_own(&plain, &DetailingPractice::default());
        let a = run(&plain);
        let b = run(&given);
        assert_eq!(a.joints, b.joints, "roll {roll}");
        assert_eq!(a.notes, b.notes, "roll {roll}");
        // The same file but for its table (the tables' order in the text may
        // differ where the table was put in).
        let read = |x: &str| StructureType::parse(Path::new("k.ohls"), x).unwrap();
        assert_eq!(
            read(&without_practice(&b.text)),
            read(&a.text),
            "roll {roll}"
        );
        // The record keeps the structure's own practice with its source.
        let t = StructureType::parse(Path::new("k.ohls"), &b.text).unwrap();
        let rec = LayoutRecord::of(&t).practice_of(&t).unwrap().unwrap();

        assert_eq!(rec.source, PracticeSource::Project);
        assert!(rec.practice.is_default());
        // The estimated eccentricities take the record's practice: the
        // same as without.
        let lib = library();
        let bolts = Default::default();
        let ask = EstimateAsk {
            library: &lib,
            bolts: &bolts,
            presets: &[],
        };
        let ea = estimate(&plain, &ask).unwrap();
        let eb = estimate(&given, &ask).unwrap();
        assert_eq!(ea.ends, eb.ends, "roll {roll}");
    }
}

#[test]
fn a_shop_without_bent_or_welded_plates_leaves_those_ends_by_hand() {
    let neither = DetailingPractice {
        fabrication: Fabrication::ALL
            .iter()
            .map(|f| (*f, !matches!(f, Fabrication::Bend | Fabrication::Welded)))
            .collect(),
        ..Default::default()
    };
    let ends = |r: &LayoutRun| -> usize { r.joints.iter().map(|j| j.plan.ends.len()).sum() };
    let own_plates = |r: &LayoutRun| -> usize {
        r.joints
            .iter()
            .flat_map(|j| &j.plan.plates)
            .filter(|p| p.own.is_some())
            .count()
    };
    // D1 turned so that its leg lies a little or far off the flange: where
    // the default lays it on a plate in its own plane (bent or welded),
    // the shop without them leaves it for a hand detail, said; nowhere a
    // plate in a member's own plane, never more ends than the default.
    let mut changed = 0;
    for roll in [10.0, 20.0, 30.0, 45.0] {
        let plain = tower(roll);
        let a = run(&plain);
        let b = run(&with_own(&plain, &neither));
        assert_eq!(own_plates(&b), 0, "roll {roll}: {:?}", b.joints);
        assert!(
            ends(&b) <= ends(&a),
            "roll {roll}: {} {}",
            ends(&b),
            ends(&a)
        );
        if own_plates(&a) == 0 {
            continue;
        }
        changed += 1;
        // D1 left for a hand detail, said, or laid out flat after all (the
        // run turning it where a turn lays it flat at both its ends).
        let n2 = b.joints.iter().find(|j| j.joint == "N2").unwrap();
        let d1_left = n2
            .hand
            .iter()
            .any(|h| h.starts_with("D1") && h.contains("makes no welds"));
        let d1_flat = n2.plan.ends.iter().any(|e| e.member == "D1");
        assert!(
            d1_left || d1_flat,
            "roll {roll}: {:?} {:?}",
            n2.hand,
            n2.plan
        );
    }
    assert!(changed >= 1, "no roll laid D1 on a plate in its own plane");
}
