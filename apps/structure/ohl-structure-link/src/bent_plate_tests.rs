// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The bent plate of the automatic joint layout (J2b, 2026-11-19): an end
//! on a flange's inner face leaving over the heel onto a plate folded round
//! the angle's other leg — on an invented joint.

use std::collections::BTreeMap;

use ohl_structures::{LibraryItem, StructureType};

use crate::joint_layout::{BoltArea, LayoutSettings, Onto};
use crate::layout_run::{LayoutRecord, RunAsk, lay_out};
use crate::{SectionLibrary, joint_details};

/// The example library with an invented 100 × 10 angle.
pub(crate) fn library() -> SectionLibrary {
    SectionLibrary::parse(&format!(
        "{}\n[[section]]\nid = \"EA-100x10\"\nname = \"Equal angle 100 × 10 (invented)\"\n\
         shape = \"equal_angle\"\nh = \"100 mm\"\nt = \"10 mm\"\nr = \"12 mm\"\n\
         [[section]]\nid = \"EA-50x5\"\nname = \"Equal angle 50 × 5 (invented)\"\n\
         shape = \"equal_angle\"\nh = \"50 mm\"\nt = \"5 mm\"\nr = \"7 mm\"\n",
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../libraries/sections/example-sections.ohlsec"),
        )
        .unwrap()
    ))
    .unwrap()
}

/// A horizontal H1–H2 running through N2 (one leg pointing +y, the other
/// down, in the face y = 0) and a plan member P1 arriving from −y: flat on
/// the inner face of H's horizontal leg, it runs towards H's other leg.
pub(crate) const OVER: &str = r#"format = "ohl-structure"
version = 18
id = "B"
name = "bent"
kind = "lattice"

[[node]]
id = "N1"
point = ["0 m", "0 m", "3 m"]
support = "fixed"

[[node]]
id = "N2"
point = ["3 m", "0 m", "3 m"]

[[node]]
id = "N3"
point = ["6 m", "0 m", "3 m"]
support = "fixed"

[[node]]
id = "N4"
point = ["3 m", "-2 m", "3 m"]
support = "fixed"

[[member]]
id = "H1"
nodes = ["N1", "N2"]
section = "EA-100x10"
orientation = "y+z-"
design = { class = "bracing" }

[[member]]
id = "H2"
nodes = ["N2", "N3"]
section = "EA-100x10"
orientation = "y+z-"
design = { class = "bracing" }

[[member]]
id = "P1"
nodes = ["N4", "N2"]
section = "EA-61x7"
orientation = "x+z-"
design = { class = "redundant" }

[[connection]]
member = "P1"
joint = "N2"
face = "inside"
"#;

fn settings() -> LayoutSettings {
    LayoutSettings {
        bolt: Some(BoltArea::picked(None, 0.016, None)),
        ..Default::default()
    }
}

fn run(text: &str, s: &LayoutSettings) -> crate::LayoutRun {
    let lib = library();
    let bolts = Default::default();
    lay_out(
        text,
        &RunAsk {
            library: &lib,
            bolts: &bolts,
            settings: s,
            preset_fields: BTreeMap::new(),
            joints: Some(["N2".to_owned()].into()),
            replace_hand: false,
        },
    )
    .unwrap()
}

fn parse(text: &str) -> StructureType {
    StructureType::parse(std::path::Path::new("s.ohls"), text).unwrap()
}

#[test]
fn an_end_over_the_heel_is_left_without_bent_plates() {
    let s = LayoutSettings {
        bent_plates: false,
        ..settings()
    };
    let r = run(OVER, &s);
    let n2 = r.joints.iter().find(|j| j.joint == "N2").unwrap();
    assert!(
        n2.hand
            .iter()
            .any(|h| h.starts_with("P1:") && h.contains("runs towards the angle's other leg")),
        "{:?}",
        n2.hand
    );
}

