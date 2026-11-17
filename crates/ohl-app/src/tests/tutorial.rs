// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The tutorial "Start from scratch — your first line" (docs/tutorial/),
//! done chapter by chapter through the UI, as far as the harness can: a
//! project from the starter template, the ground from its survey, the
//! route and the alignment, six structures, the set numbers their types
//! bring, six sections strung, the result looked at, the jumpers at the
//! angle structure, the results in detail, the checks, the load cases per
//! group, the project saved and opened again. Each step names its
//! chapter: a UI change that breaks the tutorial fails here.
//!
//! The tutorial's pictures: with `OHL_RENDER_PNG` set to a folder (and a
//! GPU) each moment in [`IMAGES`] is saved there as
//! `tutorial-<name>.png` (the tutorial's, docs/images/tutorial/; made as
//! `USER=tester`). [`every_tutorial_image_is_made_here`] checks that each
//! picture the tutorial shows is one of them (or a drawing) and exists.
//!
//! What the harness cannot do is done through the same function the UI
//! calls, said where: the native file dialogs (rfd), the right-clicks in
//! the views (the tool is used on what the click would give, as the other
//! tests do), the dropdowns of the windows (their field set) and, for a
//! picture, the camera of the 3D view (where the reader turns and zooms
//! with the mouse).

use std::sync::{Arc, Mutex};

use super::*;

use crate::editors::Form;
use crate::tools::Tool;

/// The alignment points of the tutorial (chapter 2): E, N in metres, on
/// the starter's invented ground (each on a survey point); an angle at
/// the fourth.
pub(crate) const POINTS: [(f64, f64); 6] = [
    (0.0, 0.0),
    (200.0, 0.0),
    (400.0, 0.0),
    (600.0, 0.0),
    (800.0, 100.0),
    (1000.0, 200.0),
];

/// The tension structures (chapter 3): at the ends and the angle.
const TENSION: [usize; 3] = [0, 3, 5];

/// The two sections of each phase (chapter 5): their structures in line
/// order, from the first tension structure's set ahead (40 + phase) over
/// the suspension structures' (phase) to the last one's set behind (20 +
/// phase).
const SECTIONS: [&[&str]; 2] = [&["T1", "T2", "T3", "T4"], &["T4", "T5", "T6"]];

/// The starter's structure types (chapter 3).
const TENSION_TYPE: &str = "starter-tension";
const SUSPENSION: &str = "starter-suspension";

/// Every picture this test makes, `tutorial-<name>.png` in
/// docs/images/tutorial/, in the tutorial's order.
pub(crate) const IMAGES: &[&str] = &[
    // index and chapter 0
    "finished",
    "start-page",
    "main-window",
    // chapter 1
    "new-project",
    "first-steps",
    "terrain-surfaces",
    "ground-3d",
    // chapter 2
    "name-the-route",
    "alignment-clicking",
    "point-coordinates",
    "alignment-route",
    "alignment-plan",
    // chapter 3
    "structure-picker",
    "add-structure",
    "structures-3d",
    // chapter 4
    "structure-display-options",
    "suspension-sets",
    "tension-sets",
    "string-window",
    // chapter 5
    "string-wire-clicking",
    "string-wire-status",
    "string-wire",
    "strung-3d",
    "display-weather-cases",
    "two-cases-profile",
    "span-properties",
    // chapter 6
    "jumpers-3d",
    "jumper-properties",
    // chapter 7
    "profile-sags",
    "sections-table",
    "sagging",
    "line-3d",
    // chapter 8
    "wire-clearances",
    "vertical-clearances-setup",
    "vertical-clearances-clear",
    "vertical-clearances",
    "wire-tensions",
    "string-checks",
    // chapter 9
    "load-cases-per-group",
    "open-in-ohl-structure",
    // chapter 10
    "project-lines",
    "operation-log",
];

/// The folder the pictures go to, when they are made.
fn rendering() -> Option<std::ffi::OsString> {
    ohl_core::env::var_os("OHL_RENDER_PNG")
}

/// Saves the window, cut to `rect` (points) when given, as
/// `tutorial-<name>.png` in `OHL_RENDER_PNG` (when set and a GPU renders).
fn save(h: &mut Harness<'_, App>, name: &str, rect: Option<egui::Rect>) {
    assert!(IMAGES.contains(&name), "{name}: not in IMAGES");
    let Some(dir) = rendering() else {
        return;
    };
    h.run();
    let Ok(image) = h.render() else {
        return;
    };
    // The status bar shows the login name ("editing as …"): the docs'
    // pictures are made as a neutral user.
    assert_eq!(crate::files::user(), "tester", "made with USER=tester");
    let image = match rect {
        Some(rect) => {
            let ppp = image.width() as f32 / 1500.0;
            super::tool_shots::crop(&image, rect, ppp)
        }
        None => image,
    };
    let path = std::path::PathBuf::from(dir).join(format!("tutorial-{name}.png"));
    crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
}

/// The whole window as `name`.
fn shot(h: &mut Harness<'_, App>, name: &str) {
    save(h, name, None);
}

/// The window titled `title` alone as `name`: a step's window.
fn shot_of(h: &mut Harness<'_, App>, name: &str, title: &str) {
    let rect = rendering().map(|_| {
        h.run();
        super::tool_shots::window_rect(h, title)
            .unwrap_or_else(|| panic!("{name}: no window titled {title}"))
    });
    save(h, name, rect);
}

/// The middle pages' area (3D, Plan and the tables share it) as `name`.
fn shot_view(h: &mut Harness<'_, App>, name: &str) {
    h.run();
    let rect = h.state_mut().ready().unwrap().view3d.rect;
    save(h, name, rect);
}

/// The Profile page as `name` (with its row of fields above).
fn shot_profile(h: &mut Harness<'_, App>, name: &str) {
    h.run();
    let rect = h.state_mut().ready().unwrap().profile.rect;
    save(h, name, rect.map(|r| r.with_min_y(r.top() - 30.0)));
}

/// The floating page or window holding the node labelled `label` (the
/// area it is drawn in) as `name`: a table opened from a menu.
fn shot_area(h: &mut Harness<'_, App>, name: &str, label: &str) {
    h.run();
    let at = h
        .query_all_by_label_contains(label)
        .next()
        .unwrap_or_else(|| panic!("{name}: no {label}"))
        .rect()
        .center();
    // The smallest area shown there.
    let rect = h.ctx.memory(|m| {
        m.areas()
            .visible_layer_ids()
            .into_iter()
            .filter_map(|l| m.area_rect(l.id))
            .filter(|r| r.contains(at))
            .min_by(|a, b| a.area().total_cmp(&b.area()))
    });
    assert!(rect.is_some(), "{name}: no area at {label}");
    save(h, name, rect);
}

