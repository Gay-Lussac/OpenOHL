// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The estimated eccentricities (O45 A1–A2) on invented joints: the option
//! in the record, the planner's shift estimated where a run would write
//! it, the values in the file taken as written, the upper bound where no
//! layout fits (with a welded plate's offset where one would be needed),
//! the same estimates every time, and the offsets those of the level 4
//! analysis for a shift in the file.

use std::collections::BTreeMap;

use ohl_structures::{LibraryItem, StructureType};
use ohl_structures_fe::Model;

use crate::eccentricities::{
    EstimateAsk, Estimates, Source, estimate, offsets, option_on, set_option,
};
use crate::joint_layout::{BoltArea, LayoutSettings};
use crate::joint_layout_tests::K;
use crate::joints_tests::library;
use crate::layout_run::{LayoutRecord, RunAsk, lay_out};

fn parse(text: &str) -> StructureType {
    StructureType::parse(std::path::Path::new("s.ohls"), text).unwrap()
}

fn estimates(text: &str) -> Estimates {
    let lib = library();
    let bolts = Default::default();
    estimate(
        text,
        &EstimateAsk {
            library: &lib,
            bolts: &bolts,
            presets: &[],
        },
    )
    .unwrap()
}

/// The layout run with the settings the estimates take on a file without
/// a record (the starting values, the starting bolt).
fn laid_out(text: &str) -> crate::LayoutRun {
    let lib = library();
    let bolts = Default::default();
    let s = LayoutSettings {
        bolt: Some(BoltArea::picked(None, 0.016, None)),
        ..Default::default()
    };
    lay_out(
        text,
        &RunAsk {
            library: &lib,
            bolts: &bolts,
            settings: &s,
            preset_fields: BTreeMap::new(),
            joints: None,
            replace_hand: false,
        },
    )
    .unwrap()
}

/// A shallow redundant at N2 beside the K-brace's diagonals.
const SHALLOW: &str = r#"
[[node]]
id = "N6"
point = ["2.5 m", "0 m", "0.13 m"]
support = "fixed"

[[member]]
id = "R1"
nodes = ["N6", "N2"]
section = "EA-61x7"
orientation = "y-z+"
design = { class = "redundant" }
"#;

/// D1 turned `deg` off its face, both its ends held by horizontals (no
/// roll lays it flat at both): a welded plate in its own plane, or a hand
/// detail.
fn d1_spoiled(deg: f64) -> String {
    format!(
        "{}\n[[node]]\nid = \"N6\"\npoint = [\"5 m\", \"0 m\", \"0 m\"]\nsupport = \"fixed\"\n\
         \n[[member]]\nid = \"H0\"\nnodes = [\"N1\", \"N4\"]\nsection = \"EA-61x7\"\n\
         design = {{ class = \"bracing\" }}\n\
         \n[[member]]\nid = \"H1\"\nnodes = [\"N4\", \"N6\"]\nsection = \"EA-61x7\"\n\
         design = {{ class = \"bracing\" }}\n",
        K.replace(
            "orientation = \"x+y+\"\n",
            &format!("orientation = \"x+y+\"\nroll = \"{deg} deg\"\n")
        )
    )
}

#[test]
fn the_option_lives_in_the_record_and_off_leaves_the_file_as_before() {
    let mut doc: toml_edit::DocumentMut = K.parse().unwrap();
    assert!(!option_on(&parse(K)));
    set_option(&mut doc, true);
    let on = doc.to_string();
    let t = parse(&on);
    assert!(option_on(&t));
    let rec = LayoutRecord::of(&t);
    assert!(rec.estimated && rec.is_empty() && rec.settings.is_none());
    // The body generators do not see it; the joints read as before.
    assert!(ohl_structure_edit::body::generators(&t).unwrap().is_empty());
    // Written back by the record (a layout run keeps it).
    let mut again: toml_edit::DocumentMut = on.parse().unwrap();
    rec.write(&mut again);
    assert!(option_on(&parse(&again.to_string())));
    // Off: the file as it was.
    set_option(&mut doc, false);
    assert_eq!(doc.to_string(), K);
    // Off in a laid-out file: only the key goes.
    let r = laid_out(K);
    let mut d: toml_edit::DocumentMut = r.text.parse().unwrap();
    set_option(&mut d, true);
    set_option(&mut d, false);
    assert_eq!(d.to_string(), r.text);
}