#[test]
fn an_end_over_the_heel_goes_onto_a_bent_plate_round_the_other_leg() {
    let r = run(OVER, &settings());
    let n2 = r.joints.iter().find(|j| j.joint == "N2").unwrap();
    assert!(n2.written && n2.hand.is_empty(), "{:?}", n2.hand);
    let p = &n2.plan;
    // Two flats: the one on H's other leg (bolted to it), the one in P1's
    // plane bent from it.
    assert_eq!(p.plates.len(), 2, "{p:?}");
    let flat = &p.plates[0];
    let bent = &p.plates[1];
    assert!(flat.members.is_empty() && flat.outline.is_some());
    let hold = flat.hold.as_ref().expect("its own bolts onto H");
    assert!(
        hold.member.starts_with('H') && hold.points.len() == 2,
        "{hold:?}"
    );
    let own = bent.own.as_ref().expect("in P1's plane");
    assert_eq!(own.welded_to, 0);
    let b = own.bend.as_ref().expect("bent, not welded");
    assert!((b.angle - 90.0).abs() < 0.01, "{b:?}");
    assert!(
        (b.inner_radius - 0.014).abs() < 1e-9,
        "twice the 7 mm plate: {b:?}"
    );
    // The bend runs along H (the flange's line).
    let along = [
        b.line[1][0] - b.line[0][0],
        b.line[1][1] - b.line[0][1],
        b.line[1][2] - b.line[0][2],
    ];
    assert!(along[1].abs() < 1e-9 && along[2].abs() < 1e-9, "{along:?}");
    assert!((bent.thickness - 0.007).abs() < 1e-12);
    assert_eq!(own.out_more, 0.0);
    let e = p.ends.iter().find(|e| e.member == "P1").unwrap();
    assert_eq!(e.onto, Onto::Plate(1));
    // P1's steel past the bend: the plate, its radius and half the gap
    // beyond H's other leg, its bolts their edge distance more.
    assert!(e.cut.unwrap() > 0.007 + 0.014 + 0.005, "{e:?}");

    // Written: the bend and the holding bolts, the file at version 22.
    let t = parse(&r.text);
    assert!(r.text.contains("version = 22"), "{}", &r.text[..60]);
    let pairs = t.bent_pairs("N2");
    assert_eq!(pairs.len(), 1, "{pairs:?}");
    let flat_id = &pairs[0].1;
    let lb = t
        .plates
        .iter()
        .find(|q| &q.id == flat_id)
        .and_then(|q| q.leg_bolts.as_ref())
        .expect("the flat on H bolted to it");
    assert!(lb.member.starts_with('H'));
    let rec = LayoutRecord::of(&t);
    assert!(
        rec.provisional
            .contains(&(flat_id.clone(), "N2".to_owned())),
        "{:?}",
        rec.provisional
    );
    // The joint as written: one part of two flats, no weld, P1 on the bent
    // flat, nothing laid out running into anything.
    let d = joint_details(&r.text, &library(), &Default::default()).unwrap();
    let j = d.joint("N2").unwrap();
    let bentd = j.plates.iter().find(|q| q.bend.is_some()).unwrap();
    assert_eq!(bentd.assembly.len(), 2);
    assert!(j.plates.iter().all(|q| q.welds.is_empty()));
    assert!(bentd.members.contains(&"P1".to_owned()));
    assert!(
        bentd
            .notes
            .iter()
            .any(|n| n.contains("NOT ON THE SAFE SIDE")),
        "{:?}",
        bentd.notes
    );
    let ours: Vec<String> = j
        .clashes
        .iter()
        .filter(|c| {
            [&c.a, &c.b].iter().any(|x| match x {
                crate::ClashPart::Plate(_) => true,
                other => other.member() == Some("P1"),
            })
        })
        .map(|c| c.words())
        .collect();
    assert!(ours.is_empty(), "{ours:?}");
    // P1 drawn on the bent flat, where it lay on H's inner face.
    let p1 = j.arrivals.iter().find(|a| a.member == "P1").unwrap();
    assert!(!p1.misfit, "{:?}", p1.notes);

    // A run again changes nothing.
    let again = run(&r.text, &settings());
    assert_eq!(again.text, r.text);
}

#[test]
fn a_narrow_other_leg_leaves_no_room_for_the_bolts_holding_the_plate() {
    // H as narrow as P1: no room for two bolts on its other leg beyond the
    // bend — a hand detail, said, never forced.
    let text = OVER
        .replace("EA-100x10", "EA-50x5")
        .replace("orientation = \"y+z-\"", "orientation = \"z-y+\"");
    let d = joint_details(&text, &library(), &Default::default()).unwrap();
    assert!(d.level3, "{:?}", d.notes);
    let r = run(&text, &settings());
    let n2 = r.joints.iter().find(|j| j.joint == "N2").unwrap();
    assert!(
        n2.hand
            .iter()
            .any(|h| h.starts_with("P1:") && h.contains("a bent plate round that leg does not fit")),
        "{:?}",
        n2.plan
    );
    assert!(parse(&r.text).plates.iter().all(|p| p.bend.is_none()));
}

/// The estimated eccentricities (O45) take the bent plate where the plan
/// lays one out: the end estimated (its offset the bent plate's, nil — it
/// lies where it lay on the flange's inner face), not an upper bound;
/// without bent plates it was one.
#[test]
fn the_estimates_take_the_bent_plate_instead_of_an_upper_bound() {
    use crate::eccentricities::{EstimateAsk, Source, estimate};
    let lib = library();
    let bolts = Default::default();
    let e = estimate(
        OVER,
        &EstimateAsk {
            library: &lib,
            bolts: &bolts,
            presets: &[],
        },
    )
    .unwrap();
    let p1 = e
        .ends
        .iter()
        .find(|x| x.member == "P1" && x.joint == "N2")
        .expect("P1 estimated");
    assert_eq!(p1.source, Source::Estimated, "{p1:?}");
    assert_eq!(p1.plate, 0.0, "{p1:?}");
    assert_eq!(p1.shift, 0.0, "{p1:?}");
    assert_eq!(e.count(Source::UpperBound), 0, "{:?}", e.ends);
}