/// The pages on the right of the middle ones (Properties) as `name`:
/// from their tab bar down to the status bar, or `height` points.
fn shot_right(h: &mut Harness<'_, App>, name: &str, height: Option<f32>) {
    h.run();
    let view = h.state_mut().ready().unwrap().view3d.rect.unwrap();
    let screen = h.ctx.content_rect();
    let rect = egui::Rect::from_min_max(
        egui::pos2(view.right() + 4.0, view.top() - 26.0),
        egui::pos2(screen.right() - 2.0, screen.bottom() - 26.0),
    );
    let rect = match height {
        Some(height) => rect.with_max_y(rect.top() + height),
        None => rect,
    };
    save(h, name, Some(rect));
}

/// What the nodes labelled `labels` (the first of each) cover, and
/// `margin` around: a menu, a row of the screen.
fn around(h: &Harness<'_, App>, labels: &[&str], margin: f32) -> egui::Rect {
    let mut rect = egui::Rect::NOTHING;
    for l in labels {
        let n = h
            .query_all_by_label_contains(l)
            .next()
            .unwrap_or_else(|| panic!("no {l}"));
        rect = rect.union(n.rect());
    }
    rect.expand(margin)
}

/// The whole window with numbered marks at `marks` (screen points) as
/// `name`: the tutorial's map of the window (the marks drawn over it with
/// egui, after the app's own drawing).
fn shot_marked(h: &mut Harness<'_, App>, name: &str, marks: Vec<(egui::Pos2, u32)>) {
    let marks = Arc::new(Mutex::new(marks));
    let drawn = Arc::clone(&marks);
    h.ctx.on_end_pass(
        "tutorial marks",
        Arc::new(move |ui: &mut egui::Ui| {
            let painter = ui.ctx().layer_painter(egui::LayerId::new(
                egui::Order::Tooltip,
                egui::Id::new("tutorial marks"),
            ));
            for (at, n) in drawn.lock().unwrap().iter() {
                painter.circle(
                    *at,
                    12.0,
                    egui::Color32::from_rgb(9, 105, 218),
                    egui::Stroke::new(2.0, egui::Color32::WHITE),
                );
                painter.text(
                    *at,
                    egui::Align2::CENTER_CENTER,
                    n.to_string(),
                    egui::FontId::proportional(15.0),
                    egui::Color32::WHITE,
                );
            }
        }),
    );
    shot(h, name);
    marks.lock().unwrap().clear();
    h.run();
}

/// The 3D view looking at `target` from `distance` m, from the side
/// `yaw` degrees from east (counter-clockwise) and `pitch` degrees above
/// the horizon: where the reader turns and zooms the view with the mouse.
fn look_at(h: &mut Harness<'_, App>, target: [f64; 3], distance: f64, yaw: f64, pitch: f64) {
    let r = h.state_mut().ready().unwrap();
    let mut camera = r
        .view3d
        .camera
        .unwrap_or(ohl_render::Camera::fit(&ohl_render::Bounds {
            min: target,
            max: target,
        }));
    camera.target = target;
    camera.distance = distance;
    camera.yaw = yaw.to_radians();
    camera.pitch = pitch.to_radians();
    r.view3d.set_camera(camera);
    h.run();
}

/// Where structure `id` stands, `up` m above its base.
fn above(h: &mut Harness<'_, App>, id: &str, up: f64) -> [f64; 3] {
    let p = h
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .structure(id)
        .unwrap()
        .position;
    [p[0], p[1], p[2] + up]
}

/// Clicks `label` (the only one of that name).
fn click(h: &mut Harness<'_, App>, label: &str) {
    h.get_by_label(label).click();
    h.run();
}

/// Clicks the menu entry `label` (the quick bar may have one of the same
/// name before it).
fn click_entry(h: &mut Harness<'_, App>, label: &str) {
    h.query_all_by_label(label)
        .last()
        .unwrap_or_else(|| panic!("no {label}"))
        .click();
    h.run();
}

/// A drag with the primary button from `from` to `to` (screen points).
fn drag(h: &mut Harness<'_, App>, from: egui::Pos2, to: egui::Pos2) {
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    h.input_mut().events.push(egui::Event::PointerMoved(from));
    h.run();
    h.input_mut().events.push(button(from, true));
    h.run();
    for k in 1..=10 {
        let at = from + (to - from) * (k as f32 / 10.0);
        h.input_mut().events.push(egui::Event::PointerMoved(at));
        h.run();
    }
    h.input_mut().events.push(button(to, false));
    h.run();
    // The pointer off the edge (not drawn over the page shown next).
    h.input_mut()
        .events
        .push(egui::Event::PointerMoved(to - egui::vec2(60.0, 0.0)));
    h.run();
}

/// The wheel turned sideways over `at` (screen points): what is there
/// scrolls `by` points to the right; then the pointer leaves it (no tip
/// over the picture).
fn wheel_sideways(h: &mut Harness<'_, App>, at: egui::Pos2, by: f32) {
    h.input_mut().events.push(egui::Event::PointerMoved(at));
    h.run();
    h.input_mut().events.push(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(-by, 0.0),
        phase: egui::TouchPhase::Move,
        modifiers: egui::Modifiers::NONE,
    });
    h.run();
    h.input_mut()
        .events
        .push(egui::Event::PointerMoved(egui::pos2(2.0, 2.0)));
    h.run();
}

/// Closes the menu open: a click elsewhere (on the status bar).
fn close_menus(h: &mut Harness<'_, App>) {
    h.get_by_label_contains("editing as").click();
    h.run();
    assert!(!egui::Popup::is_any_open(&h.ctx), "a menu still open");
}

