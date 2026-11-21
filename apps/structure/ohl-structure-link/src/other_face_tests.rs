// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! L10 on invented joints: members on either side of a leg's flange, bolts
//! that join nothing, a plate drawn in its flange's plane; and two of the
//! tests L8 could not write (L9's question 8): the bolts already there far
//! from the origin, a short plate's own bolts.

use std::collections::BTreeMap;

use crate::joint_layout::{BoltArea, LayoutSettings};
use crate::joints_tests::library;
use crate::layout_run::{RunAsk, lay_out};

fn settings() -> LayoutSettings {
    LayoutSettings {
        bolt: Some(BoltArea::picked(None, 0.016, None)),
        ..Default::default()
    }
}

fn run(text: &str, s: &LayoutSettings, joints: Option<&[&str]>) -> crate::LayoutRun {
    run_in(&library(), text, s, joints)
}

fn run_in(
    lib: &crate::SectionLibrary,
    text: &str,
    s: &LayoutSettings,
    joints: Option<&[&str]>,
) -> crate::LayoutRun {
    let bolts = Default::default();
    let ask = RunAsk {
        library: lib,
        bolts: &bolts,
        settings: s,
        preset_fields: BTreeMap::new(),
        joints: joints.map(|j| j.iter().map(|x| (*x).to_owned()).collect()),
        replace_hand: false,
    };
    lay_out(text, &ask).unwrap()
}

/// The K-brace, its diagonals with `n` bolts of 16 mm each.
fn k(n: usize) -> String {
    crate::joint_layout_tests::K.replace(
        "design = { class = \"bracing\" }",
        &format!(
            "design = {{ class = \"bracing\", bolts = {{ diameter = \"16 mm\", count = {n} }} }}"
        ),
    )
}

pub(crate) fn lib_wide() -> crate::SectionLibrary {
    crate::SectionLibrary::parse(&format!(
        "{}\n[[section]]\nid = \"EA-150x13\"\nname = \"Equal angle 150 × 13 (invented)\"\n\
         shape = \"equal_angle\"\nh = \"150 mm\"\nt = \"13 mm\"\nr = \"16 mm\"\n\
         \n[[section]]\nid = \"EA-65x5\"\nname = \"Equal angle 65 × 5 (invented)\"\n\
         shape = \"equal_angle\"\nh = \"65 mm\"\nt = \"5 mm\"\nr = \"7 mm\"\n",
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../libraries/sections/example-sections.ohlsec"),
        )
        .unwrap()
    ))
    .unwrap()
}

/// The invented joint: a leg 150 × 13 running through N2 (one part),
/// diagonals 65 × 5 from (`a`, 0, 0) and (`a`, 0, 6) with `n` bolts, and
/// (`h`) a horizontal to (`a`, 0, 3).
pub(crate) fn face_joint(a: f64, n: usize, h: bool) -> String {
    let mut t = crate::joint_layout_tests::K
        .replace("section = \"EA-61x7\"\norientation = \"x+y+\"", "section = \"EA-150x13\"\norientation = \"x+y+\"")
        .replace("section = \"EA-61x7\"", "section = \"EA-65x5\"")
        .replace("point = [\"2.5 m\", \"0 m\", \"0 m\"]", &format!("point = [\"{a} m\", \"0 m\", \"0 m\"]"))
        .replace("point = [\"2.5 m\", \"0 m\", \"6 m\"]", &format!("point = [\"{a} m\", \"0 m\", \"6 m\"]"))
        .replace(
            "design = { class = \"bracing\" }",
            &format!("design = {{ class = \"bracing\", bolts = {{ diameter = \"16 mm\", count = {n} }} }}"),
        );
    t.push_str(
        "\n[[connection]]\nmember = \"L1\"\njoint = \"N2\"\nkind = \"continuous\"\nonto = \"L2\"\n",
    );
    if h {
        t.push_str(&format!(
            "\n[[node]]\nid = \"N6\"\npoint = [\"{a} m\", \"0 m\", \"3 m\"]\nsupport = \"fixed\"\n\
             \n[[member]]\nid = \"H1\"\nnodes = [\"N2\", \"N6\"]\nsection = \"EA-65x5\"\n\
             orientation = \"y-z-\"\ndesign = {{ class = \"bracing\", bolts = {{ diameter = \"16 mm\", count = {n} }} }}\n"
        ));
    }
    t
}

