// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Drag handles in the line's 3D view ([`crate::handles`]): a drag in the
//! window moves a point in one command (one undo), held to an axis,
//! snapped; Escape cancels; a point of a type becomes the structure's
//! own; the body is not dragged.

use eframe::egui;
use egui_kittest::Harness;
use ohl_structure_edit::Handle;
use ohl_structures::structures::to_local;

use crate::app::{App, Place, Tab};
use crate::handles::{Constraint, Spot};
use crate::selection::Selection;
use crate::tests::{press, synthetic_project, typed_project};

fn gpu() -> bool {
    let gpu = ohl_render::headless_device().is_ok();
    assert!(
        gpu || ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
        "no GPU"
    );
    gpu
}

fn harness(model: crate::model::Model) -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .wgpu()
        .build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    h.run();
    h
}

/// The page of the structure screen moved to the right (with
/// Properties), the 3D view looking at `p` from close by.
fn screen_aside(h: &mut Harness<'_, App>, page: Tab, p: [f64; 3]) {
    crate::app::move_page(h.state_mut().dock(), page, Place::Right);
    h.run();
    let r = h.state_mut().ready().unwrap();
    let cam = r.view3d.camera.as_mut().unwrap();
    cam.target = p;
    cam.distance = 40.0;
    h.run();
}

/// Where `p` is on the screen (the 3D view as last drawn).
fn on_screen(h: &mut Harness<'_, App>, p: [f64; 3]) -> egui::Pos2 {
    let r = h.state_mut().ready().unwrap();
    let rect = r.view3d.rect.unwrap();
    let cam = r.view3d.camera.unwrap();
    let size = [f64::from(rect.width()), f64::from(rect.height())];
    let (q, _) = cam.project(p, size).unwrap();
    rect.min + egui::vec2(q[0] as f32, q[1] as f32)
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

/// A drag from `from` by `by` pixels, in steps; `keys` pressed once the
/// drag has started (before it moves).
fn drag(h: &mut Harness<'_, App>, from: egui::Pos2, by: egui::Vec2, keys: &[(egui::Key, bool)]) {
    pointer(h, from, None);
    pointer(h, from, Some(true));
    for &(key, shift) in keys {
        let modifiers = if shift {
            egui::Modifiers::SHIFT
        } else {
            egui::Modifiers::NONE
        };
        for pressed in [true, false] {
            h.input_mut().events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            });
        }
        h.step();
    }
    for k in 1..=6 {
        pointer(h, from + by * (k as f32 / 6.0), None);
    }
}

