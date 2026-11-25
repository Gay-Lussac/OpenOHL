// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The detailing practice (O44 L24): the default is the layout as before
//! exactly, a practice changes the layout the way it says on invented
//! joints, the structure's own table wins over the plugin's value by value,
//! values out of range are refused with why (in the run's notes too), the
//! record keeps the plugin's answer, and a code plugin's practice is read
//! through the real host.

use toml::Table;

use crate::joint_layout::{LayoutSettings, NO_WELD};
use crate::layout_run::LayoutRecord;
use crate::layout_run_tests::{d1_spoiled, parse, run, settings};
use crate::practice::{
    ClassPractice, DetailingPractice, Fabrication, FacePattern, PluginAnswer, PracticeSource,
    RecordedPractice, in_force, with_given, with_practice,
};

fn recorded(p: DetailingPractice) -> RecordedPractice {
    RecordedPractice {
        source: PracticeSource::Project,
        practice: p,
        own: None,
        plugin: None,
    }
}

/// The five forms in the default order, those of `off` not allowed.
fn forms_without(off: &[Fabrication]) -> Vec<(Fabrication, bool)> {
    Fabrication::ALL
        .iter()
        .map(|f| (*f, !off.contains(f)))
        .collect()
}

fn table(text: &str) -> Table {
    toml::from_str(text).unwrap()
}

fn answer(text: &str) -> PluginAnswer {
    PluginAnswer {
        plugin: "invented-code".into(),
        method: "m".into(),
        table: table(text),
    }
}

/// A practice's text read, its refusal's words (it must be refused).
fn refused(text: &str) -> String {
    DetailingPractice::parse(text).expect_err(text)
}

/// The default practice is every constant the layout had, and applying it
/// leaves the engine's settings exactly as they were — for the defaults
/// and for settings with switches off (a practice only restricts).
#[test]
fn the_default_practice_is_the_layout_as_before() {
    let p = DetailingPractice::default();
    assert!(p.is_default());
    assert_eq!(p.gap, crate::GAP_START);
    assert_eq!(p.pull_back, crate::PULL_BACK_START);
    assert_eq!(p.shift, crate::SHIFT_LIMIT_START);
    assert_eq!(p.flatness_deg, crate::FLAT_OFF);
    assert_eq!(p.small_bend_deg, crate::BEND_SMALL_MAX);
    assert_eq!(p.own_plane_offset, crate::joint_layout::OWN_OUT_MAX);
    assert_eq!(p.bend_radius, crate::BEND_RADIUS_RATIO);
    assert_eq!(p.between_least, crate::joint_layout::BETWEEN_LEAST);
    assert_eq!(p.fabrication, forms_without(&[]));
    assert!(p.refusals().is_empty(), "{:?}", p.refusals());
    assert!(p.notes().is_empty(), "{:?}", p.notes());
    let off = LayoutSettings {
        small_bends: false,
        other_face: false,
        short_plates: false,
        welded: true,
        ..settings()
    };
    for s in [LayoutSettings::default(), settings(), off] {
        let with = with_practice(s.clone(), Some(&recorded(p.clone())));
        assert_eq!(
            LayoutSettings {
                practice: None,
                ..with.clone()
            },
            s
        );
        assert_eq!(with.practice, Some(recorded(p.clone())));
        assert_eq!(with_practice(s.clone(), None), s);
    }
    // An empty table, the structure's or a plugin's: the default.
    let empty = in_force(Some(&Table::new()), None).unwrap().unwrap();
    assert!(empty.practice.is_default());
    assert_eq!(in_force(None, None), Ok(None));
}

