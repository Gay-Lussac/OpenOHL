// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Turns weighed over every joint they reach ([`crate::weighed_turns`],
//! O44 L19) on invented towers.

use std::collections::BTreeMap;

use ohl_structures::{LibraryItem, StructureType};

use crate::joint_layout::{BoltArea, LayoutSettings};
use crate::joints_tests::library;
use crate::layout_run::{LayoutRecord, RunAsk, lay_out};
use crate::orient::{OrientAsk, orient_angles};
use crate::weighed_turns::{candidates_for, candidates_of, reach_of, settle_turns, stage_of};

fn settings(weigh: bool) -> LayoutSettings {
    LayoutSettings {
        bolt: Some(BoltArea::picked(None, 0.016, None)),
        weigh_turns: weigh,
        // (L25's turns weighed after these: tested on their own,
        // `flange_turns_tests`.)
        flange_half: false,
        flange_quarter: false,
        ..Default::default()
    }
}

fn run(text: &str, s: &LayoutSettings) -> crate::LayoutRun {
    run_at(text, s, None)
}

fn run_at(
    text: &str,
    s: &LayoutSettings,
    joints: Option<std::collections::BTreeSet<String>>,
) -> crate::LayoutRun {
    let lib = library();
    let bolts = Default::default();
    let ask = RunAsk {
        library: &lib,
        bolts: &bolts,
        settings: s,
        preset_fields: BTreeMap::new(),
        joints,
        replace_hand: false,
    };
    lay_out(text, &ask).unwrap()
}