/// The bill of materials weighs a bent plate from its developed area: by
/// hand, a 7 mm plate bent 90° at 14 mm inside radius is shorter by
/// (14 + 3.5)(2 tan 45° − π/2) = 7.511 mm per metre of bend than its two
/// flats taken to the bend line.
#[test]
fn a_bent_plate_is_weighed_from_its_developed_area() {
    let s = crate::quantities::bend_shortening(0.014, 0.007, 90.0);
    assert!((s - 0.0175 * (2.0 - std::f64::consts::FRAC_PI_2)).abs() < 1e-12);
    assert!((s * 1000.0 - 7.511).abs() < 1e-3, "{s}");
    let r = run(OVER, &settings());
    let bom =
        crate::quantities::structure_quantities(&r.text, &library(), &Default::default()).unwrap();
    let bent: Vec<_> = bom
        .parts
        .iter()
        .filter(|p| p.item.contains("bent from"))
        .collect();
    assert_eq!(bent.len(), 1, "{:?}", bom.parts);
    let t = parse(&r.text);
    let flat = t.plates.iter().find(|p| p.bend.is_some()).unwrap();
    let b = flat.bend.as_ref().unwrap();
    let l: Vec<[f64; 3]> = b.line.iter().map(|q| q.map(|v| v.si())).collect();
    let len =
        ((l[1][0] - l[0][0]).powi(2) + (l[1][1] - l[0][1]).powi(2) + (l[1][2] - l[0][2]).powi(2))
            .sqrt();
    let outline: Vec<[f64; 3]> = flat.outline.iter().map(|q| q.map(|v| v.si())).collect();
    let whole = ohl_bom::structure::polygon_area(&outline);
    assert!(
        (bent[0].quantity - (whole - len * s)).abs() < 1e-9,
        "{} {} {}",
        bent[0].quantity,
        whole,
        len
    );
}

/// The bend's own bending checked by both code plugins (C12, the owner's
/// decision of 2026-11-19): the joint as laid out, P1 pulling 20 kN — a row
/// "plate …/…, its bend" with a usage by ASCE 10-97 Eq. 3.13-1 or SIA 263
/// 4.3.5.4 (6), its section the bend's length by the 7 mm plate; twice the
/// force, twice the usage (linear in it: the moments are the force's).
/// Without a design code's check: said not checked, NOT ON THE SAFE SIDE.
#[test]
fn the_bend_is_checked_by_the_code_plugins() {
    let built = |name: &str| {
        let wasm = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../../plugins/codes/{name}/target/wasm32-wasip2/release/{name}.wasm"
        ));
        if wasm.is_file() {
            return true;
        }
        assert!(
            std::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
            "plugins/codes/{name} not built"
        );
        false
    };
    let laid = run(OVER, &settings()).text;
    let lib = library();
    for (code, capacity, clause) in [
        ("asce10", "", "3.13-1"),
        (
            "sia263",
            "\n[analysis]\ncapacity = \"sia263-2013\"\n",
            "4.3.5.4",
        ),
    ] {
        if !built(code) {
            return;
        }
        let text = format!("{laid}{capacity}");
        let plans = crate::plate_plans(&text, &lib, &Default::default()).unwrap();
        let f = |p1: f64| -> BTreeMap<String, f64> {
            [("P1", p1), ("H1", -50e3), ("H2", -45e3)]
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v))
                .collect()
        };
        let (one, two) = (f(20e3), f(40e3));
        let rows = crate::plate_usage_cases(&plans, &[("one", 1.0, &one), ("two", 1.0, &two)]);
        let bend = |n: usize| {
            rows[n]
                .iter()
                .find(|x| x.part == crate::PlatePart::Bend)
                .cloned()
                .unwrap_or_else(|| panic!("{code}: {:?}", rows[n]))
        };
        let (a, b) = (bend(0), bend(1));
        assert!(a.what().ends_with(", its bend"), "{}", a.what());
        let (ua, ub) = (
            a.usage.unwrap_or_else(|| panic!("{code}: {a:?}")),
            b.usage.unwrap(),
        );
        assert!(
            ua > 0.0 && (ub - 2.0 * ua).abs() < 1e-9 * ub.max(1.0),
            "{code}: {ua} {ub}"
        );
        assert!(a.governs.contains(clause), "{code}: {}", a.governs);
        assert!(!a.governs.contains("NOT CHECKED"), "{code}: {}", a.governs);
    }
    // The simple check (not a design code): said, no usage.
    let text = format!("{laid}\n[analysis]\ncapacity = \"simple\"\n");
    let plans = crate::plate_plans(&text, &lib, &Default::default()).unwrap();
    let none = BTreeMap::new();
    let rows = crate::plate_usage_cases(&plans, &[("one", 1.0, &none)]);
    let b = rows[0]
        .iter()
        .find(|x| x.part == crate::PlatePart::Bend)
        .unwrap_or_else(|| panic!("{:?}", rows[0]));
    assert!(
        b.usage.is_none() && b.governs.contains("NOT ON THE SAFE SIDE"),
        "{b:?}"
    );
}
