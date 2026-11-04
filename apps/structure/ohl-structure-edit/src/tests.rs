// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

use std::path::{Path, PathBuf};

use ohl_structures::{Detail, InsulatorLibrary, LibraryItem, StructureType};
use toml_edit::DocumentMut;

use super::*;

const PYLON: &str = r#"id = "P"
name = "p"
kind = "lattice"

[[node]]
id = "N1"
point = ["0 m", "0 m", "0 m"]
support = "pinned"

[[node]]
id = "N2"
point = ["0 m", "0 m", "30 m"]

[[node]]
id = "N3"
point = ["-6 m", "0 m", "30 m"]

[[member]]
id = "M1"
nodes = ["N1", "N2"]

[[member]]
id = "M2"
nodes = ["N2", "N3"]

[[point]]
label = "A1"
node = "N3"
"#;

fn read(doc: &DocumentMut) -> StructureType {
    StructureType::parse(Path::new("p.ohls"), &doc.to_string()).unwrap()
}

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/synthetic-line/Structures")
}

fn example(name: &str) -> StructureFile {
    StructureFile::open(&examples().join(name)).unwrap()
}

#[test]
fn joints_members_points_and_strings_are_added() {
    let mut doc: DocumentMut = PYLON.parse().unwrap();
    let t = read(&doc);
    assert_eq!(t.detail(), Detail::Outline);
    let id = add_joint(&mut doc, Some(&t), [6.0, 0.0, 30.0]);
    assert_eq!(id, "N4");
    let t = read(&doc);
    assert_eq!(add_member(&mut doc, &t, "N2", "N4").unwrap(), "M3");
    let t = read(&doc);
    assert!(add_member(&mut doc, &t, "N4", "N2").is_err());
    assert!(add_member(&mut doc, &t, "N4", "N4").is_err());
    let a2 = add_point_at_joint(&mut doc, &t, "N4");
    let t = read(&doc);
    assert_eq!(t.point_local(&a2, None), Some([6.0, 0.0, 30.0]));
    // An I under A1, a V from A1 and A2.
    let w = add_strings(&mut doc, &t, &["A1".into()], 4.0).unwrap();
    let t = read(&doc);
    let placed = t
        .place_strings([0.0; 3], 0.0, None, &InsulatorLibrary::default())
        .unwrap();
    let wp = placed.wires.iter().find(|x| x.label == w).unwrap();
    // No set: set numbers are put by hand.
    assert_eq!((wp.position, wp.set), ([-6.0, 0.0, 26.0], None));
    let v = add_strings(&mut doc, &t, &["A1".into(), a2], 3.0).unwrap();
    let t = read(&doc);
    let placed = t
        .place_strings([0.0; 3], 0.0, None, &InsulatorLibrary::default())
        .unwrap();
    let vp = placed.wires.iter().find(|x| x.label == v).unwrap();
    assert!((vp.position[2] - 27.0).abs() < 1e-3 && vp.position[0].abs() < 1e-9);
    assert_eq!(vp.set, None);
    assert!(add_strings(&mut doc, &t, &[], 3.0).is_err());
    assert!(add_strings(&mut doc, &t, &["A1".into()], 80.0).is_err());
}

#[test]
fn a_free_point_is_added_and_moved() {
    let mut doc: DocumentMut = PYLON.parse().unwrap();
    let t = read(&doc);
    let a = add_point(&mut doc, &t, [3.0, 0.0, 20.0]);
    assert_eq!(a, "A2");
    let t = read(&doc);
    assert_eq!(t.point_local(&a, None), Some([3.0, 0.0, 20.0]));
    move_to(&mut doc, &t, &Handle::Point(a.clone()), [3.5, 0.0, 21.0]).unwrap();
    let t = read(&doc);
    assert_eq!(t.point_local(&a, None), Some([3.5, 0.0, 21.0]));
    assert!(move_to(&mut doc, &t, &Handle::Member("M1".into()), [0.0; 3]).is_err());
}

#[test]
fn deleting_a_joint_takes_its_members_points_and_strings() {
    let mut doc: DocumentMut = PYLON.parse().unwrap();
    let t = read(&doc);
    add_strings(&mut doc, &t, &["A1".into()], 4.0).unwrap();
    let t = read(&doc);
    let r = removal(&t, &Handle::Joint("N3".into()));
    assert_eq!(r.joints, vec!["N3"]);
    assert_eq!(r.members, vec!["M2"]);
    assert_eq!(r.points, vec!["A1"]);
    assert_eq!(r.strings, vec!["S1"]);
    assert_eq!(r.lines().len(), 4);
    remove(&mut doc, &r);
    let t = read(&doc);
    assert_eq!((t.nodes.len(), t.members.len(), t.points.len()), (2, 1, 0));
    assert!(t.strings.is_empty());
}

/// A point on a joint moved stays on it, its offset the distance; the
/// joint does not move (owner decision 2026-10-21: the link follows
/// edits).
#[test]
fn moving_a_point_on_a_joint_keeps_the_joint() {
    let mut doc: DocumentMut = PYLON.parse().unwrap();
    let t = read(&doc);
    let joint = t.nodes[2].point;
    move_to(&mut doc, &t, &Handle::Point("A1".into()), [-7.0, 0.0, 31.0]).unwrap();
    let t = read(&doc);
    assert_eq!(t.point_local("A1", None), Some([-7.0, 0.0, 31.0]));
    assert_eq!(t.nodes[2].point, joint);
    assert_eq!(
        t.point_link("A1"),
        Some((t.nodes[2].id.as_str(), [-1.0, 0.0, 1.0]))
    );
}

/// Insulators: a new one from a point (its wire point placed by its
/// length, hanging), a second to the same wire point (a V: where their
/// lengths meet), one from the library (strain: level), saved back to the
/// library, deleted (its wire point with it when nothing else is on it).
#[test]
fn insulators_are_made_from_the_library_saved_and_deleted() {
    let mut doc: DocumentMut = PYLON.parse().unwrap();
    let mut t = read(&doc);
    let a2 = add_point_at_joint(&mut doc, &t, "N2");
    t = read(&doc);
    let none = InsulatorLibrary::default();
    let (s1, w1) = new_string(&mut doc, &t, "A1", None, 3.0, None).unwrap();
    t = read(&doc);
    let g = geometry(&t, &none);
    assert_eq!(g.at(&w1), Some([-6.0, 0.0, 27.0]));
    // A V: a second insulator to the same wire point, from A2 (6 m away).
    let (_, w) = new_string(&mut doc, &t, &a2, None, 5.0, Some(&w1)).unwrap();
    assert_eq!(w, w1);
    t = read(&doc);
    let g = geometry(&t, &none);
    // The first leg 5 m too: they meet 4 m below the middle.
    set_string(&mut doc, &t, &g, &none, &s1, StringChange::Length(5.0)).unwrap();
    t = read(&doc);
    let g = geometry(&t, &none);
    let v = g.at(&w1).unwrap();
    assert!(
        (v[0] + 3.0).abs() < 1e-9 && (v[2] - 26.0).abs() < 1e-9,
        "{v:?}"
    );
    // From the library: a strain assembly, level (ahead).
    let lib_dir = std::env::temp_dir().join(format!("ohl-edit-ins-{}", std::process::id()));
    std::fs::create_dir_all(&lib_dir).unwrap();
    std::fs::write(
        lib_dir.join("D.ohli"),
        "id = \"D\"\nname = \"dead end\"\nkind = \"strain\"\nlength = \"4 m\"\nweight = \"1 kN\"\n",
    )
    .unwrap();
    let lib = InsulatorLibrary::load_dir(&lib_dir).unwrap();
    let d = lib.get("D").unwrap();
    let (s3, w3) = new_string(&mut doc, &t, "A1", Some(d), 4.0, None).unwrap();
    t = read(&doc);
    let g = geometry(&t, &lib);
    assert_eq!(g.at(&w3), Some([-6.0, 4.0, 30.0]));
    assert!(t.strings.iter().find(|x| x.label == s3).unwrap().strain);
    // Saved to the library: a strain assembly of its length and weight.
    let text = assembly_text(&t, &g, &lib, &s3, "D2").unwrap();
    let back = ohl_structures::InsulatorAssembly::parse(Path::new("D2.ohli"), &text).unwrap();
    assert_eq!(back.kind, ohl_structures::AssemblyKind::Strain);
    assert!((back.length.si() - 4.0).abs() < 1e-9 && (back.weight.si() - 1000.0).abs() < 1e-9);
    // The V's leg: a V-string assembly, half width from its tops.
    let leg = assembly_text(&t, &g, &lib, &s1, "V2").unwrap();
    assert!(
        leg.contains("kind = \"v_string\"") && leg.contains("half_width = \"3 m\""),
        "{leg}"
    );
    // Deleted: its own wire point goes; the V's stays (the other leg).
    delete_string(&mut doc, &t, &g, &s3);
    delete_string(&mut doc, &t, &g, &s1);
    t = read(&doc);
    assert!(t.wires.iter().all(|w| w.label != w3));
    assert!(t.wires.iter().any(|w| w.label == w1));
    std::fs::remove_dir_all(&lib_dir).ok();
}

