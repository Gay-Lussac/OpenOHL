// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Bolt grade (the owner's decision of 2026-11-19): Design › Bolt grade…
//! opens the window, Apply sets the grade of the whole tower's bolts as one
//! undo step, recorded as set by command with its source; Escape changes
//! nothing. With `OHL_RENDER_PNG` set the window is rendered before and
//! after (`tool-bolt-grade.png`, `tool-bolt-grade-after.png`).

use egui_kittest::kittest::Queryable;

use crate::tests::{copy_of, harness, menu};

/// The example structure as a lattice tower (Design's tower commands).
fn tower(tag: &str) -> std::path::PathBuf {
    let path = copy_of("type-02.ohls", tag);
    let text = std::fs::read_to_string(&path).unwrap().replacen(
        "kind = \"other\"",
        "kind = \"lattice\"",
        1,
    );
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn the_towers_bolts_get_a_grade_in_one_undo_step() {
    let path = tower("bolt-grade-window");
    let mut h = harness(&path);
    let before = h.state().file.as_ref().unwrap().text();
    menu(&mut h, "Design", "Bolt grade…");
    h.run_steps(2);
    {
        let w = h.state().bolt_grade_window.as_ref().expect("the window");
        assert_eq!(w.scope, crate::bolt_grade_window::GradeScope::Tower);
        assert_eq!(w.grade, Some("8.8"));
    }
    crate::tool_window_tests::window_shot(&mut h, "Bolt grade", "bolt-grade");
    // Nothing written before Apply.
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    h.get_all_by_label_contains("Apply (")
        .next()
        .unwrap()
        .click();
    h.run_steps(2);
    assert!(h.state().bolt_grade_window.is_none());
    let f = h.state().file.as_ref().unwrap();
    assert_ne!(f.text(), before);
    assert!(f.undo_name().unwrap().starts_with("Bolt grade 8.8"));
    let t = f.structure();
    assert!(!t.members.is_empty());
    for m in &t.members {
        let g = t.member_bolt_grade(&m.id).expect("every member's bolts");
        assert_eq!(g.grade, "8.8");
        assert!((g.ultimate.si() - 800e6).abs() < 1.0);
        assert_eq!(g.set_by.as_deref(), Some("command"));
        assert_eq!(g.source.as_deref(), Some("SIA 263:2013, 3.4.2, Table 2"));
    }
    // Opened again: what is set now, said.
    h.state_mut().open_bolt_grade_window();
    h.run_steps(2);
    assert!(
        h.query_all_by_label_contains("bolt grade 8.8 set by command")
            .next()
            .is_some()
    );
    crate::tool_window_tests::window_shot(&mut h, "Bolt grade", "bolt-grade-after");
    h.state_mut().bolt_grade_window = None;
    h.state_mut().file.as_mut().unwrap().undo();
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

#[test]
fn escape_closes_the_bolt_grade_window_and_changes_nothing() {
    let path = copy_of("type-02.ohls", "bolt-grade-escape");
    let mut h = harness(&path);
    let before = h.state().file.as_ref().unwrap().text();
    h.state_mut().open_bolt_grade_window();
    h.step();
    h.key_press(eframe::egui::Key::Escape);
    h.step();
    assert!(h.state().bolt_grade_window.is_none());
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}