/// L10 (the owner's photographs, 2026-11-21): the horizontal H1 and the two
/// diagonals all on the outer face of the leg's flange need a plate; with
/// the diagonals half a turn onto the flange's inner face (their leg then
/// nearer the work point across the flange), all three bolt directly — no
/// plate. Kept, said with each end; a run again changes nothing; without
/// the step, the plate as before.
#[test]
fn the_diagonals_go_onto_the_inner_face_where_that_saves_the_plate() {
    let lib = lib_wide();
    let text = face_joint(1.5, 2, true);
    let off = LayoutSettings {
        other_face: false,
        ..settings()
    };
    let r0 = run_in(&lib, &text, &off, Some(&["N2"]));
    let n0 = r0.joints.iter().find(|o| o.joint == "N2").unwrap();
    assert!(n0.written && n0.hand.is_empty(), "{:?}", n0.hand);
    assert_eq!(
        (n0.plan.ends.len(), n0.plan.plates.len()),
        (3, 1),
        "{:?}",
        n0.plan
    );
    let r = run_in(&lib, &text, &settings(), Some(&["N2"]));
    let n2 = r.joints.iter().find(|o| o.joint == "N2").unwrap();
    assert!(n2.written && n2.hand.is_empty(), "{:?}", n2.hand);
    assert_eq!(
        (n2.plan.ends.len(), n2.plan.plates.len()),
        (3, 0),
        "{:?}",
        n2.plan
    );
    for m in ["D1", "D2"] {
        assert!(
            r.notes.iter().any(|x| x
                .starts_with(&format!("{m}: half a turn onto the inner face of L"))
                && x.contains("(L10, members on either side of a leg's flange)")
                && x.contains("no farther")),
            "{:?}",
            r.notes
        );
    }
    let d = crate::joints::joint_details(&r.text, &lib, &Default::default()).unwrap();
    let j = d.joint("N2").unwrap();
    let face = |m: &str| {
        j.arrivals
            .iter()
            .find(|a| a.member == m)
            .unwrap()
            .sits
            .value
    };
    assert_eq!(face("D1"), Some(crate::Face::Inside));
    assert_eq!(face("D2"), Some(crate::Face::Inside));
    assert_eq!(face("H1"), Some(crate::Face::Outside));
    // Nothing runs into anything there.
    assert!(
        j.clashes.is_empty(),
        "{:?}",
        j.clashes.iter().map(|c| c.words()).collect::<Vec<_>>()
    );
    // A run again changes nothing.
    let again = run_in(&lib, &r.text, &settings(), Some(&["N2"]));
    assert_eq!(again.text, r.text);
}

