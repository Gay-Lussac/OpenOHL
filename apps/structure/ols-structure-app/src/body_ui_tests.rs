// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The Body tab driven as a user would (egui_kittest): a generator
//! previewed and applied in one undo step, crossarms on it, a group set
//! on members picked with Shift + click.

use egui_kittest::kittest::Queryable;
use ols_structure_edit::Handle;

use crate::app::{Sheet, Tab};
use crate::tests::{copy_of, harness};

#[test]
fn a_body_is_generated_grouped_and_undone() {
    let path = copy_of("type-01.olss", "body");
    let mut h = harness(&path);
    h.get_by_label("Body").click();
    h.run();
    h.get_by_label("Generators").click();
    h.run();
    assert_eq!(h.state().sheets[1], Sheet::Generators);
    h.get_by_label("legs and panels").click();
    h.run();
    // Four X panels with horizontals up to the points (30 m): previewed.
    assert_eq!(h.state().body.preview.len(), 64);
    let before = h.state().file.as_ref().unwrap().text();
    h.get_by_label("Apply").click();
    h.run();
    let t = h.state().file.as_ref().unwrap().structure().clone();
    assert_eq!((t.nodes.len(), t.members.len()), (20, 64));
    assert_eq!(t.detail().level(), 2);
    assert!(h.state().body.form.is_none());
    // One undo step.
    h.get_by_label("⟲ Undo").click();
    h.run();
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    h.get_by_label("⟳ Redo").click();
    h.run();
    // Crossarms to the two points (off any joint): on the face at 22.5 m
    // and 30 m.
    h.get_by_label("crossarms").click();
    h.run();
    assert_eq!(h.state().body.preview.len(), 8);
    h.get_by_label("Apply").click();
    h.run();
    let t = h.state().file.as_ref().unwrap().structure().clone();
    assert_eq!(t.members.len(), 72);
    assert!(t.points.iter().all(|p| p.node.is_some()));
    // Two members picked (the second with Shift), a group set on them.
    h.state_mut().sheets[1] = Sheet::Members;
    h.run();
    h.get_by_label("body-M1").click();
    h.run();
    h.get_by_label("body-M2")
        .click_modifiers(eframe::egui::Modifiers::SHIFT);
    h.run();
    assert_eq!(
        h.state().selected,
        vec![
            Handle::Member("body-M1".into()),
            Handle::Member("body-M2".into())
        ]
    );
    h.state_mut().body.group_text = "main-leg".into();
    h.run();
    h.get_by_label("Set on 2 member(s)").click();
    h.run();
    let t = h.state().file.as_ref().unwrap().structure().clone();
    let grouped: Vec<&str> = t
        .members
        .iter()
        .filter(|m| m.group.as_deref() == Some("main-leg"))
        .map(|m| m.id.as_str())
        .collect();
    assert_eq!(grouped, ["body-M1", "body-M2"]);
    // Design › Groups lists it.
    h.state_mut().tab = Tab::Design;
    h.state_mut().sheets[2] = Sheet::Groups;
    h.run();
    h.get_by_label("main-leg");
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}
