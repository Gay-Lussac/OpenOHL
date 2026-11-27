// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Each turn the far-out welded plates change, decided with and without
//! them ([`crate::own_turns`], O44 L28), on invented structures.

use std::collections::BTreeMap;

use ohl_structures::{LibraryItem, StructureType};

use crate::joint_layout::{BoltArea, LayoutSettings};
use crate::joints_tests::library;
use crate::layout_run::{LayoutRecord, RunAsk, lay_out, put_roll};
use crate::own_turns::{OwnTurnRow, WITH, WITHOUT};

fn settings(weighed: bool) -> LayoutSettings {
    LayoutSettings {
        bolt: Some(BoltArea::picked(None, 0.016, None)),
        own_turns_weighed: weighed,
        // (L25's turns apart: what is decided here alone.)
        flange_half: false,
        flange_quarter: false,
        ..Default::default()
    }
}

fn run(text: &str, s: &LayoutSettings) -> crate::LayoutRun {
    let lib = library();
    let bolts = Default::default();
    let ask = RunAsk {
        library: &lib,
        bolts: &bolts,
        settings: s,
        preset_fields: BTreeMap::new(),
        joints: None,
        replace_hand: false,
    };
    lay_out(text, &ask).unwrap()
}

fn parse(text: &str) -> StructureType {
    StructureType::parse(std::path::Path::new("s.ohls"), text).unwrap()
}

fn roll(text: &str, m: &str) -> String {
    let t = parse(text);
    crate::weighed_turns::roll_text(t.members.iter().find(|x| x.id == m).unwrap().roll.as_ref())
}

/// An invented body tapering in steps: bracing ends off their flanges,
/// some on plates in their own plane welded to the flat plate putting
/// their axis more than a plate's thickness out (measured: nine such
/// plates; seven members whose turn they change, kept with them at three,
/// without them at four).
fn body() -> String {
    crate::orient_tests::body(
        &[
            (0.0, 7.0, 7.0),
            (4.0, 4.0, 4.0),
            (8.0, 2.5, 2.5),
            (12.0, 2.5, 2.5),
        ],
        &[1, 2],
    )
}

/// The weighing on: each member whose roll the turns chosen without the
/// far-out welded plates change is decided and recorded — its roll the
/// one its row keeps, no joint it reaches laid out worse with the roll kept
/// with the plates than without them; said in the notes. A run again
/// decides nothing again and changes nothing. Off: nothing decided,
/// nothing recorded, the file as before L28.
#[test]
fn the_turns_the_welded_plates_change_are_decided_once_and_recorded() {
    let text = body();
    let off = run(&text, &settings(false));
    assert!(LayoutRecord::of(&parse(&off.text)).own_turns.is_empty());
    assert!(off.notes.iter().all(|n| !n.contains("(L28)")));
    let r = run(&text, &settings(true));
    assert!(
        r.notes
            .iter()
            .any(|n| n.starts_with("the turns the far-out welded plates change (L28)")),
        "{:?}",
        r.notes
    );
    let rec = LayoutRecord::of(&parse(&r.text));
    assert!(
        rec.own_turns.values().any(|x| x.kind == WITH)
            && rec.own_turns.values().any(|x| x.kind == WITHOUT),
        "the invented body has both: {:?}",
        rec.own_turns
    );
    // The layout with the turns chosen without the plates (what the
    // weighing compares against).
    let rb = run(
        &text,
        &LayoutSettings {
            turns_without_own: true,
            ..settings(false)
        },
    );
    let (by_b, by_r) = (
        crate::weighed_turns::counts_of(&rb),
        crate::weighed_turns::counts_of(&r),
    );
    let t = parse(&text);
    for (m, row) in &rec.own_turns {
        // Kept with the plates: every joint it reaches no worse than
        // without them, one better.
        if row.kind == WITH {
            let nodes = &t.members.iter().find(|x| &x.id == m).unwrap().nodes;
            let b: Vec<_> = nodes.iter().map(|j| by_b[j.as_str()]).collect();
            let a: Vec<_> = nodes.iter().map(|j| by_r[j.as_str()]).collect();
            assert!(crate::weighed_turns::helps(&b, &a), "{m}: {b:?} → {a:?}");
        }
        assert!(row.kind == WITH || row.kind == WITHOUT, "{m}: {row:?}");
        assert_ne!(row.with, row.without, "{m}");
        assert_eq!(roll(&r.text, m), row.kept(), "{m}");
        assert!(
            row.words.starts_with(&format!("{m}: ")),
            "{m}: {}",
            row.words
        );
        // Held where the roll without the plates is kept: not the layout's
        // own turn (a run restores those).
        if row.held() {
            assert!(!rec.rolled.contains_key(m), "{m}");
        }
    }
    // Nothing decided: the file as without the weighing.
    if rec.own_turns.is_empty() {
        assert_eq!(r.text, off.text);
    }
    // A run again: nothing decided again, nothing changed.
    let again = run(&r.text, &settings(true));
    assert_eq!(again.text, r.text);
}

/// The turns chosen without the far-out welded plates (L23's variant, the
/// second layout the weighing compares) lay out no plate in its own plane
/// further out than before only through the turns: with no such plate at
/// all (L20 item 2 off) the setting changes nothing.
#[test]
fn without_the_offset_step_the_turns_chosen_without_it_change_nothing() {
    let text = body();
    let base = LayoutSettings {
        own_offset_analysed: false,
        own_turns_weighed: false,
        ..settings(false)
    };
    let a = run(&text, &base);
    let b = run(
        &text,
        &LayoutSettings {
            turns_without_own: true,
            ..base.clone()
        },
    );
    assert_eq!(a.text, b.text);
}

/// A row read back as written; a roll kept without the plates held by a
/// run (never turned again, the row still the member's, not the layout's
/// own turn); an edit of that roll makes the row no longer the member's
/// and the follow drops it.
#[test]
fn a_roll_kept_without_the_plates_is_held_and_an_edit_drops_its_row() {
    let mut doc: toml_edit::DocumentMut = body().parse().unwrap();
    put_roll(&mut doc, "D00", "-170 deg");
    let t = parse(&doc.to_string());
    let mut rec = LayoutRecord::of(&t);
    let row = OwnTurnRow {
        before: String::new(),
        with: "10 deg".into(),
        without: "-170 deg".into(),
        kind: WITHOUT.into(),
        words: "D00: the roll without them kept".into(),
    };
    rec.own_turns.insert("D00".into(), row.clone());
    rec.write(&mut doc);
    let text = doc.to_string();
    let t = parse(&text);
    assert_eq!(LayoutRecord::of(&t).own_turns["D00"], row);
    assert!(row.current(&t, "D00"));
    let r = run(&text, &settings(true));
    assert_eq!(roll(&r.text, "D00"), "-170 deg");
    let rec = LayoutRecord::of(&parse(&r.text));
    assert_eq!(rec.own_turns["D00"], row);
    assert!(!rec.rolled.contains_key("D00"));
    // The roll changed by hand: the row no longer the member's; the
    // follow drops it.
    let mut doc: toml_edit::DocumentMut = r.text.parse().unwrap();
    put_roll(&mut doc, "D00", "5 deg");
    let edited = doc.to_string();
    let t1 = parse(&edited);
    assert!(!LayoutRecord::of(&t1).own_turns["D00"].current(&t1, "D00"));
    let followed =
        crate::layout_run::follow(&r.text, &edited, &library(), &Default::default(), &[])
            .expect("followed");
    assert!(
        !LayoutRecord::of(&parse(&followed.text))
            .own_turns
            .contains_key("D00"),
        "the follow drops the row"
    );
}
