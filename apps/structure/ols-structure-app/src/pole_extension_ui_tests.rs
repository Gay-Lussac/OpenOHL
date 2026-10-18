// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The Pole and Extensions panels driven as a user would (egui_kittest):
//! a named pole brought into the file and its outline edited; an
//! extension added, its piece built, a leg made longer — each one undo
//! step.

use egui_kittest::kittest::Queryable;
use ols_structures::{Detail, PoleRef};

use crate::layout::Panel;
use crate::tests::{copy_of, harness, menu};

fn near(a: &[(f64, f64)], b: &[(f64, f64)]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| (x.0 - y.0).abs() < 1e-9 && (x.1 - y.1).abs() < 1e-9)
}

fn rings(h: &egui_kittest::Harness<'static, crate::app::StructureApp>) -> Vec<(f64, f64)> {
    h.state()
        .file
        .as_ref()
        .unwrap()
        .structure()
        .pole
        .as_ref()
        .and_then(PoleRef::inline)
        .map(|p| p.rings())
        .unwrap_or_default()
}

#[test]
fn a_pole_outline_is_edited() {
    let path = copy_of("type-05.ohls", "pole-ui");
    copy_of("steel-pole-30m.ohlp", "pole-ui");
    let mut h = harness(&path);
    // The named .ohlp: level 3.
    h.get_by_label("Level 3 — full design");
    menu(&mut h, "Body", "Pole…");
    assert_eq!(h.state_mut().focused(), Some(Panel::Pole));
    h.get_by_label("Bring it into this file").click();
    h.run();
    assert!(near(&rings(&h), &[(0.0, 0.7), (27.0, 0.3)]));
    h.get_by_label("Level 3 — full design");
    // A height half-way up: one undo step.
    h.get_by_label("+ Height").click();
    h.run();
    assert_eq!(rings(&h).len(), 3);
    assert!((rings(&h)[1].1 - 0.5).abs() < 1e-9);
    h.get_by_label("Undo").click();
    h.run();
    assert_eq!(rings(&h).len(), 2);
    h.get_by_label("Redo").click();
    h.run();
    // The middle height deleted again.
    h.get_all_by_label("🗑").nth(1).unwrap().click();
    h.run();
    assert!(near(&rings(&h), &[(0.0, 0.7), (27.0, 0.3)]));
    // Drawn as a wireframe: a ring at each height, lines along it.
    assert_eq!(
        ols_structure_edit::geometry::pole_wireframe(h.state().file.as_ref().unwrap().structure())
            .len(),
        2 + 8
    );
    // Design › Pole: the pole check can read it.
    menu(&mut h, "Design", "Pole design…");
    assert_eq!(h.state_mut().focused(), Some(Panel::PoleDesign));
    h.get_by_label("The pole check can read the pole (level 3).");
    let t = h.state().file.as_ref().unwrap().structure().clone();
    assert_eq!(t.detail(), Detail::Design);
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

#[test]
fn an_extension_gets_its_piece_and_a_leg() {
    let path = copy_of("type-02.ohls", "ext-ui");
    let mut h = harness(&path);
    menu(&mut h, "Body", "Extensions…");
    assert_eq!(h.state_mut().focused(), Some(Panel::Extensions));
    h.get_by_label("+ Extension").click();
    h.run();
    assert_eq!(h.state().ext.picked.as_deref(), Some("+3"));
    // A plain extension at level 3: advice, the model leaves it out.
    h.get_by_label("advice (41)");
    h.get_by_label("Build the piece").click();
    h.run();
    let t = h.state().file.as_ref().unwrap().structure().clone();
    let e = t.extension("+3").unwrap();
    assert_eq!(e.nodes.len(), 4);
    assert_eq!(e.members.len(), 4 + 8);
    assert_eq!(t.detail(), Detail::Design);
    // Shown in 3D: the view stands on it.
    assert_eq!(h.state().ext.shown.as_deref(), Some("+3"));
    // One undo step.
    h.get_by_label("Undo").click();
    h.run();
    let t = h.state().file.as_ref().unwrap().structure().clone();
    assert!(t.extension("+3").unwrap().nodes.is_empty());
    h.get_by_label("Redo").click();
    h.run();
    // Leg A one metre longer.
    h.get_by_label("+ Leg").click();
    h.run();
    let t = h.state().file.as_ref().unwrap().structure().clone();
    let x = t.extended(Some("+3")).unwrap();
    assert_eq!(x.lowered.len(), 1);
    h.get_by_label("leg A");
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}
