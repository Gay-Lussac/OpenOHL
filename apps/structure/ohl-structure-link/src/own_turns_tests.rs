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
        // (The invented body measured before L32 — the millimetre search,
        // L1's guard's fallback kept: with L32's taken back its joints lay
        // out better with the plates and every member keeps the roll with
        // them, one kind only.)
        range_search: crate::joint_layout::RangeSearch::Millimetre,
        guard_taken_back: false,
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

/// `text` with member `m` split at its middle into two pieces in line,
/// `{m}a` and `{m}b`, written one part (`continuous`) at the new joint.
fn split(text: &str, m: &str) -> String {
    let t = parse(text);
    let x = t.members.iter().find(|x| x.id == m).unwrap();
    let p = |n: &str| t.nodes.iter().find(|y| y.id == n).unwrap().point;
    let (a, b) = (p(&x.nodes[0]), p(&x.nodes[1]));
    let mid: Vec<f64> = (0..3).map(|k| (a[k] + b[k]).meters() / 2.0).collect();
    let mut doc: toml_edit::DocumentMut = text.parse().unwrap();
    let ms = doc["member"].as_array_of_tables_mut().unwrap();
    let k = ms
        .iter()
        .position(|y| y.get("id").and_then(|v| v.as_str()) == Some(m))
        .unwrap();
    let mut second = ms.get(k).unwrap().clone();
    let first = ms.get_mut(k).unwrap();
    let (n0, n1) = (x.nodes[0].clone(), x.nodes[1].clone());
    first["id"] = toml_edit::value(format!("{m}a"));
    first["nodes"] = toml_edit::value(toml_edit::Array::from_iter([n0, "JM".to_owned()]));
    second["id"] = toml_edit::value(format!("{m}b"));
    second["nodes"] = toml_edit::value(toml_edit::Array::from_iter(["JM".to_owned(), n1]));
    ms.push(second);
    let mut s = doc.to_string();
    s.push_str(&format!(
        "\n[[node]]\nid = \"JM\"\npoint = [\"{} m\", \"{} m\", \"{} m\"]\n\n[[connection]]\n\
         member = \"{m}a\"\njoint = \"JM\"\nkind = \"continuous\"\nonto = \"{m}b\"\n",
        mid[0], mid[1], mid[2]
    ));
    s
}

/// O44 L32: members one part with another decided as a whole part at
/// once — a decided member of the invented body split into two pieces in
/// line written one part: both pieces decided together (the same kind,
/// their rolls alike), over every joint either reaches, said as one part;
/// a run again decides nothing again and changes nothing.
#[test]
fn a_whole_part_is_decided_at_once() {
    let text = body();
    let r = run(&text, &settings(true));
    let rec = LayoutRecord::of(&parse(&r.text));
    // (Measured: D00 and D03 split stay with the plates, D01 and D02
    // without them — both kinds.)
    let mut kinds = std::collections::BTreeSet::new();
    for m in ["D00", "D01"] {
        assert!(rec.own_turns.contains_key(m), "{m} decided whole");
        let s = split(&text, m);
        let r = run(&s, &settings(true));
        let rec = LayoutRecord::of(&parse(&r.text));
        let (a, b) = (format!("{m}a"), format!("{m}b"));
        let (ra, rb) = (&rec.own_turns[&a], &rec.own_turns[&b]);
        // Both pieces decided, the same way, as one part.
        assert_eq!(ra.kind, rb.kind, "{m}");
        kinds.insert(ra.kind.clone());
        assert!(
            ra.words.starts_with(&format!("{a}: with {b}, one part")),
            "{}",
            ra.words
        );
        assert!(ra.words.contains("decided at once (O44 L32)"));
        for (x, row) in [(&a, ra), (&b, rb)] {
            assert_eq!(roll(&r.text, x), row.kept(), "{x}");
            if row.held() {
                assert!(!rec.rolled.contains_key(x.as_str()), "{x}");
            }
        }
        assert!(
            r.notes
                .iter()
                .any(|n| n.starts_with(&format!("{a}, {b}: one part"))),
            "{:?}",
            r.notes
        );
        // A run again: nothing decided again, nothing changed.
        let again = run(&r.text, &settings(true));
        assert_eq!(again.text, r.text, "{m}");
    }
    assert_eq!(kinds.len(), 2, "{kinds:?}");
}

/// O44 L32: L1's guard's fallback taken back where it writes a joint
/// worse — on the invented body (where it does: measured) the run lays out
/// no fewer member ends and no joint fewer than with the fallback kept, and
/// a run again on its result changes nothing.
#[test]
fn l1_guards_fallback_taken_back_where_it_writes_a_joint_worse() {
    let text = body();
    let kept = run(&text, &settings(true));
    let on = LayoutSettings {
        guard_taken_back: true,
        ..settings(true)
    };
    let back = run(&text, &on);
    let (a, b) = (
        crate::weighed_turns::counts_of(&kept),
        crate::weighed_turns::counts_of(&back),
    );
    let ends = |c: &std::collections::BTreeMap<&str, crate::weighed_turns::Counts>| {
        c.values().map(|x| x.ends).sum::<usize>()
    };
    assert!(ends(&b) > ends(&a), "{} → {}", ends(&a), ends(&b));
    for (j, x) in &a {
        assert!(b[j].ends >= x.ends, "{j}: {x:?} → {:?}", b[j]);
    }
    let again = run(&back.text, &on);
    assert_eq!(again.text, back.text);
}
