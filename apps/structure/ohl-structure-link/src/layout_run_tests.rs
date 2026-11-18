// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The layout run on structure files: written, recorded, a run again
//! changing nothing, values given by hand kept or replaced, the plates
//! made (bolted or welded), following later changes.

use std::collections::{BTreeMap, BTreeSet};

use ohl_structures::{LibraryItem, StructureType};

use crate::joint_layout::{BoltArea, LayoutSettings};
use crate::joints::joint_details;
use crate::joints_tests::library;
use crate::layout_run::{LayoutRecord, RunAsk, follow, lay_out, provisional_bolts};

fn settings() -> LayoutSettings {
    LayoutSettings {
        bolt: Some(BoltArea::picked(None, 0.016, None)),
        ..Default::default()
    }
}

fn run(text: &str, s: &LayoutSettings, joints: Option<&[&str]>, replace: bool) -> crate::LayoutRun {
    let lib = library();
    let bolts = Default::default();
    let ask = RunAsk {
        library: &lib,
        bolts: &bolts,
        settings: s,
        preset_fields: BTreeMap::new(),
        joints: joints.map(|j| j.iter().map(|x| (*x).to_owned()).collect()),
        replace_hand: replace,
    };
    lay_out(text, &ask).unwrap()
}

fn parse(text: &str) -> StructureType {
    StructureType::parse(std::path::Path::new("s.ohls"), text).unwrap()
}

fn k_brace() -> String {
    crate::joint_layout_tests::K.to_owned()
}

#[test]
fn a_run_writes_the_layout_records_it_and_a_run_again_changes_nothing() {
    let text = k_brace();
    let r = run(&text, &settings(), Some(&["N2"]), false);
    let n2 = r.joints.iter().find(|j| j.joint == "N2").unwrap();
    assert!(n2.written, "{:?}", n2.hand);
    let t = parse(&r.text);
    // The values where they always are, the record beside them.
    for m in ["D1", "D2"] {
        let c = t.connection(m, "N2").expect("a connection");
        assert!(c.cut.is_some() && c.onto.is_some(), "{c:?}");
    }
    let rec = LayoutRecord::of(&t);
    assert!(rec.joints.contains("N2"));
    assert!(rec.ends.contains_key(&("D1".to_owned(), "N2".to_owned())));
    // Provisional bolts (none of their own): listed.
    assert_eq!(provisional_bolts(&t).len(), 2);
    // The body generators do not see the record.
    assert!(ohl_structure_edit::body::check(&t).is_ok());
    assert!(ohl_structure_edit::body::generators(&t).unwrap().is_empty());
    // No clash at N2 after; the joint details read it.
    let d = joint_details(&r.text, &library(), &Default::default()).unwrap();
    assert!(d.joint("N2").unwrap().clashes.is_empty());
    assert_eq!(r.clashes.1, 0);
    // A run again: the same file.
    let again = run(&r.text, &settings(), Some(&["N2"]), false);
    let diff: Vec<(&str, &str)> = again
        .text
        .lines()
        .zip(r.text.lines())
        .filter(|(a, b)| a != b)
        .collect();
    assert_eq!(again.text, r.text, "{diff:?}");
    // And over the whole tower.
    let all = run(&r.text, &settings(), None, false);
    let all2 = run(&all.text, &settings(), None, false);
    assert_eq!(all2.text, all.text);
}

#[test]
fn values_given_by_hand_are_kept_unless_replaced() {
    let text = format!(
        "{}\n[[connection]]\nmember = \"D1\"\njoint = \"N2\"\ncut = \"250 mm\"\n",
        k_brace()
    );
    let r = run(&text, &settings(), Some(&["N2"]), false);
    let t = parse(&r.text);
    let c = t.connection("D1", "N2").unwrap();
    assert_eq!(c.cut.map(|x| (x.si() * 1000.0).round()), Some(250.0));
    let rec = LayoutRecord::of(&t);
    let auto = &rec.ends[&("D1".to_owned(), "N2".to_owned())];
    assert!(!auto.contains("cut"), "{auto:?}");
    // Replaced: the layout's own.
    let r = run(&text, &settings(), Some(&["N2"]), true);
    let t = parse(&r.text);
    let c = t.connection("D1", "N2").unwrap();
    assert_ne!(c.cut.map(|x| (x.si() * 1000.0).round()), Some(250.0));
    assert!(LayoutRecord::of(&t).ends[&("D1".to_owned(), "N2".to_owned())].contains("cut"));
}