/// The planner's shift is estimated where a run would write it; once laid
/// out, the values written are taken; the estimates are the same every
/// time.
#[test]
fn the_planned_shift_is_estimated_and_the_laid_out_one_taken() {
    let text = format!("{K}{SHALLOW}");
    let e = estimates(&text);
    let r = laid_out(&text);
    let t = parse(&r.text);
    let n2: Vec<_> = e.ends.iter().filter(|x| x.joint == "N2").collect();
    assert!(!n2.is_empty(), "{e:?}");
    let n2_run = r.joints.iter().find(|o| o.joint == "N2").unwrap();
    for x in &n2 {
        let written = t
            .connection(&x.member, "N2")
            .and_then(|c| c.shift)
            .map_or(0.0, |s| s.si());
        match x.source {
            // As the run writes it (no clash round changes it here).
            Source::Estimated => {
                assert!((x.shift - written).abs() < 1e-9, "{x:?} vs {written}");
            }
            // Left by the planner (R1, in line with D1 on one plate): the
            // run leaves it too, the shift limit its bound, why said.
            Source::UpperBound => {
                assert!(n2_run.plan.hand_members.contains(&x.member), "{x:?}");
                assert_eq!(x.shift, crate::SHIFT_LIMIT_START);
                assert!(
                    x.why.as_deref().is_some_and(|w| w.contains(&x.member)),
                    "{x:?}"
                );
            }
            _ => panic!("{x:?}"),
        }
    }
    assert!(n2.iter().any(|x| x.source == Source::Estimated));
    assert_eq!(estimates(&text).ends, e.ends, "deterministic");
    // Laid out: the values as written.
    let laid = estimates(&r.text);
    for x in laid
        .ends
        .iter()
        .filter(|x| x.joint == "N2" && x.source != Source::UpperBound)
    {
        assert_eq!(x.source, Source::LaidOut, "{x:?}");
        let written = t
            .connection(&x.member, "N2")
            .and_then(|c| c.shift)
            .map_or(0.0, |s| s.si());
        assert_eq!(x.shift, written);
    }
}

/// No layout fits D1 (a welded plate would move it too far out): the
/// upper bound — the shift limit and the plate's offset at its limit, its
/// leg's thickness — said why.
#[test]
fn where_no_layout_fits_the_upper_bound_with_the_plate_case() {
    let e = estimates(&d1_spoiled(30.0));
    let d1 = e
        .ends
        .iter()
        .find(|x| x.member == "D1" && x.joint == "N2")
        .unwrap_or_else(|| panic!("{e:?}"));
    assert_eq!(d1.source, Source::UpperBound, "{d1:?}");
    assert_eq!(d1.shift, crate::SHIFT_LIMIT_START);
    assert!((d1.plate - 0.007).abs() < 1e-9, "{d1:?}");
    assert!(
        d1.why.as_deref().is_some_and(|w| w.contains("farther out")),
        "{d1:?}"
    );
    assert!(d1.either_sign());
    assert!(e.plate_bounds() >= 1);
    // In the model: both ways, its size the shift and the plate's offset
    // together, across the member.
    let text = d1_spoiled(30.0);
    let m = Model::from_structure_type_with(&text, &library()).unwrap();
    let (plus, _) = offsets(&m, &e, 1.0);
    let (minus, _) = offsets(&m, &e, -1.0);
    let k = m.members.iter().position(|x| x.id == "D1").unwrap();
    let p = plus.iter().find(|o| o.member == k).unwrap();
    let q = minus.iter().find(|o| o.member == k).unwrap();
    let size = |r: [f64; 3]| r.iter().map(|x| x * x).sum::<f64>().sqrt();
    let expect = (0.030f64.powi(2) + 0.007f64.powi(2)).sqrt();
    assert!((size(p.offset) - expect).abs() < 1e-9, "{p:?}");
    assert!((0..3).all(|c| (p.offset[c] + q.offset[c]).abs() < 1e-12));
    let a = m.nodes[m.members[k].i].point;
    let b = m.nodes[m.members[k].j].point;
    let along: f64 = (0..3).map(|c| (b[c] - a[c]) * p.offset[c]).sum();
    assert!(along.abs() < 1e-9, "across the member");
}

/// A shift in the file goes into the model as the level 4 analysis takes
/// it from the connection.
#[test]
fn a_shift_in_the_file_is_offset_as_level_4_takes_it() {
    let text = format!(
        "{K}\n[[connection]]\nmember = \"D1\"\njoint = \"N2\"\nonto = \"L1\"\nshift = \"15 mm\"\n"
    );
    let e = estimates(&text);
    let d1 = e
        .ends
        .iter()
        .find(|x| x.member == "D1" && x.joint == "N2")
        .unwrap();
    assert_eq!((d1.source, d1.shift), (Source::Given, 0.015));
    let lib = library();
    let simple = Model::from_structure_type_with(&text, &lib).unwrap();
    assert!(!simple.has_offsets(), "level 3: no offsets of its own");
    let level4 =
        Model::from_structure_type_with_joint_details(&text, &lib, &Default::default(), None)
            .unwrap();
    let k = level4.members.iter().position(|x| x.id == "D1").unwrap();
    let end = usize::from(level4.nodes[level4.members[k].j].id == "N2");
    let (plus, _) = offsets(&simple, &e, 1.0);
    let o = plus.iter().find(|o| o.member == k && o.end == end).unwrap();
    for c in 0..3 {
        assert!(
            (o.offset[c] - level4.members[k].offsets[end][c]).abs() < 1e-12,
            "{:?} vs {:?}",
            o.offset,
            level4.members[k].offsets[end]
        );
    }
}
