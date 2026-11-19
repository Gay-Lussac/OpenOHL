// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Dragging the ends of strings keeps their lengths ([`crate::string_drag`],
//! owner 2026-11-18): in a placed structure's Edit window (the structure
//! screen on a structure without a type) and in the line's 3D view.

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use ohl_structure_edit::Handle;
use ohl_structures::structures::to_local;

use crate::app::App;
use crate::selection::Selection;
use crate::string_drag::distance;
use crate::tests::synthetic_project_with;

fn gpu() -> bool {
    let gpu = ohl_render::headless_device().is_ok();
    assert!(
        gpu || ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
        "no GPU"
    );
    gpu
}

fn pointer(h: &mut Harness<'_, App>, at: egui::Pos2, pressed: Option<bool>, m: egui::Modifiers) {
    h.input_mut().events.push(egui::Event::ModifiersChanged(m));
    h.input_mut().events.push(egui::Event::PointerMoved(at));
    if let Some(pressed) = pressed {
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: m,
        });
    }
    h.step();
}

/// A drag from `from` by `by` pixels, in steps, the modifiers `m` held.
fn drag(h: &mut Harness<'_, App>, from: egui::Pos2, by: egui::Vec2, m: egui::Modifiers) {
    pointer(h, from, None, m);
    pointer(h, from, Some(true), m);
    for k in 1..=8 {
        pointer(h, from + by * (k as f32 / 8.0), None, m);
    }
    pointer(h, from + by, Some(false), m);
    h.input_mut()
        .events
        .push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    h.run();
}

/// T3 of the example with a V: W2 held also by S2b from a new point A3
/// beside A2.
fn with_v(name: &str) -> crate::model::Model {
    synthetic_project_with(name, |line| {
        let s = line.structures.iter_mut().find(|s| s.id == "T3").unwrap();
        let a2 = s.point("A2").unwrap().position;
        s.points.push(ohl_line::StructurePoint {
            label: "A3".into(),
            position: [a2[0] + 1.0, a2[1] + 0.5, a2[2]],
            ..Default::default()
        });
        s.strings.push(ohl_line::InsulatorString {
            label: "S2b".into(),
            top: "A3".into(),
            wire: "W2".into(),
            ..Default::default()
        });
    })
}

