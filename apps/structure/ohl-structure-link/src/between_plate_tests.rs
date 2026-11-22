// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! O44 L15: a plate between a built-up member's angles, on an invented
//! joint — planned, written (`between`, structure file version 25), drawn
//! in the gap, its own bolts through both angles, nothing running into
//! anything, a run again identical.

use std::collections::BTreeMap;

use crate::joint_layout::{BoltArea, LayoutSettings};
use crate::joints::joint_details;
use crate::joints_tests::library;
use crate::layout_run::{RunAsk, lay_out};

/// An invented built-up diagonal (two angles back to back, a 9 mm gap) in
/// two pieces B1, B2 through joint N, one part there, and a redundant `R`
/// arriving at N from `far`, rolled `roll`°.
fn diagonal(far: [f64; 3], roll: f64) -> String {
    format!(
        r#"format = "ohl-structure"
version = 18
id = "B"
name = "b"
kind = "lattice"

[[node]]
id = "N0"
point = ["0 m", "0 m", "0 m"]
support = "fixed"

[[node]]
id = "N"
point = ["2 m", "0 m", "2 m"]

[[node]]
id = "N1"
point = ["4 m", "0 m", "4 m"]
support = "fixed"

[[node]]
id = "F"
point = ["{} m", "{} m", "{} m"]
support = "fixed"

[[member]]
id = "B1"
nodes = ["N0", "N"]
section = "2L-83x52x6"
design = {{ class = "bracing" }}

[[member]]
id = "B2"
nodes = ["N", "N1"]
section = "2L-83x52x6"
design = {{ class = "bracing" }}

[[member]]
id = "R"
nodes = ["N", "F"]
section = "EA-61x7"
roll = "{roll} deg"
design = {{ class = "redundant" }}

[[connection]]
member = "B1"
joint = "N"
kind = "continuous"
onto = "B2"
"#,
        far[0], far[1], far[2]
    )
}

fn settings() -> LayoutSettings {
    LayoutSettings {
        bolt: Some(BoltArea::picked(None, 0.016, None)),
        ..Default::default()
    }
}

