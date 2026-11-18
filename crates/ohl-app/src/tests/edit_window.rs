// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: a placed structure's Edit window (owner, 2026-11-18) — a click
//! in its view shows the object's row; no Wire points table; a string set
//! by its two angles; its parameters from its library entry, a library
//! entry made from a string without one.

use super::*;
use crate::placed_strings::{angles, turn_wire};
use crate::string_drag::distance;
use crate::type_workspace::{Handle, Sheet};

fn gpu() -> bool {
    let gpu = ohl_render::headless_device().is_ok();
    assert!(
        gpu || ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
        "no GPU"
    );
    gpu
}

/// The app on `model`, T3's Edit window open.
fn edit_window(model: model::Model) -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(2200.0, 1000.0))
        .wgpu()
        .build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    h.run();
    h.state_mut().ready().unwrap().selection = Selection::Structure("T3".into());
    h.run();
    h.get_by_label(crate::properties::STRINGS_AND_POINTS)
        .click();
    h.run();
    h.run();
    h
}

/// The window's geometry (its document, the project's insulators).
fn geometry(
    h: &mut Harness<'_, App>,
) -> (
    ohl_structures::StructureType,
    crate::type_workspace::Geometry,
) {
    let r = h.state_mut().ready().unwrap();
    let empty = ohl_structures::InsulatorLibrary::default();
    let insulators = r.model.types.as_ref().map_or(&empty, |(_, i)| i);
    let t = r
        .placed
        .as_ref()
        .unwrap()
        .workspace
        .shown()
        .unwrap()
        .clone();
    let g = crate::type_workspace::geometry(&t, insulators);
    (t, g)
}

fn click(h: &mut Harness<'_, App>, at: egui::Pos2) {
    h.input_mut().events.push(egui::Event::PointerMoved(at));
    h.step();
    for pressed in [true, false] {
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
    }
    h.run();
}

/// Where `p` (the structure's axes) is drawn in the window's view.
fn screen(h: &mut Harness<'_, App>, p: [f64; 3]) -> egui::Pos2 {
    let r = h.state_mut().ready().unwrap();
    r.placed.as_ref().unwrap().workspace.screen_of(p).unwrap()
}

/// A click on an attachment point, a wire point, a string in the view
/// shows its table, its row picked and its first value focused; the
/// window has no Wire points table.
#[test]
fn a_click_in_the_view_shows_the_row() {
    if !gpu() {
        return;
    }
    let mut h = edit_window(synthetic_project("edit-window-click"));
    // No Wire points table: asked for, the Strings table.
    h.state_mut()
        .ready()
        .unwrap()
        .placed
        .as_mut()
        .unwrap()
        .workspace
        .sheet = Sheet::Wires;
    h.run();
    h.run();
    assert_eq!(
        h.state_mut()
            .ready()
            .unwrap()
            .placed
            .as_ref()
            .unwrap()
            .workspace
            .sheet,
        Sheet::Strings
    );
    let (_, g) = geometry(&mut h);
    let focused = |h: &mut Harness<'_, App>| h.ctx.memory(|m| m.focused()).is_some();
    let state = |h: &mut Harness<'_, App>| {
        let w = &h
            .state_mut()
            .ready()
            .unwrap()
            .placed
            .as_ref()
            .unwrap()
            .workspace;
        (w.sheet, w.selected.clone())
    };
    // The Strings table first, then A1 clicked: the Attachment points.
    h.state_mut()
        .ready()
        .unwrap()
        .placed
        .as_mut()
        .unwrap()
        .workspace
        .sheet = Sheet::Strings;
    h.run();
    let at = screen(&mut h, g.at("A1").unwrap());
    click(&mut h, at);
    assert_eq!(
        state(&mut h),
        (Sheet::Points, vec![Handle::Point("A1".into())])
    );
    assert!(focused(&mut h), "its x focused");
    // W1: its string's row (no Wire points table).
    let at = screen(&mut h, g.at("W1").unwrap());
    click(&mut h, at);
    assert_eq!(
        state(&mut h),
        (Sheet::Strings, vec![Handle::Wire("W1".into())])
    );
    assert!(focused(&mut h), "its angle from vertical focused");
    // The user guide's picture (OHL_RENDER_PNG: its folder): W1 clicked,
    // its string's row and angles shown.
    if let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG") {
        h.run();
        let image = h.render().unwrap();
        let path = PathBuf::from(dir).join("ohl-design-edit-window.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
    // The middle of S2: its row, its wire point picked.
    let (a, b) = (g.at("A2").unwrap(), g.at("W2").unwrap());
    let mid = [0, 1, 2].map(|k| 0.5 * (a[k] + b[k]));
    let at = screen(&mut h, mid);
    click(&mut h, at);
    assert_eq!(
        state(&mut h),
        (Sheet::Strings, vec![Handle::Wire("W2".into())])
    );
    // A row chosen picks its object in the view (two-way).
    h.get_by_label("S1").click_accesskit();
    h.run();
    assert_eq!(state(&mut h).1, vec![Handle::Wire("W1".into())]);
    // Only looked at: nothing changed.
    {
        let p = h.state_mut().ready().unwrap().placed.as_ref().unwrap();
        assert!(
            !p.dirty(),
            "a row focused and left changes nothing: {:?}\n{}",
            p.workspace.message,
            p.doc
        );
    }
}

