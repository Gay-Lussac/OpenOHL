// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The detailing practice (O44 L24): the default is the layout as before
//! exactly, a practice changes the layout the way it says on invented
//! joints, values out of range are refused with why, the record keeps it,
//! and a code plugin's practice is read through the real host.

use crate::joint_layout::{LayoutSettings, NO_WELD};
use crate::layout_run::LayoutRecord;
use crate::layout_run_tests::{d1_spoiled, parse, run, settings};
use crate::practice::{
    DetailingPractice, Fabrication, FacePattern, PracticeSource, RecordedPractice, with_practice,
};

fn recorded(p: DetailingPractice) -> RecordedPractice {
    RecordedPractice {
        source: PracticeSource::Project,
        practice: p,
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
}

/// The default practice written and read back is the default; an empty
/// text and the record's own table are too.
#[test]
fn a_practice_reads_back_as_written() {
    let p = DetailingPractice::default();
    assert_eq!(DetailingPractice::parse(&p.to_text()).unwrap(), p);
    assert_eq!(DetailingPractice::parse("").unwrap(), p);
    let q = DetailingPractice {
        name: "invented".into(),
        fabrication: vec![Fabrication::Direct, Fabrication::Plate, Fabrication::Bends],
        face_pattern: FacePattern::LaterClassFirst,
        both_sides_of_flange: false,
        shift: 0.015,
        flatness_deg: 1.5,
        small_bend_deg: 12.0,
        bend_radius: 3.0,
        short_plates: false,
        bolt_count: 4,
        ..p.clone()
    };
    assert_eq!(DetailingPractice::parse(&q.to_text()).unwrap(), q);
    let r = RecordedPractice {
        source: PracticeSource::Plugin {
            plugin: "invented-code".into(),
            method: "m".into(),
        },
        practice: q.clone(),
    };
    assert_eq!(RecordedPractice::from_table(&r.to_table()).unwrap(), r);
    assert!(
        q.notes().iter().any(|n| n.contains("not built")),
        "{:?}",
        q.notes()
    );
    assert!(
        q.notes().iter().any(|n| n.contains("no welding")),
        "{:?}",
        q.notes()
    );
    // Units: a length as text or metres, an angle in degrees or as text.
    let x = DetailingPractice::parse(
        "[limits]\ngap = 0.012\nshift = \"20 mm\"\nflatness_deg = \"0.025 rad\"\n",
    )
    .unwrap();
    assert!((x.gap - 0.012).abs() < 1e-12 && (x.shift - 0.02).abs() < 1e-12);
    assert!((x.flatness_deg - 0.025f64.to_degrees()).abs() < 1e-9);
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
    // The fabrication order.
    let e = refused("fabrication = [\"direct\", \"bend\"]\n");
    assert!(e.contains("\"plate\" left out"), "{e}");
    let e = refused("fabrication = [\"plate\", \"direct\"]\n");
    assert!(e.contains("engine's order"), "{e}");
    let e = refused("fabrication = [\"direct\", \"plate\", \"glued\"]\n");
    assert!(e.contains("\"glued\" is not a kind"), "{e}");
    let e = refused("fabrication = [\"direct\", \"plate\", \"plate\"]\n");
    assert!(e.contains("given twice"), "{e}");
    // Keys and kinds.
    let e = refused("[limits]\nshfit = \"20 mm\"\n");
    assert!(e.contains("limits.shfit: not a key"), "{e}");
    let e = refused("[faces]\nboth_sides_of_flange = \"yes\"\n");
    assert!(e.contains("true or false"), "{e}");
    let e = refused("format = \"something-else\"\n");
    assert!(e.contains("format"), "{e}");
    let e = refused("version = 2\n");
    assert!(e.contains("version"), "{e}");
    // Several at once: all said.
    let e = refused("[limits]\nshift = \"50 mm\"\nflatness_deg = 5\n");
    assert!(
        e.contains("limits.shift") && e.contains("limits.flatness_deg"),
        "{e}"
    );
    // Not a number at all.
    let e = refused("[limits]\nshift = \"far\"\n");
    assert!(e.contains("a length expected"), "{e}");
}

/// A changed practice changes the layout the way it says, on an invented
/// K-brace whose diagonal D1 lies 10° off its flange: by default on a plate
/// bent by as much (L3); without "bend" on a plate welded in its own plane
/// (step 4); without "bend" and "welded" left for a hand detail, said.
#[test]
fn leaving_a_fabrication_kind_out_changes_the_layout_as_it_says() {
    let text = d1_spoiled(10.0);
    let d1 = |p: Option<DetailingPractice>| {
        let s = with_practice(settings(), p.map(recorded).as_ref());
        let r = run(&text, &s, Some(&["N2"]), false);
        let o = r.joints.iter().find(|j| j.joint == "N2").unwrap().clone();
        let end = o.plan.ends.iter().find(|e| e.member == "D1").cloned();
        (o, end)
    };
    // The default: bent.
    let (o, e) = d1(None);
    let crate::Onto::Plate(k) = e.expect("D1 laid out").onto else {
        panic!()
    };
    assert!(o.plan.plates[k].own.as_ref().unwrap().bend.is_some());
    // The default practice given: the same plan.
    let (o2, _) = d1(Some(DetailingPractice::default()));
    assert_eq!(o2.plan, o.plan);
    // No bends: welded in its own plane.
    let no_bend = DetailingPractice {
        fabrication: vec![Fabrication::Direct, Fabrication::Plate, Fabrication::Welded],
        ..Default::default()
    };
    let (o, e) = d1(Some(no_bend));
    let crate::Onto::Plate(k) = e.expect("D1 laid out").onto else {
        panic!()
    };
    let own = o.plan.plates[k].own.as_ref().unwrap();
    assert!(own.bend.is_none() && own.weld_size > 0.0, "{own:?}");
    // Neither: a hand detail, said.
    let neither = DetailingPractice {
        fabrication: vec![Fabrication::Direct, Fabrication::Plate],
        ..Default::default()
    };
    let (o, e) = d1(Some(neither));
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

/// The picked bolt's pattern: three bolts per end where an end has none.
#[test]
fn the_picked_bolts_pattern_gives_the_ends_their_bolts() {
    let text = d1_spoiled(0.0);
    let count = |s: &LayoutSettings| {
        let r = run(&text, s, Some(&["N2"]), false);
        let o = r.joints.iter().find(|j| j.joint == "N2").unwrap().clone();
        o.plan
            .ends
            .iter()
            .filter_map(|e| e.bolts.as_ref().map(|b| b.count))
            .collect::<Vec<_>>()
    };
    let before = count(&settings());
    assert!(
        !before.is_empty() && before.iter().all(|c| *c == 2),
        "{before:?}"
    );
    let p = DetailingPractice {
        bolt_count: 3,
        ..Default::default()
    };
    let after = count(&with_practice(settings(), Some(&recorded(p))));
    assert_eq!(after.len(), before.len());
    assert!(after.iter().all(|c| *c == 3), "{after:?}");
}

/// The record keeps the practice a run took, with its source; read back it
/// is the same, a run again changes nothing, and a run with the default
/// keeps the structure's own practice but drops a plugin's.
#[test]
fn the_record_keeps_the_practice_a_run_took() {
    let p = DetailingPractice {
        name: "invented shop".into(),
        flatness_deg: 1.5,
        ..Default::default()
    };
    let own = recorded(p.clone());
    let s = with_practice(settings(), Some(&own));
    let r = run(&d1_spoiled(10.0), &s, None, false);
    let rec = LayoutRecord::of(&parse(&r.text));
    assert_eq!(rec.practice().unwrap(), Some(own.clone()));
    assert_eq!(rec.own_practice(), Some(Ok(own.clone())));
    assert!(r.text.contains("[generator.practice]"), "{}", r.text);
    assert!(r.text.contains("source = \"project\""), "{}", r.text);
    assert_eq!(run(&r.text, &s, None, false).text, r.text);
    // A run with the default keeps the structure's own.
    let again = run(&r.text, &settings(), None, false);
    let rec2 = LayoutRecord::of(&parse(&again.text));
    assert_eq!(rec2.practice().unwrap(), Some(own));
    // A plugin's goes with a run that takes none.
    let theirs = RecordedPractice {
        source: PracticeSource::Plugin {
            plugin: "invented-code".into(),
            method: "m".into(),
        },
        practice: p,
    };
    let r = run(
        &d1_spoiled(10.0),
        &with_practice(settings(), Some(&theirs)),
        None,
        false,
    );
    assert_eq!(
        LayoutRecord::of(&parse(&r.text)).practice().unwrap(),
        Some(theirs)
    );
    let r = run(&r.text, &settings(), None, false);
    assert_eq!(LayoutRecord::of(&parse(&r.text)).practice, None);
    // One refused (written by hand out of range): kept as written, the
    // default used, said by `practice()`.
    let bad = format!(
        "{}\n[[generator]]\nid = \"joint-layout\"\nkind = \"joint_layout\"\n\n\
         [generator.practice]\nsource = \"project\"\n\n[generator.practice.limits]\n\
         flatness_deg = 4\n",
        d1_spoiled(10.0)
    );
    let rec = LayoutRecord::of(&parse(&bad));
    let e = rec.practice().unwrap_err();
    assert!(e.contains("flatness_deg 4°"), "{e}");
    let s = rec.with_practice(settings());
    assert_eq!(s, settings());
    let r = run(&bad, &settings(), None, false);
    assert!(r.text.contains("flatness_deg = 4"), "{}", r.text);
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
    let p = crate::practice::plugin_practice(&text, Some(&dir))
        .unwrap()
        .expect("the example's practice");
    assert_eq!(
        p.source,
        PracticeSource::Plugin {
            plugin: "member-check-example".into(),
            method: "example-yield-euler".into(),
        }
    );
    let x = &p.practice;
    assert_eq!(x.name, "example practice (invented)");
    assert!(!x.makes(Fabrication::Welded) && x.makes(Fabrication::Bend));
    assert!((x.shift - 0.020).abs() < 1e-12);
    assert!(!x.short_plates);
    assert_eq!(x.bolt_count, 3);
    // The layout with it: no weld anywhere, three bolts per end given.
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