/// A practice written and read back is the same; lengths and angles read
/// in their units.
#[test]
fn a_practice_reads_back_as_written() {
    let p = DetailingPractice::default();
    assert_eq!(DetailingPractice::parse(&p.to_text()).unwrap(), p);
    assert_eq!(DetailingPractice::parse("").unwrap(), p);
    let mut fab = forms_without(&[Fabrication::Welded]);
    fab.swap(2, 4);
    let q = DetailingPractice {
        name: "invented".into(),
        fabrication: fab,
        face_pattern: FacePattern::LaterClassFirst,
        both_sides_of_flange: false,
        shift: 0.015,
        flatness_deg: 1.5,
        small_bend_deg: 12.0,
        bend_radius: 3.0,
        short_plates: false,
        bolt_count: 4,
        classes: [(
            "redundant".to_owned(),
            ClassPractice {
                bolt_count: Some(3),
            },
        )]
        .into(),
        ..p.clone()
    };
    assert_eq!(DetailingPractice::parse(&q.to_text()).unwrap(), q);
    assert!(
        q.notes().iter().any(|n| n.contains("no welding")),
        "{:?}",
        q.notes()
    );
    let x = DetailingPractice::parse(
        "[limits]\ngap = 0.012\nshift = \"20 mm\"\nflatness_deg = \"0.025 rad\"\n",
    )
    .unwrap();
    assert!((x.gap - 0.012).abs() < 1e-12 && (x.shift - 0.02).abs() < 1e-12);
    assert!((x.flatness_deg - 0.025f64.to_degrees()).abs() < 1e-9);
    // The fabrication list: a form not allowed as a table.
    let y = DetailingPractice::parse(
        "fabrication = [\"direct\", \"plate\", \"welded\", { form = \"bend\", allowed = false }, \
         \"bends\"]\n",
    )
    .unwrap();
    assert!(!y.makes(Fabrication::Bend) && y.makes(Fabrication::Welded));
    assert!(y.place(Fabrication::Welded) < y.place(Fabrication::Bend));
}

/// Every value out of its range is refused, each with what, why and what
/// to give instead; several at once are all said.
#[test]
fn values_out_of_range_are_refused_with_why() {
    let e = refused("[limits]\nflatness_deg = 3\n");
    assert!(
        e.contains("flatness_deg 3°") && e.contains("give 1° to 2°"),
        "{e}"
    );
    assert!(e.contains("forced fit"), "{e}");
    let e = refused("[limits]\nshift = \"50 mm\"\n");
    assert!(e.contains("give 0 to 30 mm") && e.contains("J5"), "{e}");
    let e = refused("[limits]\nown_plane_offset = \"60 mm\"\n");
    assert!(e.contains("give 0 to 40 mm"), "{e}");
    let e = refused("[limits]\ngap = \"1 mm\"\n");
    assert!(e.contains("clash check"), "{e}");
    let e = refused("[limits]\nflatness_deg = 1.5\nsmall_bend_deg = 1\n");
    assert!(e.contains("small_bend_deg 1°"), "{e}");
    let e = refused("[plates]\nbend_radius = 0.5\n");
    assert!(e.contains("crack"), "{e}");
    let e = refused("[plates]\nbetween_least = 0.25\n");
    assert!(e.contains("between_least"), "{e}");
    let e = refused("[bolts]\ncount = 1\n");
    assert!(e.contains("J8"), "{e}");
    let e = refused("[bolts]\nlines = 2\n");
    assert!(e.contains("give 1"), "{e}");
    // The fabrication list.
    let e = refused("fabrication = [\"direct\", \"plate\", \"bend\"]\n");
    assert!(
        e.contains("\"bends\" missing") && e.contains("\"welded\" missing"),
        "{e}"
    );
    let e = refused(
        "fabrication = [\"direct\", { form = \"plate\", allowed = false }, \"bend\", \"bends\", \
         \"welded\"]\n",
    );
    assert!(e.contains("\"plate\" not allowed"), "{e}");
    let e = refused("fabrication = [\"direct\", \"plate\", \"glued\"]\n");
    assert!(e.contains("\"glued\" is not a form"), "{e}");
    let e = refused(
        "fabrication = [\"direct\", \"plate\", \"plate\", \"bend\", \"bends\", \"welded\"]\n",
    );
    assert!(e.contains("given twice"), "{e}");
    // Per member class: only the picked bolt's pattern.
    let e = refused("[classes.bracing.limits]\nshift = \"10 mm\"\n");
    assert!(e.contains("not per member class yet"), "{e}");
    let e = refused("[classes.girder.bolts]\ncount = 3\n");
    assert!(e.contains("not a member class"), "{e}");
    let e = refused("[classes.redundant.bolts]\ncount = 1\n");
    assert!(e.contains("classes.redundant.bolts.count 1"), "{e}");
    // Keys and kinds.
    let e = refused("[limits]\nshfit = \"20 mm\"\n");
    assert!(e.contains("limits.shfit: not a key"), "{e}");
    let e = refused("[faces]\nboth_sides_of_flange = \"yes\"\n");
    assert!(e.contains("true or false"), "{e}");
    let e = refused("format = \"something-else\"\n");
    assert!(e.contains("format"), "{e}");
    let e = refused("version = 2\n");
    assert!(e.contains("version"), "{e}");
    let e = refused("[limits]\nshift = \"50 mm\"\nflatness_deg = 5\n");
    assert!(
        e.contains("limits.shift") && e.contains("limits.flatness_deg"),
        "{e}"
    );
    let e = refused("[limits]\nshift = \"far\"\n");
    assert!(e.contains("a length expected"), "{e}");
}