/// Waits (up to two minutes) until `done`.
fn wait(h: &mut Harness<'_, App>, what: &str, mut done: impl FnMut(&mut App) -> bool) {
    let started = std::time::Instant::now();
    while !done(h.state_mut()) {
        assert!(
            started.elapsed().as_secs() < 120,
            "{what}: not done in 120 s"
        );
        h.step();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    h.run();
}

/// Run in the Vertical clearances window (open): the spans found below
/// a rule.
fn vertical(h: &mut Harness<'_, App>) -> usize {
    run_in(h, "Vertical clearances").click();
    h.step();
    wait(h, "the vertical clearances", |a| {
        matches!(
            a.ready().unwrap().vertical_clearances.result(),
            (Some(_), false)
        )
    });
    let r = h.state_mut().ready().unwrap();
    let (Some(rep), _) = r.vertical_clearances.result() else {
        unreachable!("waited for");
    };
    let rep = rep.as_ref().unwrap();
    assert!(!rep.states.is_empty());
    rep.zone.findings.len()
}

#[test]
fn the_tutorial_builds_a_first_line() {
    use crate::files::{FileAction, LOCAL_CRS, Template};
    let gpu = ohl_render::headless_device().is_ok();
    assert!(
        gpu || ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
        "no GPU"
    );
    let dir = std::env::temp_dir().join(format!("ohl-tutorial-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let user = crate::files::user();
    let template = Sources {
        point_budget: 1000,
        corridor: 60.0,
        user: Some(user.clone()),
        ..Sources::default()
    };
    let mut builder = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .with_max_steps(64);
    if gpu {
        builder = builder.wgpu();
    }
    let mut h = builder.build_eframe(move |cc| App::empty(template, cc.wgpu_render_state.clone()));
    h.run();

    // Chapter 1: the start page's New project from the starter template…
    // opens the New project window: the starter chosen, local
    // coordinates, the line named "My first line"; the default weather
    // and load cases unticked (the tutorial works with the starter's
    // five weather cases).
    h.get_by_label("New project from the starter template…");
    let start = around(
        &h,
        &[
            "version ",
            "The tutorial goes from the starter template",
            "Join a server…",
        ],
        30.0,
    );
    // With the mark and the name above.
    save(
        &mut h,
        "start-page",
        Some(start.with_min_y(start.top() - 100.0)),
    );
    click(&mut h, "New project from the starter template…");
    h.get_by_label("Starter template (for learning)");
    h.get_by_label("Coordinates: local, in metres");
    click(&mut h, crate::default_cases::NEW_PROJECT_TICK);
    let np = h.state_mut().new_project_for_test().clone().unwrap();
    assert_eq!(np.template, Template::Starter);
    assert_eq!(np.crs().unwrap(), LOCAL_CRS);
    assert_eq!(np.name, "My first line");
    assert!(!np.defaults, "unticked");
    shot_of(&mut h, "new-project", "New project");
    // "Choose where to save it and create…" asks for the file in a native
    // dialog: the project is made with the function the window calls,
    // the window closed and the project opened as the window opens it
    // (File › Open).
    *h.state_mut().new_project_for_test() = None;
    let path = crate::files::create_project_from(
        &dir.join("My first line"),
        &np.name,
        &np.crs().unwrap(),
        &user,
        np.template,
        np.defaults,
    )
    .unwrap();
    h.state_mut()
        .file_action_for_test(FileAction::Open(Some(path.clone())));
    wait(&mut h, "opening the project", |a| {
        a.ready().is_some() || a.failed().is_some()
    });
    assert_eq!(h.state_mut().failed(), None);
    // The first steps window asks for the route: Later (chapter 2).
    h.get_by_label("New project: first steps");
    shot_of(&mut h, "first-steps", "New project: first steps");
    click(&mut h, "Later");
    {
        let r = h.state_mut().ready().unwrap();
        assert!(r.first_steps.is_none());
        let line = &r.model.line;
        assert!(line.routes.is_empty() && line.structures.is_empty());
        assert_eq!(r.model.types.as_ref().map(|(t, _)| t.len()), Some(2));
        assert_eq!(r.model.cases.len(), 5);
        assert!(r.model.ground.is_none());
    }
    // The ground: Line › Terrain › Terrain surfaces (TIN)… › Import… — a
    // native file dialog: the survey beside the project imported as the
    // button does (surfaces::Action::Import).
    crate::tests::line_menu(&mut h, "Terrain");
    click(&mut h, "Terrain surfaces (TIN)…");
    h.get_by_label("Terrain surfaces (TIN)");
    h.get_by_label("Import…");
    {
        let ctx = egui::Context::default();
        let app = h.state_mut();
        let mut plugins = std::mem::take(&mut app.plugins);
        let r = app.ready().unwrap();
        let project = r.model.sources.file.clone();
        r.surfaces.act(
            crate::surfaces::Action::Import(dir.join(crate::starter::SURVEY)),
            &mut r.model,
            &mut plugins,
            &user,
            &project,
            &ctx,
        );
        app.plugins = plugins;
    }
    wait(&mut h, "the ground", |a| {
        a.ready().unwrap().model.ground_source == "surface starter-survey"
    });
    assert!(h.state_mut().ready().unwrap().model.ground.is_some());
    shot_of(&mut h, "terrain-surfaces", "Terrain surfaces (TIN)");
    h.state_mut().ready().unwrap().surfaces.open = false;
    h.run();
    // View › Fit views: the 3D view on the new ground.
    h.state_mut().show_tab(crate::app::Tab::View3d);
    h.run();
    click(&mut h, "View");
    click_entry(&mut h, "Fit views");
    shot_view(&mut h, "ground-3d");

    // Chapter 2: the route (Next steps › Name the route… opens the first
    // steps again), then the alignment with New alignment.
    h.state_mut().show_tab(crate::app::Tab::NextSteps);
    h.run();
    click(&mut h, "Next: Name the route…");
    h.get_by_label("New project: first steps");
    shot_of(&mut h, "name-the-route", "New project: first steps");
    click(&mut h, "Create the route");
    {
        let r = h.state_mut().ready().unwrap();
        assert_eq!(r.model.line.routes.len(), 1, "{:?}", r.status);
        assert_eq!(r.model.line.routes[0].id, "route 1");
        assert_eq!(r.hotbar.tool(), Tool::NewAlignment);
    }
    // A click on the ground at each point (in the 3D view, where the
    // ground and the points so far are drawn): no point to snap to, so
    // the coordinates window, E and N typed (here given as the click), the
    // ground's height; Add the point.
    for (k, (x, y)) in POINTS.into_iter().enumerate() {
        let z = h
            .state_mut()
            .ready()
            .unwrap()
            .model
            .ground_at(x, y)
            .unwrap();
        use_tool(&mut h, Tool::NewAlignment, Selection::None, Some([x, y, z]));
        h.get_by_label("Point coordinates");
        if k == 1 {
            shot_of(&mut h, "point-coordinates", "Point coordinates");
        }
        click(&mut h, "Add the point");
        assert!(h.state_mut().ready().unwrap().coord_dialog.is_none());
        if k == 2 {
            // Mid-way: three points, the line drawn through them.
            assert_eq!(h.state_mut().ready().unwrap().new_alignment.len(), 3);
            shot_view(&mut h, "alignment-clicking");
        }
    }
    // Enter: its route (the only one), Make the alignment.
    press(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
    h.get_by_label("New alignment: its route");
    shot_of(&mut h, "alignment-route", "New alignment: its route");
    click(&mut h, "Make the alignment");
    let points: Vec<(String, [f64; 3])> = {
        let r = h.state_mut().ready().unwrap();
        let line = &r.model.line;
        assert_eq!(line.alignments.len(), 1, "{:?}", r.status);
        assert_eq!(line.alignments[0].route, "route 1");
        line.points_of(&line.alignments[0])
            .iter()
            .map(|p| (p.id.clone(), p.position))
            .collect()
    };
    assert_eq!(points.len(), POINTS.len());
    for ((_, p), (x, y)) in points.iter().zip(POINTS) {
        assert!((p[0] - x).abs() < 1e-3 && (p[1] - y).abs() < 1e-3, "{p:?}");
    }
    // The Plan page in front; the reader zooms there with the wheel: about
    // 1 100 m wide on the new points. Then the 3D page again.
    h.state_mut().show_tab(crate::app::Tab::Plan);
    h.run();
    h.state_mut()
        .ready()
        .unwrap()
        .plan
        .set_view([500.0, 100.0], 0.68);
    shot_view(&mut h, "alignment-plan");
    h.state_mut().show_tab(crate::app::Tab::View3d);
    h.run();

    // Chapter 3: Add structure, a click on the first alignment point: the
    // tension type there (its button opens the structure picker: the row,
    // Choose); Also on other points of this alignment: All, the
    // suspension type at the second, third and fifth (their type
    // dropdowns: the field set), the tension type above at the others.
    use_tool(
        &mut h,
        Tool::AddStructure,
        Selection::AlignmentPoint {
            id: points[0].0.clone(),
        },
        Some(points[0].1),
    );
    h.get_by_label("Add structure");
    {
        let r = h.state_mut().ready().unwrap();
        let Some(Form::Structure(f)) = &r.editor else {
            panic!("{:?}", r.editor);
        };
        assert_eq!(f.id, "T1");
        assert!(f.on_alignment.is_some() && f.along_line);
        // The first type of the library to start with.
        assert_eq!(f.structure_type.as_deref(), Some(SUSPENSION));
    }
    h.get_by_label_contains(&format!("{SUSPENSION} — ")).click();
    h.run();
    h.get_by_label("Structure type — T1");
    h.query_all_by_label(TENSION_TYPE).next().unwrap().click();
    h.run();
    shot_of(&mut h, "structure-picker", "Structure type — T1");
    click(&mut h, "Choose");
    {
        let r = h.state_mut().ready().unwrap();
        let Some(Form::Structure(f)) = &r.editor else {
            panic!("{:?}", r.editor);
        };
        assert_eq!(f.structure_type.as_deref(), Some(TENSION_TYPE));
    }
    h.get_by_label_contains("Also on other points of this alignment")
        .click();
    h.run();
    click(&mut h, "All");
    {
        let r = h.state_mut().ready().unwrap();
        let Some(Form::Structure(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        let more = f.more.as_mut().expect("the other points");
        assert_eq!(more.rows.len(), POINTS.len() - 1);
        assert!(more.rows.iter().all(|r| r.on));
        for row in &mut more.rows {
            if !TENSION.contains(&row.index) {
                row.structure_type = Some(SUSPENSION.into());
            }
        }
    }
    h.run();
    shot_of(&mut h, "add-structure", "Add structure");
    click(&mut h, "Apply");
    let ids: Vec<String> = {
        let r = h.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        let line = &r.model.line;
        assert_eq!(line.structures.len(), POINTS.len());
        line.points_of(&line.alignments[0])
            .iter()
            .map(|p| p.structure.clone().expect("a structure on each point"))
            .collect()
    };
    assert_eq!(ids, ["T1", "T2", "T3", "T4", "T5", "T6"]);
    {
        let r = h.state_mut().ready().unwrap();
        let line = &r.model.line;
        for (k, id) in ids.iter().enumerate() {
            let s = line.structure(id).unwrap();
            let tension = TENSION.contains(&k);
            let want = if tension { TENSION_TYPE } else { SUSPENSION };
            assert_eq!(s.structure_type.as_deref(), Some(want), "{id}");
            // Chapter 4: the strings come with the type — four at a
            // suspension structure (three phases, the earth wire), two at
            // each place of a tension structure (eight).
            assert_eq!(s.strings.len(), if tension { 8 } else { 4 }, "{id}");
        }
    }
    // The 3D view fitted to the first structures.
    shot_view(&mut h, "structures-3d");

    // Chapter 4: Line › Structures › Display options: Wire points (label,
    // set) ticked for the 3D view (the menu stays open).
    crate::tests::line_menu(&mut h, "Structures");
    h.get_by_label_contains("Display options").hover();
    h.run();
    click(&mut h, "Wire points (label, set) in 3D");
    h.get_by_label("Strings (label) in 3D");
    let menu = around(
        &h,
        &[
            "Labels in each view",
            "With descriptions in 3D",
            "Wire points (label, set)",
        ],
        24.0,
    );
    save(&mut h, "structure-display-options", Some(menu));
    close_menus(&mut h);
    {
        let r = h.state_mut().ready().unwrap();
        assert!(r.model.section_display.wire_label[crate::annotations::View::ThreeD as usize]);
    }
    // G (Go to structure): T2 (the dropdown: the field set), Enter; the
    // reader zooms in (the wheel) and looks along the line.
    press(&mut h, egui::Key::G, egui::Modifiers::NONE);
    h.get_by_label("Go to structure");
    h.state_mut().ready().unwrap().goto = Some("T2".into());
    h.run();
    press(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
    assert_eq!(
        h.state_mut().ready().unwrap().selection,
        Selection::Structure("T2".into())
    );
    let t2 = above(&mut h, "T2", 20.0);
    look_at(&mut h, t2, 46.0, 245.0, 26.0);
    shot_view(&mut h, "suspension-sets");
    // A string's window (Edit, a click on it): the set and its values;
    // Cancel.
    use_tool(&mut h, Tool::Edit, string_at("T2", 1), None);
    h.get_by_label("Strings of set 1 of T2");
    shot_of(&mut h, "string-window", "Strings of set 1 of T2");
    // Cancel (here Escape, the same: the window opens where the pointer
    // was, partly below the harness's screen).
    h.get_by_label("Cancel");
    press(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    {
        let r = h.state_mut().ready().unwrap();
        assert!(
            r.editor.is_none(),
            "{:?} {:?}",
            r.editor.as_ref().map(|f| f.title()),
            r.status
        );
    }
    // Then T4 the same way.
    press(&mut h, egui::Key::G, egui::Modifiers::NONE);
    h.state_mut().ready().unwrap().goto = Some("T4".into());
    h.run();
    press(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
    let t4 = above(&mut h, "T4", 20.0);
    look_at(&mut h, t4, 46.0, 245.0, 26.0);
    shot_view(&mut h, "tension-sets");

    // Chapter 5: String wire — the strings in line order (clicks), Enter,
    // the section's window: the circuit and the phase typed (the fields
    // set), Apply. Each phase in two sections: T1 to T4 and T4 to T6 (the
    // tension structures' sets 41-43 ahead, 21-23 behind).
    let first = above(&mut h, "T1", 20.0);
    look_at(
        &mut h,
        [first[0] + 100.0, first[1], first[2]],
        230.0,
        220.0,
        18.0,
    );
    for phase in 1..=3u32 {
        for (k, structures) in SECTIONS.iter().enumerate() {
            let last = structures.len() - 1;
            for (i, structure) in structures.iter().enumerate() {
                let set = match i {
                    0 => 40 + phase,
                    i if i == last => 20 + phase,
                    _ => phase,
                };
                use_tool(&mut h, Tool::StringWire, string_at(structure, set), None);
                if (phase, k, i) == (1, 0, 1) {
                    // Mid-way: two points counted, the line drawn.
                    let r = h.state_mut().ready().unwrap();
                    let status = r.status.clone().unwrap().0;
                    assert!(status.starts_with("String wire: 2 points"), "{status}");
                    shot_view(&mut h, "string-wire-clicking");
                    let bar = around(&h, &["editing as", "String wire: 2 points"], 4.0);
                    save(&mut h, "string-wire-status", Some(bar));
                }
            }
            assert_eq!(h.state_mut().ready().unwrap().chain.len(), structures.len());
            press(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
            h.get_by_label("String wire: new section");
            {
                let r = h.state_mut().ready().unwrap();
                let Some(Form::Section(f)) = &mut r.editor else {
                    panic!("{:?}", r.editor);
                };
                assert!(f.new);
                assert_eq!(f.cable, "acsr-drake-26-7");
                // After the first: the window proposes the circuit and the
                // phase — the same phase across a tension structure, the
                // next one at a new wire point.
                if (phase, k) != (1, 0) {
                    assert_eq!(
                        (f.circuit.as_str(), f.phase_label.clone()),
                        ("C1", format!("L{phase}"))
                    );
                }
                f.circuit = "C1".into();
                f.phase_label = format!("L{phase}");
            }
            h.run();
            if phase == 1 && k == 0 {
                shot_of(&mut h, "string-wire", "String wire: new section");
            }
            // Apply (here Enter, the same: the window opens where the
            // pointer was, partly below the harness's screen).
            h.get_by_label("Apply");
            press(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
            let r = h.state_mut().ready().unwrap();
            assert!(r.editor.is_none(), "{:?}", r.status);
        }
    }
    {
        let r = h.state_mut().ready().unwrap();
        assert_eq!(r.model.line.sections.len(), 6);
        // Every weather case solved, every section in each.
        for k in 0..r.model.cases.len() {
            r.model.ensure_solved(k);
        }
        for c in &r.model.cases {
            assert!(c.failed.is_empty(), "{}: {:?}", c.case.name, c.failed);
            assert_eq!(c.shapes.len(), 6, "{}", c.case.name);
        }
    }
    // Look at the result: the strung line (Escape holds Select again).
    press(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    assert_eq!(h.state_mut().ready().unwrap().hotbar.tool(), Tool::Select);
    look_at(
        &mut h,
        [first[0] + 280.0, first[1], first[2] - 5.0],
        600.0,
        250.0,
        20.0,
    );
    shot_view(&mut h, "strung-3d");
    // View › Display weather cases…: max temperature ticked beside
    // everyday, Apply: both drawn.
    click(&mut h, "View");
    click(&mut h, "Display weather cases…");
    h.get_by_label("Display weather cases");
    h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "max temperature")
        .click();
    h.run();
    shot_of(&mut h, "display-weather-cases", "Display weather cases");
    click(&mut h, "Apply");
    {
        let r = h.state_mut().ready().unwrap();
        assert!(r.display_window.is_none());
        let names: Vec<&str> = r
            .model
            .shown_cases
            .iter()
            .map(|&k| r.model.cases[k].case.name.as_str())
            .collect();
        assert_eq!(names, ["everyday", "max temperature"]);
    }
    shot_profile(&mut h, "two-cases-profile");
    // A click on a span of S1 (Select held): its results in Properties.
    use_tool(
        &mut h,
        Tool::Select,
        Selection::Span {
            section: "S1".into(),
            span: 1,
        },
        None,
    );
    h.state_mut().show_tab(crate::app::Tab::Properties);
    h.state_mut().show_tab(crate::app::Tab::NextSteps);
    h.run();
    assert!(h.query_all_by_label("S1 span 2").next().is_some());
    // The window and its parts, numbered (chapter 0).
    let marks = {
        let pos = |h: &Harness<'_, App>, label: &str| {
            h.query_all_by_label(label)
                .map(|n| n.rect())
                .min_by(|a, b| a.top().total_cmp(&b.top()))
                .unwrap_or_else(|| panic!("no {label}"))
        };
        let view = h.state_mut().ready().unwrap().view3d.rect.unwrap();
        let profile = h.state_mut().ready().unwrap().profile.rect.unwrap();
        let help = pos(&h, "Help");
        let status = h.get_by_label_contains("editing as").rect();
        // The tabs (not nodes of their own) in the bar above each page.
        let tabs = view.top() - 13.0;
        vec![
            (help.right_center() + egui::vec2(24.0, 0.0), 1),
            (help.right_center() + egui::vec2(262.0, 0.0), 2),
            (egui::pos2(view.left() + 165.0, tabs), 3),
            (view.center_top() + egui::vec2(0.0, 40.0), 4),
            (egui::pos2(view.center().x - 250.0, view.bottom() - 48.0), 5),
            (profile.left_top() + egui::vec2(112.0, -42.0), 6),
            (egui::pos2(view.left() - 90.0, tabs), 7),
            (egui::pos2(view.right() + 150.0, tabs), 8),
            (status.center_top() + egui::vec2(0.0, -16.0), 9),
        ]
    };
    shot_marked(&mut h, "main-window", marks);
    // The page widened (its left edge dragged) to show the sags; then
    // narrowed again.
    let view = h.state_mut().ready().unwrap().view3d.rect.unwrap();
    let edge = egui::pos2(view.right() + 3.0, view.center().y);
    drag(&mut h, edge, edge - egui::vec2(380.0, 0.0));
    h.get_by_label("Sag [m]");
    shot_right(&mut h, "span-properties", Some(560.0));
    drag(&mut h, edge - egui::vec2(380.0, 0.0), edge);
    // File › Save all (⌘S): the end of Part 1.
    press(&mut h, egui::Key::S, egui::Modifiers::COMMAND);

    // Chapter 6: Add jumper (Next steps › Hold Add jumper), a click on T4:
    // the three jumpers missing there (same circuit and phase).
    click(&mut h, "Hold Add jumper");
    assert_eq!(
        h.state_mut().ready().unwrap().hotbar.tool(),
        Tool::AddJumper
    );
    use_tool(
        &mut h,
        Tool::AddJumper,
        Selection::Structure("T4".into()),
        None,
    );
    {
        let r = h.state_mut().ready().unwrap();
        assert_eq!(r.model.line.jumpers.len(), 3, "{:?}", r.status);
        assert!(
            r.status
                .as_ref()
                .is_some_and(|(s, _)| s.contains("3 jumpers")),
            "{:?}",
            r.status
        );
    }
    // None missing at T1 (one section end per phase).
    use_tool(
        &mut h,
        Tool::AddJumper,
        Selection::Structure("T1".into()),
        None,
    );
    {
        let r = h.state_mut().ready().unwrap();
        assert_eq!(r.model.line.jumpers.len(), 3);
        assert!(
            r.status
                .as_ref()
                .is_some_and(|(s, _)| s.contains("none missing at T1")),
            "{:?}",
            r.status
        );
        for k in 0..r.model.cases.len() {
            r.model.ensure_solved(k);
        }
        for c in &r.model.cases {
            assert!(c.failed.is_empty(), "{}: {:?}", c.case.name, c.failed);
            assert_eq!(c.jumpers.len(), 3, "{}", c.case.name);
        }
    }
    // A click on a jumper (Select held, Escape): its properties.
    press(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    let jumper = h.state_mut().ready().unwrap().model.line.jumpers[0]
        .id
        .clone();
    use_tool(
        &mut h,
        Tool::Select,
        Selection::Jumper(jumper.clone()),
        None,
    );
    let t4 = above(&mut h, "T4", 21.0);
    look_at(&mut h, t4, 42.0, 140.0, 12.0);
    shot_view(&mut h, "jumpers-3d");
    h.get_by_label(&format!("Jumper {jumper}"));
    shot_right(&mut h, "jumper-properties", Some(500.0));
    // The tutorial's opening picture: the finished line (without the wire
    // points' labels, for this picture only).
    let three_d = crate::annotations::View::ThreeD as usize;
    h.state_mut()
        .ready()
        .unwrap()
        .model
        .section_display
        .wire_label[three_d] = false;
    h.state_mut().ready().unwrap().select(Selection::None);
    h.state_mut().ready().unwrap().refresh_views();
    let end = above(&mut h, "T4", 12.0);
    look_at(&mut h, end, 330.0, 318.0, 24.0);
    shot_view(&mut h, "finished");
    h.state_mut()
        .ready()
        .unwrap()
        .model
        .section_display
        .wire_label[three_d] = true;
    h.state_mut().ready().unwrap().refresh_views();

    // Chapter 7: the results — every span sags, more at the maximum
    // temperature than in the everyday case; View › Fit views.
    {
        let r = h.state_mut().ready().unwrap();
        let sag = |name: &str| -> f64 {
            let c = r.model.cases.iter().find(|c| c.case.name == name).unwrap();
            c.shapes
                .iter()
                .flat_map(|s| s.spans.iter().map(|x| x.sag))
                .fold(0.0, f64::max)
        };
        let (everyday, hot) = (sag("everyday"), sag("max temperature"));
        assert!(everyday > 1.0 && hot > everyday, "{everyday} {hot}");
    }
    click(&mut h, "View");
    click_entry(&mut h, "Fit views");
    // Line › Sections › Display options: Sag ticked for the Profile.
    crate::tests::line_menu(&mut h, "Sections");
    h.get_by_label_contains("Display options").hover();
    h.run();
    click(&mut h, "Sag in Profile");
    close_menus(&mut h);
    shot_profile(&mut h, "profile-sags");
    // Line › Sections › Sections table.
    crate::tests::line_menu(&mut h, "Sections");
    click(&mut h, "Sections table");
    // Scrolled sideways to the results (the wheel over the table).
    let row = h.get_by_label("+ Add row").rect().left_bottom() + egui::vec2(300.0, 120.0);
    wheel_sideways(&mut h, row, 1600.0);
    h.get_by_label("Max sag [m]");
    shot_area(&mut h, "sections-table", "+ Add row");
    // Closed (its tab's ×).
    h.state_mut()
        .close_tab(crate::app::Tab::Table(crate::tables::TableKind::Sections));
    h.run();
    // Edit, a click on the span: its section's window, Sagging opened;
    // Cancel (Escape).
    use_tool(
        &mut h,
        Tool::Edit,
        Selection::Span {
            section: "S1".into(),
            span: 1,
        },
        None,
    );
    h.get_by_label("Section S1");
    click(&mut h, "Sagging");
    click(&mut h, "Calculate");
    shot_of(&mut h, "sagging", "Section S1");
    press(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(h.state_mut().ready().unwrap().editor.is_none());
    press(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    h.state_mut().ready().unwrap().select(Selection::Span {
        section: "S6".into(),
        span: 0,
    });
    h.run();
    shot(&mut h, "line-3d");

    // Chapter 8: Checks › Wire clearances › Check wire clearances, with
    // the starter's rule file.
    click(&mut h, "Checks");
    h.get_by_label_contains("Wire clearances").hover();
    h.run();
    h.query_all_by_label("Check wire clearances")
        .last()
        .unwrap()
        .click();
    wait(&mut h, "the wire clearances", |a| {
        let r = a.ready().unwrap();
        r.clearances.report.is_some() && !r.clearances.running()
    });
    {
        let r = h.state_mut().ready().unwrap();
        assert!(
            r.clearances
                .files
                .iter()
                .any(|f| f.to_string_lossy().contains("starter-clearance-rules")),
            "{:?}",
            r.clearances.files
        );
        let rep = r.clearances.report.clone().unwrap().unwrap();
        // Every weather case of the rules (2026-10-28, not only the case
        // shown): the ground rule, the one between the wires and the
        // maximum temperature's.
        assert_eq!(rep.rules.len(), 3, "{:?}", rep.notes);
        assert!(rep.findings.is_empty(), "{:?}", rep.rules);
    }
    shot_of(&mut h, "wire-clearances", "Clearances");
    // Checks › Ground clearances › Vertical clearances…: the first rule's
    // distance set (7 m, as the starter's rule file; typed in its field),
    // Run.
    // The Clearances window closed (its ×), then the next window.
    h.state_mut().ready().unwrap().clearances.open = false;
    h.run();
    open_clearance_window(&mut h, "Vertical clearances…");
    {
        let r = h.state_mut().ready().unwrap();
        let w = &mut r.vertical_clearances;
        assert!(w.open);
        assert_eq!(w.rules, vec![crate::clearance_zones::first_rule()]);
        w.rules[0].distance = ohl_clearance::Requirement::Fixed(7.0);
    }
    h.run();
    shot_of(&mut h, "vertical-clearances-setup", "Vertical clearances");
    // Every case, every span clear (the lowest at the maximum
    // temperature, about 9.7 m).
    assert_eq!(vertical(&mut h), 0);
    shot_of(&mut h, "vertical-clearances-clear", "Vertical clearances");
    // S1 at 10 kN (Edit on a span of S1, its horizontal tension typed —
    // the field set —, Apply): too low at the maximum temperature; Run
    // again. Undo (⌘Z) brings the 20 kN back.
    use_tool(
        &mut h,
        Tool::Edit,
        Selection::Span {
            section: "S1".into(),
            span: 0,
        },
        None,
    );
    {
        let r = h.state_mut().ready().unwrap();
        let Some(Form::Section(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        assert_eq!(
            f.stringing,
            ohl_line::Stringing::Tension {
                condition: ohl_line::StrungAt::Initial,
                horizontal_tension: 20_000.0,
                temperature: 20.0
            }
        );
        f.stringing = ohl_line::Stringing::Tension {
            condition: ohl_line::StrungAt::Initial,
            horizontal_tension: 10_000.0,
            temperature: 20.0,
        };
    }
    h.run();
    press(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
    let low = vertical(&mut h);
    assert!(low > 0);
    {
        let r = h.state_mut().ready().unwrap();
        let (Some(Ok(rep)), _) = r.vertical_clearances.result() else {
            unreachable!("waited for");
        };
        assert!(
            rep.zone.findings.iter().all(|f| f.section == "S1"),
            "{:?}",
            rep.zone.findings
        );
    }
    shot_of(&mut h, "vertical-clearances", "Vertical clearances");
    // Undo (⌘Z), Run again: clear.
    press(&mut h, egui::Key::Z, egui::Modifiers::COMMAND);
    assert_eq!(vertical(&mut h), 0);
    h.state_mut().ready().unwrap().vertical_clearances.open = false;
    h.run();
    // Checks › Wire tensions…: Add rule, its factor of RTS typed (0.5,
    // invented; the field set and kept as a change of it is), Run: every
    // span of every case listed.
    click(&mut h, "Checks");
    click(&mut h, "Wire tensions…");
    assert!(
        h.state_mut()
            .ready()
            .unwrap()
            .wire_tensions
            .rules
            .is_empty()
    );
    click(&mut h, "Add rule");
    {
        let r = h.state_mut().ready().unwrap();
        let w = &mut r.wire_tensions;
        assert!(w.open);
        assert_eq!(w.rules.len(), 1);
        assert!(w.why_not_run().is_some(), "the factor to set");
        w.rules[0].factor = 0.5;
        w.save(&mut r.model);
    }
    h.run();
    run_in(&h, "Wire tensions").click();
    wait(&mut h, "the wire tensions", |a| {
        a.ready().unwrap().wire_tensions.job.result.is_some()
    });
    {
        let r = h.state_mut().ready().unwrap();
        let Some(Ok(rep)) = &r.wire_tensions.job.result else {
            panic!(
                "{:?}",
                r.wire_tensions.job.result.as_ref().map(|r| r.is_ok())
            );
        };
        // Three phases of five spans, in every case; none above half the
        // RTS.
        assert_eq!(rep.report.rows.len(), 3 * 5 * r.model.cases.len());
        assert_eq!(rep.report.failures().count(), 0);
    }
    shot_of(&mut h, "wire-tensions", "Wire tensions");
    h.state_mut().ready().unwrap().wire_tensions.open = false;
    h.run();
    // Checks › String checks…: the window's first rule (the strings' own
    // limits and uplift), Run.
    click(&mut h, "Checks");
    click(&mut h, "String checks…");
    assert!(h.state_mut().ready().unwrap().string_checks.open);
    run_in(&h, "String checks").click();
    wait(&mut h, "the string checks", |a| {
        a.ready().unwrap().string_checks.job.result.is_some()
    });
    {
        let r = h.state_mut().ready().unwrap();
        let Some(Ok(rep)) = &r.string_checks.job.result else {
            panic!("no string checks");
        };
        // The uplift checked; the starter's strings have no limits of
        // their own: said in the notes.
        assert!(!rep.report.rows.is_empty());
        assert_eq!(rep.report.failures().count(), 0);
        assert_eq!(rep.report.notes.len(), 2, "{:?}", rep.report.notes);
    }
    shot_of(&mut h, "string-checks", "String checks");
    h.state_mut().ready().unwrap().string_checks.open = false;
    h.run();

    // Chapter 9 (2026-11-15): T2 clicked (Select held); Line ›
    // Structures › Load cases per group…: New group "suspension", rule
    // type starter-suspension, everyday and wind ticked. The suspension
    // structures take those two, the others (in no group) every case; a
    // placed structure gets its library cases only.
    use_tool(
        &mut h,
        Tool::Select,
        Selection::Structure("T2".into()),
        None,
    );
    {
        let r = h.state_mut().ready().unwrap();
        assert_eq!(r.selection, Selection::Structure("T2".into()));
        assert!(r.model.selected_also.is_empty());
    }
    crate::tests::line_menu(&mut h, "Structures");
    click(&mut h, "Load cases per group…");
    assert!(h.state_mut().ready().unwrap().structure_groups.open);
    click(&mut h, "New group");
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "name")
        .focus();
    h.run();
    press(&mut h, egui::Key::A, egui::Modifiers::COMMAND);
    h.get_by_role_and_label(egui::accesskit::Role::TextInput, "name")
        .type_text("suspension");
    h.run();
    press(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
    h.get_by_label(&format!("type {SUSPENSION}")).scroll_to_me();
    h.run();
    click(&mut h, &format!("type {SUSPENSION}"));
    let tick = egui::accesskit::Role::CheckBox;
    for case in ["everyday", "wind"] {
        h.get_by_role_and_label(tick, case).scroll_to_me();
        h.run();
        h.get_by_role_and_label(tick, case).click();
        h.run();
    }
    shot_of(&mut h, "load-cases-per-group", "Load cases per group");
    {
        let r = h.state_mut().ready().unwrap();
        let g = &r.model.line.structure_groups;
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].name, "suspension");
        assert_eq!(g[0].rules.types, [SUSPENSION]);
        assert_eq!(g[0].load_cases, ["everyday", "wind"]);
        let sets = r.model.line.case_sets();
        assert!(sets.takes("T2", "wind") && !sets.takes("T2", "ice"));
        assert!(sets.takes("T1", "ice"), "a tension structure: every case");
        let loads = crate::structure_app::LineInputs::of(&r.model)
            .compute()
            .unwrap();
        let t2 = ohl_structure_link::placed_values_with(&loads, &r.model.line, "T2", "tutorial");
        assert_eq!(t2.cases, ["everyday", "wind"]);
        r.structure_groups.open = false;
    }
    h.run();
    // T2's properties: Open in OHL-Structure… (the other application,
    // started beside this one: not in this test).
    h.state_mut().show_tab(crate::app::Tab::Properties);
    h.state_mut().show_tab(crate::app::Tab::NextSteps);
    h.run();
    h.get_by_label_contains("Group suspension: it takes 2 case(s): everyday, wind");
    h.get_by_label("Open in OHL-Structure…");
    shot_right(&mut h, "open-in-ohl-structure", Some(610.0));

    // Chapter 10: File › Save all (⌘S); Versions › Project lines… and
    // Operation log…; File › Close, opened again (File › Open recent: the
    // same action): the line as built.
    press(&mut h, egui::Key::S, egui::Modifiers::COMMAND);
    click(&mut h, "Versions");
    click(&mut h, "Project lines…");
    shot_of(&mut h, "project-lines", "Project lines");
    h.state_mut().ready().unwrap().lines_window = None;
    h.run();
    click(&mut h, "Versions");
    click(&mut h, "Operation log…");
    assert!(h.state_mut().ready().unwrap().history_open);
    shot_of(&mut h, "operation-log", "Operation log");
    h.state_mut().ready().unwrap().history_open = false;
    h.run();
    h.state_mut().file_action_for_test(FileAction::Close);
    h.run();
    assert!(h.state_mut().ready().is_none());
    h.state_mut()
        .file_action_for_test(FileAction::Open(Some(path.clone())));
    wait(&mut h, "opening the project again", |a| {
        a.ready().is_some() || a.failed().is_some()
    });
    {
        let r = h.state_mut().ready().unwrap();
        assert!(r.first_steps.is_none());
        let line = &r.model.line;
        assert_eq!(line.structures.len(), POINTS.len());
        assert_eq!(line.sections.len(), 6);
        assert_eq!(line.jumpers.len(), 3);
        assert!(r.model.ground.is_some());
    }
    drop(h);
    std::fs::remove_dir_all(&dir).ok();
}

/// Every picture the tutorial shows (docs/tutorial/*.md, Markdown images
/// and `<img src>`) is in docs/images/tutorial/ and either made by
/// [`the_tutorial_builds_a_first_line`] ([`IMAGES`]) or a drawing (SVG);
/// every picture made there is shown, and the folder holds nothing else.
#[test]
fn every_tutorial_image_is_made_here() {
    let docs = root().join("docs");
    let folder = docs.join("images/tutorial");
    let mut shown = std::collections::BTreeSet::new();
    for e in std::fs::read_dir(docs.join("tutorial")).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_none_or(|x| x != "md") {
            continue;
        }
        let text = std::fs::read_to_string(&p).unwrap();
        for (k, _) in text.match_indices("images/tutorial/") {
            let rest = &text[k + "images/tutorial/".len()..];
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '.'))
                .unwrap_or(rest.len());
            let file = &rest[..end];
            assert!(
                folder.join(file).is_file(),
                "{}: no picture {file}",
                p.display()
            );
            shown.insert(file.to_owned());
        }
    }
    for file in &shown {
        if let Some(name) = file
            .strip_prefix("tutorial-")
            .and_then(|f| f.strip_suffix(".png"))
        {
            assert!(IMAGES.contains(&name), "{file}: not made by the test");
        } else {
            assert!(file.ends_with(".svg"), "{file}: a picture or a drawing");
        }
    }
    for name in IMAGES {
        let file = format!("tutorial-{name}.png");
        assert!(shown.contains(&file), "{file}: made but not shown");
    }
    for e in std::fs::read_dir(&folder).unwrap() {
        let file = e.unwrap().file_name().to_string_lossy().into_owned();
        assert!(shown.contains(&file), "{file}: in the folder, not shown");
    }
}