/// Every field of an insulator row: direction, weight, wind area,
/// compression, description, kind, top; a bad length refused.
#[test]
fn insulator_fields_are_changed() {
    let mut doc: DocumentMut = PYLON.parse().unwrap();
    let t = read(&doc);
    let none = InsulatorLibrary::default();
    let (s, w) = new_string(&mut doc, &t, "A1", None, 3.0, None).unwrap();
    let change = |doc: &mut DocumentMut, c: StringChange| {
        let t = read(doc);
        let g = geometry(&t, &none);
        set_string(doc, &t, &g, &none, &s, c)
    };
    change(&mut doc, StringChange::Direction(90.0, 90.0)).unwrap();
    let g = geometry(&read(&doc), &none);
    let p = g.at(&w).unwrap();
    assert!(
        (p[0] + 3.0).abs() < 1e-9 && (p[2] - 30.0).abs() < 1e-9,
        "{p:?}"
    );
    change(&mut doc, StringChange::Weight(500.0)).unwrap();
    change(&mut doc, StringChange::WindArea(0.4)).unwrap();
    change(&mut doc, StringChange::Compression(Some(2000.0))).unwrap();
    change(&mut doc, StringChange::Description("jumper".into())).unwrap();
    change(&mut doc, StringChange::Strain(true)).unwrap();
    let t = read(&doc);
    let x = &t.strings[0];
    assert_eq!(x.weight.unwrap().si(), 500.0);
    assert!((x.wind_area.unwrap().si() - 0.4).abs() < 1e-12);
    assert_eq!(x.compression.unwrap().si(), 2000.0);
    assert_eq!(x.description, "jumper");
    assert!(x.strain);
    change(&mut doc, StringChange::Compression(None)).unwrap();
    change(&mut doc, StringChange::Description(" ".into())).unwrap();
    let t = read(&doc);
    assert!(t.strings[0].compression.is_none() && t.strings[0].description.is_empty());
    assert!(change(&mut doc, StringChange::Length(0.0)).is_err());
    assert!(change(&mut doc, StringChange::Top(w.clone())).is_err());
    assert!(change(&mut doc, StringChange::Assembly(Some("nope".into()))).is_err());
}

/// A member made tension-only: one edit writes the kind and raises the
/// file to the version that has it; other kinds leave the version as it
/// is. The tables offer the four kinds.
#[test]
fn a_tension_only_member_raises_the_version() {
    let mut doc: DocumentMut = format!("format = \"ohl-structure\"\nversion = 3\n{PYLON}")
        .parse()
        .unwrap();
    set_member_kind(&mut doc, "M1", "frame");
    assert!(doc.to_string().contains("version = 3"));
    set_member_kind(&mut doc, "M2", "tension");
    let text = doc.to_string();
    assert!(
        text.contains("version = 5") && text.contains("kind = \"tension\""),
        "{text}"
    );
    assert_eq!(read(&doc).members[1].kind.as_deref(), Some("tension"));
    // A file without a version stays without (it reads as the newest).
    let mut plain: DocumentMut = PYLON.parse().unwrap();
    set_member_kind(&mut plain, "M1", "tension");
    assert!(!plain.to_string().contains("version"));
    let names: Vec<&str> = MEMBER_KINDS.iter().map(|k| k.0).collect();
    assert_eq!(names, ["truss", "frame", "cable", "tension"]);
}

/// An insulator's limits (kept, not checked): each set and cleared by
/// its own edit, written with its unit; the file raised to version 4
/// when one is given; bad values refused (nothing written); saved to
/// the library with them.
#[test]
fn insulator_limits_are_edited() {
    let mut doc: DocumentMut = format!("format = \"ohl-structure\"\nversion = 2\n{PYLON}")
        .parse()
        .unwrap();
    let t = read(&doc);
    let none = InsulatorLibrary::default();
    let (s, _) = new_string(&mut doc, &t, "A1", None, 3.0, None).unwrap();
    let change = |doc: &mut DocumentMut, c: StringChange| {
        let t = read(doc);
        let g = geometry(&t, &none);
        set_string(doc, &t, &g, &none, &s, c)
    };
    // Clearing one not given changes nothing (the version stays).
    change(&mut doc, StringChange::MaxLoad(None)).unwrap();
    assert!(doc.to_string().contains("version = 2"));
    change(&mut doc, StringChange::MaxLoad(Some(120_000.0))).unwrap();
    assert!(doc.to_string().contains("version = 4"));
    change(&mut doc, StringChange::MaxSwing(Some(60.0))).unwrap();
    change(&mut doc, StringChange::MaxSwingAcross(Some(55.5))).unwrap();
    change(&mut doc, StringChange::MaxSwingAlong(Some(30.0))).unwrap();
    let text = doc.to_string();
    assert!(text.contains("max_load = \"120000 N\""), "{text}");
    assert!(text.contains("max_swing_across = \"55.5 deg\""), "{text}");
    let t = read(&doc);
    let l = t.strings[0].limits();
    assert_eq!(l.max_load, Some(120_000.0));
    assert_eq!(l.max_swing_deg, Some(60.0));
    assert_eq!(l.max_swing_across_deg, Some(55.5));
    assert_eq!(l.max_swing_along_deg, Some(30.0));
    // Saved to the library: its limits go along (an .ohli version 2).
    let g = geometry(&t, &none);
    let ohli = assembly_text(&t, &g, &none, &s, "lim").unwrap();
    assert!(
        ohli.contains("version = 2") && ohli.contains("max_swing_along = \"30 deg\""),
        "{ohli}"
    );
    for bad in [
        StringChange::MaxLoad(Some(0.0)),
        StringChange::MaxSwing(Some(181.0)),
        StringChange::MaxSwingAlong(Some(-1.0)),
    ] {
        assert!(change(&mut doc, bad).is_err());
    }
    assert_eq!(read(&doc).strings[0].limits(), l);
    change(&mut doc, StringChange::MaxLoad(None)).unwrap();
    change(&mut doc, StringChange::MaxSwingAcross(None)).unwrap();
    let t = read(&doc);
    let l = t.strings[0].limits();
    assert_eq!((l.max_load, l.max_swing_across_deg), (None, None));
    assert_eq!(l.max_swing_deg, Some(60.0));
    assert!(!doc.to_string().contains("max_load"));
}