/// The structure's own table over the plugin's, value by value: its shift
/// replaces the plugin's alone, the plugin's other values stay; each value
/// says where it comes from; a refusal names whose table.
#[test]
fn the_structures_own_values_win_over_the_plugins_value_by_value() {
    let plugin = answer(
        "fabrication = [\"direct\", \"plate\", \"bend\", \"bends\", { form = \"welded\", \
         allowed = false }]\n[limits]\nshift = \"20 mm\"\nflatness_deg = 1.5\n",
    );
    let own = table("[limits]\nshift = \"10 mm\"\n");
    let p = in_force(Some(&own), Some(&plugin)).unwrap().unwrap();
    assert!((p.practice.shift - 0.010).abs() < 1e-12);
    assert!((p.practice.flatness_deg - 1.5).abs() < 1e-12);
    assert!(!p.practice.makes(Fabrication::Welded));
    assert!(matches!(p.source, PracticeSource::ProjectOverPlugin { .. }));
    assert_eq!(p.source_of(&["limits.shift"]), "this file");
    assert_eq!(p.source_of(&["limits.flatness_deg"]), "the code plugin");
    assert_eq!(p.source_of(&["plates.bend_radius"]), "the default");
    assert_eq!(p.source_of(&["fabrication"]), "the code plugin");
    // The plugin's alone, the file's alone.
    let q = in_force(None, Some(&plugin)).unwrap().unwrap();
    assert!(matches!(q.source, PracticeSource::Plugin { .. }));
    assert!((q.practice.shift - 0.020).abs() < 1e-12);
    let r = in_force(Some(&own), None).unwrap().unwrap();
    assert_eq!(r.source, PracticeSource::Project);
    // Refused: whose table.
    let bad = answer("[limits]\nflatness_deg = 4\n");
    let e = in_force(Some(&own), Some(&bad)).unwrap_err();
    assert!(
        e.contains("code plugin invented-code") && e.contains("flatness_deg 4°"),
        "{e}"
    );
    let e = in_force(Some(&table("[limits]\nshift = \"40 mm\"\n")), Some(&plugin)).unwrap_err();
    assert!(e.contains("structure file's [layout_practice]"), "{e}");
}

