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
    // Provisional bolts (none of their own): listed — D1's, D2's and
    // (L5o) their plate's own bolts onto the leg.
    assert_eq!(provisional_bolts(&t).len(), 3);
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
fn an_onto_given_by_hand_is_kept_when_the_end_goes_onto_a_plate_on_that_member() {
    // (O44 L2's follow-up, 2026-11-20.) Where the layout puts D1 onto a
    // plate: the member that plate lies on.
    let text = on_a_plate();
    let r = run(&text, &settings(), Some(&["N2"]), false);
    let t = parse(&r.text);
    assert_eq!(t.plates.len(), 1, "{}", r.text);
    let host = t.plates[0].on.clone().expect("the plate names its member");
    // The same, given by hand: written onto the plate, the record keeps the
    // hand value (not the layout's), and a run again keeps it.
    let given =
        format!("{text}\n[[connection]]\nmember = \"D1\"\njoint = \"N2\"\nonto = \"{host}\"\n");
    let r = run(&given, &settings(), Some(&["N2"]), false);
    let t = parse(&r.text);
    let plate = t.plates.first().expect("a plate").id.clone();
    assert_eq!(
        t.connection("D1", "N2").and_then(|c| c.onto.clone()),
        Some(plate)
    );
    let key = ("D1".to_owned(), "N2".to_owned());
    let mark = format!("{}{host}", crate::layout_run::ONTO_HAND);
    let rec = LayoutRecord::of(&t);
    assert!(rec.ends[&key].contains(&mark), "{:?}", rec.ends[&key]);
    assert!(!rec.ends[&key].contains("onto"));
    let again = run(&r.text, &settings(), Some(&["N2"]), false);
    assert_eq!(again.text, r.text);
    assert!(LayoutRecord::of(&parse(&again.text)).ends[&key].contains(&mark));
    // Values given by hand replaced: the layout's own.
    let replaced = run(&given, &settings(), Some(&["N2"]), true);
    let rec = LayoutRecord::of(&parse(&replaced.text));
    assert!(rec.ends[&key].contains("onto"), "{:?}", rec.ends[&key]);
    assert!(!rec.ends[&key].contains(&mark));
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
    // The legs a bolted splice (not joined as one part: nothing else to
    // lay out at N2 than the diagonals).
    let text = k_brace().replace(
        "design = { class = \"leg\" }",
        "design = { class = \"leg\", bolts = { diameter = \"16 mm\", count = 4 } }",
    );
    let r = run(&text, &settings(), Some(&["N2"]), false);
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
    // (D1's, D2's and their plate's own bolts onto the leg, L5o.)
    assert_eq!(provisional_bolts(&t).len(), 3);
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

/// A tall body (eight panels) for the faster follow: a few joints are
/// worked out from the file cut down to what they read.
fn tall() -> String {
    let levels: Vec<(f64, f64, f64)> = (0..9)
        .map(|i| {
            let w = 3.0 - 0.2 * i as f64;
            (3.0 * i as f64, w, w)
        })
        .collect();
    crate::orient_tests::body(&levels, &[2, 5])
}

#[test]
fn a_few_joints_in_detail_from_the_file_cut_down_are_as_from_the_whole() {
    let text =
        crate::orient::orient_angles(&tall(), &library(), &crate::orient::OrientAsk::default())
            .unwrap()
            .text;
    let doc: toml_edit::DocumentMut = text.parse().unwrap();
    for j in ["J10", "J21", "J52", "J83"] {
        let joints = BTreeSet::from([j.to_owned()]);
        let keep = crate::reduce::members_for(&doc, &joints);
        assert!(crate::reduce::worth(&doc, &keep));
        let small = crate::reduce::cut_down(&doc, &keep);
        let a =
            crate::joints::joint_details_at(&text, &library(), &Default::default(), Some(&joints))
                .unwrap();
        let b = crate::joints::joint_details_at(
            &small.to_string(),
            &library(),
            &Default::default(),
            Some(&joints),
        )
        .unwrap();
        assert_eq!(a.joints, b.joints, "{j}");
        assert!(!a.joints.is_empty());
    }
    // A run on a few joints and its follow: the same file either way.
    let s = settings();
    let lib = library();
    let bolts = Default::default();
    let ask = RunAsk {
        library: &lib,
        bolts: &bolts,
        settings: &s,
        preset_fields: BTreeMap::new(),
        joints: Some(BTreeSet::from(["J21".to_owned(), "J52".to_owned()])),
        replace_hand: false,
    };
    let fast = lay_out(&text, &ask).unwrap();
    let whole = crate::layout_run::lay_out_whole(&text, &ask).unwrap();
    assert_eq!(fast.text, whole.text);
    assert_eq!(fast.joints, whole.joints);
    assert_eq!(fast.clashes, whole.clashes);
    let edited = fast.text.replacen("gap = \"10 mm\"", "gap = \"14 mm\"", 1);
    assert_ne!(edited, fast.text);
    let a = follow(&fast.text, &edited, &lib, &bolts, &[]).map(|f| f.text);
    let b = crate::layout_run::follow_whole(&fast.text, &edited, &lib, &bolts, &[]).map(|f| f.text);
    assert_eq!(a, b);
}

/// `text` with member `m`'s roll set to `deg` degrees (its orientation
/// kept: the roll turns it further).
fn rolled(text: &str, m: &str, deg: f64) -> String {
    let at = format!("id = \"{m}\"\nnodes = ");
    let i = text.find(&at).unwrap();
    let end = text[i..].find("design = ").unwrap() + i;
    format!("{}roll = \"{deg} deg\"\n{}", &text[..end], &text[end..])
}

/// Tuning on real towers (2026-11-18): an X-brace's crossing diagonal
/// bolting onto nothing named, whatever its roll — its leg bolted not the
/// one lying on the diagonal running through, or on that one's inner face
/// running towards its other leg — is laid out: the member running
/// through named first (its leg lying on it bolted, a turn laying it
/// flat), half a turn where its inner face does not fit.
#[test]
fn a_crossing_diagonal_is_laid_out_whatever_its_roll() {
    let x = crate::joint_layout_tests::X;
    for deg in [0.0, 45.0, 90.0, 180.0, 270.0] {
        // The rolls automatic (as Orient angles sets them).
        let text = format!(
            "{}\n[[generator]]\nid = \"joint-layout\"\nkind = \"joint_layout\"\n\
             oriented = [[\"X2a\", \"\", \"face\"], [\"X2b\", \"\", \"face\"]]\n",
            rolled(&rolled(x, "X2a", deg), "X2b", deg)
        );
        let r = run(&text, &settings(), None, false);
        let c = r.joints.iter().find(|j| j.joint == "C").unwrap();
        assert!(c.written, "{deg}°: {:?} {:?}", c.hand, r.notes);
        let ends: BTreeSet<&str> = c.plan.ends.iter().map(|e| e.member.as_str()).collect();
        assert_eq!(ends, BTreeSet::from(["X2a", "X2b"]), "{deg}°: {:?}", c.plan);
        assert!(c.hand.is_empty(), "{deg}°: {:?}", c.hand);
        let d = joint_details(&r.text, &library(), &Default::default()).unwrap();
        let j = d.joint("C").unwrap();
        assert!(j.clashes.is_empty(), "{deg}°: {:?}", j.clashes);
        // Onto the diagonal running through (or a plate on it), flat.
        for a in j.arrivals.iter().filter(|a| a.member.starts_with("X2")) {
            assert!(
                a.onto
                    .value
                    .as_deref()
                    .is_some_and(|o| o.starts_with("X1") || c.plates.iter().any(|p| p == o)),
                "{deg}°: {a:?}"
            );
            assert!(a.flat.is_some_and(|f| f.off < 0.5), "{deg}°: {:?}", a.flat);
        }
        // A run again changes nothing.
        assert_eq!(
            run(&r.text, &settings(), None, false).text,
            r.text,
            "{deg}°"
        );
    }
}

/// A vertical post V (class leg), a horizontal running through the joint
/// (H1, H2 one part) and a plan diagonal G meeting them there: G
/// horizontal, in no flange plane of the post.
const POST: &str = r#"id = "P"
name = "p"
kind = "lattice"

[[node]]
id = "B0"
point = ["0 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "P"
point = ["0 m", "0 m", "3 m"]

[[node]]
id = "W"
point = ["-3 m", "0 m", "3 m"]
support = "fixed"

[[node]]
id = "E"
point = ["3 m", "0 m", "3 m"]
support = "fixed"

[[node]]
id = "G0"
point = ["2 m", "2 m", "3 m"]
support = "fixed"

[[member]]
id = "V"
nodes = ["B0", "P"]
section = "EA-61x7"
design = { class = "leg" }

[[member]]
id = "H1"
nodes = ["W", "P"]
section = "EA-61x7"
design = { class = "bracing" }

[[member]]
id = "H2"
nodes = ["P", "E"]
section = "EA-61x7"
design = { class = "bracing" }

[[member]]
id = "G"
nodes = ["P", "G0"]
section = "EA-61x7"
design = { class = "crossing" }

[[connection]]
member = "H1"
joint = "P"
kind = "continuous"
onto = "H2"
"#;

/// Tuning on real towers (2026-11-18): a plan diagonal whose worked-out
/// `onto` is a vertical post (no turn lays it flat there: its axis lies in
/// neither flange's plane) bolts onto the horizontal running through the
/// joint in whose flange's plane it lies.
#[test]
fn a_plan_diagonal_bolts_onto_the_horizontal_running_through() {
    let d = joint_details(POST, &library(), &Default::default()).unwrap();
    let p = d.joint("P").unwrap();
    let g = p.arrivals.iter().find(|a| a.member == "G").unwrap();
    assert_eq!(g.onto.value.as_deref(), Some("V"), "{g:?}");
    let named = crate::joint_layout::hosts_to_name(p);
    assert!(
        named.iter().any(|(m, h)| m == "G" && h.starts_with('H')),
        "{named:?}"
    );
    // In the run G is laid out onto the horizontal, or left with a reason
    // on the horizontal's flange — never measured against the post's.
    let r = run(POST, &settings(), Some(&["P"]), false);
    let o = r.joints.iter().find(|j| j.joint == "P").unwrap();
    match o.plan.ends.iter().find(|e| e.member == "G") {
        Some(ge) => {
            let on = match &ge.onto {
                crate::joint_layout::Onto::Member(h) => h.clone(),
                crate::joint_layout::Onto::Plate(k) => o.plan.plates[*k].on.clone(),
            };
            assert!(on.starts_with('H'), "{ge:?}");
        }
        None => {
            let why: Vec<&String> = o.hand.iter().filter(|h| h.starts_with("G:")).collect();
            assert!(!why.is_empty(), "{:?}", o.hand);
            for h in why {
                assert!(h.contains("H1") || h.contains("H2"), "{h}");
                assert!(!h.contains("axis runs"), "{h}");
            }
        }
    }
}

/// Tuning on real towers (2026-11-18): bolts that do not all land on an
/// inner face go onto a plate there (clear of the angle's other leg and
/// root), and the joint fits.
#[test]
fn bolts_off_an_inner_face_go_onto_a_plate_clear_of_the_other_leg() {
    let k = k_brace().replace(
        "design = { class = \"bracing\" }",
        "design = { class = \"bracing\", bolts = { diameter = \"16 mm\", count = 6 } }",
    );
    // Half a turn more: on the inner face of the leg's flange.
    let text = rolled(&rolled(&k, "D1", 180.0), "D2", 180.0);
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    let n2 = d.joint("N2").unwrap();
    for m in ["D1", "D2"] {
        let a = n2.arrivals.iter().find(|a| a.member == m).unwrap();
        assert_eq!(a.sits.value, Some(crate::Face::Inside), "{m}: {a:?}");
    }
    let p = crate::joint_layout::plan_joint(n2, &settings(), &|_, _| None);
    assert!(p.laid_out(), "{:?}", p.hand);
    assert_eq!(p.plates.len(), 1, "{p:?}");
    let r = run(&text, &settings(), Some(&["N2"]), false);
    let o = r.joints.iter().find(|j| j.joint == "N2").unwrap();
    assert!(o.written, "{:?} {:?}", o.hand, r.notes);
    assert_eq!(o.plates.len(), 1, "{:?}", o.plan);
    let d = joint_details(&r.text, &library(), &Default::default()).unwrap();
    assert!(
        d.joint("N2").unwrap().clashes.is_empty(),
        "{:?}",
        d.joint("N2").unwrap().clashes
    );
}

/// Tuning on real towers (2026-11-18): a member whose laid-out steel would
/// run into something leaves the rest of its joint laid out — it alone is
/// left as it was, with the clash (R1 on a plate given by hand, its steel
/// where D1 would go). Since J1 (2026-11-19) the plan itself sees R1's
/// steel (fixed: not laid out) and leaves D1 with it.
#[test]
fn one_members_clash_does_not_take_its_whole_joint_back() {
    let text = format!(
        "{}\n[[node]]\nid = \"N6\"\npoint = [\"2.5 m\", \"0 m\", \"0.13 m\"]\nsupport = \"fixed\"\n\
         \n[[member]]\nid = \"R1\"\nnodes = [\"N6\", \"N2\"]\nsection = \"EA-61x7\"\n\
         orientation = \"y-z+\"\ndesign = {{ class = \"redundant\" }}\n\
         \n[[plate]]\nid = \"GH\"\njoint = \"N2\"\nthickness = \"8 mm\"\n\
         \n[[connection]]\nmember = \"R1\"\njoint = \"N2\"\nonto = \"GH\"\ncut = \"0 mm\"\n",
        k_brace()
    );
    let r = run(&text, &settings(), Some(&["N2"]), false);
    let o = r.joints.iter().find(|j| j.joint == "N2").unwrap();
    assert!(o.written, "{:?} {:?}", o.hand, r.notes);
    assert!(o.plan.ends.iter().any(|e| e.member == "D2"), "{:?}", o.plan);
    assert!(
        o.plan.hand_members.iter().any(|m| m == "D1"),
        "{:?}",
        o.plan
    );
    assert!(
        o.hand
            .iter()
            .any(|h| h.starts_with("D1: ") && h.contains("R1")),
        "{:?}",
        o.hand
    );
    // What was laid out runs into nothing.
    let d = joint_details(&r.text, &library(), &Default::default()).unwrap();
    let j = d.joint("N2").unwrap();
    for c in &j.clashes {
        let m = [c.a.member(), c.b.member()];
        assert!(!m.contains(&Some("D2")), "{c:?}");
    }
}

/// Two chords in line through J (C1 below, C2 above, of `upper`'s
/// section, `bolts` their design data) and a brace D to J.
pub(crate) fn in_line(upper: &str, bolts: &str) -> String {
    format!(
        r#"id = "I"
name = "i"
kind = "lattice"

[[node]]
id = "A"
point = ["0 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "J"
point = ["0 m", "0 m", "3 m"]

[[node]]
id = "B"
point = ["0 m", "0 m", "6 m"]
support = "fixed"

[[node]]
id = "E"
point = ["2.5 m", "0 m", "0 m"]
support = "fixed"

[[member]]
id = "C1"
nodes = ["A", "J"]
section = "EA-61x7"
orientation = "x+y+"
design = {{ class = "bracing"{bolts} }}

[[member]]
id = "C2"
nodes = ["J", "B"]
section = "{upper}"
orientation = "x+y+"
design = {{ class = "bracing"{bolts} }}

[[member]]
id = "D"
nodes = ["E", "J"]
section = "EA-61x7"
orientation = "y-z+"
design = {{ class = "bracing" }}
"#
    )
}

/// The example sections and a smaller invented angle that nests in the
/// 61 mm one.
fn library_with_small() -> crate::SectionLibrary {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../libraries/sections/example-sections.ohlsec");
    let text = std::fs::read_to_string(path).unwrap();
    crate::SectionLibrary::parse(&format!(
        "{text}\n[[section]]\nid = \"EA-41x5\"\nname = \"Equal angle 41 × 5 (invented)\"\n\
         shape = \"equal_angle\"\nh = \"41 mm\"\nt = \"5 mm\"\nr = \"5 mm\"\n"
    ))
    .unwrap()
}

fn run_with(text: &str, lib: &crate::SectionLibrary) -> crate::LayoutRun {
    let bolts = Default::default();
    let s = settings();
    let ask = RunAsk {
        library: lib,
        bolts: &bolts,
        settings: &s,
        preset_fields: BTreeMap::new(),
        joints: None,
        replace_hand: false,
    };
    lay_out(text, &ask).unwrap()
}

/// The owner's step 1 (2026-11-18): the members in line through a joint
/// joined first — one part when they are the same section, turned alike
/// and bolt-free; the smaller nested in the bigger when it fits; a bolted
/// splice left as it is (its bolts stay checked).
#[test]
fn members_in_line_are_joined_first_one_part_or_nested() {
    let lib = library_with_small();
    // The same, turned alike, no bolts: one part.
    let r = run_with(&in_line("EA-61x7", ""), &lib);
    let o = r.joints.iter().find(|j| j.joint == "J").unwrap();
    assert!(o.written, "{:?}", o.hand);
    assert_eq!(o.plan.joins.len(), 1, "{:?}", o.plan);
    assert_eq!(o.plan.joins[0].how, crate::JoinKind::Continuous);
    let d = joint_details(&r.text, &lib, &Default::default()).unwrap();
    let j = d.joint("J").unwrap();
    for m in ["C1", "C2"] {
        let a = j.arrivals.iter().find(|a| a.member == m).unwrap();
        assert!(a.continuous.is_some(), "{m}: {a:?}");
    }
    assert!(j.clashes.is_empty(), "{:?}", j.clashes);
    assert_eq!(run_with(&r.text, &lib).text, r.text);
    // With bolts (a splice whose bolts are checked): not one part.
    let r = run_with(
        &in_line("EA-61x7", ", bolts = { diameter = \"16 mm\", count = 4 }"),
        &lib,
    );
    let o = r.joints.iter().find(|j| j.joint == "J").unwrap();
    assert!(o.plan.joins.is_empty(), "{:?}", o.plan);
    // The smaller angle fits in the bigger: nested, bolted to it.
    let r = run_with(&in_line("EA-41x5", ""), &lib);
    let o = r.joints.iter().find(|j| j.joint == "J").unwrap();
    assert!(o.written, "{:?}", o.hand);
    assert_eq!(o.plan.joins.len(), 1, "{:?}", o.plan);
    assert_eq!(o.plan.joins[0].how, crate::JoinKind::Nested);
    assert_eq!(o.plan.joins[0].member, "C2");
    let d = joint_details(&r.text, &lib, &Default::default()).unwrap();
    let j = d.joint("J").unwrap();
    let c2 = j.arrivals.iter().find(|a| a.member == "C2").unwrap();
    assert_eq!(c2.sits.value, Some(crate::Face::Nested), "{c2:?}");
    assert_eq!(c2.onto.value.as_deref(), Some("C1"));
    assert!(!c2.misfit, "{:?}", c2.notes);
    assert!(j.clashes.is_empty(), "{:?}", j.clashes);
    assert_eq!(run_with(&r.text, &lib).text, r.text);
}

/// The K-brace with its legs turned `deg` by hand: D1 lies off the leg's
/// flange at N2 and flat on the bottom chord H0–H1 running through N4 —
/// no roll lays it flat at both its ends.
/// The value `field` of the layout's plate `id` the user's (as an edit's
/// follow marks it).
fn yours(doc: &mut toml_edit::DocumentMut, id: &str, field: &str) {
    let t = parse(&doc.to_string());
    let mut rec = LayoutRecord::of(&t);
    if let Some(m) = rec.made.get_mut(id) {
        m.1.remove(field);
    }
    rec.write(doc);
}

/// L3 (2026-11-20): an end whose leg lies a few degrees off its flange, no
/// roll laying both its ends flat, goes onto a plate bent by as much from
/// the flat plate on the flange (`[plate.bend]`, no weld); its member lies
/// flat on it, nothing clashes, its bend is a row of the joint checks, its
/// weight is the developed plate's, a run again is the same and the follow
/// of an edit is as the whole file's (a flat plate's thickness given by
/// hand kept).
#[test]
fn an_end_a_few_degrees_off_goes_onto_a_plate_bent_by_as_much() {
    let text = d1_spoiled(10.0);
    let r = run(&text, &settings(), Some(&["N2"]), false);
    let o = r.joints.iter().find(|j| j.joint == "N2").unwrap();
    assert!(o.written, "{:?} {:?}", o.hand, r.notes);
    let e = o
        .plan
        .ends
        .iter()
        .find(|e| e.member == "D1")
        .unwrap_or_else(|| panic!("D1 not laid out: {:?} {:?}", o.plan, r.notes));
    let crate::Onto::Plate(k) = e.onto else {
        panic!("{e:?}")
    };
    let own = o.plan.plates[k].own.as_ref().expect("in its own plane");
    let bend = own.bend.as_ref().expect("bent");
    assert!(
        bend.angle > crate::FLAT_OFF && bend.angle <= crate::joint_layout::BEND_SMALL_MAX,
        "{bend:?}"
    );
    assert_eq!((own.weld_size, own.weld_length), (0.0, 0.0));
    assert!(own.out_more.abs() <= 0.007 + 1e-9, "{own:?}");
    // Written: bent from the flat plate, no weld, the record's marks.
    let t = parse(&r.text);
    let id = &o.plates[k];
    let p = t.plates.iter().find(|p| &p.id == id).unwrap();
    assert!(p.welds.is_empty(), "{p:?}");
    let b = p.bend.as_ref().expect("[plate.bend]");
    assert_eq!(b.to, o.plates[own.welded_to]);
    assert!((b.angle.si().to_degrees() - bend.angle).abs() < 0.01);
    let rec = LayoutRecord::of(&t);
    assert!(rec.made[id].1.contains(crate::layout_run::OWN_PLANE));
    assert!(rec.made[id].1.contains(crate::layout_run::BENT));
    // D1 lies flat on it, no clash.
    let d = joint_details(&r.text, &library(), &Default::default()).unwrap();
    let j = d.joint("N2").unwrap();
    let d1 = j.arrivals.iter().find(|a| a.member == "D1").unwrap();
    assert!(d1.flat.is_some_and(|f| f.off < 1.0), "{:?}", d1.flat);
    assert!(!d1.misfit, "{:?}", d1.notes);
    assert!(j.clashes.is_empty(), "{:?}", j.clashes);
    // Its bend a row of the joint checks (no code plugin here: not
    // checked, said NOT ON THE SAFE SIDE); no weld row.
    let plans = crate::plate_plans(&r.text, &library(), &Default::default()).unwrap();
    let forces: BTreeMap<String, f64> = [("D1".to_owned(), 30e3), ("D2".to_owned(), -20e3)].into();
    let u = crate::plate_checks::plate_usage(&plans, &forces, 1.0);
    assert!(
        u.iter().any(|x| matches!(&x.part, crate::PlatePart::Bend)),
        "{u:?}"
    );
    assert!(
        !u.iter()
            .any(|x| matches!(&x.part, crate::PlatePart::Weld(_)))
    );
    // A run again: the same; the whole tower twice: the same.
    assert_eq!(run(&r.text, &settings(), Some(&["N2"]), false).text, r.text);
    let all = run(&text, &settings(), None, false);
    assert_eq!(run(&all.text, &settings(), None, false).text, all.text);
    // An edit (the flat plate 2 mm thicker): followed as the whole file's,
    // the thickness given so the user's, kept.
    let mut doc: toml_edit::DocumentMut = r.text.parse().unwrap();
    let fp = &o.plates[own.welded_to];
    let th = t
        .plates
        .iter()
        .find(|p| &p.id == fp)
        .unwrap()
        .thickness
        .si();
    ohl_structure_edit::connection_edits::set_plate(&mut doc, fp, Some(th + 0.002), None).unwrap();
    let edited = doc.to_string();
    let a = follow(&r.text, &edited, &library(), &Default::default(), &[]).expect("followed");
    let b = crate::layout_run::follow_whole(&r.text, &edited, &library(), &Default::default(), &[])
        .expect("followed");
    assert_eq!(a.text, b.text);
    let ta = parse(&a.text);
    let to = ta
        .plates
        .iter()
        .find_map(|p| p.bend.as_ref().map(|b| b.to.clone()))
        .expect("bent");
    let kept = ta.plates.iter().find(|p| p.id == to).unwrap();
    assert!(
        (kept.thickness.si() - (th + 0.002)).abs() < 1e-9,
        "{kept:?}"
    );
    // Off: the welded plate, as before L3.
    let s = LayoutSettings {
        small_bends: false,
        ..settings()
    };
    let w = run(&text, &s, Some(&["N2"]), false);
    let tw = parse(&w.text);
    assert!(tw.plates.iter().all(|p| p.bend.is_none()), "{}", w.text);
    assert!(tw.plates.iter().any(|p| !p.welds.is_empty()), "{}", w.text);
}

/// L2's follow-up (a), 2026-11-20: a plate with the layout's `short` mark
/// is drawn stopping short of the work point — round its members' legs from
/// the flange's toe over their bolts and the flange under them — convex,
/// every bolt keeping its end and edge distances.
#[test]
fn a_plate_marked_short_is_drawn_short_of_the_work_point() {
    // (D1 alone on its plate: D2 taken out.)
    let one = on_a_plate().replace(
        "[[member]]\nid = \"D2\"\nnodes = [\"N5\", \"N2\"]\nsection = \"EA-61x7\"\n\
         orientation = \"z-y-\"\ndesign = { class = \"bracing\" }\n",
        "",
    );
    assert!(!one.contains("\"D2\""));
    // (Without L5o's holding bolts: the plate drawn round its member only.)
    let s = LayoutSettings {
        hold_plates: false,
        ..settings()
    };
    let r = run(&one, &s, Some(&["N2"]), false);
    let t = parse(&r.text);
    let id = t.plates[0].id.clone();
    let mut doc: toml_edit::DocumentMut = r.text.parse().unwrap();
    // Its written outline out (drawn again), the mark on.
    for p in doc["plate"].as_array_of_tables_mut().unwrap().iter_mut() {
        if p.get("id").and_then(toml_edit::Item::as_str) == Some(id.as_str()) {
            p.remove("outline");
        }
    }
    let mut rec = LayoutRecord::of(&t);
    rec.made
        .get_mut(&id)
        .unwrap()
        .1
        .insert(crate::layout_run::SHORT.into());
    rec.write(&mut doc);
    let full = {
        let mut d = doc.clone();
        let mut rec = LayoutRecord::of(&parse(&d.to_string()));
        rec.made
            .get_mut(&id)
            .unwrap()
            .1
            .remove(crate::layout_run::SHORT);
        rec.write(&mut d);
        d.to_string()
    };
    let lib = library();
    let outline = |text: &str| {
        let d = joint_details(text, &lib, &Default::default()).unwrap();
        let j = d.joint("N2").unwrap().clone();
        let p = j.plates.iter().find(|p| p.id == id).unwrap().clone();
        (j, p)
    };
    let (j, short) = outline(&doc.to_string());
    let (_, round) = outline(&full);
    assert!(short.notes.is_empty(), "{:?}", short.notes);
    // The work point (on the plate's plane) outside the short outline,
    // inside the one round it; the short one smaller.
    let n = crate::welds::polygon_normal(&short.outline).unwrap();
    let inside = |poly: &[[f64; 3]], q: [f64; 3]| {
        (0..poly.len()).all(|i| {
            let a = poly[i];
            let b = poly[(i + 1) % poly.len()];
            let e = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let w = [q[0] - a[0], q[1] - a[1], q[2] - a[2]];
            let c = [
                e[1] * w[2] - e[2] * w[1],
                e[2] * w[0] - e[0] * w[2],
                e[0] * w[1] - e[1] * w[0],
            ];
            c[0] * n[0] + c[1] * n[1] + c[2] * n[2] >= -1e-9
        })
    };
    let on_plane = |poly: &[[f64; 3]]| {
        let a = poly[0];
        let w = [
            j.position[0] - a[0],
            j.position[1] - a[1],
            j.position[2] - a[2],
        ];
        let h = w[0] * n[0] + w[1] * n[1] + w[2] * n[2];
        [
            j.position[0] - h * n[0],
            j.position[1] - h * n[1],
            j.position[2] - h * n[2],
        ]
    };
    // (Convex: every corner on the inner side of every edge.)
    for q in &short.outline {
        assert!(inside(&short.outline, *q), "{:?}", short.outline);
    }
    assert!(!inside(&short.outline, on_plane(&short.outline)));
    assert!(inside(&round.outline, on_plane(&round.outline)));
    let area = |poly: &[[f64; 3]]| {
        let mut s = [0.0; 3];
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            s[0] += a[1] * b[2] - a[2] * b[1];
            s[1] += a[2] * b[0] - a[0] * b[2];
            s[2] += a[0] * b[1] - a[1] * b[0];
        }
        (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt() / 2.0
    };
    assert!(area(&short.outline) < area(&round.outline));
}

fn d1_spoiled(deg: f64) -> String {
    format!(
        "{}\n[[node]]\nid = \"N6\"\npoint = [\"5 m\", \"0 m\", \"0 m\"]\nsupport = \"fixed\"\n\
         \n[[member]]\nid = \"H0\"\nnodes = [\"N1\", \"N4\"]\nsection = \"EA-61x7\"\n\
         design = {{ class = \"bracing\" }}\n\
         \n[[member]]\nid = \"H1\"\nnodes = [\"N4\", \"N6\"]\nsection = \"EA-61x7\"\n\
         design = {{ class = \"bracing\" }}\n",
        k_brace().replace(
            "orientation = \"x+y+\"\n",
            &format!("orientation = \"x+y+\"\nroll = \"{deg} deg\"\n")
        )
    )
}

/// Step 4 never forced: 30° off, the welded plate would put D1's axis
/// more than the plate's thickness farther out of the flange than a flat
/// plate would — its line of action off the work point, not in the
/// analysis: a hand detail, said.
#[test]
fn a_welded_plate_that_would_move_the_member_far_out_is_a_hand_detail() {
    let r = run(&d1_spoiled(30.0), &settings(), Some(&["N2"]), false);
    let o = r.joints.iter().find(|j| j.joint == "N2").unwrap();
    assert!(o.plan.ends.iter().all(|e| e.member != "D1"), "{:?}", o.plan);
    assert!(
        o.hand
            .iter()
            .any(|h| h.starts_with("D1: its bolted leg lies") && h.contains("farther out")),
        "{:?}",
        o.hand
    );
}

/// The owner's step 4 (2026-11-19): an end no roll lays flat goes onto a
/// plate in its own plane, welded (both sides, its throat and length
/// written: the weld checked) to a flat plate on the flange; its member
/// lies flat on it, nothing clashes, a run again is the same, and the
/// follow of an edit is as the whole file's.
#[test]
fn an_end_no_roll_lays_flat_goes_onto_a_welded_plate_in_its_own_plane() {
    let text = d1_spoiled(10.0);
    // (L3, 2026-11-20: a plate bent by its few degrees first; here the
    // welded one alone. An edit's follow lays out with the default switches:
    // the follow is tested with the bent plate, below.)
    let settings = || LayoutSettings {
        small_bends: false,
        ..settings()
    };
    let r = run(&text, &settings(), Some(&["N2"]), false);
    let o = r.joints.iter().find(|j| j.joint == "N2").unwrap();
    assert!(o.written, "{:?} {:?}", o.hand, r.notes);
    let e = o
        .plan
        .ends
        .iter()
        .find(|e| e.member == "D1")
        .unwrap_or_else(|| panic!("D1 not laid out: {:?} {:?}", o.plan, r.notes));
    let crate::Onto::Plate(k) = e.onto else {
        panic!("{e:?}")
    };
    let own = o.plan.plates[k].own.as_ref().expect("in its own plane");
    assert!(own.off > crate::FLAT_OFF, "{own:?}");
    assert_eq!(own.weld_size, 0.003);
    let flat = &o.plan.plates[own.welded_to];
    assert!(flat.own.is_none() && flat.reach.is_some(), "{flat:?}");
    // Written: the record's mark, the weld with its size and length.
    let t = parse(&r.text);
    let id = &o.plates[k];
    let rec = LayoutRecord::of(&t);
    assert!(rec.made[id].1.contains(crate::layout_run::OWN_PLANE));
    let p = t.plates.iter().find(|p| &p.id == id).unwrap();
    assert_eq!(p.welds.len(), 1, "{p:?}");
    assert_eq!(p.welds[0].to, o.plates[own.welded_to]);
    assert!(p.welds[0].size.is_some() && p.welds[0].length.is_some());
    // D1 lies flat on it, the plates meet along the seam, no clash.
    let d = joint_details(&r.text, &library(), &Default::default()).unwrap();
    let j = d.joint("N2").unwrap();
    let d1 = j.arrivals.iter().find(|a| a.member == "D1").unwrap();
    assert!(d1.flat.is_some_and(|f| f.off < 1.0), "{:?}", d1.flat);
    assert!(!d1.misfit, "{:?}", d1.notes);
    let seams: usize = j
        .plates
        .iter()
        .map(|x| {
            x.welds
                .iter()
                .filter(|s| &x.id == id || &s.with == id)
                .count()
        })
        .sum();
    assert_eq!(seams, 1, "{:?}", j.plates);
    assert!(j.clashes.is_empty(), "{:?}", j.clashes);
    // The weld is checked: its plan has its size.
    let plans = crate::plate_plans(&r.text, &library(), &Default::default()).unwrap();
    let forces: BTreeMap<String, f64> = [("D1".to_owned(), 30e3), ("D2".to_owned(), -20e3)].into();
    let u = crate::plate_checks::plate_usage(&plans, &forces, 1.0);
    assert!(
        u.iter()
            .any(|x| matches!(&x.part, crate::PlatePart::Weld(_))),
        "{u:?}"
    );
    // A run again: the same; the whole tower twice: the same.
    assert_eq!(run(&r.text, &settings(), Some(&["N2"]), false).text, r.text);
    let all = run(&text, &settings(), None, false);
    assert_eq!(run(&all.text, &settings(), None, false).text, all.text);
    // An edit (the flat plate 2 mm thicker), laid out again.
    let mut doc: toml_edit::DocumentMut = r.text.parse().unwrap();
    let fp = &o.plates[own.welded_to];
    let th = t
        .plates
        .iter()
        .find(|p| &p.id == fp)
        .unwrap()
        .thickness
        .si();
    ohl_structure_edit::connection_edits::set_plate(&mut doc, fp, Some(th + 0.002), None).unwrap();
    yours(&mut doc, fp, "thickness");
    let edited = doc.to_string();
    let a = run(&edited, &settings(), Some(&["N2"]), false);
    // The flat plate's thickness given so: the user's, kept (the plate has
    // nothing bolted onto it: known by its welded plate's member).
    let ta = parse(&a.text);
    let to = ta
        .plates
        .iter()
        .find_map(|p| p.welds.first().map(|w| w.to.clone()))
        .expect("welded");
    let kept = ta.plates.iter().find(|p| p.id == to).unwrap();
    assert!(
        (kept.thickness.si() - (th + 0.002)).abs() < 1e-9,
        "{kept:?}"
    );
    // The weld's throat given by hand: the user's, kept by a run again.
    let mut doc: toml_edit::DocumentMut = r.text.parse().unwrap();
    let edit = ohl_structure_edit::connection_edits::WeldEdit {
        size: Some(Some(0.005)),
        ..Default::default()
    };
    ohl_structure_edit::connection_edits::set_weld(&mut doc, &t, id, fp, edit).unwrap();
    yours(&mut doc, id, "weld");
    let edited = doc.to_string();
    let again = run(&edited, &settings(), Some(&["N2"]), false);
    let t2 = parse(&again.text);
    let w2 = t2
        .plates
        .iter()
        .find(|p| !p.welds.is_empty())
        .expect("the welded plate");
    assert_eq!(w2.welds[0].size.map(|v| v.si()), Some(0.005), "{w2:?}");
}

/// J1/J2 (2026-11-19) in a run: a member left for a hand detail for its
/// own sake (R1, too short for its bolts) keeps its place — the rest of
/// the joint is laid out and written, and nothing written runs into R1;
/// a run again changes nothing.
#[test]
fn a_run_keeps_the_place_of_a_member_left_by_hand() {
    let text = crate::joint_layout_tests::yielding(20.0, 0.15);
    let r = run(&text, &settings(), Some(&["N2"]), false);
    let o = r.joints.iter().find(|j| j.joint == "N2").unwrap();
    assert!(o.written, "{:?} {:?}", o.hand, r.notes);
    assert_eq!(o.plan.hand_members, ["R1"], "{:?}", o.plan);
    assert_eq!(o.plan.kept_free, ["R1"]);
    assert!(o.plan.ends.iter().any(|e| e.member == "D1"));
    let d = joint_details(&r.text, &library(), &Default::default()).unwrap();
    for c in &d.joint("N2").unwrap().clashes {
        let m = [c.a.member(), c.b.member()];
        assert!(!m.contains(&Some("D1")), "{}", c.words());
    }
    assert_eq!(run(&r.text, &settings(), Some(&["N2"]), false).text, r.text);
}

/// L8 (L7's proposal 3): a member the run names what it bolts onto in its
/// planning copy but does not lay out is written without the name — drawn
/// elsewhere. Its joint is planned once more with it where the file draws
/// it, kept where that lays out more ends (or as many on no more plates):
/// on the invented tower, more ends than without, each such joint said in
/// the notes, the name of such a member not written, nothing taken back
/// for a plate's bolts off its outline; a run again changes nothing.
#[test]
fn a_member_named_but_not_laid_out_is_planned_where_it_is_drawn() {
    let text = tall();
    let r = run(&text, &settings(), None, false);
    let off = LayoutSettings {
        as_drawn: false,
        ..settings()
    };
    let r0 = run(&text, &off, None, false);
    let ends = |r: &crate::LayoutRun| r.laid_out().map(|o| o.plan.ends.len()).sum::<usize>();
    assert!(ends(&r) > ends(&r0), "{} {}", ends(&r), ends(&r0));
    let said: Vec<&String> = r
        .notes
        .iter()
        .filter(|n| n.contains("where the file draws it"))
        .collect();
    assert!(!said.is_empty(), "{:?}", r.notes);
    let t = parse(&r.text);
    for n in &said {
        let (j, rest) = n.split_once(": planned with ").unwrap();
        let ms = rest.split(" where").next().unwrap();
        let o = r.joints.iter().find(|o| o.joint == j).unwrap();
        for m in ms.split(", ") {
            assert!(!o.plan.ends.iter().any(|e| e.member == m), "{n}");
            assert!(
                t.connection(m, j).is_none_or(|c| c.onto.is_none()),
                "{n}: {m}'s name written"
            );
        }
    }
    assert!(
        r.joints
            .iter()
            .flat_map(|o| &o.hand)
            .all(|h| !h.contains("would lie outside its outline")),
        "{:?}",
        r.joints.iter().flat_map(|o| &o.hand).collect::<Vec<_>>()
    );
    assert_eq!(run(&r.text, &settings(), None, false).text, r.text);
}

/// J1 in a run: the short redundant the owner's order filled one after
/// the other would leave out is laid out (D1 pulled back past it), and the
/// joint is laid out whole, running into nothing.
#[test]
fn a_run_lays_out_whole_what_one_after_the_other_leaves() {
    let text = crate::joint_layout_tests::yielding(20.0, 0.4);
    let r = run(&text, &settings(), Some(&["N2"]), false);
    let o = r.joints.iter().find(|j| j.joint == "N2").unwrap();
    assert!(o.written && o.hand.is_empty(), "{:?} {:?}", o.hand, r.notes);
    let d = joint_details(&r.text, &library(), &Default::default()).unwrap();
    for c in &d.joint("N2").unwrap().clashes {
        let m = [c.a.member(), c.b.member()];
        assert!(
            !m.contains(&Some("D1")) && !m.contains(&Some("R1")),
            "{}",
            c.words()
        );
    }
}

/// A plan strut H: at the column's joint N2 on the column's upright
/// flange, at N7 on a strut S1–S2 running through, on its flange facing
/// up — S sloping `slope` m over 1.5 m (its flange turned about H by as
/// much). An equal-leg angle (`section`), rolled 12° by hand (off at both
/// ends): its leg bolted upright at N2, the other leg at N7 (the engine
/// takes the leg lying flattest).
fn plan_strut(section: &str, slope: f64) -> String {
    format!(
        r#"id = "P"
name = "plan strut"
kind = "lattice"

[[node]]
id = "N1"
point = ["0 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "N2"
point = ["0 m", "0 m", "3 m"]

[[node]]
id = "N3"
point = ["0 m", "0 m", "6 m"]
support = "fixed"

[[node]]
id = "NA"
point = ["-1.5 m", "2.5 m", "{za} m"]
support = "fixed"

[[node]]
id = "N7"
point = ["0 m", "2.5 m", "3 m"]

[[node]]
id = "NB"
point = ["1.5 m", "2.5 m", "{zb} m"]
support = "fixed"

[[member]]
id = "L1"
nodes = ["N1", "N2"]
section = "EA-61x7"
orientation = "x+y+"
design = {{ class = "leg" }}

[[member]]
id = "L2"
nodes = ["N2", "N3"]
section = "EA-61x7"
orientation = "x+y+"
design = {{ class = "leg" }}

[[member]]
id = "S1"
nodes = ["NA", "N7"]
section = "EA-61x7"
orientation = "z-y+"
design = {{ class = "bracing" }}

[[member]]
id = "S2"
nodes = ["N7", "NB"]
section = "EA-61x7"
orientation = "z-y+"
design = {{ class = "bracing" }}

[[member]]
id = "H"
nodes = ["N2", "N7"]
section = "{section}"
design = {{ class = "bracing" }}
roll = "12 deg"
"#,
        za = 3.0 + slope,
        zb = 3.0 - slope
    )
}

/// H's turn and how far off it lies at joint `j` of `text` (at N7 onto
/// S1, as the run names it where nothing is named).
fn h_flat(text: &str, j: &str) -> crate::welds::Flat {
    let named = if parse(text).connection("H", "N7").is_some() {
        text.to_owned()
    } else {
        format!("{text}\n[[connection]]\nmember = \"H\"\njoint = \"N7\"\nonto = \"S1\"\n")
    };
    let d = joint_details(&named, &library(), &Default::default()).unwrap();
    d.joint(j)
        .unwrap()
        .arrivals
        .iter()
        .find(|a| a.member == "H")
        .and_then(|a| a.flat)
        .unwrap()
}

/// J3 (2026-11-19): one roll for the whole member. The strut's flange at
/// N7 turned 1.6° about H: H flat at N2 lies 1.6° off at N7; the middle
/// turn leaves each end 0.8° off (within the owner's 2° flatness of
/// 2026-11-19; since L9 the 0.8° at N2 is taken by its plate, whose
/// outline is written in H's plane): H is turned once, both its joints laid out
/// with it (its leg bolted upright at N2, the other at N7 — an equal-leg
/// angle's leg bolted changing between its ends); a run again changes
/// nothing, and the relaxed planner (the estimated eccentricities, O45)
/// plans with the same turn.
#[test]
fn one_roll_for_the_whole_member_lets_both_its_ends_lie_flat() {
    let text = plan_strut("EA-61x7", 1.5 * 1.6f64.to_radians().tan());
    let (n2, n7) = (h_flat(&text, "N2"), h_flat(&text, "N7"));
    assert!(n2.off > 5.0 && n7.off > 5.0, "{n2:?} {n7:?}");
    let r = run(&text, &settings(), Some(&["N2", "N7"]), true);
    assert!(roll_of(&r.text, "H").is_some(), "{:?}", r.notes);
    assert!(
        !r.notes.iter().any(|n| n.starts_with("H: lies off")),
        "{:?}",
        r.notes
    );
    for j in ["N2", "N7"] {
        let f = h_flat(&r.text, j);
        // (L10, 2026-11-21: a plate on a flange whose members lie within
        // 2° of it is drawn in the flange's plane. At N2 the layout writes
        // G1's outline so: H lies its 0.8° off it — within the outline's
        // corners written to 0.1 mm over its 61 mm. Before L10 G1 was
        // drawn in H's plane, taking the 0.8° to L2's flange. At N7 G2 is
        // written without an outline: its plane is the one its members'
        // axes span (S1's and H's), which H's leg lies 0.8° off.)
        assert!((f.off - 0.8).abs() < 0.1, "{j}: {f:?}");
        let o = r.joints.iter().find(|o| o.joint == j).unwrap();
        assert!(
            o.written && o.plan.ends.iter().any(|e| e.member == "H"),
            "{j}: {:?} {:?}",
            o.plan,
            r.notes
        );
        assert!(o.turned.iter().any(|m| m == "H"), "{j}: {:?}", o.turned);
    }
    // Its leg bolted changes between its ends (an equal-leg angle).
    let after = joint_details(&r.text, &library(), &Default::default()).unwrap();
    // At N2 its plate lies in the plane of the flange of the leg it lies
    // on (within the outline's 0.1 mm over 61 mm: 0.1°; before L10 it took
    // H's 0.8°).
    let n2 = after.joint("N2").unwrap();
    let g1 = n2.plates.iter().find(|p| p.members == ["H"]).unwrap();
    assert!(g1.outline_given, "{g1:?}");
    let leg = n2
        .arrivals
        .iter()
        .find(|a| Some(&a.member) == g1.leg.as_ref())
        .and_then(|a| a.profile.as_ref())
        .unwrap();
    let n = g1.through.unwrap();
    let to_flange = [leg.axes[1], leg.axes[2]]
        .iter()
        .map(|m| {
            let c: f64 = (0..3).map(|k| n[k] * m[k]).sum();
            c.abs().min(1.0).acos().to_degrees()
        })
        .fold(f64::INFINITY, f64::min);
    assert!(to_flange < 0.1, "G1 to its flange {to_flange}°");
    let leg = |j: &str| {
        after
            .joint(j)
            .unwrap()
            .arrivals
            .iter()
            .find(|a| a.member == "H")
            .unwrap()
            .leg_y
    };
    assert_ne!(leg("N2"), leg("N7"));
    assert_eq!(
        run(&r.text, &settings(), Some(&["N2", "N7"]), true).text,
        r.text
    );
    let lib = library();
    let bolts = Default::default();
    let s = settings();
    let relaxed = crate::layout_run::plan_relaxed(
        &text,
        &RunAsk {
            library: &lib,
            bolts: &bolts,
            settings: &s,
            preset_fields: BTreeMap::new(),
            joints: Some(["N2", "N7"].iter().map(|x| (*x).to_owned()).collect()),
            replace_hand: true,
        },
    )
    .unwrap();
    for j in ["N2", "N7"] {
        let o = relaxed.joints.iter().find(|o| o.joint == j).unwrap();
        assert!(
            o.plan.ends.iter().any(|e| e.member == "H") && o.turned.iter().any(|m| m == "H"),
            "{j}: {:?}",
            o.plan
        );
    }
}

/// J3, never forced: the strut's flange turned 8° about H — no roll lays
/// both ends flat (their turns more than the 2° flatness apart; the middle
/// would leave each 4° off: a turn leaving each end within the whole
/// flatness was tried at the 5° of then, it laid out fewer ends on real
/// towers). H is not
/// turned, said with both ends' turns, and its end at N7 is not laid out
/// flat on the flange (a welded plate in its own plane or a hand detail,
/// with the reason).
#[test]
fn no_roll_for_both_ends_leaves_the_member_as_it_was() {
    let text = plan_strut("EA-61x7", 1.5 * 8f64.to_radians().tan());
    // (Bottom up, then out — L1 a, on by default — turns it to lie flat at
    // its end on the body: below.)
    let s = LayoutSettings {
        bottom_up: false,
        ..settings()
    };
    let r = run(&text, &s, Some(&["N2", "N7"]), true);
    assert!(
        roll_of(&r.text, "H").and_then(|v| v.as_str().map(str::to_owned)) == Some("12 deg".into()),
        "{:?}",
        r.notes
    );
    assert!(
        r.notes
            .iter()
            .any(|n| n.starts_with("H: lies off") && n.contains("by different turns")),
        "{:?}",
        r.notes
    );
    let o = r.joints.iter().find(|o| o.joint == "N7").unwrap();
    let on_flange = o
        .plan
        .ends
        .iter()
        .any(|e| e.member == "H" && e.onto == crate::joint_layout::Onto::Member("S1".into()));
    assert!(!on_flange, "{:?}", o.plan);
}

/// L1 (d), what an end bolts onto chosen in the search (owner
/// 2026-11-19): the K-brace on a wide built-up main angle running through
/// (a star of two), D1 with six 16 mm bolts — they do not land on the
/// angle's flange and no plate is laid out on a built-up member, so today
/// D1 is left for a hand detail (D2's two bolts land). Searched (on by
/// default), D1 is laid onto D2, the other member it could bolt onto, and
/// the joint is laid out whole. A run again changes nothing. (L2,
/// 2026-11-20: D1 goes onto a plate on D2's flange, which reaches the work
/// point; with the plate seated on D2 (its `on`) the clash check sees it run
/// into the leg below the joint — hidden before, when the plate counted as
/// lying on the leg — and the clash-aware plan sees it before writing: D1
/// is left, said. The search as L1 measured it with L2's two off.)
#[test]
fn an_end_left_bolts_onto_another_member_where_that_lays_it_out() {
    // (J6: a built-up angle's root radius now kept clear by the nuts — a
    // small one, so that D2's bolts still land on it as this test needs.)
    let (lib, wide) = crate::joint_layout_tests::wide_main_with("2 mm");
    let text = wide
        .replace("section = \"EA-160x15\"", "section = \"2S-160x15\"")
        .replacen(
            "bolts = { diameter = \"12 mm\", count = 2 }",
            "bolts = { diameter = \"16 mm\", count = 6 }",
            1,
        );
    assert_eq!(text.matches("2S-160x15").count(), 2);
    let bolts = Default::default();
    let go = |text: &str, s: &LayoutSettings| {
        lay_out(
            text,
            &RunAsk {
                library: &lib,
                bolts: &bolts,
                settings: s,
                preset_fields: BTreeMap::new(),
                joints: Some(["N2".to_owned()].into()),
                replace_hand: true,
            },
        )
        .unwrap()
    };
    // (J6 off: the built-up main member takes no plate, as the test asks.)
    let today = LayoutSettings {
        onto_search: false,
        built_up_plates: false,
        ..settings()
    };
    let base = go(&text, &today);
    let n2 = base.joints.iter().find(|o| o.joint == "N2").unwrap();
    assert!(
        n2.plan.hand_members.iter().any(|m| m == "D1")
            && n2.plan.ends.iter().any(|e| e.member == "D2"),
        "{:?}",
        n2.plan
    );
    let no_j6 = LayoutSettings {
        built_up_plates: false,
        ..settings()
    };
    let l2 = go(&text, &no_j6);
    let n2 = l2.joints.iter().find(|o| o.joint == "N2").unwrap();
    assert!(
        n2.plan.hand_members.iter().any(|m| m == "D1")
            && n2.plan.ends.iter().any(|e| e.member == "D2"),
        "{:?}",
        n2.plan
    );
    let s = LayoutSettings {
        plate_on: false,
        exact_plan: false,
        built_up_plates: false,
        ..settings()
    };
    assert!(s.onto_search);
    let r = go(&text, &s);
    let n2 = r.joints.iter().find(|o| o.joint == "N2").unwrap();
    assert!(
        n2.written && n2.hand.is_empty(),
        "{:?} {:?}",
        n2.hand,
        n2.plan
    );
    let e = n2.plan.ends.iter().find(|e| e.member == "D1").unwrap();
    let on_d2 = match &e.onto {
        crate::joint_layout::Onto::Member(m) => m == "D2",
        crate::joint_layout::Onto::Plate(k) => n2.plan.plates[*k].on == "D2",
    };
    assert!(on_d2, "{:?}", n2.plan);
    assert_eq!(go(&r.text, &s).text, r.text);
}

/// L1 (a), bottom up, then out (owner 2026-11-19): the strut's flange
/// turned 8° about H, H's roll 12° — no one roll lays both its ends flat
/// and neither lies flat now. N2 is on the body (the leg's line), N7 out
/// of it (an arm): H is turned to lie flat at N2, its end first in the
/// order, and laid out there; its end at N7 takes what comes (a welded
/// plate or a hand detail). Off: not turned, as before (above). A run
/// again changes nothing.
#[test]
fn bottom_up_turns_a_member_to_lie_flat_at_its_end_on_the_body() {
    let text = plan_strut("EA-61x7", 1.5 * 8f64.to_radians().tan());
    let o = crate::joint_order(&parse(&text));
    assert_eq!(o.zone_of("N2"), crate::Zone::Body);
    assert_eq!(o.zone_of("N7"), crate::Zone::Arm);
    assert!(o.rank_of("N2") < o.rank_of("N7"));
    let s = LayoutSettings {
        bottom_up: true,
        ..settings()
    };
    let r = run(&text, &s, Some(&["N2", "N7"]), true);
    assert!(
        r.notes
            .iter()
            .any(|n| n.starts_with("H: no one roll") && n.contains("turned to lie flat at N2")),
        "{:?}",
        r.notes
    );
    assert_ne!(
        roll_of(&r.text, "H").and_then(|v| v.as_str().map(str::to_owned)),
        Some("12 deg".into())
    );
    assert!(
        h_flat(&r.text, "N2").off < 0.1,
        "{:?}",
        h_flat(&r.text, "N2")
    );
    let n2 = r.joints.iter().find(|o| o.joint == "N2").unwrap();
    assert!(
        n2.written && n2.plan.ends.iter().any(|e| e.member == "H"),
        "{:?}",
        n2.plan
    );
    assert_eq!(run(&r.text, &s, Some(&["N2", "N7"]), true).text, r.text);
}

/// The after-writing check of holes in one member (2026-11-20, found
/// measuring L1): the joint checks' own rule — two holes nearer than their
/// radii together overlap, one hole (a bolt through both) does not.
#[test]
fn holes_overlap_as_the_joint_checks_say() {
    use crate::layout_run::holes_overlap;
    let d = 0.0175;
    assert!(holes_overlap([0.0; 3], d, [0.013, 0.0, 0.0], d));
    assert!(!holes_overlap([0.0; 3], d, [0.001, 0.0, 0.0], d));
    assert!(!holes_overlap([0.0; 3], d, [0.0, 0.03, 0.0], d));
}

/// L3b (2026-11-20): how an end off its flange would lie with its member
/// turned more — a half turn keeps how far off it is and turns its
/// outstanding leg the other way (the plate bent by as much takes it one
/// way only); an equal-leg angle's quarter turns lay its other leg there,
/// one of them taking the bent plate, the other not.
#[test]
fn a_half_or_quarter_turn_decides_whether_a_bent_plate_takes_an_end() {
    use std::f64::consts::{FRAC_PI_2, PI};
    let text = format!(
        "{}\n[[connection]]\nmember = \"D1\"\njoint = \"N2\"\nonto = \"L1\"\n",
        d1_spoiled(10.0)
    );
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    let j = d.joint("N2").unwrap();
    let a = j.arrivals.iter().find(|a| a.member == "D1").unwrap();
    let off = a.flat.unwrap().off;
    assert!(
        off > crate::FLAT_OFF && off < crate::BEND_SMALL_MAX,
        "{off}"
    );
    let at = |t: f64, either: bool| crate::joint_layout::turned_end(j, a, t, either).unwrap();
    let (o0, k0) = at(0.0, false);
    let (o1, k1) = at(PI, false);
    assert!(
        (o0 - off).abs() < 0.01 && (o1 - off).abs() < 0.01,
        "{o0} {o1} {off}"
    );
    assert_ne!(k0, k1);
    // Equal legs, the other leg bolted: as far off, one way round each.
    let (q0, j0) = at(FRAC_PI_2, true);
    let (q1, j1) = at(-FRAC_PI_2, true);
    assert!(
        (q0 - off).abs() < 0.01 && (q1 - off).abs() < 0.01,
        "{q0} {q1}"
    );
    assert_ne!(j0, j1);
}

/// L5o (2026-11-20, the owner's decision of that day): a gusset plate on a
/// leg running on through the joint (L1 into L2, said one part), which no
/// leg's end bolts onto, is given at least two provisional bolts of its own
/// onto the leg — within its flange, inside the plate, the default minimum
/// spacing apart —, written as `[plate.leg_bolts]` and recorded
/// provisional; the joint checks then check how it is held (before: "not
/// checked"). Off: as before. A run again changes nothing.
#[test]
fn a_plate_on_a_leg_running_on_gets_bolts_of_its_own_holding_it() {
    let text = format!(
        "{}\n[[connection]]\nmember = \"L1\"\njoint = \"N2\"\nkind = \"continuous\"\nonto = \"L2\"\n",
        k_brace().replace(
            "design = { class = \"bracing\" }",
            "design = { class = \"bracing\", bolts = { diameter = \"16 mm\", count = 4 } }",
        )
    );
    let holding = |t: &str| {
        let plans = crate::plate_plans(t, &library(), &Default::default()).unwrap();
        let rows = crate::plate_usage(&plans, &BTreeMap::new(), 1.0);
        let nc = rows.iter().any(
            |x| matches!(&x.part, crate::PlatePart::NotChecked(w) if w.contains("is held to the")),
        );
        let held = rows
            .iter()
            .any(|x| matches!(&x.part, crate::PlatePart::Holding(_)));
        (nc, held)
    };
    let r = run(&text, &settings(), Some(&["N2"]), false);
    let n2 = r.joints.iter().find(|j| j.joint == "N2").unwrap();
    assert!(n2.written && n2.hand.is_empty(), "{:?}", n2.hand);
    assert_eq!(n2.plan.plates.len(), 1, "{:?}", n2.plan);
    let hold = n2.plan.plates[0].hold.as_ref().expect("its own bolts");
    assert!(hold.member == "L1" || hold.member == "L2", "{hold:?}");
    assert!(hold.provisional && hold.points.len() >= 2, "{hold:?}");
    // Within the 61 mm flange: clear of the heel's leg and root by the
    // nut, short of the toe.
    for p in &hold.points {
        assert!(p[1] > 0.007 && p[1] < 0.061, "{hold:?}");
    }
    // Apart by the default minimum spacing (three diameters).
    let d = (hold.points[1][0] - hold.points[0][0]).hypot(hold.points[1][1] - hold.points[0][1]);
    assert!(d >= 0.048 - 1e-9, "{hold:?}");
    assert!(
        n2.plan
            .notes
            .iter()
            .any(|n| n.contains("holding a plate on it")),
        "{:?}",
        n2.plan.notes
    );
    let t = parse(&r.text);
    let id = &n2.plates[0];
    let lb = t
        .plates
        .iter()
        .find(|q| &q.id == id)
        .and_then(|q| q.leg_bolts.as_ref())
        .expect("written");
    assert_eq!(lb.member, hold.member);
    assert!(
        LayoutRecord::of(&t)
            .provisional
            .contains(&(id.clone(), "N2".to_owned()))
    );
    // Nothing laid out runs into anything; the bolts within the plate.
    let dd = joint_details(&r.text, &library(), &Default::default()).unwrap();
    let j = dd.joint("N2").unwrap();
    assert!(j.clashes.is_empty(), "{:?}", j.clashes);
    let pd = j.plates.iter().find(|q| &q.id == id).unwrap();
    assert!(pd.leg_bolts.is_some(), "{:?}", pd.notes);
    assert_eq!(holding(&r.text), (false, true));
    // Off: no bolts of its own, its holding not checked (as before).
    let off = run(
        &text,
        &LayoutSettings {
            hold_plates: false,
            ..settings()
        },
        Some(&["N2"]),
        false,
    );
    let o2 = off.joints.iter().find(|j| j.joint == "N2").unwrap();
    assert!(o2.plan.plates[0].hold.is_none());
    assert_eq!(holding(&off.text), (true, false));
    // A run again: the same file.
    let again = run(&r.text, &settings(), Some(&["N2"]), false);
    assert_eq!(again.text, r.text);
}

/// J6 (2026-11-20): the K-brace on a leg of four angles in a cross (its
/// angles known), the diagonals' bolts too many for the facing angle's
/// flange: onto a plate on that angle (the member it lies on written,
/// `on`), drawn and seated on it — not through the work point — and
/// nothing laid out running into the leg's other angles. Off: those that
/// do not land are left, said, as before. A run again changes nothing.
#[test]
fn a_plate_on_a_built_up_leg_lies_on_the_angle_facing_the_ends() {
    let text = k_brace()
        .replace(
            "section = \"EA-61x7\"\norientation = \"x+y+\"",
            "section = \"4X-61x7\"",
        )
        .replace(
            "design = { class = \"bracing\" }",
            "design = { class = \"bracing\", bolts = { diameter = \"12 mm\", count = 3 } }",
        );
    let off = run(
        &text,
        &LayoutSettings {
            built_up_plates: false,
            ..settings()
        },
        Some(&["N2"]),
        false,
    );
    let o2 = off.joints.iter().find(|j| j.joint == "N2").unwrap();
    assert!(
        o2.hand.iter().any(|h| h.contains("is built up")),
        "{:?}",
        o2.hand
    );
    let r = run(&text, &settings(), Some(&["N2"]), false);
    let n2 = r.joints.iter().find(|j| j.joint == "N2").unwrap();
    assert!(n2.written && n2.hand.is_empty(), "{:?}", n2.hand);
    assert!(!n2.plan.plates.is_empty(), "{:?}", n2.plan);
    let on = &n2.plan.plates[0].on;
    assert!(on == "L1" || on == "L2", "{on}");
    let t = parse(&r.text);
    let id = &n2.plates[0];
    assert_eq!(
        t.plates
            .iter()
            .find(|q| &q.id == id)
            .and_then(|q| q.on.as_deref()),
        Some(on.as_str())
    );
    // Drawn on the facing angle: its outline off the work point's planes
    // (the leg's axis), no clash of anything laid out, no bolt off the
    // plate.
    let d = joint_details(&r.text, &library(), &Default::default()).unwrap();
    let j = d.joint("N2").unwrap();
    assert!(j.clashes.is_empty(), "{:?}", j.clashes);
    let pd = j.plates.iter().find(|q| &q.id == id).unwrap();
    let n = pd.through.expect("its plane");
    let off_wp = pd
        .outline
        .iter()
        .map(|q| {
            ((q[0] - j.position[0]) * n[0]
                + (q[1] - j.position[1]) * n[1]
                + (q[2] - j.position[2]) * n[2])
                .abs()
        })
        .fold(0.0, f64::max);
    assert!(off_wp > 0.005, "{off_wp} {:?}", pd.outline);
    for m in ["D1", "D2"] {
        let a = j.arrivals.iter().find(|a| a.member == m).unwrap();
        assert!(!a.misfit, "{:?}", a.notes);
    }
    // A run again: the same file.
    let again = run(&r.text, &settings(), Some(&["N2"]), false);
    assert_eq!(again.text, r.text);
}

/// L9 (2026-11-21, L8's question 5): D1 no roll lays flat on L1 goes onto
/// a plate in its own plane welded to a flat plate on L1's flange; a
/// horizontal running through the joint as one part (HA into HB) lies
/// across that flange where the flat plate would reach round the work
/// point. The plan now sees the flat plate as it will be written (before:
/// not checked, found after writing and D1 taken back): it stops short of
/// the work point clear of the horizontal, or D1 is left for a hand detail
/// in the plan, said — nothing written runs into the horizontal. Off: as
/// before.
#[test]
fn a_welded_plates_flat_plate_is_planned_as_written_clear_of_a_run_on_its_face() {
    let text = format!(
        "{}\n[[node]]\nid = \"N7\"\npoint = [\"-2 m\", \"0 m\", \"3 m\"]\nsupport = \"fixed\"\n\
         \n[[node]]\nid = \"N8\"\npoint = [\"2 m\", \"0 m\", \"3 m\"]\nsupport = \"fixed\"\n\
         \n[[member]]\nid = \"HA\"\nnodes = [\"N7\", \"N2\"]\nsection = \"EA-61x7\"\n\
         design = {{ class = \"bracing\" }}\n\
         \n[[member]]\nid = \"HB\"\nnodes = [\"N2\", \"N8\"]\nsection = \"EA-61x7\"\n\
         design = {{ class = \"bracing\" }}\n\
         \n[[connection]]\nmember = \"HA\"\njoint = \"N2\"\nkind = \"continuous\"\nonto = \"HB\"\n\
         \n[[connection]]\nmember = \"HB\"\njoint = \"N2\"\nkind = \"continuous\"\nonto = \"HA\"\n\
         \n[[connection]]\nmember = \"L1\"\njoint = \"N2\"\nkind = \"continuous\"\nonto = \"L2\"\n\
         \n[[connection]]\nmember = \"L2\"\njoint = \"N2\"\nkind = \"continuous\"\nonto = \"L1\"\n\
         \n[[connection]]\nmember = \"D1\"\njoint = \"N2\"\nonto = \"L1\"\n\
         \n[[connection]]\nmember = \"D2\"\njoint = \"N2\"\nonto = \"L2\"\n",
        d1_spoiled(10.0)
    );
    let welded = |reach_clear: bool| LayoutSettings {
        small_bends: false,
        reach_clear,
        ..settings()
    };
    let on = run(&text, &welded(true), Some(&["N2"]), false);
    let off = run(&text, &welded(false), Some(&["N2"]), false);
    let o_on = on.joints.iter().find(|j| j.joint == "N2").unwrap();
    let o_off = off.joints.iter().find(|j| j.joint == "N2").unwrap();
    // Nothing written runs into the horizontal, either way.
    let into_run = |t: &str| -> Vec<String> {
        let d = joint_details(t, &library(), &Default::default()).unwrap();
        d.joint("N2")
            .unwrap()
            .clashes
            .iter()
            .filter(|c| {
                let run = |x: &crate::ClashPart| x.member().is_some_and(|m| m.starts_with('H'));
                let plate = |x: &crate::ClashPart| matches!(x, crate::ClashPart::Plate(_));
                (run(&c.a) && plate(&c.b)) || (run(&c.b) && plate(&c.a))
            })
            .map(crate::Clash::words)
            .collect()
    };
    assert!(into_run(&on.text).is_empty(), "{:?}", into_run(&on.text));
    assert!(into_run(&off.text).is_empty(), "{:?}", into_run(&off.text));
    // Planned so: no end taken back after writing for it (laid out on a
    // flat plate stopping short of the work point, or left in the plan).
    assert!(
        !o_on
            .hand
            .iter()
            .any(|h| h.contains("would run") && h.contains('H')),
        "{:?}",
        o_on.hand
    );
    for e in &o_on.plan.ends {
        if let crate::Onto::Plate(k) = e.onto
            && let Some(w) = &o_on.plan.plates[k].own
        {
            assert!(o_on.plan.plates[w.welded_to].short, "{:?}", o_on.plan);
        }
    }
    // Off: found after writing only (the flat plate, nothing bolted on it,
    // now its welded plates' members' too: before, written so), the ends
    // on it taken back.
    assert!(
        !o_off.plan.ends.iter().any(|e| matches!(
            e.onto,
            crate::Onto::Plate(k) if o_off.plan.plates[k].own.is_some()
        )),
        "{:?}",
        o_off.plan
    );
    // A run again: the same file.
    let again = run(&on.text, &welded(true), Some(&["N2"]), false);
    assert_eq!(again.text, on.text);
}
