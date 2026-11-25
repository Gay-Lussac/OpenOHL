// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Leg splices proposed (O44 J7, part a) on an invented joint: a leg in two
//! pieces of other sections (C1 an L 61 × 7, C2 an L 61 × 9) through a
//! joint J with a brace D bolting onto them. Proposed, never written by a
//! run; confirmed in one edit; the brace then bolted through the outer
//! cover; provisional bolts where none are given; the same section with
//! bolts proposed one part (the owner's rule); reasons against.

use std::collections::BTreeMap;

use ohl_structures::{LibraryItem, StructureType};

use crate::joint_layout::{BoltArea, LayoutSettings, Onto};
use crate::joints::joint_details;
use crate::layout_run::{LayoutRecord, RunAsk, lay_out};
use crate::layout_run_tests::in_line;
use crate::leg_splices::confirm_text;

fn parse(text: &str) -> StructureType {
    StructureType::parse(std::path::Path::new("s.ohls"), text).unwrap()
}

/// The example sections and invented angles: L 61 × 9, L 100 × 10 and
/// L 100 × 12.
fn library() -> crate::SectionLibrary {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../libraries/sections/example-sections.ohlsec");
    let mut text = std::fs::read_to_string(path).unwrap();
    for (id, h, t, r) in [
        ("EA-61x9", 61, 9, 7),
        ("EA-150x12", 150, 12, 16),
        ("EA-150x15", 150, 15, 16),
    ] {
        text.push_str(&format!(
            "\n[[section]]\nid = \"{id}\"\nname = \"Equal angle {h} × {t} (invented)\"\n\
             shape = \"equal_angle\"\nh = \"{h} mm\"\nt = \"{t} mm\"\nr = \"{r} mm\"\n"
        ));
    }
    crate::SectionLibrary::parse(&text).unwrap()
}

fn picked() -> BoltArea {
    BoltArea::picked(None, 0.016, None)
}

fn run(text: &str) -> crate::LayoutRun {
    let lib = library();
    let bolts = Default::default();
    let s = LayoutSettings {
        bolt: Some(picked()),
        ..Default::default()
    };
    let ask = RunAsk {
        library: &lib,
        bolts: &bolts,
        settings: &s,
        preset_fields: BTreeMap::new(),
        joints: None,
        replace_hand: true,
    };
    lay_out(text, &ask).unwrap()
}

/// The leg C1–C2 of `upper` above J (C1 an L 61 × 7), both of class `leg`,
/// with `bolts` in their design data; the brace D bolting onto them.
fn leg(upper: &str, bolts: &str) -> String {
    in_line(upper, bolts).replacen(
        "design = { class = \"bracing\"",
        "design = { class = \"leg\"",
        2,
    )
}

const BOLTS: &str = ", bolts = { diameter = \"16 mm\", count = 4 }";
/// Two bolts a piece: the splice's bolts near the joint leave room for the
/// brace beyond them.
const TWO: &str = ", bolts = { diameter = \"16 mm\", count = 2 }";

#[test]
fn a_change_of_section_in_a_leg_is_proposed_spliced_and_never_written_by_a_run() {
    let text = leg("EA-61x9", BOLTS);
    let r = run(&text);
    assert_eq!(r.splices.len(), 1, "{:?}", r.splices);
    let s = &r.splices[0];
    assert!(s.proposed(), "{s:?}");
    assert_eq!(
        (s.joint.as_str(), s.member.as_str(), s.onto.as_str()),
        ("J", "C1", "C2")
    );
    // The thicker piece's leg, the import's bolts (none provisional).
    assert!((s.thickness - 0.009).abs() < 1e-12, "{s:?}");
    assert!(!s.covers.is_empty());
    assert!(s.provisional.is_empty());
    assert!(s.assumed().contains("9 mm"), "{}", s.assumed());
    // Never written by the run (never silently).
    assert!(parse(&r.text).splices.is_empty(), "{}", r.text);
    // Confirmed: a splice, both ends onto it, its covers outer and inner.
    let done = confirm_text(&text, &r.splices, Some(&picked())).unwrap();
    let t = parse(&done);
    assert_eq!(t.splices.len(), 1, "{:?}", t.splices);
    let sp = &t.splices[0];
    assert_eq!(sp.members, ["C1", "C2"]);
    assert_eq!(sp.covers.len(), 2 * s.covers.len());
    for m in ["C1", "C2"] {
        assert_eq!(
            t.connection(m, "J").unwrap().onto.as_deref(),
            Some(sp.id.as_str())
        );
    }
    // Not the layout's: a run (replacing values given by hand) keeps it,
    // proposes nothing more, and a run again changes nothing.
    let r2 = run(&done);
    let t2 = parse(&r2.text);
    assert_eq!(t2.splices.len(), 1, "{}", r2.text);
    assert_eq!(
        t2.connection("C1", "J").unwrap().onto.as_deref(),
        Some(sp.id.as_str())
    );
    assert!(r2.splices.is_empty(), "{:?}", r2.splices);
    assert!(!LayoutRecord::of(&t2).made.contains_key(&sp.id));
    assert_eq!(run(&r2.text).text, r2.text);
    // Two shear planes per bolt of the pieces (outer and inner cover).
    let d = joint_details(&r2.text, &library(), &Default::default()).unwrap();
    let j = d.joint("J").unwrap();
    for m in ["C1", "C2"] {
        let a = j.arrivals.iter().find(|a| a.member == m).unwrap();
        assert_eq!(a.planes.0.value, Some(2), "{m}: {:?}", a.planes);
    }
}

