// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Several objects selected (owner, 2026-10-27): Shift+click adds,
//! Cmd/Ctrl+click adds or takes out, in the views and the tables; all
//! highlighted; Properties says how many; Delete deletes them all in one
//! command; Escape clears.

use super::session_edits::{click_at, press, synthetic_project};
use super::*;

/// A click in the plan on structure `id` (the plan centred on it) with
/// `modifiers` held.
fn click_structure(h: &mut Harness<'_, App>, id: &str, modifiers: egui::Modifiers) {
    let at = {
        let r = h.state_mut().ready().unwrap();
        let p = r.model.line.structure(id).unwrap().position;
        r.plan.centre_on(p);
        r.plan.drawn.unwrap().center()
    };
    h.run();
    h.input_mut()
        .events
        .push(egui::Event::ModifiersChanged(modifiers));
    click_at(h, at, egui::PointerButton::Primary);
    h.input_mut()
        .events
        .push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    h.run();
}

fn selected(h: &mut Harness<'_, App>) -> (Selection, Vec<Selection>) {
    let r = h.state_mut().ready().unwrap();
    (r.selection.clone(), r.model.selected_also.clone())
}

fn st(id: &str) -> Selection {
    Selection::Structure(id.into())
}

/// In the plan: a click selects one; Shift+click adds (the last clicked
/// is the selection, in Properties with "2 selected"); both highlighted
/// (the 3D selection layer covers both, the tables' rows); Cmd/Ctrl+click
/// takes one out and adds it again; a plain click makes one the only one;
/// Escape clears.
#[test]
fn shift_and_command_click_add_and_toggle() {
    let model = synthetic_project("multi-select");
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    h.run();
    h.state_mut().show_tab(crate::app::Tab::Plan);
    h.run();
    click_structure(&mut h, "T3", egui::Modifiers::NONE);
    assert_eq!(selected(&mut h), (st("T3"), vec![]));
    click_structure(&mut h, "T4", egui::Modifiers::SHIFT);
    assert_eq!(selected(&mut h), (st("T4"), vec![st("T3")]));
    h.get_by_label_contains("2 selected");
    // Both highlighted: the 3D view's selection layer reaches both, the
    // tables' rows of both are marked.
    {
        let r = h.state_mut().ready().unwrap();
        let line = &r.model.line;
        let (a, b) = (
            line.structure("T3").unwrap().position,
            line.structure("T4").unwrap().position,
        );
        let layer = crate::scene::layer(
            &r.model,
            crate::scene::Layer::Selection,
            r.case,
            &r.selection,
        );
        let bounds = layer.bounds().unwrap();
        assert!(bounds.max[0] - bounds.min[0] >= (a[0] - b[0]).abs() - 1e-6);
        assert!(bounds.max[1] - bounds.min[1] >= (a[1] - b[1]).abs() - 1e-6);
        for id in ["T3", "T4"] {
            assert!(crate::selection::is_selected(
                &r.selection,
                &r.model,
                &st(id)
            ));
        }
        assert!(!crate::selection::is_selected(
            &r.selection,
            &r.model,
            &st("T5")
        ));
    }
    // Cmd/Ctrl+click: T3 out, then in again (the last clicked).
    click_structure(&mut h, "T3", egui::Modifiers::COMMAND);
    assert_eq!(selected(&mut h), (st("T4"), vec![]));
    click_structure(&mut h, "T3", egui::Modifiers::COMMAND);
    assert_eq!(selected(&mut h), (st("T3"), vec![st("T4")]));
    // Cmd/Ctrl+click on the selection itself: the other one follows.
    click_structure(&mut h, "T3", egui::Modifiers::COMMAND);
    assert_eq!(selected(&mut h), (st("T4"), vec![]));
    click_structure(&mut h, "T5", egui::Modifiers::SHIFT);
    // A plain click: only that one.
    click_structure(&mut h, "T2", egui::Modifiers::NONE);
    assert_eq!(selected(&mut h), (st("T2"), vec![]));
    click_structure(&mut h, "T3", egui::Modifiers::SHIFT);
    assert_eq!(selected(&mut h).1.len(), 1);
    press(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    assert_eq!(selected(&mut h), (Selection::None, vec![]));
}

/// In a table: Shift+⤴ adds the row's object, ⤴ alone selects only it.
#[test]
fn shift_click_in_a_table_adds() {
    let model = synthetic_project("multi-select-table");
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    h.run();
    h.state_mut()
        .dock()
        .push_to_focused_leaf(crate::app::Tab::Table(crate::tables::TableKind::Structures));
    h.run();
    h.get_all_by_label("⤴").next().unwrap().click();
    h.run();
    let first = selected(&mut h).0;
    assert!(matches!(first, Selection::Structure(_)), "{first:?}");
    h.input_mut()
        .events
        .push(egui::Event::ModifiersChanged(egui::Modifiers::SHIFT));
    h.get_all_by_label("⤴").nth(1).unwrap().click();
    h.run();
    h.input_mut()
        .events
        .push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    h.run();
    let (now, also) = selected(&mut h);
    assert_ne!(now, first);
    assert_eq!(also, vec![first.clone()]);
    h.get_all_by_label("⤴").nth(2).unwrap().click();
    h.run();
    assert!(selected(&mut h).1.is_empty());
}

/// Delete selected with several: one command for all (one undo brings
/// them back), with the confirmation of what they take with them.
#[test]
fn delete_selected_deletes_all_in_one_command() {
    let model = synthetic_project("multi-select-delete");
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    h.run();
    {
        let r = h.state_mut().ready().unwrap();
        r.go_to(st("T6"));
        r.select_more(st("T7"), false);
        r.delete_selected();
        // T6 and T7 take the branch's section with them: asked first.
        assert!(r.confirm.is_some(), "a confirmation");
        assert!(r.model.line.structure("T6").is_some());
        let (impact, action) = r.confirm.take().unwrap();
        assert!(!impact.sections.is_empty(), "{impact:?}");
        assert!(r.perform(action), "{:?}", r.status);
        assert!(r.model.line.structure("T6").is_none());
        assert!(r.model.line.structure("T7").is_none());
        assert_eq!(r.selection, Selection::None);
        assert!(r.model.selected_also.is_empty());
        r.perform(crate::edit::Action::Undo);
        assert!(r.model.line.structure("T6").is_some());
        assert!(r.model.line.structure("T7").is_some());
    }
}

/// The rule itself: add, the same again, toggle out (the selection: the
/// last of the others follows), a click on nothing.
#[test]
fn multi_click_rules() {
    use crate::selection::multi_click;
    let (mut s, mut also) = (Selection::None, Vec::new());
    multi_click(&mut s, &mut also, st("A"), false);
    assert_eq!((s.clone(), also.clone()), (st("A"), vec![]));
    multi_click(&mut s, &mut also, st("B"), false);
    multi_click(&mut s, &mut also, st("B"), false);
    assert_eq!((s.clone(), also.clone()), (st("B"), vec![st("A")]));
    multi_click(&mut s, &mut also, Selection::None, true);
    assert_eq!((s.clone(), also.clone()), (st("B"), vec![st("A")]));
    multi_click(&mut s, &mut also, st("B"), true);
    assert_eq!((s.clone(), also.clone()), (st("A"), vec![]));
    multi_click(&mut s, &mut also, st("A"), true);
    assert_eq!((s, also), (Selection::None, vec![]));
}