fn run(text: &str, s: &LayoutSettings) -> crate::LayoutRun {
    let lib = library();
    let bolts = Default::default();
    let ask = RunAsk {
        library: &lib,
        bolts: &bolts,
        settings: s,
        preset_fields: BTreeMap::new(),
        joints: Some(["N".to_owned()].into()),
        replace_hand: false,
    };
    lay_out(text, &ask).unwrap()
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The joint with `R` in the plane of the gap between B's angles (along
/// B's axis and the backs), leaving towards the backs' toes: the roll of
/// the ones laying `R`'s leg bolted flat on the backs.
fn across_the_gap() -> Vec<String> {
    let probe = diagonal([2.0, 0.0, 0.0], 0.0);
    let d = joint_details(&probe, &library(), &Default::default()).unwrap();
    let b1 = d
        .joint("N")
        .unwrap()
        .arrivals
        .iter()
        .find(|a| a.member == "B1")
        .unwrap();
    let q = b1.profile.as_ref().unwrap();
    assert!(q.angle.is_none() && q.part_angles.len() == 2, "{q:?}");
    // The backs (the long legs, along the section's z) face along its y.
    let far = [0, 1, 2].map(|k| {
        let n = [2.0, 0.0, 2.0][k];
        ((n + 1.5 * q.axes[2][k] + 0.6 * q.axes[0][k]) * 1e4).round() / 1e4
    });
    let mut out = Vec::new();
    for roll in (0..720).map(|k| f64::from(k) * 0.5) {
        let text = diagonal(far, roll);
        let d = joint_details(&text, &library(), &Default::default()).unwrap();
        let r = d
            .joint("N")
            .unwrap()
            .arrivals
            .iter()
            .find(|a| a.member == "R")
            .unwrap();
        let p = r.profile.as_ref().unwrap();
        let nb = if r.leg_y { p.axes[2] } else { p.axes[1] };
        if dot(nb, q.axes[1]).abs() > 0.9999 {
            out.push(text);
        }
    }
    assert!(!out.is_empty());
    out
}

#[test]
fn an_end_across_the_gap_goes_onto_a_plate_between_the_angles() {
    for text in across_the_gap() {
        // Off: as before — no plate between the angles.
        let off = run(
            &text,
            &LayoutSettings {
                between_plates: false,
                ..settings()
            },
        );
        let o = off.joints.iter().find(|o| o.joint == "N").unwrap();
        assert!(o.plan.plates.iter().all(|p| !p.between), "{:?}", o.plan);
        assert!(!off.text.contains("between = true"));
        // On: R onto a plate between B's angles, as thick as the gap.
        let r = run(&text, &settings());
        let o = r.joints.iter().find(|o| o.joint == "N").unwrap();
        assert!(o.written && o.hand.is_empty(), "{:?} {:?}", o.hand, o.plan);
        let pl = o
            .plan
            .plates
            .iter()
            .find(|p| p.between)
            .expect("a plate between");
        assert!((pl.thickness - 0.009).abs() < 1e-9, "{}", pl.thickness);
        assert!(pl.on == "B1" || pl.on == "B2", "{}", pl.on);
        assert!(pl.members == ["R"], "{:?}", pl.members);
        assert!(pl.hold.is_some(), "{:?}", o.plan.notes);
        assert!(r.text.contains("between = true"), "{}", r.text);
        assert!(r.text.contains("version = 25"));
        // Drawn in the gap: R on the plate's far face (the other angle's
        // back), the plate touching both backs only; its own bolts through
        // both angles, two shear planes; nothing runs into anything.
        let d = joint_details(&r.text, &library(), &Default::default()).unwrap();
        let j = d.joint("N").unwrap();
        assert!(j.clashes.is_empty(), "{:?}", j.clashes);
        let pd = j.plates.iter().find(|p| p.between).expect("drawn between");
        let n = pd.through.unwrap();
        let host = j.arrivals.iter().find(|a| a.member == pl.on).unwrap();
        let q = host.profile.as_ref().unwrap();
        let backs: Vec<f64> = q
            .part_angles
            .iter()
            .map(|g| {
                let h: [f64; 3] = std::array::from_fn(|k| {
                    j.position[k]
                        + host.offset[k]
                        + q.axes[1][k] * g.heel[0]
                        + q.axes[2][k] * g.heel[1]
                });
                dot(h, n)
            })
            .collect();
        // (The outline written to a tenth of a millimetre.)
        let face: Vec<f64> = pd.outline.iter().map(|c| dot(*c, n)).collect();
        let mid = face.iter().sum::<f64>() / face.len() as f64;
        assert!(face.iter().all(|f| (f - mid).abs() < 1e-4), "{face:?}");
        let at = |v: f64| backs.iter().any(|b| (b - v).abs() < 2e-4);
        assert!(
            at(mid) && (at(mid + 0.009) || at(mid - 0.009)),
            "{face:?} {backs:?}"
        );
        let rr = j.arrivals.iter().find(|a| a.member == "R").unwrap();
        assert!(!rr.misfit, "{:?}", rr.notes);
        let lb = pd.leg_bolts.as_ref().expect("its own bolts");
        assert_eq!(lb.planes.0.value, Some(2));
        assert!((lb.grip[0] - 0.006).abs() < 1e-9 && (lb.grip[1] - 0.015).abs() < 1e-9);
        // A run again: the same file.
        let again = run(&r.text, &settings());
        assert_eq!(again.text, r.text);
    }
}

/// A gap thinner than half the thickest leg on the plate: no plate between
/// the angles.
#[test]
fn a_gap_too_thin_for_a_plate_between_is_not_laid_out() {
    let text = across_the_gap()[0].replace("section = \"EA-61x7\"", "section = \"EA-100x20\"");
    let lib = crate::SectionLibrary::parse(&format!(
        "{}\n[[section]]\nid = \"EA-100x20\"\nname = \"Equal angle 100 × 20 (invented)\"\n\
         shape = \"equal_angle\"\nh = \"100 mm\"\nt = \"20 mm\"\nr = \"12 mm\"\n",
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../libraries/sections/example-sections.ohlsec"),
        )
        .unwrap()
    ))
    .unwrap();
    let bolts = Default::default();
    let s = settings();
    let ask = RunAsk {
        library: &lib,
        bolts: &bolts,
        settings: &s,
        preset_fields: BTreeMap::new(),
        joints: Some(["N".to_owned()].into()),
        replace_hand: false,
    };
    let r = lay_out(&text, &ask).unwrap();
    assert!(!r.text.contains("between = true"));
}