/// `text` with every node moved by (dx, dy, dz) metres.
fn moved(text: &str, d: [f64; 3]) -> String {
    let mut out = String::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("point = [") {
            let v: Vec<f64> = rest
                .trim_end_matches(']')
                .split(',')
                .map(|x| {
                    x.trim()
                        .trim_matches('"')
                        .trim_end_matches(" m")
                        .parse()
                        .unwrap()
                })
                .collect();
            out.push_str(&format!(
                "point = [\"{} m\", \"{} m\", \"{} m\"]\n",
                v[0] + d[0],
                v[1] + d[1],
                v[2] + d[2]
            ));
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// L10 (the owner's screenshot of 2026-11-21: two bolts at a laid-out
/// joint joining nothing): the K-brace's upright in two pieces in line of
/// bracing, not one part, the lower one with bolts on both legs in its
/// design data — drawn at the joint, bolting onto nothing. The joint is laid out (the diagonals) but
/// not whole: L1 by hand, said; without the rule, whole.
#[test]
fn bolts_that_join_nothing_leave_the_joint_not_whole() {
    // A horizontal in two pieces of bracing in line (no leg, nothing
    // upright at the joint), H1 with 4 bolts on both legs in its design
    // data; two diagonals from one side.
    let text = crate::joint_layout_tests::SPLIT
        .replace(
            "[[node]]\nid = \"T\"\npoint = [\"0 m\", \"0 m\", \"1.5 m\"]\nsupport = \"fixed\"\n",
            "[[node]]\nid = \"E\"\npoint = [\"1.5 m\", \"0 m\", \"1.5 m\"]\nsupport = \"fixed\"\n\n\
             [[node]]\nid = \"F\"\npoint = [\"1.5 m\", \"0 m\", \"-1.5 m\"]\nsupport = \"fixed\"\n",
        )
        .replace(
            "[[member]]\nid = \"V\"\nnodes = [\"M\", \"T\"]\nsection = \"EA-61x7\"\norientation = \"y-x+\"\ndesign = { class = \"redundant\" }\n",
            "[[member]]\nid = \"D1\"\nnodes = [\"F\", \"M\"]\nsection = \"EA-61x7\"\norientation = \"y-z+\"\n\
             design = { class = \"bracing\" }\n\n[[member]]\nid = \"D2\"\nnodes = [\"E\", \"M\"]\n\
             section = \"EA-61x7\"\norientation = \"z-y-\"\ndesign = { class = \"bracing\" }\n",
        )
        .replacen(
            "design = { class = \"bracing\", bolts = { diameter = \"16 mm\", count = 2 } }",
            "design = { class = \"bracing\", connected_leg = \"both\", bolts = { diameter = \"16 mm\", count = 4 } }",
            1,
        )
        .replacen(
            "design = { class = \"bracing\", bolts = { diameter = \"16 mm\", count = 2 } }",
            "design = { class = \"bracing\", bolts = { diameter = \"16 mm\", count = 0 } }",
            1,
        );
    assert!(text.contains("\"D2\"") && !text.contains("\"V\""));
    let d = crate::joints::joint_details(&text, &library(), &Default::default()).unwrap();
    assert!(d.level3, "{:?}", d.notes);
    let l1 = d
        .joint("M")
        .unwrap()
        .arrivals
        .iter()
        .find(|a| a.member == "H1")
        .unwrap()
        .clone();
    assert!(l1.onto.value.is_none(), "{l1:?}");
    assert!(
        !l1.bolt_points.is_empty() && !l1.other_points.is_empty(),
        "{l1:?}"
    );
    let r = run(&text, &settings(), Some(&["M"]));
    let n2 = r.joints.iter().find(|j| j.joint == "M").unwrap();
    assert!(n2.written && !n2.plan.ends.is_empty(), "{:?}", n2.plan);
    assert!(
        n2.hand.iter().any(|h| h.starts_with("H1: ")
            && h.contains(crate::joint_layout::BOLTS_JOIN_NOTHING)
            && h.contains("4 bolt(s)")
            && h.contains("2 on each leg")),
        "{:?}",
        n2.hand
    );
    let off = LayoutSettings {
        nothing_joined: false,
        ..settings()
    };
    let r0 = run(&text, &off, Some(&["M"]));
    let n2 = r0.joints.iter().find(|j| j.joint == "M").unwrap();
    assert!(n2.written && n2.hand.is_empty(), "{:?}", n2.hand);
}

/// L10 (L9's question 6): a plate on a leg's flange whose members lie
/// within 2° of that flange is drawn in the flange's plane (before: in its
/// first member's), and an end's own nut on its own plate touches it.
#[test]
fn a_plate_lies_in_its_flanges_plane_and_its_ends_nuts_touch_it() {
    // D1 turned 1.5° off the flange (given by hand, kept): within 2°.
    let text = k(4).replacen(
        "orientation = \"y-z+\"\n",
        "orientation = \"y-z+\"\nroll = \"1.5 deg\"\n",
        1,
    );
    assert!(text.contains("roll = \"1.5 deg\""));
    let r = run(&text, &settings(), Some(&["N2"]));
    let n2 = r.joints.iter().find(|j| j.joint == "N2").unwrap();
    assert!(n2.written && n2.hand.is_empty(), "{:?}", n2.hand);
    assert_eq!(n2.plates.len(), 1, "{:?}", n2.plan);
    let d = crate::joints::joint_details(&r.text, &library(), &Default::default()).unwrap();
    let j = d.joint("N2").unwrap();
    let pl = j.plates.iter().find(|p| p.id == n2.plates[0]).unwrap();
    let n = crate::plane_normal(&pl.outline).unwrap();
    // L2's flange in the face y = 0: its normal along y.
    assert!(n[1].abs() > (0.01f64).to_radians().cos(), "{n:?}");
    // D1 lies 1.5° off it: its own bolts' heads and nuts on the plate
    // touch it, they do not clash.
    let d1 = j.arrivals.iter().find(|a| a.member == "D1").unwrap();
    assert!(
        d1.flat.is_some_and(|f| f.off > 1.0 && f.off < 2.0),
        "{:?}",
        d1.flat
    );
    let own: Vec<String> = j
        .clashes
        .iter()
        .filter(|c| {
            c.involves_plate(&pl.id)
                && [&c.a, &c.b].iter().any(|x| {
                    matches!(x, crate::ClashPart::Bolt { member, .. } if member == "D1" || member == "D2")
                })
        })
        .map(|c| c.words())
        .collect();
    assert!(own.is_empty(), "{own:?}");
}

/// L8's fix (the bolts already there on a face found wherever the
/// structure lies), the test L8 could not write (L9's question 8): the
/// K-brace's lower leg lapped onto the upper one with two bolts of its own
/// at N2 (not laid out: in line); D1 keeps clear of their heads — pulled
/// back further than without them — and the same with the whole tower far
/// from the structure's origin (before L8 the bolts were placed with the
/// joint's position added twice there, and missed).
#[test]
fn the_bolts_already_there_are_kept_clear_of_far_from_the_origin() {
    use crate::joint_layout::plan_joint;
    let lapped = format!(
        "{}\n[[connection]]\nmember = \"L1\"\njoint = \"N2\"\nonto = \"L2\"\n\
         bolts = {{ count = 2, diameter = \"16 mm\" }}\n",
        crate::joint_layout_tests::K
    );
    let cut = |t: &str, m: &str| {
        let d = crate::joints::joint_details(t, &library(), &Default::default()).unwrap();
        let j = d.joint("N2").unwrap();
        assert_eq!(
            j.arrivals
                .iter()
                .find(|a| a.member == "L1")
                .unwrap()
                .bolt_points
                .len(),
            usize::from(t.contains("bolts = { count = 2")) * 2
        );
        let p = plan_joint(j, &settings(), &|_, _| None);
        p.ends
            .iter()
            .find(|e| e.member == m)
            .and_then(|e| e.cut)
            .unwrap()
    };
    let plain = cut(crate::joint_layout_tests::K, "D1");
    let near = cut(&lapped, "D1");
    let far = cut(&moved(&lapped, [1000.0, 2000.0, 30.0]), "D1");
    assert!(near > plain + 0.002, "{plain} {near}");
    assert!((far - near).abs() < 1e-9, "{near} {far}");
}

/// L9's short holds (the owner's answer to L6's question 2), the test L8
/// and L9 could not write (L9's question 8): the wide-leg K-brace's two
/// diagonals with four bolts each go onto a plate on the leg; a channel
/// beside the work point (no single angle: left as it is) makes it stop
/// short of the work point. With `short_holds` the short plate gets its
/// own provisional bolts onto the leg (its outline grown round them still
/// clear of the channel); without, none, said.
#[test]
fn a_plate_stopping_short_of_the_work_point_gets_its_own_bolts() {
    let (lib, base) = crate::joint_layout_tests::wide_main();
    let text = base.replacen(
        "bolts = { diameter = \"12 mm\", count = 2 }",
        "bolts = { diameter = \"16 mm\", count = 4 }",
        1,
    ) + "\n[[node]]\nid = \"N7\"\npoint = [\"-2 m\", \"0 m\", \"3 m\"]\nsupport = \"fixed\"\n\
         \n[[member]]\nid = \"C1\"\nnodes = [\"N2\", \"N7\"]\nsection = \"CH-157\"\n\
         design = { class = \"bracing\" }\n";
    let plate = |on: bool| {
        let s = LayoutSettings {
            short_holds: on,
            ..settings()
        };
        let r = run_in(&lib, &text, &s, Some(&["N2"]));
        let o = r.joints.iter().find(|o| o.joint == "N2").unwrap().clone();
        assert!(o.written && o.hand.is_empty(), "{:?}", o.hand);
        assert_eq!(o.plan.plates.len(), 1, "{:?}", o.plan);
        o
    };
    let off = plate(false);
    assert!(off.plan.plates[0].short, "{:?}", off.plan);
    assert!(off.plan.plates[0].hold.is_none(), "{:?}", off.plan);
    assert!(
        off.plan
            .notes
            .iter()
            .any(|n| n.contains("stopping short of the work point: no provisional bolts")),
        "{:?}",
        off.plan.notes
    );
    let on = plate(true);
    assert!(on.plan.plates[0].short, "{:?}", on.plan);
    let hold = on.plan.plates[0].hold.as_ref().expect("its own bolts");
    assert!(hold.member == "L1" || hold.member == "L2", "{hold:?}");
    assert!(hold.provisional && hold.points.len() >= 2, "{hold:?}");
}