#[test]
fn a_file_without_the_record_reads_as_before() {
    let text = k_brace();
    let t = parse(&text);
    assert!(LayoutRecord::of(&t).is_empty());
    assert!(provisional_bolts(&t).is_empty());
    // The record alone (no values) changes no joint detail.
    let mut doc: toml_edit::DocumentMut = text.parse().unwrap();
    let rec = LayoutRecord {
        joints: BTreeSet::from(["N9".to_owned()]),
        ..Default::default()
    };
    rec.write(&mut doc);
    let with = doc.to_string();
    assert!(with.contains("kind = \"joint_layout\""));
    let a = joint_details(&text, &library(), &Default::default()).unwrap();
    let b = joint_details(&with, &library(), &Default::default()).unwrap();
    assert_eq!(a, b);
    assert_eq!(LayoutRecord::of(&parse(&with)), rec);
}

/// Six bolts on D1: they go onto a plate.
fn on_a_plate() -> String {
    k_brace().replacen(
        "design = { class = \"bracing\" }",
        "design = { class = \"bracing\", bolts = { diameter = \"16 mm\", count = 6 } }",
        1,
    )
}

#[test]
fn plates_are_made_bolted_or_welded_and_follow_a_thickness_given_by_hand() {
    let text = on_a_plate();
    let r = run(&text, &settings(), Some(&["N2"]), false);
    let t = parse(&r.text);
    assert_eq!(t.plates.len(), 1, "{}", r.text);
    let p = &t.plates[0];
    assert!((p.thickness.si() - 0.007).abs() < 1e-9);
    assert!(p.welded_to.is_empty());
    let rec = LayoutRecord::of(&t);
    assert!(rec.made[&p.id].1.contains("thickness"));
    // The plate made thicker by hand: the joint follows, the thickness
    // kept as given, now the user's.
    let mut doc: toml_edit::DocumentMut = r.text.parse().unwrap();
    ohl_structure_edit::connection_edits::set_plate(&mut doc, &p.id, Some(0.012), None).unwrap();
    let edited = doc.to_string();
    let f = follow(&r.text, &edited, &library(), &Default::default(), &[]).expect("followed");
    assert_eq!(f.again, ["N2"]);
    let t2 = parse(&f.text);
    let p2 = t2
        .plates
        .iter()
        .find(|x| x.id == p.id)
        .expect("the same plate");
    assert!((p2.thickness.si() - 0.012).abs() < 1e-9);
    assert!(!LayoutRecord::of(&t2).made[&p.id].1.contains("thickness"));
    // A run again keeps it.
    let again = run(&f.text, &settings(), Some(&["N2"]), false);
    let p3 = parse(&again.text)
        .plates
        .into_iter()
        .find(|x| x.id == p.id)
        .unwrap();
    assert!((p3.thickness.si() - 0.012).abs() < 1e-9);
    // No edit near a laid-out joint: nothing to follow.
    let far = edited.replace("name = \"k\"", "name = \"k2\"");
    assert!(follow(&edited, &far, &library(), &Default::default(), &[]).is_none());
}

#[test]
fn two_faces_at_a_corner_get_a_plate_each_welded_when_asked() {
    let text = crate::joint_layout_tests::corner_text().replace(
        "design = { class = \"bracing\" }",
        "design = { class = \"bracing\", bolts = { diameter = \"16 mm\", count = 6 } }",
    );
    let bolted = run(&text, &settings(), Some(&["N2"]), false);
    let t = parse(&bolted.text);
    let n2 = &bolted.joints[0];
    assert!(n2.written, "{:?}", n2.hand);
    assert_eq!(t.plates.len(), 2, "{:?}", n2.plan);
    assert!(t.plates.iter().all(|p| p.welded_to.is_empty()));
    let s = LayoutSettings {
        welded: true,
        ..settings()
    };
    let welded = run(&text, &s, Some(&["N2"]), false);
    let t = parse(&welded.text);
    assert_eq!(t.plates.len(), 2);
    assert_eq!(t.plates[1].welded_to, [t.plates[0].id.clone()]);
}

