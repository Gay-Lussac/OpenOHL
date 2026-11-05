// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Lap joints, double covers and continuous members in the Joint details
//! panel (2026-11-05, stage 4), driven as a user would: the Face choice,
//! the splice menu and its covers, Design › Continuous members…; each one
//! undo step; the covers drawn in the close-up and as solids for the 3D
//! view.

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use ohl_structure_edit::Handle;
use ohl_structure_edit::connection_edits::FieldValue;
use ohl_structures::connections::{CoverFace, CoverLeg, Face};

use crate::app::StructureApp;
use crate::joint_tools::Act;
use crate::joint_ui_tests::ANGLES;
use crate::tests::{copy_of, harness, menu};

/// The angle tower in a folder of its own with the example sections, its
/// joint N2 in Joint details.
fn angles(tag: &str) -> (Harness<'static, StructureApp>, std::path::PathBuf) {
    let path = copy_of("type-02.ohls", tag);
    let dir = path.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(dir.join("Sections")).unwrap();
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/sections/example-sections.ohlsec"),
        dir.join("Sections/example-sections.ohlsec"),
    )
    .unwrap();
    let tower = dir.join("angles.ohls");
    std::fs::write(&tower, ANGLES).unwrap();
    let mut h = harness(&tower);
    menu(&mut h, "Design", "Joint details");
    h.state_mut().selected = vec![Handle::Joint("N2".into())];
    h.run();
    h.run();
    (h, dir)
}

fn file(h: &Harness<'static, StructureApp>) -> ohl_structures::StructureType {
    h.state().file.as_ref().unwrap().structure().clone()
}

fn details(h: &mut Harness<'static, StructureApp>) -> ohl_structure_link::JointDetails {
    let d = h.state_mut().joint_details().unwrap();
    d.as_ref().as_ref().unwrap().clone()
}

fn arrival(d: &ohl_structure_link::JointDetails, member: &str) -> ohl_structure_link::Arrival {
    d.joint("N2")
        .unwrap()
        .arrivals
        .iter()
        .find(|a| a.member == member)
        .unwrap()
        .clone()
}

/// The Face column (where an end sits on the member it bolts onto): shown
/// as worked out, given in one undo step; nested where it cannot nest
/// (the diagonal across the leg) is a misfit, said in red under the table.
#[test]
fn the_face_is_chosen_and_a_misfit_said() {
    let (mut h, dir) = angles("lap-face");
    h.get_by_label("Face");
    h.get_by_label("Planes");
    let d = details(&mut h);
    let a = arrival(&d, "D1");
    assert_eq!(a.sits.source, ohl_structure_link::Source::WorkedOut);
    // D1 bolted onto L1: one shear plane, worked out ("lapped onto the
    // member").
    assert_eq!(a.planes.0.value, Some(1));
    assert_eq!(a.planes.0.source, ohl_structure_link::Source::WorkedOut);
    // Given: the face it has (the same place, given now).
    let worked = a.sits.value.unwrap();
    h.state_mut().apply_joint_acts(
        vec![Act::Field {
            member: "D1".into(),
            joint: "N2".into(),
            field: "face",
            value: Some(FieldValue::Word(worked.as_str().into())),
        }],
        &d,
    );
    h.run();
    h.run();
    assert_eq!(file(&h).connection("D1", "N2").unwrap().face, Some(worked));
    let b = arrival(&details(&mut h), "D1");
    assert_eq!(b.offset, a.offset);
    assert!(!b.misfit);
    // Nested: D1 runs across L1, it cannot nest in it.
    let d = details(&mut h);
    h.state_mut().apply_joint_acts(
        vec![Act::Field {
            member: "D1".into(),
            joint: "N2".into(),
            field: "face",
            value: Some(FieldValue::Word(Face::Nested.as_str().into())),
        }],
        &d,
    );
    h.run();
    h.run();
    let b = arrival(&details(&mut h), "D1");
    assert!(b.misfit, "{:?}", b.notes);
    h.get_by_label_contains("not in line with it");
    // Undone in one step: the face worked out again.
    h.get_by_label("Undo").click();
    h.run();
    assert_eq!(file(&h).connection("D1", "N2").unwrap().face, Some(worked));
    std::fs::remove_dir_all(dir).ok();
}

