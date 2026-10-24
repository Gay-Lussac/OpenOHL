// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: edits through the session, undo, the hotbar, the tools' clicks.

use super::*;

/// The example as a project edited by "tester".
pub(crate) fn synthetic_project(name: &str) -> model::Model {
    synthetic_project_with(name, |_| {})
}

/// [`synthetic_project`] with its line changed first.
pub(crate) fn synthetic_project_with(
    name: &str,
    edit: impl FnOnce(&mut ohl_line::Line),
) -> model::Model {
    let ex = root().join("examples/synthetic-line");
    let dir = std::env::temp_dir().join(format!("ohl-app-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ohl");
    std::fs::remove_file(&path).ok();
    let mut line = ohl_line::Line::load(&ex.join("synthetic-line.ohl.json")).unwrap();
    edit(&mut line);
    ohl_session::Session::open(&path)
        .unwrap()
        .import(&line, "tester")
        .unwrap();
    solved_all(
        model::load(
            Sources {
                file: path,
                library: Some(root().join("libraries/cables")),
                weather: Some(ex.join("weather-cases.ohlwc")),
                load_cases: Some(ex.join("load-cases.ohllc")),
                point_budget: 1000,
                corridor: 60.0,
                user: Some("tester".into()),
                ..Sources::default()
            },
            &|_| {},
        )
        .unwrap(),
    )
}

/// Every case solved equals a full solve of the current line.
pub(super) fn assert_solved_fully(m: &model::Model) {
    for c in m.cases.iter().filter(|c| c.ready) {
        let full = model::Solved::new(&m.line, &m.library, c.case.clone());
        assert_eq!(c.shapes, full.shapes, "{}", c.case.name);
        assert_eq!(c.jumpers, full.jumpers, "{}", c.case.name);
    }
}

#[test]
fn edits_go_through_the_session_and_re_solve_what_they_touch() {
    use crate::edit::EditError;
    use ohl_session::Command;
    let mut m = synthetic_project("edit");
    assert!(m.can_edit().is_ok());
    let p = m.line.structure("T6").unwrap().position;
    let report = m
        .apply(
            "moved T6",
            Command::MoveStructure {
                id: "T6".into(),
                position: [p[0] + 4.0, p[1], p[2]],
            },
        )
        .unwrap();
    // T6 is on the branch only: S3 touched, re-solved in the case shown;
    // the 4 others (not shown, not wanted lately) to solve again when
    // wanted, then as a full solve.
    assert!(
        report.contains("1 sections touched, 1 re-solved")
            && report.contains("4 others to solve again when wanted"),
        "{report}"
    );
    assert_eq!(m.line.structure("T6").unwrap().position[0], p[0] + 4.0);
    assert_eq!(m.cases.iter().filter(|c| c.ready).count(), 1);
    assert_solved_fully(&m);
    for k in 0..m.cases.len() {
        m.ensure_solved(k);
    }
    assert_solved_fully(&m);
    // Cases wanted lately by a view are brought up to date at once.
    let now = std::time::Instant::now();
    m.recently_wanted = (0..m.cases.len()).map(|k| (k, now)).collect();
    m.undo().unwrap();
    assert_eq!(m.line.structure("T6").unwrap().position, p);
    assert_solved_fully(&m);
    m.redo().unwrap();
    assert_eq!(m.line.structure("T6").unwrap().position[0], p[0] + 4.0);
    // Deleting T7 takes S3 and the jumper: a confirmation first.
    let del = |confirm| Command::Delete {
        kind: ohl_session::Kind::Structure,
        id: "T7".into(),
        confirm,
    };
    match m.apply("deleted T7", del(false)) {
        Err(EditError::NeedsConfirmation(i)) => {
            assert_eq!(i.sections, vec!["S3".to_owned()]);
        }
        other => panic!("{other:?}"),
    }
    assert!(m.line.structure("T7").is_some());
    m.apply("deleted T7", del(true)).unwrap();
    assert!(m.line.structure("T7").is_none());
    assert!(m.line.sections.iter().all(|s| s.id != "S3"));
    assert_solved_fully(&m);
    m.undo().unwrap();
    assert!(m.line.structure("T7").is_some());
    assert_solved_fully(&m);
    // A preview moves the line in memory only; cancelling restores it.
    let q = m.line.structure("T2").unwrap().position;
    m.preview_move(0, "T2", [q[0], q[1] + 3.0, q[2]]);
    assert_eq!(m.line.structure("T2").unwrap().position[1], q[1] + 3.0);
    m.cancel_preview(0, "T2");
    assert_eq!(m.line.structure("T2").unwrap().position, q);
    assert_solved_fully(&m);
    // A line file is read-only.
    assert!(synthetic().can_edit().unwrap_err().contains("line file"));
}

#[test]
fn command_z_undoes_in_the_window() {
    let mut model = synthetic_project("window");
    let p = model.line.structure("T6").unwrap().position;
    model
        .apply(
            "moved T6",
            ohl_session::Command::MoveStructure {
                id: "T6".into(),
                position: [p[0] + 4.0, p[1], p[2]],
            },
        )
        .unwrap();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.run();
    // Status bar and the load log.
    assert!(harness.query_all_by_label("editing as tester").count() >= 1);
    for pressed in [true, false] {
        harness.input_mut().events.push(egui::Event::Key {
            key: egui::Key::Z,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        });
    }
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.model.line.structure("T6").unwrap().position, p);
    assert!(r.status.as_ref().unwrap().0.starts_with("undo"));
}