/// A structure without a type, its structure editor open: the pointer
/// over a point highlights it; dragged with Z it moves straight up, its
/// height snapped to the 5 cm grid, the readout says where; released, one
/// command (the editor shows it, nothing left to apply); one undo takes
/// it back exactly. Escape during another drag changes nothing.
#[test]
fn a_point_dragged_along_z_is_one_command_and_one_undo() {
    if !gpu() {
        return;
    }
    let mut h = harness(synthetic_project("handles-placed"));
    h.state_mut().ready().unwrap().selection = Selection::Structure("T3".into());
    h.run();
    // Selected, its structure screen not open: no handles.
    assert_eq!(h.state_mut().ready().unwrap().view3d.handles.target, None);
    let (label, before, stored_before) = {
        let r = h.state_mut().ready().unwrap();
        r.edit_structure("T3");
        let s = r.model.line.structure("T3").unwrap();
        let p = s.points[0].clone();
        (p.label, p.position, r.model.stored_line().unwrap().clone())
    };
    h.run();
    screen_aside(&mut h, Tab::StructureEditor, before);
    let target = h.state_mut().ready().unwrap().view3d.handles.target.clone();
    assert_eq!(
        target
            .as_ref()
            .map(|t| (t.structure.as_str(), t.grid, t.blocked.is_none())),
        Some(("T3", 0.05, true)),
        "{target:?}"
    );
    let at = on_screen(&mut h, before);
    pointer(&mut h, at, None);
    h.run();
    assert_eq!(
        h.state_mut().ready().unwrap().view3d.handles.hovered,
        Some(Handle::Point(label.clone()))
    );
    // Held to z, 60 px up.
    drag(&mut h, at, egui::vec2(0.0, -60.0), &[(egui::Key::Z, false)]);
    {
        let r = h.state_mut().ready().unwrap();
        let d = r.view3d.handles.drag.as_ref().expect("dragging");
        assert_eq!(d.constraint, Constraint::Axis(2));
        let readout = r.view3d.handles.readout().unwrap();
        assert!(
            readout.starts_with(&format!("point {label} of T3: x ")),
            "{readout}"
        );
        assert!(readout.contains("along z"), "{readout}");
    }
    pointer(&mut h, at + egui::vec2(0.0, -60.0), Some(false));
    h.run();
    let r = h.state_mut().ready().unwrap();
    let s = r.model.line.structure("T3").unwrap();
    let after = s.point(&label).unwrap().position;
    let (lb, la) = (
        to_local(s.position, s.orientation_deg, before),
        to_local(s.position, s.orientation_deg, after),
    );
    assert!(la[2] > lb[2] + 0.5, "{lb:?} ➡ {la:?}");
    assert!(
        (la[0] - lb[0]).abs() < 1e-9 && (la[1] - lb[1]).abs() < 1e-9,
        "{lb:?} ➡ {la:?}"
    );
    assert!(
        ((la[2] / 0.05).round() * 0.05 - la[2]).abs() < 1e-9,
        "{la:?}"
    );
    let status = r.status.clone().unwrap();
    assert!(!status.1, "{status:?}");
    assert!(
        status.0.starts_with(&format!("moved point {label} of T3")),
        "{status:?}"
    );
    // The command logged as one entry; the editor shows the line as it is.
    let p = r.placed.as_ref().unwrap();
    assert!(!p.dirty());
    assert!(p.doc.to_string().contains(&format!("{:.3} m", la[2])));
    // The other points and the strings are as they were; the string
    // hanging from it, given by its length and direction, takes its wire
    // point along (as the line places it), the others stay.
    let stored = r.model.stored_line().unwrap().structure("T3").unwrap();
    let was = stored_before.structure("T3").unwrap();
    // (Strings without a library label get one in the same command, as
    // after every edit putting a structure.)
    let geometry = |s: &ohl_line::Structure| -> Vec<(String, String, String, f64, f64, f64)> {
        s.strings
            .iter()
            .map(|x| {
                let x = x.clone();
                (
                    x.label,
                    x.top,
                    x.wire,
                    x.length,
                    x.angle_deg,
                    x.direction_deg,
                )
            })
            .collect()
    };
    assert_eq!(geometry(stored), geometry(was));
    let lift = after[2] - before[2];
    for (a, b) in stored.wires.iter().zip(&was.wires) {
        let hangs = was
            .strings
            .iter()
            .any(|x| x.top == label && x.wire == a.label);
        let dz = if hangs { lift } else { 0.0 };
        assert!(
            (a.position[2] - b.position[2] - dz).abs() < 1e-9,
            "{a:?} {b:?}"
        );
    }
    for (a, b) in stored.points.iter().zip(&was.points) {
        assert_eq!(a.label, b.label);
        if a.label != label {
            assert_eq!(a.position, b.position);
        }
    }
    // One undo: the line as before, to the last digit.
    assert!(r.perform(crate::edit::Action::Undo));
    assert_eq!(
        r.model.stored_line().unwrap().structure("T3"),
        stored_before.structure("T3")
    );
    // Escape in the middle of a drag: nothing changes.
    let before_esc = r.model.stored_line().unwrap().clone();
    h.run();
    let at = on_screen(&mut h, before);
    drag(&mut h, at, egui::vec2(30.0, -40.0), &[]);
    assert!(h.state_mut().ready().unwrap().view3d.handles.drag.is_some());
    press(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    pointer(&mut h, at + egui::vec2(30.0, -40.0), Some(false));
    h.run();
    let r = h.state_mut().ready().unwrap();
    assert!(r.view3d.handles.drag.is_none());
    assert_eq!(r.model.stored_line().unwrap(), &before_esc);
    assert_eq!(
        r.status.as_ref().unwrap().0,
        "drag cancelled: nothing changed"
    );
}

/// A typed structure, its type on the Structure types page: a point of
/// the type dragged along x becomes the structure's own (the status says
/// so) and the type's file is not touched; undo takes the override away.
#[test]
fn a_point_of_the_type_dragged_becomes_the_structures_own() {
    if !gpu() {
        return;
    }
    let mut h = harness(typed_project("handles-typed"));
    let before = {
        let r = h.state_mut().ready().unwrap();
        r.selection = Selection::Structure("T3".into());
        r.edit_type("type-03");
        assert!(
            r.model
                .stored_line()
                .unwrap()
                .structure("T3")
                .unwrap()
                .points
                .is_empty()
        );
        r.model
            .line
            .structure("T3")
            .unwrap()
            .point("A1")
            .unwrap()
            .position
    };
    h.run();
    let kind = crate::library::LibraryKind::StructureTypes;
    screen_aside(&mut h, Tab::Library(kind), before);
    let file = {
        let r = h.state_mut().ready().unwrap();
        assert_eq!(
            r.view3d
                .handles
                .target
                .as_ref()
                .map(|t| t.structure.as_str()),
            Some("T3")
        );
        r.libraries[&kind].file.clone().unwrap()
    };
    let text = std::fs::read_to_string(&file).unwrap();
    let at = on_screen(&mut h, before);
    drag(&mut h, at, egui::vec2(60.0, 25.0), &[(egui::Key::X, false)]);
    assert_eq!(
        h.state_mut()
            .ready()
            .unwrap()
            .view3d
            .handles
            .drag
            .as_ref()
            .map(|d| d.constraint),
        Some(Constraint::Axis(0))
    );
    pointer(&mut h, at + egui::vec2(60.0, 25.0), Some(false));
    h.run();
    let r = h.state_mut().ready().unwrap();
    let status = r.status.clone().unwrap();
    assert!(
        status
            .0
            .contains("point A1 now the structure's own, over its type's"),
        "{status:?}"
    );
    let s = r.model.line.structure("T3").unwrap();
    let (lb, la) = (
        to_local(s.position, s.orientation_deg, before),
        to_local(
            s.position,
            s.orientation_deg,
            s.point("A1").unwrap().position,
        ),
    );
    assert!((la[0] - lb[0]).abs() > 0.2, "{lb:?} ➡ {la:?}");
    assert!((la[1] - lb[1]).abs() < 1e-9 && (la[2] - lb[2]).abs() < 1e-9);
    let own = r.model.stored_line().unwrap().structure("T3").unwrap();
    assert_eq!(own.points.len(), 1);
    assert_eq!(own.points[0].label, "A1");
    assert!(own.strings.is_empty() && own.wires.is_empty());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
    assert!(r.perform(crate::edit::Action::Undo));
    assert!(
        r.model
            .stored_line()
            .unwrap()
            .structure("T3")
            .unwrap()
            .points
            .is_empty()
    );
    assert_eq!(
        r.model
            .line
            .structure("T3")
            .unwrap()
            .point("A1")
            .unwrap()
            .position,
        before
    );
}

/// The commands themselves: a wire point of a type moves with its string
/// window's strings and wire points, made the structure's own; the body
/// (a point on a joint, a joint) is refused with the reason; without a
/// structure screen open there is no target; the snapping and the
/// constraints.
#[test]
fn wire_points_override_their_strings_and_the_body_is_not_dragged() {
    let m = typed_project("handles-commands");
    // W1 of T3 (type-03), 0.5 m across.
    let s = m.line.structure("T3").unwrap();
    let w = to_local(
        s.position,
        s.orientation_deg,
        s.wire("W1").unwrap().position,
    );
    let to = [w[0] + 0.5, w[1], w[2]];
    let (what, command) =
        crate::handles::command(&m, "T3", &Handle::Wire("W1".into()), to).unwrap();
    assert!(
        what.contains("wire point W1, string S1 now the structure's own"),
        "{what}"
    );
    let ohl_session::Command::Put { value, .. } = command else {
        panic!("{command:?}")
    };
    let put: ohl_line::Structure = serde_json::from_value(value).unwrap();
    assert_eq!(put.wires.len(), 1);
    assert_eq!(put.strings.len(), 1);
    assert_eq!(put.wires[0].set, Some(1));
    let got = to_local(put.position, put.orientation_deg, put.wires[0].position);
    assert!(
        (0..3).all(|k| (got[k] - to[k]).abs() < 1e-9),
        "{got:?} {to:?}"
    );
    // T2 (type-02, level 3): its points sit on joints; joints are the body.
    let spots = crate::handles::spots(&m, "T2");
    let joint = spots
        .iter()
        .find(|x| matches!(x.handle, Handle::Joint(_)))
        .expect("joints shown");
    assert!(joint.fixed.as_ref().unwrap().contains("OHL-Structure"));
    let a1: &Spot = spots
        .iter()
        .find(|x| x.handle == Handle::Point("A1".into()))
        .unwrap();
    assert!(a1.fixed.as_ref().unwrap().contains("sits on joint A1"));
    let e = crate::handles::command(&m, "T2", &a1.handle, [0.0, 0.0, 40.0]).unwrap_err();
    assert!(e.contains("moves with the body"), "{e}");
    // Each spot where the line has it.
    let t2 = m.line.structure("T2").unwrap();
    assert_eq!(a1.global, t2.point("A1").unwrap().position);
    // Snapping: the grid in the free directions only; onto another
    // handle there.
    let (p, on) = crate::handles::snap([1.234, 2.345, 3.456], Constraint::Axis(2), 0.05, None);
    assert_eq!(p[0], 1.234);
    assert_eq!(p[1], 2.345);
    assert!((p[2] - 3.45).abs() < 1e-12);
    assert_eq!(on.as_deref(), Some("the 0.05 m grid"));
    let (p, on) = crate::handles::snap([1.234, 2.345, 3.456], Constraint::Plane(2), 0.05, Some(a1));
    assert_eq!(p, [a1.local[0], a1.local[1], 3.456]);
    assert_eq!(on.as_deref(), Some("point A1"));
    assert_eq!(
        Constraint::Free.toggled(0, false).toggled(0, false),
        Constraint::Free
    );
    assert_eq!(Constraint::Axis(0).toggled(2, true), Constraint::Plane(2));
    assert_eq!(Constraint::Plane(2).describe(), "in the xy plane");
    // Along an axis: the point of the axis nearest the ray.
    let from = [1.0, 2.0, 3.0];
    let o = ohl_structures::structures::to_global(t2.position, t2.orientation_deg, from);
    let e = crate::handles::axis(t2.orientation_deg, 1);
    // A ray straight down through the axis 4 m along.
    let a = [o[0] + 4.0 * e[0], o[1] + 4.0 * e[1], o[2] + 50.0];
    let got = crate::handles::drag_to(
        t2,
        from,
        Constraint::Axis(1),
        [0.0, 0.0, 1.0],
        a,
        [0.0, 0.0, -1.0],
    )
    .unwrap();
    assert!(
        (got[1] - 6.0).abs() < 1e-9 && got[0] == 1.0 && got[2] == 3.0,
        "{got:?}"
    );
    // Escape without a drag: nothing to cancel.
    let mut handles = crate::handles::Handles::default();
    handles.cancel();
    assert_eq!(handles.done, None);
}