#[test]
fn an_edit_that_no_longer_fits_keeps_the_edit_and_the_old_layout() {
    let r = run(&k_brace(), &settings(), Some(&["N2"]), false);
    // The longest pull-back made tiny in the record: the joint no longer
    // fits; the edit is kept, the joint's values as before, flagged.
    let edited = r
        .text
        .replace("pull_back = \"300 mm\"", "pull_back = \"1 mm\"");
    assert_ne!(edited, r.text);
    let f = follow(&r.text, &edited, &library(), &Default::default(), &[]).expect("followed");
    assert_eq!(f.misfits.len(), 1, "{f:?}");
    assert!(f.message.contains("needs a hand detail"), "{}", f.message);
    let (before, after) = (parse(&r.text), parse(&f.text));
    for m in ["D1", "D2"] {
        assert_eq!(before.connection(m, "N2"), after.connection(m, "N2"));
    }
    let rec = LayoutRecord::of(&after);
    assert!(rec.hand.contains_key("N2"));
    assert!(f.text.contains("pull_back = \"1 mm\""));
}

#[test]
fn a_preset_gives_its_bolt_area() {
    let presets = ohl_structure_edit::connection_edits::parse_presets(
        "format = \"ohl-connection-presets\"\nversion = 1\n\n[[preset]]\nname = \"2 x M16\"\nbolts = { count = 2, diameter = \"16 mm\" }\ngauge_lines = 1\npitch = \"55 mm\"\n",
    )
    .unwrap();
    let all: Vec<(String, toml_edit::Table)> = presets
        .iter()
        .map(|p| (p.name.clone(), p.fields.clone()))
        .collect();
    let by = BTreeMap::from([("bracing".to_owned(), "2 x M16".to_owned())]);
    let (areas, fields) = crate::layout_run::presets_by_class(&by, &all, &Default::default());
    let a = &areas["bracing"];
    assert_eq!((a.count, a.lines), (2, 1));
    assert!((a.diameter - 0.016).abs() < 1e-12);
    assert_eq!(a.pitch.map(|p| (p * 1000.0).round()), Some(55.0));
    assert_eq!(a.preset.as_deref(), Some("2 x M16"));
    assert!(fields["bracing"].contains_key("bolts"));
    // A run with it: the preset's fields written, provisional.
    let s = LayoutSettings {
        presets: areas,
        ..settings()
    };
    let lib = library();
    let bolts = Default::default();
    let ask = RunAsk {
        library: &lib,
        bolts: &bolts,
        settings: &s,
        preset_fields: fields,
        joints: Some(BTreeSet::from(["N2".to_owned()])),
        replace_hand: false,
    };
    let r = lay_out(&k_brace(), &ask).unwrap();
    let t = parse(&r.text);
    let c = t.connection("D1", "N2").unwrap();
    assert_eq!(c.pitch.map(|p| (p.si() * 1000.0).round()), Some(55.0));
    assert_eq!(provisional_bolts(&t).len(), 2);
    let rec = LayoutRecord::of(&t);
    assert_eq!(
        rec.settings
            .unwrap()
            .presets
            .get("bracing")
            .map(String::as_str),
        Some("2 x M16")
    );
}

/// The K-brace with D1 on the outer face of the leg's flange at N2 but
/// turned 60° off it, by its reference (no roll of its own: the layout may
/// turn it).
fn d1_off() -> String {
    let t = k_brace().replacen(
        "orientation = \"y-z+\"",
        "reference = [-0.665, 0.5, -0.554]",
        1,
    );
    assert_ne!(t, k_brace());
    t
}

fn roll_of(text: &str, m: &str) -> Option<toml::Value> {
    parse(text)
        .members
        .into_iter()
        .find(|x| x.id == m)
        .and_then(|x| x.roll)
}

#[test]
fn a_member_is_turned_where_its_joint_is_laid_out_with_the_turn() {
    let r = run(&d1_off(), &settings(), Some(&["N2"]), false);
    let n2 = r.joints.iter().find(|j| j.joint == "N2").unwrap();
    assert!(n2.written, "{:?} {:?}", n2.hand, r.notes);
    assert!(
        n2.plan.ends.iter().any(|e| e.member == "D1"),
        "{:?}",
        n2.plan
    );
    assert_eq!(n2.turned, ["D1"]);
    assert!(roll_of(&r.text, "D1").is_some());
    let rec = LayoutRecord::of(&parse(&r.text));
    assert_eq!(rec.rolled.get("D1").map(String::as_str), Some(""));
    // A run again: the same.
    assert_eq!(run(&r.text, &settings(), Some(&["N2"]), false).text, r.text);
}