/// Sets: added (next free), renumbered (a taken number refused), moved
/// to another wire point, taken off; a counterweight.
#[test]
fn sets_and_counterweights_are_edited() {
    let mut doc: DocumentMut = PYLON.parse().unwrap();
    let t = read(&doc);
    add_strings(&mut doc, &t, &["A1".into()], 4.0).unwrap();
    let t = read(&doc);
    add_strings(&mut doc, &t, &["A1".into()], 2.0).unwrap();
    let t = read(&doc);
    assert_eq!(add_set(&mut doc, &t, "W1"), Some(1));
    let t = read(&doc);
    assert_eq!(add_set(&mut doc, &t, "W2"), Some(2));
    let t = read(&doc);
    assert!(set_set(&mut doc, &t, "W1", 2).is_err());
    set_set(&mut doc, &t, "W1", 7).unwrap();
    drop_set(&mut doc, "W2");
    let t = read(&doc);
    move_set(&mut doc, "W1", "W2", 7);
    set_counterweight(&mut doc, "W2", 1500.0);
    let t2 = read(&doc);
    assert_eq!(t.wires[0].set, Some(7));
    assert_eq!((t2.wires[0].set, t2.wires[1].set), (None, Some(7)));
    assert_eq!(t2.wires[1].weight.unwrap().si(), 1500.0);
    set_counterweight(&mut doc, "W2", 0.0);
    assert!(read(&doc).wires[1].weight.is_none());
}

/// Design: supports, sections, kinds, k; sections and materials added,
/// typed, removed — up to a full design.
#[test]
fn a_design_is_completed_in_tables() {
    let mut doc: DocumentMut = PYLON.parse().unwrap();
    set_support(&mut doc, "N1", None);
    assert_eq!(read(&doc).detail(), Detail::Outline);
    let m = add_item(&mut doc, "material", "steel", &[("elasticity", "210 GPa")]);
    let s = add_item(&mut doc, "section", "S1", &[("material", &m)]);
    set_item_field(&mut doc, "section", &s, "area", "1500 mm2").unwrap();
    set_item_field(&mut doc, "section", &s, "i_min", "80 cm4").unwrap();
    set_item_field(&mut doc, "material", &m, "density", "7850 kg/m3").unwrap();
    assert!(set_item_field(&mut doc, "section", &s, "id", "x").is_err());
    // A number that is not finite is refused, never written as TOML
    // nan/inf (code review 2026-10-26).
    for bad in ["nan", "inf", "-inf", "1e400"] {
        let e = set_item_field(&mut doc, "section", &s, "area", bad).unwrap_err();
        assert!(e.contains("finite"), "{e}");
    }
    assert_eq!(item_ids(&doc, "section"), vec!["S1"]);
    assert!(
        item_fields(&doc, "section", "S1").contains(&("area".into(), "1500 mm2".into())),
        "{:?}",
        item_fields(&doc, "section", "S1")
    );
    for id in ["M1", "M2"] {
        set_member_section(&mut doc, id, Some(&s));
    }
    set_member_kind(&mut doc, "M2", "frame");
    set_member_k(&mut doc, "M2", Some(0.7));
    assert_eq!(read(&doc).detail(), Detail::Outline);
    set_support(&mut doc, "N1", Some("fixed"));
    let t = read(&doc);
    assert_eq!(t.detail(), Detail::Design);
    assert_eq!(t.members[1].kind.as_deref(), Some("frame"));
    assert_eq!(t.members[1].k, Some(0.7));
    set_member_section(&mut doc, "M1", None);
    assert_eq!(read(&doc).detail(), Detail::Outline);
    set_item_field(&mut doc, "section", &s, "i_min", "").unwrap();
    assert!(!doc.to_string().contains("i_min"));
    remove_item(&mut doc, "material", "id", &m);
    assert!(doc.get("material").is_none());
}

/// The analysis settings: a field written into `[analysis]` (made when
/// missing), read back by the structure, removed with the table when it is
/// the last.
#[test]
fn analysis_settings_are_edited() {
    let mut doc: DocumentMut = PYLON.parse().unwrap();
    set_analysis_field(&mut doc, "stabilise", "false").unwrap();
    let t = read(&doc);
    let a = t.analysis.as_ref().unwrap();
    assert_eq!(
        a.get("stabilise").and_then(toml::Value::as_bool),
        Some(false)
    );
    set_analysis_field(&mut doc, "dead_load_factor", "1.25").unwrap();
    let t = read(&doc);
    let a = t.analysis.as_ref().unwrap();
    assert_eq!(
        a.get("dead_load_factor").and_then(toml::Value::as_float),
        Some(1.25)
    );
    set_analysis_field(&mut doc, "stabilise", "").unwrap();
    set_analysis_field(&mut doc, "dead_load_factor", "").unwrap();
    assert!(doc.get("analysis").is_none());
    assert!(read(&doc).analysis.is_none());
    assert!(set_analysis_field(&mut doc, " ", "1").is_err());
    // Not finite: refused before the table is made.
    assert!(set_analysis_field(&mut doc, "dead_load_factor", "NaN").is_err());
    assert!(doc.get("analysis").is_none());
}

/// A member's analysis fields typed (one member, or every member of a
/// group), read back by the structure; empty removes them.
#[test]
fn member_analysis_fields_are_edited() {
    let mut doc: DocumentMut = PYLON.parse().unwrap();
    set_item_field(&mut doc, "member", "M1", "capacity_tension", "180 kN").unwrap();
    set_item_field(&mut doc, "member", "M1", "orientation", "x+y-").unwrap();
    let t = read(&doc);
    assert_eq!(
        t.members[0]
            .capacity_tension
            .as_ref()
            .and_then(|v| v.as_str()),
        Some("180 kN")
    );
    assert_eq!(
        t.members[0].orientation.as_ref().and_then(|v| v.as_str()),
        Some("x+y-")
    );
    let ids: Vec<String> = t.members.iter().map(|m| m.id.clone()).collect();
    set_group(&mut doc, &t, &ids, Some("legs"));
    let t = read(&doc);
    set_group_field(&mut doc, &t, "legs", "lk", "2.5 m").unwrap();
    let t = read(&doc);
    assert!(
        t.members
            .iter()
            .all(|m| m.lk.as_ref().and_then(|v| v.as_str()) == Some("2.5 m"))
    );
    set_group_field(&mut doc, &t, "legs", "lk", "").unwrap();
    assert!(read(&doc).members.iter().all(|m| m.lk.is_none()));
}

