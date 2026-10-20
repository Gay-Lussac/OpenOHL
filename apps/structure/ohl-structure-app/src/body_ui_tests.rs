// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Body panels driven as a user would (egui_kittest): a generator
//! previewed and applied in one undo step, crossarms on it, a group set
//! on members picked with Shift + click.

use egui_kittest::kittest::Queryable;
use ohl_structure_edit::Handle;

use crate::layout::Panel;
use crate::tests::{copy_of, harness, menu};

/// Clicks the generator form's Apply (scrolled into view: the form with
/// its options can be taller than the panel).
fn apply(h: &mut egui_kittest::Harness<'static, crate::app::StructureApp>) {
    h.get_by_label("Apply").scroll_to_me();
    h.run();
    h.get_by_label("Apply").click();
    h.run();
}

#[test]
fn a_body_is_generated_grouped_and_undone() {
    let path = copy_of("type-01.ohls", "body");
    let mut h = harness(&path);
    menu(&mut h, "Body", "Generators…");
    assert_eq!(h.state_mut().focused(), Some(Panel::Generators));
    h.get_by_label("legs and panels").click();
    h.run();
    // Four X panels with horizontals up to the points (30 m): previewed.
    assert_eq!(h.state().body.preview.len(), 64);
    let before = h.state().file.as_ref().unwrap().text();
    apply(&mut h);
    let t = h.state().file.as_ref().unwrap().structure().clone();
    assert_eq!((t.nodes.len(), t.members.len()), (20, 64));
    assert_eq!(t.detail().level(), 2);
    assert!(h.state().body.form.is_none());
    // One undo step.
    h.get_by_label("Undo").click();
    h.run();
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    h.get_by_label("Redo").click();
    h.run();
    // Crossarms to the two points (off any joint): on the face at 22.5 m
    // and 30 m.
    h.get_by_label("crossarms").click();
    h.run();
    assert_eq!(h.state().body.preview.len(), 8);
    apply(&mut h);
    let t = h.state().file.as_ref().unwrap().structure().clone();
    assert_eq!(t.members.len(), 72);
    assert!(t.points.iter().all(|p| p.node.is_some()));
    // Two members picked (the second with Shift), a group set on them.
    menu(&mut h, "Body", "Members…");
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
    // Design › Groups lists it (the members' table beside it too).
    menu(&mut h, "Design", "Groups…");
    assert_eq!(h.state_mut().focused(), Some(crate::layout::Panel::Groups));
    assert!(h.get_all_by_label("main-leg").count() >= 2);
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

/// The options of the generators in their forms: redundants on a panel
/// change the preview (counted by hand: the first of four X panels with
/// horizontals, one level: 4 more leg pieces, 8 more diagonal pieces, 8
/// redundants), and the crossarms' "several points on one arm" ticks.
#[test]
fn generator_options_in_the_forms() {
    let path = copy_of("type-01.ohls", "options");
    let mut h = harness(&path);
    menu(&mut h, "Body", "Generators…");
    h.get_by_label("legs and panels").click();
    h.run();
    assert_eq!(h.state().body.preview.len(), 64);
    for header in ["width", "redundants", "plan"] {
        assert!(h.query_by_label(header).is_some(), "{header}");
    }
    if let Some(f) = &mut h.state_mut().body.form
        && let ohl_structure_edit::Kind::Panels(p) = &mut f.kind
    {
        p.panels[0].redundants = ohl_structure_edit::Redundants::One;
    }
    h.run();
    assert_eq!(h.state().body.preview.len(), 84);
    apply(&mut h);
    let text = h.state().file.as_ref().unwrap().text();
    assert!(text.contains("redundants = \"one\""), "{text}");
    h.get_by_label("crossarms").click();
    h.run();
    h.get_by_label("several points on one arm").click();
    h.run();
    let one_arm = matches!(
        &h.state().body.form,
        Some(f) if matches!(&f.kind, ohl_structure_edit::Kind::Crossarm(c) if c.one_arm)
    );
    assert!(one_arm);
    // One point on each side: the same arms as without the option.
    assert_eq!(h.state().body.preview.len(), 8);
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}