pub(crate) fn press(h: &mut Harness<'_, App>, key: egui::Key, modifiers: egui::Modifiers) {
    for pressed in [true, false] {
        h.input_mut().events.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        });
    }
    h.run();
}

/// G works after a click in the 3D view (which takes the keyboard focus
/// for W A S D) — it did not.
#[test]
fn g_works_after_clicking_the_3d_view() {
    if ohl_render::headless_device().is_err() {
        assert!(
            ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
            "no GPU"
        );
        return;
    }
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .wgpu()
        .build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.run();
    let at = egui::pos2(620.0, 300.0);
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(at));
    for pressed in [true, false] {
        harness.input_mut().events.push(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    harness.run();
    press(&mut harness, egui::Key::G, egui::Modifiers::NONE);
    assert!(harness.state_mut().ready().unwrap().goto.is_some());
}

/// M holds the Move tool; a structure clicked with it opens the Move
/// window; typed values preview live; Enter applies one command; the
/// hotbar takes 1–9; E opens the inventory.
#[test]
fn the_move_tool_and_the_hotbar() {
    use crate::tools::Tool;
    let model = synthetic_project("move-tool");
    let mut builder = Harness::builder().with_size(egui::vec2(1400.0, 900.0));
    if ohl_render::headless_device().is_ok() {
        builder = builder.wgpu();
    }
    let mut harness =
        builder.build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.run();
    press(&mut harness, egui::Key::M, egui::Modifiers::NONE);
    assert_eq!(
        harness.state_mut().ready().unwrap().hotbar.tool(),
        Tool::Move
    );
    let p = {
        let r = harness.state_mut().ready().unwrap();
        r.use_tool_on("T6");
        r.move_dialog.as_ref().unwrap().start
    };
    harness.run();
    harness.get_by_label("Move structure T6");
    // Type 5 m east: previewed, not yet a command.
    harness
        .state_mut()
        .ready()
        .unwrap()
        .move_dialog
        .as_mut()
        .unwrap()
        .position[0] = p[0] + 5.0;
    harness.run();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(
            r.model.line.structure("T6").unwrap().position[0],
            p[0] + 5.0
        );
        let log = r.model.editing.as_ref().unwrap().session.log().unwrap();
        // (The library files copied in when it opened: the program's.)
        let edits = log.iter().filter(|e| e.kind != "library").count();
        assert_eq!(edits, 1, "only the import so far");
    }
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.move_dialog.is_none());
        let log = r.model.editing.as_ref().unwrap().session.log().unwrap();
        let edits = log.iter().filter(|e| e.kind != "library").count();
        assert_eq!(edits, 2, "one move command");
        assert_eq!(
            r.model.line.structure("T6").unwrap().position[0],
            p[0] + 5.0
        );
    }
    // A cancelled window puts the structure back.
    let q = {
        let r = harness.state_mut().ready().unwrap();
        r.use_tool_on("T2");
        r.move_dialog.as_ref().unwrap().start
    };
    harness
        .state_mut()
        .ready()
        .unwrap()
        .move_dialog
        .as_mut()
        .unwrap()
        .position[1] = q[1] + 2.0;
    harness.run();
    press(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.move_dialog.is_none());
        assert_eq!(r.model.line.structure("T2").unwrap().position, q);
    }
    // Hotbar keys and the inventory.
    press(&mut harness, egui::Key::Num3, egui::Modifiers::NONE);
    assert_eq!(
        harness.state_mut().ready().unwrap().hotbar.tool(),
        Tool::Delete
    );
    press(&mut harness, egui::Key::E, egui::Modifiers::NONE);
    harness.get_by_label("Inventory — tools");
    if ohl_render::headless_device().is_ok()
        && let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG")
    {
        let image = harness.render().unwrap();
        let path = PathBuf::from(dir).join("ohl-design-inventory.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
    // Escape closes it.
    press(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(!harness.state_mut().ready().unwrap().inventory);
    assert!(harness.query_by_label("Inventory — tools").is_none());
    // The hotbar sits at the bottom of the 3D view, not of the window.
    let view = harness.state_mut().ready().unwrap().view3d.rect.unwrap();
    let bar = harness
        .ctx
        .memory(|m| m.area_rect(egui::Id::new("hotbar")))
        .unwrap();
    assert!(view.contains_rect(bar), "{bar:?} in {view:?}");
    assert!(view.bottom() - bar.bottom() < 20.0, "{bar:?} in {view:?}");
}

#[test]
fn the_hotbar_is_remembered() {
    let mut h = crate::tools::Hotbar::default();
    h.slots.swap(0, 4);
    h.active = 2;
    let mut storage = Memory::default();
    h.save(&mut storage);
    assert_eq!(crate::tools::Hotbar::load(&storage), Some(h));
}

/// The hotbar and the inventory show the tool icons of `assets/` (images,
/// no glyphs of the font, which showed as □ for several tools); the
/// Tools menu too.
#[test]
fn the_hotbar_shows_image_icons() {
    use crate::tools::Tool;
    let gpu = ohl_render::headless_device().is_ok();
    assert!(
        gpu || ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
        "no GPU"
    );
    let mut builder = Harness::builder().with_size(egui::vec2(1400.0, 900.0));
    if gpu {
        builder = builder.wgpu();
    }
    let model = synthetic();
    let mut harness =
        builder.build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.run();
    let hotbar = harness.state_mut().ready().unwrap().hotbar.clone();
    let icons = |h: &Harness<'_, App>| -> Vec<String> {
        h.ctx
            .tex_manager()
            .read()
            .allocated()
            .filter_map(|(_, m)| m.name.strip_prefix("icon:").map(str::to_owned))
            .collect()
    };
    for t in hotbar.slots.iter().flatten() {
        harness.get_by_label(&format!("slot: {}", t.name()));
        assert!(icons(&harness).iter().any(|n| n == t.key_name()), "{t:?}");
    }
    for t in Tool::ALL {
        assert!(
            harness.query_by_label_contains(t.icon()).is_none(),
            "{t:?}: a glyph shown"
        );
    }
    // The inventory: every tool's icon.
    press(&mut harness, egui::Key::E, egui::Modifiers::NONE);
    harness.run();
    for t in Tool::ALL {
        assert!(icons(&harness).iter().any(|n| n == t.key_name()), "{t:?}");
    }
    if gpu && let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG") {
        let image = harness.render().unwrap();
        let path = PathBuf::from(dir).join("ohl-design-hotbar-inventory.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
    press(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    // The Tools menu: names without glyphs, the icons beside them.
    harness.get_by_label("Tools").click();
    harness.run();
    harness.get_by_label("Replace structure");
    if gpu && let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG") {
        let image = harness.render().unwrap();
        let path = PathBuf::from(dir).join("ohl-design-tools-menu.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
}

/// The start page shows the OpenOHL mark above the name.
#[test]
fn the_start_page_shows_the_mark() {
    let gpu = ohl_render::headless_device().is_ok();
    let mut builder = Harness::builder().with_size(egui::vec2(1000.0, 700.0));
    if gpu {
        builder = builder.wgpu();
    }
    let mut harness =
        builder.build_eframe(|cc| App::empty(Sources::default(), cc.wgpu_render_state.clone()));
    harness.run();
    harness.get_by_label("New project…");
    let mark = harness
        .ctx
        .tex_manager()
        .read()
        .allocated()
        .any(|(_, m)| m.name == "icon:openohl-mark");
    assert!(mark, "the mark on the start page");
    if gpu && let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG") {
        let image = harness.render().unwrap();
        let path = PathBuf::from(dir).join("ohl-design-start-page.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
}

/// Choosing a slot only holds the tool; a right-click uses it.
#[test]
fn tools_act_on_a_right_click_only() {
    use crate::tools::Tool;
    if ohl_render::headless_device().is_err() {
        assert!(
            ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
            "no GPU"
        );
        return;
    }
    let model = synthetic_project("right-click");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .wgpu()
        .build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.run();
    // Holding Go to (slot 4) opens nothing.
    press(&mut harness, egui::Key::Num4, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.hotbar.tool(), Tool::GoTo);
        assert!(r.goto.is_none());
    }
    // First person, crosshair on T3's first insulator point, Move held:
    // a left click selects, a right-click opens the Move window.
    harness.state_mut().ready().unwrap().view3d.controls =
        crate::controls::Controls::by_name("minecraft").unwrap();
    press(&mut harness, egui::Key::M, egui::Modifiers::NONE);
    let at = egui::pos2(620.0, 300.0);
    let click = |h: &mut Harness<'_, App>, button| {
        for pressed in [true, false] {
            h.input_mut().events.push(egui::Event::PointerButton {
                pos: at,
                button,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        h.run();
    };
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(at));
    harness.run();
    click(&mut harness, egui::PointerButton::Primary); // captures
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.view3d.captured);
        let p = r.model.line.structure("T3").unwrap().points[0].position;
        let cam = r.view3d.camera.as_mut().unwrap();
        cam.target = p;
        cam.distance = 80.0;
    }
    click(&mut harness, egui::PointerButton::Primary);
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.selection, Selection::Structure("T3".into()));
        assert!(r.move_dialog.is_none(), "a left click only selects");
    }
    click(&mut harness, egui::PointerButton::Secondary);
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.move_dialog.as_ref().map(|d| d.id.as_str()), Some("T3"));
        assert!(!r.view3d.captured, "the window frees the mouse");
        // It opens at the crosshair (the 3D view's centre).
        assert_eq!(r.popup_at, r.view3d.rect.unwrap().center());
    }
    press(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(harness.state_mut().ready().unwrap().move_dialog.is_none());
    // Delete held, first person again: the confirmation opens with the
    // mouse freed; Escape then cancels the deletion, nothing else.
    press(&mut harness, egui::Key::Num3, egui::Modifiers::NONE);
    click(&mut harness, egui::PointerButton::Primary); // captures again
    assert!(harness.state_mut().ready().unwrap().view3d.captured);
    click(&mut harness, egui::PointerButton::Secondary);
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.confirm.is_some(), "T3 takes sections with it");
        assert!(!r.view3d.captured, "the confirmation frees the mouse");
    }
    press(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    let r = harness.state_mut().ready().unwrap();
    assert!(r.confirm.is_none());
    assert!(r.model.line.structure("T3").is_some(), "not deleted");
}