/// The file's own fields and the height extensions.
#[test]
fn name_height_foot_and_extensions_are_edited() {
    let mut doc: DocumentMut = PYLON.parse().unwrap();
    set_top_field(&mut doc, "name", Some("tower"), false).unwrap();
    set_top_field(&mut doc, "height", Some("32.5"), true).unwrap();
    set_top_field(&mut doc, "foot", Some("-1 m"), true).unwrap();
    // A mistyped or non-finite length is refused, the file unchanged
    // (code review 2026-10-26: it was written as 0 m).
    let before = doc.to_string();
    for bad in ["32,5", "abc m", "nan", "inf m", "1e400"] {
        let e = set_top_field(&mut doc, "height", Some(bad), true).unwrap_err();
        assert!(
            e.starts_with("height: ") && e.contains("a length in m"),
            "{e}"
        );
    }
    assert_eq!(doc.to_string(), before);
    set_extension(&mut doc, "+3", 3.0);
    set_extension(&mut doc, "+6", 6.0);
    set_extension(&mut doc, "+3", 3.5);
    let t = read(&doc);
    assert_eq!(t.name, "tower");
    assert_eq!(t.height.unwrap().si(), 32.5);
    assert_eq!(t.foot_level(), -1.0);
    assert_eq!(t.extensions.len(), 2);
    assert_eq!(t.extensions[0].dz.si(), 3.5);
    remove_extension(&mut doc, "+3");
    set_top_field(&mut doc, "height", None, true).unwrap();
    let t = read(&doc);
    assert_eq!(t.extensions.len(), 1);
    assert!(t.height.is_none());
}

/// Comments and the order of the file stay through edits.
#[test]
fn comments_are_kept() {
    let mut f = example("type-02.ohls");
    f.edit("support", |doc, _| set_support(doc, "L0A", Some("fixed")));
    let text = f.text();
    assert!(text.contains("# The structure (invented, NOT a real design)"));
    assert!(text.contains("node = \"A1\"                          # at the right arm's tip"));
    assert!(text.starts_with("# Derived from the geometry of an existing line"));
}

/// Every edit is one undo step; a group (a drag) is one; redo makes them
/// again; a new edit drops the redo.
#[test]
fn edits_are_undone_and_redone() {
    let mut f = StructureFile::parse(PYLON, None).unwrap();
    let start = f.text();
    assert!(f.undo().is_none());
    let id = f.edit("add joint", |doc, t| {
        add_joint(doc, Some(t), [6.0, 0.0, 30.0])
    });
    assert_eq!(f.structure().nodes.len(), 4);
    let after_joint = f.text();
    f.edit("add member", |doc, t| add_member(doc, t, "N2", &id))
        .unwrap();
    assert_eq!(f.undo_name(), Some("add member"));
    // A drag: many moves, one step.
    f.begin("move joint");
    for k in 1..=5 {
        f.edit("move", |doc, t| {
            move_to(
                doc,
                t,
                &Handle::Joint(id.clone()),
                [6.0, 0.0, 30.0 + f64::from(k)],
            )
        })
        .unwrap();
    }
    f.end();
    assert_eq!(f.structure().nodes[3].point[2].si(), 35.0);
    assert_eq!(f.undo().as_deref(), Some("move joint"));
    assert_eq!(f.structure().nodes[3].point[2].si(), 30.0);
    assert_eq!(f.undo().as_deref(), Some("add member"));
    assert_eq!(f.text(), after_joint);
    assert_eq!(f.redo().as_deref(), Some("add member"));
    assert_eq!(f.structure().members.len(), 3);
    assert_eq!(f.redo_name(), Some("move joint"));
    // A new edit: no redo.
    f.edit("support", |doc, _| set_support(doc, "N2", Some("fixed")));
    assert!(f.redo().is_none());
    // Nothing changed: no step.
    f.edit("nothing", |_, _| ());
    assert_eq!(f.undo_name(), Some("support"));
    while f.undo().is_some() {}
    assert_eq!(f.text(), start);
}

/// The revision: another after every change of the text (edit, undo,
/// redo), the same while nothing changes, never the same for two files;
/// the text kept for a revision is the document's.
#[test]
fn every_change_is_a_new_revision() {
    let mut f = StructureFile::parse(PYLON, None).unwrap();
    let g = StructureFile::parse(PYLON, None).unwrap();
    assert_ne!(f.revision(), g.revision());
    let r0 = f.revision();
    assert_eq!(f.text_ref(), f.doc().to_string());
    f.edit("nothing", |_, _| ());
    assert_eq!(f.revision(), r0);
    f.edit("add joint", |doc, t| {
        add_joint(doc, Some(t), [6.0, 0.0, 30.0])
    });
    let r1 = f.revision();
    assert_ne!(r1, r0);
    assert_eq!(f.text_ref(), f.doc().to_string());
    assert!(f.text_ref().contains("30"));
    f.undo();
    assert!(f.revision() != r1 && f.revision() != r0);
    assert_eq!(f.text(), StructureFile::parse(PYLON, None).unwrap().text());
    let r2 = f.revision();
    f.redo();
    assert_ne!(f.revision(), r2);
    assert_eq!(f.text_ref(), f.doc().to_string());
    assert_eq!(f.structure().nodes.len(), 4);
}

/// Saving writes the file; Save as moves it; a file that does not read is
/// not saved.
#[test]
fn saved_and_saved_as() {
    let dir = std::env::temp_dir().join(format!("ohl-edit-save-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ohls");
    std::fs::write(&path, PYLON).unwrap();
    let mut f = StructureFile::open(&path).unwrap();
    assert!(!f.dirty());
    f.edit("support", |doc, _| set_support(doc, "N2", Some("fixed")));
    assert!(f.dirty());
    f.save(None).unwrap();
    assert!(!f.dirty());
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("support = \"fixed\"")
    );
    let other = dir.join("q.ohls");
    f.save(Some(&other)).unwrap();
    assert_eq!(f.path(), Some(other.as_path()));
    // A text that does not read: kept, the structure as it was, not saved.
    f.edit("bad", |doc, _| {
        doc.insert("kind", toml_edit::value("no such kind"));
    });
    assert!(f.error().is_some());
    assert_eq!(f.structure().nodes.len(), 3);
    assert!(f.save(None).is_err());
    f.undo();
    assert!(f.error().is_none());
    std::fs::remove_dir_all(&dir).ok();
}

fn level_items(m: &[Missing]) -> Vec<&str> {
    m.iter()
        .filter(|m| m.need == Need::Level)
        .map(|m| m.reason.as_str())
        .collect()
}

/// A stick figure (example type-01): for level 2 it needs members; no
/// advice without members.
#[test]
fn missing_for_level_two_of_a_stick_figure() {
    let f = example("type-01.ohls");
    let t = f.structure();
    assert_eq!(t.detail(), Detail::Stick);
    assert_eq!(next_level(t.detail()), Some(2));
    let m = missing(t, 2);
    assert_eq!(m.len(), 1, "{m:?}");
    assert_eq!(m[0].need, Need::Level);
    assert_eq!(m[0].place, Place::Members);
    assert!(m[0].reason.contains("no member"));
}

/// A full design (example type-02): nothing needed; advice for the
/// X-braces crossing without a joint, not for the arms.
#[test]
fn a_full_design_needs_nothing_but_gets_advice() {
    let f = example("type-02.ohls");
    let t = f.structure();
    assert_eq!(t.detail(), Detail::Design);
    assert_eq!(next_level(t.detail()), None);
    let m = missing(t, 3);
    assert!(level_items(&m).is_empty(), "{m:?}");
    let crossing: Vec<&Missing> = m.iter().filter(|m| m.reason.contains("cross")).collect();
    // Four faces of ten panels, and the plan bracing never crosses.
    assert_eq!(crossing.len(), 40, "{crossing:?}");
    assert!(
        crossing
            .iter()
            .any(|m| m.reason == "members m5 and m6 cross without a joint"),
        "{crossing:?}"
    );
    assert!(m.iter().all(|m| m.need == Need::Advice));
}