/// Once the splice is confirmed, the brace D bolts through the outer cover
/// onto the leg (the owner's "bracing bolted through the outer splice
/// cover"): seated on the cover, its bolts clamping the cover, the leg and
/// the inner cover; the cover drawn round its bolts and checked for its
/// whole force; nothing written runs into anything; a run again is the
/// same.
#[test]
fn the_brace_bolts_through_the_outer_cover_once_the_splice_is_confirmed() {
    let text = leg("EA-150x15", TWO)
        .replacen("section = \"EA-61x7\"", "section = \"EA-150x12\"", 1)
        .replace(
            "point = [\"2.5 m\", \"0 m\", \"0 m\"]",
            "point = [\"1.2 m\", \"0 m\", \"6 m\"]",
        );
    let r = run(&text);
    let done = confirm_text(&text, &r.splices, Some(&picked())).unwrap();
    let r2 = run(&done);
    let o = r2.joints.iter().find(|j| j.joint == "J").unwrap();
    let e = o.plan.ends.iter().find(|e| e.member == "D");
    assert!(
        e.is_some_and(|e| matches!(&e.onto, Onto::Member(m) if m == "C1" || m == "C2")),
        "{:?} {:?}",
        o.plan,
        o.hand
    );
    assert!(o.written, "{:?}", o.hand);
    let d = joint_details(&r2.text, &library(), &Default::default()).unwrap();
    let j = d.joint("J").unwrap();
    let a = j.arrivals.iter().find(|a| a.member == "D").unwrap();
    let (sid, _) = a.through.clone().expect("through the outer cover");
    assert_eq!(sid, parse(&done).splices[0].id);
    // Its grip: its own leg (7 mm), then the outer cover (15), the leg's
    // (12 or 15) and the inner cover (15).
    let host_t = if a.onto.value.as_deref() == Some("C1") {
        0.012
    } else {
        0.015
    };
    assert!((a.grip[0] - 0.007).abs() < 1e-9, "{:?}", a.grip);
    assert!(
        (a.grip[1] - (0.015 + host_t + 0.015)).abs() < 1e-9,
        "{:?}",
        a.grip
    );
    assert!(j.clashes.is_empty(), "{:?}", j.clashes);
    // The outer cover's checks take D's whole force.
    let plans =
        crate::plate_checks::plate_plans_in(&r2.text, &library(), &Default::default(), None)
            .unwrap();
    let forces: BTreeMap<String, f64> = [("D".to_owned(), 50e3)].into();
    let rows = crate::plate_usage_cases(&plans, &[("one", 1.0, &forces)]).remove(0);
    let d_on_cover: Vec<_> = rows
        .iter()
        .filter(|p| p.part == crate::PlatePart::Member("D".into()) && p.plate.contains("outer"))
        .collect();
    assert!(!d_on_cover.is_empty(), "{rows:?}");
    for p in &d_on_cover {
        assert!((p.force - 50e3).abs() < 1e-6, "{p:?}");
        assert!(
            p.notes.iter().any(|n| n.contains("whole force")),
            "{:?}",
            p.notes
        );
        // The packing, said on the row (2026-11-25).
        assert!(
            p.notes
                .iter()
                .any(|n| n.starts_with("NOT ON THE SAFE SIDE") && n.contains("is a packing")),
            "{:?}",
            p.notes
        );
    }
    assert_eq!(run(&r2.text).text, r2.text);
    // (2026-11-25) Seated on the 15 mm outer cover: with the joint details
    // taken, the cover's thickness is an offset of unknown sign at D's end
    // in J, along its bolts — its moment N·t in D's own check, whole.
    let mut model = ohl_structures_fe::Model::from_structure_type_with_joint_details(
        &r2.text,
        &library(),
        &Default::default(),
        None,
    )
    .unwrap();
    let note =
        crate::eccentricities::cover_offsets(&mut model, &r2.text, &library(), &Default::default())
            .expect("said");
    assert!(
        note.starts_with("1 member end(s)") && note.contains("15 mm"),
        "{note}"
    );
    let idx = model.members.iter().position(|m| m.id == "D").unwrap();
    let ends = model.eccentric_ends(idx);
    assert_eq!(ends.len(), 1, "{ends:?}");
    assert!((ends[0].free_e() - 0.015).abs() < 1e-12, "{ends:?}");
    let axis = a.bolt_axis;
    let r = ends[0].free[0];
    let along = (r[0] * axis[0] + r[1] * axis[1] + r[2] * axis[2]).abs();
    assert!((along - 0.015).abs() < 1e-9, "{r:?} {axis:?}");
    // Its moment in D's own check: N·t about axes square to it, 50 kN ×
    // 15 mm = 0.75 kN m in size.
    let m = ohl_structures_fe::offsets::uncarried_moments(&model, idx, 50e3).expect("taken");
    assert!(m[0] + m[1] >= 750.0 - 1e-6, "{m:?}");
    // Not taken where the joint details are not (level 3): nothing.
    let mut plain = ohl_structures_fe::Model::from_structure_type_with_bolts(
        &r2.text,
        &library(),
        &Default::default(),
        None,
    )
    .unwrap();
    if !plain.joint_use.taken() {
        assert!(
            crate::eccentricities::cover_offsets(
                &mut plain,
                &r2.text,
                &library(),
                &Default::default()
            )
            .is_none()
        );
    }
}