/// A click of `button` at `at` (moved there first).
pub(super) fn click_at(h: &mut Harness<'_, App>, at: egui::Pos2, button: egui::PointerButton) {
    h.input_mut().events.push(egui::Event::PointerMoved(at));
    h.run();
    for pressed in [true, false] {
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    h.run();
}

/// Option "Tools act on left-click" in the plan: on, a left-click uses the
/// tool held and a right-click selects (Select held: a left-click
/// selects); off, as before. A drag still pans with either setting.
#[test]
fn tools_act_on_left_click_with_the_option() {
    use crate::tools::Tool;
    use egui::PointerButton::{Primary, Secondary};
    let model = synthetic_project("left-click");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.state_mut().show_tab(crate::app::Tab::Plan);
    harness.run();
    let at = {
        let r = harness.state_mut().ready().unwrap();
        let p = r.model.line.structure("T3").unwrap().position;
        r.plan.centre_on(p);
        r.plan.drawn.unwrap().center()
    };
    harness.run();
    press(&mut harness, egui::Key::M, egui::Modifiers::NONE);
    let reset = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        r.move_dialog = None;
        r.selection = Selection::None;
        h.run();
    };
    // Off (the default): a left-click selects, a right-click moves.
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.hotbar.tool(), Tool::Move);
        assert!(!r.view3d.controls.tools_on_left, "off by default");
    }
    click_at(&mut harness, at, Primary);
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.selection, Selection::Structure("T3".into()));
        assert!(r.move_dialog.is_none(), "a left click only selects");
    }
    reset(&mut harness);
    click_at(&mut harness, at, Secondary);
    assert_eq!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .move_dialog
            .as_ref()
            .map(|d| d.id.as_str()),
        Some("T3")
    );
    reset(&mut harness);
    // On, ticked in the Tools menu (which stays open on the tick).
    harness.get_by_label("Tools").click();
    harness.run();
    harness.get_by_label("Tools act on left-click").click();
    harness.run();
    assert!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .view3d
            .controls
            .tools_on_left
    );
    harness.get_by_label("Tools act on left-click");
    harness.key_press(egui::Key::Escape);
    harness.run();
    reset(&mut harness);
    click_at(&mut harness, at, Primary);
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.move_dialog.as_ref().map(|d| d.id.as_str()), Some("T3"));
    }
    reset(&mut harness);
    click_at(&mut harness, at, Secondary);
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.selection, Selection::Structure("T3".into()));
        assert!(r.move_dialog.is_none(), "a right-click only selects");
    }
    reset(&mut harness);
    // Select held: a left-click selects (as always).
    harness
        .state_mut()
        .ready()
        .unwrap()
        .hotbar
        .hold(Tool::Select);
    harness.run();
    click_at(&mut harness, at, Primary);
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.selection, Selection::Structure("T3".into()));
        assert!(r.move_dialog.is_none());
    }
    // A left drag still pans (no tool used).
    harness.state_mut().ready().unwrap().hotbar.hold(Tool::Move);
    reset(&mut harness);
    let before = harness.state_mut().ready().unwrap().plan.centre().unwrap();
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(at));
    harness.run();
    let press_at = |h: &mut Harness<'_, App>, pos: egui::Pos2, pressed: bool| {
        h.input_mut().events.push(egui::Event::PointerButton {
            pos,
            button: Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.run();
    };
    press_at(&mut harness, at, true);
    for k in 1..=5 {
        harness.input_mut().events.push(egui::Event::PointerMoved(
            at + egui::vec2(20.0 * k as f32, 0.0),
        ));
        harness.run();
    }
    press_at(&mut harness, at + egui::vec2(100.0, 0.0), false);
    let r = harness.state_mut().ready().unwrap();
    assert_ne!(r.plan.centre().unwrap(), before, "the drag panned");
    assert!(r.move_dialog.is_none(), "a drag is no click");
}