/// The full design without its sections and supports: level 2, and what
/// level 3 lacks — the members without a section in one line (many),
/// the supports.
#[test]
fn missing_for_level_three_names_the_members() {
    let mut f = example("type-02.ohls");
    f.edit("strip", |doc, t| {
        for m in &t.members {
            set_member_section(doc, &m.id, None);
        }
        for n in &t.nodes {
            set_support(doc, &n.id, None);
        }
    });
    let t = f.structure();
    assert_eq!(t.detail(), Detail::Outline);
    let m = missing(t, 3);
    let needed: Vec<&Missing> = m.iter().filter(|m| m.need == Need::Level).collect();
    assert_eq!(needed.len(), 2, "{needed:?}");
    assert!(
        needed[0]
            .reason
            .starts_with("178 members without a cross-section"),
        "{needed:?}"
    );
    assert!(needed.last().unwrap().reason.contains("no supported joint"));
    assert_eq!(needed.last().unwrap().place, Place::Joints);
}

/// Advice: a point on no joint, a joint on one member, a joint on none.
#[test]
fn advice_on_points_and_joints() {
    let mut doc: DocumentMut = PYLON.parse().unwrap();
    let t = read(&doc);
    add_point(&mut doc, &t, [1.0, 0.0, 10.0]);
    let t = read(&doc);
    add_joint(&mut doc, Some(&t), [0.0, 5.0, 0.0]);
    let t = read(&doc);
    let m = missing(&t, 3);
    let advice: Vec<&str> = m
        .iter()
        .filter(|m| m.need == Need::Advice)
        .map(|m| m.reason.as_str())
        .collect();
    assert_eq!(
        advice,
        vec![
            "point A2 is on no joint or member: it carries no load in the analysis",
            "joint N1 is on one member only",
            "joint N3 is on one member only",
            "joint N4 is on no member",
        ]
    );
    // Level 3 lacks a section on each member (from the section library
    // or the file's own [[section]]s: few, each named) and the point on
    // a joint of the members (as the level asks).
    let level = level_items(&m);
    assert_eq!(level.len(), 3, "{m:?}");
    assert!(
        level
            .iter()
            .any(|x| x.starts_with("point A2 is on no joint of the members")),
        "{m:?}"
    );
}

/// Two members crossing in space without a joint, and two that only
/// pass near each other.
#[test]
fn crossing_members_are_found() {
    let text = format!(
        "{PYLON}\n[[node]]\nid = \"N4\"\npoint = [\"-1 m\", \"0 m\", \"10 m\"]\n\n\
         [[node]]\nid = \"N5\"\npoint = [\"1 m\", \"0 m\", \"20 m\"]\n\n\
         [[node]]\nid = \"N6\"\npoint = [\"-1 m\", \"0.1 m\", \"25 m\"]\n\n\
         [[node]]\nid = \"N7\"\npoint = [\"1 m\", \"0.1 m\", \"26 m\"]\n\n\
         [[member]]\nid = \"M3\"\nnodes = [\"N4\", \"N5\"]\n\n\
         [[member]]\nid = \"M4\"\nnodes = [\"N6\", \"N7\"]\n"
    );
    let t = StructureType::parse(Path::new("p.ohls"), &text).unwrap();
    let m = missing(&t, 2);
    let crossing: Vec<&str> = m
        .iter()
        .filter(|m| m.reason.contains("cross"))
        .map(|m| m.reason.as_str())
        .collect();
    assert_eq!(crossing, vec!["members M1 and M3 cross without a joint"]);
    // An X of one group (as a generator makes them): one line for it.
    let x = format!(
        "{text}\n[[node]]\nid = \"X1\"\npoint = [\"-1 m\", \"0 m\", \"40 m\"]\n\n\
         [[node]]\nid = \"X2\"\npoint = [\"1 m\", \"0 m\", \"42 m\"]\n\n\
         [[node]]\nid = \"X3\"\npoint = [\"1 m\", \"0 m\", \"40 m\"]\n\n\
         [[node]]\nid = \"X4\"\npoint = [\"-1 m\", \"0 m\", \"42 m\"]\n\n\
         [[member]]\nid = \"D1\"\nnodes = [\"X1\", \"X2\"]\ngroup = \"body-x\"\n\n\
         [[member]]\nid = \"D2\"\nnodes = [\"X3\", \"X4\"]\ngroup = \"body-x\"\n"
    );
    let t = StructureType::parse(Path::new("p.ohls"), &x).unwrap();
    let m = missing(&t, 2);
    let crossing: Vec<&str> = m
        .iter()
        .filter(|m| m.reason.contains("cross"))
        .map(|m| m.reason.as_str())
        .collect();
    assert_eq!(
        crossing,
        vec![
            "members M1 and M3 cross without a joint",
            "two members of group body-x cross without a joint"
        ]
    );
}

#[test]
fn ids_follow_the_last() {
    let taken = ["T7", "T8", "S"];
    assert_eq!(next_id(taken.iter().copied(), "T7"), "T9");
    assert_eq!(next_id(taken.iter().copied(), "S"), "S-1");
    assert_eq!(next_id(taken.iter().copied(), "A09"), "A10");
}

/// Picking on the screen: a joint near the pointer before a member under
/// it; nothing far away.
#[test]
fn picked_on_the_screen() {
    let t = StructureType::parse(Path::new("p.ohls"), PYLON).unwrap();
    let g = geometry(&t, &InsulatorLibrary::default());
    // Seen from the front, 10 px per metre, z up.
    let project = |p: [f64; 3]| Some([100.0 + 10.0 * p[0], 400.0 - 10.0 * p[2]]);
    assert_eq!(
        pick(&g, &project, [41.0, 100.0], 8.0),
        Some(Handle::Point("A1".into()))
    );
    assert_eq!(
        pick(&g, &project, [101.0, 250.0], 8.0),
        Some(Handle::Member("M1".into()))
    );
    assert_eq!(pick(&g, &project, [300.0, 250.0], 8.0), None);
    assert_eq!(
        position(&g, &Handle::Member("M1".into())),
        Some([0.0, 0.0, 15.0])
    );
    assert!(compass_radius(&g) >= 0.35 * 30.0);
}

/// Connections and a plate at N2 (version 9, 2026-11-02).
const AT_N2: &str = r#"
[[plate]]
id = "G1"
joint = "N2"
thickness = "10 mm"

[[connection]]
member = "M2"
joint = "N2"
onto = "G1"

[[connection]]
member = "M1"
joint = "N2"
onto = "M2"

[[connection]]
member = "M2"
joint = "N3"
"#;

