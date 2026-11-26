// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Solve the clashes after the run ([`crate::solve_pass`], O44 L30) on
//! invented joints.

use std::collections::BTreeMap;

use ohl_structures::{LibraryItem, StructureType};

use crate::joint_layout::{BoltArea, LayoutSettings};
use crate::layout_run::LayoutRecord;
use crate::solve_pass::{PassJoint, solve_left};
use crate::{RunAsk, lay_out};

fn settings(solve_left: bool) -> LayoutSettings {
    LayoutSettings {
        bolt: Some(BoltArea::picked(None, 0.016, None)),
        solve_left,
        ..Default::default()
    }
}

fn run(text: &str, solve_left: bool, replace_hand: bool) -> crate::LayoutRun {
    let lib = crate::joints_tests::library();
    let bolts = Default::default();
    let s = settings(solve_left);
    lay_out(
        text,
        &RunAsk {
            library: &lib,
            bolts: &bolts,
            settings: &s,
            preset_fields: BTreeMap::new(),
            joints: None,
            replace_hand,
        },
    )
    .unwrap()
}

/// The invented tower of Solve the clashes' tests, its crowded joint as
/// drawn (D1 and H1 run into each other at N2), given to the pass: solved
/// better on every count, every turn and move said marked in its notes,
/// every value it wrote at the joint kept as the layout's in the record
/// (a later run takes it out first), and deterministic.
#[test]
fn a_crowded_joint_is_solved_its_changes_said_and_recorded() {
    let lib = crate::joints_tests::library();
    let bolts = Default::default();
    let s = settings(false);
    let ask = RunAsk {
        library: &lib,
        bolts: &bolts,
        settings: &s,
        preset_fields: BTreeMap::new(),
        joints: None,
        replace_hand: true,
    };
    let left = [PassJoint {
        joint: "N2".to_owned(),
        left: 0,
        hand: Vec::new(),
    }];
    let text = crate::solve_tests::ANGLES;
    let res = solve_left(text, &left, &ask).unwrap();
    assert_eq!(res.solved.len(), 1, "{:?}", res.not_solved);
    let s = &res.solved[0];
    assert!(s.after.clashes < s.before.clashes, "{s:?}");
    assert!(s.after.problems <= s.before.problems && s.after.left <= s.before.left);
    let notes = res.notes();
    let note = notes
        .iter()
        .find(|n| n.contains("(L30) at N2:"))
        .unwrap_or_else(|| panic!("{notes:?}"));
    for e in s.effects.iter().filter(|e| e.marked()) {
        assert!(note.contains("MARKED") && note.contains(&e.words), "{note}");
    }
    assert!(notes.last().unwrap().contains("1 solved"), "{notes:?}");
    // What it wrote at N2 is the layout's in the record.
    let parse = |t: &str| StructureType::parse(std::path::Path::new("s.ohls"), t).unwrap();
    let (t0, t1) = (parse(text), parse(&res.text));
    let rec = LayoutRecord::of(&t1);
    for c in t1.connections.iter().filter(|c| c.joint == "N2") {
        let before = t0
            .connections
            .iter()
            .find(|x| x.member == c.member && x.joint == "N2");
        if before.is_some_and(|b| b.cut != c.cut) || (before.is_none() && c.cut.is_some()) {
            assert!(
                rec.ends
                    .get(&(c.member.clone(), "N2".to_owned()))
                    .is_some_and(|f| f.contains("cut")),
                "{}: {:?}",
                c.member,
                rec.ends
            );
        }
    }
    for m in &t1.members {
        let m0 = t0.members.iter().find(|x| x.id == m.id).unwrap();
        if m0.roll != m.roll {
            assert!(rec.rolled.contains_key(&m.id), "{}", m.id);
        }
    }
    assert_eq!(solve_left(text, &left, &ask).unwrap().text, res.text);
}

/// Invented joints a run leaves not whole (an end that would need more
/// than the longest pull-back past a member given by hand; bolts joining
/// nothing): the pass tries them, takes nothing that is not better, and a
/// run again with the pass gives the same file.
#[test]
fn the_joints_left_are_tried_and_a_run_again_changes_nothing() {
    let r1 = format!(
        "{}\n[[node]]\nid = \"N6\"\npoint = [\"2.5 m\", \"0 m\", \"0.13 m\"]\nsupport = \"fixed\"\n\
         \n[[member]]\nid = \"R1\"\nnodes = [\"N6\", \"N2\"]\nsection = \"EA-61x7\"\n\
         orientation = \"y-z+\"\ndesign = {{ class = \"redundant\" }}\n\
         \n[[plate]]\nid = \"GH\"\njoint = \"N2\"\nthickness = \"8 mm\"\n\
         \n[[connection]]\nmember = \"R1\"\njoint = \"N2\"\nonto = \"GH\"\ncut = \"0 mm\"\n",
        crate::layout_run_tests::k_brace()
    );
    for text in [r1, crate::in_line_tests::diagonal()] {
        for replace in [false, true] {
            let plain = run(&text, false, replace);
            assert!(!crate::solve_pass::pass_joints(&plain).is_empty());
            let r = run(&text, true, replace);
            assert!(r.solved.iter().all(|s| {
                s.after.clashes <= s.before.clashes
                    && s.after.problems <= s.before.problems
                    && s.after.left <= s.before.left
            }));
            let whole = |r: &crate::LayoutRun| r.laid_out().filter(|o| o.hand.is_empty()).count();
            assert!(whole(&r) >= whole(&plain), "{:?}", r.notes);
            assert!(
                r.notes
                    .iter()
                    .any(|n| n.contains("Solve the clashes after the run (L30):")),
                "{:?}",
                r.notes
            );
            let again = run(&r.text, true, replace);
            assert_eq!(again.text, r.text, "replace {replace}");
            assert_eq!(run(&text, true, replace).text, r.text);
        }
    }
}

/// Off (the default): the run as before.
#[test]
fn off_the_run_is_as_before() {
    assert!(!LayoutSettings::default().solve_left);
    let r = run(crate::solve_tests::ANGLES, false, false);
    assert!(r.solved.is_empty());
    assert!(!r.notes.iter().any(|n| n.contains("(L30)")));
}
