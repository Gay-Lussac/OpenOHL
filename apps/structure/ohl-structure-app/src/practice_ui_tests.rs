// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Detailing practice (O44 L29, the owner's L24 question 5): Design ›
//! Detailing practice… opens the editor; Save writes only what differs from
//! the default as the file's own `[layout_practice]`, one undo step, the
//! file raised to 30 with a plate thickness rule; a value out of range
//! greys Save and is said; Escape changes nothing; Lay out joints says
//! where its own settings go beyond the practice. With `OHL_RENDER_PNG`
//! set the window is rendered before and after
//! (`tool-detailing-practice.png`, `tool-detailing-practice-after.png`).

use egui_kittest::kittest::Queryable;

use crate::tests::{copy_of, harness, menu};

/// The example structure as a lattice tower (Design's tower commands),
/// checked by the core's simple check (no code plugin asked for a
/// practice: the pictures show no machine's plugin folder).
fn tower(tag: &str) -> std::path::PathBuf {
    let path = copy_of("type-02.ohls", tag);
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replacen("kind = \"other\"", "kind = \"lattice\"", 1)
        .replacen("[analysis]\n", "[analysis]\ncapacity = \"simple\"\n", 1);
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn the_practice_is_saved_in_the_file_in_one_undo_step() {
    let path = tower("practice-window");
    let mut h = harness(&path);
    let before = h.state().file.as_ref().unwrap().text();
    assert!(!before.contains("[layout_practice"));
    menu(&mut h, "Design", "Detailing practice…");
    h.run_steps(2);
    {
        let w = h.state().practice_window.as_ref().expect("the window");
        assert!(w.edit.is_default(), "nothing given: the default in force");
        assert_eq!(w.under_from, "the program's default");
    }
    crate::tool_window_tests::window_shot(&mut h, "Detailing practice", "detailing-practice");
    // A shop that never welds, a 20 mm shift, plates at least 6 mm in its
    // stock thicknesses.
    {
        let w = h.state_mut().practice_window.as_mut().unwrap();
        w.edit.name = "Example shop".into();
        for f in &mut w.edit.fabrication {
            if f.0 == ohl_structure_link::practice::Fabrication::Welded {
                f.1 = false;
            }
        }
        w.edit.shift = 0.020;
        w.edit.least_thickness = 0.006;
        w.thicknesses = "6, 8, 10, 12 mm".into();
    }
    h.run_steps(2);
    assert!(
        h.query_all_by_label_contains("NOT ON THE SAFE SIDE where a plate is thicker")
            .next()
            .is_some(),
        "the thickness rule's mark shown"
    );
    crate::tool_window_tests::window_shot(&mut h, "Detailing practice", "detailing-practice-after");
    // Nothing written before Save.
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    h.get_all_by_label_contains("Save in the structure file")
        .next()
        .unwrap()
        .click();
    h.run_steps(2);
    assert!(h.state().practice_window.is_none());
    let f = h.state().file.as_ref().unwrap();
    assert_eq!(f.undo_name(), Some("Detailing practice"));
    let text = f.text();
    assert!(
        text.contains("version = 30"),
        "raised for the thickness rule"
    );
    let own = f
        .structure()
        .layout_practice
        .clone()
        .expect("its own table");
    // Only what differs from the default.
    assert!(ohl_structure_link::practice::gives(&own, "limits.shift"));
    assert!(!ohl_structure_link::practice::gives(&own, "limits.gap"));
    assert!(!ohl_structure_link::practice::gives(&own, "faces"));
    assert!(ohl_structure_link::practice::gives(
        &own,
        "plates.thicknesses"
    ));
    let p = ohl_structure_link::practice::in_force(Some(&own), None)
        .unwrap()
        .unwrap()
        .practice;
    assert_eq!(p.name, "Example shop");
    assert!(!p.makes(ohl_structure_link::practice::Fabrication::Welded));
    assert!((p.shift - 0.020).abs() < 1e-9);
    assert_eq!(p.thicknesses.len(), 4);
    assert!((p.least_thickness - 0.006).abs() < 1e-9);
    // Opened again: the file's values.
    h.state_mut().open_practice_window();
    h.run_steps(1);
    assert_eq!(
        h.state().practice_window.as_ref().unwrap().thicknesses,
        "6, 8, 10, 12"
    );
    // Lay out joints: its shift (the last run's or the practice's, 20 mm)
    // set to 40 mm goes beyond the practice — said, kept.
    h.state_mut().practice_window = None;
    h.state_mut()
        .open_layout_window(crate::layout_window::Scope::Tower);
    h.state_mut().layout_window.as_mut().unwrap().shift_mm = 40.0;
    h.run_steps(2);
    assert!(
        h.query_all_by_label_contains("Beyond the detailing practice")
            .next()
            .is_some()
    );
    h.state_mut().layout_window = None;
    h.state_mut().file.as_mut().unwrap().undo();
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

#[test]
fn a_value_out_of_range_waits_and_escape_changes_nothing() {
    let path = tower("practice-escape");
    let mut h = harness(&path);
    let before = h.state().file.as_ref().unwrap().text();
    h.state_mut().open_practice_window();
    h.state_mut().practice_window.as_mut().unwrap().thicknesses = "4, 8, x".into();
    h.run_steps(2);
    let (_, refused) = h.state().practice_window.as_ref().unwrap().checked();
    assert!(refused.iter().any(|r| r.contains("\"x\"")), "{refused:?}");
    // Save greyed: a click does nothing.
    if let Some(b) = h.get_all_by_label_contains("Save").next() {
        b.click();
    }
    h.run_steps(2);
    assert!(h.state().practice_window.is_some());
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    h.key_press(eframe::egui::Key::Escape);
    h.step();
    assert!(h.state().practice_window.is_none());
    assert_eq!(h.state().file.as_ref().unwrap().text(), before);
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}