/// The splice menu (+ Splice) stays open on a tick, the members in line ticked at
/// first; Add the splice: covers outer and inner on both legs (8 mm), both
/// ends onto it, two shear planes worked out — one undo step; a cover's
/// thickness typed and a cover removed, each one undo step; the covers
/// drawn in the close-up and as solids for the 3D view.
#[test]
fn a_splice_is_added_with_its_covers() {
    let (mut h, dir) = angles("lap-splice");
    h.get_by_label("+ Splice").scroll_to_me();
    h.run();
    h.get_by_label("+ Splice").click();
    h.run();
    // L1 and L2 (in line, the same section) ticked; a tick keeps it open.
    let ticked = |h: &Harness<'static, StructureApp>, name: &str| {
        h.get_all_by_role(eframe::egui::accesskit::Role::CheckBox)
            .find(|n| n.accesskit_node().label().is_some_and(|l| l == name))
            .map(|n| n.accesskit_node().toggled())
    };
    assert_eq!(
        ticked(&h, "L1"),
        Some(Some(eframe::egui::accesskit::Toggled::True))
    );
    assert_eq!(
        ticked(&h, "D1"),
        Some(Some(eframe::egui::accesskit::Toggled::False))
    );
    h.get_by_label("Add the splice").click();
    h.run();
    h.run();
    let t = file(&h);
    assert_eq!(t.splices.len(), 1);
    let s = &t.splices[0];
    assert_eq!(s.members, vec!["L1".to_owned(), "L2".to_owned()]);
    assert_eq!(s.covers.len(), 4);
    assert_eq!(s.shear_planes(Some(CoverLeg::Long)), 2);
    for m in ["L1", "L2"] {
        assert_eq!(
            t.connection(m, "N2").unwrap().onto.as_deref(),
            Some(s.id.as_str())
        );
    }
    let id = s.id.clone();
    h.get_by_label(&format!("Splice {id}"));
    let d = details(&mut h);
    let l1 = arrival(&d, "L1");
    assert_eq!(l1.planes.0.value, Some(2), "{:?}", l1.planes);
    assert_eq!(l1.planes.0.source, ohl_structure_link::Source::WorkedOut);
    // The covers drawn: in the close-up (green) and as solids.
    let pieces = crate::close_up::pieces(d.joint("N2").unwrap(), true);
    assert!(pieces.iter().any(|p| p.base == crate::close_up::COVER));
    let solids = ohl_structure_link::plate_solids(&d);
    assert_eq!(solids.iter().filter(|p| p.cover).count(), 4);
    // A cover 10 mm thick, then the inner short one removed.
    h.state_mut().apply_joint_acts(
        vec![Act::Cover(
            id.clone(),
            CoverFace::Outer,
            Some(CoverLeg::Long),
            Some(0.010),
        )],
        &d,
    );
    h.state_mut().apply_joint_acts(
        vec![Act::Cover(
            id.clone(),
            CoverFace::Inner,
            Some(CoverLeg::Short),
            None,
        )],
        &d,
    );
    h.run();
    let t = file(&h);
    assert_eq!(t.splices[0].covers.len(), 3);
    assert_eq!(t.splices[0].shear_planes(Some(CoverLeg::Short)), 1);
    assert!(
        t.splices[0]
            .covers
            .iter()
            .any(|c| (c.thickness.si() - 0.010).abs() < 1e-12)
    );
    // The missing cover offered again.
    h.get_by_label("+ inner short");
    // Undone: the cover back; again: the thickness; again: no splice.
    for _ in 0..3 {
        h.get_by_label("Undo").click();
        h.run();
    }
    assert!(file(&h).splices.is_empty());
    std::fs::remove_dir_all(dir).ok();
}

/// Design › Continuous members…: L1 and L2 (in line, the same section)
/// proposed, ticked; nothing written before Apply; Apply makes them one
/// part in one undo step (both ends); one solid through N2.
#[test]
fn the_continuous_command_proposes_and_applies() {
    let (mut h, dir) = angles("lap-continuous");
    menu(&mut h, "Design", "Continuous members…");
    h.run();
    h.get_by_label("L1 into L2 at N2");
    assert!(file(&h).continuous_with("L1", "N2").is_none());
    // Ticked at first but not one part yet: the change is the tick.
    let list = h.state().joint_tools.continuous.clone().unwrap();
    assert_eq!(list.len(), 1, "{list:?}");
    assert!(list[0].4 && !list[0].3);
    // Tick none: nothing to apply; tick all again.
    h.get_by_label("Tick none").click();
    h.run();
    h.get_by_label("Apply (0)");
    h.get_by_label("Tick all").click();
    h.run();
    h.get_by_label("Apply (1)").click();
    h.run();
    h.run();
    let t = file(&h);
    assert_eq!(t.continuous_with("L1", "N2"), Some("L2"));
    assert_eq!(t.continuous_with("L2", "N2"), Some("L1"));
    assert!(h.state().joint_tools.continuous.is_none());
    // One solid through the joint: no end face there.
    let d = details(&mut h);
    let solids = ohl_structure_link::member_solids(&d);
    let l1 = solids.iter().find(|s| s.member == "L1").unwrap();
    assert_eq!(l1.runs_on, [false, true]);
    // The Held cell says it.
    h.get_by_value("continuous into L2");
    // One undo step.
    h.get_by_label("Undo").click();
    h.run();
    assert!(file(&h).continuous_with("L1", "N2").is_none());
    std::fs::remove_dir_all(dir).ok();
}