/// The option in the first-person 3D view: captured, a left-click uses the
/// tool at the crosshair and a right-click selects; the press that
/// captures the mouse does neither.
#[test]
fn tools_act_on_left_click_in_first_person() {
    use crate::tools::Tool;
    use egui::PointerButton::{Primary, Secondary};
    if ohl_render::headless_device().is_err() {
        assert!(
            ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
            "no GPU"
        );
        return;
    }
    let model = synthetic_project("left-click-3d");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .wgpu()
        .build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        r.view3d.controls = crate::controls::Controls::by_name("minecraft").unwrap();
        r.view3d.controls.tools_on_left = true;
    }
    press(&mut harness, egui::Key::M, egui::Modifiers::NONE);
    let at = egui::pos2(620.0, 300.0);
    click_at(&mut harness, at, Primary); // captures
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.view3d.captured);
        assert!(r.move_dialog.is_none(), "the capturing press uses no tool");
        assert_eq!(r.hotbar.tool(), Tool::Move);
        let p = r.model.line.structure("T3").unwrap().points[0].position;
        let cam = r.view3d.camera.as_mut().unwrap();
        cam.target = p;
        cam.distance = 80.0;
    }
    click_at(&mut harness, at, Secondary);
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.selection, Selection::Structure("T3".into()));
        assert!(r.move_dialog.is_none(), "a right-click only selects");
    }
    click_at(&mut harness, at, Primary);
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.move_dialog.as_ref().map(|d| d.id.as_str()), Some("T3"));
    assert!(!r.view3d.captured, "the window frees the mouse");
}

