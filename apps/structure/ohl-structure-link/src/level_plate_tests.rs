// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! O44 L20: a level plate across two horizontals — a plan diagonal at a
//! post onto a plate lying on the level legs of the two horizontals meeting
//! there, held by its own bolts onto both (`[plate.also_bolts]`), on an
//! invented corner.

use std::collections::BTreeMap;

use ohl_structures::{LibraryItem, StructureType};

use crate::joint_layout::{BoltArea, LayoutSettings, level_pair};
use crate::joints::joint_details;
use crate::joints_tests::library;
use crate::layout_run::{RunAsk, lay_out};

/// An invented corner P: a post V (two pieces, one part), two horizontals
/// H1 (along x) and H2 (along y) ending at P, their level legs in one
/// plane, and a plan diagonal G (level, 45° in plan, a smaller class)
/// between them.
const CORNER: &str = r#"format = "ohl-structure"
version = 18
id = "C"
name = "c"
kind = "lattice"

[[node]]
id = "B0"
point = ["0 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "P"
point = ["0 m", "0 m", "3 m"]

[[node]]
id = "T0"
point = ["0 m", "0 m", "6 m"]
support = "fixed"

[[node]]
id = "W"
point = ["-3 m", "0 m", "3 m"]
support = "fixed"

[[node]]
id = "S"
point = ["0 m", "-3 m", "3 m"]
support = "fixed"

[[node]]
id = "G0"
point = ["-2 m", "-2 m", "3 m"]
support = "fixed"

[[member]]
id = "V1"
nodes = ["B0", "P"]
section = "EA-61x7"
roll = "180 deg"
design = { class = "leg" }

[[member]]
id = "V2"
nodes = ["P", "T0"]
section = "EA-61x7"
roll = "180 deg"
design = { class = "leg" }

[[connection]]
member = "V1"
joint = "P"
kind = "continuous"
onto = "V2"

[[member]]
id = "H1"
nodes = ["W", "P"]
section = "EA-61x7"
design = { class = "bracing" }

[[member]]
id = "H2"
nodes = ["P", "S"]
section = "EA-61x7"
design = { class = "bracing" }

[[member]]
id = "G"
nodes = ["P", "G0"]
section = "EA-61x7"
roll = "180 deg"
design = { class = "redundant" }
"#;

fn settings(level: bool) -> LayoutSettings {
    LayoutSettings {
        bolt: Some(BoltArea {
            count: 3,
            ..BoltArea::picked(None, 0.016, None)
        }),
        level_plates: level,
        ..Default::default()
    }
}

fn ask<'a>(
    lib: &'a crate::SectionLibrary,
    bolts: &'a ohl_structures::bolts::BoltLibrary,
    s: &'a LayoutSettings,
) -> RunAsk<'a> {
    RunAsk {
        library: lib,
        bolts,
        settings: s,
        preset_fields: BTreeMap::new(),
        joints: Some(["P".to_owned()].into_iter().collect()),
        replace_hand: false,
    }
}

#[test]
fn the_two_horizontals_are_found_as_the_diagonals_level_plate() {
    let d = joint_details(CORNER, &library(), &Default::default()).unwrap();
    let p = d.joint("P").unwrap();
    let g = p.arrivals.iter().find(|a| a.member == "G").unwrap();
    let (a, b) = level_pair(&p.arrivals, g).expect("a level pair");
    let mut names = [a.member.as_str(), b.member.as_str()];
    names.sort_unstable();
    assert_eq!(names, ["H1", "H2"]);
    // The post (not level) and a horizontal's own end never take part.
    let h1 = p.arrivals.iter().find(|a| a.member == "H1").unwrap();
    assert!(level_pair(&p.arrivals, h1).is_none_or(|(x, y)| x.member != "V1" && y.member != "V1"));
    let v = p.arrivals.iter().find(|a| a.member == "V2").unwrap();
    assert!(level_pair(&p.arrivals, v).is_none());
}