/// An invented body with a sharp taper change (the outline leaves the
/// lower legs' flanges a few degrees off their faces: the joints' planes
/// turn them) and plan diagonals.
pub(crate) fn tower() -> String {
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

fn parse(text: &str) -> StructureType {
    StructureType::parse(std::path::Path::new("s.ohls"), text).unwrap()
}

fn roll(t: &StructureType, m: &str) -> String {
    crate::weighed_turns::roll_text(
        t.members
            .iter()
            .find(|x| x.id == m)
            .and_then(|x| x.roll.as_ref()),
    )
}

/// An invented tapering tower oriented from its joints' planes: every run
/// turned listed in the layout record (the roll before and with the turn,
/// its kind and run), read back as the turns to weigh.
#[test]
fn orient_angles_lists_the_turns_from_the_planes_not_weighed_yet() {
    let lib = library();
    let o = orient_angles(&tower(), &lib, &OrientAsk::default()).unwrap();
    let turned: Vec<_> = o.planes.iter().filter(|r| r.turns()).collect();
    assert!(!turned.is_empty(), "{:?}", o.planes);
    let t = parse(&o.text);
    let rec = LayoutRecord::of(&t);
    for r in &turned {
        for m in &r.members {
            let row = rec.plane_turns.get(m).expect(m);
            assert_eq!(row.run, r.members[0]);
            assert_eq!(row.kind, r.kind.as_str());
            assert_eq!(row.with, roll(&t, m));
            assert_ne!(row.before, row.with);
        }
    }
    let c = candidates_of(&t);
    assert_eq!(c.len(), turned.len());
    for x in &c {
        assert_eq!(x.stage, stage_of(&x.kind));
        let r = turned.iter().find(|r| r.members[0] == x.run).unwrap();
        assert!((x.turn() - r.turn).abs() < 0.02, "{x:?} {r:?}");
    }
    // Written back exactly (the record read and written again).
    let mut doc: toml_edit::DocumentMut = o.text.parse().unwrap();
    rec.write(&mut doc);
    assert_eq!(LayoutRecord::of(&parse(&doc.to_string())), rec);
    // Orient angles from the outline alone takes the rows of what it sets.
    let back = orient_angles(
        &o.text,
        &lib,
        &OrientAsk {
            method: crate::OrientMethod::Outline,
            ..Default::default()
        },
    )
    .unwrap();
    let rb = LayoutRecord::of(&parse(&back.text));
    for o2 in back.turned.iter().chain(&back.same) {
        assert!(!rb.plane_turns.contains_key(&o2.member), "{}", o2.member);
    }
}

/// The layout run weighs each turn over every joint its members reach:
/// kept only where none is laid out worse and one better, else its rolls
/// back to before; the rows gone; a run again lays out the same and
/// weighs nothing.
#[test]
fn the_run_keeps_a_turn_only_where_every_joint_it_reaches_lays_out_better() {
    let lib = library();
    let o = orient_angles(&tower(), &lib, &OrientAsk::default()).unwrap();
    let t0 = parse(&o.text);
    let rows = LayoutRecord::of(&t0).plane_turns;
    let r = run(&o.text, &settings(true));
    let n = candidates_of(&t0).len();
    assert_eq!(r.weighed.len(), n);
    let t1 = parse(&r.text);
    let rec1 = LayoutRecord::of(&t1);
    assert!(rec1.plane_turns.is_empty());
    for w in &r.weighed {
        for m in &w.members {
            let row = &rows[m];
            let want = if w.kept { &row.with } else { &row.before };
            // (A face member the layout turned flat after: its roll before
            // that, as the record keeps it.)
            let now = rec1.rolled.get(m).cloned().unwrap_or_else(|| roll(&t1, m));
            assert_eq!(&now, want, "{w:?}");
        }
        if w.kept {
            assert!(w.worse.is_empty(), "{w:?}");
            assert!(
                w.after.0 > w.before.0 || (w.after.0 == w.before.0 && w.after.1 < w.before.1),
                "{w:?}"
            );
            assert!(w.words().contains("kept"), "{}", w.words());
        } else {
            assert!(w.words().contains("taken back"), "{}", w.words());
        }
    }
    assert!(
        r.notes
            .iter()
            .any(|x| x.contains("weighed over every joint"))
    );
    // A run again: nothing to weigh, the same file.
    let again = run(&r.text, &settings(true));
    assert!(again.weighed.is_empty());
    assert_eq!(again.text, r.text);
    // Deterministic.
    assert_eq!(run(&o.text, &settings(true)).text, r.text);
    // Off: the rolls as written (L18), the rows kept.
    let plain = run(&o.text, &settings(false));
    assert!(plain.weighed.is_empty());
    assert_eq!(LayoutRecord::of(&parse(&plain.text)).plane_turns, rows);
}

/// A roll changed since Orient angles turned it is the user's: its run is
/// not weighed (never taken back).
#[test]
fn a_roll_changed_since_is_not_weighed() {
    let lib = library();
    let o = orient_angles(&tower(), &lib, &OrientAsk::default()).unwrap();
    let t = parse(&o.text);
    let c = candidates_of(&t);
    let first = c.first().expect("a turn").clone();
    let m = &first.members[0].member;
    let mut doc: toml_edit::DocumentMut = o.text.parse().unwrap();
    crate::layout_run::put_roll(&mut doc, m, "12.34 deg");
    let c2 = candidates_of(&parse(&doc.to_string()));
    assert_eq!(c2.len() + 1, c.len());
    assert!(c2.iter().all(|x| x.run != first.run));
}

/// A run asking for some joints weighs only the turns whose every joint it
/// lays out; the others stay listed for a run that does. Settled once on
/// the whole tower (as Lay out joints does), the file laid out plainly gives
/// what the weighing run gives.
#[test]
fn a_turn_is_weighed_only_by_a_run_laying_out_every_joint_it_reaches() {
    let lib = library();
    let o = orient_angles(&tower(), &lib, &OrientAsk::default()).unwrap();
    let t = parse(&o.text);
    let all = candidates_of(&t);
    let c = all
        .iter()
        .find(|c| c.stage == 0)
        .expect("a leg's or chord's turn");
    let reach = reach_of(&t, c);
    let one: std::collections::BTreeSet<String> = reach.iter().take(1).cloned().collect();
    assert!(
        candidates_for(&t, Some(&one))
            .iter()
            .all(|x| x.run != c.run)
    );
    assert!(
        candidates_for(&t, Some(&reach))
            .iter()
            .any(|x| x.run == c.run)
    );
    // Laid out at one joint: nothing reaching past it weighed, its row kept.
    let r = run_at(&o.text, &settings(true), Some(one.clone()));
    assert!(r.weighed.iter().all(|w| w.run != c.run));
    let rec = LayoutRecord::of(&parse(&r.text));
    assert!(
        c.members
            .iter()
            .all(|m| rec.plane_turns.contains_key(&m.member))
    );
    // Settled on the whole tower, then laid out plainly: as the weighing run.
    let s = settings(true);
    let bolts = Default::default();
    let ask = RunAsk {
        library: &lib,
        bolts: &bolts,
        settings: &s,
        preset_fields: BTreeMap::new(),
        joints: None,
        replace_hand: false,
    };
    let (settled, w) = settle_turns(&o.text, &ask)
        .unwrap()
        .expect("turns to weigh");
    assert!(LayoutRecord::of(&parse(&settled)).plane_turns.is_empty());
    assert_eq!(w.weighed.len(), all.len());
    let plain = run(&settled, &s);
    assert!(plain.weighed.is_empty());
    assert_eq!(plain.text, w.text);
    assert_eq!(plain.text, run(&o.text, &s).text);
    // Nothing listed: nothing to settle.
    assert!(settle_turns(&plain.text, &ask).unwrap().is_none());
}

/// The weighing's promise: no joint laid out worse than with every turn
/// taken back (the base), but those it counts in its notes (no turn
/// reaches them, no member turned otherwise meets them).
#[test]
fn no_joint_is_laid_out_worse_than_with_every_turn_taken_back() {
    let lib = library();
    let o = orient_angles(&tower(), &lib, &OrientAsk::default()).unwrap();
    let t = parse(&o.text);
    let mut doc: toml_edit::DocumentMut = o.text.parse().unwrap();
    for c in candidates_of(&t) {
        for r in &c.members {
            crate::layout_run::put_roll(&mut doc, &r.member, &r.before);
        }
    }
    let mut rec = LayoutRecord::of(&t);
    rec.plane_turns.clear();
    rec.write(&mut doc);
    let s = settings(true);
    let base = run(&doc.to_string(), &s);
    assert!(base.weighed.is_empty());
    let r = run(&o.text, &s);
    assert!(!r.weighed.is_empty());
    let left: usize = r
        .notes
        .iter()
        .find(|n| n.contains("where no turn reaches"))
        .and_then(|n| n.split_whitespace().next()?.parse().ok())
        .unwrap_or(0);
    let (b, a) = (
        crate::weighed_turns::counts_of(&base),
        crate::weighed_turns::counts_of(&r),
    );
    let worse = a
        .iter()
        .filter(|(j, x)| x.worse_than(&b.get(*j).copied().unwrap_or_default()))
        .count();
    assert!(worse <= left, "{worse} worse, {left} said: {:?}", r.notes);
    assert!(
        r.notes
            .iter()
            .any(|n| n.starts_with("the weighing laid the tower out")),
        "{:?}",
        r.notes
    );
}