/// A member deleted takes its connections; one onto it loses its onto.
/// A joint deleted takes the connections and plates there.
#[test]
fn deletions_take_the_connections_along() {
    let text = format!("{PYLON}{AT_N2}");
    let mut doc: DocumentMut = text.parse().unwrap();
    let t = read(&doc);
    let r = removal(&t, &crate::Handle::Member("M2".into()));
    assert!(
        r.lines()
            .iter()
            .any(|l| l.contains("M2 at N2") && l.contains("M2 at N3"))
    );
    remove(&mut doc, &r);
    let t = read(&doc);
    assert_eq!(t.connections.len(), 1);
    assert_eq!(t.connections[0].member, "M1");
    assert_eq!(t.connections[0].onto, None);
    assert_eq!(t.plates.len(), 1);
    let mut doc: DocumentMut = text.parse().unwrap();
    let t = read(&doc);
    let r = removal(&t, &crate::Handle::Joint("N2".into()));
    assert!(r.connections.contains(&"plate G1".to_owned()));
    remove(&mut doc, &r);
    let t = read(&doc);
    assert!(t.connections.is_empty() && t.plates.is_empty());
    assert!(!doc.to_string().contains("[[connection]]"));
}

/// A joint put on a member: the member's far end is the second piece's,
/// its connection there follows.
#[test]
fn a_split_member_keeps_its_far_connection() {
    let mut doc: DocumentMut = format!("{PYLON}{AT_N2}").parse().unwrap();
    let t = read(&doc);
    crate::links::insert_joint(&mut doc, &t, "M2", 0.5).unwrap();
    let t = read(&doc);
    assert!(t.connection("M2", "N2").is_some());
    let far = t.connections.iter().find(|c| c.joint == "N3").unwrap();
    assert_ne!(far.member, "M2");
    assert!(
        t.members
            .iter()
            .any(|m| m.id == far.member && m.nodes.contains(&"N3".into()))
    );
    assert!(
        t.connection_notes().is_empty(),
        "{:?}",
        t.connection_notes()
    );
}

/// The kind toggled: written, nothing else changed, what is left unshown
/// said.
#[test]
fn the_kind_is_toggled_and_the_rest_kept() {
    use ohl_structures::structures::StructureKind;
    let mut doc: DocumentMut = format!("{PYLON}{AT_N2}").parse().unwrap();
    let t = read(&doc);
    let note = kind_change_note(&t, StructureKind::Pole).unwrap();
    assert!(
        note.contains("3 connection(s) and 1 plate(s) kept"),
        "{note}"
    );
    assert!(kind_change_note(&t, StructureKind::Portal).is_none());
    set_structure_kind(&mut doc, "pole").unwrap();
    let t = read(&doc);
    assert_eq!(t.kind, StructureKind::Pole);
    assert_eq!(t.connections.len(), 3);
    assert!(!is_tower(t.kind));
    assert!(set_structure_kind(&mut doc, "mast").is_err());
}

/// Stage 3: a field given makes the connection, cleared drops it; a
/// layout in one go; carried fields copied; a copy onto a plate copies the
/// plate; plates added, changed, removed.
#[test]
fn connections_are_defined_field_by_field() {
    use crate::connection_edits::{self as ce, BoltLayout, CopyEnd, FieldValue};
    let mut doc: DocumentMut = PYLON.parse().unwrap();
    ce::set_connection_field(
        &mut doc,
        "M2",
        "N2",
        "leg",
        Some(&FieldValue::Word("long".into())),
    )
    .unwrap();
    ce::set_connection_field(
        &mut doc,
        "M2",
        "N2",
        "pitch",
        Some(&FieldValue::Length(0.055)),
    )
    .unwrap();
    let t = read(&doc);
    let c = t.connection("M2", "N2").unwrap();
    assert!((c.pitch.unwrap().si() - 0.055).abs() < 1e-12);
    assert!(doc.to_string().contains("pitch = \"55 mm\""));
    assert!(ce::set_connection_field(&mut doc, "M2", "N2", "member", None).is_err());
    ce::set_connection_field(&mut doc, "M2", "N2", "pitch", None).unwrap();
    ce::set_connection_field(&mut doc, "M2", "N2", "leg", None).unwrap();
    assert!(!doc.to_string().contains("[[connection]]"));
    // A layout in one go.
    let l = BoltLayout {
        count: 3,
        bolt: None,
        diameter: 0.016,
        hole: Some(0.0175),
        grade: Some("8.8".into()),
        gauges: vec![0.03],
        pitch: 0.05,
        end: 0.03,
        edge: Some(0.025),
    };
    assert!((l.reach() - 0.13).abs() < 1e-12);
    // Three gauge lines (wide legs), the middle one staggered by half a
    // pitch: six bolts in two rows, the farthest the middle line's second,
    // 30 + 1.5 × 50 mm; a fourth line refused.
    let three = BoltLayout {
        count: 6,
        gauges: vec![0.035, 0.08, 0.125],
        ..l.clone()
    };
    assert!((three.reach() - 0.105).abs() < 1e-12);
    assert!(three.misfits(Some(0.2)).is_empty());
    let four = BoltLayout {
        gauges: vec![0.03, 0.06, 0.09, 0.12],
        ..three.clone()
    };
    assert!(
        four.misfits(Some(0.2))
            .iter()
            .any(|m| m.contains("one to 3 gauge lines"))
    );
    assert!(l.misfits(Some(0.06)).is_empty());
    assert!(l.misfits(Some(0.05))[0].contains("beyond the leg"));
    ce::set_bolt_layout(&mut doc, "M2", "N2", &l);
    let t = read(&doc);
    let c = t.connection("M2", "N2").unwrap();
    assert_eq!(c.bolts.as_ref().unwrap().count, 3);
    assert_eq!(c.gauge_lines, Some(1));
    // On a plate; copied to the other end (N3) with the plate.
    let t = read(&doc);
    let g = ce::add_plate(&mut doc, &t, "N2", 0.01);
    ce::set_connection_field(
        &mut doc,
        "M2",
        "N2",
        "onto",
        Some(&FieldValue::Word(g.clone())),
    )
    .unwrap();
    // Its end's own fit (2026-11-04): copied with it.
    ce::set_connection_field(&mut doc, "M2", "N2", "cut", Some(&FieldValue::Length(0.04))).unwrap();
    ce::set_connection_field(
        &mut doc,
        "M2",
        "N2",
        "shift",
        Some(&FieldValue::SignedLength(-0.015)),
    )
    .unwrap();
    let t = read(&doc);
    let n = ce::copy_ends(
        &mut doc,
        &t,
        &[CopyEnd {
            from_member: "M2".into(),
            from_joint: "N2".into(),
            to_member: "M2".into(),
            to_joint: "N3".into(),
            onto: None,
        }],
    );
    assert_eq!(n, 1);
    let t = read(&doc);
    assert_eq!(t.plates.len(), 2);
    let far = t.connection("M2", "N3").unwrap();
    assert_eq!(far.bolts.as_ref().unwrap().count, 3);
    assert!(
        (far.cut.unwrap().si() - 0.04).abs() < 1e-9,
        "the steel stops as at N2"
    );
    assert!(
        (far.shift.unwrap().si() + 0.015).abs() < 1e-9,
        "shifted as at N2"
    );
    let copy = far.onto.clone().unwrap();
    assert_ne!(copy, g);
    let copied = t.plates.iter().find(|p| p.id == copy).unwrap();
    assert_eq!(copied.joint, "N3");
    assert!(copied.welded_to.is_empty());
    assert!(
        t.connection_notes().is_empty(),
        "{:?}",
        t.connection_notes()
    );
    // The carried fields onto another member (onto kept out).
    let fields = ce::carried(&doc, "M2", "N2");
    assert!(fields.contains_key("bolts") && !fields.contains_key("onto"));
    ce::put_carried(&mut doc, "M1", "N2", &fields);
    let t = read(&doc);
    assert_eq!(
        t.connection("M1", "N2")
            .unwrap()
            .bolts
            .as_ref()
            .unwrap()
            .count,
        3
    );
    // Plates: thickness, material, removed (onto dropped).
    ce::set_plate(&mut doc, &g, Some(0.012), Some("S355")).unwrap();
    assert!(ce::set_plate(&mut doc, &g, Some(0.0), None).is_err());
    let t = read(&doc);
    let p = t.plates.iter().find(|p| p.id == g).unwrap();
    assert!((p.thickness.si() - 0.012).abs() < 1e-12);
    assert_eq!(p.material.as_deref(), Some("S355"));
    assert!(ce::set_welded_to(&mut doc, "nope", &[]).is_err());
    ce::remove_plate(&mut doc, &g);
    let t = read(&doc);
    assert!(t.connection("M2", "N2").unwrap().onto.is_none());
    ce::clear_connection(&mut doc, "M2", "N2");
    assert!(read(&doc).connection("M2", "N2").is_none());
}

