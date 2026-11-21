// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Pieces in line proposed "continuous" (O44 J9, D15) on an invented
//! joint: a horizontal in two pieces through a mid-panel joint with a
//! redundant standing on it — proposed, never written by a run, confirmed
//! in one edit; legs with bolts, splices given and pieces turned apart
//! not proposed, said why; the layout before and after confirming.

use std::collections::BTreeMap;

use ohl_structures::{LibraryItem, StructureType};

use crate::in_line::{confirm_text, in_line_pieces};
use crate::joint_layout::{BoltArea, LayoutSettings, Onto, plan_joint};
use crate::joint_layout_tests::SPLIT;
use crate::joints::joint_details;
use crate::joints_tests::library;
use crate::layout_run::{RunAsk, lay_out};

fn parse(text: &str) -> StructureType {
    StructureType::parse(std::path::Path::new("s.ohls"), text).unwrap()
}

fn settings() -> LayoutSettings {
    LayoutSettings {
        bolt: Some(BoltArea::picked(None, 0.016, None)),
        ..Default::default()
    }
}

fn run(text: &str) -> crate::LayoutRun {
    let lib = library();
    let bolts = Default::default();
    let s = settings();
    let ask = RunAsk {
        library: &lib,
        bolts: &bolts,
        settings: &s,
        preset_fields: BTreeMap::new(),
        joints: None,
        replace_hand: true,
    };
    lay_out(text, &ask).unwrap()
}

#[test]
fn pieces_in_line_of_one_section_are_proposed_and_never_written_by_a_run() {
    let t = parse(SPLIT);
    let list = in_line_pieces(&t, None);
    assert_eq!(list.len(), 1, "{list:?}");
    let x = &list[0];
    assert_eq!(
        (x.joint.as_str(), x.member.as_str(), x.onto.as_str()),
        ("M", "H1", "H2")
    );
    assert!(x.proposed(), "{x:?}");
    // A run says it and writes nothing of it (never a silent default).
    let r = run(SPLIT);
    assert_eq!(r.in_line, list);
    let after = parse(&r.text);
    assert!(after.continuous_with("H1", "M").is_none());
    // Confirmed: one part at both ends, proposed no longer.
    let text = confirm_text(SPLIT, &list).unwrap();
    let t = parse(&text);
    assert_eq!(t.continuous_with("H1", "M"), Some("H2"));
    assert_eq!(t.continuous_with("H2", "M"), Some("H1"));
    assert!(in_line_pieces(&t, None).is_empty());
    // A run replacing values given by hand keeps it (what the steel is,
    // not a layout value), and a run again changes nothing.
    let r = run(&text);
    let after = parse(&r.text);
    assert_eq!(after.continuous_with("H1", "M"), Some("H2"), "{}", r.text);
    assert!(r.in_line.is_empty());
    assert_eq!(run(&r.text).text, r.text);
}

#[test]
fn splices_given_and_pieces_turned_apart_are_not_proposed_legs_with_bolts_flagged() {
    // Pieces of a leg with bolts: a leg splice place (J7's), proposed one
    // part by the owner's rule and flagged so.
    let legs = SPLIT.replace("class = \"bracing\"", "class = \"leg\"");
    let list = in_line_pieces(&parse(&legs), None);
    assert_eq!(list.len(), 1);
    assert!(list[0].proposed() && list[0].splice_bolts, "{list:?}");
    assert!(!in_line_pieces(&parse(SPLIT), None)[0].splice_bolts);
    // A splice given at the joint.
    let spliced = format!(
        "{SPLIT}\n[[splice]]\nid = \"S1\"\njoint = \"M\"\nmembers = [\"H1\", \"H2\"]\n\n\
         [[splice.cover]]\nface = \"outer\"\nthickness = \"8 mm\"\n"
    );
    let list = in_line_pieces(&parse(&spliced), None);
    assert!(
        list[0].not.as_deref().is_some_and(|w| w.contains("splice")),
        "{list:?}"
    );
    // Bolts given at an end by hand.
    let bolted = format!(
        "{SPLIT}\n[[connection]]\nmember = \"H2\"\njoint = \"M\"\n\
         bolts = {{ count = 3, diameter = \"16 mm\" }}\n"
    );
    let list = in_line_pieces(&parse(&bolted), None);
    assert!(
        list[0]
            .not
            .as_deref()
            .is_some_and(|w| w.contains("bolts are given")),
        "{list:?}"
    );
    // Turned apart: the core would not read them one part.
    let turned = SPLIT.replacen("orientation = \"z+y-\"", "orientation = \"z+y+\"", 1);
    let list = in_line_pieces(&parse(&turned), None);
    assert!(
        list.first()
            .and_then(|x| x.not.as_deref())
            .is_some_and(|w| w.contains("turned alike")),
        "{list:?}"
    );
    // Confirming those writes nothing.
    assert_eq!(
        confirm_text(&turned, &in_line_pieces(&parse(&turned), None)).unwrap(),
        {
            let mut d: toml_edit::DocumentMut = turned.parse().unwrap();
            crate::in_line::confirm(&mut d, &parse(&turned), &[]).unwrap();
            d.to_string()
        }
    );
}