/// A practice that changes the layout changes it the way it says, on an
/// invented K-brace whose diagonal D1 lies 10° off its flange: by default on
/// a plate bent by as much (L3); bends not allowed, on a plate welded in its
/// own plane (step 4); neither allowed, left for a hand detail, said; the
/// welded plate before the bent one in the order (a tie: both fit), welded.
#[test]
fn the_fabrication_forms_and_their_order_change_the_layout_as_they_say() {
    let text = d1_spoiled(10.0);
    let d1 = |p: Option<DetailingPractice>| {
        let s = with_practice(settings(), p.map(recorded).as_ref());
        let r = run(&text, &s, Some(&["N2"]), false);
        let o = r.joints.iter().find(|j| j.joint == "N2").unwrap().clone();
        let end = o.plan.ends.iter().find(|e| e.member == "D1").cloned();
        (o, end)
    };
    let own_of = |o: &crate::JointOutcome, e: Option<crate::EndPlan>| {
        let crate::Onto::Plate(k) = e.expect("D1 laid out").onto else {
            panic!()
        };
        o.plan.plates[k].own.clone().expect("in its own plane")
    };
    // The default: bent.
    let (o, e) = d1(None);
    assert!(own_of(&o, e).bend.is_some());
    // The default practice given: the same plan.
    let (o2, _) = d1(Some(DetailingPractice::default()));
    assert_eq!(o2.plan, o.plan);
    // No bends: welded in its own plane.
    let (o, e) = d1(Some(DetailingPractice {
        fabrication: forms_without(&[Fabrication::Bend]),
        ..Default::default()
    }));
    let own = own_of(&o, e);
    assert!(own.bend.is_none() && own.weld_size > 0.0, "{own:?}");
    // Welded before bent in the order: welded (the tie decided by it).
    let mut fab = forms_without(&[]);
    fab.swap(2, 4);
    let (o, e) = d1(Some(DetailingPractice {
        fabrication: fab,
        ..Default::default()
    }));
    let own = own_of(&o, e);
    assert!(own.bend.is_none() && own.weld_size > 0.0, "{own:?}");
    // Neither: a hand detail, said.
    let (o, e) = d1(Some(DetailingPractice {
        fabrication: forms_without(&[Fabrication::Bend, Fabrication::Welded]),
        ..Default::default()
    }));
    assert!(e.is_none(), "{:?}", o.plan);
    assert!(
        o.hand
            .iter()
            .any(|h| h.starts_with("D1") && h.contains(NO_WELD)),
        "{:?}",
        o.hand
    );
}

/// The welded plate's axis offset beyond a plate's thickness (L20 item 2):
/// D1 30° off is laid out with its offset by default; a practice allowing
/// none leaves it for a hand detail, said (its axis farther out than a
/// plate's thickness).
#[test]
fn a_smaller_offset_limit_leaves_the_far_end_by_hand() {
    let text = d1_spoiled(30.0);
    let r = run(&text, &settings(), Some(&["N2"]), false);
    let o = r.joints.iter().find(|j| j.joint == "N2").unwrap();
    assert!(o.plan.ends.iter().any(|e| e.member == "D1"), "{:?}", o.hand);
    let p = DetailingPractice {
        own_plane_offset: 0.0,
        ..Default::default()
    };
    let s = with_practice(settings(), Some(&recorded(p)));
    let r = run(&text, &s, Some(&["N2"]), false);
    let o = r.joints.iter().find(|j| j.joint == "N2").unwrap();
    assert!(o.plan.ends.iter().all(|e| e.member != "D1"), "{:?}", o.plan);
    assert!(
        o.hand
            .iter()
            .any(|h| h.starts_with("D1") && h.contains("farther out of the flange")),
        "{:?}",
        o.hand
    );
}

/// The picked bolt's pattern: three bolts per end where an end has none;
/// per member class: the diagonals' class four.
#[test]
fn the_picked_bolts_pattern_gives_the_ends_their_bolts_per_class_too() {
    let text = d1_spoiled(0.0);
    let counts = |s: &LayoutSettings| {
        let r = run(&text, s, Some(&["N2"]), false);
        let o = r.joints.iter().find(|j| j.joint == "N2").unwrap().clone();
        o.plan
            .ends
            .iter()
            .filter_map(|e| e.bolts.as_ref().map(|b| (e.member.clone(), b.count)))
            .collect::<Vec<_>>()
    };
    let before = counts(&settings());
    assert!(
        !before.is_empty() && before.iter().all(|c| c.1 == 2),
        "{before:?}"
    );
    let three = DetailingPractice {
        bolt_count: 3,
        ..Default::default()
    };
    let after = counts(&with_practice(settings(), Some(&recorded(three.clone()))));
    assert_eq!(after.len(), before.len());
    assert!(after.iter().all(|c| c.1 == 3), "{after:?}");
    // The K-brace's diagonals are of class bracing.
    let per_class = DetailingPractice {
        classes: [(
            "bracing".to_owned(),
            ClassPractice {
                bolt_count: Some(4),
            },
        )]
        .into(),
        ..three
    };
    let by_class = counts(&with_practice(settings(), Some(&recorded(per_class))));
    assert!(by_class.iter().all(|c| c.1 == 4), "{by_class:?}");
}

