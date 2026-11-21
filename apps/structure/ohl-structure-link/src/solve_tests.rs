// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Solve the clashes ([`crate::solve`], O44 L11) on invented joints.

use std::collections::BTreeMap;

use crate::joint_layout::{BoltArea, LayoutSettings};
use crate::joints_tests::library;
use crate::solve::{EffectKind, SolveAsk, solve_joint};

/// An invented tower: the leg L1–L2 at N2, the diagonal D1 bolted onto it
/// with three bolts given, the horizontal H1 with its design data's two
/// (the joint details' fixture of the application, the same text).
pub(crate) const ANGLES: &str = r#"id = "A"
name = "angles"
kind = "lattice"

[[node]]
id = "N1"
point = ["0 m", "1 m", "0 m"]
support = "fixed"

[[node]]
id = "N2"
point = ["0 m", "1 m", "5 m"]

[[node]]
id = "N3"
point = ["0 m", "1 m", "10 m"]

[[node]]
id = "N4"
point = ["4 m", "1 m", "0 m"]
support = "fixed"

[[node]]
id = "N5"
point = ["4 m", "1 m", "5 m"]

[[member]]
id = "L1"
nodes = ["N1", "N2"]
section = "EA-61x7"
orientation = "y-x+"
design = { class = "leg" }

[[member]]
id = "L2"
nodes = ["N2", "N3"]
section = "EA-61x7"
orientation = "y-x+"
design = { class = "leg" }

[[member]]
id = "D1"
nodes = ["N4", "N2"]
section = "UA-83x52x6"
orientation = "y-z+"
design = { class = "bracing" }

[[member]]
id = "H1"
nodes = ["N2", "N5"]
section = "UA-83x52x6"
orientation = "z+y-"
design = { class = "bracing", bolts = { diameter = "16 mm", count = 2 } }

[[member]]
id = "R1"
nodes = ["N4", "N5"]
section = "UA-83x52x6"
orientation = "x-y-"

[[connection]]
member = "D1"
joint = "N2"
onto = "L1"
leg = "long"
bolts = { count = 3, diameter = "16 mm", grade = "8.8" }
gauge_lines = 1
gauges = ["30 mm"]
pitch = "55 mm"
end_distance = "30 mm"
edge_distance = "25 mm"
"#;

fn settings() -> LayoutSettings {
    LayoutSettings {
        bolt: Some(BoltArea::picked(None, 0.016, None)),
        ..Default::default()
    }
}

fn solve(text: &str, joint: &str) -> crate::Solution {
    let lib = library();
    let bolts = Default::default();
    let s = settings();
    solve_joint(
        text,
        &SolveAsk {
            library: &lib,
            bolts: &bolts,
            settings: &s,
            preset_fields: BTreeMap::new(),
            leg_bolt: None,
            joint: joint.to_owned(),
        },
    )
    .unwrap()
}

/// D1 and H1 run into each other at N2 (on the leg's inner face as
/// written): the solution clears the clashes, its effects say what changes
/// (their steel stopping short, a turn marked), nothing of the legs is
/// pulled back or turned, and it is deterministic.
#[test]
fn the_clashes_of_a_joint_are_solved_as_one() {
    let s = solve(ANGLES, "N2");
    assert!(s.before.clashes > 0, "{:?}", s.before);
    let best = s.best().unwrap_or_else(|| panic!("{:?}", s.why_none));
    assert!(best.counts.clashes < s.before.clashes, "{:?}", best.counts);
    assert!(best.refused.is_none());
    assert!(!best.effects.is_empty());
    // (The layout joins the leg's two pieces of one section and roll as
    // one part, its first step: said.)
    assert!(
        best.effects.iter().all(|e| {
            !(e.words.starts_with("L1") || e.words.starts_with("L2"))
                || e.words.contains("as one part")
        }),
        "{:?}",
        best.effects
    );
    assert!(
        best.effects
            .iter()
            .any(|e| e.kind == EffectKind::PlateAdded || e.kind == EffectKind::Cut)
    );
    // Every turn is marked.
    assert!(
        best.effects
            .iter()
            .filter(|e| e.kind == EffectKind::Turned)
            .all(|e| e.marked())
    );
    let again = solve(ANGLES, "N2");
    assert_eq!(again.best().map(|c| &c.text), Some(&best.text));
}

/// A joint with nothing to solve: none chosen, said why.
#[test]
fn nothing_better_is_said() {
    let s = solve(ANGLES, "N1");
    if s.before.clashes + s.before.problems == 0 {
        assert!(s.best().is_none());
        assert!(s.why_none.is_some());
    }
}