/// A string's two angles: typed, its wire point goes where they say, the
/// length and the attachment point kept, what it says read back to 1e-9°;
/// a V's are shown, not typed, and say why.
#[test]
fn a_string_is_set_by_its_angles() {
    if !gpu() {
        return;
    }
    let model = crate::tests::synthetic_project_with("edit-window-angles", |line| {
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
    });
    let mut h = edit_window(model);
    h.state_mut()
        .ready()
        .unwrap()
        .placed
        .as_mut()
        .unwrap()
        .workspace
        .sheet = Sheet::Strings;
    h.run();
    // The V: its legs' angles shown, said why.
    let v = h.get_all_by_label_contains("° (V)").count();
    assert_eq!(v, 4, "S2 and S2b: both angles shown, not typed");
    let (t, g) = geometry(&mut h);
    let (a1, w1) = (g.at("A1").unwrap(), g.at("W1").unwrap());
    let length = distance(a1, w1);
    // 30° from vertical, compass 75°.
    let (dn, dr) = (30.0, 75.0);
    let u = ohl_structures::structures::local_direction(dn, dr);
    let to = [0, 1, 2].map(|k| a1[k] + length * u[k]);
    {
        let p = h.state_mut().ready().unwrap().placed.as_mut().unwrap();
        turn_wire(&mut p.doc, &t, &g, "W1", to).unwrap();
        p.workspace.redraw();
    }
    h.run();
    let (_, g) = geometry(&mut h);
    let (a1b, w1b) = (g.at("A1").unwrap(), g.at("W1").unwrap());
    assert_eq!(a1b, a1, "its attachment point kept");
    assert!((distance(a1, w1b) - length).abs() < 1e-9);
    assert!(distance(w1b, to) < 1e-9, "{w1b:?} {to:?}");
    let (down, compass) = angles(a1, w1b);
    assert!(
        (down - dn).abs() < 1e-9 && (compass - dr).abs() < 1e-9,
        "{down} {compass}"
    );
    // Applied: on the line, the same.
    h.get_by_label("Apply").click();
    h.run();
    let r = h.state_mut().ready().unwrap();
    let s = r.model.line.structure("T3").unwrap();
    let local = |p| ohl_structures::structures::to_local(s.position, s.orientation_deg, p);
    let (a, w) = (
        local(s.point("A1").unwrap().position),
        local(s.wire("W1").unwrap().position),
    );
    let (down, compass) = angles(a, w);
    assert!(
        (down - dn).abs() < 1e-6 && (compass - dr).abs() < 1e-6,
        "{down} {compass}"
    );
    assert!((distance(a, w) - length).abs() < 1e-6);
}

/// A string without a library entry: its values read-only, said; **Make
/// a library entry from this string** makes one (one command) and the
/// string names it — every case solved as before, to the last digit.
#[test]
fn a_library_entry_from_a_string() {
    if !gpu() {
        return;
    }
    let mut h = edit_window(synthetic_project("edit-window-entry"));
    h.state_mut()
        .ready()
        .unwrap()
        .placed
        .as_mut()
        .unwrap()
        .workspace
        .sheet = Sheet::Strings;
    h.run();
    let (before, entries) = {
        let r = h.state_mut().ready().unwrap();
        let shapes: Vec<_> = r
            .model
            .cases
            .iter()
            .map(|c| c.ready.then(|| c.shapes.clone()))
            .collect();
        let log = r.model.session().unwrap().log().unwrap().len();
        (shapes, log)
    };
    h.get_all_by_label_contains("not from a library entry")
        .next()
        .unwrap();
    h.get_all_by_label(crate::placed_strings::MAKE_ENTRY)
        .next()
        .unwrap()
        .click_accesskit();
    h.run();
    let r = h.state_mut().ready().unwrap();
    assert_eq!(
        r.model.session().unwrap().log().unwrap().len(),
        entries + 1,
        "one command: {:?} {:?}",
        r.status,
        r.placed.as_ref().unwrap().message
    );
    let s = r.model.stored_line().unwrap().structure("T3").unwrap();
    let id = s.string("S1").unwrap().insulator.clone().expect("named");
    assert!(id.starts_with("Suspension-3.500m-900N"), "{id}");
    let a = r
        .model
        .types
        .as_ref()
        .unwrap()
        .1
        .get(&id)
        .expect("in the library");
    assert_eq!(a.length.si(), 3.5);
    assert_eq!(a.weight.si(), 900.0);
    for (k, c) in r.model.cases.iter().enumerate().filter(|(_, c)| c.ready) {
        let full = model::Solved::new(&r.model.line, &r.model.library, c.case.clone());
        assert_eq!(c.shapes, full.shapes, "{}", c.case.name);
        if let Some(Some(b)) = before.get(k) {
            assert_eq!(&c.shapes, b, "{}: the results unchanged", c.case.name);
        }
    }
}
