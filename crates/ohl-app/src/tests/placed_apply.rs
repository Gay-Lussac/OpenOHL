// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: a structure's Edit window (a structure without a type, its
//! structure screen) applied reaches the line and every view; undo and
//! redo of that Apply too.

use super::*;

fn gpu() -> bool {
    let gpu = ohl_render::headless_device().is_ok();
    assert!(
        gpu || ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
        "no GPU"
    );
    gpu
}

fn pointer(h: &mut Harness<'_, App>, at: egui::Pos2, pressed: Option<bool>) {
    h.input_mut().events.push(egui::Event::PointerMoved(at));
    if let Some(pressed) = pressed {
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    h.step();
}

/// What the views show of structure T3 and its wires: its points and wire
/// points in the line, and every section's solved shape in the case shown.
fn shown(r: &mut crate::app::Ready) -> (ohl_line::Structure, Vec<ohl_line::solve::SectionShape>) {
    let s = r.model.line.structure("T3").unwrap().clone();
    let case = r.case;
    let shapes = r
        .model
        .line
        .sections
        .iter()
        .filter(|x| x.points.iter().any(|p| p.structure == "T3"))
        .filter_map(|x| r.model.shape(&x.id, case).cloned())
        .collect();
    (s, shapes)
}

/// Every view has drawn the line as it is: nothing left to rebuild in the
/// 3D view, the shown case solved as a full solve of the line gives it.
fn views_current(r: &mut crate::app::Ready) {
    assert!(
        r.view3d.dirty.is_empty(),
        "the 3D view has not drawn the change yet"
    );
    assert!(r.model.cases[r.case].ready, "the case shown is solved");
    assert_solved_fully(&r.model);
}

/// The owner's report (2026-11-18): the changes of a structure's Edit
/// window did not seem to reach OHL-Design's main view. T3 (no type):
/// its attachment point A1 dragged up in the window, Apply — the line has
/// it (one command), the window says it is applied, and every view shows
/// it at once (the 3D view drawn again, the case shown solved again: the
/// wires of the sections through T3 hang from where W1 is now); undo and
/// redo of that Apply likewise, the window following the line.
#[test]
fn the_edit_windows_apply_reaches_every_view() {
    if !gpu() {
        return;
    }
    let model = synthetic_project("placed-apply");
    let mut h = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .wgpu()
        .build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    h.run();
    h.state_mut().ready().unwrap().selection = Selection::Structure("T3".into());
    h.run();
    h.get_by_label(crate::properties::STRINGS_AND_POINTS)
        .click();
    h.run();
    h.run();
    let before = shown(h.state_mut().ready().unwrap());
    let a1 = {
        let p = h.state_mut().ready().unwrap().placed.as_ref().unwrap();
        let t = p.workspace.shown().unwrap();
        crate::type_workspace::geometry(t, &ohl_structures::InsulatorLibrary::default())
            .at("A1")
            .unwrap()
    };
    let from = h
        .state_mut()
        .ready()
        .unwrap()
        .placed
        .as_ref()
        .unwrap()
        .workspace
        .screen_of(a1)
        .unwrap();
    pointer(&mut h, from, None);
    pointer(&mut h, from, Some(true));
    for k in 1..=8 {
        pointer(&mut h, from + egui::vec2(0.0, -8.0 * k as f32), None);
    }
    pointer(&mut h, from + egui::vec2(0.0, -64.0), Some(false));
    h.run();
    assert!(
        h.state_mut()
            .ready()
            .unwrap()
            .placed
            .as_ref()
            .unwrap()
            .dirty()
    );
    h.get_by_label("* not applied");
    let entries = h
        .state_mut()
        .ready()
        .unwrap()
        .model
        .session()
        .unwrap()
        .log()
        .unwrap()
        .len();
    h.get_by_label("Apply").click();
    h.run();
    let after = {
        let r = h.state_mut().ready().unwrap();
        let log = r.model.session().unwrap().log().unwrap();
        let kinds: Vec<(String, Vec<(ohl_session::Kind, String)>)> = log[entries..]
            .iter()
            .map(|e| (e.kind.clone(), e.objects.clone()))
            .collect();
        // One command putting T3 (its strings without a library entry
        // given one first: the library file written, as after every edit
        // putting a structure).
        let puts = kinds
            .iter()
            .filter(|k| k.1.iter().any(|o| o.0 == ohl_session::Kind::Structure))
            .count();
        assert_eq!(puts, 1, "{kinds:?}");
        assert!(
            kinds.iter().all(|k| k.1.iter().all(|o| matches!(
                o.0,
                ohl_session::Kind::Structure | ohl_session::Kind::Library
            ))),
            "{kinds:?}"
        );
        let p = r.placed.as_ref().unwrap();
        assert!(!p.dirty());
        assert_eq!(p.message.as_ref().map(|m| m.0.as_str()), Some("applied"));
        let now = shown(r);
        let (a, b) = (
            before.0.point("A1").unwrap().position,
            now.0.point("A1").unwrap().position,
        );
        assert!(b[2] > a[2] + 0.3, "{a:?} ➡ {b:?}");
        // W1 came along (its string keeps its length).
        let (wa, wb) = (
            before.0.wire("W1").unwrap().position,
            now.0.wire("W1").unwrap().position,
        );
        assert!(
            (wb[2] - wa[2] - (b[2] - a[2])).abs() < 1e-3,
            "{wa:?} ➡ {wb:?}"
        );
        assert_ne!(now.1, before.1, "the wires at T3 solved again");
        views_current(r);
        now
    };
    // Undo (⌘Z): back as it was, every view; the window follows.
    press(&mut h, egui::Key::Z, egui::Modifiers::COMMAND);
    h.run();
    {
        let r = h.state_mut().ready().unwrap();
        let now = shown(r);
        assert_eq!(now.0, before.0);
        assert_eq!(now.1, before.1);
        views_current(r);
        let p = r.placed.as_ref().unwrap();
        assert!(!p.dirty(), "the window shows the line as it is");
        assert_eq!(
            p.doc.to_string(),
            crate::placed_editor::as_type(r.model.line.structure("T3").unwrap()).to_string()
        );
    }
    // Redo: the change again.
    press(
        &mut h,
        egui::Key::Z,
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
    );
    h.run();
    let r = h.state_mut().ready().unwrap();
    let now = shown(r);
    assert_eq!(now.0, after.0);
    assert_eq!(now.1, after.1);
    views_current(r);
    let p = r.placed.as_ref().unwrap();
    assert_eq!(
        p.doc.to_string(),
        crate::placed_editor::as_type(r.model.line.structure("T3").unwrap()).to_string()
    );
}
