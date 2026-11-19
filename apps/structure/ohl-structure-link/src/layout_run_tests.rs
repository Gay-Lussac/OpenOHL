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
/// where D1 would go).
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
            .any(|h| h.starts_with("D1: laid out, ") && h.contains("would run")),
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
fn in_line(upper: &str, bolts: &str) -> String {
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
    let r = run(&text, &settings(), Some(&["N2"]), false);
    let o = r.joints.iter().find(|j| j.joint == "N2").unwrap();
    assert!(o.written, "{:?} {:?}", o.hand, r.notes);
    let e = o
        .plan
        .ends
        .iter()
        .find(|e| e.member == "D1")
        .unwrap_or_else(|| panic!("D1 not laid out: {:?}", o.plan));
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
    // An edit (the flat plate 2 mm thicker): followed as the whole file's.
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
    let edited = doc.to_string();
    let f = follow(&r.text, &edited, &library(), &Default::default(), &[]).unwrap_or(
        crate::layout_run::Followed {
            text: edited.clone(),
            message: String::new(),
            again: Vec::new(),
            misfits: Vec::new(),
        },
    );
    let again = run(&f.text, &settings(), Some(&["N2"]), false);
    let t2 = parse(&again.text);
    let w2 = t2
        .plates
        .iter()
        .find(|p| !p.welds.is_empty())
        .expect("the welded plate");
    assert_eq!(w2.welds[0].size.map(|v| v.si()), Some(0.005), "{w2:?}");
}

/// The owner's step 1c (2026-11-19): two angles in line of other sections
/// that do not nest, both with their bolts (a splice where the section
/// changes, nothing else bolting onto them there), each bolted onto a
/// splice of covers on both faces of its leg (two shear planes); nothing
/// clashes, a run again is the same, and a cover changed by hand is kept.
/// With a brace bolting onto them at the joint (its bolts would pass
/// through a cover): no splice, as before.
#[test]
fn other_sections_in_line_are_spliced_with_covers() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../libraries/sections/example-sections.ohlsec");
    let lib = crate::SectionLibrary::parse(&format!(
        "{}\n[[section]]\nid = \"EA-61x9\"\nname = \"Equal angle 61 × 9 (invented)\"\n\
         shape = \"equal_angle\"\nh = \"61 mm\"\nt = \"9 mm\"\nr = \"7 mm\"\n",
        std::fs::read_to_string(path).unwrap()
    ))
    .unwrap();
    let braced = in_line("EA-61x9", ", bolts = { diameter = \"16 mm\", count = 4 }");
    let r = run_with(&braced, &lib);
    let o = r.joints.iter().find(|j| j.joint == "J").unwrap();
    assert!(o.plan.joins.is_empty(), "{:?}", o.plan);
    let text = braced.replace(
        "[[member]]\nid = \"D\"\nnodes = [\"E\", \"J\"]\nsection = \"EA-61x7\"\n\
         orientation = \"y-z+\"\ndesign = { class = \"bracing\" }\n",
        "",
    );
    assert_ne!(text, braced);
    let r = run_with(&text, &lib);
    let o = r.joints.iter().find(|j| j.joint == "J").unwrap();
    assert!(o.written, "{:?}", o.hand);
    assert_eq!(o.plan.joins.len(), 1, "{:?}", o.plan);
    let x = &o.plan.joins[0];
    assert_eq!(x.how, crate::JoinKind::Spliced);
    assert!((x.thickness - 0.009).abs() < 1e-9, "{x:?}");
    let t = parse(&r.text);
    assert_eq!(t.splices.len(), 1, "{:?}", t.splices);
    let sp = &t.splices[0];
    assert_eq!(sp.joint, "J");
    assert_eq!(sp.covers.len(), 2 * x.covers.len());
    for m in ["C1", "C2"] {
        assert_eq!(
            t.connection(m, "J").unwrap().onto.as_deref(),
            Some(sp.id.as_str())
        );
    }
    assert!(
        LayoutRecord::of(&t).made[&sp.id]
            .1
            .contains(crate::layout_run::SPLICE)
    );
    let d = joint_details(&r.text, &lib, &Default::default()).unwrap();
    let j = d.joint("J").unwrap();
    assert_eq!(j.splices.len(), 1);
    for m in ["C1", "C2"] {
        let a = j.arrivals.iter().find(|a| a.member == m).unwrap();
        assert_eq!(a.planes.0.value, Some(2), "{m}: {:?}", a.planes);
    }
    // What was laid out runs into nothing but a member left as it was
    // (here D, whose bolts would need to pass through the cover: said).
    let left = |p: &crate::ClashPart| {
        p.member()
            .is_some_and(|m| o.plan.hand_members.iter().any(|h| h == m))
    };
    assert!(
        j.clashes.iter().all(|c| left(&c.a) || left(&c.b)),
        "{:?} {:?}",
        j.clashes,
        o.hand
    );
    assert_eq!(run_with(&r.text, &lib).text, r.text);
    // A cover made thicker by hand: kept on a run again, the user's.
    let mut doc: toml_edit::DocumentMut = r.text.parse().unwrap();
    let face = sp.covers[0].face;
    let leg = sp.covers[0].leg;
    ohl_structure_edit::connection_edits::set_cover(&mut doc, &sp.id, face, leg, Some(0.012))
        .unwrap();
    let edited = doc.to_string();
    let again = run_with(&edited, &lib);
    let t2 = parse(&again.text);
    assert_eq!(t2.splices.len(), 1, "{:?}", t2.splices);
    assert!(
        t2.splices[0]
            .covers
            .iter()
            .any(|c| (c.thickness.si() - 0.012).abs() < 1e-9)
    );
    for m in ["C1", "C2"] {
        assert_eq!(
            t2.connection(m, "J").unwrap().onto.as_deref(),
            Some(t2.splices[0].id.as_str())
        );
    }
}