#[test]
fn a_turn_that_helps_no_joint_is_not_made() {
    // D1's cut given by hand too short at N2: D1 is left for a hand detail
    // there whatever its roll, and its other end lays out nothing: not
    // turned, said; the rest of N2 laid out.
    let text = format!(
        "{}\n[[connection]]\nmember = \"D1\"\njoint = \"N2\"\ncut = \"1 mm\"\n",
        d1_off()
    );
    let r = run(&text, &settings(), Some(&["N2"]), false);
    assert!(roll_of(&r.text, "D1").is_none(), "turned: {}", r.text);
    assert!(
        r.notes
            .iter()
            .any(|n| n.starts_with("D1: not turned: no joint of it is laid out")),
        "{:?}",
        r.notes
    );
    let rec = LayoutRecord::of(&parse(&r.text));
    assert!(!rec.rolled.contains_key("D1"));
    let n2 = r.joints.iter().find(|j| j.joint == "N2").unwrap();
    assert!(n2.turned.is_empty());
    assert!(
        n2.plan.hand_members.iter().any(|m| m == "D1"),
        "{:?}",
        n2.plan
    );
}

#[test]
fn a_turn_that_would_spoil_an_end_that_was_flat_is_not_made() {
    // The legs turned 30° (by hand): D1 lies 30° off at N2; at N4 it
    // bolts onto the bottom chord H0–H1 (running on through N4), flat
    // there. Laying out N2 alone may not turn it.
    let text = format!(
        "{}\n[[node]]\nid = \"N6\"\npoint = [\"5 m\", \"0 m\", \"0 m\"]\nsupport = \"fixed\"\n\
         \n[[member]]\nid = \"H0\"\nnodes = [\"N1\", \"N4\"]\nsection = \"EA-61x7\"\n\
         design = {{ class = \"bracing\" }}\n\
         \n[[member]]\nid = \"H1\"\nnodes = [\"N4\", \"N6\"]\nsection = \"EA-61x7\"\n\
         design = {{ class = \"bracing\" }}\n\
         \n[[connection]]\nmember = \"D1\"\njoint = \"N4\"\nonto = \"H0\"\n",
        k_brace().replace(
            "orientation = \"x+y+\"\n",
            "orientation = \"x+y+\"\nroll = \"30 deg\"\n"
        )
    );
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    let flat = |j: &str| {
        d.joint(j)
            .unwrap()
            .arrivals
            .iter()
            .find(|a| a.member == "D1")
            .and_then(|a| a.flat)
            .map(|f| f.off)
    };
    assert!(flat("N2").unwrap() > 20.0, "{:?}", flat("N2"));
    assert!(flat("N4").unwrap() < 1.0, "{:?}", flat("N4"));
    let r = run(&text, &settings(), Some(&["N2"]), false);
    assert!(roll_of(&r.text, "D1").is_none());
    assert!(
        r.notes
            .iter()
            .any(|n| n.starts_with("D1:") && n.contains("spoil")),
        "{:?}",
        r.notes
    );
}

#[test]
fn an_automatic_roll_is_taken_back_when_its_joints_become_hand_details() {
    let r = run(&d1_off(), &settings(), Some(&["N2"]), false);
    assert!(roll_of(&r.text, "D1").is_some());
    // D1's cut at N2 given by hand, too short: N2 laid out again (the
    // follow), D1 now a hand detail there and nowhere laid out: its roll
    // back to what it was (none).
    let mut doc: toml_edit::DocumentMut = r.text.parse().unwrap();
    ohl_structure_edit::connection_edits::set_connection_field(
        &mut doc,
        "D1",
        "N2",
        "cut",
        Some(&ohl_structure_edit::connection_edits::FieldValue::Length(
            0.001,
        )),
    )
    .unwrap();
    let edited = doc.to_string();
    let f = follow(&r.text, &edited, &library(), &Default::default(), &[]).expect("followed");
    assert!(roll_of(&f.text, "D1").is_none(), "{}", f.text);
    assert!(!LayoutRecord::of(&parse(&f.text)).rolled.contains_key("D1"));
}
