// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! An outstanding leg pointing into the flange: a half or a quarter turn,
//! weighed ([`crate::flange_turns`], O44 L25) on invented structures.

use std::collections::{BTreeMap, BTreeSet};

use ohl_structures::{LibraryItem, StructureType};

use crate::flange_turns::{FlangeTurnRow, HALF, NONE, OFFERED, apply_offered, offered};
use crate::joint_layout::{BoltArea, LayoutSettings};
use crate::joints_tests::library;
use crate::layout_run::{LayoutRecord, RunAsk, lay_out, put_roll};

fn settings(half: bool, quarter: bool) -> LayoutSettings {
    LayoutSettings {
        bolt: Some(BoltArea::picked(None, 0.016, None)),
        flange_half: half,
        flange_quarter: quarter,
        ..Default::default()
    }
}

fn run(text: &str, s: &LayoutSettings, replace: bool) -> crate::LayoutRun {
    let lib = library();
    let bolts = Default::default();
    let ask = RunAsk {
        library: &lib,
        bolts: &bolts,
        settings: s,
        preset_fields: BTreeMap::new(),
        joints: None,
        replace_hand: replace,
    };
    lay_out(text, &ask).unwrap()
}

fn parse(text: &str) -> StructureType {
    StructureType::parse(std::path::Path::new("s.ohls"), text).unwrap()
}

/// An invented body with a sharp taper change: several of its bracing
/// ends have their outstanding leg pointing into the flange they bolt
/// onto (measured: 17 such ends, 12 members).
fn body() -> String {
    crate::orient_tests::body(
        &[
            (0.0, 4.0, 4.0),
            (4.0, 2.5, 2.5),
            (10.0, 2.0, 2.0),
            (14.0, 2.0, 2.0),
        ],
        &[1, 2],
    )
}

fn roll(text: &str, m: &str) -> String {
    let t = parse(text);
    crate::weighed_turns::roll_text(t.members.iter().find(|x| x.id == m).unwrap().roll.as_ref())
}

/// Each member with an end whose outstanding leg points into the flange
/// weighed — half a turn and a quarter turn either way, over every joint it
/// reaches — and recorded; no joint laid out worse than without the
/// weighing; a run again weighs none of them again and changes nothing.
/// Off: nothing weighed, nothing recorded.
#[test]
fn members_with_a_leg_into_the_flange_are_weighed_once_and_recorded() {
    let text = body();
    let off = run(&text, &settings(false, false), false);
    let into: usize = off
        .joints
        .iter()
        .map(|o| {
            o.hand
                .iter()
                .filter(|h| h.contains(crate::flange_turns::INTO_FLANGE))
                .count()
        })
        .sum();
    assert!(into > 0, "the invented body has such ends");
    assert!(LayoutRecord::of(&parse(&off.text)).flange_turns.is_empty());
    assert!(off.notes.iter().all(|n| !n.contains("(L25)")));
    let r = run(&text, &settings(true, true), false);
    assert!(
        r.notes
            .iter()
            .any(|n| n.starts_with("an outstanding leg pointing into the flange (L25)")),
        "{:?}",
        r.notes
    );
    let rec = LayoutRecord::of(&parse(&r.text));
    assert!(!rec.flange_turns.is_empty());
    let weighed: BTreeSet<&str> = r
        .weighed
        .iter()
        .filter(|w| crate::weighed_turns::is_flange(&w.kind))
        .map(|w| w.run.as_str())
        .collect();
    assert_eq!(
        weighed,
        rec.flange_turns.keys().map(String::as_str).collect(),
        "every member weighed has its row"
    );
    // No joint worse than without the weighing.
    let by = crate::weighed_turns::counts_of(&off);
    for (j, c) in crate::weighed_turns::counts_of(&r) {
        assert!(!c.worse_than(&by[j]), "{j}: {c:?} against {:?}", by[j]);
    }
    // A member left keeps its roll; one turned has its roll with the turn.
    for (m, row) in &rec.flange_turns {
        if row.kind == NONE {
            assert_eq!(row.before, row.with, "{m}");
        } else if row.applied() {
            assert_eq!(roll(&r.text, m), row.with, "{m}");
        }
    }
    // A run again: nothing weighed again, nothing changed.
    let again = run(&r.text, &settings(true, true), false);
    assert!(
        again
            .weighed
            .iter()
            .all(|w| !crate::weighed_turns::is_flange(&w.kind)),
        "{:?}",
        again.notes
    );
    assert_eq!(again.text, r.text);
}