#[test]
fn a_horizontal_turned_over_is_no_level_plate() {
    // H2 turned half a turn: its level leg's outer face up, H1's down.
    let text = CORNER.replace(
        "id = \"H2\"\nnodes = [\"P\", \"S\"]",
        "id = \"H2\"\nnodes = [\"P\", \"S\"]\nroll = \"180 deg\"",
    );
    assert_ne!(text, CORNER);
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    let p = d.joint("P").unwrap();
    let g = p.arrivals.iter().find(|a| a.member == "G").unwrap();
    assert!(level_pair(&p.arrivals, g).is_none());
}

#[test]
fn the_run_lays_the_diagonal_onto_a_plate_held_by_both_horizontals() {
    let lib = library();
    let bolts = Default::default();
    let s = settings(true);
    let r = lay_out(CORNER, &ask(&lib, &bolts, &s)).unwrap();
    let o = r.joints.iter().find(|j| j.joint == "P").unwrap();
    let ge = o
        .plan
        .ends
        .iter()
        .find(|e| e.member == "G")
        .unwrap_or_else(|| panic!("G not laid out: {:?} {:?}", o.hand, o.plan.notes));
    let crate::joint_layout::Onto::Plate(k) = ge.onto else {
        panic!("G bolted directly: {ge:?}");
    };
    let pl = &o.plan.plates[k];
    let hold = pl.hold.as_ref().expect("held by the first");
    let also = pl
        .also
        .as_ref()
        .unwrap_or_else(|| panic!("{:?}", o.plan.notes));
    let mut names = [hold.member.as_str(), also.member.as_str()];
    names.sort_unstable();
    assert_eq!(names, ["H1", "H2"]);
    assert_eq!(hold.member, pl.on);
    assert!(also.provisional && also.points.len() >= 2, "{also:?}");
    // Written: version 26, read again with the plate held by both.
    assert!(r.text.contains("[plate.also_bolts]"), "{}", r.text);
    let t = StructureType::parse(std::path::Path::new("c.ohls"), &r.text).unwrap();
    let tp = t
        .plates
        .iter()
        .find(|q| q.also_bolts.is_some())
        .expect("a plate with also_bolts");
    assert_eq!(tp.also_bolts.as_ref().unwrap().member, also.member);
    assert!(
        t.connection_notes().is_empty(),
        "{:?}",
        t.connection_notes()
    );
    assert!(r.text.contains(&format!(
        "version = {}",
        ohl_structures::connections::PLATE_ALSO_BOLTS_VERSION
    )));
    // Drawn: its bolts onto the second member, as that member's end bolted
    // onto it; the plate drawn round them, nothing of the joint in it.
    let d = joint_details(&r.text, &lib, &bolts).unwrap();
    let p = d.joint("P").unwrap();
    let pd = p.plates.iter().find(|q| q.id == tp.id).unwrap();
    let a = pd.also_bolts.as_deref().expect("drawn");
    assert_eq!(a.member, also.member);
    assert_eq!(a.bolt_points.len(), also.points.len());
    assert!(pd.members.contains(&also.member), "{:?}", pd.members);
    for q in &a.bolt_points {
        assert!(
            crate::plate_checks::ray_to_outline(&pd.outline, *q, a.direction).is_some(),
            "a bolt outside the plate"
        );
    }
    assert!(p.clashes.is_empty(), "{:?}", p.clashes);
    // A run again: the same file.
    let again = lay_out(&r.text, &ask(&lib, &bolts, &s)).unwrap();
    assert_eq!(again.text, r.text);
}

#[test]
fn off_the_level_plate_is_not_made() {
    let lib = library();
    let bolts = Default::default();
    let s = settings(false);
    let r = lay_out(CORNER, &ask(&lib, &bolts, &s)).unwrap();
    assert!(!r.text.contains("also_bolts"), "{}", r.text);
}