/// The owner's case (2026-11-18), in the Edit window of a structure
/// without a type (T3; its structure screen, the drags of the type
/// workspace): a wire point dragged turns its string about its point —
/// the length kept (to the file's millimetre), the point not moved, the
/// window saying the string's angles; before, the wire point went where
/// the pointer was and the string's length changed. An attachment point
/// dragged takes its string along — the wire point moved by the same
/// vector; before, the wire point stayed and the string's length
/// changed. No key held changes a length (owner, 2026-11-18: a string's
/// length is its library entry's). Apply puts it
/// on the line (one command), the lengths as dragged.
#[test]
fn the_edit_window_keeps_the_strings_lengths() {
    if !gpu() {
        return;
    }
    let model = crate::tests::synthetic_project("string-drag-window");
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
    // Where the window's document has a point or wire point, and where it
    // is drawn.
    let at = |h: &mut Harness<'_, App>, l: &str| -> [f64; 3] {
        let r = h.state_mut().ready().unwrap();
        let empty = ohl_structures::InsulatorLibrary::default();
        let insulators = r.model.types.as_ref().map_or(&empty, |(_, i)| i);
        let p = r.placed.as_ref().unwrap();
        let t = p.workspace.shown().unwrap();
        crate::type_workspace::geometry(t, insulators)
            .at(l)
            .unwrap()
    };
    let screen = |h: &mut Harness<'_, App>, p: [f64; 3]| -> egui::Pos2 {
        let r = h.state_mut().ready().unwrap();
        r.placed.as_ref().unwrap().workspace.screen_of(p).unwrap()
    };
    let message = |h: &mut Harness<'_, App>| {
        let p = h.state_mut().ready().unwrap().placed.as_ref().unwrap();
        p.workspace.message.clone().unwrap_or_default()
    };
    // W1 hangs 3.5 m below A1 (S1): dragged 80 px to the side and up.
    let (a1, w1) = (at(&mut h, "A1"), at(&mut h, "W1"));
    assert!((distance(a1, w1) - 3.5).abs() < 1e-9);
    let from = screen(&mut h, w1);
    drag(&mut h, from, egui::vec2(80.0, -40.0), egui::Modifiers::NONE);
    let w1_now = at(&mut h, "W1");
    assert!(distance(w1_now, w1) > 0.3, "{w1:?} ➡ {w1_now:?}");
    assert_eq!(at(&mut h, "A1"), a1, "its point does not move");
    assert!(
        (distance(a1, w1_now) - 3.5).abs() < 1e-3,
        "S1 {} m (the file's millimetre)",
        distance(a1, w1_now)
    );
    let (m, error) = message(&mut h);
    assert!(!error, "{m}");
    assert!(m.contains("lengths kept: S1 3.50"), "{m}");
    assert!(m.contains("from the vertical (across"), "{m}");
    // A2 dragged up: W2 (S2 nearly level) moves by the same vector.
    let (a2, w2) = (at(&mut h, "A2"), at(&mut h, "W2"));
    let from = screen(&mut h, a2);
    drag(&mut h, from, egui::vec2(0.0, -60.0), egui::Modifiers::NONE);
    let (a2_now, w2_now) = (at(&mut h, "A2"), at(&mut h, "W2"));
    let d = [0, 1, 2].map(|k| a2_now[k] - a2[k]);
    assert!(d.iter().map(|v| v.abs()).sum::<f64>() > 0.3, "{d:?}");
    for k in 0..3 {
        assert!(
            (w2_now[k] - w2[k] - d[k]).abs() < 1e-9,
            "{w2:?} ➡ {w2_now:?}, {d:?}"
        );
    }
    let (m, _) = message(&mut h);
    assert!(m.contains("wire point W2 moved along"), "{m}");
    // Apply: one command; on the line the strings keep their lengths.
    let (s1, s2) = {
        let s = h
            .state_mut()
            .ready()
            .unwrap()
            .model
            .line
            .structure("T3")
            .unwrap();
        (
            s.string("S1").unwrap().length,
            s.string("S2").unwrap().length,
        )
    };
    h.get_by_label("Apply").click();
    h.run();
    {
        let r = h.state_mut().ready().unwrap();
        let s = r.model.line.structure("T3").unwrap();
        let length = |l: &str| s.string(l).unwrap().length;
        assert!((length("S1") - s1).abs() < 1e-3, "{} {s1}", length("S1"));
        assert!((length("S2") - s2).abs() < 1e-3, "{} {s2}", length("S2"));
        let w = to_local(
            s.position,
            s.orientation_deg,
            s.wire("W1").unwrap().position,
        );
        assert!(distance(w, w1_now) < 1e-3, "{w:?} {w1_now:?}");
        assert!(!r.placed.as_ref().unwrap().dirty());
    }
    // Ctrl held: the length kept all the same.
    let w1 = at(&mut h, "W1");
    let from = screen(&mut h, w1);
    drag(
        &mut h,
        from,
        egui::vec2(-60.0, 50.0),
        egui::Modifiers::COMMAND,
    );
    let w1_now = at(&mut h, "W1");
    assert!(distance(w1_now, w1) > 0.3, "{w1:?} ➡ {w1_now:?}");
    assert!(
        (distance(a1, w1_now) - 3.5).abs() < 1e-3,
        "S1 {} m",
        distance(a1, w1_now)
    );
    let (m, _) = message(&mut h);
    assert!(m.contains("lengths kept: S1 3.50"), "{m}");
}