/// Presets written and read back; refused when wrong.
#[test]
fn connection_presets_round_trip() {
    use crate::connection_edits::{self as ce, FieldValue};
    let mut doc: DocumentMut = PYLON.parse().unwrap();
    ce::set_connection_field(
        &mut doc,
        "M2",
        "N2",
        "bolts",
        Some(&FieldValue::Bolts {
            count: 2,
            bolt: Some("X17-example".into()),
            diameter: Some(0.016),
            hole: None,
            grade: Some("8.8".into()),
        }),
    )
    .unwrap();
    ce::set_connection_field(
        &mut doc,
        "M2",
        "N2",
        "onto",
        Some(&FieldValue::Word("M1".into())),
    )
    .unwrap();
    let p = ce::Preset {
        name: "bracing 2 × M16".into(),
        fields: ce::carried(&doc, "M2", "N2"),
    };
    let text = ce::presets_text(&[p]);
    let back = ce::parse_presets(&text).unwrap();
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].name, "bracing 2 × M16");
    assert!(back[0].fields.contains_key("bolts") && !back[0].fields.contains_key("onto"));
    for (bad, says) in [
        (
            "[[preset]]\nname = \"a\"\nonto = \"L1\"\n",
            "not a preset's",
        ),
        (
            "[[preset]]\nname = \"a\"\n[[preset]]\nname = \"a\"\n",
            "named twice",
        ),
        ("[[preset]]\nleg = \"long\"\n", "without a name"),
        ("[[preset]]\nname = \"a\"\nleg = \"longer\"\n", "preset a"),
        ("format = \"ohl-wire\"\n", "not ohl-connection-presets"),
    ] {
        let e = ce::parse_presets(bad).unwrap_err();
        assert!(e.contains(says), "{says}: {e}");
    }
}

/// The Detail view's edits: a member end's bolts written as their places
/// (the layout goes, the count follows), all taken away; a plate's
/// outline written.
#[test]
fn bolt_points_and_plate_outlines_are_written() {
    use crate::connection_edits::{self as ce, FieldValue};
    let mut doc: DocumentMut = PYLON.parse().unwrap();
    ce::set_connection_field(
        &mut doc,
        "M2",
        "N2",
        "pitch",
        Some(&FieldValue::Length(0.05)),
    )
    .unwrap();
    let kind = FieldValue::Bolts {
        count: 2,
        bolt: None,
        diameter: Some(0.016),
        hole: None,
        grade: None,
    };
    ce::set_bolt_points(
        &mut doc,
        "M2",
        "N2",
        &[[0.03, 0.025], [0.08, 0.025]],
        Some(&kind),
    );
    let t = read(&doc);
    let c = t.connection("M2", "N2").unwrap();
    assert_eq!(c.bolt_points.len(), 2);
    assert!(c.pitch.is_none());
    assert_eq!(c.bolts.as_ref().unwrap().count, 2);
    ce::set_bolt_points(&mut doc, "M2", "N2", &[[0.03, 0.025]], Some(&kind));
    assert_eq!(
        read(&doc)
            .connection("M2", "N2")
            .unwrap()
            .bolts
            .as_ref()
            .unwrap()
            .count,
        1
    );
    ce::set_bolt_points(&mut doc, "M2", "N2", &[], None);
    assert!(read(&doc).connection("M2", "N2").is_none());
    let t = read(&doc);
    let g = ce::add_plate(&mut doc, &t, "N2", 0.01);
    assert!(ce::set_plate_outline(&mut doc, &g, &[[0.0, 0.0, 30.0]]).is_err());
    ce::set_plate_outline(
        &mut doc,
        &g,
        &[[0.0, 0.0, 30.0], [0.1, 0.0, 30.0], [0.0, 0.0, 30.1]],
    )
    .unwrap();
    assert_eq!(read(&doc).plates[0].outline.len(), 3);
    // A shift may be negative.
    ce::set_connection_field(
        &mut doc,
        "M1",
        "N2",
        "shift",
        Some(&FieldValue::SignedLength(-0.015)),
    )
    .unwrap();
    assert!(
        (read(&doc)
            .connection("M1", "N2")
            .unwrap()
            .shift
            .unwrap()
            .si()
            + 0.015)
            .abs()
            < 1e-12
    );
}

/// A weld's size, length and kind (2026-11-04, `[[plate.weld]]`, still
/// version 9): written on the plate whose `welded_to` named the other
/// (the name moved into the weld), read back with the file's round trip;
/// cleared; the kind written only when not a fillet; unwelding and
/// removing a plate take the weld out; a size of 0 refused.
#[test]
fn a_weld_gets_its_size_and_length() {
    use crate::connection_edits::{self as ce, WeldEdit};
    use ohl_structures::connections::WeldKind;
    let mut doc: DocumentMut = PYLON.parse().unwrap();
    let t = read(&doc);
    let g1 = ce::add_plate(&mut doc, &t, "N2", 0.01);
    let t = read(&doc);
    let g2 = ce::add_plate(&mut doc, &t, "N2", 0.01);
    let t = read(&doc);
    let g3 = ce::add_plate(&mut doc, &t, "N2", 0.01);
    let t = read(&doc);
    ce::weld_pair(&mut doc, &t, &g2, &g1, true).unwrap();
    let t = read(&doc);
    let edit = WeldEdit {
        size: Some(Some(0.006)),
        length: Some(Some(0.22)),
        ..WeldEdit::default()
    };
    ce::set_weld(&mut doc, &t, &g1, &g2, edit).unwrap();
    let text = doc.to_string();
    assert!(
        text.contains(&format!(
            "id = \"{g2}\"\njoint = \"N2\"\nthickness = \"10 mm\"\n\n[[plate.weld]]\nto = \
             \"{g1}\"\nsize = \"6 mm\"\nlength = \"220 mm\""
        )),
        "{text}"
    );
    // Round trip: parsed from the text written.
    let doc2: DocumentMut = text.parse().unwrap();
    let t = read(&doc2);
    assert!(t.plates[1].welded_to.is_empty(), "moved into the weld");
    let w = t.weld_between(&g1, &g2).unwrap();
    assert!((w.size.unwrap().si() - 0.006).abs() < 1e-12);
    assert!((w.length.unwrap().si() - 0.22).abs() < 1e-12);
    assert_eq!(w.kind, None);
    assert_eq!(t.welded_assemblies("N2")[0], vec![g1.clone(), g2.clone()]);
    // A plate after it keeps its own data (the weld stays with G2).
    assert!(t.plates[2].welds.is_empty() && t.plates[2].id == g3);
    // The length cleared (the seam's), the kind full penetration.
    let edit = WeldEdit {
        length: Some(None),
        kind: Some(WeldKind::FullPenetration),
        ..WeldEdit::default()
    };
    ce::set_weld(&mut doc, &t, &g2, &g1, edit).unwrap();
    let t = read(&doc);
    let w = t.weld_between(&g1, &g2).unwrap();
    assert!(w.length.is_none() && w.size.is_some());
    assert_eq!(w.kind(), WeldKind::FullPenetration);
    // A new weld welds the pair.
    let edit = WeldEdit {
        size: Some(Some(0.005)),
        ..WeldEdit::default()
    };
    ce::set_weld(&mut doc, &t, &g3, &g2, edit).unwrap();
    let t = read(&doc);
    assert_eq!(t.welded_assemblies("N2").len(), 1);
    assert!(
        ce::set_weld(
            &mut doc,
            &t,
            &g3,
            &g2,
            WeldEdit {
                size: Some(Some(0.0)),
                ..WeldEdit::default()
            }
        )
        .is_err()
    );
    // Unwelded: the weld gone; a plate removed: its welds gone.
    ce::weld_pair(&mut doc, &t, &g1, &g2, false).unwrap();
    let t = read(&doc);
    assert!(t.weld_between(&g1, &g2).is_none());
    ce::remove_plate(&mut doc, &g2);
    let t = read(&doc);
    assert!(t.plates.iter().all(|p| p.welds.is_empty()), "{doc}");
    assert!(
        t.connection_notes().is_empty(),
        "{:?}",
        t.connection_notes()
    );
}

