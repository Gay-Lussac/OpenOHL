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
    assert_eq!(again.text, r.text);
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
    let f = follow(
        &r.text,
        &edited,
        &library(),
        &Default::default(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("followed");
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
    assert!(
        follow(
            &edited,
            &far,
            &library(),
            &Default::default(),
            &BTreeMap::new(),
            &BTreeMap::new()
        )
        .is_none()
    );
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
    let f = follow(
        &r.text,
        &edited,
        &library(),
        &Default::default(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("followed");
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