/// The click rows of Help › Controls follow the option; every drag keeps
/// its action; the option is saved and read back with the preferences.
#[test]
fn the_click_option_is_listed_and_remembered() {
    use crate::controls::{Controls, click_buttons, click_rows};
    use egui::PointerButton::{Middle, Primary, Secondary};
    assert_eq!(click_buttons(false, false), (Primary, Secondary));
    assert_eq!(click_buttons(true, false), (Secondary, Primary));
    assert_eq!(
        click_buttons(true, true),
        (Primary, Secondary),
        "Select held"
    );
    assert_eq!(click_rows(false)[0], ("left-click".into(), "select".into()));
    assert_eq!(click_rows(true)[1], ("right-click".into(), "select".into()));
    // Every drag binding is the same with the option (no gesture lost or
    // doubled): only the clicks move.
    for mut c in Controls::presets() {
        let drags = |c: &Controls| [Primary, Secondary, Middle].map(|b| c.drag(b));
        let off = drags(&c);
        c.tools_on_left = true;
        assert_eq!(drags(&c), off, "{}", c.name);
        let rows = c.describe();
        let has = |k: &str| rows.iter().any(|(key, _)| key.contains(k));
        assert!(has("left drag") && has("right drag") && has("middle drag"));
        assert!(has("right double-click"), "{}: {rows:?}", c.name);
    }
    #[derive(Default)]
    struct Store(std::collections::HashMap<String, String>);
    impl eframe::Storage for Store {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.into(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }
        fn flush(&mut self) {}
    }
    let mut store = Store::default();
    assert!(
        !Controls::load_tools_on_left(&store),
        "off when never saved"
    );
    let c = Controls {
        tools_on_left: true,
        ..Controls::default()
    };
    c.save(&mut store);
    assert!(Controls::load_tools_on_left(&store));
    assert!(Controls::load(&store).unwrap().tools_on_left);
}

/// First person, the Edit tool on a structure: its page (or structure
/// screen) opens and the mouse is freed (it stayed locked to the view).
#[test]
fn the_edit_tool_frees_the_mouse() {
    use crate::tools::Tool;
    if ohl_render::headless_device().is_err() {
        assert!(
            ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
            "no GPU"
        );
        return;
    }
    let model = synthetic_project("edit-frees");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .wgpu()
        .build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        r.view3d.controls = crate::controls::Controls::by_name("minecraft").unwrap();
    }
    let at = egui::pos2(620.0, 300.0);
    let click = |h: &mut Harness<'_, App>, button| {
        for pressed in [true, false] {
            h.input_mut().events.push(egui::Event::PointerButton {
                pos: at,
                button,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        h.run();
    };
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(at));
    harness.run();
    click(&mut harness, egui::PointerButton::Primary); // captures
    assert!(harness.state_mut().ready().unwrap().view3d.captured);
    // The Edit tool used on T3 (as a right-click on it reports).
    use_tool(
        &mut harness,
        Tool::Edit,
        Selection::Structure("T3".into()),
        None,
    );
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.selection, Selection::Structure("T3".into()));
    assert!(!r.view3d.captured, "the page frees the mouse");
}