/// A turn offered (the roll the designer's): listed by `offered`, applied
/// by Lay out joints' tick (`apply_offered`: the roll written as offered,
/// the row then an applied half turn), and then kept by a run — never
/// turned again, the row still the member's. An edit of that roll makes
/// the row no longer current.
#[test]
fn an_offered_turn_is_applied_when_ticked_and_kept() {
    let mut doc: toml_edit::DocumentMut = body().parse().unwrap();
    put_roll(&mut doc, "D00", "10 deg");
    let t = parse(&doc.to_string());
    let mut rec = LayoutRecord::of(&t);
    rec.flange_turns.insert(
        "D00".into(),
        FlangeTurnRow {
            before: "10 deg".into(),
            with: "-170 deg".into(),
            kind: OFFERED.into(),
            words: "D00: half a turn offered".into(),
        },
    );
    rec.write(&mut doc);
    let text = doc.to_string();
    // Read back as written.
    assert_eq!(
        LayoutRecord::of(&parse(&text)).flange_turns["D00"],
        rec.flange_turns["D00"]
    );
    assert!(offered(&parse(&text)).contains_key("D00"));
    let one: BTreeSet<String> = ["D00".to_owned()].into();
    let applied = apply_offered(&text, &one).unwrap();
    assert_eq!(roll(&applied, "D00"), "-170 deg");
    let row = &LayoutRecord::of(&parse(&applied)).flange_turns["D00"];
    assert_eq!(row.kind, HALF);
    assert!(offered(&parse(&applied)).is_empty());
    // Kept by a run, values given by hand replaced or not.
    for replace in [false, true] {
        let r = run(&applied, &settings(true, true), replace);
        assert_eq!(roll(&r.text, "D00"), "-170 deg", "replace {replace}");
        let rec = LayoutRecord::of(&parse(&r.text));
        assert!(rec.flange_turns["D00"].applied(), "replace {replace}");
        assert!(!rec.rolled.contains_key("D00"), "replace {replace}");
    }
    // The roll changed by hand: the row no longer the member's.
    let mut doc: toml_edit::DocumentMut = applied.parse().unwrap();
    put_roll(&mut doc, "D00", "5 deg");
    let t = parse(&doc.to_string());
    let rec = LayoutRecord::of(&t);
    assert!(!rec.flange_turns["D00"].current(&t, &rec.rolled, "D00"));
}

/// The quarter turn of an unequal angle bolts its other leg: tried only
/// where no leg is given; with the member's `connected_leg` given, only the
/// half turn is tried (a value given is never changed silently).
#[test]
fn an_unequal_angle_is_turned_a_quarter_only_where_no_leg_is_given() {
    let unequal = body().replace(
        "section = \"EA-61x7\"\nkind = \"frame\"\ndesign = { class = \"bracing\" }",
        "section = \"UA-83x52x6\"\nkind = \"frame\"\ndesign = { class = \"bracing\" }",
    );
    assert_ne!(unequal, body(), "the bracing made unequal angles");
    // The members weighed, and those with a quarter turn tried (their
    // words name +90° or −90°).
    let tries = |text: &str| -> (usize, usize) {
        let r = run(text, &settings(true, true), false);
        let ours: Vec<&String> = r
            .notes
            .iter()
            .filter(|n| n.contains("into the flange at"))
            .collect();
        (
            ours.len(),
            ours.iter().filter(|n| n.contains("90°")).count(),
        )
    };
    let (weighed, quarter) = tries(&unequal);
    assert!(weighed > 0, "members weighed");
    assert!(quarter > 0, "the quarter turns tried");
    let given = unequal.replace(
        "design = { class = \"bracing\" }",
        "design = { class = \"bracing\", connected_leg = \"long\" }",
    );
    let (weighed, quarter) = tries(&given);
    assert!(weighed > 0, "members weighed");
    assert_eq!(quarter, 0, "no quarter turn where the leg is given");
}
