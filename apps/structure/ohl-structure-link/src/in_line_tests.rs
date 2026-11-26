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
pub(crate) fn diagonal() -> String {
    SPLIT.replace(
        "point = [\"0 m\", \"0 m\", \"1.5 m\"]",
        "point = [\"1.5 m\", \"0 m\", \"1.5 m\"]",
    )
}

/// The layout before and after confirming (a diagonal to the middle of a
/// horizontal in two pieces): not confirmed, the pieces end at the joint —
/// the diagonal is laid onto a plate on the piece on its side (whose steel
/// lies under its bolts), the splice whose place is not known said; (L14)
/// that piece's own end bolts (its design data) bolted onto the plate on
/// its flange, the plate's holding bolts (`held_by`, version 24), checked
/// as such by the joint checks; (L10) the joint still not whole: the other
/// piece's own bolts join nothing there; confirmed, the horizontal runs on
/// through, nothing said of a splice, laid out whole; a run again changes
/// nothing either way.
#[test]
fn the_layout_before_and_after_confirming() {
    let text = diagonal();
    let r = run(&text);
    let m = r.joints.iter().find(|o| o.joint == "M").unwrap();
    assert!(m.written, "{m:?}");
    assert!(
        m.hand
            .iter()
            .any(|h| h.starts_with("H1: ") && h.contains(crate::joint_layout::BOLTS_JOIN_NOTHING)),
        "{:?}",
        m.hand
    );
    assert!(
        !m.hand.iter().any(|h| h.starts_with("H2: ")),
        "{:?}",
        m.hand
    );
    let h2 = m
        .plan
        .ends
        .iter()
        .find(|e| e.member == "H2")
        .expect("H2 laid");
    let Onto::Plate(k) = h2.onto else {
        panic!("{h2:?}")
    };
    assert_eq!(m.plan.plates[k].on, "H2");
    assert!(m.plan.plates[k].hold.is_none(), "{:?}", m.plan.plates[k]);
    assert!(
        m.plan
            .notes
            .iter()
            .any(|n| n.starts_with("H2: ") && n.contains(crate::joint_layout::BASE_HOLDS)),
        "{:?}",
        m.plan.notes
    );
    let t = parse(&r.text);
    let g = t.plates.iter().find(|p| p.joint == "M").expect("a plate");
    assert_eq!(g.held_by.as_deref(), Some("H2"));
    assert!(
        t.connections.iter().any(|c| c.member == "H2"
            && c.joint == "M"
            && c.onto.as_deref() == Some(g.id.as_str())),
        "{}",
        r.text
    );
    // The joint checks: H2's bolts on the plate are its holding bolts.
    let plans = crate::plate_plans(&r.text, &library(), &Default::default()).unwrap();
    let rows = crate::plate_usage(&plans, &BTreeMap::new(), 1.0);
    assert!(
        rows.iter()
            .any(|x| x.plate == g.id && x.part == crate::PlatePart::Holding("H2".into())),
        "{rows:?}"
    );
    // Off: as before (H2 by hand too).
    let off = LayoutSettings {
        base_holds: false,
        ..settings()
    };
    let lib = library();
    let bolts = Default::default();
    let r0 = lay_out(
        &text,
        &RunAsk {
            library: &lib,
            bolts: &bolts,
            settings: &off,
            preset_fields: BTreeMap::new(),
            joints: None,
            replace_hand: true,
        },
    )
    .unwrap();
    let m0 = r0.joints.iter().find(|o| o.joint == "M").unwrap();
    assert!(
        m0.hand
            .iter()
            .any(|h| h.starts_with("H2: ") && h.contains(crate::joint_layout::BOLTS_JOIN_NOTHING)),
        "{:?}",
        m0.hand
    );
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

/// L16 (L15's item 4): the base member's end bolts hold the plate on its
/// flange only where no end left as it is bolts onto it directly — also as
/// the file written gives that end: a member the run named onto another
/// only while planning, left as it is with no connection at the joint, is
/// written without the name and bolts onto the joint's leg worked out for
/// it; where that is the base member, the base member does not hold the
/// plate (its bolts would share its flange with that end's).
#[test]
fn an_end_left_with_no_connection_onto_the_base_member_keeps_it_from_holding() {
    let text = diagonal();
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    let j = d.joint("M").unwrap();
    let plan = plan_joint(j, &settings(), &|_, _| None);
    // As the run plans it: H2 holds the plate on its flange.
    let mut p = plan.clone();
    assert_eq!(
        crate::joint_layout::hold_by_base(&mut p, j, &|_| None),
        1,
        "{p:?}"
    );
    // An invented end left as it is (a copy of V's, "X"): named onto H1
    // only while planning, its leg worked out as H2.
    let mut j2 = j.clone();
    let mut x = j.arrivals.iter().find(|a| a.member == "V").unwrap().clone();
    x.member = "X".into();
    x.onto = crate::Valued {
        value: Some("H1".into()),
        source: crate::Source::Given,
    };
    x.joint_leg = Some("H2".into());
    j2.arrivals.push(x);
    // Written without a connection there: onto H2 worked out — H2 holds
    // nothing.
    let mut p = plan.clone();
    assert_eq!(
        crate::joint_layout::hold_by_base(&mut p, &j2, &|_| None),
        0,
        "{p:?}"
    );
    assert!(
        p.hand
            .iter()
            .any(|h| h.starts_with("H2: ") && h.contains(crate::joint_layout::BOLTS_JOIN_NOTHING)),
        "{:?}",
        p.hand
    );
    // Written onto H1 (given so in the file): H2 holds the plate.
    let mut p = plan.clone();
    let on_h1 = |m: &str| (m == "X").then(|| "H1".to_owned());
    assert_eq!(
        crate::joint_layout::hold_by_base(&mut p, &j2, &on_h1),
        1,
        "{p:?}"
    );
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

/// O44 L27: a piece confirmed onto a part already one (a run's turns
/// brought its roll within the tolerance of the part's): the part's pieces
/// join the chain and all get its one roll — never the new pair alone,
/// which would leave the part's pieces turned apart (the core then reads
/// them as two pieces again).
#[test]
fn a_piece_confirmed_onto_a_part_already_one_turns_the_whole_part() {
    let three = format!(
        "{SPLIT}\n[[node]]\nid = \"C\"\npoint = [\"4 m\", \"0 m\", \"0 m\"]\nsupport = \"fixed\"\n\n\
         [[member]]\nid = \"H3\"\nnodes = [\"B\", \"C\"]\nsection = \"EA-61x7\"\n\
         orientation = \"z+y-\"\nroll = \"1.5 deg\"\n\
         design = {{ class = \"bracing\", bolts = {{ diameter = \"16 mm\", count = 2 }} }}\n"
    );
    // H1 and H2 confirmed first (alike: no roll written).
    let list = in_line_pieces(&parse(&three), None);
    let at_m: Vec<_> = list.iter().filter(|x| x.joint == "M").cloned().collect();
    assert_eq!(at_m.len(), 1, "{list:?}");
    let once = confirm_text(&three, &at_m).unwrap();
    let t = parse(&once);
    assert_eq!(t.continuous_with("H1", "M"), Some("H2"), "{once}");
    // Then H3 onto H2 (1.5° apart): proposed, confirmed.
    let list = in_line_pieces(&t, None);
    assert_eq!(list.len(), 1, "{list:?}");
    assert!(list[0].proposed() && list[0].joint == "B", "{list:?}");
    let twice = confirm_text(&once, &list).unwrap();
    let t = parse(&twice);
    let member = |m: &str| t.members.iter().find(|x| x.id == m).unwrap();
    assert_eq!(member("H1").roll, member("H2").roll, "{twice}");
    assert_eq!(member("H2").roll, member("H3").roll, "{twice}");
    assert_eq!(t.continuous_with("H1", "M"), Some("H2"), "{twice}");
    assert_eq!(t.continuous_with("H2", "B"), Some("H3"), "{twice}");
    // The mean of the three (0°, 0°, 1.5°): 0.5°.
    let deg = ohl_structure_edit::edits::member_roll(member("H1")).to_degrees();
    assert!((deg - 0.5).abs() < 0.01, "{deg}");
    assert!(in_line_pieces(&t, None).is_empty());
}