/// Pieces without bolts: provisional bolts written when confirmed, listed
/// so (the tower stays level 3) and kept by a run again.
#[test]
fn pieces_without_bolts_get_provisional_bolts_when_confirmed() {
    let text = leg("EA-61x9", "");
    let r = run(&text);
    let s = &r.splices[0];
    assert!(s.proposed(), "{s:?}");
    assert_eq!(s.provisional, ["C1", "C2"]);
    assert!(s.assumed().contains("provisional"), "{}", s.assumed());
    let done = confirm_text(&text, &r.splices, Some(&picked())).unwrap();
    let t = parse(&done);
    for m in ["C1", "C2"] {
        assert_eq!(
            t.connection(m, "J").unwrap().bolts.as_ref().unwrap().count,
            2
        );
    }
    assert_eq!(
        t.provisional_bolts().len(),
        2,
        "{:?}",
        t.provisional_bolts()
    );
    let again = parse(&run(&done).text);
    assert_eq!(again.provisional_bolts().len(), 2);
    assert!(again.connection("C1", "J").unwrap().bolts.is_some());
}

/// The same section with bolts (a leg): one part by the owner's rule —
/// J9's pieces in line, flagged a leg splice place; not proposed spliced.
#[test]
fn the_same_section_of_a_leg_with_bolts_is_proposed_one_part() {
    let text = leg("EA-61x7", BOLTS);
    let r = run(&text);
    assert!(r.splices.is_empty(), "{:?}", r.splices);
    assert_eq!(r.in_line.len(), 1, "{:?}", r.in_line);
    let x = &r.in_line[0];
    assert!(x.proposed() && x.splice_bolts, "{x:?}");
}

/// Not proposed, said why: bolts given by hand at an end; a splice of
/// angles turned apart.
#[test]
fn reasons_against_a_splice_are_said() {
    let given = format!(
        "{}\n[[connection]]\nmember = \"C2\"\njoint = \"J\"\n\
         bolts = {{ count = 3, diameter = \"16 mm\" }}\n",
        leg("EA-61x9", BOLTS)
    );
    let r = run(&given);
    let why = r.splices[0].not.as_deref().expect("not proposed");
    assert!(why.contains("given by hand"), "{why}");
    let turned = leg("EA-61x9", BOLTS).replacen(
        "orientation = \"x+y+\"",
        "orientation = \"x+y+\"\nroll = \"30 deg\"",
        1,
    );
    let r = run(&turned);
    let why = r.splices[0].not.as_deref().expect("not proposed");
    assert!(why.contains("not along each other"), "{why}");
    assert_eq!(
        confirm_text(&turned, &r.splices, Some(&picked())).unwrap(),
        turned
    );
}