/// Pieces whose rolls are a little apart (Orient angles turns each to its
/// face on its own): proposed, and confirming gives both the roll half-way
/// between; more apart than the tolerance: not proposed.
#[test]
fn pieces_turned_a_little_apart_get_one_roll() {
    let h2 = "id = \"H2\"\nnodes = [\"M\", \"B\"]\nsection = \"EA-61x7\"\n";
    let a_little = SPLIT.replace(h2, &format!("{h2}roll = \"2 deg\"\n"));
    assert_ne!(a_little, SPLIT);
    let list = in_line_pieces(&parse(&a_little), None);
    assert!(list[0].proposed(), "{list:?}");
    let r = list[0].roll.expect("one roll written");
    assert!((r.to_degrees() - 1.0).abs() < 1e-9, "{r}");
    assert!(list[0].also().is_some());
    let text = confirm_text(&a_little, &list).unwrap();
    let t = parse(&text);
    assert_eq!(t.continuous_with("H1", "M"), Some("H2"), "{text}");
    let roll = |m: &str| t.members.iter().find(|x| x.id == m).unwrap().roll.clone();
    assert_eq!(roll("H1"), roll("H2"));
    let far = SPLIT.replace(h2, &format!("{h2}roll = \"12 deg\"\n"));
    let list = in_line_pieces(&parse(&far), None);
    assert!(
        list[0]
            .not
            .as_deref()
            .is_some_and(|w| w.contains("rolls 12.0° apart")),
        "{list:?}"
    );
}

/// The horizontal in two pieces with a diagonal to its middle instead of
/// the redundant.
fn diagonal() -> String {
    SPLIT.replace(
        "point = [\"0 m\", \"0 m\", \"1.5 m\"]",
        "point = [\"1.5 m\", \"0 m\", \"1.5 m\"]",
    )
}

/// The layout before and after confirming (a diagonal to the middle of a
/// horizontal in two pieces): not confirmed, the pieces end at the joint —
/// the diagonal is laid onto the piece on its side (whose steel lies under
/// its bolts), the splice whose place is not known said, and (L10) the
/// joint not whole: the pieces' own bolts (their design data) join
/// nothing there; confirmed, the horizontal runs on through, nothing said
/// of a splice, laid out whole; a run again changes nothing either way.
#[test]
fn the_layout_before_and_after_confirming() {
    let text = diagonal();
    let r = run(&text);
    let m = r.joints.iter().find(|o| o.joint == "M").unwrap();
    assert!(m.written, "{m:?}");
    for p in ["H1", "H2"] {
        assert!(
            m.hand.iter().any(|h| h.starts_with(&format!("{p}: "))
                && h.contains(crate::joint_layout::BOLTS_JOIN_NOTHING)),
            "{:?}",
            m.hand
        );
    }
    let host = |p: &crate::JointPlan| match &p.ends[0].onto {
        Onto::Member(h) => h.clone(),
        Onto::Plate(k) => p.plates[*k].on.clone(),
    };
    assert_eq!(host(&m.plan), "H2", "{m:?}");
    assert!(
        m.plan
            .left
            .iter()
            .any(|l| l.contains(crate::SPLICE_UNKNOWN)),
        "{:?}",
        m.plan.left
    );
    assert_eq!(run(&r.text).text, r.text);
    let text = confirm_text(&text, &in_line_pieces(&parse(&text), None)).unwrap();
    let r = run(&text);
    let m = r.joints.iter().find(|o| o.joint == "M").unwrap();
    assert!(m.written && m.hand.is_empty(), "{m:?}");
    assert!(
        m.plan
            .left
            .iter()
            .all(|l| !l.contains(crate::SPLICE_UNKNOWN)
                && !l.contains("each cut back onto the other")),
        "{:?}",
        m.plan.left
    );
    assert_eq!(parse(&r.text).continuous_with("H1", "M"), Some("H2"));
    assert_eq!(run(&r.text).text, r.text);
}

/// The redundant standing on the horizontal in two pieces (the guard's
/// joint): not confirmed, the pieces stay what it bolts onto, their own
/// ends left (D16); confirmed, they run on as one part. Laid out either
/// way, never onto a member whose own end the plan lays out.
#[test]
fn the_redundant_on_the_pieces_before_and_after_confirming() {
    let lib = library();
    for confirmed in [false, true] {
        let text = if confirmed {
            confirm_text(SPLIT, &in_line_pieces(&parse(SPLIT), None)).unwrap()
        } else {
            SPLIT.to_owned()
        };
        let d = joint_details(&text, &lib, &Default::default()).unwrap();
        // (Without L8's partner bolts: the guard's own scenario.)
        let s = LayoutSettings {
            partner_bolts: false,
            ..settings()
        };
        let p = plan_joint(d.joint("M").unwrap(), &s, &|_, _| None);
        let ends: Vec<&str> = p.ends.iter().map(|e| e.member.as_str()).collect();
        assert_eq!(ends, ["V"], "{p:?}");
        let mutual = p
            .left
            .iter()
            .filter(|l| l.contains("each cut back onto the other"))
            .count();
        assert_eq!(mutual, if confirmed { 0 } else { 2 }, "{:?}", p.left);
        if confirmed {
            assert!(
                p.left.iter().all(|l| l.contains("as one part")),
                "{:?}",
                p.left
            );
        }
    }
}
