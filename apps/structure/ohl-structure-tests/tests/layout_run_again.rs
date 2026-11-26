// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A run again changes nothing, also after the pieces in line proposed
//! since are confirmed (O44 L27), end to end through the link as
//! OHL-Structure runs it, on an invented member line: a horizontal in
//! three pieces of one section, H1 and H2 one part already (given
//! `continuous` at M), H3 turned 1.5° from them (as a run's turns can
//! leave a piece within the 2° of one part), a redundant standing on the
//! line at M and at B.
//!
//! Confirming H3 onto H2 gives all three pieces one roll (the mean, 0.5°):
//! H1 and H2 stay one part. (Before L27 only H2 and H3 turned, half-way
//! between them: H1 and H2 were then turned apart, the core read them as
//! two pieces again, and the next run laid the joint out otherwise.) The
//! layout then laid out twice: the same file both times.

use std::collections::BTreeMap;
use std::path::Path;

use ohl_structure_link::in_line::{confirm_text, in_line_pieces};
use ohl_structure_link::{BoltArea, LayoutSettings, RunAsk, lay_out};
use ohl_structures::{LibraryItem, StructureType};

const LINE: &str = r#"id = "L"
name = "line in three pieces"
kind = "lattice"

[[node]]
id = "A"
point = ["-2 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "M"
point = ["0 m", "0 m", "0 m"]

[[node]]
id = "B"
point = ["2 m", "0 m", "0 m"]

[[node]]
id = "C"
point = ["4 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "T"
point = ["0 m", "0 m", "1.5 m"]
support = "fixed"

[[node]]
id = "U"
point = ["2 m", "0 m", "1.5 m"]
support = "fixed"

[[member]]
id = "H1"
nodes = ["A", "M"]
section = "EA-61x7"
orientation = "z+y-"
design = { class = "bracing", bolts = { diameter = "16 mm", count = 2 } }

[[member]]
id = "H2"
nodes = ["M", "B"]
section = "EA-61x7"
orientation = "z+y-"
design = { class = "bracing", bolts = { diameter = "16 mm", count = 2 } }

[[member]]
id = "H3"
nodes = ["B", "C"]
section = "EA-61x7"
orientation = "z+y-"
roll = "1.5 deg"
design = { class = "bracing", bolts = { diameter = "16 mm", count = 2 } }

[[member]]
id = "V1"
nodes = ["M", "T"]
section = "EA-61x7"
orientation = "y-x+"
design = { class = "redundant" }

[[member]]
id = "V2"
nodes = ["B", "U"]
section = "EA-61x7"
orientation = "y-x+"
design = { class = "redundant" }

[[connection]]
member = "H1"
joint = "M"
kind = "continuous"
onto = "H2"

[[connection]]
member = "H2"
joint = "M"
kind = "continuous"
onto = "H1"
"#;

fn parse(text: &str) -> StructureType {
    StructureType::parse(Path::new("l.ohls"), text).unwrap()
}

fn library() -> ohl_sections::SectionLibrary {
    ohl_sections::SectionLibrary::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/sections/example-sections.ohlsec"),
    )
    .unwrap()
}

/// The layout run (the default settings, bolts of 16 mm) on `text`: the
/// file written.
fn run(text: &str, lib: &ohl_sections::SectionLibrary) -> String {
    let bolts = Default::default();
    let s = LayoutSettings {
        bolt: Some(BoltArea::picked(None, 0.016, None)),
        ..Default::default()
    };
    let ask = RunAsk {
        library: lib,
        bolts: &bolts,
        settings: &s,
        preset_fields: BTreeMap::new(),
        joints: None,
        replace_hand: true,
    };
    lay_out(text, &ask).unwrap().text
}

/// The pieces one part: H1–H2 at M and H2–H3 at B, their rolls alike.
fn one_line(text: &str) {
    let t = parse(text);
    assert_eq!(t.continuous_with("H1", "M"), Some("H2"), "{text}");
    assert_eq!(t.continuous_with("H2", "M"), Some("H1"), "{text}");
    assert_eq!(t.continuous_with("H2", "B"), Some("H3"), "{text}");
    assert_eq!(t.continuous_with("H3", "B"), Some("H2"), "{text}");
    let roll = |m: &str| t.members.iter().find(|x| x.id == m).unwrap().roll.clone();
    assert_eq!(roll("H1"), roll("H2"), "{text}");
    assert_eq!(roll("H2"), roll("H3"), "{text}");
}

#[test]
fn a_run_again_after_confirming_the_pieces_proposed_since_changes_nothing() {
    let lib = library();
    // H1–H2 one part already; H3 onto H2 proposed (1.5° apart).
    let t = parse(LINE);
    assert_eq!(t.continuous_with("H1", "M"), Some("H2"));
    let list = in_line_pieces(&t, None);
    assert_eq!(list.len(), 1, "{list:?}");
    assert!(list[0].proposed() && list[0].joint == "B", "{list:?}");
    let confirmed = confirm_text(LINE, &list).unwrap();
    one_line(&confirmed);
    // The mean of the three pieces' rolls (0°, 0°, 1.5°).
    let t = parse(&confirmed);
    let h1 = t.members.iter().find(|x| x.id == "H1").unwrap();
    let deg = ohl_structure_edit::edits::member_roll(h1).to_degrees();
    assert!((deg - 0.5).abs() < 0.01, "{deg}");
    // Laid out, and again: the same file; the line one part throughout.
    let once = run(&confirmed, &lib);
    one_line(&once);
    let twice = run(&once, &lib);
    assert_eq!(twice, once);
    // Nothing left to confirm: confirming again changes nothing either.
    let list = in_line_pieces(&parse(&once), None);
    assert!(list.iter().all(|x| !x.proposed()), "{list:?}");
    assert_eq!(confirm_text(&once, &list).unwrap(), once);
}