/// A run records the code plugin's answer it took; read back, the file's
/// own table over it gives the practice for an edit's follow and the
/// estimates; a run again changes nothing; a refused practice is said in
/// the run's notes and the default used.
#[test]
fn the_record_keeps_the_plugins_answer_and_a_refusal_is_in_the_notes() {
    let plugin = answer("[limits]\nflatness_deg = 1.5\n");
    let p = in_force(None, Some(&plugin));
    let s = with_given(settings(), &p);
    let r = run(&d1_spoiled(10.0), &s, None, false);
    let t = parse(&r.text);
    let rec = LayoutRecord::of(&t);
    assert_eq!(rec.plugin_practice(), Some(plugin.clone()));
    assert!(r.text.contains("[generator.practice]"), "{}", r.text);
    assert!(
        r.text.contains("source = \"plugin:invented-code/m\""),
        "{}",
        r.text
    );
    assert_eq!(rec.practice_of(&t), p);
    assert_eq!(run(&r.text, &s, None, false).text, r.text);
    // With the file's own table too (structure file version 27).
    let own = format!("{}\n[layout_practice.limits]\nshift = \"10 mm\"\n", r.text);
    let t = parse(&own);
    let q = LayoutRecord::of(&t).practice_of(&t).unwrap().unwrap();
    assert!((q.practice.shift - 0.010).abs() < 1e-12);
    assert!((q.practice.flatness_deg - 1.5).abs() < 1e-12);
    // A run that takes no plugin's practice drops the record's.
    let r2 = run(&r.text, &settings(), None, false);
    assert_eq!(LayoutRecord::of(&parse(&r2.text)).practice, None);
    // Refused: the default used, said in the run's notes.
    let bad = in_force(Some(&table("[limits]\nflatness_deg = 4\n")), None);
    assert!(bad.is_err());
    let s = with_given(settings(), &bad);
    assert_eq!(
        LayoutSettings {
            practice_refused: None,
            ..s.clone()
        },
        settings()
    );
    let r = run(&d1_spoiled(10.0), &s, None, false);
    assert!(
        r.notes
            .first()
            .is_some_and(|n| n.contains("flatness_deg 4°") && n.contains("default")),
        "{:?}",
        r.notes
    );
}

/// A code plugin's practice through the real host: the example plugin's
/// (invented: no welding, plates reaching the work point, a 20 mm shift,
/// three bolts) read and checked; a structure whose method is the core's
/// simple check gives none.
#[test]
fn a_code_plugins_practice_is_read_and_checked() {
    let Some(dir) = crate::plate_checks_tests::with_plugin(
        "examples/member-check-example",
        "member-check-example",
    ) else {
        return;
    };
    let text = format!(
        "{}\n[analysis]\ncapacity = \"plugin:member-check-example/example-yield-euler\"\n",
        d1_spoiled(10.0)
    );
    let a = crate::practice::plugin_practice(&text, Some(&dir))
        .unwrap()
        .expect("the example's practice");
    assert_eq!(
        (a.plugin.as_str(), a.method.as_str()),
        ("member-check-example", "example-yield-euler")
    );
    let p = in_force(None, Some(&a)).unwrap().unwrap();
    let x = &p.practice;
    assert_eq!(x.name, "example practice (invented)");
    assert!(!x.makes(Fabrication::Welded) && x.makes(Fabrication::Bend));
    assert!((x.shift - 0.020).abs() < 1e-12);
    assert!(!x.short_plates);
    assert_eq!(x.bolt_count, 3);
    // The layout with it: no weld anywhere.
    let s = with_practice(settings(), Some(&p));
    assert!(!s.own_plane_welded && !s.welded && !s.short_plates);
    let r = run(&text, &s, Some(&["N2"]), false);
    let o = r.joints.iter().find(|j| j.joint == "N2").unwrap();
    assert!(
        o.plan
            .plates
            .iter()
            .all(|pl| pl.own.as_ref().is_none_or(|w| w.weld_size == 0.0))
    );
    // The core's simple check: no plugin, no practice.
    let simple = format!("{}\n[analysis]\ncapacity = \"simple\"\n", d1_spoiled(10.0));
    assert_eq!(
        crate::practice::plugin_practice(&simple, Some(&dir)),
        Ok(None)
    );
}