/// The line's 3D view, the commands: W1 of T3 dragged far away keeps
/// S1's length (to 1e-9 m) and A1, pointing at the pointer; a V point dragged turns the V about its other point,
/// both lengths kept (said), one undo back exactly; three strings
/// holding a wire point refuse its drag, said.
#[test]
fn the_3d_views_commands_keep_the_lengths() {
    let mut m = with_v("string-drag-commands");
    let before = m.line.structure("T3").unwrap().clone();
    let local = |p| to_local(before.position, before.orientation_deg, p);
    let a1 = local(before.point("A1").unwrap().position);
    let to = [a1[0] + 30.0, a1[1] - 12.0, a1[2] - 2.0];
    let (what, command) =
        crate::handles::command(&m, "T3", &Handle::Wire("W1".into()), to).unwrap();
    assert!(what.starts_with("moved wire point W1 of T3"), "{what}");
    let ohl_session::Command::Put { value, .. } = command else {
        panic!("{command:?}")
    };
    let put: ohl_line::Structure = serde_json::from_value(value).unwrap();
    let a1g = before.point("A1").unwrap().position;
    let w1 = put.wire("W1").unwrap().position;
    assert!(
        (distance(a1g, w1) - 3.5).abs() < 1e-9,
        "{}",
        distance(a1g, w1)
    );
    let (u, v) = (
        [0, 1, 2].map(|k| local(w1)[k] - a1[k]),
        [0, 1, 2].map(|k| to[k] - a1[k]),
    );
    let cos =
        (0..3).map(|k| u[k] * v[k]).sum::<f64>() / distance([0.0; 3], u) / distance([0.0; 3], v);
    assert!((cos - 1.0).abs() < 1e-9, "{cos}");
    // The V: A2 up 0.5 m, W2 turns about A3, S2 and S2b keep their
    // lengths.
    let stored_before = m.stored_line().unwrap().structure("T3").unwrap().clone();
    let a2 = local(before.point("A2").unwrap().position);
    let (what, command) = crate::handles::command(
        &m,
        "T3",
        &Handle::Point("A2".into()),
        [a2[0], a2[1], a2[2] + 0.5],
    )
    .unwrap();
    assert!(
        what.contains("wire point W2 also held by a string from a point not moved: turned"),
        "{what}"
    );
    m.apply(&what, command).unwrap();
    let after = m.line.structure("T3").unwrap();
    let length = |s: &ohl_line::Structure, l: &str| {
        let x = s.string(l).unwrap();
        distance(
            s.node(&x.top).unwrap().position(),
            s.wire(&x.wire).unwrap().position,
        )
    };
    for l in ["S2", "S2b"] {
        assert!(
            (length(after, l) - length(&before, l)).abs() < 1e-9,
            "{l}: {} ➡ {}",
            length(&before, l),
            length(after, l)
        );
        assert!((after.string(l).unwrap().length - length(after, l)).abs() < 1e-9);
    }
    assert!(
        distance(
            after.wire("W2").unwrap().position,
            before.wire("W2").unwrap().position
        ) > 0.05
    );
    m.undo().unwrap();
    assert_eq!(
        m.stored_line().unwrap().structure("T3"),
        Some(&stored_before)
    );
    assert_eq!(m.line.structure("T3"), Some(&before));
    // A third string to W2, from a point not in line with A2 and A3.
    let m = synthetic_project_with("string-drag-three", |line| {
        let s = line.structures.iter_mut().find(|s| s.id == "T3").unwrap();
        let a2 = s.point("A2").unwrap().position;
        for (l, p) in [("A3", [1.0, 0.5, 0.0]), ("A4", [-0.5, 1.0, 0.3])] {
            s.points.push(ohl_line::StructurePoint {
                label: l.into(),
                position: [a2[0] + p[0], a2[1] + p[1], a2[2] + p[2]],
                ..Default::default()
            });
            s.strings.push(ohl_line::InsulatorString {
                label: format!("S2{l}"),
                top: l.into(),
                wire: "W2".into(),
                ..Default::default()
            });
        }
    });
    let s = m.line.structure("T3").unwrap();
    let w2 = to_local(
        s.position,
        s.orientation_deg,
        s.wire("W2").unwrap().position,
    );
    let e = crate::handles::command(
        &m,
        "T3",
        &Handle::Wire("W2".into()),
        [w2[0] + 1.0, w2[1], w2[2]],
    )
    .unwrap_err();
    assert!(
        e.contains("held by 3 strings (S2, S2A3, S2A4): it cannot move without changing a length"),
        "{e}"
    );
}

