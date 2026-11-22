// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! O44 L13: an end's host chosen by the plane its axis lies in — a member
//! ending at the joint (a face horizontal) for a plan diagonal at a post,
//! on invented joints.

use std::collections::BTreeMap;

use crate::joint_layout::{BoltArea, LayoutSettings, hosts_to_name_by};
use crate::joints::joint_details;
use crate::joints_tests::library;
use crate::layout_run::{RunAsk, lay_out};

/// An invented corner: a post V, a horizontal H ending at the corner P
/// (along x), a plan diagonal G (level, 45° in plan) of a smaller class
/// ending there too; nothing runs through P but the post.
const CORNER: &str = r#"id = "C"
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
id = "G0"
point = ["-2 m", "2 m", "3 m"]
support = "fixed"

[[member]]
id = "V1"
nodes = ["B0", "P"]
section = "EA-61x7"
design = { class = "leg" }

[[member]]
id = "V2"
nodes = ["P", "T0"]
section = "EA-61x7"
design = { class = "leg" }

[[connection]]
member = "V1"
joint = "P"
kind = "continuous"
onto = "V2"

[[member]]
id = "H"
nodes = ["W", "P"]
section = "EA-61x7"
design = { class = "bracing" }

[[member]]
id = "G"
nodes = ["P", "G0"]
section = "EA-61x7"
design = { class = "redundant" }
"#;

fn settings(plane_hosts: bool) -> LayoutSettings {
    LayoutSettings {
        bolt: Some(BoltArea::picked(None, 0.016, None)),
        plane_hosts,
        ..Default::default()
    }
}

#[test]
fn a_plan_diagonal_is_named_onto_the_horizontal_ending_at_the_post() {
    let d = joint_details(CORNER, &library(), &Default::default()).unwrap();
    let p = d.joint("P").unwrap();
    let g = p.arrivals.iter().find(|a| a.member == "G").unwrap();
    // Today's choice: the post (worked out), in neither of whose flanges'
    // planes G's axis lies.
    assert_eq!(g.onto.value.as_deref(), Some("V2"), "{g:?}");
    let on = hosts_to_name_by(p, &settings(true));
    assert!(on.iter().any(|(m, h)| m == "G" && h == "H"), "{on:?}");
    // Off: as before (the post kept, nothing named for G).
    let off = hosts_to_name_by(p, &settings(false));
    assert!(!off.iter().any(|(m, _)| m == "G"), "{off:?}");
    // The horizontal (a larger class) is never named onto the diagonal.
    assert!(!on.iter().any(|(m, h)| m == "H" && h == "G"), "{on:?}");
}

#[test]
fn two_ends_of_one_class_and_size_are_never_named_onto_each_other() {
    // G and a second plan diagonal G2 of the same class and section: each
    // lies in the other's level flange's plane; at most one is named onto
    // the other (the id decides).
    let text = CORNER.replace(
        "design = { class = \"bracing\" }",
        "design = { class = \"redundant\" }",
    ) + r#"
[[node]]
id = "G1"
point = ["2 m", "2 m", "3 m"]
support = "fixed"

[[member]]
id = "G2"
nodes = ["P", "G1"]
section = "EA-61x7"
design = { class = "redundant" }
"#;
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    let p = d.joint("P").unwrap();
    let on = hosts_to_name_by(p, &settings(true));
    for (m, h) in &on {
        assert!(
            !on.iter().any(|(x, y)| x == h && y == m),
            "named onto each other: {on:?}"
        );
    }
}

#[test]
fn the_run_lays_the_diagonal_onto_the_horizontal_or_says_why_and_again_changes_nothing() {
    let lib = library();
    let bolts = Default::default();
    let s = settings(true);
    let ask = RunAsk {
        library: &lib,
        bolts: &bolts,
        settings: &s,
        preset_fields: BTreeMap::new(),
        joints: Some(["P".to_owned()].into_iter().collect()),
        replace_hand: false,
    };
    let r = lay_out(CORNER, &ask).unwrap();
    let o = r.joints.iter().find(|j| j.joint == "P").unwrap();
    match o.plan.ends.iter().find(|e| e.member == "G") {
        Some(ge) => {
            let on = match &ge.onto {
                crate::joint_layout::Onto::Member(h) => h.clone(),
                crate::joint_layout::Onto::Plate(k) => o.plan.plates[*k].on.clone(),
            };
            assert_eq!(on, "H", "{ge:?}");
            // H ends at P: its own end bolts carry G's force too, checked
            // for H's own only — said.
            assert!(
                r.notes
                    .iter()
                    .any(|n| n.contains("G onto H at P") && n.contains("NOT ON THE SAFE SIDE")),
                "{:?}",
                r.notes
            );
        }
        None => {
            // Left: said, never measured against the post's flanges.
            let why: Vec<&String> = o.hand.iter().filter(|h| h.starts_with("G:")).collect();
            assert!(!why.is_empty(), "{:?} {:?}", o.hand, o.plan);
            for h in why {
                assert!(!h.contains("axis runs"), "{h}");
            }
        }
    }
    // A run again: the same file.
    let again = lay_out(&r.text, &ask).unwrap();
    assert_eq!(again.text, r.text);
}