/// Plates welded to each other (owner 2026-11-04: a welded plate is
/// several plates welded together): a pair welded and unwelded, the
/// 2026-11-03 `kind = "welded"` written as what it is read as, a plate
/// removed taken out of the others' `welded_to`; a welded plate made for
/// the members of a joint, each onto its plate, the plates welded together
/// (one edit), the joint's old plates replaced.
#[test]
fn plates_are_welded_together() {
    use crate::connection_edits as ce;
    let mut doc: DocumentMut = PYLON.parse().unwrap();
    let t = read(&doc);
    let g1 = ce::add_plate(&mut doc, &t, "N2", 0.01);
    let t = read(&doc);
    let g2 = ce::add_plate(&mut doc, &t, "N2", 0.01);
    let t = read(&doc);
    let g3 = ce::add_plate(&mut doc, &t, "N3", 0.01);
    let t = read(&doc);
    ce::weld_pair(&mut doc, &t, &g2, &g1, true).unwrap();
    let t = read(&doc);
    assert_eq!(t.plates[1].welded_to, vec![g1.clone()]);
    assert_eq!(
        t.welded_assemblies("N2"),
        vec![vec![g1.clone(), g2.clone()]]
    );
    // Welded again from the other side: nothing more written.
    let t = read(&doc);
    ce::weld_pair(&mut doc, &t, &g1, &g2, true).unwrap();
    assert!(read(&doc).plates[0].welded_to.is_empty());
    // Plates of two joints are not welded.
    let t = read(&doc);
    assert!(ce::weld_pair(&mut doc, &t, &g1, &g3, true).is_err());
    let t = read(&doc);
    ce::weld_pair(&mut doc, &t, &g1, &g2, false).unwrap();
    let t = read(&doc);
    assert!(t.plates.iter().all(|p| p.welded_to.is_empty()));
    assert_eq!(t.welded_assemblies("N2").len(), 2);
    // The old kind: both welded plates read as one welded plate, written
    // as such on the first change (the kind dropped).
    let text = doc.to_string().replace(
        &format!("[[plate]]\nid = \"{g2}\""),
        &format!(
            "[[plate]]\nid = \"G9\"\njoint = \"N2\"\nthickness = \"8 mm\"\nkind = \"welded\"\n\n\
             [[plate]]\nid = \"{g2}\"\nkind = \"welded\""
        ),
    );
    let mut doc: DocumentMut = text.parse().unwrap();
    let t = read(&doc);
    assert!(
        t.welded_assemblies("N2")
            .iter()
            .any(|a| a.contains(&g2) && a.contains(&"G9".to_owned())),
        "{:?}",
        t.welded_assemblies("N2")
    );
    assert!(
        t.connection_notes()
            .iter()
            .any(|n| n.contains("kind = \"welded\""))
    );
    ce::weld_pair(&mut doc, &t, &g1, &g2, true).unwrap();
    let t = read(&doc);
    assert!(t.plates.iter().all(|p| p.kind.is_none()), "{doc}");
    assert_eq!(t.welded_assemblies("N2").len(), 1);
    // Removed: no longer named by the others.
    ce::remove_plate(&mut doc, "G9");
    let t = read(&doc);
    assert!(
        t.plates
            .iter()
            .all(|p| !p.welded_to.contains(&"G9".to_owned()))
    );
    assert!(
        t.connection_notes().is_empty(),
        "{:?}",
        t.connection_notes()
    );
    // A welded plate made: the joint's plates replaced, M1 and M2 each
    // onto its own, the second welded to the first.
    let t = read(&doc);
    let ids = ce::make_welded_plates(
        &mut doc,
        &t,
        "N2",
        &[vec!["M1".to_owned()], vec!["M2".to_owned()]],
        0.012,
    );
    let t = read(&doc);
    assert_eq!(ids.len(), 2);
    let here: Vec<_> = t.plates.iter().filter(|p| p.joint == "N2").collect();
    assert_eq!(here.len(), 2);
    assert_eq!(here[1].welded_to, vec![ids[0].clone()]);
    assert!((here[0].thickness.si() - 0.012).abs() < 1e-12);
    assert_eq!(
        t.connection("M1", "N2").unwrap().onto.as_ref(),
        Some(&ids[0])
    );
    assert_eq!(
        t.connection("M2", "N2").unwrap().onto.as_ref(),
        Some(&ids[1])
    );
    assert_eq!(t.welded_assemblies("N2"), vec![ids.clone()]);
    // The plate of N3 kept.
    assert!(t.plates.iter().any(|p| p.id == g3));
}

/// A member's roll (owner 2026-11-04: its correction angle about its
/// axis): written in degrees within ±180°, removed at 0, read back.
#[test]
fn a_member_roll_is_written_in_degrees() {
    let mut doc: DocumentMut = PYLON.parse().unwrap();
    crate::set_member_roll(&mut doc, "M2", std::f64::consts::FRAC_PI_2);
    let t = read(&doc);
    let m = t.members.iter().find(|m| m.id == "M2").unwrap();
    assert!(doc.to_string().contains("roll = \"90 deg\""), "{doc}");
    assert!((crate::member_roll(m) - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    // Three quarters on: −90°.
    crate::set_member_roll(&mut doc, "M2", 1.5 * std::f64::consts::PI);
    assert!(doc.to_string().contains("roll = \"-90 deg\""), "{doc}");
    crate::set_member_roll(&mut doc, "M2", 0.0);
    assert!(!doc.to_string().contains("roll"));
    let t = read(&doc);
    assert_eq!(crate::member_roll(&t.members[1]), 0.0);
}