/// The line's 3D view (the Move tool, T3's Edit window open): W1 dragged
/// turns S1 about A1 — the readout gives S1's length and angles, released
/// S1 is 3.5 m long (to 1e-9 m) and A1 has not moved; with Ctrl held
/// too (no key changes a length).
#[test]
fn the_3d_view_turns_a_dragged_wire_point() {
    use crate::handle_tests::{harness, on_screen, screen_aside};
    if !gpu() {
        return;
    }
    let mut h = harness(crate::tests::synthetic_project("string-drag-3d"));
    h.state_mut().ready().unwrap().selection = Selection::Structure("T3".into());
    h.run();
    let (a1, w1) = {
        let r = h.state_mut().ready().unwrap();
        r.edit_structure("T3");
        let s = r.model.line.structure("T3").unwrap();
        (
            s.point("A1").unwrap().position,
            s.wire("W1").unwrap().position,
        )
    };
    h.run();
    screen_aside(&mut h, crate::app::Tab::StructureEditor, w1);
    let at = on_screen(&mut h, w1);
    let none = egui::Modifiers::NONE;
    pointer(&mut h, at, None, none);
    h.run();
    pointer(&mut h, at, Some(true), none);
    for k in 1..=6 {
        pointer(&mut h, at + egui::vec2(15.0, -5.0) * k as f32, None, none);
    }
    let readout = h
        .state_mut()
        .ready()
        .unwrap()
        .view3d
        .handles
        .readout()
        .unwrap();
    assert!(readout.contains("lengths kept: S1 3.500 m"), "{readout}");
    pointer(&mut h, at + egui::vec2(90.0, -30.0), Some(false), none);
    h.run();
    let w1_now = {
        let r = h.state_mut().ready().unwrap();
        let status = r.status.clone().unwrap();
        assert!(
            !status.1 && status.0.starts_with("moved wire point W1"),
            "{status:?}"
        );
        let s = r.model.line.structure("T3").unwrap();
        assert_eq!(s.point("A1").unwrap().position, a1);
        let w = s.wire("W1").unwrap().position;
        assert!((distance(a1, w) - 3.5).abs() < 1e-9, "{}", distance(a1, w));
        assert!((s.string("S1").unwrap().length - 3.5).abs() < 1e-9);
        assert!(distance(w, w1) > 0.3, "{w:?} {w1:?}");
        w
    };
    // Ctrl held: the same.
    h.run();
    let at = on_screen(&mut h, w1_now);
    let ctrl = egui::Modifiers::COMMAND;
    pointer(&mut h, at, None, ctrl);
    h.run();
    pointer(&mut h, at, Some(true), ctrl);
    for k in 1..=6 {
        pointer(&mut h, at + egui::vec2(-10.0, 12.0) * k as f32, None, ctrl);
    }
    let readout = h
        .state_mut()
        .ready()
        .unwrap()
        .view3d
        .handles
        .readout()
        .unwrap();
    assert!(readout.contains("lengths kept: S1 3.500 m"), "{readout}");
    pointer(&mut h, at + egui::vec2(-60.0, 72.0), Some(false), ctrl);
    h.input_mut()
        .events
        .push(egui::Event::ModifiersChanged(none));
    h.run();
    let r = h.state_mut().ready().unwrap();
    let s = r.model.line.structure("T3").unwrap();
    let w = s.wire("W1").unwrap().position;
    assert!((distance(a1, w) - 3.5).abs() < 1e-9, "{}", distance(a1, w));
    assert!(distance(w, w1_now) > 0.3, "{w:?} {w1_now:?}");
}
