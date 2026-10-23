// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

use std::path::{Path, PathBuf};

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::app::App;
use crate::model::{self, Sources};
use crate::selection::Selection;

pub(crate) fn root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// Every weather case solved (the application solves the others when
/// they are shown; the tests look at all of them).
fn solved_all(mut m: model::Model) -> model::Model {
    for k in 0..m.cases.len() {
        m.ensure_solved(k);
    }
    m
}

/// Works on project line `name`, every weather case solved at once (the
/// application solves them in the background).
fn switch(m: &mut model::Model, name: &str) {
    m.switch_line(name).unwrap();
    for k in 0..m.cases.len() {
        m.ensure_solved(k);
    }
}

/// The Run button of the check window titled `title` (in its footer, at
/// the bottom left): of the Run buttons in the window (its accessible
/// node is named by its title), the one nearest its bottom left corner
/// (several check windows may be open, overlapping).
pub(crate) fn run_in<'h>(h: &'h Harness<'_, App>, title: &str) -> egui_kittest::Node<'h> {
    let w = h.get_by_label(title).rect();
    h.query_all_by_label("Run")
        .filter(|n| w.contains_rect(n.rect()))
        .min_by(|a, b| {
            let d = |n: &egui_kittest::Node<'_>| {
                (n.rect().left() - w.left()).abs() + (w.bottom() - n.rect().bottom()).abs()
            };
            d(a).total_cmp(&d(b))
        })
        .unwrap_or_else(|| panic!("no Run in \"{title}\""))
}

pub(crate) fn synthetic() -> model::Model {
    let ex = root().join("examples/synthetic-line");
    solved_all(
        model::load(
            Sources {
                file: ex.join("synthetic-line.ohl.json"),
                library: Some(root().join("libraries/cables")),
                terrain: Some(ex.join("synthetic-survey.csv")),
                weather: Some(ex.join("weather-cases.ohlwc")),
                point_budget: 1000,
                corridor: 60.0,
                ..Sources::default()
            },
            &|_| {},
        )
        .unwrap(),
    )
}

#[test]
fn loads_the_example_with_every_case() {
    let m = synthetic();
    assert_eq!(m.line.structures.len(), 7);
    assert_eq!(m.cases.len(), 5);
    assert!(m.cases.iter().all(|c| c.failed.is_empty()));
    assert!(m.ground.is_some());
    // Every layer builds; wires only of the chosen case.
    for layer in crate::scene::Layer::ALL {
        crate::scene::layer(&m, layer, 0, &Selection::None);
    }
    let wires = crate::scene::layer(&m, crate::scene::Layer::Wires, 0, &Selection::None);
    let spans: usize = m.cases[0].shapes.iter().map(|s| s.spans.len()).sum();
    assert!(wires.counts().0 > spans * 10, "{:?}", wires.counts());
}

/// The whole window with the example: views, properties of a selection,
/// the case list. With a GPU the 3D view renders too; set OHL_RENDER_PNG
/// to a directory to save the window as an image.
#[test]
fn the_window_shows_the_model_and_the_selection() {
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
    harness.get_by_label("nothing selected");
    // Select a structure: the properties show it and its spans.
    harness.state_mut().ready().unwrap().selection = Selection::Structure("T3".into());
    harness.run();
    harness.get_by_label("Structure T3");
    harness.get_by_label("structure T3");
    let span_link = harness
        .query_all_by_label_contains("S1 span 2")
        .next()
        .is_some();
    assert!(span_link, "spans at T3 listed");
    // Another weather case: the wires rebuild, the table follows.
    harness.state_mut().ready().unwrap().selection = Selection::Span {
        section: "S1".into(),
        span: 1,
    };
    harness.run();
    if gpu && let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG") {
        let image = harness.render().unwrap();
        let path = PathBuf::from(dir).join("ohl-design-window.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
    // Top bar and properties.
    assert_eq!(harness.query_all_by_label("S1 span 2").count(), 2);
    assert!(harness.query_all_by_label("glaze ice").count() >= 1);
}

/// Holding W over the 3D view flies forward; Space climbs.
#[test]
fn keys_fly_the_camera() {
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
    let cam = |h: &mut Harness<'_, App>| h.state_mut().ready().unwrap().view3d.camera.unwrap();
    let before = cam(&mut harness);
    // The 3D view is in the middle of the default layout.
    let key = |key, pressed| egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(egui::pos2(620.0, 300.0)));
    harness.input_mut().events.push(key(egui::Key::W, true));
    harness.input_mut().events.push(key(egui::Key::Space, true));
    for _ in 0..10 {
        harness.step();
    }
    harness.input_mut().events.push(key(egui::Key::W, false));
    harness
        .input_mut()
        .events
        .push(key(egui::Key::Space, false));
    harness.step();
    let after = cam(&mut harness);
    let dir = [-before.yaw.cos(), -before.yaw.sin()];
    let moved = [
        after.target[0] - before.target[0],
        after.target[1] - before.target[1],
    ];
    let forward = moved[0] * dir[0] + moved[1] * dir[1];
    assert!(forward > 0.1, "moved {moved:?} along {dir:?}");
    assert!(after.target[2] > before.target[2], "climbed");
    // Released: no more movement.
    harness.step();
    harness.step();
    assert_eq!(cam(&mut harness).target, after.target);
}

/// Minecraft scheme: a click captures the mouse, moving it looks around,
/// a click selects at the crosshair, Escape frees the mouse.
#[test]
fn first_person_capture_aim_and_escape() {
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
    harness.state_mut().ready().unwrap().view3d.controls = crate::controls::Controls::presets()
        .into_iter()
        .find(|c| c.name == "Minecraft creative")
        .unwrap();
    let click = |h: &mut Harness<'_, App>, pos: egui::Pos2| {
        for pressed in [true, false] {
            h.input_mut().events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        h.run();
    };
    let at = egui::pos2(620.0, 300.0);
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(at));
    harness.run();
    click(&mut harness, at);
    assert!(
        harness.state_mut().ready().unwrap().view3d.captured,
        "captured"
    );
    assert_eq!(
        harness.state_mut().ready().unwrap().selection,
        Selection::None
    );
    // Mouse to the right: the view turns right (yaw decreases).
    let yaw0 = harness
        .state_mut()
        .ready()
        .unwrap()
        .view3d
        .camera
        .unwrap()
        .yaw;
    harness
        .input_mut()
        .events
        .push(egui::Event::MouseMoved(egui::vec2(50.0, 0.0)));
    harness.run();
    let yaw1 = harness
        .state_mut()
        .ready()
        .unwrap()
        .view3d
        .camera
        .unwrap()
        .yaw;
    assert!(yaw1 < yaw0, "{yaw0} ➡ {yaw1}");
    // Aim at T3's first point and click anywhere: T3 selected.
    {
        let r = harness.state_mut().ready().unwrap();
        let p = r.model.line.structure("T3").unwrap().points[0].position;
        let cam = r.view3d.camera.as_mut().unwrap();
        cam.target = p;
        cam.distance = 80.0;
    }
    click(&mut harness, egui::pos2(300.0, 200.0));
    assert_eq!(
        harness.state_mut().ready().unwrap().selection,
        Selection::Structure("T3".into())
    );
    // Escape frees the mouse.
    harness.input_mut().events.push(egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run();
    assert!(
        !harness.state_mut().ready().unwrap().view3d.captured,
        "freed"
    );
}

/// A real click: press, move a few pixels, release in a later frame — it
/// still captures the mouse, and its release selects nothing.
#[test]
fn a_wobbly_click_captures_the_mouse() {
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
    harness.state_mut().ready().unwrap().view3d.controls = crate::controls::Controls::presets()
        .into_iter()
        .find(|c| c.capture)
        .unwrap();
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let (a, b) = (egui::pos2(620.0, 300.0), egui::pos2(632.0, 306.0));
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(a));
    harness.run();
    harness.input_mut().events.push(button(a, true));
    harness.step();
    assert!(harness.state_mut().ready().unwrap().view3d.captured);
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(b));
    harness.step();
    harness.input_mut().events.push(button(b, false));
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.view3d.captured, "still captured after the release");
    assert_eq!(r.selection, Selection::None);
}

/// While captured: the wheel alone leaves the speed; Ctrl + wheel sets it.
#[test]
fn ctrl_wheel_sets_the_speed() {
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
    harness.state_mut().ready().unwrap().view3d.controls =
        crate::controls::Controls::by_name("minecraft").unwrap();
    let at = egui::pos2(620.0, 300.0);
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(at));
    harness.run();
    for pressed in [true, false] {
        harness.input_mut().events.push(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    harness.run();
    assert!(harness.state_mut().ready().unwrap().view3d.captured);
    let speed = |h: &mut Harness<'_, App>| h.state_mut().ready().unwrap().view3d.controls.speed;
    let s0 = speed(&mut harness);
    let wheel = |ctrl: bool| egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, 3.0),
        phase: egui::TouchPhase::Move,
        modifiers: if ctrl {
            egui::Modifiers::CTRL
        } else {
            egui::Modifiers::NONE
        },
    };
    harness.input_mut().events.push(wheel(false));
    harness.run();
    assert_eq!(speed(&mut harness), s0, "the wheel alone leaves the speed");
    harness.input_mut().events.push(wheel(true));
    harness.run();
    let s1 = speed(&mut harness);
    assert!((s1 / s0 - 0.3f64.exp()).abs() < 1e-9, "{s0} ➡ {s1}");
}

#[derive(Default)]
struct Memory(std::collections::HashMap<String, String>);

impl eframe::Storage for Memory {
    fn get_string(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
    fn set_string(&mut self, key: &str, value: String) {
        self.0.insert(key.to_owned(), value);
    }
    fn remove_string(&mut self, key: &str) {
        self.0.remove(key);
    }
    fn flush(&mut self) {}
}

#[test]
fn the_scheme_is_remembered() {
    let mut c = crate::controls::Controls::by_name("minecraft").unwrap();
    c.speed = 123.0;
    c.mouse_sensitivity = 0.002;
    let mut storage = Memory::default();
    c.save(&mut storage);
    assert_eq!(crate::controls::Controls::load(&storage), Some(c));
    assert_eq!(crate::controls::Controls::load(&Memory::default()), None);
    assert!(crate::controls::Controls::by_name("nope").is_none());
}

/// Jumpers can be picked and show their properties; the jumper of the
/// example takes the single-wire bundle of its section.
#[test]
fn jumpers_are_selectable() {
    let model = synthetic();
    let cands = crate::selection::candidates(&model, 0, &crate::selection::Snaps::default());
    assert!(
        cands
            .iter()
            .any(|(s, _, _)| *s == Selection::Jumper("J-T3-branch".into()))
    );
    let gpu = ohl_render::headless_device().is_ok();
    let mut builder = Harness::builder().with_size(egui::vec2(1400.0, 900.0));
    if gpu {
        builder = builder.wgpu();
    }
    let mut harness =
        builder.build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.run();
    harness.state_mut().ready().unwrap().selection = Selection::Jumper("J-T3-branch".into());
    harness.run();
    harness.get_by_label("Jumper J-T3-branch");
    assert!(
        harness
            .query_all_by_label_contains("single (from its section)")
            .next()
            .is_some()
    );
    // From the structure, the jumper is a link.
    harness.state_mut().ready().unwrap().selection = Selection::Structure("T3".into());
    harness.run();
    assert!(
        harness
            .query_all_by_label_contains("J-T3-branch: set 2")
            .next()
            .is_some()
    );
}

/// Only the kinds switched on can be clicked; a point wins over the line
/// it sits on.
#[test]
fn snaps_choose_what_can_be_clicked() {
    use crate::selection::{Snap, Snaps, candidates, pick};
    let model = synthetic();
    let kinds = |snaps: &Snaps| {
        candidates(&model, 0, snaps)
            .iter()
            .filter_map(|(s, _, _)| s.kind())
            .collect::<std::collections::BTreeSet<_>>()
    };
    // Strings (kind Attachment) are clicked with the structures.
    assert_eq!(
        kinds(&Snaps::default()),
        [Snap::Structure, Snap::Span, Snap::Jumper, Snap::Attachment]
            .into_iter()
            .collect()
    );
    let all = Snaps(Snap::ALL.into_iter().collect());
    // The example has no LiDAR points; every other kind is there.
    // (LiDAR and survey points are picked straight from the model.)
    let mut expected: std::collections::BTreeSet<Snap> = Snap::ALL.into_iter().collect();
    expected.remove(&Snap::LidarPoint);
    expected.remove(&Snap::SurveyPoint);
    assert_eq!(kinds(&all), expected);
    assert!(candidates(&model, 0, &Snaps(Default::default())).is_empty());
    // A plan projection (1 px per m): T2's first wire point.
    let a = model.line.structure("T2").unwrap().wires[0].clone();
    let project = |p: [f64; 3]| Some([p[0], -p[1]]);
    let at = [a.position[0], -a.position[1]];
    let with = |snaps: &Snaps| pick(&candidates(&model, 0, snaps), at, 5.0, project);
    assert_eq!(with(&Snaps::default()), Selection::Structure("T2".into()));
    assert_eq!(
        with(&all),
        Selection::Attachment {
            structure: "T2".into(),
            set: a.set.unwrap(),
        }
    );
    // Saved and restored.
    let mut storage = Memory::default();
    all.save(&mut storage);
    assert_eq!(Snaps::load(&storage), Some(all));
    assert_eq!(Snaps::load(&Memory::default()), None);
}

/// The new kinds show their properties.
#[test]
fn attachment_and_alignment_point_properties() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.run();
    harness.state_mut().ready().unwrap().selection = Selection::Attachment {
        structure: "T3".into(),
        set: 2,
    };
    harness.run();
    // Top bar and properties.
    assert_eq!(harness.query_all_by_label("T3 set 2").count(), 2);
    assert!(
        harness
            .query_all_by_label_contains("jumper J-T3-branch")
            .next()
            .is_some()
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let id = r.model.line.alignment_by_id("main").unwrap().points[1].clone();
        r.selection = Selection::AlignmentPoint { id };
    }
    harness.run();
    assert_eq!(
        harness
            .query_all_by_label_contains("Alignment point P")
            .count(),
        1
    );
    harness.get_by_label_contains("main #2");
    harness.get_by_label("tied to structure T3");
    // Edit › Snap lists every kind.
    harness.get_by_label("Edit").click();
    harness.run();
    harness
        .get_by_label_contains("Snap: what can be clicked")
        .click();
    harness.run();
    harness.get_by_label("Strings and attachment points");
    harness.get_by_label("Spans (wires)");
}

/// LiDAR points are picked straight from the model when switched on.
#[test]
fn lidar_points_can_be_picked() {
    use crate::selection::{Snap, Snaps, pick_in_model};
    let mut model = synthetic();
    // A point 200 m east of the origin, 30 m up (in mm from the origin).
    model.points.push(([200_000, 0, 30_000], 5));
    let p = crate::selection::lidar_xyz(&model, &[200_000, 0, 30_000]);
    let project = |q: [f64; 3]| Some([q[0], -q[2]]);
    let at = [p[0] + 1.0, -p[2]];
    let off = pick_in_model(&model, 0, &Snaps(Default::default()), at, 5.0, project);
    assert_eq!(off, Selection::None);
    let on = Snaps([Snap::LidarPoint].into_iter().collect());
    assert_eq!(
        pick_in_model(&model, 0, &on, at, 5.0, project),
        Selection::LidarPoint(([200_000, 0, 30_000], 5))
    );
}

/// G opens "Go to structure"; Enter goes there (selection, 3D camera),
/// Escape closes without going.
#[test]
fn g_goes_to_a_structure() {
    let model = synthetic();
    let gpu = ohl_render::headless_device().is_ok();
    let mut builder = Harness::builder().with_size(egui::vec2(1400.0, 900.0));
    if gpu {
        builder = builder.wgpu();
    }
    let mut harness =
        builder.build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.run();
    let key = |h: &mut Harness<'_, App>, key| {
        for pressed in [true, false] {
            h.input_mut().events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
        }
        h.run();
    };
    key(&mut harness, egui::Key::G);
    assert_eq!(
        harness.state_mut().ready().unwrap().goto.as_deref(),
        Some("T1")
    );
    harness.get_by_label("Go to structure");
    // Escape closes it.
    key(&mut harness, egui::Key::Escape);
    assert!(harness.state_mut().ready().unwrap().goto.is_none());
    // Again, choose T6, Enter.
    key(&mut harness, egui::Key::G);
    harness.state_mut().ready().unwrap().goto = Some("T6".into());
    harness.run();
    key(&mut harness, egui::Key::Enter);
    let r = harness.state_mut().ready().unwrap();
    assert!(r.goto.is_none());
    assert_eq!(r.selection, Selection::Structure("T6".into()));
    if gpu {
        let cam = r.view3d.camera.unwrap();
        let f = r.selection.focus(&r.model, 0).unwrap();
        assert_eq!(cam.target, f);
        assert!(cam.distance <= 150.0);
    }
}

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
fn assert_solved_fully(m: &model::Model) {
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
    // T6 is on the branch only: S3 touched, re-solved in the 5 cases.
    assert!(
        report.contains("1 sections touched, 5 re-solved"),
        "{report}"
    );
    assert_eq!(m.line.structure("T6").unwrap().position[0], p[0] + 4.0);
    assert_solved_fully(&m);
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
fn click_at(h: &mut Harness<'_, App>, at: egui::Pos2, button: egui::PointerButton) {
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

fn clearance_input(m: &model::Model, rules: Vec<ohl_clearance::Rule>) -> crate::clearance::Input {
    crate::clearance::Input {
        line: m.line.clone(),
        case: m.cases[0].case.name.clone(),
        shapes: m.cases[0].shapes.clone(),
        jumpers: m.cases[0].jumpers.clone(),
        rules,
        scheme: m.scheme.clone(),
        cloud: m.cloud.clone(),
        ground: m.ground.clone(),
        surfaces: Default::default(),
        corridor: 60.0,
    }
}

/// A ground rule far above the wires finds every span; one far below,
/// none.
#[test]
fn clearance_violations_of_the_shown_case() {
    let m = synthetic();
    let high = crate::clearance::run(&clearance_input(
        &m,
        ohl_clearance::RuleSet::ground(100.0).rules,
    ))
    .unwrap();
    let spans: usize = m.cases[0].shapes.iter().map(|s| s.spans.len()).sum();
    let wires: std::collections::BTreeSet<String> =
        high.findings.iter().map(|f| f.wire.describe()).collect();
    assert!(wires.len() >= spans, "{} of {spans}", wires.len());
    for f in &high.findings {
        assert!(f.distance < f.required);
        assert_eq!(f.measure, "vertical");
        // The target is the ground under the wire.
        assert!(f.target[2] < f.at[2]);
    }
    let low = crate::clearance::run(&clearance_input(
        &m,
        ohl_clearance::RuleSet::ground(0.5).rules,
    ))
    .unwrap();
    assert!(low.findings.is_empty(), "{:?}", low.findings);
    assert_eq!(low.rules.len(), 1);
}

/// The Clearances tab finds the example rules next to the model; checks run
/// on demand (Checks › Run checks), not at start; the violations are drawn.
#[test]
fn the_checks_menu_runs_the_clearance_check() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.run();
    assert!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .clearances
            .report
            .is_none()
    );
    harness.get_by_label("Checks").click();
    harness.run();
    harness.get_by_label_contains("Wire clearances").hover();
    harness.run();
    // The menu's (the quick bar has one too).
    harness
        .query_all_by_label("Check wire clearances")
        .last()
        .unwrap()
        .click();
    let started = std::time::Instant::now();
    loop {
        harness.step();
        let r = harness.state_mut().ready().unwrap();
        if r.clearances.report.is_some() && !r.clearances.running() {
            break;
        }
        assert!(started.elapsed().as_secs() < 60, "no report in 60 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let r = harness.state_mut().ready().unwrap();
    assert!(r.clearances.files[0].ends_with("clearance-rules.ohlr"));
    let rep = r.clearances.report.clone().unwrap().unwrap();
    assert_eq!(r.view3d.marks.len(), rep.findings.len());
    // Rules for other lines and across cases are noted, not run.
    assert!(
        rep.notes.iter().any(|n| n.contains("Crossed lines")),
        "{:?}",
        rep.notes
    );
    // Run, in the tab's footer.
    harness.get_by_label("Run");
}

#[test]
fn the_checks_menu_opens_the_galloping_window() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.get_by_label("Checks").click();
    harness.run();
    harness.get_by_label("Galloping ellipses…").click();
    harness.run();
    // The example parameters next to the line are chosen; the glaze ice
    // case is the one they name.
    let r = harness.state_mut().ready().unwrap();
    assert!(r.galloping.open);
    assert!(
        r.galloping
            .file
            .as_ref()
            .is_some_and(|f| f.ends_with("galloping-example.ohlg"))
    );
    r.galloping.case = r
        .model
        .cases
        .iter()
        .position(|c| c.case.name == "glaze ice")
        .unwrap();
    harness.run();
    harness.get_by_label("Run").click();
    let started = std::time::Instant::now();
    loop {
        harness.step();
        let r = harness.state_mut().ready().unwrap();
        if let (Some(rep), false) = r.galloping.report() {
            let rep = rep.as_ref().unwrap();
            // Two phases on T1–T5: four spans, two methods.
            assert_eq!(rep.rows.len(), 8);
            assert!(rep.notes.is_empty(), "{:?}", rep.notes);
            break;
        }
        assert!(started.elapsed().as_secs() < 60, "no report in 60 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    // A pair's span link goes to the span.
    harness
        .query_all_by_label("S1 span 1")
        .next()
        .unwrap()
        .click();
    harness.run();
    assert_eq!(
        harness.state_mut().ready().unwrap().selection,
        Selection::Span {
            section: "S1".into(),
            span: 1
        }
    );
}

/// Opens Checks › Ground clearances › `item`.
fn open_clearance_window(harness: &mut Harness<'_, App>, item: &str) {
    harness.get_by_label("Checks").click();
    harness.run();
    harness.get_by_label_contains("Ground clearances").hover();
    harness.run();
    harness.get_by_label(item).click();
    harness.run();
}

/// Checks › Vertical clearances: the project's rule file
/// next to the model; a ground rule run with the wind stepped finds every
/// span, the violations are drawn, a span link goes to it.
#[test]
fn the_vertical_clearances_window_runs_with_the_wind_stepped() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1600.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    open_clearance_window(&mut harness, "Vertical clearances…");
    let dir = std::env::temp_dir().join(format!("ohl-app-{}-zones-v", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let w = &mut r.vertical_clearances;
        assert!(w.open && !r.radial_clearances.open);
        assert!(
            w.file
                .as_ref()
                .is_some_and(|f| f.ends_with("synthetic-line.vertical-clearances.ohlr")),
            "{:?}",
            w.file
        );
        // It starts with the ground's rule, its distance to set: not run.
        assert_eq!(w.rules, vec![crate::clearance_zones::first_rule()]);
        assert!(w.why_not_run().is_some());
        w.rules.clear();
        // Kept in a scratch folder, not next to the example.
        w.file = Some(dir.join("rules.ohlr"));
        w.wind = ohl_clearance::WindSteps {
            stepped: true,
            step: 200.0,
            sides: ohl_clearance::WindSides::Both,
            arc: false,
            ..Default::default()
        };
        w.rules.push(ohl_clearance::ZoneRule {
            name: "High ground".into(),
            voltages: vec![220.0],
            cases: vec!["high wind".into()],
            distance: ohl_clearance::Requirement::Fixed(100.0),
            classes: Vec::new(),
            ground: true,
            surface: None,
            wires: ohl_clearance::Wires::Sections,
            width: None,
        });
    }
    harness.run();
    harness.get_by_label("Run").click();
    let started = std::time::Instant::now();
    let worst = loop {
        harness.step();
        let r = harness.state_mut().ready().unwrap();
        if let (Some(rep), false) = r.vertical_clearances.result() {
            let rep = rep.as_ref().unwrap();
            // 600 Pa in steps of 200 Pa, from both sides, and no wind.
            assert_eq!(rep.states.len(), 1 + 3 * 2);
            let spans: usize = r.model.cases[0].shapes.iter().map(|s| s.spans.len()).sum();
            assert_eq!(rep.zone.findings.len(), spans);
            assert!(rep.zone.findings.iter().all(|f| f.class.is_none()));
            assert_eq!(r.view3d.marks.len(), spans);
            // Drawn vertical (owner, 2026-10-22): each mark from the
            // ground point straight up to the wire's height — the same x
            // and y at both ends, in the 3D view (and so the cross-section
            // panel), the plan and the profile.
            for ((top, ground), f) in r.view3d.marks.iter().zip(&rep.zone.findings) {
                assert_eq!([top[0], top[1]], [ground[0], ground[1]]);
                assert_eq!(*ground, f.target);
                assert_eq!(top[2], f.wire[2]);
                assert!(top[2] > ground[2]);
            }
            assert_eq!(r.profile.marks, r.view3d.marks);
            let plan: Vec<[f64; 3]> = r.view3d.marks.iter().map(|m| m.0).collect();
            assert_eq!(r.plan.marks, plan);
            let f = &rep.zone.findings[0];
            break Selection::Span {
                section: f.section.clone(),
                span: f.span,
            };
        }
        assert!(started.elapsed().as_secs() < 60, "no report in 60 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    harness.run();
    // The worst first: its link goes to it.
    harness.get_by_label(&worst.describe()).click();
    harness.run();
    assert_eq!(harness.state_mut().ready().unwrap().selection, worst);
    std::fs::remove_dir_all(dir).ok();
}

/// Checks › Vertical clearances with the swing arc: the window says how
/// many solves a windy case needs (and suggests the arc for fine steps),
/// runs the arc (three positions), names the arc and its angle in the
/// results; its groups link to the user guide.
#[test]
fn the_vertical_clearances_window_runs_the_swing_arc() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1600.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    open_clearance_window(&mut harness, "Vertical clearances…");
    let dir = std::env::temp_dir().join(format!("ohl-app-{}-zones-arc", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let w = &mut r.vertical_clearances;
        w.rules.clear();
        w.file = Some(dir.join("rules.ohlr"));
        w.wind = ohl_clearance::WindSteps {
            stepped: true,
            step: 10.0,
            sides: ohl_clearance::WindSides::Both,
            arc: false,
            ..Default::default()
        };
        w.rules.push(ohl_clearance::ZoneRule {
            name: "High ground".into(),
            voltages: Vec::new(),
            cases: vec!["high wind".into()],
            distance: ohl_clearance::Requirement::Fixed(100.0),
            classes: Vec::new(),
            ground: true,
            surface: None,
            wires: ohl_clearance::Wires::Sections,
            width: None,
        });
    }
    harness.run();
    // 600 Pa in 10 Pa steps: 1 + 59 × 2 + 2 solves, the arc suggested.
    let text = harness
        .query_all_by_label_contains("121 solves for a windy case of 600 Pa")
        .count();
    assert!(text > 0);
    assert!(
        harness
            .query_all_by_label_contains("The swing arc needs 3")
            .next()
            .is_some()
    );
    for g in [
        "Wind: user guide ↗",
        "Zone: user guide ↗",
        "Results: user guide ↗",
    ] {
        harness.get_by_label(g);
    }
    {
        let w = &mut harness.state_mut().ready().unwrap().vertical_clearances;
        w.wind.stepped = false;
        w.wind.arc = true;
    }
    harness.run();
    // The band's margin beside the arc (2026-10-21), 0.15 m by default.
    harness.get_by_label("band margin");
    assert!(
        harness
            .query_all_by_label_contains("3 solves for a windy case of 600 Pa")
            .next()
            .is_some()
    );
    harness.get_by_label("Run").click();
    let started = std::time::Instant::now();
    loop {
        harness.step();
        let r = harness.state_mut().ready().unwrap();
        if let (Some(rep), false) = r.vertical_clearances.result() {
            let rep = rep.as_ref().unwrap();
            // Still air and the two blowouts; every span's arc.
            assert_eq!(rep.states.len(), 3);
            assert!(rep.arc_spans > 0);
            assert!(rep.zone.findings.iter().any(|f| f.arc.is_some()));
            break;
        }
        assert!(started.elapsed().as_secs() < 120, "no report in 120 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    harness.run();
    assert!(harness.query_all_by_label_contains("arc ").next().is_some());
    std::fs::remove_dir_all(dir).ok();
}

/// The time left to solve: none before the first position, then the
/// mean time of those solved times those left.
#[test]
fn the_clearance_windows_say_the_time_left() {
    use crate::clearance_zones::time_left;
    assert_eq!(time_left(3.0, 0, 10), None);
    assert_eq!(time_left(3.0, 3, 10), Some(7.0));
    assert_eq!(time_left(3.0, 10, 10), None);
}

/// Checks › Radial clearances: a rule added is kept in the
/// project's rule file; removing it asks first.
#[test]
fn the_radial_clearances_window_keeps_its_rules() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1600.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    open_clearance_window(&mut harness, "Radial clearances…");
    let dir = std::env::temp_dir().join(format!("ohl-app-{}-zones-r", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("rules.ohlr");
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.radial_clearances.open && !r.vertical_clearances.open);
        r.radial_clearances.file = Some(file.clone());
    }
    harness.run();
    // The ground's rule to start with (its distance to set): the only
    // rule, not removed.
    assert_eq!(
        harness.state_mut().ready().unwrap().radial_clearances.rules,
        vec![crate::clearance_zones::first_rule()]
    );
    {
        use egui_kittest::kittest::NodeT as _;
        assert!(
            harness
                .get_by_label("Remove")
                .accesskit_node()
                .is_disabled()
        );
    }
    harness.get_by_label("Add rule").click();
    harness.run();
    harness.run();
    let set = ohl_clearance::RuleSet::load(&file).unwrap();
    assert_eq!(
        set.rules.len(),
        0,
        "a rule without targets or distance is not written"
    );
    assert_eq!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .radial_clearances
            .rules
            .len(),
        2
    );
    {
        let w = &mut harness.state_mut().ready().unwrap().radial_clearances;
        w.rules.truncate(1);
        w.rules[0].distance = ohl_clearance::Requirement::Fixed(5.0);
    }
    harness.run();
    harness.get_by_label("Duplicate").click();
    harness.run();
    harness.run();
    let set = ohl_clearance::RuleSet::load(&file).unwrap();
    assert_eq!(set.rules.len(), 2);
    assert!(
        set.rules
            .iter()
            .all(|r| r.radial.is_some() && r.vertical.is_none())
    );
    harness.query_all_by_label("Remove").next().unwrap().click();
    harness.run();
    assert_eq!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .radial_clearances
            .rules
            .len(),
        2,
        "asks first"
    );
    harness.get_by_label("Keep").click();
    harness.run();
    harness.query_all_by_label("Remove").next().unwrap().click();
    harness.run();
    // The question under the rules (scrolled to: the window is long).
    harness
        .query_all_by_label("Remove")
        .last()
        .unwrap()
        .scroll_to_me();
    harness.run();
    harness.query_all_by_label("Remove").last().unwrap().click();
    harness.run();
    harness.run();
    assert_eq!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .radial_clearances
            .rules
            .len(),
        1
    );
    assert_eq!(ohl_clearance::RuleSet::load(&file).unwrap().rules.len(), 1);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn vegetation_and_as_built_wires_from_a_point_cloud() {
    // A scan at 35 °C: points along every span (wire class 14) and a tree
    // (class 5) 3 m below the middle of S1's first span.
    let mut model = synthetic();
    let hot = model::Solved::new(
        &model.line,
        &model.library,
        ohl_weather::WeatherCase::still("scan", ohl_core::units::Temperature::from_celsius(35.0)),
    );
    let point = |p: [f64; 3], class: u8| ohl_lidar::LidarPoint {
        x: p[0],
        y: p[1],
        z: p[2],
        class,
        intensity: 0,
        return_number: 1,
        number_of_returns: 1,
    };
    let mut points = Vec::new();
    for shape in &hot.shapes {
        for span in &shape.spans {
            for s in span.points.windows(2) {
                for k in 0..10 {
                    let t = k as f64 / 10.0;
                    let p = [0, 1, 2].map(|i| s[0][i] + (s[1][i] - s[0][i]) * t);
                    points.push(point(p, 14));
                }
            }
        }
    }
    let mid = hot.shapes[0].spans[0].mid_point;
    points.push(point([mid[0], mid[1], mid[2] - 3.0], 5));
    let dir = std::env::temp_dir().join(format!("ohl-app-{}-scan", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("scan.ohl");
    std::fs::remove_file(&file).ok();
    ohl_pointcloud::Store::open(&file)
        .unwrap()
        .import_points(
            "scan",
            "",
            None,
            64.0,
            0.001,
            points.iter().cloned(),
            &ohl_core::budget::Budget::new(),
        )
        .unwrap();
    model.cloud = Some(format!("{}#scan", file.display()));
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    r.wirefit.open = true;
    harness.run();
    harness.get_by_label("Run").click();
    harness.step();
    let r = harness.state_mut().ready().unwrap();
    r.vegetation.show(0);
    r.vegetation.settings.iso.0 = true;
    harness.step();
    harness.step();
    run_in(&harness, crate::vegetation::CHECK_TITLE).click_accesskit();
    let started = std::time::Instant::now();
    loop {
        harness.step();
        let r = harness.state_mut().ready().unwrap();
        if let ((Some(fit), false), (Some(veg), false)) =
            (r.wirefit.result(), r.vegetation.result())
        {
            let rows = fit.as_ref().unwrap();
            assert_eq!(rows.len(), 10);
            for row in rows {
                let t = row.temperature.expect("estimated");
                assert!((t - 35.0).abs() < 2.0, "{t}");
            }
            let veg = veg.as_ref().unwrap();
            assert_eq!(veg.points, 1);
            let found = &veg.spans[0];
            assert_eq!((found.section.as_str(), found.span), ("S1", 0));
            assert_eq!(found.encroaching, 1);
            // Every grow-in point listed (one here).
            assert_eq!(veg.grow.len(), 1);
            // The case shown, as it is, and the default root ball.
            assert_eq!(veg.cases.len(), 1);
            assert_eq!(
                veg.settings.root_ball,
                ohl_clearance::vegetation::DEFAULT_ROOT_BALL
            );
            break;
        }
        assert!(started.elapsed().as_secs() < 120, "no answers in 120 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    // Drawn on its layers: the tree under the span grows in.
    harness.step();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.view3d.vegetation.grow.len(), 1);
    assert_eq!(r.plan.vegetation, r.view3d.vegetation);
    assert_eq!(r.profile.vegetation, r.view3d.vegetation);
    // The isoclearance lines too, and for the drawings' plan.
    assert!(!r.view3d.vegetation.lines.is_empty());
    let lines = r.drawings.settings().isoclearance.clone().unwrap();
    assert_eq!(lines.0.len(), r.view3d.vegetation.lines.len());
    std::fs::remove_dir_all(dir).ok();
}

/// Right-clicks `target` with `tool` held (as a view reports it).
fn use_tool(
    h: &mut Harness<'_, App>,
    tool: crate::tools::Tool,
    target: Selection,
    at: Option<[f64; 3]>,
) {
    let r = h.state_mut().ready().unwrap();
    r.hotbar.hold(tool);
    r.use_tool(crate::selection::Use {
        target,
        at,
        on: None,
    });
    h.run();
}

fn string_at(structure: &str, set: u32) -> Selection {
    Selection::Attachment {
        structure: structure.into(),
        set,
    }
}

/// Add a structure on the ground, a string on it, string a wire to it from
/// T5, change the section's tension and the string, then delete the
/// string with the Delete tool (the section goes with it, after the
/// confirmation): each step is one session command, solved at once.
#[test]
fn structures_strings_and_wires_are_added_edited_and_deleted() {
    use crate::editors::Form;
    use crate::tools::Tool;
    let model = synthetic_project("tools");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let enter = |h: &mut Harness<'_, App>| press(h, egui::Key::Enter, egui::Modifiers::NONE);
    // Add structure: 150 m beyond T5, on the line's axis.
    let (t4, t5) = {
        let r = harness.state_mut().ready().unwrap();
        let l = &r.model.line;
        (
            l.structure("T4").unwrap().position,
            l.structure("T5").unwrap().position,
        )
    };
    let at = [2.0 * t5[0] - t4[0], 2.0 * t5[1] - t4[1], t5[2]];
    use_tool(&mut harness, Tool::AddStructure, Selection::None, Some(at));
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Structure(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        assert_eq!(f.id, "T8");
        f.structure_type = None;
        f.on_ground = false;
    }
    harness.get_by_label("Add structure");
    enter(&mut harness);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        assert_eq!(r.model.line.structure("T8").unwrap().position, at);
        assert_eq!(r.selection, Selection::Structure("T8".into()));
    }
    // Add string: T5's set 1 copied in its own axes (an I-string: its top,
    // its wire point, its weight).
    let t5_string =
        crate::editors::StringForm::of(&harness.state_mut().ready().unwrap().model, "T5", 1)
            .unwrap();
    let t5_top = t5_string
        .points
        .iter()
        .find(|p| p.label == t5_string.strings[0].top)
        .unwrap()
        .at;
    let t5_wire = t5_string.wires[0].at;
    use_tool(
        &mut harness,
        Tool::AddString,
        Selection::Structure("T8".into()),
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::String(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        assert_eq!(f.set, 1);
        let top = f.strings[0].top.clone();
        f.points.iter_mut().find(|p| p.label == top).unwrap().at = t5_top;
        f.wires[0].at = t5_wire;
        f.strings[0].weight = t5_string.strings[0].weight;
    }
    enter(&mut harness);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        let a = r
            .model
            .line
            .wire_point(&ohl_line::SectionPoint::new("T8", 1))
            .unwrap();
        assert!((a.position[2] - (at[2] + t5_wire[2])).abs() < 1e-9);
    }
    // String wire: T5 1:1 → T8 1:1, Enter: the section window, like S1.
    harness
        .state_mut()
        .ready()
        .unwrap()
        .select(Selection::Span {
            section: "S1".into(),
            span: 0,
        });
    use_tool(&mut harness, Tool::StringWire, string_at("T5", 1), None);
    use_tool(&mut harness, Tool::StringWire, string_at("T8", 1), None);
    assert_eq!(harness.state_mut().ready().unwrap().view3d.draft.len(), 2);
    enter(&mut harness);
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Section(f)) = &r.editor else {
            panic!("{:?}", r.editor);
        };
        assert!(f.new);
        assert_eq!(f.id, "S4");
        assert_eq!(f.cable, "acsr-drake-26-7");
        assert!(r.chain.is_empty() && r.view3d.draft.is_empty());
    }
    enter(&mut harness);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        for c in &r.model.cases {
            assert!(
                c.shapes.iter().any(|s| s.section == "S4"),
                "{}",
                c.case.name
            );
            assert!(c.failed.is_empty(), "{:?}", c.failed);
        }
        assert_solved_fully(&r.model);
    }
    // Edit the section: a higher tension, less sag.
    let sag = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        r.model.cases[0]
            .shapes
            .iter()
            .find(|s| s.section == "S4")
            .unwrap()
            .spans[0]
            .sag
    };
    let before = sag(&mut harness);
    use_tool(
        &mut harness,
        Tool::Edit,
        Selection::Span {
            section: "S4".into(),
            span: 0,
        },
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Section(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        let ohl_line::Stringing::Tension {
            horizontal_tension, ..
        } = &mut f.stringing
        else {
            panic!("{:?}", f.stringing);
        };
        *horizontal_tension *= 1.5;
    }
    enter(&mut harness);
    assert!(sag(&mut harness) < before * 0.8);
    // Edit the string: 1 m shorter.
    use_tool(&mut harness, Tool::Edit, string_at("T8", 1), None);
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::String(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        f.wires[0].at[2] += 1.0;
    }
    enter(&mut harness);
    {
        let r = harness.state_mut().ready().unwrap();
        let a = r
            .model
            .line
            .wire_point(&ohl_line::SectionPoint::new("T8", 1))
            .unwrap();
        assert!((a.position[2] - (at[2] + t5_wire[2] + 1.0)).abs() < 1e-9);
        assert_solved_fully(&r.model);
    }
    // Delete the string: S4 goes with it, after the confirmation.
    use_tool(&mut harness, Tool::Delete, string_at("T8", 1), None);
    {
        let r = harness.state_mut().ready().unwrap();
        let (impact, _) = r.confirm.as_ref().unwrap();
        assert_eq!(impact.sections, vec!["S4"]);
    }
    harness.get_by_label("Delete all").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.confirm.is_none());
        assert!(r.model.line.structure("T8").unwrap().wires.is_empty());
        assert!(r.model.line.sections.iter().all(|s| s.id != "S4"));
    }
    // Undo brings both back.
    press(&mut harness, egui::Key::Z, egui::Modifiers::COMMAND);
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.line.sections.iter().any(|s| s.id == "S4"));
    assert_solved_fully(&r.model);
}

/// A section's points picked again: one more structure in the middle;
/// unstressed lengths for the old spans become a tension.
#[test]
fn a_section_is_strung_again_through_other_points() {
    use crate::editors::Form;
    use crate::tools::Tool;
    let model = synthetic_project("restring");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // S3 (T3 2:1 → T6 → T7) without T6.
    use_tool(
        &mut harness,
        Tool::Edit,
        Selection::Span {
            section: "S3".into(),
            span: 0,
        },
        None,
    );
    harness.get_by_label("Pick points again").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none());
        assert_eq!(r.restring.as_deref(), Some("S3"));
        assert_eq!(r.hotbar.tool(), Tool::StringWire);
    }
    use_tool(&mut harness, Tool::StringWire, string_at("T3", 2), None);
    use_tool(&mut harness, Tool::StringWire, string_at("T7", 1), None);
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Section(f)) = &r.editor else {
            panic!("{:?}", r.editor);
        };
        assert!(!f.new);
        assert_eq!(f.points.len(), 2);
    }
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    let r = harness.state_mut().ready().unwrap();
    assert!(r.editor.is_none(), "{:?}", r.status);
    let s3 = r.model.line.sections.iter().find(|s| s.id == "S3").unwrap();
    assert_eq!(s3.points.len(), 2);
    assert_solved_fully(&r.model);
}

/// A section sagged to a sag in the ruling span, clipped in (lengths of
/// that condition, which it remembers), then back in the blocks.
#[test]
fn a_section_is_sagged_clipped_in_and_back_in_the_blocks() {
    use crate::editors::Form;
    use crate::tools::Tool;
    use ohl_line::Stringing;
    let model = synthetic_project("clip");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let s2 = Selection::Span {
        section: "S2".into(),
        span: 0,
    };
    let sag = Stringing::Sag {
        sag: 7.5,
        temperature: 15.0,
        span: None,
    };
    let edit = |h: &mut Harness<'_, App>,
                change: &dyn Fn(&mut crate::editors::SectionForm, &model::Model)| {
        use_tool(h, Tool::Edit, s2.clone(), None);
        {
            let r = h.state_mut().ready().unwrap();
            let Some(Form::Section(mut f)) = r.editor.clone() else {
                panic!("{:?}", r.editor);
            };
            change(&mut f, &r.model);
            r.editor = Some(Form::Section(f));
        }
        press(h, egui::Key::Enter, egui::Modifiers::NONE);
        let r = h.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        r.model
            .line
            .sections
            .iter()
            .find(|s| s.id == "S2")
            .unwrap()
            .stringing
            .clone()
    };
    let st = edit(&mut harness, &|f, _| f.stringing = sag.clone());
    assert_eq!(st, sag);
    let st = edit(&mut harness, &|f, m| f.clip_in(m).unwrap());
    let Stringing::UnstressedLengths {
        lengths, sagged, ..
    } = &st
    else {
        panic!("{st:?}");
    };
    assert_eq!(lengths.len(), 4);
    assert_eq!(sagged.as_deref(), Some(&sag));
    // The same shape at stringing as in the blocks, solved fully.
    assert_solved_fully(&harness.state_mut().ready().unwrap().model);
    let st = edit(&mut harness, &|f, m| f.stringing = f.unclipped(m, 0));
    assert_eq!(st, sag);
}

/// Criteria › Weather cases opens its page in a floating window; a saved
/// change reaches the model (solved again in the new cases).
#[test]
fn a_saved_library_reaches_the_model() {
    use crate::library::{LibraryKind, PageAction};
    let mut model = synthetic_project("library");
    let dir = model.sources.file.parent().unwrap().to_path_buf();
    let weather = dir.join("cases.ohlwc");
    std::fs::write(
        &weather,
        "format = \"ohl-weather-cases\"\nversion = 1\n\n[[case]]\nname = \"cold\"\ntemperature = \"-10 °C\"\n",
    )
    .unwrap();
    model.sources.weather = Some(weather.clone());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.get_by_label("Criteria").click();
    harness.run();
    harness.get_by_label("Weather cases library").click();
    harness.run();
    harness.run();
    assert!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .libraries
            .contains_key(&LibraryKind::WeatherCases)
    );
    harness.get_by_label("Save");
    // A second case written, as the page saves it.
    std::fs::write(
        &weather,
        "format = \"ohl-weather-cases\"\nversion = 1\n\n[[case]]\nname = \"cold\"\ntemperature = \"-10 °C\"\n\n[[case]]\nname = \"hot\"\ntemperature = \"60 °C\"\n",
    )
    .unwrap();
    let r = harness.state_mut().ready().unwrap();
    r.library_saved(LibraryKind::WeatherCases, PageAction::Saved(weather));
    assert_eq!(r.model.cases.len(), 2, "{:?}", r.status);
    assert_eq!(r.model.cases[1].case.name, "hot");
    assert_solved_fully(&r.model);
}

/// "Use for the model" with a browsed wire file: one without the line's
/// cables does not load — the model keeps its wires; a good one is used.
#[test]
fn a_browsed_library_is_used_for_the_model_only_when_it_loads() {
    use crate::library::{LibraryKind, PageAction};
    let model = synthetic_project("use-library");
    let dir = model.sources.file.parent().unwrap().to_path_buf();
    let before = model.sources.library.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    let theirs = dir.join("theirs.ohlw");
    std::fs::write(&theirs, LibraryKind::Cables.template("theirs")).unwrap();
    r.library_saved(LibraryKind::Cables, PageAction::UseForModel(theirs));
    assert!(r.status.as_ref().unwrap().1, "{:?}", r.status);
    assert_eq!(r.model.sources.library, before, "kept");
    let good = dir.join("good.ohlw");
    std::fs::copy(root().join("libraries/cables/acsr-examples.ohlw"), &good).unwrap();
    r.library_saved(LibraryKind::Cables, PageAction::UseForModel(good.clone()));
    assert!(!r.status.as_ref().unwrap().1, "{:?}", r.status);
    assert_eq!(r.model.sources.library, Some(good));
    assert_solved_fully(&r.model);
}

/// Sets a cell of an existing row of `kind`.
fn set_cell(
    h: &mut Harness<'_, App>,
    kind: crate::tables::TableKind,
    key: &str,
    col: &str,
    text: &str,
) {
    let c = kind.column(col);
    let r = h.state_mut().ready().unwrap();
    let rows = kind.rows(&r.model, r.case);
    let original = rows.iter().find(|x| x.key == key).unwrap().cells[c].clone();
    r.tables
        .entry(kind)
        .or_default()
        .set(key, c, text, &original);
}

/// Apply (deletions confirmed when `confirm`); the error if it failed.
fn apply_table(
    h: &mut Harness<'_, App>,
    kind: crate::tables::TableKind,
    confirm: bool,
) -> Result<(), String> {
    let r = h.state_mut().ready().unwrap();
    let mut page = r.tables.remove(&kind).unwrap_or_default();
    let prepared = page.prepare(kind, &r.model, r.case, confirm);
    r.tables.insert(kind, page);
    match prepared? {
        Some(c) => {
            r.apply_table(kind, c);
            match &r.tables[&kind].message {
                Some((m, true)) => Err(m.clone()),
                _ => Ok(()),
            }
        }
        None => Err("needs a confirmation".into()),
    }
}

/// Every table edits the line in one command (one undo), renames keep what
/// refers to them, a cell that does not read blocks Apply.
#[test]
fn tables_edit_the_line_in_one_command_each() {
    use crate::tables::TableKind;
    let model = synthetic_project("tables");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // Opened from the menu.
    // The menu button (the Model page has a "Structures" layer too).
    harness
        .get_by_role_and_label(egui::accesskit::Role::Button, "Structures")
        .click();
    harness.run();
    harness.get_by_label("Structures table").click();
    harness.run();
    harness.run();
    assert!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .tables
            .contains_key(&TableKind::Structures)
    );
    let (t2, t4) = {
        let r = harness.state_mut().ready().unwrap();
        (
            r.model.line.structure("T2").unwrap().position,
            r.model.line.structure("T4").unwrap().position,
        )
    };
    set_cell(
        &mut harness,
        TableKind::Structures,
        "T2",
        "E",
        &format!("{}", t2[0] + 2.0),
    );
    set_cell(
        &mut harness,
        TableKind::Structures,
        "T4",
        "z",
        &format!("{}", t4[2] + 1.0),
    );
    harness.run();
    harness.get_by_label("Apply 2 changes");
    set_cell(&mut harness, TableKind::Structures, "T4", "N", "north");
    let err = apply_table(&mut harness, TableKind::Structures, false).unwrap_err();
    assert!(err.contains("not a number"), "{err}");
    set_cell(
        &mut harness,
        TableKind::Structures,
        "T4",
        "N",
        &format!("{:.3}", t4[1]),
    );
    apply_table(&mut harness, TableKind::Structures, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.tables[&TableKind::Structures].pending(), 0);
        assert!((r.model.line.structure("T2").unwrap().position[0] - t2[0] - 2.0).abs() < 1e-9);
        assert!((r.model.line.structure("T4").unwrap().position[2] - t4[2] - 1.0).abs() < 1e-9);
        assert_solved_fully(&r.model);
    }
    press(&mut harness, egui::Key::Z, egui::Modifiers::COMMAND);
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.model.line.structure("T2").unwrap().position, t2);
        assert_eq!(
            r.model.line.structure("T4").unwrap().position,
            t4,
            "one undo"
        );
    }
    // Renamed and moved at once: the sections follow.
    set_cell(&mut harness, TableKind::Structures, "T3", "id", "Y3");
    set_cell(
        &mut harness,
        TableKind::Structures,
        "T3",
        "orientation °",
        "5",
    );
    apply_table(&mut harness, TableKind::Structures, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.model.line.structures[2].id, "Y3");
        assert_eq!(r.model.line.structures[2].orientation_deg, 5.0);
        assert!(
            r.model.line.sections[0]
                .points
                .iter()
                .any(|p| p.structure == "Y3")
        );
        assert_solved_fully(&r.model);
    }
    // Sections: condition by sag, with its value; a string's weight; a
    // jumper's sag; a system's voltage and phases.
    set_cell(
        &mut harness,
        TableKind::Sections,
        "S1",
        "condition",
        "sag, span 2",
    );
    set_cell(&mut harness, TableKind::Sections, "S1", "value", "8");
    apply_table(&mut harness, TableKind::Sections, false).unwrap();
    assert_eq!(
        harness.state_mut().ready().unwrap().model.line.sections[0].stringing,
        ohl_line::Stringing::Sag {
            sag: 8.0,
            temperature: 15.0,
            span: Some(1)
        }
    );
    set_cell(
        &mut harness,
        TableKind::Strings,
        "Y3\tS2",
        "weight N",
        "1234",
    );
    apply_table(&mut harness, TableKind::Strings, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let a = r.model.line.structure("Y3").unwrap().string("S2").unwrap();
        assert_eq!(a.weight, 1234.0);
        assert_eq!(r.model.line.sections[2].points[0].set, 2);
    }
    set_cell(
        &mut harness,
        TableKind::Jumpers,
        "J-T3-branch",
        "sag m",
        "2",
    );
    apply_table(&mut harness, TableKind::Jumpers, false).unwrap();
    assert_eq!(
        harness.state_mut().ready().unwrap().model.line.jumpers[0].sag,
        Some(2.0)
    );
    let id = harness.state_mut().ready().unwrap().model.line.systems[0]
        .id
        .clone();
    set_cell(&mut harness, TableKind::Systems, &id, "kV", "230");
    set_cell(&mut harness, TableKind::Systems, &id, "id", "sys-230");
    apply_table(&mut harness, TableKind::Systems, false).unwrap();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.model.line.systems[0].voltage_kv, 230.0, "{:?}", r.status);
    assert_eq!(r.model.line.systems[0].id, "sys-230");
    assert!(
        r.model
            .line
            .sections
            .iter()
            .any(|s| s.system.as_deref() == Some("sys-230"))
    );
    assert_solved_fully(&r.model);
}

/// Rows added and deleted; a deletion taking sections waits for its
/// confirmation.
#[test]
fn table_rows_are_added_and_deleted() {
    use crate::tables::TableKind;
    let model = synthetic_project("table-rows");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let add = |h: &mut Harness<'_, App>, kind: TableKind, cells: &[(&str, &str)]| {
        let r = h.state_mut().ready().unwrap();
        let mut row = kind.new_row(&r.model, r.case, &[]);
        for (col, text) in cells {
            row[kind.column(col)] = (*text).to_owned();
        }
        r.tables.entry(kind).or_default().added.push(row);
    };
    let t5 = harness
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .structure("T5")
        .unwrap()
        .position;
    add(
        &mut harness,
        TableKind::Structures,
        &[
            ("id", "T8"),
            ("E", &format!("{}", t5[0] + 200.0)),
            ("N", &format!("{}", t5[1])),
            ("z", &format!("{}", t5[2])),
        ],
    );
    apply_table(&mut harness, TableKind::Structures, false).unwrap();
    // A string on it (the strings table changes strings; points and wire
    // points come with the string editor).
    use_tool(
        &mut harness,
        crate::tools::Tool::AddString,
        Selection::Structure("T8".into()),
        None,
    );
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    add(
        &mut harness,
        TableKind::Sections,
        &[("id", "S4"), ("points", "T5 1:1, T8 1:1"), ("value", "20")],
    );
    apply_table(&mut harness, TableKind::Sections, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(
            r.model.line.sections.iter().any(|s| s.id == "S4"),
            "{:?}",
            r.status
        );
        assert_solved_fully(&r.model);
    }
    // Deleting T8 takes S4: listed first, then applied.
    harness
        .state_mut()
        .ready()
        .unwrap()
        .tables
        .get_mut(&TableKind::Structures)
        .unwrap()
        .deleted
        .insert("T8".into());
    assert!(apply_table(&mut harness, TableKind::Structures, false).is_err());
    {
        let r = harness.state_mut().ready().unwrap();
        let takes = r.tables[&TableKind::Structures].confirm.clone().unwrap();
        assert!(takes.iter().any(|t| t.contains("S4")), "{takes:?}");
        assert!(r.model.line.structure("T8").is_some(), "not yet");
    }
    apply_table(&mut harness, TableKind::Structures, true).unwrap();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.line.structure("T8").is_none());
    assert!(r.model.line.sections.iter().all(|s| s.id != "S4"));
}

/// A target largest sag in the section's display weather case: the
/// condition giving it is found; the display case is per section.
#[test]
fn sections_get_a_target_sag_in_their_display_case() {
    use crate::tables::TableKind;
    let model = synthetic_project("table-target");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let hot = {
        let r = harness.state_mut().ready().unwrap();
        r.model
            .cases
            .iter()
            .position(|c| c.case.name == "max temperature")
            .unwrap()
    };
    // Two display cases: drawn in both, the first reported.
    set_cell(
        &mut harness,
        TableKind::Sections,
        "S1",
        "display cases",
        "max temperature; glaze ice",
    );
    apply_table(&mut harness, TableKind::Sections, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let ice = r
            .model
            .cases
            .iter()
            .position(|c| c.case.name == "glaze ice")
            .unwrap();
        assert_eq!(r.model.section_cases("S1", 0), vec![hot, ice]);
        assert_eq!(r.model.section_case("S1", 0), hot);
        assert_eq!(r.model.section_cases("S3", 0), vec![0]);
        let shown = r.model.shapes_shown(0);
        assert_eq!(shown.iter().filter(|x| x.section == "S1").count(), 2);
        let rows = TableKind::Sections.rows(&r.model, 0);
        assert!(rows[0].cells[TableKind::Sections.column("max sag m")].contains(" / "));
    }
    set_cell(&mut harness, TableKind::Sections, "S1", "max sag m", "12");
    apply_table(&mut harness, TableKind::Sections, false).unwrap();
    let r = harness.state_mut().ready().unwrap();
    let sag = r
        .model
        .shape("S1", 0)
        .unwrap()
        .spans
        .iter()
        .map(|s| s.sag)
        .fold(0.0, f64::max);
    assert!((sag - 12.0).abs() < 0.01, "{sag}");
    assert_eq!(r.model.shape("S1", 0).unwrap().section, "S1");
    let rows = TableKind::Sections.rows(&r.model, 0);
    assert_eq!(
        rows[0].cells[TableKind::Sections.column("display cases")],
        "max temperature; glaze ice"
    );
}

/// Alignment points: moved, detached, added between two, a row deleted —
/// one command; a row deleted takes its point off that alignment only (a
/// shared point stays on the other), the point going with its last row.
#[test]
fn alignment_points_are_edited_in_their_table() {
    use crate::tables::TableKind;
    let model = synthetic_project("table-alignment");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let (n, p0, key0, key1) = {
        let r = harness.state_mut().ready().unwrap();
        let rows = TableKind::AlignmentPoints.rows(&r.model, 0);
        let mine: Vec<&crate::tables::Row> = rows.iter().filter(|x| x.cells[0] == "main").collect();
        (
            mine.len(),
            mine[0].cells.clone(),
            mine[0].key.clone(),
            mine[1].key.clone(),
        )
    };
    assert!(n >= 2, "{n}");
    let e = p0[3].parse::<f64>().unwrap() + 3.0;
    set_cell(
        &mut harness,
        TableKind::AlignmentPoints,
        &key0,
        "E",
        &format!("{e}"),
    );
    set_cell(
        &mut harness,
        TableKind::AlignmentPoints,
        &key0,
        "structure",
        "",
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let mut row = TableKind::AlignmentPoints.new_row(&r.model, 0, &[]);
        row[0] = "main".into();
        row[1] = "1.5".into();
        row[3] = format!("{e}");
        row[4] = p0[4].clone();
        let page = r.tables.entry(TableKind::AlignmentPoints).or_default();
        page.added.push(row);
        page.deleted.insert(key1.clone());
    }
    // The point's row on main deleted: off main, still on the branch
    // (nothing to confirm).
    let p1 = key1.split_once('\t').unwrap().1.to_owned();
    let shared = harness
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .point_alignments(&p1)
        .len();
    apply_table(&mut harness, TableKind::AlignmentPoints, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let line = &r.model.line;
        assert!(line.alignment_point(&p1).is_some(), "the point stays");
        let on = line.point_alignments(&p1);
        assert!(!on.contains(&"main".to_owned()), "{on:?}");
        assert_eq!(on.len(), shared - 1);
    }
    let r = harness.state_mut().ready().unwrap();
    let rows = TableKind::AlignmentPoints.rows(&r.model, 0);
    let mine: Vec<&crate::tables::Row> = rows.iter().filter(|x| x.cells[0] == "main").collect();
    assert_eq!(mine.len(), n, "one added, one deleted");
    assert_eq!(mine[0].cells[6], "", "detached");
    assert!((mine[0].cells[3].parse::<f64>().unwrap() - e).abs() < 1e-6);
    assert!(
        (mine[1].cells[3].parse::<f64>().unwrap() - e).abs() < 1e-6,
        "the new one second"
    );
    // Its last rows deleted: the point goes.
    let keys: Vec<String> = rows
        .iter()
        .filter(|x| x.key.split_once('\t').is_some_and(|(_, p)| p == p1))
        .map(|x| x.key.clone())
        .collect();
    assert!(!keys.is_empty());
    r.tables
        .entry(TableKind::AlignmentPoints)
        .or_default()
        .deleted
        .extend(keys);
    apply_table(&mut harness, TableKind::AlignmentPoints, false).unwrap();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.line.alignment_point(&p1).is_none());
}

/// Weather cases: changed, added (between two), deleted — written to the
/// case file (comments kept), the model solved again.
#[test]
fn the_weather_case_table_writes_its_file() {
    use crate::tables::TableKind;
    let mut model = synthetic_project("weather-table");
    let path = model.sources.file.parent().unwrap().join("cases.ohlwc");
    std::fs::copy(
        root().join("examples/synthetic-line/weather-cases.ohlwc"),
        &path,
    )
    .unwrap();
    model.sources.weather = Some(path.clone());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let kind = TableKind::WeatherCases;
    let n = harness.state_mut().ready().unwrap().model.cases.len();
    {
        // The air density factor's default shown, dimmed.
        let r = harness.state_mut().ready().unwrap();
        let rows = kind.rows(&r.model, 0);
        let q = kind.column("Air density factor\nPa/(m/s)²");
        assert_eq!(rows[0].cells[q], "0.613");
        assert!(rows[0].defaulted[q]);
    }
    set_cell(&mut harness, kind, "2", "Ice thickness\nmm", "20");
    set_cell(
        &mut harness,
        kind,
        "2",
        "Tags\n ",
        "clearance, loads, heavy",
    );
    set_cell(&mut harness, kind, "2", "Ambient temp\n°C", "-5");
    // A velocity typed: the pressure is Q·V² (Q 0.613, then 0.6).
    set_cell(&mut harness, kind, "1", "Wind velocity\nm/s", "40");
    set_cell(
        &mut harness,
        kind,
        "3",
        "Air density factor\nPa/(m/s)²",
        "0.6",
    );
    set_cell(&mut harness, kind, "3", "Wind velocity\nm/s", "20");
    set_cell(&mut harness, kind, "0", "Wire temp\n°C", "warm");
    assert!(apply_table(&mut harness, kind, false).is_err());
    set_cell(&mut harness, kind, "0", "Wire temp\n°C", "10.0");
    {
        let r = harness.state_mut().ready().unwrap();
        let mut row = kind.new_row(&r.model, 0, &[]);
        row[kind.column("#\n ")] = "1.5".into();
        row[kind.column("Name\n ")] = "cold".into();
        row[kind.column("Wire temp\n°C")] = "-20".into();
        let page = r.tables.entry(kind).or_default();
        page.added.push(row);
        page.deleted.insert("4".into());
    }
    apply_table(&mut harness, kind, false).unwrap();
    let r = harness.state_mut().ready().unwrap();
    let names: Vec<&str> = r.model.cases.iter().map(|c| c.case.name.as_str()).collect();
    assert_eq!(names.len(), n, "{names:?}");
    assert_eq!(names[1], "cold");
    let glaze = r
        .model
        .cases
        .iter()
        .find(|c| c.case.name == "glaze ice")
        .unwrap();
    assert!((glaze.case.ice_thickness.si() - 0.02).abs() < 1e-12);
    assert_eq!(glaze.case.tags, vec!["clearance", "loads", "heavy"]);
    assert_eq!(
        glaze.case.ambient_temperature.map(|t| t.celsius()),
        Some(-5.0)
    );
    let case = |name: &str| {
        &r.model
            .cases
            .iter()
            .find(|c| c.case.name == name)
            .unwrap()
            .case
    };
    let high = case("max temperature");
    assert!(
        (high.wind_pressure.si() - 0.613 * 1600.0).abs() < 1e-6,
        "{}",
        high.wind_pressure.si()
    );
    assert!((high.wind_velocity() - 40.0).abs() < 1e-9);
    let rime = case("rime ice + wind");
    assert_eq!(rime.air_density_factor, Some(0.6));
    assert!(
        (rime.wind_pressure.si() - 240.0).abs() < 1e-6,
        "{}",
        rime.wind_pressure.si()
    );
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(
        written.starts_with("# Example weather cases"),
        "comments kept"
    );
    assert_solved_fully(&r.model);
}

/// Alignment points with the tools: added on the ground (into the nearest
/// segment; a route following its structures gets points under them
/// first), moved in the Edit window, deleted.
#[test]
fn alignment_points_are_added_edited_and_deleted_with_the_tools() {
    use crate::editors::Form;
    use crate::tools::Tool;
    let model = synthetic_project("alignment-tools");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let (t1, t2) = {
        let r = harness.state_mut().ready().unwrap();
        let l = &r.model.line;
        (
            l.structure("T1").unwrap().position,
            l.structure("T2").unwrap().position,
        )
    };
    // Between T1 and T2, 5 m aside: nothing to snap to, the coordinates
    // given in its window.
    let at = [0.5 * (t1[0] + t2[0]), 0.5 * (t1[1] + t2[1]) + 5.0, t1[2]];
    use_tool(&mut harness, Tool::AddAlignment, Selection::None, Some(at));
    {
        let r = harness.state_mut().ready().unwrap();
        let d = r.coord_dialog.as_mut().expect("asked");
        assert_eq!(d.purpose, crate::alignment_tools::CoordFor::AddAlignment);
        assert!(!d.connect, "not connected unless ticked");
        d.text = at.map(|v| format!("{v}"));
        d.connect = true;
    }
    harness.run();
    harness.get_by_label("Add the point").click();
    harness.run();
    let id = {
        let r = harness.state_mut().ready().unwrap();
        let Selection::AlignmentPoint { id } = r.selection.clone() else {
            panic!("{:?} {:?}", r.selection, r.status);
        };
        id
    };
    let route = "main".to_owned();
    let points = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        r.model.line.alignment_by_id("main").unwrap().points.clone()
    };
    let before = points(&mut harness);
    assert_eq!(before[1], id, "between the first two");
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.model.line.alignment_point(&id).unwrap().position, at);
    }
    // Moved 2 m east in its window.
    use_tool(
        &mut harness,
        Tool::Edit,
        Selection::AlignmentPoint { id: id.clone() },
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Alignment(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        assert_eq!(f.orders, [(route.clone(), 2)]);
        f.position[0] += 2.0;
    }
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(
            r.model.line.alignment_point(&id).unwrap().position[0],
            at[0] + 2.0
        );
    }
    // Deleted: off the alignment, the others in order.
    use_tool(
        &mut harness,
        Tool::Delete,
        Selection::AlignmentPoint { id: id.clone() },
        None,
    );
    let mut kept = before.clone();
    kept.remove(1);
    assert_eq!(points(&mut harness), kept);
    // A shared point (on the branch too): the Delete tool asks which
    // alignments; off the branch only, it stays on this one.
    let shared = points(&mut harness).into_iter().find(|p| {
        harness
            .state_mut()
            .ready()
            .unwrap()
            .model
            .line
            .point_alignments(p)
            .len()
            > 1
    });
    {
        let p = shared.expect("a point on the branch too");
        use_tool(
            &mut harness,
            Tool::Delete,
            Selection::AlignmentPoint { id: p.clone() },
            None,
        );
        {
            let r = harness.state_mut().ready().unwrap();
            let (id, ticks) = r.point_delete.as_mut().expect("asked which alignments");
            assert_eq!(*id, p);
            // Only the other alignments ticked.
            for (a, on) in ticks.iter_mut() {
                *on = *a != route;
            }
        }
        harness.run();
        harness.get_by_label("Take it off the ticked ones").click();
        harness.run();
        let r = harness.state_mut().ready().unwrap();
        assert!(r.point_delete.is_none());
        assert_eq!(r.model.line.point_alignments(&p), vec![route.clone()]);
    }
    // Down to none: the alignment stays (a shared point: "Delete from
    // all", then the confirmation of what goes with it).
    while let Some(first) = points(&mut harness).first().cloned() {
        use_tool(
            &mut harness,
            Tool::Delete,
            Selection::AlignmentPoint { id: first.clone() },
            None,
        );
        if harness.state_mut().ready().unwrap().point_delete.is_some() {
            harness.get_by_label("Delete from all").click();
            harness.run();
        }
        let asked = harness.state_mut().ready().unwrap().confirm.is_some();
        if asked {
            harness.get_by_label("Delete all").click();
            harness.run();
        }
        let r = harness.state_mut().ready().unwrap();
        assert!(r.model.line.alignment_point(&first).is_none());
    }
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.line.alignment_by_id(&route).is_some());
}

/// View › Display weather cases: two cases ticked for every section
/// (the Sections table's display cases kept, not used); the strings are
/// drawn to their clamp in each; the wind from both sides draws each case
/// with wind twice.
#[test]
fn several_weather_cases_are_shown_for_the_whole_line() {
    let mut model = synthetic();
    // A section's own display case, set in the Sections table before.
    model.display_cases.insert("S1".into(), vec![0]);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let strings = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        crate::scene::layer(
            &r.model,
            crate::scene::Layer::Structures,
            r.case,
            &Selection::None,
        )
        .counts()
        .0
    };
    let one = strings(&mut harness);
    // No weather button on the quick bar any more: the View menu.
    assert!(harness.query_by_label_contains("Weather: ").is_none());
    // Only in the View menu.
    for menu in ["Sections", "Criteria"] {
        harness.get_by_label(menu).click();
        harness.run();
        assert!(
            harness.query_by_label("Display weather cases…").is_none(),
            "{menu}"
        );
    }
    harness.get_by_label("View").click();
    harness.run();
    harness
        .query_all_by_label("Display weather cases…")
        .last()
        .unwrap()
        .click();
    harness.run();
    harness.get_by_label("Display weather cases");
    harness.get_by_label("high wind").click();
    harness.run();
    harness.get_by_label("Apply").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let wind = r
            .model
            .cases
            .iter()
            .position(|c| c.case.name == "high wind")
            .unwrap();
        assert_eq!(r.model.shown(r.case), vec![0, wind]);
        assert_eq!(r.model.section_cases("S2", 0), vec![0, wind]);
        // The Sections table is not changed, its case not used.
        assert_eq!(r.model.display_cases.get("S1"), Some(&vec![0]));
        assert_eq!(r.model.section_cases("S1", 0), vec![0, wind]);
        assert!(r.display_window.is_none());
        // Every section drawn twice.
        let n = r.model.line.sections.len();
        assert_eq!(r.model.shapes_shown(r.case).len(), 2 * n);
    }
    assert!(strings(&mut harness) > one, "strings in both cases");
    // The swung strings: in high wind a suspension clamp is not plumb.
    let r = harness.state_mut().ready().unwrap();
    let wind = r.model.shown(0)[1];
    let s1 = &r.model.cases[wind].shapes[0];
    let a = r
        .model
        .line
        .wire_point(&r.model.line.sections[0].points[1])
        .unwrap()
        .clone();
    let c = s1.clamps[1];
    assert!((c[0] - a.position[0]).hypot(c[1] - a.position[1]) > 0.3);
    // Each section's own case (Sections table): S1 in its case only.
    r.model.cases_shown = crate::model::CasesShown::PerSection;
    assert_eq!(r.model.section_cases("S1", 0), vec![0]);
    assert_eq!(r.model.section_cases("S2", 0), vec![0, wind]);
    r.model.cases_shown = crate::model::CasesShown::AllSections;
    let strings_of = |m: &model::Model| {
        crate::scene::layer(m, crate::scene::Layer::Structures, 0, &Selection::None)
            .counts()
            .0
    };
    let one_side = strings_of(&r.model);
    // Wind from both sides: the windy case solved twice, swung both ways.
    r.model.set_wind(crate::model::WindDisplay::Both);
    r.model.ensure_solved(wind);
    assert!(
        strings_of(&r.model) > one_side,
        "strings drawn to both sides' clamps"
    );
    let solved = &r.model.cases[wind];
    let mirror = solved.mirror.as_ref().expect("solved from the left too");
    assert_eq!(solved.case.wind_from, ohl_weather::WindSide::Right);
    assert_eq!(mirror.case.wind_from, ohl_weather::WindSide::Left);
    let side = |s: &crate::model::Solved| {
        let c = s.shapes[0].clamps[1];
        (c[0] - a.position[0], c[1] - a.position[1])
    };
    let (r1, l1) = (side(solved), side(mirror));
    assert!(
        r1.0 * l1.0 + r1.1 * l1.1 < 0.0,
        "opposite swings {r1:?} {l1:?}"
    );
    let n = r.model.line.sections.len();
    assert_eq!(r.model.shapes_shown(0).len(), 3 * n);
    // Back to one side: the mirror goes.
    r.model.set_wind(crate::model::WindDisplay::Left);
    r.model.ensure_solved(wind);
    assert!(r.model.cases[wind].mirror.is_none());
    assert_eq!(
        r.model.cases[wind].case.wind_from,
        ohl_weather::WindSide::Left
    );
}

/// Weather case files of earlier versions: the wind side is dropped.
#[test]
fn old_weather_files_lose_the_wind_side() {
    let old = "format = \"ohl-weather-cases\"\nversion = 1\n\n[[case]]\nname = \"w\"\ntemperature = \"15 °C\"\nwind_pressure = \"400 Pa\"\nwind_from = \"left\"\n";
    let new = crate::model::without_wind_side(old).unwrap();
    assert!(!new.contains("wind_from") && new.contains("wind_pressure = \"400 Pa\""));
    assert!(crate::model::without_wind_side(&new).is_none());
    ohl_weather::WeatherCases::parse(&new).unwrap();
}

/// The project keeps its libraries: opened the first time, the wire
/// library and weather case file it reads are copied into it; opened again
/// without any library given, it reads its own (and the weather case file
/// used last); a library file saved goes into it; Export writes them out.
#[test]
fn the_project_keeps_its_libraries() {
    let mut first = synthetic_project("libraries-kept");
    let file = first.sources.file.clone();
    assert!(first.library_dir.is_some());
    assert!(
        first
            .log
            .iter()
            .any(|l| l.contains("copied into the project")),
        "{:?}",
        first.log
    );
    let session = &first.editing.as_ref().unwrap().session;
    let cables = session.library_files("cables").unwrap();
    let weather = session.library_files("weather").unwrap();
    assert!(!cables.is_empty() && weather.len() == 1);
    let weather_name = weather[0].0.clone();
    // A file of the working copy changed and saved: into the project.
    let lib = first.sources.library.clone().unwrap();
    let edited = lib.join(&cables[0].0);
    let text = std::fs::read_to_string(&edited).unwrap();
    std::fs::write(&edited, format!("# edited\n{text}")).unwrap();
    assert!(first.store_libraries().unwrap() > cables.len());
    let export = std::env::temp_dir().join(format!("ohl-app-export-{}", std::process::id()));
    std::fs::remove_dir_all(&export).ok();
    let n = first.export_libraries(&export).unwrap();
    assert!(n > cables.len());
    assert!(export.join("weather").join(&weather_name).is_file());
    drop(first);
    // Opened again with nothing given: its own libraries.
    let again = model::load(
        Sources {
            file,
            point_budget: 1000,
            corridor: 60.0,
            user: Some("tester".into()),
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    assert!(
        again
            .log
            .iter()
            .any(|l| l.contains("cables:") && l.contains("from the project"))
    );
    let w = again.sources.weather.clone().unwrap();
    assert_eq!(w.file_name().unwrap().to_string_lossy(), weather_name);
    assert!(again.cases.len() > 1);
    let lib = again.sources.library.clone().unwrap();
    let text = std::fs::read_to_string(lib.join(&cables[0].0)).unwrap();
    assert!(text.starts_with("# edited"), "the edit was kept");
    assert_solved_fully(&again);
}

/// Clearance rules, galloping, ambient cases, unit costs, poles and
/// structure models next to the project are copied into it when it opens
/// (poles and models from its structure folder, written next to the
/// types); then the windows read the project's copies, a file saved goes
/// into it, and each project line keeps its own.
#[test]
fn the_project_keeps_its_rules_costs_poles_and_models() {
    let ex = root().join("examples/synthetic-line");
    let dir = std::env::temp_dir().join(format!("ohl-app-{}-more-libraries", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(dir.join("Structures")).unwrap();
    let beside = [
        ("rules", "clearance-rules.ohlr"),
        ("galloping", "galloping-example.ohlg"),
        ("ambient", "thermal-ambient.ohlta"),
        ("costs", "unit-costs.ohlcost"),
    ];
    for (_, name) in beside {
        std::fs::copy(ex.join(name), dir.join(name)).unwrap();
    }
    let structures = [
        ("structures", "type-02.ohls"),
        ("femodels", "type-02-model.ohlfe"),
        ("poles", "steel-pole-30m.ohlp"),
    ];
    for (_, name) in structures {
        let to = dir.join("Structures").join(name);
        std::fs::copy(ex.join("Structures").join(name), to).unwrap();
    }
    // Loads typed for placed structure T2 in the structure application
    // (kind `loads`: only `*.loads.csv`, not another CSV there).
    let typed = "case,load,at,label,transverse,longitudinal,vertical,source,\
                 evaluated_transverse,evaluated_longitudinal,evaluated_vertical,missing,note\n\
                 wind,1,point,A1,2500,0,0,typed,,,,,\n";
    let structures_dir = dir.join("Structures");
    std::fs::write(structures_dir.join("type-02.T2.loads.csv"), typed).unwrap();
    std::fs::write(structures_dir.join("notes.csv"), "a,b\n").unwrap();
    let file = dir.join("p.ohl");
    let line = ohl_line::Line::load(&ex.join("synthetic-line.ohl.json")).unwrap();
    ohl_session::Session::open(&file)
        .unwrap()
        .import(&line, "tester")
        .unwrap();
    let open = || {
        model::load(
            Sources {
                file: file.clone(),
                library: Some(root().join("libraries/cables")),
                weather: Some(ex.join("weather-cases.ohlwc")),
                point_budget: 1000,
                corridor: 60.0,
                user: Some("tester".into()),
                ..Sources::default()
            },
            &|_| {},
        )
        .unwrap()
    };
    let m = open();
    let session = &m.editing.as_ref().unwrap().session;
    for (kind, name) in beside.iter().chain(&structures) {
        let files = session.library_files(kind).unwrap();
        assert!(files.iter().any(|(n, _)| n == name), "{kind}: {files:?}");
    }
    let work = m.library_dir.clone().unwrap();
    for (kind, name) in beside {
        assert_eq!(m.project_files(kind), [work.join(kind).join(name)]);
    }
    // Next to the structure types that name them.
    for (_, name) in structures {
        assert!(work.join("structures").join(name).is_file(), "{name}");
    }
    let loads = session.library_files("loads").unwrap();
    assert_eq!(
        loads,
        [("type-02.T2.loads.csv".to_owned(), typed.to_owned())]
    );
    assert!(work.join("structures/type-02.T2.loads.csv").is_file());
    drop(m);
    // The files next to the project gone: the project's are read.
    for (_, name) in beside {
        std::fs::remove_file(dir.join(name)).unwrap();
    }
    std::fs::remove_dir_all(dir.join("Structures")).unwrap();
    let mut m = open();
    let work = m.library_dir.clone().unwrap();
    assert_eq!(
        m.project_files("costs"),
        [work.join("costs").join("unit-costs.ohlcost")]
    );
    assert!(
        work.join("structures")
            .join("steel-pole-30m.ohlp")
            .is_file()
    );
    // T2's typed loads come back from the project; T4's, saved in the
    // structure application, go in.
    assert_eq!(
        std::fs::read_to_string(work.join("structures/type-02.T2.loads.csv")).unwrap(),
        typed
    );
    std::fs::write(work.join("structures/type-02.T4.loads.csv"), typed).unwrap();
    m.store_libraries().unwrap();
    let session = &m.editing.as_ref().unwrap().session;
    assert_eq!(session.library_files("loads").unwrap().len(), 2);
    let r = crate::clearance::Clearances::new(&m, None);
    assert_eq!(r.files, [work.join("rules").join("clearance-rules.ohlr")]);
    assert!(matches!(r.rules, Some(Ok(_))), "the only file chosen");
    // A rule file saved in the working copy goes into the project.
    let rules = std::fs::read_to_string(&r.files[0]).unwrap();
    std::fs::write(work.join("rules").join("more.ohlr"), &rules).unwrap();
    m.store_libraries().unwrap();
    let session = &mut m.editing.as_mut().unwrap().session;
    assert_eq!(session.library_files("rules").unwrap().len(), 2);
    // Each project line keeps its own.
    session.create_line("tester", "phase 1", "main").unwrap();
    m.switch_line("phase 1").unwrap();
    let session = &mut m.editing.as_mut().unwrap().session;
    let changed = format!("# phase 1\n{rules}");
    session
        .put_library_file("tester", "rules", "more.ohlr", &changed)
        .unwrap();
    m.refresh_project_libraries().unwrap();
    let read = |m: &model::Model| {
        let w = m.library_dir.clone().unwrap();
        std::fs::read_to_string(w.join("rules").join("more.ohlr")).unwrap()
    };
    assert_eq!(read(&m), changed);
    m.switch_line("main").unwrap();
    assert_eq!(read(&m), rules, "main keeps its own");
    let session = &m.editing.as_ref().unwrap().session;
    let main = session.library_files("rules").unwrap();
    assert!(main.iter().all(|(_, t)| !t.starts_with("# phase 1")));
    std::fs::remove_dir_all(&dir).ok();
}

/// The section library (`.ohlsec`) in the `Sections/` folder next to the
/// project is copied into it when it opens; then the project's copy is
/// read (the folder gone), and a project without one has none.
#[test]
fn the_project_keeps_its_section_library() {
    let ex = root().join("examples/synthetic-line");
    let dir = std::env::temp_dir().join(format!("ohl-app-{}-sections", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(dir.join("Sections")).unwrap();
    let name = "example-sections.ohlsec";
    std::fs::copy(
        root().join("libraries/sections").join(name),
        dir.join("Sections").join(name),
    )
    .unwrap();
    let file = dir.join("p.ohl");
    let line = ohl_line::Line::load(&ex.join("synthetic-line.ohl.json")).unwrap();
    ohl_session::Session::open(&file)
        .unwrap()
        .import(&line, "tester")
        .unwrap();
    let open = || {
        model::load(
            Sources {
                file: file.clone(),
                library: Some(root().join("libraries/cables")),
                weather: Some(ex.join("weather-cases.ohlwc")),
                point_budget: 1000,
                corridor: 60.0,
                user: Some("tester".into()),
                ..Sources::default()
            },
            &|_| {},
        )
        .unwrap()
    };
    let m = open();
    let session = &m.editing.as_ref().unwrap().session;
    let files = session.library_files("sections").unwrap();
    assert!(files.iter().any(|(n, _)| n == name), "{files:?}");
    let work = m.library_dir.clone().unwrap();
    assert!(work.join("sections").join(name).is_file());
    assert!(m.section_library().unwrap().section("EA-61x7").is_some());
    drop(m);
    // The folder gone: the project's copy.
    std::fs::remove_dir_all(dir.join("Sections")).unwrap();
    let m = open();
    let lib = m.section_library().unwrap();
    assert!(lib.section("EA-61x7").is_some());
    let mut out = dir.join("export");
    m.export_libraries(&out).unwrap();
    out.push("sections");
    assert!(out.join(name).is_file(), "exported with the others");
    drop(m);
    // Another project with no section library next to it: none.
    let other = dir.join("q.ohl");
    ohl_session::Session::open(&other)
        .unwrap()
        .import(&line, "tester")
        .unwrap();
    let m = model::load(
        Sources {
            file: other,
            library: Some(root().join("libraries/cables")),
            point_budget: 1000,
            user: Some("tester".into()),
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    let session = &m.editing.as_ref().unwrap().session;
    assert!(session.library_files("sections").unwrap().is_empty());
    assert!(m.section_library().unwrap().sections.is_empty());
    std::fs::remove_dir_all(&dir).ok();
}

/// Lines: new lines started from the one worked on, shown next to it (two
/// ticked in a row: the menu stays open), worked on one at a time (each
/// its own undo), brought up to date from the main line, submitted for
/// review.
#[test]
fn project_lines_are_made_shown_worked_on_and_brought_up_to_date() {
    use crate::project_lines::LinesWindow;
    let model = synthetic_project("lines");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        // A twin bundle, to see it drawn in the other lines too.
        let r = harness.state_mut().ready().unwrap();
        let mut s = r.model.line.sections[0].clone();
        (s.bundle, s.bundle_spacing) = (2, Some(0.4));
        let (id, value) = (s.id.clone(), serde_json::to_value(&s).unwrap());
        r.perform(crate::edit::Action::Command {
            what: "twin bundle".into(),
            command: ohl_session::Command::Put {
                kind: ohl_session::Kind::Section,
                id,
                value,
                ord: None,
            },
        });
        assert_eq!(r.model.line.sections[0].bundle, 2, "{:?}", r.status);
    }
    // Two phases from the main line, through the menu.
    for phase in ["phase 1", "phase 2"] {
        harness.get_by_label("Versions").click();
        harness.run();
        harness.get_by_label("New line from main…").click();
        harness.run();
        {
            let r = harness.state_mut().ready().unwrap();
            let Some(LinesWindow::New { name, from, .. }) = &mut r.lines_window else {
                panic!("{:?}", r.lines_window);
            };
            assert_eq!(from, "main");
            *name = phase.into();
        }
        harness.run();
        harness.get_by_label("Create").click();
        harness.run();
        let r = harness.state_mut().ready().unwrap();
        assert!(r.lines_window.is_none(), "{:?}", r.status);
    }
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.model.project_line(), "main", "still on main");
        let names: Vec<String> = r
            .model
            .session()
            .unwrap()
            .lines()
            .unwrap()
            .into_iter()
            .map(|l| l.name)
            .collect();
        assert_eq!(names, ["main", "phase 1", "phase 2"]);
    }
    // Both shown, ticked in a row: the menu stays open.
    harness.get_by_label("Versions").click();
    harness.run();
    harness.get_by_label("phase 1").click();
    harness.run();
    harness.get_by_label("phase 2").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.model.other_lines.len(), 2);
        assert_ne!(r.model.other_lines[0].color, r.model.other_lines[1].color);
        // Drawn exactly as the line worked on: every structure, string,
        // subconductor, yoke and jumper.
        r.model.other_lines.truncate(1);
        let count = |m: &model::Model, l: crate::scene::Layer| {
            crate::scene::layer(m, l, 0, &Selection::None).counts().0
        };
        // Just started, it differs in nothing: nothing drawn of it.
        assert_eq!(count(&r.model, crate::scene::Layer::OtherLines), 0);
        // Whole lines asked for: drawn exactly as the line worked on (every
        // structure, string, subconductor, yoke and jumper) once solved in
        // the case shown (in the background; exit clause: a minute).
        let shown = vec![0usize];
        r.model.ensure_solved(0);
        let ctx = egui::Context::default();
        let start = std::time::Instant::now();
        while !r.model.other_lines[0].cases[0].ready {
            r.model.want_others(&shown, &ctx);
            r.model.poll_others();
            assert!(start.elapsed().as_secs() < 60, "not solved in a minute");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        r.model.other_lines_whole = true;
        let working = count(&r.model, crate::scene::Layer::Structures)
            + count(&r.model, crate::scene::Layer::Wires);
        assert_eq!(count(&r.model, crate::scene::Layer::OtherLines), working);
        r.model.other_lines_whole = false;
    }
    // Work on phase 1: T7 deleted there only; undone there.
    {
        let r = harness.state_mut().ready().unwrap();
        r.switch_line("phase 1");
        for k in 0..r.model.cases.len() {
            r.model.ensure_solved(k);
        }
    }
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.model.project_line(), "phase 1");
        assert!(r.model.other_lines.iter().all(|o| o.name != "phase 1"));
        assert_solved_fully(&r.model);
        r.perform(crate::edit::Action::Delete {
            kind: ohl_session::Kind::Structure,
            id: "T7".into(),
            confirm: true,
        });
        assert!(r.model.line.structure("T7").is_none(), "{:?}", r.status);
        let s = r.model.session().unwrap();
        assert!(s.line_named("main").unwrap().structure("T7").is_some());
    }
    press(&mut harness, egui::Key::Z, egui::Modifiers::COMMAND);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.model.line.structure("T7").is_some(), "undone on phase 1");
        r.perform(crate::edit::Action::Delete {
            kind: ohl_session::Kind::Structure,
            id: "T7".into(),
            confirm: true,
        });
        // Main corrected meanwhile (T2 moved).
        r.switch_line("main");
        for k in 0..r.model.cases.len() {
            r.model.ensure_solved(k);
        }
        let p = r.model.line.structure("T2").unwrap().position;
        r.perform(crate::edit::Action::Command {
            what: "T2 east".into(),
            command: ohl_session::Command::MoveStructure {
                id: "T2".into(),
                position: [p[0] + 2.0, p[1], p[2]],
            },
        });
        r.switch_line("phase 1");
        for k in 0..r.model.cases.len() {
            r.model.ensure_solved(k);
        }
        assert_eq!(r.model.line.structure("T2").unwrap().position, p, "a copy");
        r.lines_window = Some(LinesWindow::BringIn {
            from: "main".into(),
            choices: Default::default(),
            plan: None,
        });
    }
    harness.run();
    assert!(harness.query_by_label_contains("No conflict").is_some());
    harness.get_by_label("Bring in").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let t2 = r.model.line.structure("T2").unwrap().position[0];
        let main_t2 = r
            .model
            .session()
            .unwrap()
            .line_named("main")
            .unwrap()
            .structure("T2")
            .unwrap()
            .position[0];
        assert_eq!(t2, main_t2, "main's correction brought in: {:?}", r.status);
        assert!(
            r.model.line.structure("T7").is_none(),
            "the phase's own change kept"
        );
        assert_solved_fully(&r.model);
        r.lines_window = Some(LinesWindow::Review {
            note: "phase 1 ready".into(),
        });
    }
    harness.run();
    harness.get_by_label("Submit").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    let reviews = r.model.session().unwrap().reviews().unwrap();
    assert_eq!(reviews.len(), 1, "{:?}", r.status);
    assert_eq!(
        (reviews[0].branch.as_str(), reviews[0].target.as_str()),
        ("phase 1", "main")
    );
    // Remembered for this user: the project opens on phase 1 again.
    assert_eq!(r.model.view_prefs().line.as_deref(), Some("phase 1"));
}

/// Lines › Project lines: the window lists every line; rights shown.
#[test]
fn the_project_lines_window_lists_the_lines() {
    use crate::project_lines::LinesWindow;
    let model = synthetic_project("lines-window");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        r.project_op(
            "phase",
            ohl_server::Op::CreateLine {
                name: "phase 1".into(),
                from: "main".into(),
            },
        );
    }
    harness.get_by_label("Versions").click();
    harness.run();
    harness.get_by_label_contains("Project lines").hover();
    harness.run();
    harness.get_by_label("Project lines…").click();
    harness.run();
    assert!(harness.query_by_label("main line").is_some());
    assert!(harness.query_by_label("phase 1").is_some());
    // Rename phase 1, then delete it (back to the list in between).
    harness.state_mut().ready().unwrap().lines_window = Some(LinesWindow::Rename {
        from: "phase 1".into(),
        to: "phase A".into(),
    });
    harness.run();
    harness.get_by_label("Rename").click();
    harness.run();
    assert_eq!(
        harness.state_mut().ready().unwrap().lines_window,
        Some(LinesWindow::Manage)
    );
    harness.state_mut().ready().unwrap().lines_window = Some(LinesWindow::Delete("phase A".into()));
    harness.run();
    harness.get_by_label("Delete").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(
        r.model.session().unwrap().lines().unwrap().len(),
        1,
        "{:?}",
        r.status
    );
}

/// Add jumper: on a structure, the jumper missing between two section
/// ends of the same circuit and phase; between two strings right-clicked.
#[test]
fn jumpers_are_added_where_none_are() {
    use crate::tools::Tool;
    let model = synthetic_project("jumpers");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // S2 given S1's circuit and phase: their ends at T1 are to be joined.
    {
        let r = harness.state_mut().ready().unwrap();
        let s1 = r.model.line.sections[0].clone();
        let mut s2 = r.model.stored_line().unwrap().sections[1].clone();
        s2.circuit_name = s1.circuit_name.clone();
        s2.phase_label = s1.phase_label.clone();
        assert!(r.perform(crate::edit::Action::Command {
            what: "same phase".into(),
            command: ohl_session::Command::Put {
                kind: ohl_session::Kind::Section,
                id: s2.id.clone(),
                value: serde_json::to_value(&s2).unwrap(),
                ord: None,
            },
        }));
    }
    let n = harness
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .jumpers
        .len();
    use_tool(
        &mut harness,
        Tool::AddJumper,
        Selection::Structure("T1".into()),
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        // T1 and T5: both ends of S1 and S2 — at T1 here.
        assert_eq!(r.model.line.jumpers.len(), n + 1, "{:?}", r.status);
        let j = r.model.line.jumpers.last().unwrap();
        assert_eq!(j.from.structure(), Some("T1"));
        assert_eq!(j.to.structure(), Some("T1"));
        assert!(j.sag.is_some());
        assert_solved_fully(&r.model);
    }
    // Again: none missing.
    use_tool(
        &mut harness,
        Tool::AddJumper,
        Selection::Structure("T1".into()),
        None,
    );
    assert_eq!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .model
            .line
            .jumpers
            .len(),
        n + 1
    );
    // Two strings.
    use_tool(&mut harness, Tool::AddJumper, string_at("T5", 1), None);
    use_tool(&mut harness, Tool::AddJumper, string_at("T5", 11), None);
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.model.line.jumpers.len(), n + 2, "{:?}", r.status);
    assert!(matches!(r.selection, Selection::Jumper(_)));
}

/// A jumper between two different circuits: told first (Continue /
/// Cancel), then both compared and the one that stays chosen; the other's
/// sections take its name, phase and system with the jumper, undone
/// together.
#[test]
fn a_jumper_joining_two_circuits_asks_which_stays() {
    use crate::tools::Tool;
    let model = synthetic_project("circuit-join");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let n = harness
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .jumpers
        .len();
    // S1 (L1) and S2 (L2) at T5: different phases.
    use_tool(&mut harness, Tool::AddJumper, string_at("T5", 1), None);
    use_tool(&mut harness, Tool::AddJumper, string_at("T5", 11), None);
    {
        let r = harness.state_mut().ready().unwrap();
        let join = r.circuit_join.as_ref().expect("asked first");
        assert_eq!(join.step, crate::circuit_join::Step::Warn);
        assert_eq!(r.model.line.jumpers.len(), n, "nothing added yet");
    }
    harness.run();
    harness.get_by_label("You are trying to connect 2 different conductor paths");
    // Cancel: nothing changes.
    harness.get_by_label("Cancel").click();
    harness.run();
    assert!(harness.state_mut().ready().unwrap().circuit_join.is_none());
    use_tool(&mut harness, Tool::AddJumper, string_at("T5", 1), None);
    use_tool(&mut harness, Tool::AddJumper, string_at("T5", 11), None);
    harness.run();
    harness.get_by_label("Continue").click();
    harness.run();
    harness.get_by_label_contains("system's phases");
    // Keep the circuit at the jumper's other end (S2's phase).
    harness
        .state_mut()
        .ready()
        .unwrap()
        .circuit_join
        .as_mut()
        .unwrap()
        .keep = 1;
    harness.run();
    harness.get_by_label_contains("Join into").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.circuit_join.is_none());
        assert_eq!(r.model.line.jumpers.len(), n + 1, "{:?}", r.status);
        let line = &r.model.line;
        assert_eq!(line.sections[0].phase_label, line.sections[1].phase_label);
        assert_eq!(line.sections[0].phase_label, "L2");
    }
    // One undo takes both back.
    press(&mut harness, egui::Key::Z, egui::Modifiers::COMMAND);
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.model.line.jumpers.len(), n);
    assert_eq!(r.model.line.sections[0].phase_label, "L1");
}

#[test]
#[ignore = "writes images of the circuit join windows to OHL_SHOTS (a look by eye)"]
fn circuit_join_window_images() {
    use crate::tools::Tool;
    let Ok(dir) = ohl_core::env::var("OHL_SHOTS") else {
        return;
    };
    let model = synthetic_project("circuit-join-shots");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    use_tool(&mut harness, Tool::AddJumper, string_at("T5", 1), None);
    use_tool(&mut harness, Tool::AddJumper, string_at("T5", 11), None);
    let shot = |h: &mut Harness<'_, App>, name: &str| {
        h.run();
        h.render()
            .unwrap()
            .save(format!("{dir}/{name}.png"))
            .unwrap();
    };
    shot(&mut harness, "circuit-join-1");
    harness.get_by_label("Continue").click();
    shot(&mut harness, "circuit-join-2");
}

/// ⌘S writes the project file through and says so; the hotbar is at the
/// bottom of the 3D view and goes with it when another page is in front.
#[test]
fn command_s_saves_and_the_hotbar_goes_with_the_3d_view() {
    let model = synthetic_project("save");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    press(&mut harness, egui::Key::S, egui::Modifiers::COMMAND);
    {
        let r = harness.state_mut().ready().unwrap();
        let (text, error) = r.status.clone().unwrap();
        assert!(text.starts_with("saved: project file"), "{text}");
        assert!(!error);
    }
    let hotbar_shown = |h: &Harness<'_, App>| {
        h.query_by_label("Select — right-click an object to select it · 1–9 or wheel (captured) · E: inventory (arrange)").is_some()
        || h.query_all_by_label_contains("E: inventory").next().is_some()
    };
    harness.state_mut().show_tab(crate::app::Tab::View3d);
    harness.run();
    let view = harness.state_mut().ready().unwrap().view3d.rect.unwrap();
    let bar = harness
        .ctx
        .memory(|m| m.area_rect(egui::Id::new("hotbar")))
        .unwrap();
    assert!(view.contains_rect(bar), "{bar:?} in {view:?}");
    assert!(hotbar_shown(&harness));
    // The plan in front: no hotbar.
    harness.state_mut().show_tab(crate::app::Tab::Plan);
    harness.run();
    harness.run();
    assert!(!hotbar_shown(&harness));
}

/// Parameters › Controls › 3D, Plan, Profile: a window each; the 3D one
/// chooses the navigation scheme (no longer in the Model page or written
/// over the 3D view).
#[test]
fn parameters_show_the_controls_of_each_view() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    assert!(harness.query_by_label("Controls (3D)").is_none());
    for (page, line) in [
        ("3D", "Navigation scheme"),
        ("Plan", "zoom at the pointer"),
        ("Profile", "move along the route"),
    ] {
        harness.get_by_label("Help").click();
        harness.run();
        harness.get_by_label_contains("Controls").click();
        harness.run();
        harness.get_by_label(page).click();
        harness.run();
        harness.get_by_label(&format!("Controls: {page}"));
        harness.get_by_label_contains(line);
        harness.state_mut().ready().unwrap().controls_window = None;
        harness.run();
    }
}

/// Going to an object (from the 3D view, a table, a failure) before the
/// plan was ever shown centred it without a scale: shown later, it was
/// empty (every object on one point). It fits its scale, keeping that
/// centre.
#[test]
fn the_plan_shows_the_line_after_a_go_to_before_it_was_shown() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.state_mut().show_tab(crate::app::Tab::View3d);
    harness.run();
    harness
        .state_mut()
        .ready()
        .unwrap()
        .go_to(Selection::Structure("T3".into()));
    harness.state_mut().show_tab(crate::app::Tab::Plan);
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.plan.scale() > 0.0, "{}", r.plan.scale());
    // Zoomed in on the line, not out to a point.
    let rect = r.plan.drawn.unwrap();
    assert!(r.plan.scale() * 100.0 < f64::from(rect.width()));
}

/// Tables show the values used where an object leaves them to a default
/// (a jumper's bundle, EI and clamp angles; a section's system and display
/// cases), marked as defaults; Apply leaves them unset unless typed in.
#[test]
fn tables_show_defaults_and_leave_them_unset() {
    use crate::tables::TableKind;
    let model = synthetic_project("defaults");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let (key, ei) = {
        let r = harness.state_mut().ready().unwrap();
        let j = r.model.line.jumpers[0].clone();
        assert!(j.bending_stiffness.is_none() && j.angle_from_deg.is_none());
        let rows = TableKind::Jumpers.rows(&r.model, r.case);
        let row = rows.iter().find(|x| x.key == j.id).unwrap();
        let c = |name| TableKind::Jumpers.column(name);
        let cable = r.model.library.get(&j.cable).unwrap();
        let ei = ohl_line::jumper::estimated_bending_stiffness(
            cable,
            ohl_line::jumper::DEFAULT_STIFFNESS_FACTOR,
        );
        assert_eq!(row.cells[c("EI N·m²")], format!("{ei:.1}"));
        assert_eq!(row.cells[c("EI factor")], "0.20");
        assert!(row.defaulted[c("EI factor")]);
        assert_eq!(row.cells[c("angle from °")], "30.0");
        assert_eq!(row.cells[c("bundle")], "1");
        assert!(row.defaulted[c("EI N·m²")] && row.defaulted[c("angle to °")]);
        let sections = TableKind::Sections.rows(&r.model, r.case);
        let s = TableKind::Sections.column("display cases");
        assert!(!sections[0].cells[s].is_empty() && sections[0].defaulted[s]);
        (j.id, ei)
    };
    // Another value typed in: the defaults stay unset.
    set_cell(&mut harness, TableKind::Jumpers, &key, "temp °C", "20");
    apply_table(&mut harness, TableKind::Jumpers, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let j = r.model.line.jumpers.iter().find(|j| j.id == key).unwrap();
        assert_eq!(j.temperature, 20.0);
        assert!(j.bending_stiffness.is_none() && j.angle_from_deg.is_none() && j.bundle.is_none());
    }
    // A default typed over is set.
    set_cell(&mut harness, TableKind::Jumpers, &key, "angle to °", "10");
    apply_table(&mut harness, TableKind::Jumpers, false).unwrap();
    let r = harness.state_mut().ready().unwrap();
    let j = r.model.line.jumpers.iter().find(|j| j.id == key).unwrap();
    assert_eq!(j.angle_to_deg, Some(10.0));
    assert!(j.bending_stiffness.is_none() && ei > 0.0);
}

/// Terrain › LiDAR classes shown: a class hidden is neither drawn nor
/// picked.
#[test]
fn hidden_lidar_classes_are_not_drawn() {
    let mut model = synthetic();
    // Ground (2) and high vegetation (5) points.
    model.points = vec![([0, 0, 0], 2), ([1000, 0, 0], 5), ([2000, 0, 0], 5)];
    let drawn = |m: &model::Model| {
        crate::scene::layer(m, crate::scene::Layer::Points, 0, &Selection::None)
            .counts()
            .1
    };
    let all = drawn(&model);
    let class = model.points[0].1;
    let of_class = model.points.iter().filter(|p| p.1 == class).count();
    assert_eq!(all, 3);
    model.hidden_classes.insert(class);
    assert_eq!(drawn(&model), all - of_class);
    model.hidden_classes.insert(5);
    assert_eq!(drawn(&model), 0);
}

/// The chunks of a layer at level 0 together are the whole layer; levels
/// 1 and 2 draw less (structures: a mast line each at level 2).
#[test]
fn chunks_at_full_detail_are_the_whole_layer_and_coarser_levels_less() {
    use crate::chunks::ChunkIndex;
    use crate::scene::Layer;
    let model = synthetic();
    let lod = crate::lod::LodSettings::default();
    for layer in [
        Layer::Structures,
        Layer::Wires,
        Layer::OtherCases,
        Layer::Points,
    ] {
        let whole = crate::scene::layer(&model, layer, 0, &Selection::None).counts();
        let index = ChunkIndex::new(&model, layer, 0);
        let row = lod.row(layer);
        let at = |level: usize| {
            let mut sum = (0, 0, 0);
            for (tile, _) in index.keys() {
                let c = index.build(&model, layer, &tile, level, &row).counts();
                sum = (sum.0 + c.0, sum.1 + c.1, sum.2 + c.2);
            }
            sum
        };
        assert_eq!(at(0), whole, "{layer:?}");
        if whole.0 > 0 {
            assert!(index.len() > 1, "{layer:?} in several chunks");
            // Stick-drawn structures (no lattice) are the same at level 1.
            if layer == Layer::Structures {
                assert!(at(1).0 <= whole.0, "{layer:?} level 1");
            } else {
                assert!(at(1).0 < whole.0, "{layer:?} level 1");
            }
            assert!(at(2).0 < whole.0, "{layer:?} level 2");
        }
    }
    // Terrain: level 0 every TIN triangle; levels 1 and 2 the surface on
    // grids of 8 and 32 m, coarser and coarser.
    let index = ChunkIndex::new(&model, Layer::Ground, 0);
    let row = lod.row(Layer::Ground);
    let triangles = |level: usize| -> usize {
        index
            .keys()
            .map(|(t, _)| {
                index
                    .build(&model, Layer::Ground, &t, level, &row)
                    .counts()
                    .2
            })
            .sum()
    };
    let whole = crate::scene::layer(&model, Layer::Ground, 0, &Selection::None)
        .counts()
        .2;
    assert!(whole > 0);
    assert_eq!(triangles(0), whole);
    let (one, two) = (triangles(1), triangles(2));
    assert!(two > 0 && two < one, "{two} < {one}");
    // Structures at level 2: one mast line (one segment) each.
    let index = ChunkIndex::new(&model, Layer::Structures, 0);
    let row = lod.row(Layer::Structures);
    let segments: usize = index
        .keys()
        .map(|(t, _)| {
            index
                .build(&model, Layer::Structures, &t, 2, &row)
                .counts()
                .0
        })
        .sum();
    assert_eq!(segments, model.line.structures.len());
}

/// Wires thinned keep their ends and the number of points asked.
#[test]
fn thinning_keeps_the_ends() {
    let pts: Vec<[f64; 3]> = (0..65).map(|i| [f64::from(i), 0.0, 0.0]).collect();
    let t = crate::scene::thin(&pts, 5);
    assert_eq!(t.len(), 5);
    assert_eq!(t[0], pts[0]);
    assert_eq!(t[4], pts[64]);
    assert_eq!(crate::scene::thin(&pts[..3], 5).len(), 3);
}

/// A marker ball with a size is drawn as a sphere on its wire, kept
/// through the project file; one without a size is not.
#[test]
fn a_sized_marker_ball_is_drawn_on_its_wire() {
    let m = synthetic_project_with("ball", |l| {
        l.sections[0].objects_mut().next().unwrap().shape =
            ohl_line::Shape::Sphere { diameter: 0.6 }
    });
    let (p, o) = m.line.sections[0].objects().next().unwrap();
    assert_eq!(o.shape, ohl_line::Shape::Sphere { diameter: 0.6 });
    let balls = |m: &model::Model| -> Vec<crate::scene::Prim> {
        crate::scene::prims(m, crate::scene::Layer::Wires, 0)
            .into_iter()
            .filter(|p| p.filled)
            .collect()
    };
    let drawn = balls(&m);
    // The ball, and the dots of the other points (spread ends).
    let pts = &drawn
        .iter()
        .find(|d| {
            let r = d.points.iter().map(|q| q[2]).fold(f64::MIN, f64::max)
                - d.points.iter().map(|q| q[2]).fold(f64::MAX, f64::min);
            (r - 0.6).abs() < 0.05
        })
        .expect("a ball 0.6 m round")
        .points;
    assert_eq!(pts.len() % 3, 0);
    // Centred on the wire at the ball's place, 0.3 m round.
    // Where the point is solved.
    let shape = &m.cases[0].shapes[0];
    let at = shape
        .span_points
        .iter()
        .find(|(l, _)| *l == p.label)
        .unwrap()
        .1;
    let mid = |k: usize| pts.iter().map(|p| p[k]).sum::<f64>() / pts.len() as f64;
    for (k, a) in at.iter().enumerate() {
        assert!((mid(k) - a).abs() < 0.05, "{k}");
    }
    assert!(pts.iter().all(|p| {
        let d = (0..3).map(|k| (p[k] - at[k]).powi(2)).sum::<f64>().sqrt();
        (d - 0.3).abs() < 1e-9
    }));
    let mut s = ohl_render::Scene::new(m.origin);
    crate::scene::draw_prims(&mut s, &drawn, 0, 60);
    assert_eq!(s.counts().2, pts.len() / 3);
    assert!(balls(&synthetic_project("no-ball")).is_empty());
}

/// Levels of detail of the small things on the wires: a body to its shape
/// close, a dot farther, nothing at level 2; clamps and bare points close
/// only; line codes' dashes longer farther (fewer of them).
#[test]
fn small_things_on_the_wires_are_drawn_close() {
    let mut m = synthetic_project_with("lod-ball", |l| {
        l.sections[0].objects_mut().next().unwrap().shape =
            ohl_line::Shape::Sphere { diameter: 0.6 }
    });
    let prims = crate::scene::prims(&m, crate::scene::Layer::Wires, 0);
    let ball = prims.iter().find(|p| p.filled).expect("the ball");
    assert_eq!(ball.levels, (0, 0));
    let dot = prims.iter().find(|p| p.dot).expect("its dot");
    assert_eq!(dot.levels, (1, 1));
    assert!(
        dot.points[0]
            .iter()
            .zip(ball.points[0])
            .all(|(a, b)| (a - b).abs() < 1.0)
    );
    // Nothing filled or dotted at level 2.
    assert!(
        prims
            .iter()
            .filter(|p| p.filled || p.dot)
            .all(|p| p.levels.1 < 2)
    );
    // A phase in a line code (S2 by system): fewer, longer dashes farther.
    m.section_display.coloring = crate::wire_style::Coloring::Systems;
    let prims = crate::scene::prims(&m, crate::scene::Layer::Wires, 0);
    let at = |level: usize| {
        prims
            .iter()
            .filter(|p| p.levels == (level, level) && p.thinned)
            .count()
    };
    assert!(at(1) > 2 * at(2) && at(2) > 0, "{} {}", at(1), at(2));
}

/// The point at a fraction of a span: along its plan chord.
#[test]
fn a_fraction_of_a_span_is_along_its_chord() {
    let pts = [[0.0, 0.0, 10.0], [50.0, 5.0, 4.0], [100.0, 0.0, 10.0]];
    let p = crate::scene::at_fraction(&pts, 0.25).unwrap();
    assert!((p[0] - 25.0).abs() < 1e-12 && (p[2] - 7.0).abs() < 1e-12);
    assert_eq!(crate::scene::at_fraction(&pts, 1.0).unwrap(), pts[2]);
    assert!(crate::scene::at_fraction(&pts[..1], 0.5).is_none());
}

/// The 3D view draws the chunked layers chunk by chunk: they load over a
/// few frames; a render distance of 0 draws none; other details rebuild.
#[test]
fn the_3d_view_draws_chunks_by_the_lod_table() {
    if ohl_render::headless_device().is_err() {
        assert!(
            ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
            "no GPU"
        );
        return;
    }
    use crate::scene::Layer;
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .wgpu()
        .build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.state_mut().show_tab(crate::app::Tab::View3d);
    for _ in 0..20 {
        harness.step();
    }
    let stats = |h: &mut Harness<'_, App>, l: Layer| {
        h.state_mut().ready().unwrap().view3d.lod_stats(l).unwrap()
    };
    let s = stats(&mut harness, Layer::Wires);
    assert!(s.chunks > 1 && s.pending == 0, "{s:?}");
    assert_eq!(
        s.drawn.iter().sum::<usize>(),
        s.chunks,
        "the whole line in view"
    );
    // Render distance 0: nothing drawn.
    let mut row = harness.state().lod.row(Layer::Wires);
    row.render_distance = 0.0;
    row.from = [0.0, 0.0];
    harness.state_mut().lod.set(Layer::Wires, row).unwrap();
    harness.step();
    assert_eq!(stats(&mut harness, Layer::Wires).drawn, [0, 0, 0]);
    // Everything at level 2 (from 0 m), with other details: rebuilt.
    row.render_distance = 1e6;
    row.detail = [8.0, 3.0];
    harness.state_mut().lod.set(Layer::Wires, row).unwrap();
    for _ in 0..20 {
        harness.step();
    }
    let s = stats(&mut harness, Layer::Wires);
    assert_eq!(s.drawn[2], s.chunks, "{s:?}");
}

/// Every LiDAR class is loaded: a small one whole, a large one thinned
/// (a real survey lost classes of 6 to 500 points when every class was
/// thinned alike).
#[test]
fn every_lidar_class_is_loaded() {
    let counts = [(2u8, 1_000_000u64), (55, 6), (84, 507), (72, 40)];
    let mut t = model::Thinning::new(&counts, 50);
    let mut kept = std::collections::BTreeMap::new();
    for (c, n) in counts {
        for _ in 0..n {
            if t.keep(c) {
                *kept.entry(c).or_insert(0u64) += 1;
            }
        }
    }
    assert_eq!(kept[&2], 20_000);
    assert_eq!((kept[&55], kept[&84], kept[&72]), (6, 507, 40));
    // Noise has a colour: it can be shown.
    let scheme = ohl_lidar::ClassScheme::asprs();
    for code in [7u8, 18] {
        assert!(crate::scene::class_color(&scheme, code).is_some(), "{code}");
    }
}

/// File: a new project (an empty line, the user its admin), opened from
/// the start page; Close goes back to it, asking first when a table has
/// changes not applied; Open recent opens it again.
#[test]
fn files_are_created_opened_and_closed() {
    use crate::files::FileAction;
    let dir = std::env::temp_dir().join(format!("ohl-app-{}-files", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("fresh.ohl");
    std::fs::remove_file(&path).ok();
    let created =
        crate::files::create_project(&dir.join("fresh"), "Fresh line", "EPSG:2056", "tester")
            .unwrap();
    assert_eq!(created, path);
    // Never replaced.
    assert!(crate::files::create_project(&path, "Again", "EPSG:2056", "tester").is_err());
    let template = Sources {
        point_budget: 1000,
        corridor: 60.0,
        user: Some("tester".into()),
        library: Some(root().join("libraries/cables")),
        ..Sources::default()
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::empty(template, None));
    harness.run();
    assert!(harness.state().is_empty());
    harness.get_by_label("New project…");
    // Open (as the menu or the start page would).
    harness
        .state_mut()
        .file_action_for_test(FileAction::Open(Some(path.clone())));
    // Loading runs in the background: wait for it (exit clause 30 s).
    let started = std::time::Instant::now();
    while harness.state_mut().ready().is_none()
        && harness.state().failure().is_none()
        && started.elapsed() < std::time::Duration::from_secs(30)
    {
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(harness.state().failure(), None);
    {
        let r = harness.state_mut().ready().expect("loaded");
        assert_eq!(r.model.line.name, "Fresh line");
        assert!(r.model.line.structures.is_empty());
        assert!(r.model.can_edit().is_ok(), "the creator edits it");
        // A table change not applied.
        r.tables
            .entry(crate::tables::TableKind::Structures)
            .or_default()
            .added
            .push(vec![String::new(); 9]);
    }
    harness.run();
    harness.state_mut().file_action_for_test(FileAction::Close);
    harness.run();
    harness.get_by_label("Discard them").click();
    harness.run();
    assert!(harness.state().is_empty());
    assert_eq!(harness.state().recent.0.len(), 1);
    // Open recent.
    let again = harness.state().recent.0[0].clone();
    harness
        .state_mut()
        .file_action_for_test(FileAction::Open(Some(again)));
    // Loading runs in the background: wait for it (exit clause 30 s).
    let started = std::time::Instant::now();
    while harness.state_mut().ready().is_none()
        && harness.state().failure().is_none()
        && started.elapsed() < std::time::Duration::from_secs(30)
    {
        harness.step();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(harness.state_mut().ready().is_some());
}

/// Terrain › Survey data › Survey data display options: a window that
/// stays open while classes are shown and hidden, several in a row.
#[test]
fn the_survey_window_stays_open_for_several_classes() {
    let mut model = synthetic();
    // Ground (2), low and high vegetation (3, 5), building (6).
    model.points = vec![
        ([0, 0, 0], 2),
        ([1000, 0, 0], 3),
        ([2000, 0, 0], 5),
        ([3000, 0, 0], 6),
    ];
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.get_by_label("Terrain").click();
    harness.run();
    harness.get_by_label_contains("Survey data").click();
    harness.run();
    harness.get_by_label("Survey data display options…").click();
    harness.run();
    let hidden = |h: &mut Harness<'_, App>| {
        h.state_mut()
            .ready()
            .unwrap()
            .model
            .hidden_classes
            .iter()
            .copied()
            .collect::<Vec<u8>>()
    };
    harness.get_by_label("None").click();
    harness.run();
    // Not shown before Apply.
    assert!(hidden(&mut harness).is_empty());
    harness.get_by_label("changes not applied");
    harness.get_by_label("Apply").click();
    harness.run();
    assert_eq!(hidden(&mut harness), vec![2, 3, 5, 6]);
    // Still open: vegetation back, then everything.
    harness.get_by_label("vegetation").click();
    harness.run();
    harness.get_by_label("Invert").click();
    harness.run();
    harness.get_by_label("Apply").click();
    harness.run();
    assert_eq!(hidden(&mut harness), vec![3, 5]);
    // Revert drops what is not applied.
    harness.get_by_label("All").click();
    harness.run();
    harness.get_by_label("Revert").click();
    harness.run();
    harness.get_by_label("Apply").click();
    harness.run();
    assert_eq!(hidden(&mut harness), vec![3, 5]);
    assert!(harness.state_mut().ready().unwrap().survey_window.is_some());
}

/// In the survey window, pressing on a class and dragging down gives every
/// class passed the first one's new state.
#[test]
fn dragging_over_classes_gives_them_one_state() {
    let mut model = synthetic();
    model.points = vec![
        ([0, 0, 0], 2),
        ([1000, 0, 0], 3),
        ([2000, 0, 0], 4),
        ([3000, 0, 0], 5),
        ([4000, 0, 0], 6),
    ];
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        r.survey_window = Some(crate::app::SurveyDraft::of(&r.model));
    }
    harness.run();
    let centre = |h: &Harness<'_, App>, label: &str| h.get_by_label_contains(label).rect().center();
    let (from, to) = (centre(&harness, "3 Low"), centre(&harness, "5 High"));
    let press = |pos: egui::Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    harness.event(egui::Event::PointerMoved(from));
    harness.step();
    harness.event(press(from, true));
    harness.step();
    for k in 1..=10 {
        let p = from + (to - from) * (k as f32 / 10.0);
        harness.event(egui::Event::PointerMoved(p));
        harness.step();
    }
    harness.event(press(to, false));
    harness.run();
    harness.get_by_label("Apply").click();
    harness.run();
    let hidden: Vec<u8> = harness
        .state_mut()
        .ready()
        .unwrap()
        .model
        .hidden_classes
        .iter()
        .copied()
        .collect();
    assert_eq!(hidden, vec![3, 4, 5], "the classes dragged over, hidden");
}

/// The survey window stays on the screen with many classes: its list
/// scrolls.
#[test]
fn the_survey_list_scrolls() {
    let mut model = synthetic();
    model.points = (1..=60u8)
        .map(|c| ([i32::from(c) * 1000, 0, 0], c))
        .collect();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        r.survey_window = Some(crate::app::SurveyDraft::of(&r.model));
    }
    harness.run();
    let slider = harness
        .get_by_label_contains("Points shown near the camera")
        .rect();
    assert!(slider.bottom() < 900.0, "{slider:?}");
    // It keeps its size (it grew every frame until it filled the screen).
    for _ in 0..60 {
        harness.step();
    }
    let later = harness
        .get_by_label_contains("Points shown near the camera")
        .rect();
    assert_eq!(later, slider);
}

/// A survey stored in the project: its points are read by node as they
/// come in view, the share chosen of them (100 %: every point).
#[test]
fn stored_points_are_streamed_at_the_share_chosen() {
    if ohl_render::headless_device().is_err() {
        assert!(
            ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
            "no GPU"
        );
        return;
    }
    use crate::scene::Layer;
    // The project, then a survey of 0.5 m grid points around T3: ground
    // and vegetation alternating, 48 000 each (thinned: above 20 000).
    let first = synthetic_project("stream");
    let file = first.sources.file.clone();
    drop(first);
    let dir = file.parent().unwrap().to_path_buf();
    let laz = dir.join("survey.laz");
    {
        let mut b = las::Builder::from((1, 4));
        b.point_format.is_compressed = true;
        for t in [
            &mut b.transforms.x,
            &mut b.transforms.y,
            &mut b.transforms.z,
        ] {
            t.scale = 0.001;
        }
        let mut w = las::Writer::from_path(&laz, b.into_header().unwrap()).unwrap();
        for i in 0..600 {
            for j in 0..160 {
                let veg = (i + j) % 2 == 0;
                w.write_point(las::Point {
                    x: 500.0 + f64::from(i) * 0.5,
                    y: -40.0 + f64::from(j) * 0.5,
                    z: if veg { 530.0 } else { 520.0 },
                    classification: if veg {
                        las::point::Classification::HighVegetation
                    } else {
                        las::point::Classification::Ground
                    },
                    ..Default::default()
                })
                .unwrap();
            }
        }
        w.close().unwrap();
    }
    let mut store = ohl_pointcloud::Store::open(&file).unwrap();
    store
        .import_tiles(
            "survey",
            &[laz],
            "EPSG:2056",
            None,
            64.0,
            0.001,
            &ohl_core::budget::Budget::new(),
            |_, _, _| {},
        )
        .unwrap();
    drop(store);
    let model = model::load(
        Sources {
            file,
            point_budget: 1000,
            corridor: 60.0,
            user: Some("tester".into()),
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    assert!(model.cloud.is_some());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .wgpu()
        .build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.state_mut().show_tab(crate::app::Tab::View3d);
    harness.step();
    {
        // Every node at full detail (the smallest error allowed), the
        // survey seen from above.
        let mut row = harness.state().lod.row(Layer::Points);
        row.render_distance = 1e6;
        harness.state_mut().lod.set(Layer::Points, row).unwrap();
        harness.state_mut().lod.points.max_error = 0.1;
        let r = harness.state_mut().ready().unwrap();
        r.model.points_percent = 100.0;
        let cam = r.view3d.camera.as_mut().unwrap();
        cam.target = [650.0, 0.0, 520.0];
        cam.distance = 1500.0;
        cam.pitch = 1.5;
    }
    let settle = |h: &mut Harness<'_, App>| {
        for _ in 0..400 {
            h.step();
            let s = h
                .state_mut()
                .ready()
                .unwrap()
                .view3d
                .stream_stats()
                .unwrap();
            if s.pending == 0 && s.drawn > 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        h.state_mut()
            .ready()
            .unwrap()
            .view3d
            .drawn_points(Layer::Points)
    };
    let all = settle(&mut harness);
    assert_eq!(all, 600 * 160, "100 %: every point");
    harness.state_mut().ready().unwrap().model.points_percent = 10.0;
    let tenth = settle(&mut harness);
    assert!(
        (f64::from(tenth) / f64::from(all) - 0.1).abs() < 0.01,
        "{tenth} of {all}"
    );
}

/// Synthetic points: a `w` × `h` m grid at 0.5 m from (x0, y0), ground
/// and vegetation alternating.
fn grid_points(x0: f64, y0: f64, w: u32, h: u32) -> impl Iterator<Item = ohl_lidar::LidarPoint> {
    (0..2 * w).flat_map(move |i| {
        (0..2 * h).map(move |j| ohl_lidar::LidarPoint {
            x: x0 + f64::from(i) * 0.5,
            y: y0 + f64::from(j) * 0.5,
            z: 520.0 + f64::from((i * 7 + j * 3) % 11) * 0.3,
            class: if (i + j) % 2 == 0 { 5 } else { 2 },
            intensity: 0,
            return_number: 1,
            number_of_returns: 1,
        })
    })
}

/// A cloud far larger than the point budget (4 million points, a budget
/// of 300 000): the 3D view draws at most the budget, the nearest nodes
/// finest, and keeps at most 1.5 × the budget on the GPU; frames stay
/// quick while nodes stream in.
#[test]
fn a_large_cloud_is_drawn_within_the_point_budget() {
    if ohl_render::headless_device().is_err() {
        assert!(
            ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
            "no GPU"
        );
        return;
    }
    use crate::scene::Layer;
    let first = synthetic_project("budget");
    let file = first.sources.file.clone();
    drop(first);
    let mut store = ohl_pointcloud::Store::open(&file).unwrap();
    store
        .import_points(
            "big",
            "",
            None,
            64.0,
            0.001,
            grid_points(-200.0, -500.0, 1000, 1000),
            &ohl_core::budget::Budget::new(),
        )
        .unwrap();
    drop(store);
    let model = model::load(
        Sources {
            file,
            point_budget: 1000,
            corridor: 60.0,
            user: Some("tester".into()),
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .wgpu()
        .build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.state_mut().show_tab(crate::app::Tab::View3d);
    harness.step();
    let budget = 300_000;
    {
        harness.state_mut().lod.points.budget = budget;
        let mut row = harness.state().lod.row(Layer::Points);
        row.render_distance = 1e6;
        harness.state_mut().lod.set(Layer::Points, row).unwrap();
        let r = harness.state_mut().ready().unwrap();
        r.model.points_percent = 100.0;
        let cam = r.view3d.camera.as_mut().unwrap();
        cam.target = [300.0, 0.0, 520.0];
        cam.distance = 400.0;
        cam.pitch = 0.4;
    }
    let mut slowest = std::time::Duration::ZERO;
    let mut stats = None;
    for _ in 0..600 {
        let t = std::time::Instant::now();
        harness.step();
        slowest = slowest.max(t.elapsed());
        let s = harness
            .state_mut()
            .ready()
            .unwrap()
            .view3d
            .stream_stats()
            .unwrap();
        stats = Some(s);
        if s.pending == 0 && s.drawn > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let s = stats.unwrap();
    assert_eq!(s.pending, 0, "every node read: {s:?}");
    assert!(s.capped, "the budget limits the view: {s:?}");
    let drawn = harness
        .state_mut()
        .ready()
        .unwrap()
        .view3d
        .drawn_points(Layer::Points);
    assert!(u64::from(drawn) <= budget, "{drawn} points drawn");
    assert!(u64::from(drawn) > budget / 2, "{drawn} points drawn");
    assert!(s.cached.1 <= budget * 3 / 2, "{s:?}");
    println!("4 M points, budget 300 000: slowest frame {slowest:.1?}, {s:?}");
}

/// Frame times with a large cloud (release: `cargo test --release -p
/// ohl-app -- --ignored --nocapture point_budget_frame_times`): 25 million
/// synthetic points in the project, a 10 million point budget; the time
/// until the view is read, then frames while the camera turns.
#[test]
#[ignore = "benchmark: run in release with --ignored --nocapture"]
fn point_budget_frame_times() {
    use crate::scene::Layer;
    use std::time::Instant;
    let first = synthetic_project("frames");
    let file = first.sources.file.clone();
    drop(first);
    let t = Instant::now();
    let mut store = ohl_pointcloud::Store::open(&file).unwrap();
    store
        .import_points(
            "big",
            "",
            None,
            64.0,
            0.001,
            grid_points(-1000.0, -1250.0, 2500, 2500),
            &ohl_core::budget::Budget::new(),
        )
        .unwrap();
    drop(store);
    println!("import of 25 M points with levels: {:.1?}", t.elapsed());
    let t = Instant::now();
    let model = model::load(
        Sources {
            file,
            point_budget: 1_000_000,
            corridor: 60.0,
            user: Some("tester".into()),
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    println!("model opened in {:.1?}", t.elapsed());
    let mut rs = None;
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 1000.0))
        .wgpu()
        .build_eframe(|cc| {
            rs = cc.wgpu_render_state.clone();
            App::with_model(model, cc.wgpu_render_state.clone())
        });
    let device = rs.unwrap().device;
    // A frame's time includes the GPU's work on it.
    let wait = || {
        device
            .poll(eframe::wgpu::PollType::wait_indefinitely())
            .unwrap();
    };
    harness.state_mut().show_tab(crate::app::Tab::View3d);
    harness.step();
    {
        // A fine error: the budget (10 million, or OHL_BENCH_BUDGET), not
        // the error, limits the view.
        harness.state_mut().lod.points.budget = ohl_core::env::var("OHL_BENCH_BUDGET")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10_000_000);
        harness.state_mut().lod.points.max_error = 0.3;
        let r = harness.state_mut().ready().unwrap();
        r.model.points_percent = 100.0;
        let cam = r.view3d.camera.as_mut().unwrap();
        cam.target = [250.0, 0.0, 520.0];
        cam.distance = 600.0;
        cam.pitch = 0.5;
    }
    let stats = |h: &mut Harness<'_, App>| {
        h.state_mut()
            .ready()
            .unwrap()
            .view3d
            .stream_stats()
            .unwrap()
    };
    let t = Instant::now();
    let mut frames = 0;
    loop {
        harness.step();
        wait();
        frames += 1;
        let s = stats(&mut harness);
        if (s.pending == 0 && s.drawn > 0) || t.elapsed().as_secs() > 120 {
            break;
        }
    }
    let read = t.elapsed();
    let s = stats(&mut harness);
    let drawn = harness
        .state_mut()
        .ready()
        .unwrap()
        .view3d
        .drawn_points(Layer::Points);
    println!(
        "view read in {read:.1?} ({frames} frames): {drawn} points drawn, {:.1} M points/s; {s:?}",
        f64::from(drawn) / read.as_secs_f64() / 1e6
    );
    let mut times = Vec::new();
    for k in 0..120 {
        harness
            .state_mut()
            .ready()
            .unwrap()
            .view3d
            .camera
            .as_mut()
            .unwrap()
            .orbit(0.01, 0.0);
        let t = Instant::now();
        harness.step();
        wait();
        if k >= 20 {
            times.push(t.elapsed());
        }
    }
    times.sort();
    println!(
        "turning: median frame {:.1?}, 95 % {:.1?}, slowest {:.1?}; {:?}",
        times[times.len() / 2],
        times[times.len() * 95 / 100],
        times[times.len() - 1],
        stats(&mut harness)
    );
}

/// A COPC file as the point cloud: opened by its octree (the points shown
/// from its shallow nodes, the ground from its ground classes in the
/// corridor) and streamed by node in the 3D view.
#[test]
fn a_copc_file_is_the_point_cloud() {
    let first = synthetic_project("copc");
    let file = first.sources.file.clone();
    drop(first);
    let copc = file.with_file_name("survey.copc.laz");
    let points: Vec<_> = grid_points(-100.0, -60.0, 600, 120).collect();
    ohl_pointcloud::copc::write_copc(&copc, &points, 0.001, &ohl_core::budget::Budget::new())
        .unwrap();
    let model = model::load(
        Sources {
            file,
            pointcloud: Some(copc.display().to_string()),
            point_budget: 5000,
            corridor: 60.0,
            user: Some("tester".into()),
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    assert_eq!(
        model.cloud.as_deref(),
        Some(copc.display().to_string().as_str())
    );
    let n = model.points.len();
    assert!((4000..=12_000).contains(&n), "{n} points shown");
    assert!(model.class_counts.iter().any(|(c, _)| *c == 2));
    assert!(model.ground.is_some(), "ground from the COPC ground class");
    if ohl_render::headless_device().is_err() {
        assert!(
            ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
            "no GPU"
        );
        return;
    }
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .wgpu()
        .build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.state_mut().show_tab(crate::app::Tab::View3d);
    for _ in 0..400 {
        harness.step();
        let s = harness.state_mut().ready().unwrap().view3d.stream_stats();
        if s.is_some_and(|s| s.pending == 0 && s.drawn > 0) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let s = harness
        .state_mut()
        .ready()
        .unwrap()
        .view3d
        .stream_stats()
        .unwrap();
    assert!(s.drawn > 0 && s.points > 0, "{s:?}");
}

/// Writes a LAZ survey of 0.5 m grid points over [x0, x0 + w) × [-40, 40):
/// ground and high vegetation alternating.
fn write_survey(path: &std::path::Path, x0: f64, w: usize) {
    let mut b = las::Builder::from((1, 4));
    b.point_format.is_compressed = true;
    for t in [
        &mut b.transforms.x,
        &mut b.transforms.y,
        &mut b.transforms.z,
    ] {
        t.scale = 0.001;
    }
    let mut wr = las::Writer::from_path(path, b.into_header().unwrap()).unwrap();
    for i in 0..w {
        for j in 0..160 {
            let veg = (i + j) % 2 == 0;
            wr.write_point(las::Point {
                x: x0 + i as f64 * 0.5,
                y: -40.0 + j as f64 * 0.5,
                z: if veg { 530.0 } else { 520.0 },
                classification: if veg {
                    las::point::Classification::HighVegetation
                } else {
                    las::point::Classification::Ground
                },
                ..Default::default()
            })
            .unwrap();
        }
    }
    wr.close().unwrap();
}

/// File › Import / Export › LiDAR points (LAS/LAZ): tiles imported into
/// the project's point cloud; exported again, every class or one, the
/// whole survey or around the line, with the class scheme next to it.
#[test]
fn surveys_are_imported_and_exported() {
    let first = synthetic_project("survey-io");
    let file = first.sources.file.clone();
    // A line file cannot take a point cloud; the project can.
    let line_file = synthetic();
    assert!(
        crate::survey_io::ImportDialog::new(&line_file)
            .error
            .is_some()
    );
    assert!(crate::survey_io::ImportDialog::new(&first).error.is_none());
    drop(first);
    let dir = file.parent().unwrap().to_path_buf();
    let laz = dir.join("tile.laz");
    write_survey(&laz, 600.0, 200);
    let progress = std::cell::RefCell::new(Vec::new());
    let report = crate::survey_io::import(
        &file,
        &[laz],
        "lidar",
        "EPSG:2056",
        64.0,
        Some(
            &std::fs::read_to_string(root().join("libraries/lidar-classes/asprs-las-1.4.ohlc"))
                .unwrap(),
        ),
        &|t, f| progress.borrow_mut().push((t, f)),
    )
    .unwrap();
    assert!(report.contains("32000 points"), "{report}");
    assert_eq!(progress.borrow().last().unwrap().1, 1.0);
    // Opened again: the cloud is the project's.
    let model = model::load(
        Sources {
            file: file.clone(),
            point_budget: 100_000,
            corridor: 60.0,
            user: Some("tester".into()),
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    assert!(model.cloud.is_some());
    assert_eq!(model.class_counts.iter().map(|c| c.1).sum::<u64>(), 32_000);
    // Exported: all, then the vegetation only, around the line.
    let out = dir.join("all.laz");
    let r = crate::survey_io::export(&file, None, &out, None, None, true, &|_, _| {}).unwrap();
    assert!(r.contains("32000 points"), "{r}");
    // The class scheme stored at import, next to it.
    let ohlc = std::fs::read_to_string(out.with_extension("ohlc")).unwrap();
    assert!(ohl_lidar::ClassScheme::parse(&ohlc).is_ok());
    let only = dir.join("veg.las");
    let r = crate::survey_io::export(
        &file,
        None,
        &only,
        Some(&[5]),
        Some(([600.0, -40.0], [650.0, 40.0])),
        false,
        &|_, _| {},
    )
    .unwrap();
    // 100 columns of 160 points in 600–650 m, half of them vegetation.
    assert!(r.contains("8000 points"), "{r}");
    assert!(!only.with_extension("ohlc").exists());
}

/// Terrain › Survey data opens the import window (export needs a point
/// cloud: disabled without one).
#[test]
fn the_survey_menu_opens_import_and_export() {
    use egui_kittest::kittest::NodeT;
    let model = synthetic_project("survey-menu");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label_contains("Export").hover();
    harness.run();
    assert!(
        harness
            .get_by_label("LiDAR points (LAS/LAZ)…")
            .accesskit_node()
            .is_disabled()
    );
    harness.get_by_label_contains("Import").hover();
    harness.run();
    harness.get_by_label("LiDAR points (LAS/LAZ)…").click();
    harness.run();
    harness.get_by_label("Choose files…");
    harness.get_by_label("Choose a folder…");
    assert!(
        harness
            .get_by_label("Import")
            .accesskit_node()
            .is_disabled(),
        "no file chosen yet"
    );
}

/// Sections are calculated as chosen in the Sections table (8 choices:
/// Initial, Creep, Load, Max Sag; RS or FE); Criteria › Creep-Stretch
/// names the weather cases for after creep and after load, as one command
/// (undone with ⌘Z), and every section is solved again.
#[test]
fn sections_are_calculated_after_creep_or_load_as_chosen() {
    use crate::tables::TableKind;
    use ohl_line::condition::{Calculation, CreepStretch};
    let model = synthetic_project("calculation");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let kind = TableKind::Sections;
    // The column's choices.
    {
        let r = harness.state_mut().ready().unwrap();
        let rows = kind.rows(&r.model, r.case);
        assert_eq!(rows[0].cells[kind.column("calculation")], "Initial FE");
    }
    set_cell(&mut harness, kind, "S1", "calculation", "Creep RS");
    apply_table(&mut harness, kind, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.model.line.sections[0].calculation, Calculation::CREEP_RS);
        // No creep weather case yet: S1 fails, saying where to choose one.
        let failed = &r.model.cases[0].failed;
        assert!(
            failed
                .iter()
                .any(|f| f.starts_with("S1") && f.contains("Creep-Stretch")),
            "{failed:?}"
        );
        // Criteria › Creep-Stretch: OK.
        r.creep_stretch_window = Some(CreepStretch {
            creep_case: Some("everyday".into()),
            load_cases: vec!["glaze ice".into(), "rime ice + wind".into()],
        });
    }
    harness.run();
    harness.get_by_label("OK").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(
            r.model.line.creep_stretch.creep_case.as_deref(),
            Some("everyday")
        );
        assert_eq!(r.model.line.condition_cases.loads.len(), 2);
        assert!(
            r.model.cases.iter().all(|c| c.failed.is_empty()),
            "solved again"
        );
        // A linear cable without creep data: after creep sags as initial.
        assert_solved_fully(&r.model);
    }
    press(&mut harness, egui::Key::Z, egui::Modifiers::COMMAND);
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.line.creep_stretch.is_empty(), "undone");
    assert!(
        !r.model.cases[0].failed.is_empty(),
        "S1 needs its case again"
    );
}

/// The wire library takes the cable models and, for bimetallic cables,
/// the curves of the outer and the core materials.
#[test]
fn cable_models_and_bimetallic_curves_are_read() {
    let text = r#"
format = "ohl-wire"
version = 1

[[cable]]
id = "bimetallic"
name = "Aluminium on steel"
kind = "acsr"
diameter = "28 mm"
mass_per_length = "1.6 kg/m"
rated_tensile_strength = "140 kN"
model = "nonlinear"

[cable.outer]
area = "400 mm²"
modulus = "55 GPa"
thermal_expansion = 23.0e-6
stress_strain = { stress_unit = "MPa", basis = "total", initial = [0.0, 250.0, -110.0, 40.0, -8.0], creep = [0.0, 160.0, -60.0, 15.0, -2.0], final_modulus = 550.0, max_compression = 0.0, reference_temperature = "20 °C" }

[cable.core]
area = "65 mm²"
modulus = "190 GPa"
thermal_expansion = 11.5e-6
stress_strain = { stress_unit = "MPa", basis = "total", initial = [0.0, 1800.0, -120.0, 30.0, -5.0], final_modulus = 1900.0, reference_temperature = "20 °C" }

[[cable]]
id = "linear-shift"
name = "Linear, creep as +15 °C"
kind = "aaac"
diameter = "30 mm"
mass_per_length = "1.5 kg/m"
rated_tensile_strength = "160 kN"
model = "linear_creep_temperature"
creep_temperature_shift = 15.0
outer = { area = "550 mm²", modulus = "55 GPa", thermal_expansion = 23e-6 }
"#;
    crate::library::LibraryKind::Cables
        .check(std::path::Path::new("x.ohlw"), text)
        .unwrap();
    let mut lib = ohl_cables::Library::default();
    lib.add_toml(std::path::Path::new("x.ohlw"), text).unwrap();
    let b = lib.get("bimetallic").unwrap();
    assert!(b.outer.stress_strain.is_some() && b.core.as_ref().unwrap().stress_strain.is_some());
    assert_eq!(
        lib.get("linear-shift").unwrap().model,
        ohl_cables::CableModel::LinearCreepTemperature
    );
    // A linear model without its shift is refused.
    let bad = text.replace("creep_temperature_shift = 15.0\n", "");
    assert!(
        crate::library::LibraryKind::Cables
            .check(std::path::Path::new("x.ohlw"), &bad)
            .is_err()
    );
}

/// The wire library page: the model is a list at the top (the fields it
/// needs appear, not removable), with the Bimetallic tick.
#[test]
fn the_cable_model_is_a_list_and_adds_its_fields() {
    use crate::library::LibraryKind;
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.state_mut().ready().unwrap().open_page =
        Some((crate::app::Tab::Library(LibraryKind::Cables), None));
    harness.run();
    harness.run();
    // The example's first cable: ACSR, a core.
    let bimetallic = "Bimetallic conductor (outer strands and a core)";
    harness.get_by_label(bimetallic);
    // The list shows the model chosen (nonlinear by default).
    harness
        .get_by_value(crate::library::CABLE_MODELS[0].1)
        .click();
    harness.run();
    harness
        .get_by_label(
            "Linear elastic, permanent stretch due to creep given as a temperature increase",
        )
        .click();
    harness.run();
    harness.get_by_label("creep_temperature_shift");
    let text = {
        let r = harness.state_mut().ready().unwrap();
        r.libraries[&LibraryKind::Cables].text().unwrap()
    };
    assert!(
        text.contains("model = \"linear_creep_temperature\""),
        "{text}"
    );
    assert!(text.contains("creep_temperature_shift"));
    // Only the temperature shift is asked for.
    assert!(harness.query_by_label("creep_shift_tension").is_none());
    // Bimetallic options, the conductor's own: bird cage hides the
    // largest compressive stress, compression asks for it.
    harness
        .get_by_label(crate::library::BIMETALLIC_BEHAVIORS[1].1)
        .click();
    harness.run();
    assert!(harness.query_by_label("max_virtual_compression").is_none());
    {
        let r = harness.state_mut().ready().unwrap();
        let text = r.libraries[&LibraryKind::Cables].text().unwrap();
        assert!(
            text.contains("bimetallic_behavior = \"bird_cage\""),
            "{text}"
        );
    }
    harness
        .get_by_label(crate::library::BIMETALLIC_BEHAVIORS[2].1)
        .click();
    harness.run();
    harness.get_by_label("max_virtual_compression");
    {
        let r = harness.state_mut().ready().unwrap();
        let text = r.libraries[&LibraryKind::Cables].text().unwrap();
        assert!(
            text.contains("bimetallic_behavior = \"compression\""),
            "{text}"
        );
        assert!(text.contains("max_virtual_compression = \"68947.4 MPa\""));
    }
    // The line's choice: nothing written.
    harness
        .get_by_label(crate::library::BIMETALLIC_BEHAVIORS[0].1)
        .click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let text = r.libraries[&LibraryKind::Cables].text().unwrap();
        assert!(!text.contains("bimetallic_behavior"), "{text}");
    }
    // Unticking Bimetallic removes the core (and its options).
    harness.get_by_label(bimetallic).click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    let page = &r.libraries[&LibraryKind::Cables];
    let text = page.text().unwrap();
    let first = text.split("[[cable]]").nth(1).unwrap();
    assert!(!first.contains("core"), "{first}");
    assert!(!first.contains("bimetallic_behavior"), "{first}");
    assert!(page.dirty());
}

/// Runs frames until `done` (at most 30 s: an exit clause).
fn run_until(h: &mut Harness<'_, App>, mut done: impl FnMut(&mut App) -> bool) {
    let started = std::time::Instant::now();
    while !done(h.state_mut()) {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(30),
            "not done in 30 s"
        );
        h.step();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Plugins › Plugin manager lists the example plugins with what they
/// provide; turned off, a format plugin's formats go; File › Export and
/// Import into the line run on the worker (the import one command,
/// undoable).
#[test]
fn plugins_are_managed_and_give_their_formats() {
    let model = synthetic_project("plugins");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    let user = std::env::temp_dir().join(format!("ohl-app-plugins-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&user);
    let examples = root().join("plugins/examples");
    let geojson_built = examples
        .join("geojson/target/wasm32-wasip2/release/geojson.wasm")
        .is_file();
    assert!(
        geojson_built || ohl_core::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
        "geojson plugin not built"
    );
    {
        let app = harness.state_mut();
        app.plugins = crate::plugins::Plugins::load(None, Some(user.clone()));
        app.plugins.folders = vec![examples.clone()];
        app.plugins.refresh();
        app.ready().unwrap().plugin_request = Some(crate::app::PluginRequest::Manager);
    }
    run_until(&mut harness, |a| a.plugins.formats.is_some());
    harness.step();
    harness.get_by_label("Plugin manager");
    harness.get_by_label("hello-python");
    let has_geojson = |a: &App| {
        a.plugins
            .formats
            .as_ref()
            .unwrap()
            .exporters
            .iter()
            .any(|e| e.id == "geojson" && e.origin == "plugin geojson")
    };
    assert_eq!(has_geojson(harness.state_mut()), geojson_built);
    if geojson_built {
        harness.get_by_label_contains("export geojson");
        // Turned off: its formats go.
        let name = "geojson";
        harness.get_by_label(name).click();
        harness.step();
        run_until(&mut harness, |a| {
            a.plugins.formats.is_some() && !has_geojson(a)
        });
        assert!(harness.state_mut().plugins.disabled.contains(name));
    }
    // Install the Python example for the user.
    {
        let p = &mut harness.state_mut().plugins;
        p.install(&examples.join("hello-python"), false).unwrap();
        assert!(
            p.listed
                .iter()
                .any(|l| l.installed && l.found.name() == "hello-python")
        );
        assert!(
            p.listed.iter().any(|l| l.duplicate),
            "the folder's copy comes second"
        );
        assert_eq!(p.python_scripts().len(), 1);
    }
    // Export the structures as CSV, move one in the file, import it back.
    let csv = user.join("structures.csv");
    let run = |h: &mut Harness<'_, App>, export: bool, format: &str| {
        let app = h.state_mut();
        let mut w = crate::plugins::FormatWindow {
            export,
            format: format.into(),
            ..Default::default()
        };
        let mut plugins = std::mem::take(&mut app.plugins);
        plugins.run(&mut w, &app.ready().unwrap().model, csv.clone());
        assert!(w.busy, "{:?}", w.error);
        plugins.window = Some(w);
        app.plugins = plugins;
    };
    run(&mut harness, true, "structures-csv");
    run_until(&mut harness, |a| a.plugins.window.is_none());
    let text = std::fs::read_to_string(&csv).unwrap();
    let first = text.lines().nth(1).unwrap().to_owned();
    let mut fields: Vec<String> = first.split(',').map(str::to_owned).collect();
    let id = fields[0].clone();
    let x: f64 = fields[1].parse().unwrap();
    fields[1] = format!("{}", x + 5.0);
    std::fs::write(&csv, text.replace(&first, &fields.join(","))).unwrap();
    run(&mut harness, false, "structures-csv");
    run_until(&mut harness, |a| a.plugins.window.is_none());
    harness.run();
    let position = |a: &mut App| {
        a.ready()
            .unwrap()
            .model
            .line
            .structures
            .iter()
            .find(|s| s.id == id)
            .unwrap()
            .position[0]
    };
    assert!((position(harness.state_mut()) - (x + 5.0)).abs() < 1e-6);
    press(&mut harness, egui::Key::Z, egui::Modifiers::COMMAND);
    assert!((position(harness.state_mut()) - x).abs() < 1e-6, "undone");
    std::fs::remove_dir_all(&user).unwrap();
}

/// Plugins › Python console: a script reads the line and results, moves a
/// structure with a session command (solved again, undoable), undoes it;
/// a refused command raises in the script.
#[cfg(feature = "python")]
#[test]
fn the_python_console_drives_the_project() {
    use crate::python::Line;
    let _turn = crate::python::TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let model = synthetic_project("python");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.get_by_label("Plugins").click();
    harness.run();
    harness.get_by_label("Python console").click();
    harness.run();
    let (id, x) = {
        let r = harness.state_mut().ready().unwrap();
        let s = &r.model.line.structures[1];
        (s.id.clone(), s.position)
    };
    let submit = |h: &mut Harness<'_, App>, code: String| {
        h.state_mut().ready().unwrap().console.submit(code);
        run_until(h, |a| a.ready().unwrap().console.running.is_none());
        h.state_mut().ready().unwrap().console.lines.last().cloned()
    };
    let out = submit(
        &mut harness,
        "len(ohl.line()['structures']), ohl.case(), len(ohl.results()['sections']) > 0".into(),
    );
    let n = harness
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .structures
        .len();
    let case = harness.state_mut().ready().unwrap().model.cases[0]
        .case
        .name
        .clone();
    assert_eq!(out, Some(Line::Out(format!("({n}, '{case}', True)\n"))));
    // Move a structure 3 m east.
    submit(
        &mut harness,
        format!(
            "p = [s for s in ohl.line()['structures'] if s['id'] == '{id}'][0]['position']\n\
             p[0] += 3.0\n\
             ohl.apply({{'op': 'move_structure', 'id': '{id}', 'position': p}}, 'moved by a script')"
        ),
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let s = r.model.line.structures.iter().find(|s| s.id == id).unwrap();
        assert!((s.position[0] - x[0] - 3.0).abs() < 1e-9);
        assert_solved_fully(&r.model);
    }
    submit(&mut harness, "ohl.undo()".into());
    {
        let r = harness.state_mut().ready().unwrap();
        let s = r.model.line.structures.iter().find(|s| s.id == id).unwrap();
        assert_eq!(s.position, x, "undone");
    }
    // Refused: raised in the script.
    let out = submit(
        &mut harness,
        "ohl.apply({'op': 'move_structure', 'id': 'no-such', 'position': [0, 0, 0]})".into(),
    );
    assert!(
        matches!(&out, Some(Line::Err(t)) if t.contains("no-such")),
        "{out:?}"
    );
    harness.get_by_label_contains("Python 3.");
}

/// Server mode: two applications join a hosted project (File › Join a
/// server); an edit in one reaches the other; a structure in a Move
/// window is held; an admin makes a viewer a designer; undo is per user.
#[test]
fn two_applications_share_a_hosted_project() {
    use crate::app::MoveDialog;
    let model = synthetic_project("shared");
    let file = model.sources.file.clone();
    drop(model);
    {
        let s = ohl_session::Session::open(&file).unwrap();
        s.set_user("tester", "bob", ohl_session::Role::Viewer)
            .unwrap();
    }
    let server = ohl_server::server::Server::start(
        &file,
        ohl_server::server::Options {
            bind: "127.0.0.1:0".parse().unwrap(),
            advertise: None,
            code: None,
            newcomers: None,
            web: None,
        },
    )
    .unwrap();
    let dir = std::env::temp_dir().join(format!("ohl-app-shared-{}", std::process::id()));
    let open = |user: &str| {
        let mut app = App::empty(Sources::default(), None);
        app.joined_dir = dir.join(user);
        app.join_window = Some(crate::remote::JoinWindow {
            invitation: server.invitation_for(user).to_string(),
            user: user.into(),
            ..Default::default()
        });
        let mut h = Harness::builder()
            .with_size(egui::vec2(1500.0, 950.0))
            .build_eframe(|_| app);
        h.step();
        h.get_by_label("Join").click();
        run_until(&mut h, |a| {
            a.ready()
                .is_some_and(|r| r.model.remote.is_some() && !r.model.cases.is_empty())
        });
        h
    };
    let mut alice = open("tester");
    let mut bob = open("bob");
    assert!(
        bob.state_mut().ready().unwrap().model.can_edit().is_err(),
        "a viewer"
    );
    let id = alice.state_mut().ready().unwrap().model.line.structures[1]
        .id
        .clone();
    let p0 = alice
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .structure(&id)
        .unwrap()
        .position;
    let at = |h: &mut Harness<'_, App>| {
        h.state_mut()
            .ready()
            .unwrap()
            .model
            .line
            .structure(&id)
            .unwrap()
            .position
    };

    // Alice moves it: Bob's line follows, re-solved.
    let p1 = [p0[0] + 4.0, p0[1], p0[2]];
    assert!(
        alice
            .state_mut()
            .ready()
            .unwrap()
            .perform(crate::edit::Action::Move {
                id: id.clone(),
                position: p1,
            })
    );
    assert_eq!(at(&mut alice), p1);
    run_until(&mut bob, |a| {
        a.ready()
            .unwrap()
            .model
            .line
            .structure(&id)
            .unwrap()
            .position
            == p1
    });
    assert_solved_fully(&bob.state_mut().ready().unwrap().model);
    // Both see each other.
    let peers = |h: &mut Harness<'_, App>| {
        h.state_mut()
            .ready()
            .unwrap()
            .model
            .remote
            .as_ref()
            .unwrap()
            .peers
            .len()
    };
    run_until(&mut alice, |a| {
        a.ready()
            .unwrap()
            .model
            .remote
            .as_ref()
            .unwrap()
            .peers
            .len()
            == 2
    });
    assert_eq!(peers(&mut bob), 2);

    // The admin makes Bob a designer: he can edit.
    alice.state_mut().ready().unwrap().project_op(
        "bob made designer",
        ohl_server::Op::SetUser {
            name: "bob".into(),
            role: ohl_session::Role::Designer,
        },
    );
    run_until(&mut bob, |a| a.ready().unwrap().model.can_edit().is_ok());

    // Alice's Move window holds the structure: Bob cannot take it.
    alice.state_mut().ready().unwrap().move_dialog = Some(MoveDialog {
        id: id.clone(),
        start: p1,
        position: p1,
        on_ground: false,
        shown: p1,
    });
    alice.step();
    let target = ohl_server::lock_target("main", ohl_session::Kind::Structure, &id);
    run_until(&mut bob, |a| {
        a.ready().unwrap().model.locked_by_other(&target).is_some()
    });
    {
        let r = bob.state_mut().ready().unwrap();
        r.hotbar.hold(crate::tools::Tool::Move);
        r.use_tool_on(&id);
        assert!(r.move_dialog.is_none());
        let status = r.status.clone().unwrap().0;
        assert!(status.contains("being moved by tester"), "{status}");
        // The server refuses his command too.
        assert!(!r.perform(crate::edit::Action::Move {
            id: id.clone(),
            position: p0,
        }));
    }
    alice.state_mut().ready().unwrap().move_dialog = None;
    alice.step();
    run_until(&mut bob, |a| {
        a.ready().unwrap().model.locked_by_other(&target).is_none()
    });

    // Bob's undo: nothing of his; Alice undoes her move.
    {
        let r = bob.state_mut().ready().unwrap();
        assert!(!r.perform(crate::edit::Action::Undo));
    }
    assert!(
        alice
            .state_mut()
            .ready()
            .unwrap()
            .perform(crate::edit::Action::Undo)
    );
    run_until(&mut bob, |a| {
        a.ready()
            .unwrap()
            .model
            .line
            .structure(&id)
            .unwrap()
            .position
            == p0
    });
    // People lists both, with the invitation.
    {
        let r = alice.state_mut().ready().unwrap();
        r.open_page = Some((crate::app::Tab::People, None));
    }
    alice.step();
    alice.step();
    alice.get_by_label("Copy");
    alice.get_by_label_contains("(you)");
    drop(alice);
    drop(bob);
    server.stop();
    let s = ohl_session::Session::open(&file).unwrap();
    assert_eq!(s.line().structure(&id).unwrap().position, p0);
    assert_eq!(s.role("bob").unwrap(), Some(ohl_session::Role::Designer));
    let _ = std::fs::remove_dir_all(&dir);
}

/// File › Host this project: the application hosts and joins its own
/// server (its edits go through it); Stop hosting opens the project file
/// again, with the edits.
#[test]
fn an_application_hosts_its_project() {
    let model = synthetic_project("hosting");
    let file = model.sources.file.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    {
        let app = harness.state_mut();
        app.joined_dir = file.parent().unwrap().join("joined");
        app.template.user = Some("tester".into());
        app.file_action = Some(crate::files::FileAction::Host);
    }
    harness.step();
    {
        let w = harness.state_mut().host_window.as_mut().unwrap();
        w.port = 0;
        w.bind = std::net::IpAddr::from([127, 0, 0, 1]);
    }
    harness.step();
    harness.get_by_label("Start hosting").click();
    run_until(&mut harness, |a| {
        // A failure to host or join says why.
        if let Some(e) = a.host_window.as_ref().and_then(|w| w.error.clone()) {
            panic!("hosting: {e}");
        }
        a.hosting.is_some()
            && a.ready()
                .is_some_and(|r| r.model.remote.is_some() && !r.model.cases.is_empty())
    });
    let (id, p0) = {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.model.can_edit().is_ok());
        assert!(r.model.remote.as_ref().unwrap().hosting);
        let s = &r.model.line.structures[0];
        (s.id.clone(), s.position)
    };
    let p1 = [p0[0], p0[1] + 3.0, p0[2]];
    assert!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .perform(crate::edit::Action::Move {
                id: id.clone(),
                position: p1,
            })
    );
    // Stop hosting: the project file itself again, moved.
    harness.state_mut().file_action = Some(crate::files::FileAction::Leave);
    run_until(&mut harness, |a| {
        a.hosting.is_none()
            && a.ready()
                .is_some_and(|r| r.model.remote.is_none() && r.model.sources.file == file)
    });
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.model.line.structure(&id).unwrap().position, p1);
    assert!(r.model.can_edit().is_ok());
}

/// Checks › Structure loads: computed in the background from the
/// project's load cases (kept in the project), shown per load case; a
/// structure's link goes to it.
#[test]
fn structure_loads_are_computed_and_shown() {
    let model = synthetic_project("loads");
    assert!(
        model
            .sources
            .load_cases
            .as_ref()
            .is_some_and(|p| p.ancestors().any(|a| a.ends_with("ohl-design-libraries"))),
        "kept in the project: {:?}",
        model.sources.load_cases
    );
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.get_by_label("Checks").click();
    harness.run();
    harness.get_by_label("Compute structure loads").click();
    run_until(&mut harness, |a| {
        let r = a.ready().unwrap();
        r.loads.poll();
        r.loads.results.is_some()
    });
    harness.step();
    harness.step();
    {
        let r = harness.state_mut().ready().unwrap();
        let results = r.loads.results.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(results.len(), 3);
        assert!(results[0].failed.is_empty());
        assert!(!results[0].loads.is_empty());
    }
    harness.get_by_label("transverse kN");
    harness.get_all_by_label("T2").next().unwrap().click();
    harness.step();
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.selection, Selection::Structure("T2".into()));
        // The usage came with the loads: this line has no structure
        // types, so every structure is listed, not checked.
        assert_eq!(r.loads.usage.len(), 3);
        assert!(r.loads.usage[0].structures.iter().all(|s| {
            s.governing().is_none() && s.notes.iter().any(|n| n.contains("no structure type"))
        }));
    }
    harness.get_by_label("Usage").click();
    harness.step();
    harness.step();
    harness.get_by_label("weight span m");
    assert!(
        harness
            .query_all_by_label_contains("0 structures checked")
            .next()
            .is_some()
    );
}

/// Only the weather case shown is solved when the model loads; another is
/// solved in the background once shown, from the line as it is then;
/// cases never shown stay unsolved.
#[test]
fn weather_cases_are_solved_when_shown() {
    let ex = root().join("examples/synthetic-line");
    let model = model::load(
        Sources {
            file: ex.join("synthetic-line.ohl.json"),
            library: Some(root().join("libraries/cables")),
            weather: Some(ex.join("weather-cases.ohlwc")),
            point_budget: 1000,
            corridor: 60.0,
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    assert!(model.cases[0].ready);
    assert!(model.cases.len() > 2);
    assert!(
        model.cases[1..]
            .iter()
            .all(|c| !c.ready && c.shapes.is_empty())
    );
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.step();
    // Show the third case (Display weather cases): solved in the
    // background.
    harness.state_mut().ready().unwrap().model.shown_cases = vec![2];
    run_until(&mut harness, |a| a.ready().unwrap().model.cases[2].ready);
    let m = &harness.state_mut().ready().unwrap().model;
    assert!(!m.cases[1].ready, "not shown: not solved");
    assert_solved_fully(m);
    assert!(!m.cases[2].shapes.is_empty());
}

/// Terrain › Terrain surfaces (TIN): a survey file imported becomes the
/// ground (none before); another surface (LandXML) is shown in its colour;
/// those choices are the user's (remembered locally, not in the project):
/// opened again with them, the surface chosen is the ground; rename and
/// delete.
#[test]
fn terrain_surfaces_are_kept_in_the_project() {
    use crate::surfaces::Action;
    let model = synthetic_project("surfaces");
    let file = model.sources.file.clone();
    assert!(model.ground.is_none());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.step();
    let act = |h: &mut Harness<'_, App>, a: Action| {
        let ctx = egui::Context::default();
        let app = h.state_mut();
        let mut plugins = std::mem::take(&mut app.plugins);
        let r = app.ready().unwrap();
        let project = r.model.sources.file.clone();
        r.surfaces
            .act(a, &mut r.model, &mut plugins, "tester", &project, &ctx);
        app.plugins = plugins;
    };
    {
        let r = harness.state_mut().ready().unwrap();
        r.surfaces.show(&r.model);
    }
    // Import the survey points: triangulated, kept, made the ground.
    // (Nothing waiting to be drawn: the import must ask for the ground.)
    harness.state_mut().ready().unwrap().view3d.dirty.clear();
    let survey = root().join("examples/synthetic-line/synthetic-survey.csv");
    act(&mut harness, Action::Import(survey));
    run_until(&mut harness, |a| {
        a.ready().unwrap().model.ground_source == "surface synthetic-survey"
    });
    harness.get_by_label("Terrain surfaces (TIN)");
    {
        // Drawn at once (no reopening), shown.
        let r = harness.state_mut().ready().unwrap();
        assert!(r.view3d.dirty.contains(&crate::scene::Layer::Ground));
        assert!(r.visible.contains(&crate::scene::Layer::Ground));
    }
    let n = harness
        .state_mut()
        .ready()
        .unwrap()
        .model
        .ground
        .as_ref()
        .unwrap()
        .num_triangles();
    assert!(n > 100);
    // A second surface (LandXML of the first, raised 1 m), shown.
    let dir = file.parent().unwrap().to_owned();
    let xml = dir.join("raised.xml");
    {
        let g = harness
            .state_mut()
            .ready()
            .unwrap()
            .model
            .ground
            .clone()
            .unwrap();
        let raised = ohl_terrain::Terrain::from_mesh(
            g.points()
                .iter()
                .map(|p| [p[0], p[1], p[2] + 1.0])
                .collect(),
            g.triangles().to_vec(),
        )
        .unwrap();
        std::fs::write(
            &xml,
            ohl_terrain::formats::write_landxml(&[("raised", &raised)]),
        )
        .unwrap();
    }
    act(&mut harness, Action::Import(xml));
    run_until(&mut harness, |a| {
        a.ready().unwrap().surfaces.list.len() == 2
    });
    act(&mut harness, Action::Show("raised".into(), true));
    run_until(&mut harness, |a| {
        a.ready().unwrap().model.other_surfaces.len() == 1
    });
    // The choice is this user's, not the project's.
    let prefs = harness.state_mut().ready().unwrap().model.view_prefs();
    {
        let s = ohl_session::Session::open(&file).unwrap();
        assert_eq!(s.setting(crate::surfaces::SETTING).unwrap(), None);
    }
    // Opened again with this user's choice: the surface chosen is the
    // ground (no LiDAR, no survey).
    let again = model::load(
        Sources {
            file: file.clone(),
            library: Some(root().join("libraries/cables")),
            point_budget: 1000,
            corridor: 60.0,
            user: Some("tester".into()),
            view: Some(prefs),
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    assert_eq!(again.ground_source, "surface synthetic-survey");
    assert_eq!(again.ground.as_ref().unwrap().num_triangles(), n);
    assert_eq!(again.other_surfaces.len(), 1);
    assert_eq!(again.other_surfaces[0].name, "raised");
    drop(again);
    // The checks use "raised" for everyone (a project setting), while
    // this user's ground stays the survey.
    act(&mut harness, Action::Check(Some("raised".into())));
    run_until(&mut harness, |a| {
        a.ready().unwrap().model.check_ground.is_some()
    });
    {
        let r = harness.state_mut().ready().unwrap();
        let (check, shown) = (
            r.model.ground_for_checks().unwrap(),
            r.model.ground.clone().unwrap(),
        );
        let (x, y) = (shown.points()[0][0], shown.points()[0][1]);
        assert!((check.height(x, y).unwrap() - shown.height(x, y).unwrap() - 1.0).abs() < 1e-9);
        let s = ohl_session::Session::open(&file).unwrap();
        assert_eq!(
            s.setting(crate::surfaces::CHECK_SETTING)
                .unwrap()
                .as_deref(),
            Some("raised")
        );
    }
    // Rename the ground; delete the other.
    act(
        &mut harness,
        Action::Rename("synthetic-survey".into(), "survey".into()),
    );
    act(&mut harness, Action::Delete("raised".into()));
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.model.ground_source, "surface survey");
    assert!(r.model.other_surfaces.is_empty());
    let names: Vec<&str> = r.surfaces.list.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["survey"]);
    assert_eq!(r.model.surface_settings.active.as_deref(), Some("survey"));
}

/// File › New project from another program (no file open): a project
/// plugin's format read on the worker into a new project, opened, its
/// notes shown.
#[test]
fn a_project_is_created_from_another_programs_file() {
    let dir = root().join("plugins/examples/project-bundle");
    let built = dir
        .join("target/wasm32-wasip2/release/project_bundle.wasm")
        .exists();
    assert!(
        built || ohl_core::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
        "project-bundle plugin not built"
    );
    if !built {
        return;
    }
    let tmp = std::env::temp_dir().join(format!("ohl-app-import-project-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let ex = root().join("examples/synthetic-line");
    let bundle = serde_json::json!({
        "line": serde_json::from_str::<serde_json::Value>(
            &std::fs::read_to_string(ex.join("synthetic-line.ohl.json")).unwrap()).unwrap(),
        "libraries": [
            {"kind": "cables", "name": "acsr-examples.ohlw",
             "text": std::fs::read_to_string(root().join("libraries/cables/acsr-examples.ohlw")).unwrap()},
            {"kind": "weather", "name": "weather-cases.ohlwc",
             "text": std::fs::read_to_string(ex.join("weather-cases.ohlwc")).unwrap()},
        ],
    });
    let file = tmp.join("line.ohlb");
    std::fs::write(&file, bundle.to_string()).unwrap();
    let mut app = App::empty(Sources::default(), None);
    app.plugins = crate::plugins::Plugins::load(None, None);
    app.plugins.folders = vec![dir];
    app.plugins.refresh();
    app.file_action = Some(crate::files::FileAction::ImportProject);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| app);
    h.step();
    h.step();
    h.get_by_label("New project from another program");
    let into = tmp.join("imported.ohl");
    h.state_mut()
        .plugins
        .create_project(crate::plugins::ProjectImport {
            path: file,
            format: None,
            options: [("name".to_owned(), "Imported".to_owned())].into(),
            into: into.clone(),
            user: crate::files::user(),
            clip_in: true,
        });
    run_until(&mut h, |a| {
        a.ready()
            .is_some_and(|r| r.model.line.name == "Imported" && !r.model.cases.is_empty())
    });
    h.step();
    h.get_by_label("Project created");
    let app = h.state_mut();
    let notes = &app.import_notes.as_ref().unwrap().1;
    assert!(notes.iter().any(|n| n.contains("clipped in")), "{notes:?}");
    let r = app.ready().unwrap();
    assert_eq!(r.model.sources.file, into);
    assert!(r.model.line.sections.iter().all(|s| s.stringing.clipped()));
    // Its libraries are the project's: five weather cases.
    assert_eq!(r.model.cases.len(), 5);
    let _ = std::fs::remove_dir_all(&tmp);
}

/// Terrain › Survey data › Class colours: a class's colour picked, the
/// scheme saved to a file and used for the project (this user's choice);
/// back to the cloud's own.
#[test]
fn class_colours_come_from_a_scheme_file() {
    use crate::class_colors::{ClassAction, set_color};
    let model = synthetic_project("class-colours");
    let dir = model.sources.file.parent().unwrap().to_owned();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // (Terrain › Survey data › Class colours…, with a point cloud.)
    harness.state_mut().ready().unwrap().class_window = Some(Default::default());
    harness.run();
    harness.get_by_label_contains("Scheme: ");
    let file = dir.join("mine.ohlc");
    {
        let r = harness.state_mut().ready().unwrap();
        set_color(&mut r.model.scheme, 2, Some("#123456".into()));
        set_color(&mut r.model.scheme, 77, Some("#abcdef".into()));
        assert_eq!(
            crate::scene::class_color(&r.model.scheme, 2),
            Some([0x12, 0x34, 0x56])
        );
        std::fs::write(&file, r.model.scheme.to_toml()).unwrap();
        r.model.use_class_scheme(Some(file.clone())).unwrap();
        assert_eq!(
            r.model.view_prefs().class_scheme.as_deref(),
            Some(file.as_path())
        );
    }
    // The file holds the colours and the class added.
    let s = ohl_lidar::ClassScheme::load(&file).unwrap();
    assert_eq!(crate::scene::class_color(&s, 77), Some([0xab, 0xcd, 0xef]));
    assert_eq!(s.class(77).unwrap().name, "Class 77");
    // Back to the cloud's own: no file; remembered as such (an empty path).
    let r = harness.state_mut().ready().unwrap();
    r.model.use_class_scheme(None).unwrap();
    assert!(r.model.scheme.class(77).is_none());
    assert_eq!(
        r.model.view_prefs().class_scheme,
        Some(std::path::PathBuf::new())
    );
    let _ = ClassAction::Recolor;
}

/// Section display options: nothing by default; a label per section and a
/// sag per span in the views ticked only; kept in the view preferences.
#[test]
fn section_labels_and_sags_per_view() {
    use crate::annotations::{Kind, View, notes};
    let mut m = synthetic();
    for v in View::ALL {
        assert!(notes(&m, 0, v, None).is_empty());
    }
    m.section_display.label[View::Plan as usize] = true;
    m.section_display.sag[View::ThreeD as usize] = true;
    let plan = notes(&m, 0, View::Plan, None);
    assert_eq!(plan.len(), m.line.sections.len());
    assert!(plan.iter().all(|n| n.kind == Kind::Label));
    assert!(notes(&m, 0, View::Profile, None).is_empty());
    let sags = notes(&m, 0, View::ThreeD, None);
    let spans: usize = m.cases[0].shapes.iter().map(|s| s.spans.len()).sum();
    assert_eq!(sags.len(), spans);
    let first = &m.cases[0].shapes[0].spans[0];
    assert_eq!(sags[0].text, format!("{:.2} m", first.sag));
    // Remembered with this user's view of the project.
    let prefs = m.view_prefs();
    assert_eq!(prefs.section_display, m.section_display);
    let back: crate::model::ViewPrefs =
        serde_json::from_str(&serde_json::to_string(&prefs).unwrap()).unwrap();
    assert!(back.section_display.sag[View::ThreeD as usize]);
}

/// Menus of display options stay open while boxes are ticked: several are
/// ticked in a row.
#[test]
fn display_options_stay_open_while_ticking() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.get_by_label("Sections").click();
    harness.run();
    harness.get_by_label_contains("Display options").hover();
    harness.run();
    harness.get_by_label("Sag in Plan").click();
    harness.run();
    // Still open: the next box can be ticked.
    harness.get_by_label("Section label in 3D").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.section_display.sag[crate::annotations::View::Plan as usize]);
    assert!(r.model.section_display.label[crate::annotations::View::ThreeD as usize]);
}

/// Checks › Thermal rating: the ambient cases next to the model, an
/// ampacity per section and case; the clearance-limited temperature in the
/// background with the rule file next to the model.
#[test]
fn thermal_rating_window() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.get_by_label("Checks").click();
    harness.run();
    harness.get_by_label("Thermal rating…").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.thermal.open);
        assert!(matches!(&r.thermal.ambient, Some(Ok(set)) if set.cases.len() == 3));
    }
    harness.get_by_label("summer noon at 80 °C");
    harness.get_by_label("Run").click();
    harness.step();
    run_until(&mut harness, |a| {
        let r = a.ready().unwrap();
        r.thermal.poll();
        !r.thermal.running() && r.thermal.limits.is_some()
    });
    harness.step();
    harness.step();
    let r = harness.state_mut().ready().unwrap();
    let limits = r.thermal.limits.as_ref().unwrap().as_ref().unwrap();
    assert_eq!(limits.len(), r.model.line.sections.len());
    // S2 over the hill misses the 7 m ground rule even when cold.
    let s2 = limits.iter().find(|l| l.section == "S2").unwrap();
    assert_eq!(s2.result, Ok(ohl_thermal::limit::Limit::BelowRange));
    harness.get_by_label("clearance limit");
}

/// Checks › Bill of materials prices the typed example with the cost file
/// next to it; Tools › Spot structures proposes structures along the main
/// route on the survey, in the background.
#[test]
fn bill_of_materials_and_spotting_windows() {
    let ex = root().join("examples/synthetic-line");
    let model = model::load(
        Sources {
            file: ex.join("synthetic-line-typed.ohl.json"),
            library: Some(root().join("libraries/cables")),
            structures: Some(ex.join("Structures")),
            terrain: Some(ex.join("synthetic-survey.csv")),
            point_budget: 1000,
            corridor: 60.0,
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.get_by_label("File").click();
    harness.run();
    harness.get_by_label_contains("Bill of materials").hover();
    harness.run();
    harness.get_by_label("Wires…").click();
    run_until(&mut harness, |a| {
        let r = a.ready().unwrap();
        r.bom.result.is_some() && !r.bom.running()
    });
    harness.step();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(
            r.bom
                .cost_path
                .as_ref()
                .is_some_and(|p| p.ends_with("unit-costs.ohlcost"))
        );
        let e = r.bom.result.as_ref().unwrap().as_ref().unwrap();
        assert!(e.priced && e.missing.is_empty(), "{:?}", e.missing);
        assert_eq!(e.bom.structures.iter().map(|s| s.count).sum::<usize>(), 7);
        // Only the cables are offered yet.
        assert!(!e.lines.is_empty());
        assert!(
            e.lines
                .iter()
                .all(|l| crate::bom::CABLES.contains(&l.category.as_str()))
        );
    }
    assert!(
        harness
            .query_all_by_label_contains("Total: ")
            .next()
            .is_some()
    );

    // The menu (the first "Structures"; the bill's window says it too).
    harness
        .query_all_by_label("Structures")
        .next()
        .unwrap()
        .click();
    harness.run();
    harness.get_by_label("Spot structures…").click();
    harness.run();
    // The bill's window is open too: the spotting window's Run.
    run_in(&harness, "Spot structures").click_accesskit();
    run_until(&mut harness, |a| {
        let r = a.ready().unwrap();
        r.spot.result.is_some() && !r.spot.running()
    });
    harness.step();
    let r = harness.state_mut().ready().unwrap();
    let (_, s, line) = r.spot.result.as_ref().unwrap().as_ref().unwrap();
    assert!(s.structures.len() >= 3);
    assert_eq!(line.structures.len(), s.structures.len());
    assert!(s.spans.iter().all(|x| x.clearance >= 7.0 - 1e-9));
}

/// Criteria › Design code: a design-code plugin's cases written into the
/// project's libraries, kept in the project and used for the model.
#[test]
fn design_code_cases_go_into_the_project() {
    let dir = root().join("plugins/codes/en50341");
    let built = dir
        .join("target/wasm32-wasip2/release/en50341.wasm")
        .exists();
    assert!(
        built || ohl_core::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
        "en50341 plugin not built"
    );
    if !built {
        return;
    }
    let mut r = crate::app::Ready::new(synthetic_project("design-code"));
    let mut plugins = crate::plugins::Plugins::load(None, None);
    plugins.folders = vec![root().join("plugins/codes")];
    plugins.refresh();
    let choices = crate::codes::choices(&plugins);
    assert!(
        choices
            .iter()
            .any(|c| c.name == "en50341" && c.problem.is_none()),
        "{choices:?}"
    );
    let host = ohl_plugins::WasmHost::new().unwrap();
    let params = std::fs::read_to_string(dir.join("example-parameters.toml")).unwrap();
    let g = ohl_io::codes::generate(&host, &dir, &params).unwrap();
    let report = crate::codes::apply(&mut r, &g, "en50341").unwrap();
    assert!(
        report.contains("6 weather cases and 7 load cases"),
        "{report}"
    );
    let names: Vec<&str> = r.model.cases.iter().map(|c| c.case.name.as_str()).collect();
    assert_eq!(names[0], "EN extreme wind");
    assert_eq!(names.len(), 6);
    let lc = r.model.sources.load_cases.clone().unwrap();
    assert!(lc.ends_with("loadcases/en50341.ohllc"), "{}", lc.display());
    let session = &r.model.editing.as_ref().unwrap().session;
    for (kind, name) in [("weather", "en50341.ohlwc"), ("loadcases", "en50341.ohllc")] {
        let files = session.library_files(kind).unwrap();
        assert!(files.iter().any(|(n, _)| n == name), "{kind}: {name}");
    }
}

/// A design code's clearance presets: written into the project's rule
/// and weather case files when it generates; the Vertical clearances
/// window lists the vertical one, Load preset takes its rules (with their
/// own half widths), its circuits, method and search distance, and adds
/// its weather cases missing from the model to the model's weather cases.
#[test]
fn design_code_clearance_presets_load_into_the_window() {
    use ohl_plugins::{ClearanceMeasure, ClearancePreset};
    let mut r = crate::app::Ready::new(synthetic_project("clearance-preset"));
    let w = "format = \"ohl-weather-cases\"\nversion = 1\n[[case]]\nname = \"a\"\ntemperature = \"5 °C\"\n";
    let l = "format = \"ohl-load-cases\"\nversion = 1\n[[case]]\nname = \"x\"\nweather = \"a\"\n";
    let hot = "format = \"ohl-weather-cases\"\nversion = 1\nname = \"p\"\n\
               [[case]]\nname = \"Preset 80 °C\"\ntemperature = \"80 °C\"\n";
    let rules = "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"Code vertical\"\n\
                 [wind]\nsteps = false\nsides = \"both\"\n\
                 [check]\nmethod = \"fe\"\nsearch_distance = \"30 m\"\n\
                 [[rule]]\nname = \"roads\"\ntarget = \"points\"\nclasses = [11]\n\
                 vertical = \"7.5 m\"\nvertical_band = \"1 m\"\nvoltages = [220.0]\ncases = [\"Preset 80 °C\"]\n\
                 [[rule]]\nname = \"water\"\ntarget = \"points\"\nclasses = [9]\n\
                 vertical = \"6 m\"\nvertical_band = \"3 m\"\nvoltages = [220.0]\ncases = [\"Preset 80 °C\"]\n";
    let g = ohl_io::codes::Generated::check(
        "code",
        w.into(),
        l.into(),
        vec![],
        vec![ClearancePreset {
            name: "vertical".into(),
            measure: ClearanceMeasure::Vertical,
            rules: rules.into(),
            weather_cases: hot.into(),
        }],
    )
    .unwrap();
    // Written next to the generated cases' files in the project.
    let work = r.model.library_dir.clone().unwrap();
    let report = crate::codes::apply(&mut r, &g, "code").unwrap();
    assert!(report.contains("code-vertical.ohlr"), "{report}");
    assert!(work.join("rules/code-vertical.ohlr").is_file());
    assert!(work.join("weather/code-vertical.ohlwc").is_file());
    // The model's cases are the generated ones ("a"): the preset's is
    // missing.
    assert!(!r.model.cases.iter().any(|c| c.case.name == "Preset 80 °C"));
    let mut win = crate::clearance_zones::ZoneWindow::new(ohl_clearance::Measure::Vertical);
    win.show(&r.model);
    let presets = win.presets(&r.model);
    assert_eq!(presets.len(), 1, "{presets:?}");
    assert_eq!(presets[0].0, "Code vertical");
    // The radial window has none.
    let radial = crate::clearance_zones::ZoneWindow::new(ohl_clearance::Measure::Radial);
    assert!(radial.presets(&r.model).is_empty());
    let mut out = crate::clearance_zones::Out::default();
    win.load_preset(&r.model, &presets[0].1, &presets[0].2, &mut out);
    assert_eq!(win.rules.len(), 2);
    // The first rule's half width is the window's, the other keeps its own.
    assert_eq!(win.zone.width, 1.0);
    assert_eq!((win.rules[0].width, win.rules[1].width), (None, Some(3.0)));
    assert_eq!(win.zone.search, Some(30.0));
    assert_eq!(win.check.method, Some(ohl_line::condition::Method::Fe));
    assert!(out.weather_saved.is_some());
    crate::app::clearance_zone_out(&mut r, win.zone.measure, out);
    assert!(
        r.model.cases.iter().any(|c| c.case.name == "Preset 80 °C"),
        "{:?}",
        r.status
    );
    assert!(r.model.cases.iter().any(|c| c.case.name == "a"));
    // Kept: the window's rule file says it again.
    let text = win.rule_set().to_toml();
    assert!(text.contains("search_distance = \"30 m\""), "{text}");
    assert!(text.contains("vertical_band = \"3 m\""), "{text}");
}

/// A project preset of the Vertical clearances window whose cases the
/// project has under other names (owner, 2026-10-21): "Utility 80 °C" is
/// the project's "max temperature" (80 °C, still air): no case created,
/// the rule names "max temperature"; its "everyday" (20 °C) is not the
/// project's "everyday" (10 °C): created as "everyday (preset)", the rule
/// naming it so.
#[test]
fn a_clearance_preset_uses_the_projects_cases_of_the_same_weather() {
    let mut r = crate::app::Ready::new(synthetic_project("clearance-preset-same"));
    let work = r.model.library_dir.clone().unwrap();
    std::fs::create_dir_all(work.join("rules")).unwrap();
    std::fs::create_dir_all(work.join("weather")).unwrap();
    let rules = "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"Code vertical\"\n\
                 [[rule]]\nname = \"roads\"\ntarget = \"points\"\nclasses = [11]\n\
                 vertical = \"7.5 m\"\ncases = [\"Utility 80 °C\", \"everyday\"]\n";
    let cases = "format = \"ohl-weather-cases\"\nversion = 1\n\
                 [[case]]\nname = \"Utility 80 °C\"\ntemperature = \"80 °C\"\n\
                 [[case]]\nname = \"everyday\"\ntemperature = \"20 °C\"\n";
    std::fs::write(work.join("rules/code-same.ohlr"), rules).unwrap();
    std::fs::write(work.join("weather/code-same.ohlwc"), cases).unwrap();
    let before: Vec<String> = r.model.cases.iter().map(|c| c.case.name.clone()).collect();
    let mut win = crate::clearance_zones::ZoneWindow::new(ohl_clearance::Measure::Vertical);
    win.show(&r.model);
    let presets = win.presets(&r.model);
    let p = presets.iter().find(|p| p.0 == "Code vertical").unwrap();
    let mut out = crate::clearance_zones::Out::default();
    win.load_preset(&r.model, &p.1, &p.2, &mut out);
    assert_eq!(win.rules[0].cases, ["max temperature", "everyday (preset)"]);
    crate::app::clearance_zone_out(&mut r, win.zone.measure, out);
    let after: Vec<String> = r.model.cases.iter().map(|c| c.case.name.clone()).collect();
    assert_eq!(after.len(), before.len() + 1, "{after:?}");
    assert!(!after.iter().any(|n| n == "Utility 80 °C"), "{after:?}");
    let everyday = |n: &str| {
        r.model
            .cases
            .iter()
            .find(|c| c.case.name == n)
            .unwrap()
            .case
            .temperature
            .celsius()
    };
    assert_eq!(everyday("everyday"), 10.0);
    assert_eq!(everyday("everyday (preset)"), 20.0);
}

/// One big combined preset (40 rules of both measures, a `[vegetation]`
/// table; owner, 2026-10-21: presets are per check) in the Vertical
/// clearances window, as a project preset and as a plugin preset: only
/// its 24 vertical rules are taken, and only their cases reach the
/// project — "V hot" (80 °C) uses the project's "max temperature", "V
/// ice" and "B cold" are created; no radial or vegetation case.
#[test]
fn a_big_preset_brings_only_the_vertical_part_to_the_vertical_window() {
    let p = crate::preset_cases::tests::big_preset("");
    let mut r = crate::app::Ready::new(synthetic_project("big-preset-vertical"));
    let work = r.model.library_dir.clone().unwrap();
    std::fs::create_dir_all(work.join("rules")).unwrap();
    std::fs::create_dir_all(work.join("weather")).unwrap();
    std::fs::write(work.join("rules/code-big.ohlr"), &p.rules_text).unwrap();
    std::fs::write(work.join("weather/code-big.ohlwc"), &p.weather_text).unwrap();
    let before: Vec<String> = r.model.cases.iter().map(|c| c.case.name.clone()).collect();
    let mut win = crate::clearance_zones::ZoneWindow::new(ohl_clearance::Measure::Vertical);
    win.show(&r.model);
    let presets = win.presets(&r.model);
    let file = presets.iter().find(|x| x.0 == "big").unwrap();
    let mut out = crate::clearance_zones::Out::default();
    win.load_preset(&r.model, &file.1, &file.2, &mut out);
    assert_eq!(win.rules.len(), 24);
    assert!(
        win.rules
            .iter()
            .all(|x| x.cases.iter().all(|c| c != "V hot" && !c.starts_with('R'))),
        "{:?}",
        win.rules.iter().map(|x| &x.cases).collect::<Vec<_>>()
    );
    assert!(
        win.rules
            .iter()
            .any(|x| x.cases.contains(&"max temperature".to_string()))
    );
    crate::app::clearance_zone_out(&mut r, win.zone.measure, out);
    let after: Vec<String> = r.model.cases.iter().map(|c| c.case.name.clone()).collect();
    let added: Vec<&String> = after.iter().filter(|n| !before.contains(n)).collect();
    assert_eq!(added, ["V ice", "B cold"], "{:?}", r.status);
    // The plugin preset path takes the same part.
    let (adapted, plan) = crate::preset_cases::for_project(
        &r.model,
        &p,
        crate::preset_cases::Part::of(ohl_clearance::Measure::Vertical),
    )
    .unwrap();
    assert_eq!(adapted.rules.rules.len(), 24);
    assert_eq!(plan.names(), ["max temperature", "V ice", "B cold"]);
    // All three are the project's now: nothing more to create.
    assert!(adapted.weather.as_ref().unwrap().cases.is_empty());
}

/// Criteria › Design code… opens the window; without plugins it says how
/// to get one.
#[test]
fn the_design_code_window_opens_from_the_criteria_menu() {
    let model = synthetic_project("design-code-window");
    let mut app = App::with_model(model, None);
    app.plugins = crate::plugins::Plugins::load(None, None);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| app);
    harness.run();
    harness.get_by_label("Criteria").click();
    harness.run();
    harness.get_by_label("Design code…").click();
    harness.run();
    assert!(harness.state_mut().ready().unwrap().design_code.open);
    harness.get_by_label_contains("No design-code plugin");
    harness.get_by_label("Generate");
}

/// File › Export drawings: the main route's profile from 0+100 to 0+900
/// as PDF and DXF on A3, every solved case.
#[test]
fn drawings_export_in_the_background_job() {
    let m = synthetic();
    let dir = std::env::temp_dir().join(format!("ohl-app-drawings-{}", std::process::id()));
    let w = crate::drawings::DrawingsWindow {
        route: Some(0),
        formats: vec![
            ohl_drawing::export::Format::Pdf,
            ohl_drawing::export::Format::Dxf,
        ],
        range: true,
        from: 100.0,
        to: 900.0,
        profile_only: true,
        all_cases: true,
        out: Some(dir.clone()),
        ..Default::default()
    };
    let input = crate::drawings::Input::new(&m, 0, &w).unwrap();
    assert_eq!(input.cases.len(), 5);
    let report = crate::drawings::run(&input, &|_, _| {}).unwrap();
    // On A3 (772 m per sheet) split at T3: two DXF sheets, the profile in
    // model space, one PDF of two pages.
    assert!(report.starts_with("4 files"), "{report}");
    let pdf = std::fs::read(dir.join("main-profile.pdf")).unwrap();
    assert_eq!(ohl_drawing::pdf::read_back(&pdf).unwrap().len(), 2);
    assert!(dir.join("main-profile-model.dxf").is_file());
    let _ = std::fs::remove_dir_all(&dir);
    // Without a folder it does not start.
    let none = crate::drawings::DrawingsWindow::default();
    assert!(crate::drawings::Input::new(&m, 0, &none).is_err());
}

/// Regression (D5, D6): the export window draws each other weather case
/// from its own solution (not the case shown again), the markers of the
/// last clearance check, side profiles, the plan's own scale and logos,
/// and the clearance line of the project's ground rule.
#[test]
fn drawings_draw_every_case_from_its_own_solution() {
    let m = synthetic();
    let logo = std::env::temp_dir().join(format!("ohl-app-logo-{}.svg", std::process::id()));
    std::fs::write(
        &logo,
        "<svg xmlns='http://www.w3.org/2000/svg' width='10' height='5'/>",
    )
    .unwrap();
    let rules = ohl_clearance::RuleSet::parse(
        "format = \"ohl-clearance-rules\"\nversion = 1\n[[rule]]\nname = \"Ground\"\ntarget = \"ground\"\nvertical = \"6.5 m\"",
    )
    .unwrap()
    .rules;
    let w = crate::drawings::DrawingsWindow {
        route: Some(0),
        all_cases: true,
        out: Some(std::env::temp_dir()),
        side_offsets: vec![-10.0, 10.0],
        plan_scale: Some(1000.0),
        logos: vec![(logo.clone(), 20.0)],
        ground_rules: std::sync::Arc::new(rules),
        markers: std::sync::Arc::new(vec![([150.0, 0.0, 500.0], "test".into())]),
        ..Default::default()
    };
    let input = crate::drawings::Input::new(&m, 0, &w).unwrap();
    assert_eq!(input.options.clearance, Some(6.5));
    assert_eq!(input.options.side_offsets, [-10.0, 10.0]);
    assert_eq!(input.options.plan_offset_scale, Some(1000.0));
    assert_eq!(input.options.logos.len(), 1);
    let route = input.routes[0].clone();
    let d = input.drawing(&route).unwrap();
    assert_eq!(d.sides.len(), 2);
    assert_eq!(d.markers.len(), 1);
    // The first span of S1 in each case: the dashed ones are the other
    // cases' (their sag differs from the case shown).
    let lowest =
        |t: &ohl_drawing::Trace| t.points.iter().map(|p| p[2]).fold(f64::INFINITY, f64::min);
    let spans: Vec<&ohl_drawing::Trace> = d
        .wires
        .iter()
        .filter(|t| t.label.starts_with("S1 span 0 "))
        .collect();
    let solid: Vec<f64> = spans
        .iter()
        .filter(|t| t.dash.is_none())
        .map(|t| lowest(t))
        .collect();
    let dashed: Vec<f64> = spans
        .iter()
        .filter(|t| t.dash.is_some())
        .map(|t| lowest(t))
        .collect();
    assert_eq!(solid.len(), 1);
    assert!(dashed.len() >= 4, "{dashed:?}");
    assert!(
        dashed
            .iter()
            .filter(|z| (**z - solid[0]).abs() > 0.05)
            .count()
            >= 3,
        "{solid:?} {dashed:?}"
    );
    // Without the ground rule tick: the typed clearance.
    let typed = crate::drawings::DrawingsWindow {
        clearance_rule: false,
        clearance: Some(8.0),
        ..w
    };
    assert_eq!(
        crate::drawings::Input::new(&m, 0, &typed)
            .unwrap()
            .options
            .clearance,
        Some(8.0)
    );
    let _ = std::fs::remove_file(&logo);
}

/// Edit › Versions: commit, branch (a project line), work on it and merge
/// as project operations; the model follows (re-solved) and the window lists them.
#[test]
fn versions_commit_branch_checkout_and_merge() {
    use ohl_server::Op;
    let mut m = synthetic_project("versions");
    let p = m.line.structure("T6").unwrap().position;
    for (what, op) in [
        (
            "commit",
            Op::Commit {
                message: "as imported".into(),
            },
        ),
        (
            "branch",
            Op::CreateBranch {
                name: "alt".into(),
                at: None,
            },
        ),
    ] {
        m.project_op(what, op).unwrap();
    }
    switch(&mut m, "alt");
    m.apply(
        "moved T6",
        ohl_session::Command::MoveStructure {
            id: "T6".into(),
            position: [p[0] + 4.0, p[1], p[2]],
        },
    )
    .unwrap();
    m.project_op(
        "commit",
        Op::Commit {
            message: "T6 east".into(),
        },
    )
    .unwrap();
    switch(&mut m, "main");
    assert_eq!(m.line.structure("T6").unwrap().position, p);
    assert_solved_fully(&m);
    m.project_op(
        "merge",
        Op::Merge {
            other: "alt".into(),
            message: None,
            choices: Default::default(),
        },
    )
    .unwrap();
    assert_eq!(m.line.structure("T6").unwrap().position[0], p[0] + 4.0);
    assert_solved_fully(&m);

    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|cc| App::with_model(m, cc.wgpu_render_state.clone()));
    harness.run();
    harness.state_mut().ready().unwrap().versions.open = true;
    harness.run();
    assert!(harness.query_by_label_contains("On branch main").is_some());
    assert!(harness.query_by_label("T6 east").is_some());
    assert!(harness.query_by_label("as imported").is_some());
    // The working state against the head: the same.
    harness.get_by_label("Changes").click();
    harness.run();
    harness.get_by_label("Compare").click();
    harness.run();
    assert!(harness.query_by_label_contains("no differences").is_some());
}

/// Criteria › Bimetallic conductor model: bird cage for the line, one
/// command (undoable), every section solved again.
#[test]
fn the_bimetallic_model_is_chosen_for_the_line() {
    let model = synthetic_project("bimetallic");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.get_by_label("Criteria").click();
    harness.run();
    harness.get_by_label("Bimetallic conductor model…").click();
    harness.run();
    harness
        .get_by_label("The outer strands take no compression (bird cage)")
        .click();
    harness.run();
    harness.get_by_label("OK").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.bimetallic_window.is_none());
    assert_eq!(
        r.model.line.bimetallic.behavior,
        ohl_cables::BimetallicBehavior::BirdCage
    );
    // Kept in the project: the session's line has it.
    let e = r.model.editing.as_ref().unwrap();
    assert_eq!(
        e.session.line().bimetallic.behavior,
        ohl_cables::BimetallicBehavior::BirdCage
    );
    // Undone like any edit.
    r.model.undo().unwrap();
    assert!(r.model.line.bimetallic.is_default());
}

/// Criteria › Loads for structures: a tick menu that stays open while two
/// weather cases are ticked and a load case unticked in a row; each tick
/// kept in the project (the line's `structure_loads` settings row),
/// undone like any edit.
#[test]
fn loads_for_structures_are_ticked_in_a_menu_that_stays_open() {
    let model = synthetic_project("structure-loads");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.get_by_label("Criteria").click();
    harness.run();
    harness
        .get_by_label_contains("Loads for structures")
        .hover();
    harness.run();
    harness.get_by_label("glaze ice").click();
    harness.run();
    // Still open: the next boxes can be ticked.
    harness.get_by_label("high wind").click();
    harness.run();
    harness.get_by_label("wind").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    let want = ohl_line::StructureLoads {
        load_cases: Some(vec!["everyday".into(), "ice".into()]),
        weather_cases: vec!["glaze ice".into(), "high wind".into()],
    };
    assert_eq!(r.model.line.structure_loads, want);
    // Kept in the project: the session's line has it.
    let e = r.model.editing.as_ref().unwrap();
    assert_eq!(e.session.line().structure_loads, want);
    // Undone like any edit (one tick each).
    r.model.undo().unwrap();
    assert_eq!(r.model.line.structure_loads.load_cases, None);
    assert_eq!(r.model.line.structure_loads.weather_cases.len(), 2);
}

/// A transverse Mercator projection centred on the synthetic line's origin
/// (invented, for the tests).
const LOCAL_TM: &str =
    "+proj=tmerc +lat_0=46 +lon_0=8 +k=1 +x_0=0 +y_0=0 +ellps=GRS80 +units=m +no_defs";

/// The coordinate-system picker: a code typed whole is chosen, words find
/// a system by its name, a row shows code, name and unit, Enter takes
/// the first row.
#[test]
fn the_coordinate_system_picker_searches() {
    let mut h = Harness::new_ui_state(
        |ui, p: &mut crate::crs::Picker| p.ui(ui, "test"),
        crate::crs::Picker::new(""),
    );
    h.run();
    h.get_by(|n| n.role() == egui::accesskit::Role::TextInput)
        .focus();
    h.run();
    h.event(egui::Event::Text("EPSG:2056".into()));
    h.run();
    assert_eq!(h.state().spec, "EPSG:2056", "a code typed whole is chosen");
    let lv95 = ohl_crs::known_code(2056).unwrap();
    let row = crate::crs::row_text(&lv95);
    assert!(
        row.contains("2056") && row.contains(lv95.name) && row.ends_with("·  m"),
        "{row}"
    );
    h.get_by_label(&row);
    // Words of the name: a row clicked is chosen.
    let utm = ohl_crs::known_code(32632).unwrap();
    h.state_mut().filter = utm.name.to_lowercase();
    h.run();
    h.get_by_label(&crate::crs::row_text(&utm)).click();
    h.run();
    assert_eq!(h.state().spec, "EPSG:32632");
    // Enter in the search takes the first row.
    h.state_mut().filter = "lv95".into();
    h.run();
    let first = ohl_crs::search("lv95")[0].spec();
    h.get_by(|n| n.role() == egui::accesskit::Role::TextInput)
        .focus();
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(h.state().spec, first);
    // Nothing found: nothing listed, the choice stays.
    h.state_mut().filter = "no such system at all".into();
    h.run();
    h.get_by_label_contains("0 systems listed");
    assert_eq!(h.state().spec, first);
}

#[test]
fn the_coordinate_system_is_defined_converted_and_the_project_reprojected() {
    let model = synthetic_project("crs");
    let file = model.sources.file.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // Define: from the menu; the system typed as a proj4 string.
    harness.get_by_label("Terrain").click();
    harness.run();
    harness.get_by_label_contains("Coordinate system").click();
    harness.run();
    harness.get_by_label("Define…").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let d = r.crs.define.as_mut().unwrap();
        d.picker.category = crate::crs::Category::Proj4;
        d.picker.proj4 = LOCAL_TM.into();
        d.picker.spec = LOCAL_TM.into();
    }
    harness.run();
    harness.get_by_label("OK").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.crs.define.is_none());
    assert_eq!(r.model.line.crs, LOCAL_TM);
    let e = r.model.editing.as_ref().unwrap();
    assert_eq!(e.session.line().crs, LOCAL_TM, "kept in the project");
    // The calculator opens with the project's system.
    harness.get_by_label("Terrain").click();
    harness.run();
    harness.get_by_label_contains("Coordinate system").click();
    harness.run();
    harness
        .get_by_label("Coordinate system calculator…")
        .click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    let c = r.crs.calculator.as_mut().unwrap();
    assert_eq!(c.from.spec, LOCAL_TM);
    c.input = "0 0 500\n\n# a note\n1000, 2000".into();
    c.compute();
    let out: Vec<&str> = c.output.lines().collect();
    assert_eq!(out[0], "8.000000000 46.000000000 500.0000", "{}", c.output);
    assert_eq!(&out[1..3], ["", "# a note"]);
    assert!(out[3].starts_with("8.01"), "{}", c.output);
    c.input = "1 2 3 4".into();
    c.compute();
    assert!(c.output.starts_with('—'), "{}", c.output);
    r.crs.calculator = None;
    harness.run();
    // Reproject into UTM 32N: a new project, this one unchanged.
    harness.get_by_label("Terrain").click();
    harness.run();
    harness.get_by_label_contains("Coordinate system").click();
    harness.run();
    harness.get_by_label("Reproject project…").click();
    harness.run();
    assert!(harness.state_mut().ready().unwrap().crs.reproject.is_some());
    let target = file.with_file_name("moved.ohl");
    std::fs::remove_file(&target).ok();
    {
        let s = ohl_session::Session::open(&file).unwrap();
        s.put_surface(
            "tester",
            "ground",
            "test",
            &[[0.0, 0.0, 400.0], [500.0, 0.0, 410.0], [0.0, 500.0, 420.0]],
            &[[0, 1, 2]],
        )
        .unwrap();
    }
    let mut store = ohl_pointcloud::Store::open(&file).unwrap();
    let pts = (0..50).map(|k| ohl_lidar::LidarPoint {
        x: k as f64 * 3.0,
        y: 10.0,
        z: 400.0,
        class: 2,
        intensity: 7,
        return_number: 1,
        number_of_returns: 1,
    });
    store
        .import_points(
            "lidar",
            LOCAL_TM,
            None,
            64.0,
            0.001,
            pts,
            &ohl_core::budget::Budget::new(),
        )
        .unwrap();
    drop(store);
    let (from, to) = (
        ohl_crs::Crs::parse(LOCAL_TM).unwrap(),
        ohl_crs::Crs::parse("EPSG:32632").unwrap(),
    );
    let report =
        crate::crs::reproject_file(&file, &target, &from, &to, "tester", &|_, _| {}).unwrap();
    assert!(report.contains("1 surface"), "{report}");
    let t = ohl_crs::Transformation::new(&from, &to, ohl_crs::Heights::Keep);
    let old = ohl_session::Session::open(&file).unwrap();
    let new = ohl_session::Session::open(&target).unwrap();
    assert_eq!(old.line().crs, LOCAL_TM, "the old project is not changed");
    assert_eq!(new.line().crs, "EPSG:32632");
    for (a, b) in old.line().structures.iter().zip(&new.line().structures) {
        let p = t.point(a.position).unwrap();
        assert!((p[0] - b.position[0]).hypot(p[1] - b.position[1]) < 1e-6);
        assert_eq!(p[2], b.position[2]);
    }
    let (points, _) = new.surface("ground").unwrap();
    assert!((points[0][0] - t.point([0.0, 0.0, 400.0]).unwrap()[0]).abs() < 1e-6);
    let store = ohl_pointcloud::Store::open(&target).unwrap();
    let cloud = store.cloud(Some("lidar")).unwrap();
    assert_eq!((cloud.points, cloud.crs.as_str()), (50, "EPSG:32632"));
    assert!(cloud.bounds[0][0] > 400_000.0, "{:?}", cloud.bounds);
    // Not again into an existing file.
    let r = harness.state_mut().ready().unwrap();
    let err = crate::crs::reproject_check_for_tests(&r.model, "EPSG:32632", &target);
    assert!(err.unwrap_err().contains("exists already"));
}

/// The height datum in Terrain › Coordinate system › Define…, next to
/// the coordinate system (kept as it is: the project's local system is
/// no cartographic one, and OK still takes the datum): a code that is no
/// number is said and OK is off; a name and a code set the project's
/// datum (one edit, kept in the project, undone as any edit), shown in
/// the menu, the window and the properties of the project.
#[test]
fn the_height_datum_is_defined_with_the_coordinate_system() {
    let model = synthetic_project("height-datum");
    let crs = model.line.crs.clone();
    assert!(ohl_crs::Crs::parse(&crs).is_err(), "{crs}");
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    h.run();
    h.get_by_label("height datum");
    h.get_by_label("not given");
    let open_define = |h: &mut Harness<'_, App>| {
        h.get_by_label("Terrain").click();
        h.run();
        h.get_by_label_contains("Coordinate system").click();
        h.run();
        h.get_by_label("Define…").click();
        h.run();
    };
    open_define(&mut h);
    {
        let d = h.state_mut().ready().unwrap().crs.define.as_mut().unwrap();
        assert_eq!((d.datum_name.as_str(), d.datum_epsg.as_str()), ("", ""));
        d.datum_name = "LN02".into();
        d.datum_epsg = "LN".into();
    }
    h.run();
    h.get_by_label_contains("is not an EPSG code");
    h.get_by_label("OK").click();
    h.run();
    assert!(
        h.state_mut().ready().unwrap().crs.define.is_some(),
        "OK is off"
    );
    h.state_mut()
        .ready()
        .unwrap()
        .crs
        .define
        .as_mut()
        .unwrap()
        .datum_epsg = "EPSG:5728".into();
    h.run();
    h.get_by_label("On the sheets: LN02 (EPSG:5728)");
    h.get_by_label("OK").click();
    h.run();
    let r = h.state_mut().ready().unwrap();
    assert!(r.crs.define.is_none());
    let want = Some(ohl_line::HeightDatum {
        name: "LN02".into(),
        epsg: Some(5728),
    });
    assert_eq!(r.model.line.height_datum, want);
    assert_eq!(r.model.line.crs, crs, "the system kept");
    let e = r.model.editing.as_ref().unwrap();
    assert_eq!(e.session.line().height_datum, want, "kept in the project");
    h.run();
    h.get_by_label("LN02 (EPSG:5728)");
    // Shown again when the window opens.
    open_define(&mut h);
    h.get_by_label("Height datum now: LN02 (EPSG:5728)");
    {
        let r = h.state_mut().ready().unwrap();
        let d = r.crs.define.as_ref().unwrap();
        assert_eq!(
            (d.datum_name.as_str(), d.datum_epsg.as_str()),
            ("LN02", "5728")
        );
        r.crs.define = None;
    }
    // Undone as any edit.
    let r = h.state_mut().ready().unwrap();
    assert!(r.perform(crate::edit::Action::Undo));
    assert_eq!(r.model.line.height_datum, None);
}

#[test]
#[ignore = "writes images of the coordinate system windows to OHL_SHOTS (a look by eye)"]
fn coordinate_system_window_images() {
    let Ok(dir) = ohl_core::env::var("OHL_SHOTS") else {
        return;
    };
    let model = synthetic_project("crs-shots");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let shot = |h: &mut Harness<'_, App>, name: &str| {
        h.run();
        h.render()
            .unwrap()
            .save(format!("{dir}/{name}.png"))
            .unwrap();
    };
    {
        let r = harness.state_mut().ready().unwrap();
        let mut p = crate::crs::Picker::new("EPSG:2056");
        p.filter = "LV95".into();
        let mut d = crate::crs::Define::new(&r.model);
        d.picker = p;
        d.datum_name = "LN02".into();
        d.datum_epsg = "5728".into();
        r.crs.define = Some(d);
    }
    shot(&mut harness, "define");
    {
        let r = harness.state_mut().ready().unwrap();
        r.crs.define = None;
        let mut c = crate::crs::Calculator::new(&r.model);
        c.from = crate::crs::Picker::new(LOCAL_TM);
        c.input = "0 0 500\n1000 2000".into();
        c.compute();
        r.crs.calculator = Some(c);
    }
    shot(&mut harness, "calculator");
    {
        let r = harness.state_mut().ready().unwrap();
        r.crs.calculator = None;
        r.crs.reproject = Some(crate::crs::Reproject::new());
    }
    shot(&mut harness, "reproject");
}

/// A synthetic project with an orthophoto (layer "Ortho") and a DXF
/// drawing ("cadastre") over the line, read into its model.
fn project_with_backgrounds(name: &str) -> model::Model {
    let mut model = synthetic_project(name);
    let file = model.sources.file.clone();
    let dir = file.parent().unwrap().to_path_buf();
    // 1400 m × 700 m at 2 m per pixel over the line (x 0…1250, y 0…560).
    let png = dir.join("ortho.png");
    image::RgbImage::from_fn(700, 350, |x, y| {
        image::Rgb([(x % 200) as u8 + 30, (y % 200) as u8 + 30, 90])
    })
    .save(&png)
    .unwrap();
    std::fs::write(dir.join("ortho.pgw"), "2\n0\n0\n-2\n-49\n609\n").unwrap();
    let report =
        crate::backgrounds::import_images(&file, "Ortho", &[png], "", "", &|_, _| {}).unwrap();
    assert!(report.starts_with("1 image(s)"), "{report}");
    let dxf = dir.join("cadastre.dxf");
    std::fs::write(
        &dxf,
        "0\nSECTION\n2\nENTITIES\n0\nLWPOLYLINE\n8\nParcels\n62\n3\n70\n1\n38\n500\n\
         10\n100\n20\n-40\n10\n600\n20\n-40\n10\n600\n20\n40\n10\n100\n20\n40\n\
         0\nTEXT\n8\nParcels\n10\n300\n20\n0\n40\n8\n1\nParcel 7\n0\nENDSEC\n0\nEOF\n",
    )
    .unwrap();
    ohl_background::Store::open(&file)
        .unwrap()
        .import_dxf("cadastre", &dxf, "")
        .unwrap();
    let mut log = Vec::new();
    model.backgrounds = crate::backgrounds::load(&file, &mut log);
    model
}

#[test]
fn backgrounds_are_kept_shown_and_hidden() {
    use crate::scene::Layer;
    let model = project_with_backgrounds("backgrounds");
    assert_eq!(
        model.backgrounds.image_layers(),
        vec![("Ortho".to_owned(), 1)]
    );
    assert_eq!(model.backgrounds.drawings.len(), 1);
    // The drawing in 3D at its heights; orthophotos in plan only.
    let s = crate::scene::layer(&model, Layer::Drawings, 0, &Selection::None);
    assert!(s.counts().0 >= 4, "{:?}", s.counts());
    let s = crate::scene::layer(&model, Layer::Orthophotos, 0, &Selection::None);
    assert_eq!(s.counts(), (0, 0, 0));
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // Shown by default.
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.visible.contains(&Layer::Orthophotos) && r.visible.contains(&Layer::Drawings));
    }
    // Terrain › Backgrounds: two boxes ticked in a row (the menu stays
    // open).
    harness.get_by_label("Terrain").click();
    harness.run();
    harness.get_by_label_contains("Backgrounds").hover();
    harness.run();
    harness.get_by_label_contains("Ortho (1)").click();
    harness.run();
    harness.get_by_label_contains("cadastre").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.hidden_backgrounds.contains("image:Ortho"));
    assert!(r.model.hidden_backgrounds.contains("drawing:cadastre"));
    // Remembered with this user's view.
    assert_eq!(r.model.view_prefs().hidden_backgrounds.len(), 2);
    let s = crate::scene::layer(&r.model, Layer::Drawings, 0, &Selection::None);
    assert_eq!(s.counts().0, 0, "hidden in 3D too");
    // A drawing hidden: its chunks made again; an edit of the line or
    // the whole layer hidden: kept (a large drawing takes a while).
    assert!(r.view3d.dirty.contains(&Layer::Drawings));
    r.view3d.dirty.clear();
    r.refresh_views();
    assert!(r.view3d.dirty.contains(&Layer::Structures));
    assert!(!r.view3d.dirty.contains(&Layer::Drawings));
    r.view3d.dirty.clear();
    harness
        .get_by_label_contains("Drawings (plan and 3D views)")
        .click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(!r.visible.contains(&Layer::Drawings));
    assert!(!r.view3d.dirty.contains(&Layer::Drawings));
}

#[test]
#[ignore = "writes images of the backgrounds to OHL_SHOTS (a look by eye)"]
fn background_images() {
    let Ok(dir) = ohl_core::env::var("OHL_SHOTS") else {
        return;
    };
    let model = project_with_backgrounds("bg-shots");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.state_mut().show_tab(crate::app::Tab::Plan);
    for _ in 0..10 {
        harness.run();
    }
    harness
        .render()
        .unwrap()
        .save(format!("{dir}/plan.png"))
        .unwrap();
    harness.state_mut().show_tab(crate::app::Tab::View3d);
    for _ in 0..5 {
        harness.run();
    }
    harness
        .render()
        .unwrap()
        .save(format!("{dir}/3d.png"))
        .unwrap();
}

/// Terrain surfaces › Build from LiDAR: the model's classes in a list to
/// tick (the ground classes at first); nothing to tick without LiDAR.
#[test]
fn the_ground_is_built_from_the_classes_ticked() {
    use egui_kittest::kittest::NodeT;
    let mut model = synthetic_project("build-classes");
    model.points = vec![
        ([0, 0, 0], 2),
        ([1000, 0, 0], 3),
        ([2000, 0, 0], 5),
        ([3000, 0, 0], 6),
    ];
    model.class_counts = vec![(2, 40), (3, 10), (5, 7), (6, 2)];
    model.cloud = Some("survey.ohl".into());
    let classes = crate::surfaces::lidar_classes(&model);
    assert_eq!(
        classes.iter().map(|c| (c.0, c.2)).collect::<Vec<_>>(),
        [(2, 40), (3, 10), (5, 7), (6, 2)]
    );
    let label = |code: u8| {
        let c = classes.iter().find(|c| c.0 == code).unwrap();
        format!("{} {} ({} points)", c.0, c.1, c.2)
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(
            r.surfaces.lidar.chosen(&classes),
            [2].into(),
            "the ground classes"
        );
        r.surfaces.show(&r.model);
    }
    harness.run();
    harness.get_by_label(&label(3)).click();
    harness.run();
    harness.get_by_label(&label(2)).click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.surfaces.lidar.classes, Some([3].into()));
    // Without LiDAR: the list is empty and Build cannot be pressed.
    let model = synthetic_project("build-no-lidar");
    assert!(crate::surfaces::lidar_classes(&model).is_empty());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        r.surfaces.show(&r.model);
    }
    harness.run();
    harness.get_by_label("The model has no LiDAR points.");
    assert!(harness.get_by_label("Build").accesskit_node().is_disabled());
}

#[test]
#[ignore = "writes an image of the surfaces window to OHL_SHOTS (a look by eye)"]
fn surfaces_window_image() {
    let Ok(dir) = ohl_core::env::var("OHL_SHOTS") else {
        return;
    };
    let mut model = synthetic_project("surfaces-shot");
    model.points = vec![
        ([0, 0, 0], 2),
        ([1000, 0, 0], 3),
        ([2000, 0, 0], 5),
        ([3000, 0, 0], 6),
    ];
    model.class_counts = vec![
        (1, 900),
        (2, 40),
        (3, 10),
        (4, 12),
        (5, 7),
        (6, 2),
        (7, 1),
        (9, 3),
        (17, 4),
    ];
    model.cloud = Some("survey.ohl".into());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        r.surfaces.show(&r.model);
    }
    harness.run();
    harness.run();
    harness
        .render()
        .unwrap()
        .save(format!("{dir}/surfaces.png"))
        .unwrap();
}

/// A long list of errors scrolls in a box: the window stays on the screen
/// and can be closed (the rule for every message in a window).
#[test]
fn long_errors_scroll_and_the_window_closes() {
    let model = synthetic_project("long-errors");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let long: String = (0..200)
        .map(|k| format!("tile_{k:03}.ecw: an ECW file needs a converter\n"))
        .collect();
    {
        let ctx = harness.ctx.clone();
        let r = harness.state_mut().ready().unwrap();
        r.backgrounds.job = Some(crate::survey_io::Job::start(
            "Import".into(),
            true,
            &ctx,
            move |_| Err(long),
        ));
    }
    run_until(&mut harness, |a| {
        a.ready()
            .unwrap()
            .backgrounds
            .job
            .as_mut()
            .is_some_and(|j| {
                j.poll();
                j.done.is_some()
            })
    });
    harness.run();
    let close = harness.get_by_label("Close").rect();
    assert!(close.max.y < 900.0, "Close at {close:?}, off the screen");
    harness.get_by_label("Close").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.backgrounds.job.is_none());
    assert!(!r.reopen, "a failed import opens nothing again");
}

#[test]
fn image_import_failures_are_grouped_by_reason() {
    let model = synthetic_project("grouped");
    let dir = model.sources.file.parent().unwrap().to_path_buf();
    let files: Vec<PathBuf> = (0..8).map(|k| dir.join(format!("t{k}.ecw"))).collect();
    let e = crate::backgrounds::import_images(&model.sources.file, "O", &files, "", "", &|_, _| {})
        .unwrap_err();
    assert_eq!(e.lines().count(), 2, "{e}");
    assert!(
        e.contains("8 file(s): an ECW file needs a converter"),
        "{e}"
    );
    assert!(e.contains("and 3 more"), "{e}");
}

/// Drawings in the level of detail: chunked like the other 3D layers,
/// simplified at levels 1 and 2 to the row's tolerance; a row of their
/// own in the Level of detail table.
#[test]
fn drawings_have_levels_of_detail() {
    use crate::lod::{CHUNKED, DetailKind, LodSettings};
    use crate::scene::Layer;
    assert!(CHUNKED.contains(&Layer::Drawings));
    assert_eq!(DetailKind::of(Layer::Drawings), DetailKind::Tolerance);
    let row = LodSettings::default().row(Layer::Drawings);
    assert_eq!(
        (row.render_distance, row.from, row.detail),
        (10_000.0, [300.0, 2_000.0], [0.2, 1.5])
    );
    row.check(Layer::Drawings).unwrap();
    // Douglas–Peucker: a fine arc of radius 100 m (1° steps) within 0.5 m.
    let arc: Vec<[f64; 3]> = (0..=90)
        .map(|k| {
            let a = f64::from(k).to_radians();
            [100.0 * a.cos(), 100.0 * a.sin(), 0.0]
        })
        .collect();
    let s = crate::chunks::simplify(&arc, 0.5);
    assert!(s.len() < 20 && s.len() >= 3, "{}", s.len());
    assert_eq!((s[0], *s.last().unwrap()), (arc[0], arc[90]));
    // The test drawing: a 500 m parcel crossing tiles, cut into pieces.
    let mut model = project_with_backgrounds("lod-drawings");
    let index = crate::chunks::ChunkIndex::new(&model, Layer::Drawings, 0);
    assert!(index.len() >= 2, "{}", index.len());
    let lines = |level: usize, model: &crate::model::Model| -> usize {
        let index = crate::chunks::ChunkIndex::new(model, Layer::Drawings, 0);
        index
            .keys()
            .map(|(t, _)| {
                index
                    .build(model, Layer::Drawings, &t, level, &row)
                    .counts()
                    .0
            })
            .sum()
    };
    let full = lines(0, &model);
    assert!(full >= 4, "{full}");
    // A 1 m line: drawn at level 0, left out at level 2 (1.5 m tolerance).
    let d = std::sync::Arc::make_mut(&mut model.backgrounds.drawings[0].drawing);
    d.lines.push(ohl_background::dxf::Polyline {
        layer: 0,
        color: [255, 0, 0],
        points: vec![[300.0, 300.0, 500.0], [301.0, 300.0, 500.0]],
        closed: false,
    });
    assert_eq!(lines(0, &model), full + 1);
    assert_eq!(lines(2, &model), full);
    // Hidden: no chunks.
    model.hidden_backgrounds.insert("drawing:cadastre".into());
    assert_eq!(
        crate::chunks::ChunkIndex::new(&model, Layer::Drawings, 0).len(),
        0
    );
}

#[test]
#[ignore = "writes images of the Lines menu and windows to OHL_SHOTS (a look by eye)"]
fn lines_images() {
    use crate::project_lines::LinesWindow;
    let Ok(dir) = ohl_core::env::var("OHL_SHOTS") else {
        return;
    };
    let model = synthetic_project("lines-shots");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        for n in ["phase 1", "phase 2"] {
            r.project_op(
                n,
                ohl_server::Op::CreateLine {
                    name: n.into(),
                    from: "main".into(),
                },
            );
        }
        r.model.show_line("phase 2").unwrap();
        // A conflict to settle: T3 moved on both lines.
        let p = r.model.line.structure("T3").unwrap().position;
        r.perform(crate::edit::Action::Command {
            what: "T3".into(),
            command: ohl_session::Command::MoveStructure {
                id: "T3".into(),
                position: [p[0] + 3.0, p[1], p[2]],
            },
        });
        r.switch_line("phase 1");
        for k in 0..r.model.cases.len() {
            r.model.ensure_solved(k);
        }
        r.perform(crate::edit::Action::Command {
            what: "T3".into(),
            command: ohl_session::Command::MoveStructure {
                id: "T3".into(),
                position: [p[0] - 3.0, p[1], p[2]],
            },
        });
    }
    harness.run();
    harness.get_by_label("Versions").click();
    harness.run();
    harness.get_by_label_contains("Project lines").hover();
    harness.run();
    harness
        .render()
        .unwrap()
        .save(format!("{dir}/lines-menu.png"))
        .unwrap();
    harness.get_by_label("Project lines…").click();
    harness.run();
    harness.run();
    harness
        .render()
        .unwrap()
        .save(format!("{dir}/lines-window.png"))
        .unwrap();
    harness.state_mut().ready().unwrap().lines_window = Some(LinesWindow::BringIn {
        from: "main".into(),
        choices: Default::default(),
        plan: None,
    });
    harness.run();
    harness.run();
    harness
        .render()
        .unwrap()
        .save(format!("{dir}/lines-bring-in.png"))
        .unwrap();
}

/// A project opens on the line this user worked on, with the lines they
/// showed (remembered on their computer).
#[test]
fn the_line_worked_on_and_those_shown_are_remembered() {
    let mut m = synthetic_project("lines-remembered");
    m.project_op(
        "phase",
        ohl_server::Op::CreateLine {
            name: "phase 1".into(),
            from: "main".into(),
        },
    )
    .unwrap();
    switch(&mut m, "phase 1");
    m.show_line("main").unwrap();
    let prefs = m.view_prefs();
    assert_eq!(prefs.line.as_deref(), Some("phase 1"));
    assert_eq!(prefs.shown_lines, ["main"]);
    let mut sources = m.sources.clone();
    drop(m);
    sources.view = Some(prefs);
    let again = model::load(sources, &|_| {}).unwrap();
    assert_eq!(again.project_line(), "phase 1");
    assert_eq!(again.other_lines.len(), 1);
    assert_eq!(again.other_lines[0].name, "main");
}

/// The working tree: a lane per line, where each started, merges, the
/// lines deleted; opened from Lines › Project lines › Working tree; admins
/// roll a line back from it.
#[test]
fn the_working_tree_shows_the_lines_through_time() {
    use ohl_server::Op;
    let mut m = synthetic_project("tree");
    let shift = |m: &mut model::Model, id: &str, dx: f64| {
        let p = m.line.structure(id).unwrap().position;
        m.apply(
            "shift",
            ohl_session::Command::MoveStructure {
                id: id.into(),
                position: [p[0] + dx, p[1], p[2]],
            },
        )
        .unwrap();
    };
    let op = |m: &mut model::Model, op: Op| {
        m.project_op("op", op).unwrap();
    };
    op(
        &mut m,
        Op::Commit {
            message: "start".into(),
        },
    );
    let start = m.session().unwrap().head_revision().unwrap().unwrap();
    for n in ["phase 1", "phase 2"] {
        op(
            &mut m,
            Op::CreateLine {
                name: n.into(),
                from: "main".into(),
            },
        );
    }
    switch(&mut m, "phase 2");
    shift(&mut m, "T5", 1.0);
    op(
        &mut m,
        Op::Commit {
            message: "phase 2 work".into(),
        },
    );
    switch(&mut m, "phase 1");
    shift(&mut m, "T6", 2.0);
    op(
        &mut m,
        Op::Commit {
            message: "phase 1 work".into(),
        },
    );
    switch(&mut m, "main");
    shift(&mut m, "T2", 1.0);
    op(
        &mut m,
        Op::Commit {
            message: "main fix".into(),
        },
    );
    op(
        &mut m,
        Op::DeleteLine {
            name: "phase 2".into(),
        },
    );
    switch(&mut m, "phase 1");
    op(
        &mut m,
        Op::BringIn {
            from: "main".into(),
            choices: Default::default(),
        },
    );
    shift(&mut m, "T6", 1.0);
    let s = m.session().unwrap();
    let tree = crate::working_tree::Tree::layout(
        &s.revisions().unwrap(),
        &s.lines().unwrap(),
        &[],
        &|l| s.has_uncommitted(l).unwrap(),
    );
    let lanes = crate::working_tree::lanes_by_name(&tree);
    assert_eq!(lanes["main"], ["start", "main fix"]);
    assert_eq!(
        lanes["phase 1"],
        ["phase 1 work", "Bring main into phase 1"]
    );
    assert_eq!(lanes["lines deleted"], ["phase 2 work"]);
    // Time goes right: every parent before its child.
    for n in &tree.nodes {
        for p in &n.revision.parents {
            assert!(tree.nodes[tree.index[p]].col < n.col);
        }
    }
    let merge = tree
        .nodes
        .iter()
        .find(|n| n.revision.parents.len() == 2)
        .unwrap();
    assert_eq!(tree.lanes[merge.lane].name, "phase 1");
    assert!(
        tree.lanes
            .iter()
            .find(|l| l.name == "phase 1")
            .unwrap()
            .uncommitted
    );
    assert!(!tree.lanes[0].uncommitted);

    // From the menu; the admin rolls main back to the start.
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(m, None));
    harness.run();
    harness.get_by_label("Versions").click();
    harness.run();
    harness.get_by_label_contains("Project lines").hover();
    harness.run();
    harness.get_by_label("Working tree…").click();
    harness.run();
    assert!(harness.state_mut().ready().unwrap().working_tree.open);
    {
        let r = harness.state_mut().ready().unwrap();
        r.working_tree.selected = Some(start.clone());
    }
    harness.run();
    harness.get_by_label("Roll back…").click();
    harness.run();
    harness.get_by_label("Roll back").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let s = r.model.session().unwrap();
        let t2 = |line: &str| {
            s.line_named(line)
                .unwrap()
                .structure("T2")
                .unwrap()
                .position[0]
        };
        let first = s.revision_state(&start).unwrap().line().unwrap();
        assert_eq!(
            t2("main"),
            first.structure("T2").unwrap().position[0],
            "{:?}",
            r.status
        );
        assert_eq!(r.model.project_line(), "phase 1", "still on phase 1");
        // A designer sees why not.
        if let Ok(e) = &mut r.model.editing {
            e.session
                .set_user("tester", "ben", ohl_session::Role::Designer)
                .unwrap();
            e.user = "ben".into();
        }
    }
    harness.run();
    assert!(
        harness
            .query_by_label("Rolling a line back: admins only.")
            .is_some()
    );
}

#[test]
#[ignore = "writes an image of the working tree to OHL_SHOTS (a look by eye)"]
fn working_tree_image() {
    use ohl_server::Op;
    let Ok(dir) = ohl_core::env::var("OHL_SHOTS") else {
        return;
    };
    let mut m = synthetic_project("tree-shot");
    let shift = |m: &mut model::Model, id: &str, dx: f64| {
        let p = m.line.structure(id).unwrap().position;
        m.apply(
            "shift",
            ohl_session::Command::MoveStructure {
                id: id.into(),
                position: [p[0] + dx, p[1], p[2]],
            },
        )
        .unwrap();
    };
    let op = |m: &mut model::Model, op: Op| {
        m.project_op("op", op).unwrap();
    };
    op(
        &mut m,
        Op::Commit {
            message: "as imported".into(),
        },
    );
    op(
        &mut m,
        Op::Tag {
            name: "IFC rev A".into(),
            at: None,
            note: String::new(),
        },
    );
    for n in ["phase 1", "phase 2", "route B"] {
        op(
            &mut m,
            Op::CreateLine {
                name: n.into(),
                from: "main".into(),
            },
        );
    }
    for (line, id, msg) in [
        ("phase 1", "T6", "T6 for the road"),
        ("main", "T2", "T2 corrected"),
        ("phase 2", "T5", "T5 raised"),
        ("phase 1", "T7", "T7 replaced"),
        ("route B", "T4", "route B start"),
        ("main", "T3", "T3 corrected"),
    ] {
        switch(&mut m, line);
        shift(&mut m, id, 1.0);
        op(
            &mut m,
            Op::Commit {
                message: msg.into(),
            },
        );
    }
    switch(&mut m, "phase 1");
    op(
        &mut m,
        Op::BringIn {
            from: "main".into(),
            choices: Default::default(),
        },
    );
    switch(&mut m, "main");
    op(
        &mut m,
        Op::DeleteLine {
            name: "route B".into(),
        },
    );
    shift(&mut m, "T2", 0.5);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(m, None));
    harness.run();
    harness.state_mut().ready().unwrap().working_tree.open = true;
    harness.run();
    harness.run();
    harness
        .render()
        .unwrap()
        .save(format!("{dir}/working-tree.png"))
        .unwrap();
}

/// Working tree: a new line with only uncommitted work can be clicked (its
/// open dot) and committed there; "New line from it" names the revision in
/// words (its line, when, its message), not by its id.
#[test]
fn a_new_lines_work_is_committed_from_the_tree() {
    use crate::project_lines::LinesWindow;
    use ohl_server::Op;
    let mut m = synthetic_project("tree-work");
    m.project_op(
        "c",
        Op::Commit {
            message: "as imported".into(),
        },
    )
    .unwrap();
    m.project_op(
        "l",
        Op::CreateLine {
            name: "phase 1".into(),
            from: "main".into(),
        },
    )
    .unwrap();
    switch(&mut m, "phase 1");
    let p = m.line.structure("T2").unwrap().position;
    m.apply(
        "T2",
        ohl_session::Command::MoveStructure {
            id: "T2".into(),
            position: [p[0] + 1.0, p[1], p[2]],
        },
    )
    .unwrap();
    let start = m.session().unwrap().head_revision().unwrap().unwrap();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(m, None));
    harness.run();
    harness.state_mut().ready().unwrap().working_tree.open = true;
    harness.run();
    // Its uncommitted work (the open dot of lane 1), committed here.
    harness.state_mut().ready().unwrap().working_tree.work = Some((1, "T2 east".into()));
    harness.run();
    assert!(
        harness
            .query_by_label("phase 1: work not committed yet")
            .is_some()
    );
    harness.get_by_label("Commit").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let s = r.model.session().unwrap();
        assert!(!s.has_uncommitted("phase 1").unwrap(), "{:?}", r.status);
        assert_eq!(
            s.revision(&s.head_revision().unwrap().unwrap())
                .unwrap()
                .message,
            "T2 east"
        );
        // New line from the first revision: shown in words.
        r.working_tree.selected = Some(start.clone());
    }
    harness.run();
    harness.get_by_label("New line from it…").click();
    harness.run();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    let Some(LinesWindow::New {
        from,
        at: Some((id, words)),
        ..
    }) = &r.lines_window
    else {
        panic!("{:?}", r.lines_window);
    };
    assert_eq!((from, id), (&start, &start));
    assert!(
        words.starts_with("main at ") && words.ends_with("as imported"),
        "{words}"
    );
    if let Ok(dir) = ohl_core::env::var("OHL_SHOTS") {
        harness
            .render()
            .unwrap()
            .save(format!("{dir}/new-line-from-revision.png"))
            .unwrap();
    }
}

#[test]
#[ignore = "times switching project lines on the project OHL_PROJECT (a copy: it is changed)"]
fn line_switch_timing() {
    let Ok(file) = ohl_core::env::var("OHL_PROJECT") else {
        return;
    };
    let mut m = model::load(
        Sources {
            file: file.into(),
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    let names: Vec<String> = m
        .session()
        .unwrap()
        .lines()
        .unwrap()
        .into_iter()
        .map(|l| l.name)
        .collect();
    let other = names
        .iter()
        .find(|n| **n != m.project_line())
        .cloned()
        .unwrap_or_else(|| {
            m.project_op(
                "line",
                ohl_server::Op::CreateLine {
                    name: "timing".into(),
                    from: m.project_line(),
                },
            )
            .unwrap();
            "timing".into()
        });
    let back = m.project_line();
    for k in 0..m.cases.len() {
        m.ensure_solved(k);
    }
    // As before: the libraries read again, every case solved at once.
    let t = std::time::Instant::now();
    m.reload_libraries().unwrap();
    let old = t.elapsed();
    let t = std::time::Instant::now();
    m.switch_line(&other).unwrap();
    let new = t.elapsed();
    let t = std::time::Instant::now();
    m.ensure_solved(0);
    let first = t.elapsed();
    m.switch_line(&back).unwrap();
    println!(
        "{} cases, {} structures: before {old:.2?}; now {new:.2?} to switch, the case shown solved in the background in {first:.2?}",
        m.cases.len(),
        m.line.structures.len()
    );
}

/// Lines › Description of …: a long description edited in a box that
/// scrolls; shown under the working tree; read only for others.
#[test]
fn lines_are_described() {
    use crate::project_lines::LinesWindow;
    let mut m = synthetic_project("described");
    m.project_op(
        "l",
        ohl_server::Op::CreateLine {
            name: "phase 1".into(),
            from: "main".into(),
        },
    )
    .unwrap();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(m, None));
    harness.run();
    harness.get_by_label("Versions").click();
    harness.run();
    harness.get_by_label("Description of main…").click();
    harness.run();
    let long: String = (1..=80)
        .map(|k| format!("Point {k} of the scope.\n"))
        .collect();
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(LinesWindow::Description { line, text }) = &mut r.lines_window else {
            panic!("{:?}", r.lines_window);
        };
        assert_eq!(line, "main");
        *text = long.clone();
    }
    harness.run();
    // The box scrolls: Save stays on the screen.
    let save = harness.get_by_label("Save").rect();
    assert!(save.max.y < 900.0, "{save:?}");
    harness.get_by_label("Save").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.lines_window.is_none(), "{:?}", r.status);
        let lines = r.model.session().unwrap().lines().unwrap();
        assert_eq!(lines[0].description, long.trim_end());
        r.working_tree.open = true;
    }
    harness.run();
    harness.run();
    assert!(harness.query_by_label("Description of main").is_some());
    assert!(
        harness
            .query_by_label_contains("Point 1 of the scope.")
            .is_some()
    );
    if let Ok(dir) = ohl_core::env::var("OHL_SHOTS") {
        harness
            .render()
            .unwrap()
            .save(format!("{dir}/tree-description.png"))
            .unwrap();
    }
    // A designer reads the main line's description, does not edit it.
    {
        let r = harness.state_mut().ready().unwrap();
        if let Ok(e) = &mut r.model.editing {
            e.session
                .set_user("tester", "ben", ohl_session::Role::Designer)
                .unwrap();
            e.user = "ben".into();
        }
        r.working_tree.open = false;
        r.lines_window = Some(LinesWindow::Description {
            line: "main".into(),
            text: long.clone(),
        });
    }
    harness.run();
    assert!(harness.query_by_label_contains("Read only").is_some());
    assert!(harness.query_by_label("Save").is_none());
}

#[test]
#[ignore = "imports OHL_XML with the project plugins of OHL_PLUGINS through the app (local check)"]
fn local_project_import_through_the_app() {
    let (Ok(xml), Ok(plugins), Ok(out)) = (
        ohl_core::env::var("OHL_XML"),
        ohl_core::env::var("OHL_PLUGINS"),
        ohl_core::env::var("OHL_OUT"),
    ) else {
        return;
    };
    let into = PathBuf::from(out);
    std::fs::remove_file(&into).ok();
    let mut app = App::empty(Sources::default(), None);
    app.plugins = crate::plugins::Plugins::load(None, None);
    app.plugins.folders = vec![PathBuf::from(plugins)];
    app.plugins.refresh();
    app.file_action = Some(crate::files::FileAction::ImportProject);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| app);
    h.step();
    h.step();
    h.state_mut()
        .plugins
        .create_project(crate::plugins::ProjectImport {
            path: xml.into(),
            format: None,
            options: Default::default(),
            into: into.clone(),
            user: crate::files::user(),
            clip_in: false,
        });
    let started = std::time::Instant::now();
    loop {
        h.step();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let app = h.state_mut();
        if let Some(w) = &app.import_project
            && let Some(e) = &w.error
        {
            panic!("import refused: {e}");
        }
        if let Some(r) = app.ready() {
            println!(
                "opened: {} structures, {} sections, {} cases, {} solved",
                r.model.line.structures.len(),
                r.model.line.sections.len(),
                r.model.cases.len(),
                r.model.cases.iter().filter(|c| c.ready).count()
            );
            break;
        }
        if let Some(why) = app.failed() {
            panic!("opening failed: {why}");
        }
        assert!(started.elapsed().as_secs() < 240, "not opened in 240 s");
    }
    // Then a TIN into the new project (OHL_TIN): drawn without reopening.
    let Ok(tin) = ohl_core::env::var("OHL_TIN") else {
        return;
    };
    {
        let app = h.state_mut();
        let mut plugins = std::mem::take(&mut app.plugins);
        let r = app.ready().unwrap();
        r.surfaces.show(&r.model);
        let project = r.model.sources.file.clone();
        let user = crate::files::user();
        r.surfaces.act(
            crate::surfaces::Action::Import(tin.into()),
            &mut r.model,
            &mut plugins,
            &user,
            &project,
            &egui::Context::default(),
        );
        app.plugins = plugins;
    }
    let started = std::time::Instant::now();
    loop {
        h.step();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let r = h.state_mut().ready().unwrap();
        if r.model.ground_source.starts_with("surface") {
            println!(
                "ground: {} in {:.0?}; shown: {}",
                r.model.ground_source,
                started.elapsed(),
                r.visible.contains(&crate::scene::Layer::Ground)
            );
            break;
        }
        if let Some((text, true)) = &r.surfaces.status {
            panic!("TIN import: {text}");
        }
        assert!(
            started.elapsed().as_secs() < 900,
            "no ground in 900 s: {:?} {:?}",
            r.surfaces.list,
            r.surfaces.status
        );
    }
    if let Ok(dir) = ohl_core::env::var("OHL_SHOTS") {
        h.state_mut().show_tab(crate::app::Tab::View3d);
        for _ in 0..60 {
            h.step();
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        h.render()
            .unwrap()
            .save(format!("{dir}/tin-3d.png"))
            .unwrap();
    }
}

#[test]
#[ignore = "writes an image of the Terrain surfaces window to OHL_SHOTS (a look by eye)"]
fn surfaces_sections_image() {
    let Ok(dir) = ohl_core::env::var("OHL_SHOTS") else {
        return;
    };
    let mut model = synthetic_project("surfaces-sections");
    model.points = vec![([0, 0, 0], 2), ([1000, 0, 0], 5)];
    model.class_counts = vec![(2, 40), (5, 7)];
    model.cloud = Some("survey.ohl".into());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        r.surfaces.show(&r.model);
    }
    harness.run();
    harness.run();
    harness
        .render()
        .unwrap()
        .save(format!("{dir}/surfaces-sections.png"))
        .unwrap();
}

/// The window closing closes the project: every connection to the file
/// closed, nothing left beside it (no -wal, no -shm).
#[test]
fn closing_the_window_leaves_the_project_file_whole() {
    let model = synthetic_project("exit-clean");
    let file = model.sources.file.clone();
    let side = |end: &str| {
        let mut p = file.clone().into_os_string();
        p.push(end);
        std::path::PathBuf::from(p)
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // An edit: the log has something to write back.
    {
        let r = harness.state_mut().ready().unwrap();
        let p = r.model.line.structure("T2").unwrap().position;
        r.perform(crate::edit::Action::Command {
            what: "T2".into(),
            command: ohl_session::Command::MoveStructure {
                id: "T2".into(),
                position: [p[0] + 1.0, p[1], p[2]],
            },
        });
    }
    assert!(side("-wal").exists());
    eframe::App::on_exit(harness.state_mut());
    assert!(!side("-wal").exists(), "the log written into the file");
    assert!(!side("-shm").exists());
    // And the edit is in the file.
    let s = ohl_session::Session::open(&file).unwrap();
    assert_eq!(s.line().structure("T2").unwrap().position[0], 311.0);
}

/// A project made again at the same place: this user's remembered ground
/// already names the surface imported. Imported, it is drawn at once (it
/// was only after reopening).
#[test]
fn a_surface_imported_under_the_remembered_name_is_drawn_at_once() {
    use crate::surfaces::Action;
    let mut model = synthetic_project("surfaces-remembered");
    assert!(model.ground.is_none());
    model.surface_settings.active = Some("synthetic-survey".into());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.step();
    harness.state_mut().ready().unwrap().view3d.dirty.clear();
    {
        let app = harness.state_mut();
        let mut plugins = std::mem::take(&mut app.plugins);
        let r = app.ready().unwrap();
        r.surfaces.show(&r.model);
        let project = r.model.sources.file.clone();
        r.surfaces.act(
            Action::Import(root().join("examples/synthetic-line/synthetic-survey.csv")),
            &mut r.model,
            &mut plugins,
            "tester",
            &project,
            &egui::Context::default(),
        );
        app.plugins = plugins;
    }
    run_until(&mut harness, |a| {
        a.ready().unwrap().model.ground_source == "surface synthetic-survey"
    });
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.ground.is_some());
    assert!(r.view3d.dirty.contains(&crate::scene::Layer::Ground));
    assert!(r.visible.contains(&crate::scene::Layer::Ground));
}

/// The New alignment tool: from a point of an alignment, through a free
/// point on the ground, back to another point of it — a new alignment
/// whose ends are junctions. Moving a junction point (Edit) moves both;
/// the Edit tool moves a point to another alignment and place. Escape
/// gives a new alignment up.
#[test]
fn alignments_are_laid_out_joined_and_edited() {
    use crate::editors::Form;
    use crate::tools::Tool;
    let model = synthetic_project("new-alignment");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let main = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        r.model.line.alignment_by_id("main").unwrap().points.clone()
    };
    let position = |h: &mut Harness<'_, App>, id: &str| {
        let r = h.state_mut().ready().unwrap();
        r.model.line.alignment_point(id).unwrap().position
    };
    let before = main(&mut harness);
    assert!(before.len() >= 3);
    let p0 = position(&mut harness, &before[0]);
    let free = [p0[0] + 40.0, p0[1] + 60.0, p0[2]];
    let at = |id: &String| Selection::AlignmentPoint { id: id.clone() };
    use_tool(&mut harness, Tool::NewAlignment, at(&before[0]), None);
    // Nothing to snap to: the coordinates are asked (E, N, height).
    use_tool(
        &mut harness,
        Tool::NewAlignment,
        Selection::None,
        Some([free[0], free[1], 0.0]),
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let d = r.coord_dialog.as_mut().expect("the coordinates asked");
        assert_eq!(d.text[0], format!("{:.3}", free[0]));
        d.text[2].clear();
        assert!(d.point().is_err(), "the height is required");
        d.text[2] = format!("{}", free[2]);
        let p = d.point().unwrap();
        r.coord_dialog = None;
        r.place_alignment_point(
            crate::alignment_tools::CoordFor::NewAlignment,
            ohl_line::AlignmentPoint::free("", p),
        );
    }
    use_tool(&mut harness, Tool::NewAlignment, at(&before[2]), None);
    assert_eq!(harness.state_mut().ready().unwrap().view3d.draft.len(), 3);
    // Enter: its route is asked (main: its existing points are main's).
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        let c = r.route_choice.as_ref().expect("the route asked");
        assert_eq!(c.route, "main");
        r.make_new_alignment("main".into(), false);
    }
    harness.run();
    let new = {
        let r = harness.state_mut().ready().unwrap();
        assert!(
            r.new_alignment.is_empty() && r.route_choice.is_none(),
            "{:?}",
            r.status
        );
        let n = r
            .model
            .line
            .alignment_by_id("alignment 1")
            .unwrap_or_else(|| panic!("{:?}", r.status))
            .clone();
        assert_eq!(n.route, "main");
        n.points
    };
    // The existing points shared (junctions), one new point.
    assert_eq!(new.len(), 3);
    assert_eq!(new[0], before[0]);
    assert_eq!(position(&mut harness, &new[1]), free);
    assert_eq!(new[2], before[2]);
    // The junction moved with the Edit tool: one point, on both.
    use_tool(&mut harness, Tool::Edit, at(&before[0]), None);
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Alignment(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        assert_eq!(
            f.orders,
            [("main".to_owned(), 1), ("alignment 1".to_owned(), 1)]
        );
        f.structure = None;
        f.position[0] += 2.0;
    }
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
    }
    assert!((position(&mut harness, &before[0])[0] - (p0[0] + 2.0)).abs() < 1e-9);
    // Point 2 of main also on alignment 1, as its second point: it keeps
    // its place on main.
    use_tool(&mut harness, Tool::Edit, at(&before[1]), None);
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Alignment(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        f.orders.push(("alignment 1".into(), 2));
    }
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        let a1 = &r.model.line.alignment_by_id("alignment 1").unwrap().points;
        assert_eq!(a1.len(), 4);
        assert_eq!(a1[1], before[1]);
    }
    assert_eq!(main(&mut harness), before);
    // An alignment without structures: its profile is drawn.
    {
        let r = harness.state_mut().ready().unwrap();
        r.profile.show = Some(crate::profile::Show::Alignment("alignment 1".into()));
    }
    harness.state_mut().show_tab(crate::app::Tab::Profile);
    harness.run_steps(3);
    // Escape gives a new alignment up; the tool stays held.
    use_tool(&mut harness, Tool::NewAlignment, at(&before[0]), None);
    press(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    let r = harness.state_mut().ready().unwrap();
    assert!(r.new_alignment.is_empty() && r.view3d.draft.is_empty());
    assert_eq!(r.hotbar.tool(), Tool::NewAlignment);
}

/// Points placed on no alignment (New alignment point tool), then picked
/// one after another into a new alignment; with no route yet, one is
/// offered first.
#[test]
fn free_points_are_picked_into_an_alignment_and_a_route_is_offered() {
    use crate::alignment_tools::{CoordFor, RouteChoice};
    use crate::tools::Tool;
    let model = synthetic_project_with("free-points", |line| {
        line.routes.clear();
        line.alignments.clear();
        line.alignment_points.clear();
        for s in &mut line.structures {
            s.routes.clear();
        }
    });
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // Two free points (Add alignment point, not connected): one under T1
    // (tied, its coordinates given), one typed.
    use_tool(
        &mut harness,
        Tool::AddAlignment,
        Selection::Structure("T1".into()),
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let d = r.coord_dialog.as_ref().expect("its window, always");
        assert_eq!(d.purpose, CoordFor::AddAlignment);
        assert_eq!(d.structure.as_deref(), Some("T1"));
    }
    harness.run();
    harness.get_by_label("Add the point").click();
    harness.run();
    use_tool(
        &mut harness,
        Tool::AddAlignment,
        Selection::None,
        Some([10.0, 20.0, 0.0]),
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let d = r.coord_dialog.as_mut().expect("asked");
        d.text[2] = "500".into();
    }
    harness.run();
    harness.get_by_label("Add the point").click();
    harness.run();
    let ids: Vec<String> = {
        let r = harness.state_mut().ready().unwrap();
        let l = &r.model.line;
        assert_eq!(l.alignment_points.len(), 2, "{:?}", r.status);
        assert!(l.alignments.is_empty());
        assert_eq!(l.alignment_points[0].structure.as_deref(), Some("T1"));
        l.alignment_points.iter().map(|p| p.id.clone()).collect()
    };
    // Picked one after another, Enter: no route yet — one is offered.
    for id in &ids {
        use_tool(
            &mut harness,
            Tool::NewAlignment,
            Selection::AlignmentPoint { id: id.clone() },
            None,
        );
    }
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        let c: &RouteChoice = r.route_choice.as_ref().expect("asked");
        assert!(c.route.is_empty());
        assert_eq!(c.new_route, "route 1", "a new route proposed");
    }
    harness.run();
    harness
        .get_by_label("Create the route and the alignment")
        .click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    let l = &r.model.line;
    assert_eq!(l.routes.len(), 1, "{:?}", r.status);
    let a = &l.alignments[0];
    assert_eq!(a.route, "route 1");
    assert_eq!(a.points, ids);
    assert_eq!(l.routes_of("T1"), ["route 1"]);
}

/// Edit alignment: a stretch between two points replaced by new points
/// (those between stay, on no alignment); from an end, Enter adds them
/// there; from a point in the middle, Enter offers a new alignment.
#[test]
fn the_edit_alignment_tool_reconnects_extends_and_branches() {
    use crate::tools::Tool;
    let model = synthetic_project_with("reroute", |l| {
        l.alignment_points.clear();
        l.alignments = vec![ohl_line::Alignment::new("main", "main", Vec::new())];
        l.set_alignment_points(
            "main",
            [0.0, 100.0, 200.0, 300.0].map(|x| ([x, -50.0, 500.0], None)),
        );
    });
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let main = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        r.model.line.alignment_by_id("main").unwrap().points.clone()
    };
    let at = |id: &String| Selection::AlignmentPoint { id: id.clone() };
    let typed = |h: &mut Harness<'_, App>, p: [f64; 3]| {
        let r = h.state_mut().ready().unwrap();
        r.coord_dialog = None;
        r.place_alignment_point(
            crate::alignment_tools::CoordFor::Reroute,
            ohl_line::AlignmentPoint::free("", p),
        );
    };
    let before = main(&mut harness);
    // P1 → a new point → P4: P2 and P3 replaced (they stay).
    use_tool(&mut harness, Tool::EditAlignment, at(&before[0]), None);
    typed(&mut harness, [150.0, -80.0, 500.0]);
    use_tool(&mut harness, Tool::EditAlignment, at(&before[3]), None);
    let after = main(&mut harness);
    assert_eq!(
        after.len(),
        3,
        "{:?}",
        harness.state_mut().ready().unwrap().status
    );
    assert_eq!((&after[0], &after[2]), (&before[0], &before[3]));
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.new_alignment.is_empty() && !r.reroute);
        let l = &r.model.line;
        assert_eq!(
            l.alignment_point(&after[1]).unwrap().position,
            [150.0, -80.0, 500.0]
        );
        assert!(l.alignment_point(&before[1]).is_some(), "kept");
        assert!(l.point_alignments(&before[1]).is_empty());
    }
    // From the last point, Enter: added at the end.
    use_tool(&mut harness, Tool::EditAlignment, at(&before[3]), None);
    typed(&mut harness, [400.0, -50.0, 500.0]);
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    assert_eq!(main(&mut harness).len(), 4);
    // From the first point, Enter: added at the start.
    use_tool(&mut harness, Tool::EditAlignment, at(&before[0]), None);
    typed(&mut harness, [-100.0, -50.0, 500.0]);
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    let now = main(&mut harness);
    assert_eq!(now.len(), 5);
    assert_eq!(now[1], before[0]);
    // From a point in the middle, Enter: a new alignment is offered (an
    // alignment has two ends only), starting at that point.
    let middle = now[2].clone();
    use_tool(&mut harness, Tool::EditAlignment, at(&middle), None);
    typed(&mut harness, [150.0, 100.0, 500.0]);
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        let c = r.route_choice.as_ref().expect("a new alignment offered");
        assert_eq!(c.points[0].id, middle);
        assert_eq!(c.route, "main");
        r.make_new_alignment("main".into(), false);
        let l = &r.model.line;
        assert_eq!(l.alignments.len(), 2, "{:?}", r.status);
        assert_eq!(l.point_alignments(&middle).len(), 2, "shared");
    }
    assert_eq!(main(&mut harness), now, "main unchanged");
}

/// Tools › New pylon from a template…: a suspension and a strain type
/// built from a template (sets: the slots, + 20 back, + 40 ahead); Add
/// structure on an alignment point, the alignment's other points ticked
/// (what Place on alignment points… did): one command puts them on every
/// point, along the line, a row's own type at the angle point; built
/// again, the structures follow; Put it on the line again turns a
/// structure back.
#[test]
fn pylons_are_built_from_templates_and_placed_on_the_alignment() {
    use crate::editors::Form;
    use crate::pylon_builder::PylonBuilder;
    use crate::tools::Tool;
    use ohl_structures::templates::Role;
    let model = synthetic_project_with("pylon-builder", |l| {
        l.alignment_points.clear();
        l.alignments = vec![ohl_line::Alignment::new("main", "main", Vec::new())];
        l.set_alignment_points(
            "main",
            [[0.0, -80.0], [300.0, -80.0], [600.0, -20.0], [900.0, -20.0]]
                .map(|p| ([p[0], p[1], 500.0], None)),
        );
    });
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // The suspension type, then the strain one (the toggle).
    {
        let r = harness.state_mut().ready().unwrap();
        let mut b = PylonBuilder::new(&r.model);
        let k = (0..10)
            .find(|&k| {
                b.choose(&r.model, k);
                b.current().is_some_and(|t| t.id == "danube-2c")
            })
            .expect("the shipped template");
        b.choose(&r.model, k);
        assert_eq!(b.id, "danube-2c-susp");
        r.pylon_builder = Some(b);
    }
    harness.run();
    harness.get_by_label("Save the type").click();
    harness.run();
    harness.get_by_label("Strain").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let b = r.pylon_builder.as_mut().unwrap();
        assert_eq!(b.role, Role::Strain);
        b.rebuild = None;
        b.id = "danube-2c-strain".into();
    }
    harness.run();
    harness.get_by_label("Save the type").click();
    harness.run();
    let sets = |h: &mut Harness<'_, App>, id: &str| {
        let r = h.state_mut().ready().unwrap();
        let types = &r.model.types.as_ref().expect("types read").0;
        types
            .get(id)
            .unwrap_or_else(|| panic!("{id}: {:?}", r.pylon_builder.as_ref().unwrap().message))
            .sets()
    };
    assert_eq!(sets(&mut harness, "danube-2c-susp"), [1, 2, 3, 4, 5, 6, 19]);
    assert_eq!(
        sets(&mut harness, "danube-2c-strain"),
        [21, 22, 23, 24, 25, 26, 39, 41, 42, 43, 44, 45, 46, 59]
    );
    // On every point: suspension, the angle point strain.
    let before = harness
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .structures
        .len();
    // Add structure on the alignment's first point; its other points
    // ticked in the window (All), the angle point its own type.
    let first = {
        let r = harness.state_mut().ready().unwrap();
        r.pylon_builder = None;
        let l = &r.model.line;
        l.points_of(l.alignment_by_id("main").unwrap())[0]
            .id
            .clone()
    };
    use_tool(
        &mut harness,
        Tool::AddStructure,
        Selection::AlignmentPoint { id: first },
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Structure(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        assert_eq!(f.on_alignment, Some(("main".into(), 0)), "on the point");
        f.structure_type = Some("danube-2c-susp".into());
    }
    harness.run();
    harness
        .get_by_label("Also on other points of this alignment (0 ticked)")
        .click();
    harness.run();
    harness.get_by_label("All").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Structure(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        let more = f.more.as_mut().expect("the other points");
        assert_eq!(
            more.rows.iter().map(|r| r.index).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        more.rows[1].structure_type = Some("danube-2c-strain".into());
    }
    harness.run();
    harness.get_by_label("Also on other points of this alignment (3 ticked)");
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    harness.run();
    let placed: Vec<ohl_line::Structure> = {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        let l = &r.model.line;
        assert_eq!(l.structures.len(), before + 4);
        let pts = l.points_of(l.alignment_by_id("main").unwrap());
        pts.iter()
            .map(|p| {
                l.structure(p.structure.as_deref().expect("tied"))
                    .unwrap()
                    .clone()
            })
            .collect()
    };
    assert_eq!(
        placed[2].structure_type.as_deref(),
        Some("danube-2c-strain")
    );
    assert_eq!(placed[1].structure_type.as_deref(), Some("danube-2c-susp"));
    // Along the line: the first one's right is south (heading east).
    assert!(
        (placed[0].orientation_deg + 90.0).abs() < 1e-6,
        "{}",
        placed[0].orientation_deg
    );
    // One command: one undo takes all four.
    {
        let r = harness.state_mut().ready().unwrap();
        r.perform(crate::edit::Action::Undo);
        assert_eq!(r.model.line.structures.len(), before);
        r.perform(crate::edit::Action::Redo);
        assert_eq!(r.model.line.structures.len(), before + 4);
    }
    // Built again with a higher lower crossarm: its structures follow.
    let z = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        let s = r.model.line.structure(&placed[1].id).unwrap().clone();
        let ty = r
            .model
            .types
            .as_ref()
            .unwrap()
            .0
            .get("danube-2c-susp")
            .unwrap();
        ty.point_local("A-C1-L2", None).unwrap()[2] + s.position[2]
    };
    let low = z(&mut harness);
    {
        let r = harness.state_mut().ready().unwrap();
        let mut b = PylonBuilder::new(&r.model);
        b.rebuild(&r.model, "danube-2c-susp").unwrap();
        *b.values.dimensions.get_mut("lower_level").unwrap() += 2.0;
        r.pylon_builder = Some(b);
    }
    harness.run();
    harness
        .get_by_label("Build it again (keep what was added)")
        .click();
    harness.run();
    assert!((z(&mut harness) - (low + 2.0)).abs() < 1e-6);
    // Turned by hand, then put on the line again.
    {
        let r = harness.state_mut().ready().unwrap();
        let mut f = crate::editors::StructureForm::of(&r.model, &placed[0].id).unwrap();
        assert!(f.on_alignment.is_some(), "it knows its point");
        f.orientation_deg += 30.0;
        assert!(f.put_on_line(&r.model));
        assert!((f.orientation_deg - placed[0].orientation_deg).abs() < 1e-6);
    }
}

/// Add structure with no structure type in the library: the window opens
/// (also with no ground under the click), says so and offers New pylon
/// from a template.
#[test]
fn add_structure_without_types_offers_a_new_pylon() {
    use crate::tools::Tool;
    let model = synthetic_project_with("no-types", |l| {
        l.structures.clear();
        l.sections.clear();
        l.jumpers.clear();
        for p in &mut l.alignment_points {
            p.structure = None;
        }
    });
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // No ground under the click: it opens anyway.
    use_tool(&mut harness, Tool::AddStructure, Selection::None, None);
    assert!(harness.state_mut().ready().unwrap().editor.is_some());
    harness.run();
    harness.get_by_label_contains("No structure type in the library yet");
    harness
        .query_all_by_label("New structure from a template…")
        .last()
        .unwrap()
        .click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.pylon_builder.is_some() && r.editor.is_some(), "both open");
}

/// A selected structure's wire points are drawn where its strings hold
/// them in the case shown (a suspension string swung by the wind); one no
/// wire is strung to stays where the structure gives it, in still air.
#[test]
fn selected_wire_points_follow_their_strings() {
    let model = solved_all(synthetic());
    let line = &model.line;
    // A case with wind: the suspension strings swing.
    let windy = model
        .cases
        .iter()
        .position(|c| c.case.wind_pressure.si() > 0.0)
        .expect("a case with wind");
    let pieces = crate::selection::string_pieces(&model, windy);
    let strung: std::collections::BTreeSet<(String, u32)> = line
        .sections
        .iter()
        .flat_map(|s| s.points.iter().map(|p| (p.structure.clone(), p.set)))
        .collect();
    let mut moved = 0;
    for st in &line.structures {
        for w in st.wires.iter().filter(|w| w.set.is_some()) {
            let drawn = crate::scene::wire_drawn(st, w, &pieces);
            let held = st.strings.iter().any(|x| x.wire == w.label);
            if !held || !strung.contains(&(st.id.clone(), w.set.unwrap())) {
                assert_eq!(drawn, [w.position], "{} {}", st.id, w.label);
            } else if drawn.iter().any(|p| {
                (0..3)
                    .map(|k| (p[k] - w.position[k]).powi(2))
                    .sum::<f64>()
                    .sqrt()
                    > 0.05
            }) {
                moved += 1;
            }
        }
    }
    assert!(
        moved > 0,
        "some wire point drawn where its string swings to"
    );
    // A wire point no string reaches, not strung: as given.
    let mut st = line.structures[0].clone();
    st.wires.push(ohl_line::WirePoint {
        description: String::new(),
        label: "spare".into(),
        position: [1.0, 2.0, 3.0],
        set: Some(77),
        weight: 0.0,
    });
    let w = st.wires.last().unwrap().clone();
    assert_eq!(
        crate::scene::wire_drawn(&st, &w, &pieces),
        [[1.0, 2.0, 3.0]]
    );
}

/// The three tables: Routes, Alignments (each on one route, in order) and
/// Alignment points (a row per point and alignment, with its place there).
#[test]
fn routes_alignments_and_points_are_edited_in_their_tables() {
    use crate::tables::TableKind;
    let model = synthetic_project("table-new-alignment");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // A new alignment needs its route.
    {
        let r = harness.state_mut().ready().unwrap();
        let mut row = TableKind::Alignments.new_row(&r.model, 0, &[]);
        assert_eq!(row[0], "alignment 1");
        row[0] = "spur".into();
        row[2] = String::new();
        r.tables
            .entry(TableKind::Alignments)
            .or_default()
            .added
            .push(row);
    }
    let e = apply_table(&mut harness, TableKind::Alignments, false).unwrap_err();
    assert!(e.contains("choose its route"), "{e}");
    {
        let r = harness.state_mut().ready().unwrap();
        let page = r.tables.get_mut(&TableKind::Alignments).unwrap();
        page.added[0][2] = "branch".into();
        // Between main and the branch.
        page.added[0][3] = "1.5".into();
    }
    apply_table(&mut harness, TableKind::Alignments, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let ids: Vec<&str> = r
            .model
            .line
            .alignments
            .iter()
            .map(|a| a.id.as_str())
            .collect();
        assert_eq!(ids, ["main", "spur", "branch"]);
        assert_eq!(
            r.model.line.alignment_by_id("spur").unwrap().route,
            "branch"
        );
    }
    // Reordered: the branch first.
    set_cell(&mut harness, TableKind::Alignments, "branch", "#", "0.5");
    apply_table(&mut harness, TableKind::Alignments, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let ids: Vec<&str> = r
            .model
            .line
            .alignments
            .iter()
            .map(|a| a.id.as_str())
            .collect();
        assert_eq!(ids, ["branch", "main", "spur"]);
    }
    // Points: main's third point on the spur too (shared), and a new one.
    let (third, rows_before) = {
        let r = harness.state_mut().ready().unwrap();
        let third = r.model.line.alignment_by_id("main").unwrap().points[2].clone();
        let mut a = TableKind::AlignmentPoints.new_row(&r.model, 0, &[]);
        a[0] = "spur".into();
        a[1] = "1".into();
        a[2] = third.clone();
        let mut b = TableKind::AlignmentPoints.new_row(&r.model, 0, &[a.clone()]);
        b[0] = "spur".into();
        b[1] = "2".into();
        b[3] = "1300".into();
        b[4] = "150".into();
        b[5] = "505".into();
        let n = TableKind::AlignmentPoints.rows(&r.model, 0).len();
        let page = r.tables.entry(TableKind::AlignmentPoints).or_default();
        page.added.push(a);
        page.added.push(b);
        (third, n)
    };
    apply_table(&mut harness, TableKind::AlignmentPoints, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let l = &r.model.line;
        let spur = &l.alignment_by_id("spur").unwrap().points;
        assert_eq!(spur.len(), 2);
        assert_eq!(spur[0], third);
        assert_eq!(
            l.alignment_point(&spur[1]).unwrap().position,
            [1300.0, 150.0, 505.0]
        );
        // The shared point: a row on each alignment, each with its place.
        let rows = TableKind::AlignmentPoints.rows(&r.model, 0);
        assert_eq!(rows.len(), rows_before + 2);
        let on_spur = rows
            .iter()
            .find(|x| x.key == format!("spur\t{third}"))
            .unwrap();
        assert_eq!(on_spur.cells[1], "1");
        assert_eq!(on_spur.cells[7], "main #3");
    }
    // Taken off main (its alignment emptied): it stays on the spur.
    set_cell(
        &mut harness,
        TableKind::AlignmentPoints,
        &format!("main\t{third}"),
        "alignment",
        "",
    );
    apply_table(&mut harness, TableKind::AlignmentPoints, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.model.line.point_alignments(&third), ["spur"]);
    }
    // A row deleted: the point off every alignment (asked when shared).
    {
        let r = harness.state_mut().ready().unwrap();
        r.tables
            .entry(TableKind::AlignmentPoints)
            .or_default()
            .deleted
            .insert(format!("spur\t{third}"));
    }
    apply_table(&mut harness, TableKind::AlignmentPoints, false).unwrap();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.line.alignment_point(&third).is_none());
    assert_eq!(
        r.model.line.alignment_by_id("spur").unwrap().points.len(),
        1
    );
}

/// Sections › Electrical circuits: one row per conductor path, counted
/// from the connections; each end is a start or an end — ticked in
/// "starts" (several feeds possible), swapped by ⇄ — at least one of each;
/// its jumpers point along it; back to the line's direction, nothing kept.
#[test]
fn circuits_are_listed_and_turned() {
    use crate::tables::TableKind;
    let model = synthetic_project("circuits");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let row = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        let rows = TableKind::ConductorPaths.rows(&r.model, 0);
        rows.into_iter()
            .find(|x| x.cells[6].contains("S3"))
            .unwrap()
    };
    let (key, start, ends) = {
        let r = harness.state_mut().ready().unwrap();
        let rows = TableKind::ConductorPaths.rows(&r.model, 0);
        assert_eq!(
            rows.len(),
            ohl_line::electrical::conductor_paths(&r.model.line).len()
        );
        assert!(TableKind::ConductorPaths.fixed_rows());
        let x = rows.iter().find(|x| x.cells[6].contains("S3")).unwrap();
        assert!(x.cells[4].starts_with("T1 "), "{}", x.cells[4]);
        assert_eq!(x.cells[5].split(';').count(), 2, "two ends: {}", x.cells[5]);
        (x.key.clone(), x.cells[4].clone(), x.cells[5].clone())
    };
    // Reversed (⇄ puts its ends in "starts"): two starts, one end.
    set_cell(
        &mut harness,
        TableKind::ConductorPaths,
        &key,
        "starts",
        &ends,
    );
    apply_table(&mut harness, TableKind::ConductorPaths, false).unwrap();
    {
        let x = row(&mut harness);
        assert_eq!(x.cells[5], start);
        let r = harness.state_mut().ready().unwrap();
        let line = &r.model.line;
        assert_eq!(line.circuit_starts.len(), 2);
        let c = ohl_line::electrical::circuits(line)
            .into_iter()
            .find(|c| c.sections.contains(&"S3".to_owned()))
            .unwrap();
        let at = |p| c.order.iter().position(|q| q == p);
        assert!(
            at(&line.jumpers[0].from) < at(&line.jumpers[0].to),
            "along it"
        );
    }
    // Every end a start: refused.
    let all = format!("{start}; {ends}");
    set_cell(
        &mut harness,
        TableKind::ConductorPaths,
        &key,
        "starts",
        &all,
    );
    let err = apply_table(&mut harness, TableKind::ConductorPaths, false).unwrap_err();
    assert!(err.contains("at least one end"), "{err}");
    harness
        .state_mut()
        .ready()
        .unwrap()
        .tables
        .remove(&TableKind::ConductorPaths);
    // Back along the line: nothing kept.
    set_cell(
        &mut harness,
        TableKind::ConductorPaths,
        &key,
        "starts",
        &start,
    );
    apply_table(&mut harness, TableKind::ConductorPaths, false).unwrap();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.line.circuit_starts.is_empty());
}

/// The Circuits table (2026-10-21): the example's names read into
/// circuits; renaming (the sections follow), adding an empty one (noted),
/// moving a conductor path between circuits, deleting (confirmed) — each
/// Apply one command, undone at once. Refused: a path still in another
/// circuit, another circuit's name (no merge), two paths of one phase not
/// connected (from the table and from Properties' Assign to circuit).
#[test]
fn circuits_are_defined_in_their_table() {
    use crate::tables::TableKind;
    // The branch untapped and of the system: S1 L1, S2 L2, S3 L1.
    let model = synthetic_project_with("circuit-list", |l| {
        l.jumpers.clear();
        l.sections[2].system = Some("ex220".into());
    });
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let rows = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        TableKind::Circuits.rows(&r.model, 0)
    };
    let names = |h: &mut Harness<'_, App>| -> Vec<String> {
        let r = h.state_mut().ready().unwrap();
        r.model
            .line
            .sections
            .iter()
            .map(|s| s.circuit_name.clone())
            .collect()
    };
    let col = |n: &str| TableKind::Circuits.column(n);
    {
        let rows = rows(&mut harness);
        let cells: Vec<(&str, &str, &str)> = rows
            .iter()
            .map(|r| {
                (
                    r.cells[0].as_str(),
                    r.cells[col("members")].as_str(),
                    r.cells[col("notes")].as_str(),
                )
            })
            .collect();
        assert_eq!(
            cells,
            [("Example 1", "S1 S2", ""), ("Branch", "S3", "")],
            "migrated: one circuit per name, the same members"
        );
    }
    // Renamed: its sections follow; one undo takes it back.
    set_cell(
        &mut harness,
        TableKind::Circuits,
        "Example 1",
        "name",
        "North",
    );
    apply_table(&mut harness, TableKind::Circuits, false).unwrap();
    assert_eq!(names(&mut harness), ["North", "North", "Branch"]);
    harness.state_mut().ready().unwrap().model.undo().unwrap();
    assert_eq!(names(&mut harness), ["Example 1", "Example 1", "Branch"]);
    // Another circuit's name: refused (circuits are not merged).
    set_cell(
        &mut harness,
        TableKind::Circuits,
        "Branch",
        "name",
        "Example 1",
    );
    let err = apply_table(&mut harness, TableKind::Circuits, false).unwrap_err();
    assert!(err.contains("not merged"), "{err}");
    harness
        .state_mut()
        .ready()
        .unwrap()
        .tables
        .remove(&TableKind::Circuits);
    // Added, empty: allowed, noted.
    {
        let r = harness.state_mut().ready().unwrap();
        let mut row = TableKind::Circuits.new_row(&r.model, 0, &[]);
        assert_eq!(row[0], "C1");
        row[1] = "South".into();
        row[col("description")] = "invented".into();
        r.tables
            .entry(TableKind::Circuits)
            .or_default()
            .added
            .push(row);
    }
    apply_table(&mut harness, TableKind::Circuits, false).unwrap();
    {
        let rows = rows(&mut harness);
        let south = rows.iter().find(|r| r.key == "C1").unwrap();
        assert!(south.cells[col("notes")].contains("no conductor path"));
        assert_eq!(south.cells[col("description")], "invented");
    }
    // S3 into South while still in Branch: refused.
    set_cell(&mut harness, TableKind::Circuits, "C1", "members", "S3");
    let err = apply_table(&mut harness, TableKind::Circuits, false).unwrap_err();
    assert!(err.contains("is in circuit \"Branch\""), "{err}");
    // Taken out of Branch in the same Apply: moved, one command.
    set_cell(&mut harness, TableKind::Circuits, "Branch", "members", "");
    apply_table(&mut harness, TableKind::Circuits, false).unwrap();
    assert_eq!(names(&mut harness), ["Example 1", "Example 1", "South"]);
    harness.state_mut().ready().unwrap().model.undo().unwrap();
    assert_eq!(names(&mut harness), ["Example 1", "Example 1", "Branch"]);
    // S3 (L1) into Example 1, whose L1 is S1, not connected: refused.
    set_cell(&mut harness, TableKind::Circuits, "Branch", "members", "");
    set_cell(
        &mut harness,
        TableKind::Circuits,
        "Example 1",
        "members",
        "S1 S2 S3",
    );
    let err = apply_table(&mut harness, TableKind::Circuits, false).unwrap_err();
    assert!(err.contains("not connected"), "{err}");
    harness
        .state_mut()
        .ready()
        .unwrap()
        .tables
        .remove(&TableKind::Circuits);
    // The same from Properties: refused; into South (empty): moved.
    {
        let r = harness.state_mut().ready().unwrap();
        let err = crate::properties::assign_circuit_action(&r.model, "S3", Some("Example 1"))
            .err()
            .unwrap();
        assert!(err.contains("not connected"), "{err}");
        let a = crate::properties::assign_circuit_action(&r.model, "S3", Some("C1")).unwrap();
        r.perform(a);
    }
    assert_eq!(names(&mut harness), ["Example 1", "Example 1", "South"]);
    // Deleted: its paths in no circuit; listed to confirm first.
    harness
        .state_mut()
        .ready()
        .unwrap()
        .tables
        .entry(TableKind::Circuits)
        .or_default()
        .deleted
        .insert("C1".into());
    assert!(apply_table(&mut harness, TableKind::Circuits, false).is_err());
    apply_table(&mut harness, TableKind::Circuits, true).unwrap();
    assert_eq!(names(&mut harness), ["Example 1", "Example 1", ""]);
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.line.circuit("C1").is_none());
    r.model.undo().unwrap();
    assert!(r.model.line.circuit("C1").is_some());
    assert_eq!(r.model.line.sections[2].circuit_name, "South");
}

/// The Edit tool on a jumper opens its window, as on a span (the mouse is
/// freed); Apply changes the jumper in one command.
#[test]
fn jumpers_are_edited_in_their_window() {
    use crate::editors::Form;
    use crate::tools::Tool;
    let model = synthetic_project("jumper-window");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.state_mut().ready().unwrap().view3d.captured = true;
    use_tool(
        &mut harness,
        Tool::Edit,
        Selection::Jumper("J-T3-branch".into()),
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Jumper(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        assert!(!r.view3d.captured, "the window frees the mouse");
        assert_eq!(f.jumper.sag, Some(1.5));
        f.jumper.sag = Some(2.0);
    }
    harness.run();
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        assert_eq!(r.model.line.jumpers[0].sag, Some(2.0));
    }
    // Rigid: no length nor sag.
    use_tool(
        &mut harness,
        Tool::Edit,
        Selection::Jumper("J-T3-branch".into()),
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Jumper(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        f.jumper.rigid = true;
        f.jumper.sag = None;
    }
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    let r = harness.state_mut().ready().unwrap();
    assert!(r.editor.is_none(), "{:?}", r.status);
    assert!(r.model.line.jumpers[0].rigid && r.model.line.jumpers[0].sag.is_none());
}

/// A section's phase, circuit name or system changed — in its editor, in
/// the Sections table, or for the circuit in the Electrical circuits table
/// — goes on all the sections of its circuit, listed first to confirm.
#[test]
fn a_circuit_keeps_one_name_and_phase() {
    use crate::editors::Form;
    use crate::tables::TableKind;
    use crate::tools::Tool;
    let model = synthetic_project("circuit-phase");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let phase = |h: &mut Harness<'_, App>, id: &str| {
        let r = h.state_mut().ready().unwrap();
        let s = r.model.line.sections.iter().find(|s| s.id == id).unwrap();
        (s.phase_label.clone(), s.circuit_name.clone())
    };
    // S1 and the branch S3 are one circuit (the jumper at T3).
    // The editor: the first Apply shows S3, the second applies.
    use_tool(
        &mut harness,
        Tool::Edit,
        Selection::Span {
            section: "S1".into(),
            span: 0,
        },
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Section(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        f.phase_label = "L3".into();
    }
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_some(), "not applied yet");
        let warned = r.circuit_warned.clone().unwrap();
        assert_eq!(warned, vec![("S3".to_owned(), vec!["phase"])]);
    }
    assert_eq!(phase(&mut harness, "S3").0, "L1");
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        assert!(r.circuit_warned.is_none());
    }
    assert_eq!(phase(&mut harness, "S1").0, "L3");
    assert_eq!(phase(&mut harness, "S3").0, "L3");
    // The Sections table: confirmed first, then both.
    set_cell(&mut harness, TableKind::Sections, "S3", "phase", "L2");
    assert_eq!(
        apply_table(&mut harness, TableKind::Sections, false).unwrap_err(),
        "needs a confirmation"
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let confirm = r.tables[&TableKind::Sections].confirm.clone().unwrap();
        assert!(
            confirm.iter().any(|t| t.contains("S1 (phase)")),
            "{confirm:?}"
        );
    }
    apply_table(&mut harness, TableKind::Sections, true).unwrap();
    assert_eq!(phase(&mut harness, "S1").0, "L2");
    assert_eq!(phase(&mut harness, "S3").0, "L2");
    // The circuits table: its names differ ("mixed"); one name for all.
    let key = {
        let r = harness.state_mut().ready().unwrap();
        let rows = TableKind::ConductorPaths.rows(&r.model, 0);
        let row = rows.iter().find(|x| x.cells[6].contains("S3")).unwrap();
        assert!(row.cells[1].starts_with("mixed:"), "{}", row.cells[1]);
        assert_eq!(row.cells[2], "L2");
        row.key.clone()
    };
    set_cell(
        &mut harness,
        TableKind::ConductorPaths,
        &key,
        "circuit",
        "Main",
    );
    assert!(apply_table(&mut harness, TableKind::ConductorPaths, false).is_err());
    apply_table(&mut harness, TableKind::ConductorPaths, true).unwrap();
    assert_eq!(phase(&mut harness, "S1").1, "Main");
    assert_eq!(phase(&mut harness, "S3").1, "Main");
}

/// A project whose library has a crafted name ("../…": written by another
/// program, or a crafted copy): exporting the libraries refuses it, and
/// nothing is written outside the folder chosen.
#[test]
fn exported_libraries_stay_in_their_folder() {
    let mut model = synthetic_project("export-names");
    let file = model.sources.file.clone();
    // Put as an object directly (the session's own checks bypassed).
    model
        .editing
        .as_mut()
        .unwrap()
        .session
        .apply(
            "tester",
            ohl_session::Command::Put {
                kind: ohl_session::Kind::Library,
                id: "cables/../escaped.ohlw".into(),
                value: serde_json::json!({
                    "id": "cables/../escaped.ohlw",
                    "kind": "cables",
                    "name": "../escaped.ohlw",
                    "text": "x"
                }),
                ord: None,
            },
        )
        .unwrap();
    let dir = file.parent().unwrap().join("export");
    let err = model.export_libraries(&dir).unwrap_err();
    assert!(err.contains("not a plain file name"), "{err}");
    assert!(
        !dir.join("escaped.ohlw").exists() && !dir.parent().unwrap().join("escaped.ohlw").exists()
    );
}

/// In the 3D view in first person: Escape while stringing cancels the
/// wire and keeps the mouse; a second Escape frees it.
#[test]
fn escape_twice_in_first_person() {
    use crate::tools::Tool;
    if ohl_render::headless_device().is_err() {
        assert!(
            ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
            "no GPU"
        );
        return;
    }
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .wgpu()
        .build_eframe(|cc| {
            App::with_model(synthetic_project("escape-fp"), cc.wgpu_render_state.clone())
        });
    harness.run();
    harness.state_mut().ready().unwrap().view3d.controls = crate::controls::Controls::presets()
        .into_iter()
        .find(|c| c.name == "Minecraft creative")
        .unwrap();
    harness.state_mut().show_tab(crate::app::Tab::View3d);
    harness.run();
    use_tool(&mut harness, Tool::StringWire, string_at("T2", 1), None);
    harness.state_mut().ready().unwrap().view3d.captured = true;
    harness.run();
    assert!(
        harness.state_mut().ready().unwrap().view3d.captured,
        "captured before Escape"
    );
    assert!(
        !harness.state_mut().ready().unwrap().chain.is_empty(),
        "a point picked"
    );
    press(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.chain.is_empty());
        assert_eq!(r.hotbar.tool(), Tool::StringWire);
        assert!(r.view3d.captured, "the first Escape keeps the mouse");
    }
    press(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(
        !harness.state_mut().ready().unwrap().view3d.captured,
        "the second frees it"
    );
}

/// Escape cancels the wire being strung, at any point (points picked with
/// the mouse captured, or the section window with a field being typed in):
/// the points go, String wire stays held and the view keeps the mouse; a
/// second Escape frees it. The same for a new alignment.
#[test]
fn escape_cancels_the_operation_not_the_tool() {
    use crate::editors::Form;
    use crate::tools::Tool;
    let model = synthetic_project("escape-string-wire");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let escape = |h: &mut Harness<'_, App>| press(h, egui::Key::Escape, egui::Modifiers::NONE);
    let cancelled = |h: &mut Harness<'_, App>, tool: Tool| {
        let r = h.state_mut().ready().unwrap();
        assert!(r.chain.is_empty() && r.new_alignment.is_empty() && r.view3d.draft.is_empty());
        assert!(r.editor.is_none());
        assert_eq!(r.hotbar.tool(), tool);
    };
    // Points picked, the mouse captured: one Escape cancels, the mouse stays.
    use_tool(&mut harness, Tool::StringWire, string_at("T2", 1), None);
    use_tool(&mut harness, Tool::StringWire, string_at("T3", 1), None);
    harness.state_mut().ready().unwrap().view3d.captured = true;
    escape(&mut harness);
    cancelled(&mut harness, Tool::StringWire);
    assert!(harness.state_mut().ready().unwrap().view3d.captured);
    // The section window, a field focused: closed, String wire held.
    use_tool(&mut harness, Tool::StringWire, string_at("T2", 1), None);
    use_tool(&mut harness, Tool::StringWire, string_at("T3", 1), None);
    harness.state_mut().ready().unwrap().view3d.captured = false;
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(matches!(r.editor, Some(Form::Section(_))), "{:?}", r.editor);
    }
    harness
        .ctx
        .memory_mut(|m| m.request_focus(egui::Id::new("escape-test-field")));
    escape(&mut harness);
    cancelled(&mut harness, Tool::StringWire);
    // A new alignment: the same.
    let id = harness.state_mut().ready().unwrap().model.line.structures[0]
        .id
        .clone();
    use_tool(
        &mut harness,
        Tool::NewAlignment,
        Selection::Structure(id),
        None,
    );
    harness.state_mut().ready().unwrap().view3d.captured = true;
    escape(&mut harness);
    cancelled(&mut harness, Tool::NewAlignment);
    assert!(harness.state_mut().ready().unwrap().view3d.captured);
}

/// Several routes, a short spur listed first: the profile opens on the
/// route with most structures; a route chosen in the list stays.
#[test]
fn the_profile_opens_on_the_main_run() {
    // The branch (3 structures) is listed before main (5).
    let mut model = synthetic_project("profile-main-run");
    model.line.routes.reverse();
    model.line.alignments.reverse();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.state_mut().show_tab(crate::app::Tab::Profile);
    harness.run_steps(2);
    assert_eq!(
        harness.state_mut().ready().unwrap().profile.show,
        Some(crate::profile::Show::Route("main".into()))
    );
}

/// The plan view of a local project, drawn to a file (local checks only:
/// `OHL_PROJECT`, `OHL_SHOTS`).
#[test]
#[ignore = "local data"]
fn local_plan_image() {
    let (Ok(file), Ok(dir)) = (
        ohl_core::env::var("OHL_PROJECT"),
        ohl_core::env::var("OHL_SHOTS"),
    ) else {
        return;
    };
    let model = model::load(
        Sources {
            file: file.into(),
            point_budget: 20_000,
            corridor: 60.0,
            ..Sources::default()
        },
        &|m| eprintln!("{m}"),
    )
    .unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    h.run();
    h.state_mut().show_tab(crate::app::Tab::Plan);
    for _ in 0..30 {
        h.step();
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    h.render().unwrap().save(format!("{dir}/plan.png")).unwrap();
}

/// Where an edit's re-solve goes, on a local project (`OHL_PROJECT`):
/// one section re-solved in a solved case, its parts timed, with the
/// bundles as they are and without their spacing.
#[test]
#[ignore = "local data"]
fn local_resolve_timing() {
    let Ok(file) = ohl_core::env::var("OHL_PROJECT") else {
        return;
    };
    let mut model = model::load(
        Sources {
            file: file.into(),
            point_budget: 1000,
            corridor: 60.0,
            ..Sources::default()
        },
        &|m| eprintln!("{m}"),
    )
    .unwrap();
    let k = model.cases.iter().position(|c| c.ready).unwrap_or(0);
    model.ensure_solved(k);
    let library = model.library.clone();
    // A weather case solved whole: the jumpers' cuts made in the first,
    // kept for the others.
    for c in model.cases.iter().take(3) {
        let shapes: Vec<_> = ohl_line::solve::solve_line(&model.line, &library, &c.case)
            .into_iter()
            .filter_map(Result::ok)
            .collect();
        let t = std::time::Instant::now();
        let r = ohl_line::jumper::solve_jumpers(&model.line, &library, &c.case, &shapes);
        println!(
            "case {}: jumpers {:.1?} ({} failed)",
            c.case.name,
            t.elapsed(),
            r.iter().filter(|r| r.is_err()).count()
        );
    }
    for spaced in [true, false] {
        let mut line = model.line.clone();
        if !spaced {
            for s in &mut line.sections {
                s.bundle_spacing = None;
            }
            for j in &mut line.jumpers {
                j.bundle_spacing = None;
            }
        }
        let mut solved = model::Solved::from_live(
            &line,
            model.cases[k].case.clone(),
            ohl_line::incremental::SolvedLine::solve(&line, &library, &model.cases[k].case),
        );
        let dirty = vec![line.sections[10].id.clone()];
        let t = std::time::Instant::now();
        let shapes = solved.live.shapes(&line);
        let t_shapes = t.elapsed();
        let t = std::time::Instant::now();
        let _ = ohl_line::jumper::solve_jumpers(&line, &library, &solved.case, &shapes);
        let t_jumpers = t.elapsed();
        let t = std::time::Instant::now();
        let _ = ohl_line::bundles::BundleLines::new(&line, &solved.shapes, &solved.jumpers);
        let t_bundles = t.elapsed();
        let t = std::time::Instant::now();
        solved.update(&line, &library, &dirty);
        let t_update = t.elapsed();
        println!(
            "spacing {spaced}: update {t_update:.1?} = shapes {t_shapes:.1?}, jumpers {t_jumpers:.1?}, bundles {t_bundles:.1?}, rest; mirror {}",
            solved.mirror.is_some()
        );
    }
}

#[test]
#[ignore = "writes images of the wire library page scrolled right to OHL_SHOTS (a look by eye)"]
fn wire_library_scrolled_right_image() {
    let Ok(dir) = ohl_core::env::var("OHL_SHOTS") else {
        return;
    };
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 700.0))
        .build_eframe(|_| App::with_model(synthetic(), None));
    harness.run();
    harness.state_mut().ready().unwrap().open_page = Some((
        crate::app::Tab::Library(crate::library::LibraryKind::Cables),
        None,
    ));
    harness.run();
    harness.run();
    harness
        .render()
        .unwrap()
        .save(format!("{dir}/wires-0.png"))
        .unwrap();
    let at = harness
        .get_by_label("Bimetallic conductor (outer strands and a core)")
        .rect()
        .center();
    for _ in 0..6 {
        harness
            .input_mut()
            .events
            .push(egui::Event::PointerMoved(at));
        harness.input_mut().events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(-300.0, 0.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        harness.run_steps(3);
    }
    harness
        .render()
        .unwrap()
        .save(format!("{dir}/wires-1.png"))
        .unwrap();
}

/// The jumpers failing in a local project's first cases (`OHL_PROJECT`):
/// how each is defined, its bundle, its clamps' distance in each case.
#[test]
#[ignore = "local data"]
fn local_jumper_failures() {
    let Ok(file) = ohl_core::env::var("OHL_PROJECT") else {
        return;
    };
    let model = model::load(
        Sources {
            file: file.into(),
            point_budget: 1000,
            corridor: 60.0,
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    let line = &model.line;
    let lib = &model.library;
    let mut failed = std::collections::BTreeSet::new();
    let mut chords: std::collections::BTreeMap<String, Vec<(String, f64, String)>> =
        Default::default();
    for c in model.cases.iter().take(3) {
        let shapes: Vec<_> = ohl_line::solve::solve_line(line, lib, &c.case)
            .into_iter()
            .filter_map(Result::ok)
            .collect();
        for (j, r) in line
            .jumpers
            .iter()
            .zip(ohl_line::jumper::solve_jumpers(line, lib, &c.case, &shapes))
        {
            let msg = match &r {
                Ok(s) => format!("ok, sag {:.3} m, L0 {:.3}", s.sag, s.unstressed_length),
                Err(e) => {
                    failed.insert(j.id.clone());
                    e.to_string()
                }
            };
            chords
                .entry(j.id.clone())
                .or_default()
                .push((c.case.name.clone(), 0.0, msg));
        }
    }
    println!("{} jumpers fail in some of the first 3 cases", failed.len());
    for id in &failed {
        let j = line.jumpers.iter().find(|j| &j.id == id).unwrap();
        let b = line.jumper_bundle(j).count;
        let ends = |p: &ohl_line::SectionPoint| {
            line.sections
                .iter()
                .filter(|s| s.points.contains(p))
                .map(|s| {
                    let k = s.points.iter().position(|q| q == p).unwrap();
                    let end = k == 0 || k + 1 == s.points.len();
                    format!("{}{}", s.id, if end { "(end)" } else { "(through)" })
                })
                .collect::<Vec<_>>()
                .join("+")
        };
        println!(
            "{id} at {}: length {:?} sag {:?} t {} bundle {b}; from {} to {}",
            j.from,
            j.length,
            j.sag,
            j.temperature,
            ends(j.from.point().unwrap()),
            ends(j.to.point().unwrap())
        );
        for (case, _, m) in &chords[id] {
            println!("    {case}: {m}");
        }
    }
}

/// The string editor's presets put on a structure give a valid line: an
/// I-string, a V, a V with a string below, a TAK (two sets); opened again
/// at their set, the same strings come back; a V with a string below
/// strung into a section solves.
#[test]
fn string_editor_presets_make_valid_assemblies() {
    use crate::editors::{Preset, StringForm};
    let mut model = synthetic();
    for preset in Preset::ALL {
        let mut form = StringForm::new_on(&model, "T2").unwrap();
        let set = form.set;
        form.preset(preset, [0.0, 0.0, 40.0], &model);
        let mut line = model.line.clone();
        let s = line.structures.iter_mut().find(|s| s.id == "T2").unwrap();
        let resolved = model.line.structure("T2").cloned();
        form.apply_to(s, resolved.as_ref());
        line.validate()
            .unwrap_or_else(|e| panic!("{preset:?}: {e}"));
        let t2 = line.structure("T2").unwrap();
        let w = t2.wire_at_set(set).unwrap();
        let c = t2.component(&w.label);
        let strings = match preset {
            Preset::I => 1,
            Preset::V => 2,
            Preset::VAndString | Preset::Tak => 3,
        };
        assert_eq!(c.strings.len(), strings, "{preset:?}");
        if preset == Preset::Tak {
            assert!(t2.wire_at_set(set + 1).is_some());
            assert_eq!(
                c.wires
                    .iter()
                    .filter(|&&i| t2.wires[i].set.is_some())
                    .count(),
                2
            );
        }
    }
    // A V with a string below on a new structure, strung from T5's set 1.
    let mut line = model.line.clone();
    let t5 = line.structure("T5").unwrap().clone();
    let mut t9 = ohl_line::Structure::new(
        "T9",
        [t5.position[0] + 250.0, t5.position[1], t5.position[2]],
    );
    t9.orientation_deg = t5.orientation_deg;
    line.structures.push(t9);
    let t10 = ohl_line::Structure {
        wires: vec![ohl_line::WirePoint {
            description: Default::default(),
            label: "W1".into(),
            position: [
                t5.position[0] + 500.0,
                t5.position[1],
                t5.position[2] + 25.0,
            ],
            set: Some(1),
            weight: 0.0,
        }],
        ..ohl_line::Structure::new(
            "T10",
            [t5.position[0] + 500.0, t5.position[1], t5.position[2]],
        )
    };
    line.structures.push(t10);
    model.line = line.clone();
    let mut form = StringForm::new_on(&model, "T9").unwrap();
    form.preset(Preset::VAndString, [0.0, 0.0, 30.0], &model);
    let s = line.structures.iter_mut().find(|s| s.id == "T9").unwrap();
    form.apply_to(s, None);
    let mut section = line.sections[0].clone();
    section.id = "S9".into();
    section.points = vec![
        ohl_line::SectionPoint::new("T5", 1),
        ohl_line::SectionPoint::new("T9", 1),
        ohl_line::SectionPoint::new("T10", 1),
    ];
    section.clear_point_loads();
    section.stringing = ohl_line::Stringing::Tension {
        horizontal_tension: 20_000.0,
        temperature: 15.0,
    };
    line.sections.push(section);
    line.validate().unwrap();
    let case = model.cases[0].case.clone();
    let shape = ohl_line::solve::solve_line(&line, &model.library, &case)
        .pop()
        .unwrap()
        .unwrap();
    assert_eq!(shape.strings[1].len(), 3);
    assert!(shape.strings[1].iter().all(|x| x.force > 0.0));
}

/// The start screen (no project open) opens the plugin manager: a project
/// plugin can be installed or updated before the project it creates.
#[test]
fn the_start_screen_opens_the_plugin_manager() {
    let mut app = App::empty(Sources::default(), None);
    app.plugins = crate::plugins::Plugins::load(None, None);
    let mut h = Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .build_eframe(|_| app);
    h.run();
    h.get_by_label("Plugins…").click();
    // The manager repaints while it reads the plugins' formats in the
    // background (long on a busy machine): a few frames, not until idle.
    h.run_steps(4);
    assert!(h.state().plugins.open);
    h.get_by_label("Plugin manager");
}

/// A structure's "Strings and points…" (as the Edit tool) opens its type
/// on the structure screen: the tables on the left of the pylon's 3D
/// view (about 45/55), only the basics — attachment points, insulators,
/// wire points, set points; no joints or members — the level and the
/// structures using it. An attachment point added goes into the file as
/// edited (not saved); then "Edit in the structure application…" asks to
/// save first, and once the file is as saved it starts the application on
/// the type's file.
#[test]
fn the_structure_screen_shows_the_basics_left_of_the_pylon() {
    use crate::library::LibraryKind;
    use crate::type_workspace::{EDIT_IN_APP, VIEW_LABEL};
    let gpu = ohl_render::headless_device().is_ok();
    assert!(
        gpu || ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
        "no GPU"
    );
    let ex = root().join("examples/synthetic-line");
    let model = model::load(
        Sources {
            file: ex.join("synthetic-line-typed.ohl.json"),
            library: Some(root().join("libraries/cables")),
            structures: Some(ex.join("Structures")),
            point_budget: 1000,
            corridor: 60.0,
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    let mut builder = Harness::builder().with_size(egui::vec2(1500.0, 950.0));
    if gpu {
        builder = builder.wgpu();
    }
    let mut harness =
        builder.build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.run();
    harness.state_mut().ready().unwrap().selection = Selection::Structure("T2".into());
    harness.run();
    harness
        .get_by_label(crate::properties::STRINGS_AND_POINTS)
        .click();
    harness.run();
    harness.run();
    harness.get_by_label("Level 3 — full design");
    harness.get_by_label("used by 2 structures of the line (they follow when saved)");
    // The basics, and nothing of the body but its drawing.
    for sheet in ["Attachment points", "Strings", "Wire points", "Sets"] {
        assert!(
            harness.query_all_by_label(sheet).next().is_some(),
            "{sheet}"
        );
    }
    for gone in [
        "Joints",
        "Members",
        "Add joint",
        "Member",
        "Point here",
        "support",
    ] {
        assert!(harness.query_by_label(gone).is_none(), "{gone} is shown");
    }
    harness.get_by_label_contains("Body: 46 joint(s), 178 member(s)");
    // Tables left, the view right.
    let tab = harness.get_by_label("Attachment points").rect();
    let header = harness.get_by_label("attachment point").rect();
    let view = harness.get_by_label(VIEW_LABEL).rect();
    assert!(tab.right() < view.left() && header.right() < view.left());
    let tables = view.left() - tab.left();
    let share = tables / (view.right() - tab.left());
    assert!((0.35..0.55).contains(&share), "tables take {share}");
    assert!(view.width() > 400.0 && view.height() > 300.0, "{view:?}");
    if gpu && let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG") {
        let image = harness.render().unwrap();
        let path = PathBuf::from(dir).join("ohl-design-structure-screen.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
    // An attachment point beside A1.
    {
        let r = harness.state_mut().ready().unwrap();
        r.structure_apps.dry_run = Some(Vec::new());
        let page = r.libraries.get_mut(&LibraryKind::StructureTypes).unwrap();
        page.workspace.selected = vec![crate::type_workspace::Handle::Point("A1".into())];
    }
    harness.get_by_label("+ Attachment point").click();
    harness.run();
    let path = {
        let r = harness.state_mut().ready().unwrap();
        let page = &r.libraries[&LibraryKind::StructureTypes];
        assert!(page.dirty());
        let text = page.text().unwrap();
        assert_eq!(text.matches("[[point]]").count(), 3, "{text}");
        assert!(page.error.is_none(), "{:?}", page.error);
        page.file.clone().unwrap()
    };
    // Not with changes: the application edits the file as saved.
    harness.get_by_label(EDIT_IN_APP).click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.structure_apps.dry_run.as_deref(), Some(&[][..]));
        let page = r.libraries.get_mut(&LibraryKind::StructureTypes).unwrap();
        let (m, error) = page.message.clone().unwrap();
        assert!(error && m.starts_with("Save or discard"), "{m}");
        // Discarded.
        page.open(&path);
    }
    harness.run();
    harness.get_by_label(EDIT_IN_APP).click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    let launched = r.structure_apps.dry_run.clone().unwrap();
    assert_eq!(launched.len(), 1, "{:?}", r.status);
    assert_eq!(launched[0][0], path.as_os_str());
    assert!(launched[0].iter().any(|a| a == "--insulators"));
    assert!(!launched[0].iter().any(|a| a == "--structure"));
}

/// Edit… on a structure without a type: it has no file for the
/// structure application, so its structure screen asks for a type id
/// (Make it a structure file and edit it); Save, use and edit makes it a
/// structure type of the project, the structure takes it, and the
/// structure application is started on that file with the loads of this
/// place.
#[test]
fn edit_on_a_structure_without_a_file_makes_one_first() {
    use crate::placed_editor::MAKE_FILE_AND_EDIT;
    let model = synthetic_project("make-file-and-edit");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        r.selection = Selection::Structure("T3".into());
        r.structure_apps.dry_run = Some(Vec::new());
    }
    harness.run();
    harness
        .get_by_label(crate::properties::OPEN_IN_STRUCTURE_APP)
        .click();
    harness.run();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let p = r.placed.as_ref().expect("its structure screen");
        assert_eq!((p.id.as_str(), p.then_edit), ("T3", true));
        assert_eq!(p.type_id.as_deref(), Some("T3"));
        assert_eq!(r.structure_apps.dry_run.as_deref(), Some(&[][..]));
        r.placed.as_mut().unwrap().type_id = Some("t3-pylon".into());
    }
    // The screen's own button (no plain Edit in the structure application
    // there: there is no file yet).
    harness.get_by_label(MAKE_FILE_AND_EDIT);
    assert!(
        harness
            .query_by_label(crate::type_workspace::EDIT_IN_APP)
            .is_none()
    );
    harness.get_by_label("Save, use and edit").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.placed.is_none(), "{:?}", r.status);
    let s = r.model.line.structure("T3").unwrap();
    assert_eq!(s.structure_type.as_deref(), Some("t3-pylon"));
    let launched = r.structure_apps.dry_run.clone().unwrap();
    assert_eq!(launched.len(), 1, "{:?}", r.status);
    let args: Vec<String> = launched[0]
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert!(args[0].ends_with("t3-pylon.ohls"), "{args:?}");
    let k = args.iter().position(|a| a == "--structure").unwrap();
    assert_eq!(args[k + 1], "T3");
}

/// A colour typed in the Sections table goes on every section of its
/// circuit (by name), listed first to confirm; a bad colour is refused.
#[test]
fn a_circuit_takes_one_colour() {
    use crate::tables::TableKind;
    let model = synthetic_project("circuit-colour");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let colours = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        r.model
            .line
            .sections
            .iter()
            .map(|s| (s.id.clone(), s.circuit_name.clone(), s.color.clone()))
            .collect::<Vec<_>>()
    };
    set_cell(&mut harness, TableKind::Sections, "S1", "colour", "#12AB34");
    assert_eq!(
        apply_table(&mut harness, TableKind::Sections, false).unwrap_err(),
        "needs a confirmation"
    );
    apply_table(&mut harness, TableKind::Sections, true).unwrap();
    for (id, circuit, color) in colours(&mut harness) {
        let want = (circuit == "Example 1").then(|| "#12ab34".to_owned());
        assert_eq!(color, want, "{id} ({circuit})");
    }
    set_cell(&mut harness, TableKind::Sections, "S1", "colour", "red");
    assert!(
        apply_table(&mut harness, TableKind::Sections, true)
            .unwrap_err()
            .contains("#rrggbb")
    );
}

/// Wires coloured by system: the system's colour, its phases in line
/// codes (S1 L1 solid, S2 L2 ·–·–), the branch (joined to S1) like S1;
/// the 3D view and plan draw the codes and arrows (OHL_RENDER_PNG: an
/// image).
#[test]
fn wires_by_system_with_line_codes_and_arrows() {
    use crate::annotations::View;
    use crate::wire_style::{Coloring, Styles};
    let gpu = ohl_render::headless_device().is_ok();
    assert!(
        gpu || ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
        "no GPU"
    );
    let mut model = synthetic();
    let styles = Styles::new(&model.line, Coloring::Systems);
    let colour = crate::scene::rgb(&model.line.system_color(0));
    let solid: Vec<f64> = Vec::new();
    assert_eq!(
        styles.section("S1"),
        crate::wire_style::Style {
            color: colour,
            pattern: solid.clone()
        }
    );
    assert_eq!(
        styles.section("S2").pattern,
        crate::wire_style::pattern(".-")
    );
    assert_eq!(styles.section("S2").color, colour);
    assert_eq!(styles.section("S3").pattern, solid);
    assert_eq!(styles.jumper("J-T3-branch").color, colour);
    // By circuit: each section's own (phase) colours.
    let by_circuit = Styles::new(&model.line, Coloring::Circuits);
    assert_ne!(
        by_circuit.section("S1").color,
        by_circuit.section("S2").color
    );
    model.section_display.coloring = Coloring::Systems;
    model.section_display.direction = [true; 3];
    let lines = |m: &model::Model| {
        let mut out = Vec::new();
        crate::scene::wires(&mut out, m, &m.line, &m.cases, 0, true, None);
        out.len()
    };
    let coded = lines(&model);
    let display = model.section_display.clone();
    model.section_display.coloring = Coloring::Circuits;
    model.section_display.direction[View::ThreeD as usize] = false;
    let plain = lines(&model);
    assert!(coded > plain + 20, "dashes and arrows: {coded} vs {plain}");
    model.section_display = display;
    let mut builder = Harness::builder().with_size(egui::vec2(1400.0, 900.0));
    if gpu {
        builder = builder.wgpu();
    }
    let mut harness =
        builder.build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.run();
    if gpu && let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG") {
        let image = harness.render().unwrap();
        let path = PathBuf::from(dir).join("ohl-design-system-codes.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
}

/// The circuits table colours a whole circuit at once; the systems table
/// gives each phase its line code ("- --." for L1 and L2).
#[test]
fn circuit_colours_and_system_line_codes_in_the_tables() {
    use crate::tables::TableKind;
    use crate::wire_style::{Coloring, Styles, pattern};
    let model = synthetic_project("circuit-codes");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // The circuit of S1 (with the branch S3 by its jumper).
    let key = {
        let r = harness.state_mut().ready().unwrap();
        let rows = TableKind::ConductorPaths.rows(&r.model, 0);
        rows.iter()
            .find(|x| x.cells[6].contains("S3"))
            .unwrap()
            .key
            .clone()
    };
    set_cell(
        &mut harness,
        TableKind::ConductorPaths,
        &key,
        "colour",
        "#AA5500",
    );
    apply_table(&mut harness, TableKind::ConductorPaths, true).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let colour = |id: &str| {
            r.model
                .line
                .sections
                .iter()
                .find(|s| s.id == id)
                .unwrap()
                .color
                .clone()
        };
        assert_eq!(colour("S1").as_deref(), Some("#aa5500"));
        assert_eq!(colour("S3").as_deref(), Some("#aa5500"));
        assert_eq!(colour("S2"), None);
    }
    set_cell(
        &mut harness,
        TableKind::Systems,
        "ex220",
        "line codes",
        "- --.",
    );
    apply_table(&mut harness, TableKind::Systems, true).unwrap();
    let r = harness.state_mut().ready().unwrap();
    let styles = Styles::new(&r.model.line, Coloring::Systems);
    assert_eq!(styles.section("S2").pattern, pattern("--."));
    assert!(styles.section("S1").pattern.is_empty());
    // Not dots and dashes: refused.
    set_cell(
        &mut harness,
        TableKind::Systems,
        "ex220",
        "line codes",
        "- x",
    );
    assert!(
        apply_table(&mut harness, TableKind::Systems, true)
            .unwrap_err()
            .contains("dots and dashes")
    );
}

/// A string is picked all along its body, where it is drawn; the string
/// window places a wire point by angles and a length from the string's
/// top, and its set's dead end (yes/no) makes its strings tension strings
/// or suspension ones; a string whose wire point has no set (a jumper's
/// helping string) opens too.
#[test]
fn strings_are_picked_along_their_body_and_edited_by_angles() {
    use crate::editors::{StringForm, from_polar, polar};
    use crate::selection::{Snaps, candidates, pick};
    // Angles: 0° down, 90° level; direction 0° ahead, 90° to the right.
    let top = [1.0, 2.0, 30.0];
    for (down, dir, len) in [(0.0, 0.0, 4.0), (90.0, 90.0, 2.0), (30.0, 225.0, 3.5)] {
        let at = from_polar(top, down, dir, len);
        let (d, r, l) = polar(top, at);
        assert!((d - down).abs() < 1e-9 && (l - len).abs() < 1e-9, "{d} {l}");
        if down > 0.0 {
            assert!((r - dir).abs() < 1e-9, "{r}");
        }
    }
    let p = from_polar([0.0; 3], 90.0, 90.0, 2.0);
    assert!((p[0] - 2.0).abs() < 1e-12 && p[1].abs() < 1e-12 && p[2].abs() < 1e-12);

    let mut model = synthetic();
    // T2's first string, at its middle as drawn: the string.
    let pieces = crate::selection::string_pieces(&model, 0);
    let (a, b) = pieces[&("T2".to_owned(), "S1".to_owned())][0];
    let mid = [0, 1, 2].map(|k| 0.5 * (a[k] + b[k]));
    // Seen along the line (it runs east: y, z on screen), 1 px per cm.
    let project = |p: [f64; 3]| Some([p[1] * 100.0, -p[2] * 100.0]);
    let at = project(mid).unwrap();
    assert_eq!(
        pick(&candidates(&model, 0, &Snaps::default()), at, 5.0, project),
        Selection::String {
            structure: "T2".into(),
            label: "S1".into()
        }
    );
    // Its window: the wire point from its top; the set a suspension point.
    let f = StringForm::of_string(&model, "T2", "S1").unwrap();
    let w = f.wires.iter().find(|w| w.set.is_some()).unwrap();
    assert!(!w.dead_end);
    // Made a dead end: its strings become tension strings.
    let mut f2 = f.clone();
    f2.wires
        .iter_mut()
        .for_each(|w| w.dead_end = w.set.is_some());
    let mut s = model.line.structure("T2").unwrap().clone();
    f2.apply_to(&mut s, None);
    assert!(s.strings_to(&f2.wires[0].label).all(|x| x.strain));
    // A wire point without a set (a helping string): it opens, set 0.
    let st = model
        .line
        .structures
        .iter_mut()
        .find(|s| s.id == "T3")
        .unwrap();
    let w0 = st.wires[0].position;
    st.wires.push(ohl_line::WirePoint {
        description: Default::default(),
        label: "JH".into(),
        position: [w0[0], w0[1], w0[2] - 2.0],
        set: None,
        weight: 0.0,
    });
    st.strings.push(ohl_line::InsulatorString {
        label: "H1".into(),
        top: st.points[0].label.clone(),
        wire: "JH".into(),
        ..Default::default()
    });
    let h = StringForm::of_string(&model, "T3", "H1").unwrap();
    assert_eq!(h.set, 0);
    assert_eq!(h.strings.len(), 1);
}

/// A string's limits (kept, not checked) in the Strings table: typed in,
/// applied to the stored line, cleared again; a bad one refused; no other
/// string given any; nothing flagged by them.
#[test]
fn string_limits_are_edited_in_the_strings_table() {
    use crate::tables::TableKind;
    let k = TableKind::Strings;
    let model = synthetic_project("string-limits-table");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    for col in ["max load N", "max swing °", "max across °", "max along °"] {
        let r = harness.state_mut().ready().unwrap();
        let rows = k.rows(&r.model, r.case);
        assert!(
            rows.iter().all(|x| x.cells[k.column(col)].is_empty()),
            "{col}"
        );
    }
    set_cell(&mut harness, k, "T2\tS1", "max load N", "120000");
    set_cell(&mut harness, k, "T2\tS1", "max across °", "40.5");
    apply_table(&mut harness, k, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let stored = r.model.stored_line().unwrap();
        let l = stored.string_limits("T2", "S1").unwrap();
        assert_eq!(l.max_load, Some(120_000.0));
        assert_eq!(l.max_swing_across_deg, Some(40.5));
        assert_eq!((l.max_swing_deg, l.max_swing_along_deg), (None, None));
        let given: usize = stored
            .structures
            .iter()
            .flat_map(|s| &s.strings)
            .filter(|x| !x.limits().is_empty())
            .count();
        assert_eq!(given, 1);
        assert_eq!(r.model.line.string_limits("T2", "S1"), Some(l));
        let rows = k.rows(&r.model, r.case);
        let row = rows.iter().find(|x| x.key == "T2\tS1").unwrap();
        assert_eq!(row.cells[k.column("max load N")], "120000");
        assert!(!row.defaulted[k.column("max load N")]);
    }
    // Out of range: refused, nothing changed.
    set_cell(&mut harness, k, "T2\tS1", "max swing °", "200");
    assert!(apply_table(&mut harness, k, false).is_err());
    harness.state_mut().ready().unwrap().tables.remove(&k);
    set_cell(&mut harness, k, "T2\tS1", "max load N", "");
    apply_table(&mut harness, k, false).unwrap();
    let r = harness.state_mut().ready().unwrap();
    let l = r
        .model
        .stored_line()
        .unwrap()
        .string_limits("T2", "S1")
        .unwrap();
    assert_eq!((l.max_load, l.max_swing_across_deg), (None, Some(40.5)));
}

/// The string window shows a string's limits (kept, not checked), each
/// optional; given in the form, applied to the structure's string.
#[test]
fn the_string_window_edits_limits() {
    let model = synthetic();
    let mut f = crate::editors::StringForm::of_string(&model, "T2", "S1").unwrap();
    let k = f.strings.iter().position(|x| x.label == "S1").unwrap();
    assert!(f.strings[k].limits.is_empty());
    f.strings[k].limits.max_load = Some(90_000.0);
    f.strings[k].limits.max_swing_deg = Some(50.0);
    let mut s = model.line.structure("T2").unwrap().clone();
    f.apply_to(&mut s, None);
    let l = s.string("S1").unwrap().limits();
    assert_eq!((l.max_load, l.max_swing_deg), (Some(90_000.0), Some(50.0)));
    assert_eq!(l.max_swing_across_deg, None);
    // The other strings: none given.
    assert!(
        s.strings
            .iter()
            .filter(|x| x.label != "S1")
            .all(|x| x.limits().is_empty())
    );
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        r.editor = Some(crate::editors::Form::String(f));
    }
    harness.run();
    harness.get_by_label("max load");
    harness.get_by_label("max swing");
}

/// The string window: where the string hangs (a list of the structure's
/// points), its wire point by angles and length, the set's dead end, with
/// short explanations (OHL_RENDER_PNG: an image).
#[test]
fn the_string_window_explains_itself() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        r.editor = crate::editors::StringForm::of_string(&r.model, "T2", "S1")
            .map(crate::editors::Form::String);
    }
    harness.run();
    harness.get_by_label("hangs from");
    harness.get_by_label("clamp");
    harness.get_by_label("down/up");
    // A new string makes its own wire point, 3 m straight down from its
    // top; taken away, its wire point goes with it.
    let before = {
        let r = harness.state_mut().ready().unwrap();
        let Some(crate::editors::Form::String(f)) = &r.editor else {
            panic!()
        };
        (f.strings.len(), f.wires.len())
    };
    harness.get_by_label("+ string").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(crate::editors::Form::String(f)) = &mut r.editor else {
            panic!()
        };
        assert_eq!(
            (f.strings.len(), f.wires.len()),
            (before.0 + 1, before.1 + 1)
        );
        let x = f.strings.last().unwrap().clone();
        let top = f.points.iter().find(|p| p.label == x.top).unwrap().at;
        let w = f.wires.iter().find(|w| w.label == x.wire).unwrap();
        assert_eq!(crate::editors::polar(top, w.at), (0.0, 0.0, 3.0));
        // From the library: a 4.2 m tension assembly, the wire point at it.
        let k = f.strings.len() - 1;
        f.wires.iter_mut().find(|w| w.label == x.wire).unwrap().set = Some(7);
        f.take_from_library(
            k,
            &crate::editors::LibraryString {
                id: "S-420".into(),
                length: 4.2,
                weight: 800.0,
                wind_area: 0.3,
                stiffness: 4e7,
                strain: true,
            },
        );
        let w = f.wires.iter().find(|w| w.label == x.wire).unwrap();
        assert!((crate::editors::polar(top, w.at).2 - 4.2).abs() < 1e-9 && w.dead_end);
        assert_eq!(f.strings[k].weight, 800.0);
        f.strings.pop();
        f.drop_unused_wires();
        assert_eq!((f.strings.len(), f.wires.len()), before);
    }
    harness.run();
    assert_eq!(harness.query_all_by_label("direction").count(), 1);
    if let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG") {
        let image = harness.render().unwrap();
        let path = PathBuf::from(dir).join("ohl-design-string-window.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
}

/// The colour cells of the tables open the colour tool; a palette colour
/// picked and OK write it in the cell (pending until Apply), which Apply
/// sets (OHL_RENDER_PNG: an image of the tool).
#[test]
fn the_colour_tool_writes_a_palette_colour_in_the_cell() {
    use crate::tables::{ColorCell, TableKind};
    let model = synthetic_project("colour-tool");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.state_mut().ready().unwrap().open_page =
        Some((crate::app::Tab::Table(TableKind::Systems), None));
    harness.run();
    harness.run();
    // The systems' colours are defaults shown (dimmed): picked, set.
    harness
        .query_all_by_label("colour tool")
        .next()
        .expect("a colour cell")
        .click();
    harness.run();
    let key = {
        let r = harness.state_mut().ready().unwrap();
        match &r.tables[&TableKind::Systems].color {
            Some((ColorCell::Row { key, .. }, _)) => key.clone(),
            other => panic!("{:?}", other.as_ref().map(|c| &c.0)),
        }
    };
    harness.get_by_label("Wheel");
    harness.get_by_label("Sliders");
    // Opening it changes nothing.
    let unchanged = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        let t = &r.tables[&TableKind::Systems].color.as_ref().unwrap().1;
        t.old == Some(t.rgb)
    };
    assert!(unchanged(&mut harness));
    harness.get_by_label("Sliders").click();
    harness.run();
    assert!(unchanged(&mut harness));
    harness.get_by_label("Wheel").click();
    harness.run();
    harness.get_by_label("Neutral").click();
    harness.run();
    if let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG") {
        let image = harness.render().unwrap();
        let path = PathBuf::from(dir).join("ohl-design-colour-tool.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
    harness.get_by_label("OK").click();
    harness.run();
    let c = TableKind::Systems.column("colour");
    {
        let r = harness.state_mut().ready().unwrap();
        let page = &r.tables[&TableKind::Systems];
        assert!(page.color.is_none(), "OK closes it");
        assert_eq!(page.edits[&key][&c], "#b0b8c0");
    }
    apply_table(&mut harness, TableKind::Systems, true).unwrap();
    let r = harness.state_mut().ready().unwrap();
    let s = r.model.line.systems.iter().find(|s| s.id == key).unwrap();
    assert_eq!(s.color.as_deref(), Some("#b0b8c0"));
    // Cancel leaves the cell.
    let page = r.tables.get_mut(&TableKind::Systems).unwrap();
    page.open_color_tool(
        TableKind::Systems,
        ColorCell::Row {
            key: key.clone(),
            column: c,
            original: "#b0b8c0".into(),
        },
        "#b0b8c0",
    );
    harness.run();
    harness.get_by_label("Cancel").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    let page = &r.tables[&TableKind::Systems];
    assert!(page.color.is_none() && page.edits.is_empty());
}

/// Every colour reads back the same from hue, saturation and value; the
/// wheel has red to the right, green up-left, saturation outwards.
#[test]
fn colours_go_round_hsv_and_the_wheel() {
    use crate::color_tool::{hsv_to_rgb, rgb_to_hsv, wheel_hs, wheel_point};
    for r in (0..=255u8).step_by(5) {
        for g in (0..=255u8).step_by(15) {
            for b in (0..=255u8).step_by(17) {
                let c = [r, g, b];
                assert_eq!(hsv_to_rgb(rgb_to_hsv(c)), c);
            }
        }
    }
    let hex = "#4c9be8";
    let c = ohl_core::text::parse_rgb(hex).unwrap();
    assert_eq!(ohl_core::text::format_rgb(hsv_to_rgb(rgb_to_hsv(c))), hex);
    assert_eq!(hsv_to_rgb([120.0, 1.0, 1.0]), [0, 255, 0]);
    let [h, s, v] = rgb_to_hsv([0, 0, 255]);
    assert!((h - 240.0).abs() < 1e-9 && s == 1.0 && v == 1.0);
    let o = egui::pos2(100.0, 100.0);
    let near = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3;
    assert!(near(
        wheel_hs(o, 50.0, egui::pos2(125.0, 100.0)),
        (0.0, 0.5)
    ));
    assert!(near(
        wheel_hs(o, 50.0, egui::pos2(100.0, 50.0)),
        (90.0, 1.0)
    ));
    assert!(near(
        wheel_hs(o, 50.0, egui::pos2(60.0, 100.0)),
        (180.0, 0.8)
    ));
    assert!(near(
        wheel_hs(o, 50.0, egui::pos2(100.0, 200.0)),
        (270.0, 1.0)
    ));
    assert!(near(wheel_hs(o, 50.0, o), (0.0, 0.0)));
    let p = wheel_point(o, 50.0, 120.0, 0.6);
    assert!(near(wheel_hs(o, 50.0, p), (120.0, 0.6)));
    // Green (120°) is up and to the left.
    assert!(p.x < o.x && p.y < o.y);
}

/// The palette is written and read back as a file; a file that is not a
/// palette, or has bad colours, is refused with what is wrong; an import
/// replaces the palette or adds what is missing.
#[test]
fn palettes_export_and_import() {
    use crate::color_tool::{ColorTool, Palette, PaletteEntry};
    let mut p = Palette::default();
    assert_eq!(p.entries.len(), 3 + 1 + 8);
    p.entries.push(PaletteEntry {
        name: "Earth \"OPGW\"".into(),
        color: [1, 2, 3],
        description: "earth wires, 380 kV".into(),
    });
    let text = p.to_toml();
    assert!(text.contains("format = \"ohl-colors\""), "{text}");
    assert_eq!(Palette::parse(&text).unwrap(), p.entries);
    // Remembered.
    let mut storage = Memory::default();
    p.mode = crate::color_tool::Mode::Sliders;
    p.save(&mut storage);
    assert_eq!(Palette::load(&storage), p);
    assert_eq!(Palette::load(&Memory::default()), Palette::default());
    // Refused.
    let e = Palette::parse("format = \"ohl-colours\"\nversion = 1\n").unwrap_err();
    assert!(e.contains("is not \"ohl-colors\""), "{e}");
    // The header optional, as every format's.
    assert_eq!(Palette::parse("version = 1\n").unwrap(), Vec::new());
    assert!(
        Palette::parse("format = \"ohl-colors\"\nversion = 9\n")
            .unwrap_err()
            .contains("newer")
    );
    let e = Palette::parse(
        "format = \"ohl-colors\"\nversion = 1\n[[color]]\nname = \"a\"\ncolor = \"red\"\n\
         [[color]]\ncolor = \"#123456\"\n",
    )
    .unwrap_err();
    assert!(
        e.contains("colour 1 (a): color \"red\"") && e.contains("colour 2: name"),
        "{e}"
    );
    assert!(Palette::parse("[[[").unwrap_err().contains("TOML"));
    // Through a file: refused, then replaced and added.
    let dir = std::env::temp_dir().join(format!("ohl-colours-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bad = dir.join("bad.ohlcolors");
    std::fs::write(&bad, "format = \"x\"\n").unwrap();
    let mut tool = ColorTool::new("t", None);
    tool.import_file(&bad);
    assert!(tool.status.as_ref().unwrap().1);
    let file = dir.join("mine.ohlcolors");
    p.write(&file).unwrap();
    let mut q = Palette {
        entries: vec![p.entries[0].clone()],
        mode: Default::default(),
    };
    tool.import_file(&file);
    tool.finish_import(&mut q, false);
    assert_eq!(q.entries, p.entries, "the one there not twice");
    let mut r = Palette {
        entries: vec![PaletteEntry {
            name: "x".into(),
            color: [9, 9, 9],
            description: String::new(),
        }],
        mode: Default::default(),
    };
    tool.import_file(&file);
    tool.finish_import(&mut r, true);
    assert_eq!(r.entries, p.entries);
    std::fs::remove_dir_all(&dir).ok();
}

/// An empty project: the 3D view shows the ground; the first alignment is
/// the line's own (it was refused: an empty "main" beside it); the view
/// fits to the first structure.
#[test]
fn an_empty_project_gets_its_first_alignment_and_a_view() {
    use crate::tools::Tool;
    let gpu = ohl_render::headless_device().is_ok();
    assert!(
        gpu || ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
        "no GPU"
    );
    let ex = root().join("examples/synthetic-line");
    let dir = std::env::temp_dir().join(format!("ohl-app-{}-empty", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ohl");
    std::fs::remove_file(&path).ok();
    let mut line = ohl_line::Line::load(&ex.join("synthetic-line.ohl.json")).unwrap();
    line.structures.clear();
    line.sections.clear();
    line.jumpers.clear();
    line.routes.clear();
    line.alignments.clear();
    line.alignment_points.clear();
    line.systems.clear();
    ohl_session::Session::open(&path)
        .unwrap()
        .import(&line, "tester")
        .unwrap();
    let model = model::load(
        Sources {
            file: path,
            library: Some(root().join("libraries/cables")),
            terrain: Some(ex.join("synthetic-survey.csv")),
            point_budget: 1000,
            corridor: 60.0,
            user: Some("tester".into()),
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    // The 3D chunks are built within a time budget per frame: a busy
    // machine needs more frames than the default.
    let mut builder = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .with_max_steps(64);
    if gpu {
        builder = builder.wgpu();
    }
    let mut harness =
        builder.build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.run();
    if gpu {
        // Fitted to the ground, not to a point.
        let cam = harness.state_mut().ready().unwrap().view3d.camera.unwrap();
        assert!(cam.distance > 100.0, "{}", cam.distance);
    }
    let ground = |h: &mut Harness<'_, App>, x: f64, y: f64| {
        let r = h.state_mut().ready().unwrap();
        [x, y, r.model.ground_at(x, y).unwrap()]
    };
    // Step 1: the route (the window of the first steps).
    assert!(harness.state_mut().ready().unwrap().first_steps.is_some());
    harness.get_by_label("Create the route").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.first_steps.is_none(), "{:?}", r.status);
        assert_eq!(r.model.line.routes[0].id, "route 1");
        // Step 2: the New alignment tool held.
        assert_eq!(r.hotbar.tool(), Tool::NewAlignment);
    }
    for (x, y) in [(0.0, 0.0), (310.0, 0.0), (640.0, 560.0)] {
        let p = ground(&mut harness, x, y);
        // Nothing to snap to: the coordinates window, confirmed.
        use_tool(&mut harness, Tool::NewAlignment, Selection::None, Some(p));
        let r = harness.state_mut().ready().unwrap();
        let point = r.coord_dialog.take().expect("asked").point().unwrap();
        r.place_alignment_point(
            crate::alignment_tools::CoordFor::NewAlignment,
            ohl_line::AlignmentPoint::free("", point),
        );
    }
    // Enter: its route, the only one, chosen.
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.route_choice.as_ref().unwrap().route, "route 1");
        r.make_new_alignment("route 1".into(), false);
        let line = &r.model.line;
        assert_eq!(line.alignments.len(), 1, "{:?}", r.status);
        assert_eq!(line.alignments[0].points.len(), 3);
        assert_eq!(line.alignments[0].route, "route 1");
        assert_eq!(line.routes[0].id, "route 1");
    }
    let p = ground(&mut harness, 310.0, 0.0);
    use_tool(&mut harness, Tool::AddStructure, Selection::None, Some(p));
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    harness.run();
    if gpu {
        let cam = harness.state_mut().ready().unwrap().view3d.camera.unwrap();
        assert!(
            (cam.target[0] - 310.0).abs() < 1.0 && cam.target[1].abs() < 1.0,
            "{:?}",
            cam.target
        );
    }
}

/// A new structure on an alignment point: it stands there, tied to it,
/// ahead along the alignment and its cross-arms on the bisector of the
/// line angle; its own points and description kept. The project's angle
/// ahead turns the compass (270: right 0°, ahead 270°). Image with
/// OHL_RENDER_PNG.
#[test]
fn a_structure_is_put_on_an_alignment_point_along_the_line() {
    use crate::editors::{Form, PointRow, StructureForm, compass_direction, line_orientation};
    let gpu = ohl_render::headless_device().is_ok();
    let model = synthetic_project_with("on-alignment", |l| {
        // An alignment of its own: 0;0 → 300;0 → 300;300 (a right angle
        // at its middle point).
        l.routes = vec![ohl_line::Route::new("main", "main")];
        l.alignment_points.clear();
        l.alignments = vec![ohl_line::Alignment::new("main", "main", Vec::new())];
        l.set_alignment_points(
            "main",
            [
                [0.0, 0.0, 500.0],
                [300.0, 0.0, 500.0],
                [300.0, 300.0, 500.0],
            ]
            .map(|p| (p, None)),
        );
    });
    // The bisector at the corner: ahead to the north-east, the right to
    // the south-east (−45° from east); at the ends, across their stretch.
    let line = &model.line;
    let mid = line_orientation(line, [300.0, 0.0, 500.0], Some(&("main".into(), 1))).unwrap();
    assert!((mid - -45.0).abs() < 1e-9, "{mid}");
    let first = line_orientation(line, [0.0, 0.0, 500.0], Some(&("main".into(), 0))).unwrap();
    assert!((first - -90.0).abs() < 1e-9, "{first}");
    // The angle convention: ahead at 270 → 0° is the right (+x).
    let d = compass_direction(0.0, 270.0);
    assert!((d[0] - 1.0).abs() < 1e-12 && d[1].abs() < 1e-12);
    let d = compass_direction(270.0, 270.0);
    assert!(d[0].abs() < 1e-12 && (d[1] - 1.0).abs() < 1e-12);
    let mut builder = Harness::builder().with_size(egui::vec2(1400.0, 950.0));
    if gpu {
        builder = builder.wgpu();
    }
    let mut harness =
        builder.build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        r.angles_window = Some(270.0);
    }
    harness.run();
    harness.get_by_label("Apply").click();
    harness.run();
    assert_eq!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .model
            .line
            .display
            .ahead_deg,
        270.0
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let mut f = StructureForm::new_at(&r.model, [301.0, 2.0, 500.0]);
        assert_eq!(
            f.on_alignment,
            Some(("main".into(), 1)),
            "the point near the click"
        );
        f.structure_type = None;
        f.description = "angle tower, invented".into();
        f.points = vec![
            PointRow {
                description: Default::default(),
                label: "A1".into(),
                at: [-6.0, 0.0, 25.0],
            },
            PointRow {
                description: Default::default(),
                label: "A2".into(),
                at: [6.0, 0.0, 25.0],
            },
        ];
        r.editor = Some(Form::Structure(f));
    }
    harness.run();
    harness.run();
    harness.get_by_label("on an alignment point");
    if gpu && let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG") {
        let image = harness.render().unwrap();
        let path = PathBuf::from(dir).join("ohl-design-structure-window.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    let r = harness.state_mut().ready().unwrap();
    assert!(r.editor.is_none(), "{:?}", r.status);
    let id = r.model.line.structures.last().unwrap().id.clone();
    let s = r.model.line.structure(&id).unwrap();
    assert_eq!((s.position[0], s.position[1]), (300.0, 0.0));
    assert!((s.orientation_deg - -45.0).abs() < 1e-9);
    assert_eq!(s.description, "angle tower, invented");
    assert_eq!(s.points.len(), 2);
    let line = &r.model.line;
    assert_eq!(
        line.points_of(&line.alignments[0])[1].structure.as_deref(),
        Some(id.as_str())
    );
    // On the alignment's route, by itself.
    assert_eq!(r.model.line.routes_of(&id), ["main"]);
}

/// Descriptions: typed in the tables, kept, shown as labels with the
/// objects' labels when asked for (Structures › Display options).
#[test]
fn descriptions_are_typed_and_shown_as_labels() {
    use crate::annotations::{Kind, View, object_notes};
    use crate::tables::TableKind;
    let model = synthetic_project("descriptions");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    set_cell(
        &mut harness,
        TableKind::Structures,
        "T2",
        "description",
        "angle tower",
    );
    apply_table(&mut harness, TableKind::Structures, true).unwrap();
    set_cell(
        &mut harness,
        TableKind::Strings,
        "T2\tS1",
        "description",
        "glass, 2019",
    );
    apply_table(&mut harness, TableKind::Strings, true).unwrap();
    set_cell(
        &mut harness,
        TableKind::Sections,
        "S1",
        "description",
        "phase L1",
    );
    apply_table(&mut harness, TableKind::Sections, true).unwrap();
    set_cell(
        &mut harness,
        TableKind::Jumpers,
        "J-T3-branch",
        "description",
        "branch loop",
    );
    apply_table(&mut harness, TableKind::Jumpers, true).unwrap();
    set_cell(
        &mut harness,
        TableKind::Systems,
        "ex220",
        "description",
        "220 kV",
    );
    apply_table(&mut harness, TableKind::Systems, true).unwrap();
    let r = harness.state_mut().ready().unwrap();
    let line = &r.model.line;
    let t2 = line.structure("T2").unwrap();
    assert_eq!(t2.description, "angle tower");
    assert_eq!(t2.string("S1").unwrap().description, "glass, 2019");
    assert_eq!(line.sections[0].description, "phase L1");
    assert_eq!(line.jumpers[0].description, "branch loop");
    assert_eq!(line.systems[0].description, "220 kV");
    // Labels: none until asked for; then the structures, points, strings,
    // wire points, with their descriptions when ticked.
    assert!(object_notes(&r.model, 0, View::Plan, None).is_empty());
    let d = &mut r.model.section_display;
    for ticks in [
        &mut d.structure_label,
        &mut d.point_label,
        &mut d.string_label,
        &mut d.wire_label,
        &mut d.descriptions,
    ] {
        ticks[View::Plan as usize] = true;
    }
    let notes = object_notes(&r.model, 0, View::Plan, None);
    assert!(
        notes
            .iter()
            .all(|n| n.kind == Kind::Object || n.kind == Kind::Structure)
    );
    assert!(notes.iter().any(|n| n.kind == Kind::Structure));
    let texts: Vec<&str> = notes.iter().map(|n| n.text.as_str()).collect();
    assert!(texts.contains(&"T2 — angle tower"), "{texts:?}");
    assert!(texts.contains(&"S1 — glass, 2019"), "{texts:?}");
    assert!(texts.iter().any(|t| t.ends_with("(set 1)")), "{texts:?}");
    // A string's label sits where it is drawn: at its middle as solved.
    let pieces = crate::selection::string_pieces(&r.model, 0);
    let (a, b) = pieces[&("T2".to_owned(), "S1".to_owned())][0];
    let n = notes.iter().find(|n| n.text.starts_with("S1 —")).unwrap();
    assert!(
        (0..3).all(|k| (n.at[k] - 0.5 * (a[k] + b[k])).abs() < 1e-9)
            || notes.iter().filter(|n| n.text.starts_with("S1 —")).count() > 1
    );
    // The insulator's top on its own tick: apart from the structure point
    // it hangs from (a little down the string).
    assert!(!texts.iter().any(|t| t.contains(" top ➡ ")), "{texts:?}");
    r.model.section_display.insulator_label[View::Plan as usize] = true;
    let notes = object_notes(&r.model, 0, View::Plan, None);
    let top = &t2.string("S1").unwrap().top;
    let n = notes
        .iter()
        .find(|n| n.text == format!("S1 top ➡ {top}"))
        .expect("the top's label");
    let p = notes
        .iter()
        .find(|m| m.text.starts_with(top.as_str()))
        .unwrap();
    assert!((0..3).any(|k| (n.at[k] - p.at[k]).abs() > 1e-6));
    // The library labels on their own tick.
    assert!(!notes.iter().any(|n| n.text.starts_with('[')));
    let labelled = r
        .model
        .line
        .structures
        .iter()
        .flat_map(|s| &s.strings)
        .find_map(|x| x.insulator.clone());
    r.model.section_display.library_label[View::Plan as usize] = true;
    let notes = object_notes(&r.model, 0, View::Plan, None);
    if let Some(id) = labelled {
        assert!(notes.iter().any(|n| n.text == format!("[{id}]")));
    }
}

/// A structure type file opened on its own: its editor is the whole
/// window (no line), the pylon in 3D with the compass, its level; the
/// angle ahead is the user's for files opened alone. Image with
/// OHL_RENDER_PNG.
#[test]
fn a_structure_type_opens_on_its_own() {
    let gpu = ohl_render::headless_device().is_ok();
    let file = root().join("examples/synthetic-line/Structures/type-02.ohls");
    let mut builder = Harness::builder().with_size(egui::vec2(1400.0, 950.0));
    if gpu {
        builder = builder.wgpu();
    }
    let mut harness = builder.build_eframe(|cc| {
        let mut app = App::empty(Sources::default(), cc.wgpu_render_state.clone());
        app.ahead_alone = 270.0;
        app.start(
            Sources {
                file: file.clone(),
                ..Sources::default()
            },
            &cc.egui_ctx,
        );
        app
    });
    harness.run();
    harness.run();
    harness.get_by_label("Structure type, opened on its own");
    harness.get_by_label("Level 3 — full design");
    harness.get_by_label("opened alone (no line)");
    assert_eq!(
        harness.state_mut().library_ahead_for_tests(),
        Some(270.0),
        "the compass counts as the user does"
    );
    if gpu && let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG") {
        let image = harness.render().unwrap();
        let path = PathBuf::from(dir).join("ohl-design-structure-alone.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
}

/// A structure without a type opens on its structure screen ("Strings and points…"):
/// as it is, its items read back unchanged; a point moved goes on the
/// structure on Apply (one command); "Make it a type…" saves it as a
/// structure type of the project and the structure takes it.
#[test]
fn a_structure_without_a_type_is_edited_in_the_pylon_editor() {
    let model = synthetic_project("placed-editor");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.state_mut().ready().unwrap().selection = Selection::Structure("T3".into());
    harness.run();
    harness
        .get_by_label(crate::properties::STRINGS_AND_POINTS)
        .click();
    harness.run();
    harness.run();
    harness.get_by_label("its own points, wire points and strings, in its axes");
    // The four tables: attachment points, insulators (top, kind,
    // assembly, length, angles…), wire points, set points.
    harness.get_by_label("Strings").click();
    harness.run();
    harness.get_by_label("hangs from");
    harness.get_by_label("angle");
    harness.get_by_label("kind");
    harness.get_by_label("+ New string");
    for sheet in ["Attachment points", "Wire points", "Sets"] {
        // (The sheet's tab: the first of that name.)
        harness.query_all_by_label(sheet).next().unwrap().click();
        harness.run();
    }
    harness.get_by_label("on wire point");
    let near = |a: [f64; 3], b: [f64; 3]| (0..3).all(|k| (a[k] - b[k]).abs() < 1e-3);
    {
        let r = harness.state_mut().ready().unwrap();
        let p = r.placed.as_ref().unwrap();
        assert!(!p.dirty());
        let stored = r.model.stored_line().unwrap().clone();
        let insulators = ohl_structures::InsulatorLibrary::default();
        let back = p.structure(&stored, &insulators).unwrap();
        let s = stored.structure("T3").unwrap();
        assert_eq!(back.points.len(), s.points.len());
        for (a, b) in back.points.iter().zip(&s.points) {
            assert_eq!(a.label, b.label);
            assert!(near(a.position, b.position), "{a:?} {b:?}");
        }
        for (a, b) in back.wires.iter().zip(&s.wires) {
            assert_eq!((&a.label, a.set), (&b.label, b.set));
            assert!(near(a.position, b.position), "{a:?} {b:?}");
        }
        assert_eq!(back.strings.len(), s.strings.len());
        for (a, b) in back.strings.iter().zip(&s.strings) {
            assert_eq!(
                (&a.label, &a.top, &a.wire, a.strain, a.stiffness),
                (&b.label, &b.top, &b.wire, b.strain, b.stiffness)
            );
            assert!((a.weight - b.weight).abs() < 1e-6, "{a:?} {b:?}");
        }
    }
    // The first point 1 m higher.
    let (label, before) = {
        let r = harness.state_mut().ready().unwrap();
        let p = r.placed.as_mut().unwrap();
        let t = p.doc["point"].as_array_of_tables_mut().unwrap();
        let first = t.get_mut(0).unwrap();
        let z: f64 = first["point"]
            .as_array()
            .unwrap()
            .get(2)
            .unwrap()
            .as_str()
            .unwrap()
            .trim_end_matches(" m")
            .parse()
            .unwrap();
        let mut a = first["point"].as_array().unwrap().clone();
        a.replace(2, format!("{:.3} m", z + 1.0));
        first["point"] = toml_edit::value(a);
        let label = first["label"].as_str().unwrap().to_owned();
        let s = r.model.line.structure("T3").unwrap();
        let before = s.points.iter().find(|q| q.label == label).unwrap().position;
        (label, before)
    };
    harness.run();
    harness.get_by_label("Apply").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let s = r.model.line.structure("T3").unwrap();
        let after = s.points.iter().find(|q| q.label == label).unwrap().position;
        assert!(
            near(after, [before[0], before[1], before[2] + 1.0]),
            "{after:?}"
        );
        // Its insulators' wire points came along (their lengths hold).
        for x in s.strings.iter().filter(|x| x.top == label) {
            let w = s.wire(&x.wire).unwrap().position;
            let d = (0..3)
                .map(|k| (w[k] - after[k]).powi(2))
                .sum::<f64>()
                .sqrt();
            assert!((d - x.length).abs() < 1e-6, "{} {d}", x.label);
        }
        assert!(!r.placed.as_ref().unwrap().dirty());
        r.placed.as_mut().unwrap().type_id = Some("my-pylon".into());
    }
    harness.run();
    harness.get_by_label("Save and use").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(
        r.placed.is_none(),
        "{:?} {:?} {}",
        r.status,
        r.placed.as_ref().map(|p| p.message.clone()),
        r.placed
            .as_ref()
            .map_or(String::new(), |p| p.doc.to_string())
    );
    let s = r.model.line.structure("T3").unwrap();
    assert_eq!(s.structure_type.as_deref(), Some("my-pylon"));
    let after = s.points.iter().find(|q| q.label == label).unwrap().position;
    assert!(
        near(after, [before[0], before[1], before[2] + 1.0]),
        "{after:?}"
    );
    assert_eq!(s.strings.len(), 3);
    let stored = r.model.stored_line().unwrap().structure("T3").unwrap();
    assert!(stored.points.is_empty() && stored.strings.is_empty());
}

/// Two sets of a structure swapped in its editor (Set points): on Apply
/// the sections and jumpers strung there follow, in one command.
#[test]
fn swapped_sets_take_their_wires_along() {
    let model = synthetic_project("set-swap");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let (before, a, b) = {
        let r = harness.state_mut().ready().unwrap();
        let line = r.model.stored_line().unwrap().clone();
        let s = line.structure("T3").unwrap();
        let sets: Vec<u32> = s.sets().collect();
        let (a, b) = (sets[0], sets[1]);
        let mut p = crate::placed_editor::PlacedEditor::open(&line, "T3").unwrap();
        // Swap the two sets on their wire points.
        let wires = p.doc["wire"].as_array_of_tables_mut().unwrap();
        for w in wires.iter_mut() {
            let set = w.get("set").and_then(toml_edit::Item::as_integer);
            if set == Some(i64::from(a)) {
                w["set"] = toml_edit::value(i64::from(b));
            } else if set == Some(i64::from(b)) {
                w["set"] = toml_edit::value(i64::from(a));
            }
        }
        let insulators = ohl_structures::InsulatorLibrary::default();
        let (what, command) = p.command(&line, &insulators).unwrap();
        assert!(what.contains(&format!("{a}➡{b}")), "{what}");
        assert!(
            r.perform(crate::edit::Action::Command { what, command }),
            "{:?}",
            r.status
        );
        (line, a, b)
    };
    let r = harness.state_mut().ready().unwrap();
    let after = r.model.stored_line().unwrap();
    let swap = |p: &ohl_line::SectionPoint| {
        let mut p = p.clone();
        if p.structure == "T3" {
            p.set = if p.set == a {
                b
            } else if p.set == b {
                a
            } else {
                p.set
            };
        }
        p
    };
    for (x, y) in before.sections.iter().zip(&after.sections) {
        let want: Vec<ohl_line::SectionPoint> = x.points.iter().map(swap).collect();
        assert_eq!(y.points, want, "{}", x.id);
    }
    for (x, y) in before.jumpers.iter().zip(&after.jumpers) {
        assert_eq!(
            (swap(x.from.point().unwrap()), swap(x.to.point().unwrap())),
            (
                y.from.point().unwrap().clone(),
                y.to.point().unwrap().clone()
            )
        );
    }
}

/// A string's own wire point renamed in the string window: its string
/// follows each name typed, so its fields stay; an empty or taken name
/// waits (not applied) and is dropped when the field is left.
#[test]
fn a_wire_point_renamed_keeps_its_string() {
    use egui::Key;
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        r.editor = crate::editors::StringForm::of_string(&r.model, "T2", "S1")
            .map(crate::editors::Form::String);
    }
    harness.run();
    let form = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        let Some(crate::editors::Form::String(f)) = &r.editor else {
            panic!()
        };
        f.clone()
    };
    let f = form(&mut harness);
    let wire = f.strings[0].wire.clone();
    let other = f
        .points
        .iter()
        .map(|p| p.label.clone())
        .find(|l| *l != wire)
        .unwrap();
    fn field(v: &str) -> impl Fn(&egui_kittest::kittest::AccessKitNode<'_>) -> bool + '_ {
        move |n| n.role() == egui::accesskit::Role::TextInput && n.value().as_deref() == Some(v)
    }
    harness.get_by(field(&wire)).focus();
    harness.run();
    // Every letter away: the field is empty, the string keeps its wire
    // point and its fields.
    for _ in 0..wire.len() + 2 {
        harness.key_press(Key::Backspace);
        harness.run();
    }
    let f = form(&mut harness);
    assert!(
        f.wires.iter().any(|w| w.label == f.strings[0].wire),
        "{f:?}"
    );
    harness.get_by_label("clamp");
    harness.get_by_label("counterweight");
    // A taken name waits.
    // (typed into the field still focused)
    harness.event(egui::Event::Text(other.clone()));
    harness.run();
    let f = form(&mut harness);
    assert_ne!(f.strings[0].wire, other);
    assert!(f.wires.iter().any(|w| w.label == f.strings[0].wire));
    // A free one is applied, the string with it.
    harness.event(egui::Event::Text("-new".into()));
    harness.run();
    let f = form(&mut harness);
    let new = format!("{other}-new");
    assert_eq!(f.strings[0].wire, new);
    assert!(f.wires.iter().any(|w| w.label == new));
    harness.get_by_label("clamp");
}

/// A new project starts with its own empty wire library (not the
/// examples shipped with the program); weather cases are added in the
/// table, Apply keeps them in the project as its first case file.
#[test]
fn a_new_project_starts_empty_and_takes_weather_cases() {
    use crate::tables::TableKind;
    let dir = std::env::temp_dir().join(format!("ohl-app-{}-fresh-libs", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("fresh.ohl");
    std::fs::remove_file(&path).ok();
    crate::files::create_project(&path, "Fresh line", "EPSG:2056", "tester").unwrap();
    // Library files lie next to it and are given (as by a previous
    // project): not taken in.
    std::fs::copy(
        root().join("libraries/cables/acsr-examples.ohlw"),
        dir.join("around.ohlw"),
    )
    .unwrap();
    let sources = Sources {
        library: Some(root().join("libraries/cables")),
        weather: Some(root().join("examples/synthetic-line/weather-cases.ohlwc")),
        file: path.clone(),
        point_budget: 1000,
        corridor: 60.0,
        user: Some("tester".into()),
        ..Sources::default()
    };
    let model = model::load(sources.clone(), &|_| {}).unwrap();
    assert!(
        model.library.is_empty(),
        "{:?}",
        model.library.iter().map(|c| &c.id).collect::<Vec<_>>()
    );
    assert_eq!(model.cases.len(), 1);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let mut row = TableKind::WeatherCases.new_row(&r.model, r.case, &[]);
        row[1] = "cold".into();
        r.tables
            .entry(TableKind::WeatherCases)
            .or_default()
            .added
            .push(row);
    }
    apply_table(&mut harness, TableKind::WeatherCases, true).unwrap();
    let names = |m: &model::Model| {
        m.cases
            .iter()
            .map(|c| c.case.name.clone())
            .collect::<Vec<_>>()
    };
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(names(&r.model), ["15 °C", "cold"]);
    drop(harness);
    // Kept in the project: opened again, both cases are there.
    let model = model::load(sources, &|_| {}).unwrap();
    assert_eq!(names(&model), ["15 °C", "cold"]);
    assert!(model.library.is_empty());
}

/// The structure editor opens in a window of its own (it can be docked).
#[test]
fn the_structure_editor_opens_in_a_window() {
    let mut dock = crate::app::default_dock();
    crate::app::open_page(&mut dock, crate::app::Tab::StructureEditor);
    let at = dock.find_tab(&crate::app::Tab::StructureEditor).unwrap();
    assert!(!at.surface.is_main());
}

/// Clearances are in the Checks menu only (not under Sections): ground
/// clearances (the two windows) and wire clearances (a rule file's checks).
#[test]
fn clearances_are_in_the_checks_menu() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.get_by_label("Sections").click();
    harness.run();
    assert!(harness.query_by_label("Vertical clearances…").is_none());
    press(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    open_clearance_window(&mut harness, "Radial clearances…");
    assert!(harness.state_mut().ready().unwrap().radial_clearances.open);
    harness.get_by_label("Checks").click();
    harness.run();
    harness.get_by_label_contains("Wire clearances").hover();
    harness.run();
    harness.get_by_label("Show results");
    harness.get_by_label("also after every edit");
}

/// Tools › Add structure holds the tool; each use opens the window, one
/// Apply adds the structure (the mouse free after it); again for the next
/// one.
#[test]
fn add_structure_from_the_menu_takes_one_click_each() {
    let model = synthetic_project("add-structure-menu");
    let n = model.line.structures.len();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // First-person controls: a press in the 3D view captures the mouse.
    harness.state_mut().ready().unwrap().view3d.controls =
        crate::controls::Controls::by_name("minecraft").unwrap();
    for k in 1..=2 {
        harness.get_by_label("Tools").click();
        harness.run();
        harness.get_by_label("Add structure").click();
        harness.run();
        {
            let r = harness.state_mut().ready().unwrap();
            assert_eq!(r.hotbar.tool(), crate::tools::Tool::AddStructure);
            r.use_tool(crate::selection::Use {
                target: Selection::None,
                at: None,
                on: None,
            });
        }
        harness.run();
        assert!(
            harness.state_mut().ready().unwrap().editor.is_some(),
            "window {k} opened"
        );
        harness.get_by_label("Apply").click();
        harness.run();
        let r = harness.state_mut().ready().unwrap();
        assert!(!r.view3d.captured, "the mouse is free after Apply");
        assert!(r.editor.is_none(), "{:?}", r.status);
        assert_eq!(r.model.line.structures.len(), n + k, "{:?}", r.status);
    }
}

/// Opening a menu stops what was going on, as Escape would: the mouse is
/// freed, an alignment being laid out is given up.
#[test]
fn opening_a_menu_escapes_what_was_going_on() {
    let model = synthetic_project("menu-escapes");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        r.view3d.captured = true;
        r.new_alignment = vec![ohl_line::AlignmentPoint::free("", [0.0, 0.0, 500.0])];
    }
    harness.run();
    harness
        .get_all_by_label("Structures")
        .next()
        .unwrap()
        .click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(!r.view3d.captured);
    assert!(r.new_alignment.is_empty());
    harness.get_by_label("Spot structures…");
}

/// No tool twice: the Structures menu holds no tool nor the commands that
/// moved to the Tools menu (Add structure…, New pylon from a template…,
/// Place on alignment points…); the Tools menu has every tool and New
/// pylon from a template…, which opens the pylon builder.
#[test]
fn the_structures_menu_holds_no_tools() {
    use crate::tools::Tool;
    let model = synthetic_project("menus-no-duplicates");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let gone = [
        "Add structure…",
        "New structure from a template…",
        "Place on alignment points…",
    ];
    let names: Vec<&str> = Tool::ALL.iter().map(|t| t.name()).chain(gone).collect();
    // How often each name is shown with no menu open ("Edit" is a menu of
    // the bar).
    let count = |h: &Harness<'_, App>| -> Vec<usize> {
        names
            .iter()
            .map(|n| h.query_all_by_label_contains(n).count())
            .collect()
    };
    let closed = count(&harness);
    // (the menu bar's, before the Structures layer of the Model page)
    harness
        .get_all_by_label("Structures")
        .next()
        .unwrap()
        .click();
    harness.run();
    harness.get_by_label("Spot structures…");
    assert_eq!(count(&harness), closed, "{names:?}");
    harness.key_press(egui::Key::Escape);
    harness.run();
    harness.get_by_label("Tools").click();
    harness.run();
    let open = count(&harness);
    for (k, n) in names.iter().enumerate().take(Tool::ALL.len()) {
        assert!(open[k] > closed[k], "{n} in the Tools menu");
    }
    harness
        .get_by_label("New structure from a template…")
        .click();
    harness.run();
    assert!(harness.state_mut().ready().unwrap().pylon_builder.is_some());
}

/// A quick bar saved with a tool and New pylon from a template loads with
/// both (the menu entries removed from Structures were never quick-bar
/// commands; the tool's key is unchanged).
#[test]
fn saved_quick_bars_keep_the_structure_tools() {
    use crate::menu::{Command, QuickBar};
    use crate::tools::Tool;
    let mut storage = Memory::default();
    eframe::Storage::set_string(
        &mut storage,
        "quickbar",
        "undo,tool-add_structure,new-pylon,save".into(),
    );
    let bar = QuickBar::load(&storage).unwrap();
    assert_eq!(
        bar.0,
        [
            Command::Undo,
            Command::Hold(Tool::AddStructure),
            Command::NewPylon,
            Command::Save
        ]
    );
    let mut again = Memory::default();
    bar.save(&mut again);
    assert_eq!(QuickBar::load(&again), Some(bar));
}

/// Add structure on an alignment point with the base not on the ground:
/// it and the other points ticked stand at the points' own heights, the
/// window's type, extension and height adjustment on each (a row's own
/// type: its base); with no type at all, Apply says which point needs one.
#[test]
fn add_structure_on_more_points_keeps_the_window_s_choices() {
    use crate::editors::{Form, StructureForm};
    let model = typed_project_edited(
        "more-points",
        |_| {},
        |l| {
            l.alignments
                .push(ohl_line::Alignment::new("extra", "main", Vec::new()));
            l.set_alignment_points(
                "extra",
                [[0.0, -80.0], [300.0, -80.0], [600.0, -20.0]].map(|p| ([p[0], p[1], 777.0], None)),
            );
        },
    );
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let before = harness
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .structures
        .len();
    // A type with extensions: the window's extension goes with its type.
    let ty = "ext-02".to_owned();
    {
        let r = harness.state_mut().ready().unwrap();
        let m = &r.model;
        let mut f = StructureForm::new_at(m, [300.0, -80.0, 777.0]);
        assert_eq!(f.on_alignment, Some(("extra".into(), 1)), "the point near");
        f.on_ground = false;
        f.position[2] = 777.0;
        f.structure_type = None;
        let mut more = crate::placement::MorePoints::new(m, "extra", 1);
        assert_eq!(
            more.rows.iter().map(|r| r.index).collect::<Vec<_>>(),
            [0, 2]
        );
        for row in &mut more.rows {
            row.on = true;
        }
        f.more = Some(more);
        let e = Form::Structure(f.clone()).command(m, false).unwrap_err();
        assert!(e.contains("point 1"), "{e}");
        f.structure_type = Some(ty.clone());
        f.extension = Some("+3".into());
        f.height_adjust = 0.5;
        f.more.as_mut().unwrap().rows[1].structure_type = Some(ty.clone());
        r.editor = Some(Form::Structure(f));
    }
    harness.run();
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.editor.is_none(), "{:?}", r.status);
    let l = &r.model.line;
    assert_eq!(l.structures.len(), before + 3);
    let pts = l.points_of(l.alignment_by_id("extra").unwrap());
    let on: Vec<&ohl_line::Structure> = pts
        .iter()
        .map(|p| l.structure(p.structure.as_deref().expect("tied")).unwrap())
        .collect();
    for s in &on {
        assert!(
            (s.position[2] - 777.0).abs() < 1e-9,
            "{}: {:?}",
            s.id,
            s.position
        );
        assert_eq!(s.structure_type.as_deref(), Some(ty.as_str()));
    }
    // The type above: the window's extension and foundations; a row's own
    // type: its base.
    assert_eq!(
        (
            on[0].height_adjust,
            on[1].height_adjust,
            on[2].height_adjust
        ),
        (0.5, 0.5, 0.0)
    );
    let ext: Vec<Option<&str>> = on.iter().map(|s| s.extension.as_deref()).collect();
    assert_eq!(ext, [Some("+3"), Some("+3"), None]);
}

/// View › Profile aspect ratio: at 1:10 the heights are drawn ten times
/// larger than the stations, and stay so when zooming (the height under
/// the pointer stays under it); fitted, they follow what is shown.
#[test]
fn the_profile_keeps_its_aspect_ratio_when_zooming() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.state_mut().show_tab(crate::app::Tab::Profile);
    harness.run_steps(3);
    // Chosen in the menu.
    harness.get_by_label("View").click();
    harness.run();
    harness
        .get_by_label_contains("Profile aspect ratio")
        .hover();
    harness.run();
    harness.get_by_label("1:10 (heights ×10)").click();
    harness.run();
    press(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    assert_eq!(
        harness.state_mut().ready().unwrap().model.profile_ratio,
        Some(10.0)
    );
    assert_eq!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .model
            .view_prefs()
            .profile_ratio,
        Some(10.0)
    );
    harness.run_steps(2);
    let shown = |h: &mut Harness<'_, App>| h.state_mut().ready().unwrap().profile.shown.unwrap();
    // Heights per point = 10 × stations per point.
    let ratio = |(s, z, size): ([f64; 2], [f64; 2], [f64; 2])| {
        ((s[1] - s[0]) / size[0]) / ((z[1] - z[0]) / size[1])
    };
    let before = shown(&mut harness);
    assert!((ratio(before) - 10.0).abs() < 1e-6, "{before:?}");
    // Zoom in at a point of the profile.
    let rect = harness.state_mut().ready().unwrap().profile.rect.unwrap();
    let pos = rect.center() + egui::vec2(100.0, 40.0);
    harness.hover_at(pos);
    harness.run();
    harness.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, 200.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    harness.run_steps(10);
    let after = shown(&mut harness);
    assert!(
        after.0[1] - after.0[0] < before.0[1] - before.0[0],
        "zoomed in: {after:?}"
    );
    assert!((ratio(after) - 10.0).abs() < 1e-6, "{after:?}");
    // Fitted again: the heights follow what is shown.
    harness.state_mut().ready().unwrap().model.profile_ratio = None;
    harness.run_steps(2);
    let fitted = shown(&mut harness);
    assert!((ratio(fitted) - 10.0).abs() > 1e-3, "{fitted:?}");
}

/// A connection away from the alignment (80 m beside it between T1 and
/// T2, on the route because it gives it) does not take T1–T2 out of the
/// profile: its spans are drawn; the connection is shown, projected at
/// its station, only within the line width (±40 m by default: not; at
/// ±100 m: yes).
#[test]
fn the_profile_keeps_the_spans_along_the_alignment_past_a_connection() {
    let mut model = synthetic();
    let mut c = model.line.structure("T1").unwrap().clone();
    c.id = "C1".into();
    c.position = [150.0, -80.0, 490.0];
    c.routes = vec!["main".into()];
    model.line.structures.push(c);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.state_mut().show_tab(crate::app::Tab::Profile);
    harness.run_steps(3);
    {
        let r = harness.state_mut().ready().unwrap();
        let d = &r.profile.drawing.as_ref().unwrap().1.drawing;
        let ids: Vec<&str> = d.structures.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids[..2], ["T1", "T2"], "C1 outside ±40 m");
        r.drawings.set_line_width(100.0);
    }
    harness.run_steps(3);
    let r = harness.state_mut().ready().unwrap();
    let d = &r.profile.drawing.as_ref().unwrap().1.drawing;
    let ids: Vec<&str> = d.structures.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids[..3], ["T1", "C1", "T2"]);
    let m = &d.structures[1];
    assert!((m.station - 150.0).abs() < 1e-6 && (m.offset - 80.0).abs() < 1e-6);
    for section in ["S1", "S2"] {
        let spans: Vec<&str> = d
            .wires
            .iter()
            .filter(|w| w.label.starts_with(&format!("{section} span ")))
            .map(|w| w.label.split_whitespace().nth(2).unwrap())
            .collect();
        assert_eq!(spans, ["0", "1", "2", "3"], "{section}");
    }
}

/// The profile's corridor is the line width of the drawings (one
/// setting, ±40 m by default): the ground is drawn at both edges and the
/// survey points and LiDAR points within it are shown; at ±20 m (typed in
/// the profile) those within 20 m, and the export draws at ±20 m too.
/// "All alignments, in order" draws the first, then the second, each
/// starting where the one before ends.
#[test]
fn the_profile_shows_a_corridor_and_every_alignment_in_order() {
    let model = synthetic();
    let routes = model.line.alignment_views();
    assert!(routes.len() > 1);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.state_mut().show_tab(crate::app::Tab::Profile);
    harness.run_steps(3);
    let built = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        let b = &r.profile.drawing.as_ref().unwrap().1;
        (
            b.drawing.sides.len(),
            b.survey.len(),
            b.drawing.length,
            b.joints.clone(),
            b.drawing
                .structures
                .iter()
                .map(|m| (m.id.clone(), m.station))
                .collect::<Vec<_>>(),
        )
    };
    let (sides, wide, single, _, _) = built(&mut harness);
    assert_eq!(sides, 2);
    assert!(wide > 10, "{wide}");
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.drawings.line_width(), 40.0);
        let b = &r.profile.drawing.as_ref().unwrap().1;
        assert_eq!(b.drawing.sides[0].0.abs(), 40.0);
        // The LiDAR points loaded within ±40 m, projected (the synthetic
        // model has none: the same count as found by hand).
        let centre = b.drawing.centreline.clone();
        let near = r
            .model
            .points
            .iter()
            .filter(|(q, _)| {
                let p = [0, 1, 2].map(|k| r.model.origin[k] + f64::from(q[k]) / 1000.0);
                let (st, off) = ohl_line::route::project(&centre, p);
                let back = ohl_line::route::point_at(&centre, st, off);
                off.abs() <= 40.0 && (back[0] - p[0]).hypot(back[1] - p[1]) < 0.4
            })
            .count();
        assert_eq!(b.lidar.len(), near);
    }
    // ±20 m, typed in the profile's own field: the drawings' line width.
    harness.get_by_label("line width ±");
    harness
        .state_mut()
        .ready()
        .unwrap()
        .drawings
        .set_line_width(20.0);
    harness.run_steps(3);
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.drawings.settings().line_width, 20.0);
        assert_eq!(r.drawings.stored().line_width, 20.0);
        let w = crate::drawings::DrawingsWindow {
            out: Some(std::env::temp_dir()),
            ..r.drawings.settings().clone()
        };
        let input = crate::drawings::Input::new(&r.model, r.case, &w).unwrap();
        assert_eq!(input.options.plan_half_width, 20.0);
    }
    let (sides, survey, _, _, _) = built(&mut harness);
    assert!(survey < wide, "{survey} < {wide}");
    assert_eq!(sides, 2);
    assert!(survey > 10, "{survey}");
    let in_corridor = {
        let r = harness.state_mut().ready().unwrap();
        let b = &r.profile.drawing.as_ref().unwrap().1;
        let centre = b.drawing.centreline.clone();
        let pts = r.model.ground.as_ref().unwrap().points().to_vec();
        pts.iter()
            .filter(|p| {
                let (st, off) = ohl_line::route::project(&centre, **p);
                let back = ohl_line::route::point_at(&centre, st, off);
                off.abs() <= 20.0 && (back[0] - p[0]).hypot(back[1] - p[1]) < 0.2
            })
            .count()
    };
    assert_eq!(survey, in_corridor);
    // A 3D drawing line across the line, 5 m above the ground, comes in;
    // a flat (2D) one does not; hidden, it is not drawn (kept, filtered).
    {
        let r = harness.state_mut().ready().unwrap();
        let b = &r.profile.drawing.as_ref().unwrap().1;
        let c = &b.drawing.centreline;
        let mid = [0.5 * (c[0][0] + c[1][0]), 0.5 * (c[0][1] + c[1][1])];
        let (dx, dy) = (c[1][0] - c[0][0], c[1][1] - c[0][1]);
        let n = dx.hypot(dy);
        let across = [-dy / n, dx / n];
        let line = |z: f64| ohl_background::dxf::Polyline {
            layer: 0,
            color: [255, 0, 255],
            points: (-4..=4)
                .map(|k| {
                    let t = f64::from(k) * 10.0;
                    [mid[0] + across[0] * t, mid[1] + across[1] * t, z]
                })
                .collect(),
            closed: false,
        };
        let drawing = ohl_background::dxf::Drawing {
            layers: Vec::new(),
            lines: vec![line(505.0), line(0.0)],
            points: Vec::new(),
            texts: Vec::new(),
            skipped: Default::default(),
        };
        r.model
            .backgrounds
            .drawings
            .push(crate::backgrounds::LoadedDrawing {
                info: ohl_background::DrawingInfo {
                    name: "invented-3d".into(),
                    source: String::new(),
                    crs: String::new(),
                    items: 2,
                    bounds: None,
                },
                boxes: Vec::new(),
                drawing: std::sync::Arc::new(drawing),
            });
        r.profile.invalidate();
    }
    harness.run_steps(3);
    {
        let r = harness.state_mut().ready().unwrap();
        let b = &r.profile.drawing.as_ref().unwrap().1;
        assert_eq!(b.lines.len(), 1, "the 3D line only");
        // Its part within ±20 m: 5 points (−20 … 20 m).
        assert_eq!(b.lines[0].2.len(), 5);
        assert!(b.lines[0].2.iter().all(|p| p[1] == 505.0));
    }
    // Every alignment, in order.
    {
        let r = harness.state_mut().ready().unwrap();
        r.profile.show = Some(crate::profile::Show::All);
    }
    harness.run_steps(3);
    let (_, _, total, joints, structures) = built(&mut harness);
    assert_eq!(joints.len(), routes.len());
    assert_eq!(joints[0].1, format!("alignment {}", routes[0].id));
    assert_eq!(joints[1].1, format!("alignment {}", routes[1].id));
    assert!(total > single);
    // The second alignment's structures come after the first's.
    let first_end = joints[1].0;
    for id in &routes[1].structures {
        assert!(
            structures
                .iter()
                .any(|(s, at)| s == id && *at >= first_end - 1e-6),
            "{id}: {structures:?}"
        );
    }
}

/// Terrain › Routes: a route added, one renamed (its alignments follow),
/// one deleted (asked: its alignment goes with it). Structures › routes:
/// from the alignments (dimmed), or ticked for one on no alignment.
#[test]
fn routes_are_made_renamed_and_deleted_and_structures_take_them() {
    use crate::tables::TableKind;
    let model = synthetic_project("routes-table");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let rows = TableKind::Routes.rows(&r.model, 0);
        let main = rows.iter().find(|x| x.key == "main").unwrap();
        assert_eq!(main.cells[2], "main");
        assert_eq!(main.cells[3], "5", "T1–T5");
        let mut row = TableKind::Routes.new_row(&r.model, 0, &[]);
        assert_eq!(row[0], "route 1");
        row[0] = "north".into();
        row[1] = "North route".into();
        r.tables
            .entry(TableKind::Routes)
            .or_default()
            .added
            .push(row);
    }
    set_cell(&mut harness, TableKind::Routes, "branch", "route", "west");
    apply_table(&mut harness, TableKind::Routes, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let line = &r.model.line;
        assert!(
            line.routes
                .iter()
                .any(|x| x.id == "north" && x.name == "North route")
        );
        let branch = line.alignments.iter().find(|a| a.id == "branch").unwrap();
        assert_eq!(branch.route, "west");
        assert_eq!(line.routes_of("T6"), ["west"]);
    }
    // T6 on north too, ticked in the Structures table.
    {
        let r = harness.state_mut().ready().unwrap();
        let rows = TableKind::Structures.rows(&r.model, 0);
        let t6 = rows.iter().find(|x| x.key == "T6").unwrap();
        let col = TableKind::Structures.column("routes");
        assert_eq!(t6.cells[col], "west");
        assert!(t6.defaulted[col], "from the alignment: dimmed");
    }
    set_cell(
        &mut harness,
        TableKind::Structures,
        "T6",
        "routes",
        "west, north",
    );
    apply_table(&mut harness, TableKind::Structures, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(
            r.model.line.structure("T6").unwrap().routes,
            ["west", "north"]
        );
        assert_eq!(r.model.line.routes_of("T6"), ["north", "west"]);
    }
    // Deleting west: asked, its alignment goes; T6 keeps north.
    {
        let r = harness.state_mut().ready().unwrap();
        r.tables
            .entry(TableKind::Routes)
            .or_default()
            .deleted
            .insert("west".into());
    }
    assert_eq!(
        apply_table(&mut harness, TableKind::Routes, false).unwrap_err(),
        "needs a confirmation"
    );
    apply_table(&mut harness, TableKind::Routes, true).unwrap();
    let r = harness.state_mut().ready().unwrap();
    let line = &r.model.line;
    assert!(line.routes.iter().all(|x| x.id != "west"));
    assert!(line.alignments.iter().all(|a| a.id != "branch"));
    assert_eq!(line.structure("T6").unwrap().routes, ["north"]);
}

/// View › Work on route: the branch chosen, its structures, sections and
/// jumpers are what the checks and loads keep to; the profile shows it;
/// remembered with the display choices.
#[test]
fn work_on_one_route() {
    let model = synthetic_project("work-on-route");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.get_by_label("View").click();
    harness.run();
    harness.get_by_label_contains("Work on route").hover();
    harness.run();
    harness.get_by_label("branch").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.model.work_route.as_deref(), Some("branch"));
    assert_eq!(r.model.view_prefs().work_route.as_deref(), Some("branch"));
    assert_eq!(
        r.profile.show,
        Some(crate::profile::Show::Route("branch".into()))
    );
    let scope = r.model.scope().unwrap();
    assert_eq!(
        scope
            .structures
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["T3", "T6", "T7"]
    );
    // The branch's sections, and the jumper from main into it (at T3).
    let (sections, _) = r.model.line.wire_routes();
    for (id, routes) in sections {
        assert_eq!(
            scope.sections.contains(&id),
            routes.contains(&"branch".to_owned()),
            "{id}"
        );
    }
    assert!(!scope.jumpers.is_empty());
}

/// Add string on a point: a structure point gets a new string straight
/// down from it (a new set), a wire point a string below it (a TAK); a
/// window opened or changed by a right-click opens at the mouse; a
/// right-click on nothing says so.
#[test]
fn add_string_hangs_from_the_point_clicked() {
    use crate::editors::Form;
    use crate::tools::Tool;
    let model = synthetic_project("add-string-point");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let (top, sets) = {
        let r = harness.state_mut().ready().unwrap();
        let s = r.model.line.structure("T2").unwrap();
        (s.points[0].clone(), s.sets().max().unwrap())
    };
    let point = |label: &str| Selection::Point {
        structure: "T2".into(),
        label: label.into(),
    };
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(egui::pos2(300.0, 300.0)));
    harness.run();
    use_tool(&mut harness, Tool::AddString, point(&top.label), None);
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::String(f)) = &r.editor else {
            panic!("{:?} {:?}", r.editor, r.status);
        };
        assert_eq!(r.popup_at, egui::pos2(300.0, 300.0));
        assert_eq!(f.strings.len(), 1);
        assert_eq!(f.strings[0].top, top.label);
        let w = &f.wires[0];
        assert_eq!(w.set, Some(sets + 1));
        let at = f.points.iter().find(|p| p.label == top.label).unwrap().at;
        assert!((at[2] - w.at[2] - 3.0).abs() < 1e-9 && (at[0] - w.at[0]).abs() < 1e-9);
    }
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    let wire = {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        let s = r.model.line.structure("T2").unwrap();
        let w = s.wire_at_set(sets + 1).expect("the new set");
        let x = s.strings_to(&w.label).next().unwrap();
        assert_eq!(x.top, top.label);
        w.label.clone()
    };
    // From the new wire point: a string below it, the next set.
    use_tool(&mut harness, Tool::AddString, point(&wire), None);
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::String(f)) = &r.editor else {
            panic!("{:?} {:?}", r.editor, r.status);
        };
        let x = f
            .strings
            .iter()
            .find(|x| x.top == wire)
            .expect("hangs from it");
        let w = f.wires.iter().find(|w| w.label == x.wire).unwrap();
        assert_eq!(w.set, Some(sets + 2));
    }
    press(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    // On the structure itself: from one of its points (no new point).
    use_tool(
        &mut harness,
        Tool::AddString,
        Selection::Structure("T2".into()),
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::String(f)) = &r.editor else {
            panic!("{:?} {:?}", r.editor, r.status);
        };
        let s = r.model.line.structure("T2").unwrap();
        assert!(s.point(&f.strings[0].top).is_some(), "{:?}", f.strings[0]);
        assert!(f.points.iter().all(|p| s.point(&p.label).is_some()));
    }
    // Another object right-clicked while it is open: the window follows
    // the mouse.
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(egui::pos2(700.0, 500.0)));
    harness.run();
    use_tool(
        &mut harness,
        Tool::Edit,
        Selection::Span {
            section: "S1".into(),
            span: 0,
        },
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(matches!(r.editor, Some(Form::Section(_))), "{:?}", r.editor);
        assert_eq!(r.popup_at, egui::pos2(700.0, 500.0));
    }
    use_tool(&mut harness, Tool::Edit, Selection::None, None);
    let r = harness.state_mut().ready().unwrap();
    assert!(
        r.status
            .as_ref()
            .is_some_and(|(t, _)| t.contains("nothing here")),
        "{:?}",
        r.status
    );
}

/// A string hung from another point keeps its length: its wire point (and
/// what hangs below) moves with it; a V's leg keeps both legs' lengths,
/// refused where they cannot meet; a string cannot hang below itself.
#[test]
fn a_string_hung_elsewhere_keeps_its_length() {
    use crate::editors::{PointRow, Preset, StringForm};
    let model = synthetic();
    let dist = |a: [f64; 3], b: [f64; 3]| (0..3).map(|i| (b[i] - a[i]).powi(2)).sum::<f64>().sqrt();
    let at = |f: &StringForm, l: &str| {
        f.points
            .iter()
            .find(|p| p.label == l)
            .map(|p| p.at)
            .or_else(|| f.wires.iter().find(|w| w.label == l).map(|w| w.at))
            .unwrap()
    };
    let point = |f: &mut StringForm, label: &str, p: [f64; 3]| {
        f.points.push(PointRow {
            label: label.into(),
            at: p,
            description: String::new(),
        });
    };
    // An I-string with a string below it (V + string): moved, all move.
    let mut f = StringForm::new_on(&model, "T2").unwrap();
    f.preset(Preset::VAndString, [0.0, 0.0, 20.0], &model);
    point(&mut f, "X", [1.0, 0.5, 25.0]);
    let below = f
        .strings
        .iter()
        .position(|x| f.wires.iter().any(|w| w.label == x.top))
        .unwrap();
    let (tip, low) = (f.strings[below].top.clone(), f.strings[below].wire.clone());
    let (l0, low0) = (dist(at(&f, &tip), at(&f, &low)), at(&f, &low));
    f.move_top(below, "X").unwrap();
    assert_eq!(f.strings[below].top, "X");
    assert!((dist(at(&f, "X"), at(&f, &low)) - l0).abs() < 1e-9);
    assert!(dist(at(&f, &low), low0) > 1.0, "it moved with its top");
    // A V's leg: both lengths kept, the tip where they meet.
    let legs: Vec<usize> = (0..f.strings.len())
        .filter(|&i| f.strings[i].wire == tip)
        .collect();
    let lens: Vec<f64> = legs
        .iter()
        .map(|&i| dist(at(&f, &f.strings[i].top), at(&f, &tip)))
        .collect();
    let a = at(&f, &f.strings[legs[0]].top);
    point(&mut f, "Y", [a[0] - 0.5, a[1], a[2]]);
    f.move_top(legs[0], "Y").unwrap();
    for (n, &i) in legs.iter().enumerate() {
        let l = dist(at(&f, &f.strings[i].top), at(&f, &tip));
        assert!((l - lens[n]).abs() < 1e-9, "{l} {}", lens[n]);
    }
    // Too far for the two legs: refused, nothing changes.
    point(&mut f, "FAR", [40.0, 0.0, 20.0]);
    let before = f.clone();
    let why = f.move_top(legs[0], "FAR").unwrap_err();
    assert!(why.contains("cannot meet"), "{why}");
    assert_eq!(f, before);
    // Not from its own wire point, nor from below it.
    assert!(f.move_top(legs[0], &tip).is_err());
    let s = f.strings.iter().position(|x| x.wire == low).unwrap();
    f.move_top(s, &tip).unwrap();
    assert!(f.move_top(legs[0], &low).is_err());
}

/// Edit on a structure opens its structure screen (as Strings and points…):
/// one without a type in the structure editor; its placement window is
/// Placement… in Properties.
#[test]
fn edit_on_a_structure_opens_its_pylon() {
    use crate::editors::Form;
    use crate::tools::Tool;
    let model = synthetic_project("edit-pylon");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let id = {
        let r = harness.state_mut().ready().unwrap();
        let s = r
            .model
            .line
            .structures
            .iter()
            .find(|s| s.structure_type.is_none());
        s.expect("a structure without a type").id.clone()
    };
    use_tool(
        &mut harness,
        Tool::Edit,
        Selection::Structure(id.clone()),
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.editor);
        assert_eq!(r.placed.as_ref().map(|p| p.id.as_str()), Some(id.as_str()));
    }
    let r = harness.state_mut().ready().unwrap();
    r.perform(crate::edit::Action::EditPlacement { id: id.clone() });
    assert!(matches!(&r.editor, Some(Form::Structure(f)) if f.id == id));
}

/// A tool window opens next to the click even where a window of its id
/// was left elsewhere in an earlier run (window places are remembered
/// between runs): here far off the screen.
#[test]
fn a_tool_window_opens_at_the_click_not_where_it_was_before() {
    let model = synthetic_project("window-place");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    // An earlier run left the first editor window far off the screen.
    let ctx = harness.ctx.clone();
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
        egui::Window::new("stale")
            .id(egui::Id::new(("editor".to_owned(), 1_u64)))
            .constrain(false)
            .current_pos(egui::pos2(3000.0, 2000.0))
            .show(ui.ctx(), |ui| ui.label("stale"));
    });
    out.textures_delta.clear();
    harness.run();
    harness
        .input_mut()
        .events
        .push(egui::Event::PointerMoved(egui::pos2(500.0, 400.0)));
    harness.run();
    use_tool(
        &mut harness,
        crate::tools::Tool::Edit,
        Selection::Span {
            section: "S1".into(),
            span: 0,
        },
        None,
    );
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.editor.is_some());
    let state = egui::AreaState::load(&harness.ctx, egui::Id::new(("editor".to_owned(), 1_u64)))
        .expect("the window");
    let at = state.pivot_pos.unwrap();
    assert!((at - egui::pos2(516.0, 416.0)).length() < 1.0, "{at:?}");
}

/// A string's length typed: kept, with every other string's — a V's leg
/// moves the point where the legs meet, the string below keeps its length
/// and moves along; a length the legs cannot meet with is refused.
#[test]
fn a_string_length_typed_keeps_the_others() {
    use crate::editors::{Preset, StringForm};
    let model = synthetic();
    let dist = |a: [f64; 3], b: [f64; 3]| (0..3).map(|i| (b[i] - a[i]).powi(2)).sum::<f64>().sqrt();
    let at = |f: &StringForm, l: &str| {
        f.points
            .iter()
            .find(|p| p.label == l)
            .map(|p| p.at)
            .or_else(|| f.wires.iter().find(|w| w.label == l).map(|w| w.at))
            .unwrap()
    };
    let len = |f: &StringForm, k: usize| dist(at(f, &f.strings[k].top), at(f, &f.strings[k].wire));
    let mut f = StringForm::new_on(&model, "T2").unwrap();
    f.preset(Preset::VAndString, [0.0, 0.0, 20.0], &model);
    let below = f
        .strings
        .iter()
        .position(|x| f.wires.iter().any(|w| w.label == x.top))
        .unwrap();
    let tip = f.strings[below].top.clone();
    let legs: Vec<usize> = (0..f.strings.len())
        .filter(|&i| f.strings[i].wire == tip)
        .collect();
    let (l0, l1, lb) = (len(&f, legs[0]), len(&f, legs[1]), len(&f, below));
    let tip0 = at(&f, &tip);
    f.set_length(legs[0], l0 + 0.5).unwrap();
    assert!((len(&f, legs[0]) - (l0 + 0.5)).abs() < 1e-9);
    assert!(
        (len(&f, legs[1]) - l1).abs() < 1e-9,
        "the other leg keeps its length"
    );
    assert!(
        (len(&f, below) - lb).abs() < 1e-9,
        "the string below keeps its length"
    );
    assert!(dist(at(&f, &tip), tip0) > 0.1, "where they meet moved");
    // Too short to reach the other leg: refused, nothing changes.
    let before = f.clone();
    let apart = dist(
        at(&f, &f.strings[legs[0]].top),
        at(&f, &f.strings[legs[1]].top),
    );
    let why = f.set_length(legs[0], (apart - l1) * 0.5).unwrap_err();
    assert!(why.contains("cannot meet"), "{why}");
    assert_eq!(f, before);
    // The string below (an I): its wire point along it.
    f.set_length(below, lb + 1.0).unwrap();
    assert!((len(&f, below) - (lb + 1.0)).abs() < 1e-9);
    assert!((len(&f, legs[0]) - (l0 + 0.5)).abs() < 1e-9);
}

/// Ends at: any other wire point of the structure (strings chain: a V
/// under a V), keeping its length — a second leg makes a V where both
/// lengths meet; a third leg, a loop or the wire point of another set
/// pulled in.
#[test]
fn a_string_ends_at_any_other_wire_point() {
    use crate::editors::{Preset, StringForm, StringRow, WireRow};
    let model = synthetic();
    let dist = |a: [f64; 3], b: [f64; 3]| (0..3).map(|i| (b[i] - a[i]).powi(2)).sum::<f64>().sqrt();
    let at = |f: &StringForm, l: &str| {
        f.points
            .iter()
            .find(|p| p.label == l)
            .map(|p| p.at)
            .or_else(|| f.wires.iter().find(|w| w.label == l).map(|w| w.at))
            .unwrap()
    };
    let len = |f: &StringForm, l: &str| {
        let x = f.strings.iter().find(|x| x.label == l).unwrap();
        dist(at(f, &x.top), at(f, &x.wire))
    };
    let k = |f: &StringForm, l: &str| f.strings.iter().position(|x| x.label == l).unwrap();
    // An upper V (legs to its tip, no set yet).
    let mut f = StringForm::new_on(&model, "T2").unwrap();
    f.preset(Preset::V, [0.0, 0.0, 20.0], &model);
    let tip = f.strings[0].wire.clone();
    let side = f.strings[1].top.clone();
    // Two more strings, each with its own wire point: one from the tip,
    // one from a top of the upper V.
    let add = |f: &mut StringForm, label: &str, top: &str, d: [f64; 3]| {
        let t = at(f, top);
        let wire = format!("{label}-W");
        f.wires.push(WireRow {
            description: String::new(),
            label: wire.clone(),
            at: [t[0] + d[0], t[1] + d[1], t[2] + d[2]],
            set: None,
            weight: 0.0,
            dead_end: false,
            dead_end_was: false,
        });
        f.strings.push(StringRow {
            label: label.into(),
            top: top.into(),
            wire,
            ..Default::default()
        });
    };
    add(&mut f, "X1", &tip, [0.0, 0.0, -2.0]);
    add(&mut f, "X2", &side, [-1.0, 0.0, -4.0]);
    let (l1, l2) = (len(&f, "X1"), len(&f, "X2"));
    // X2 ends at X1's wire point: a lower V, both keeping their lengths.
    let low = "X1-W".to_owned();
    assert!(f.end_choices(&model, k(&f, "X2")).contains(&low));
    f.end_at(&model, k(&f, "X2"), &low).unwrap();
    assert_eq!(f.strings[k(&f, "X2")].wire, low);
    assert!((len(&f, "X1") - l1).abs() < 1e-9 && (len(&f, "X2") - l2).abs() < 1e-9);
    assert!(
        f.wires.iter().all(|w| w.label != "X2-W"),
        "its own wire point went"
    );
    // A third leg there: refused, nothing changes.
    add(&mut f, "X3", &side, [1.0, 0.0, -3.0]);
    let before = f.clone();
    let why = f.end_at(&model, k(&f, "X3"), &low).unwrap_err();
    assert!(why.contains("two strings meet there"), "{why}");
    assert_eq!(f, before);
    // A leg of the upper V cannot end below itself (a loop).
    let leg = f.strings[0].label.clone();
    assert!(!f.end_choices(&model, k(&f, &leg)).contains(&low));
    assert!(f.end_at(&model, k(&f, &leg), &low).is_err());
    // X1 alone holds the lower V's tip: sent to end at X3's wire point,
    // what hangs from the tip (nothing here) and X2 stay; with a string
    // hanging below the tip, it is hung from the new end, lengths kept.
    add(&mut f, "Y", &low, [0.0, 0.0, -1.5]);
    let ly = len(&f, "Y");
    let x2 = k(&f, "X2");
    f.strings.remove(x2);
    let x3w = f.strings[k(&f, "X3")].wire.clone();
    f.end_at(&model, k(&f, "X1"), &x3w).unwrap();
    assert_eq!(f.strings[k(&f, "Y")].top, x3w, "hung from the new end");
    assert!((len(&f, "Y") - ly).abs() < 1e-9, "its length kept");
    assert!((len(&f, "X1") - l1).abs() < 1e-9);
    assert!(f.wires.iter().all(|w| w.label != low), "the old end went");
    // A wire point of another set of the structure: taken into the window.
    let s = model.line.structure("T2").unwrap();
    let other = s
        .wires
        .iter()
        .find(|w| w.set.is_some())
        .unwrap()
        .label
        .clone();
    assert!(f.end_choices(&model, k(&f, "X3")).contains(&other));
    let r = f.end_at(&model, k(&f, "X3"), &other);
    assert!(
        f.wires.iter().any(|w| w.label == other) || r.is_err(),
        "{r:?}"
    );
    if r.is_ok() {
        assert!(f.opened.1.contains(&other));
    }
}

/// A V under a V built in the string window is applied and solved.
#[test]
fn a_v_under_a_v_is_applied() {
    use crate::editors::{Form, PointRow, StringForm, StringRow, WireRow};
    let model = synthetic_project("v-under-v");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let mut f = StringForm::of(&r.model, "T2", 1).unwrap();
        let x0 = f.strings[0].clone();
        let tip = x0.wire.clone();
        // Two new points on the structure, either side of the first top.
        let top0 = f.points.iter().find(|p| p.label == x0.top).unwrap().at;
        let tops = ["PA".to_owned(), "PB".to_owned()];
        for (label, dx) in [("PA", 2.5), ("PB", -2.5)] {
            f.points.push(PointRow {
                label: label.into(),
                at: [top0[0] + dx, top0[1], top0[2]],
                description: String::new(),
            });
        }
        let at = |f: &StringForm, l: &str| {
            f.points
                .iter()
                .find(|p| p.label == l)
                .map(|p| p.at)
                .or_else(|| f.wires.iter().find(|w| w.label == l).map(|w| w.at))
                .unwrap()
        };
        let add = |f: &mut StringForm, label: &str, top: &str, end: [f64; 3]| {
            let wire = format!("{label}-W");
            f.wires.push(WireRow {
                description: String::new(),
                label: wire.clone(),
                at: end,
                set: None,
                weight: 0.0,
                dead_end: false,
                dead_end_was: false,
            });
            f.strings.push(StringRow {
                label: label.into(),
                top: top.into(),
                wire,
                weight: 300.0,
                ..Default::default()
            });
        };
        // Upper V: a second leg to the set's wire point, from another
        // point (its length: to where the first ends).
        let w = at(&f, &tip);
        add(&mut f, "UV", &tops[0], w);
        let n = f.strings.len() - 1;
        f.end_at(&r.model, n, &tip).unwrap();
        // Lower V: from the upper tip and from another point, meeting
        // below; the set moved down to it.
        let t = at(&f, &tip);
        add(&mut f, "L1", &tip, [t[0], t[1], t[2] - 2.0]);
        add(&mut f, "L2", &tops[1], [t[0], t[1], t[2] - 2.0]);
        let k2 = f.strings.len() - 1;
        f.end_at(&r.model, k2, "L1-W").unwrap();
        for w in &mut f.wires {
            w.set = if w.label == "L1-W" {
                Some(1)
            } else if w.label == tip {
                None
            } else {
                w.set
            };
        }
        r.editor = Some(Form::String(f));
    }
    harness.run();
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    let r = harness.state_mut().ready().unwrap();
    assert!(r.editor.is_none(), "{:?}", r.status);
    let s = r.model.line.structure("T2").unwrap();
    assert_eq!(s.strings_to("L1-W").count(), 2);
    assert!(r.model.shape(&r.model.line.sections[0].id, 0).is_some());
}

/// Chains of strings of any length end in a V — four strings on one side,
/// five on the other, meeting at the set's wire point — applied, and
/// solved in every weather case (high wind: by relaxation, then Newton).
#[test]
fn chains_of_strings_end_in_a_v() {
    use crate::editors::{Form, PointRow, StringForm, StringRow, WireRow};
    let model = synthetic_project("chains");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let mut f = StringForm::of(&r.model, "T2", 1).unwrap();
        let x0 = f.strings[0].clone();
        let top0 = f.points.iter().find(|p| p.label == x0.top).unwrap().at;
        let w0 = f.wires.iter().find(|w| w.label == x0.wire).unwrap().at;
        f.strings.clear();
        let n: [usize; 2] = [4, 5];
        // Two chains from points 3 m either side, each hanging straight
        // from its point to the tip's height, then the last strings meet.
        let depth = top0[2] - w0[2] + 3.0;
        let mut ends = Vec::new();
        for (side, dx, count) in [("A", 3.0, n[0]), ("B", -3.0, n[1])] {
            let label = format!("P{side}");
            let p = [top0[0] + dx, top0[1], top0[2]];
            f.points.push(PointRow {
                label: label.clone(),
                at: p,
                description: String::new(),
            });
            let mut top = label;
            for i in 0..count {
                let wire = format!("{side}{i}-W");
                let fr = (i + 1) as f64 / count as f64;
                // Straight from the point towards the tip's spot.
                let at = [p[0] - dx * fr, p[1], p[2] - depth * fr];
                f.wires.push(WireRow {
                    description: String::new(),
                    label: wire.clone(),
                    at,
                    set: None,
                    weight: 0.0,
                    dead_end: false,
                    dead_end_was: false,
                });
                f.strings.push(StringRow {
                    label: format!("{side}{i}"),
                    top: top.clone(),
                    wire: wire.clone(),
                    weight: 100.0,
                    ..Default::default()
                });
                top = wire;
            }
            ends.push(top);
        }
        let k = f
            .strings
            .iter()
            .position(|x| x.label == format!("B{}", n[1] - 1))
            .unwrap();
        f.end_at(&r.model, k, &ends[0]).unwrap();
        f.wires.retain(|w| w.label != x0.wire);
        for w in &mut f.wires {
            if w.label == ends[0] {
                w.set = Some(1);
            }
        }
        r.editor = Some(Form::String(f));
    }
    harness.run();
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    let r = harness.state_mut().ready().unwrap();
    assert!(r.editor.is_none(), "{:?}", r.status);
    let s = r.model.line.structure("T2").unwrap();
    assert_eq!(s.strings_to("A3-W").count(), 2);
    for c in 0..r.model.cases.len() {
        r.model.ensure_solved(c);
        assert!(
            r.model.cases[c].failed.is_empty(),
            "case {c}: {:?}",
            r.model.cases[c].failed
        );
    }
}

/// The new string in the open string window given 900 N.
fn with_weight(h: &mut Harness<'_, App>) {
    let r = h.state_mut().ready().unwrap();
    let Some(crate::editors::Form::String(f)) = &mut r.editor else {
        panic!("{:?}", r.editor);
    };
    let row = f.strings.last_mut().expect("the new string");
    assert_eq!(row.weight, 0.0);
    row.weight = 900.0;
}

/// A string added with a weight gets a library label in the same command:
/// its entry written into the project's library (same values: the same
/// entry for the next one), one undo taking both back. One added without a
/// weight (0 N) stays unlabelled: no 0 N entry (2026-10-21).
#[test]
fn a_new_string_is_labelled_from_the_library() {
    use crate::tools::Tool;
    let model = synthetic_project("string-library");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let top = {
        let r = harness.state_mut().ready().unwrap();
        r.model.line.structure("T2").unwrap().points[0]
            .label
            .clone()
    };
    let point = Selection::Point {
        structure: "T2".into(),
        label: top.clone(),
    };
    // Without a weight first: no label, no entry.
    let before = {
        let r = harness.state_mut().ready().unwrap();
        r.model.types.as_ref().map_or(0, |t| t.1.iter().count())
    };
    use_tool(&mut harness, Tool::AddString, point.clone(), None);
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        let s = r.model.stored_line().unwrap().structure("T2").unwrap();
        let x = s
            .strings
            .iter()
            .rfind(|x| x.top == top)
            .expect("the new string");
        assert_eq!(x.weight, 0.0);
        assert!(x.insulator.is_none(), "{:?}", x.insulator);
        let lib = &r.model.types.as_ref().expect("libraries").1;
        assert!(
            lib.iter().all(|a| a.weight.si() > 0.0),
            "no 0 N entry ({before} entries before)"
        );
        r.perform(crate::edit::Action::Undo);
    }
    use_tool(&mut harness, Tool::AddString, point.clone(), None);
    with_weight(&mut harness);
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    let (label, entries) = {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        let s = r
            .model
            .stored_line()
            .unwrap()
            .structure("T2")
            .unwrap()
            .clone();
        let x = s
            .strings
            .iter()
            .rfind(|x| x.top == top)
            .expect("the new string")
            .clone();
        let label = x.insulator.clone().expect("a library label");
        let lib = &r.model.types.as_ref().expect("libraries").1;
        assert!(lib.get(&label).is_some(), "{label} in the library");
        // Every own string of T2 with a weight is labelled.
        assert!(
            s.strings
                .iter()
                .all(|x| x.insulator.is_some() || x.weight == 0.0)
        );
        (label, lib.iter().count())
    };
    // The same again: the same entry.
    use_tool(&mut harness, Tool::AddString, point, None);
    with_weight(&mut harness);
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        let lib = &r.model.types.as_ref().unwrap().1;
        assert_eq!(lib.iter().count(), entries);
        let s = r.model.stored_line().unwrap().structure("T2").unwrap();
        assert_eq!(
            s.strings
                .iter()
                .filter(|x| x.insulator.as_deref() == Some(&label))
                .count(),
            2
        );
    }
    // One undo: the second string goes, with its label.
    let n = {
        let r = harness.state_mut().ready().unwrap();
        r.model
            .stored_line()
            .unwrap()
            .structure("T2")
            .unwrap()
            .strings
            .len()
    };
    let r = harness.state_mut().ready().unwrap();
    r.perform(crate::edit::Action::Undo);
    let s = r.model.stored_line().unwrap().structure("T2").unwrap();
    assert_eq!(s.strings.len(), n - 1);
}

/// The string library as a table: on opening, every own string is
/// labelled; a row's "used" counts its strings; a weight or length typed
/// goes to all of them (their wire points moved); a row added; a used row
/// deleted (confirmed) leaves its strings their values under a matching
/// or new label.
#[test]
fn the_string_library_is_a_table() {
    use crate::tables::TableKind;
    let k = TableKind::Insulators;
    let model = synthetic_project("string-library-table");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.state_mut().ready().unwrap().catalogue_strings();
    let used = |h: &mut Harness<'_, App>, id: &str| -> Vec<(String, String)> {
        let r = h.state_mut().ready().unwrap();
        r.model
            .line
            .structures
            .iter()
            .flat_map(|s| {
                s.strings
                    .iter()
                    .filter(|x| x.insulator.as_deref() == Some(id))
                    .map(|x| (s.id.clone(), x.label.clone()))
            })
            .collect()
    };
    let (id, rows) = {
        let r = harness.state_mut().ready().unwrap();
        let stored = r.model.stored_line().unwrap();
        assert!(
            stored
                .structures
                .iter()
                .flat_map(|s| &s.strings)
                .all(|x| x.insulator.is_some() || x.weight == 0.0),
            "every own string with a weight labelled (weightless ones keep the estimate)"
        );
        let rows = k.rows(&r.model, r.case);
        let total: usize = rows
            .iter()
            .map(|x| x.cells[k.column("used")].parse::<usize>().unwrap())
            .sum();
        let strings: usize = r
            .model
            .line
            .structures
            .iter()
            .map(|s| s.strings.iter().filter(|x| x.insulator.is_some()).count())
            .sum();
        assert_eq!(total, strings);
        // A suspension entry in use.
        let row = rows
            .iter()
            .find(|x| x.cells[k.column("kind")] == "suspension" && x.cells[k.column("used")] != "0")
            .expect("a suspension entry in use");
        (row.key.clone(), rows.len())
    };
    let users = used(&mut harness, &id);
    assert_eq!(
        k.rows(&harness.state_mut().ready().unwrap().model, 0)
            .iter()
            .find(|x| x.key == id)
            .unwrap()
            .cells[k.column("used")],
        users.len().to_string()
    );
    // Weight and length typed: every string with the label takes them.
    let old_len = {
        let r = harness.state_mut().ready().unwrap();
        let (s, l) = &users[0];
        let st = r.model.line.structure(s).unwrap();
        st.string_length(st.string(l).unwrap()).unwrap()
    };
    set_cell(&mut harness, k, &id, "weight", "1234");
    set_cell(
        &mut harness,
        k,
        &id,
        "length",
        &format!("{:.3}", old_len + 0.5),
    );
    apply_table(&mut harness, k, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        for (s, l) in &users {
            let st = r.model.line.structure(s).unwrap();
            let x = st.string(l).unwrap();
            assert!((x.weight - 1234.0).abs() < 1e-6, "{s} {l}: {}", x.weight);
            let len = st.string_length(x).unwrap();
            assert!((len - (old_len + 0.5)).abs() < 2e-3, "{s} {l}: {len}");
        }
    }
    // A row added.
    {
        let r = harness.state_mut().ready().unwrap();
        let mut row = k.new_row(&r.model, r.case, &[]);
        row[0] = "glass-I-380".into();
        r.tables.entry(k).or_default().added.push(row);
    }
    apply_table(&mut harness, k, false).unwrap();
    assert_eq!(
        k.rows(&harness.state_mut().ready().unwrap().model, 0).len(),
        rows + 1
    );
    // The used row deleted: asked first; its strings keep their values.
    harness
        .state_mut()
        .ready()
        .unwrap()
        .tables
        .entry(k)
        .or_default()
        .deleted
        .insert(id.clone());
    assert!(apply_table(&mut harness, k, false).is_err(), "asks first");
    apply_table(&mut harness, k, true).unwrap();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.types.as_ref().unwrap().1.get(&id).is_none());
    for (s, l) in &users {
        let x = r.model.line.structure(s).unwrap().string(l).unwrap();
        assert!((x.weight - 1234.0).abs() < 1e-6);
        assert!(
            x.insulator.as_deref().is_some_and(|i| i != id),
            "{:?}",
            x.insulator
        );
    }
}

/// Add structure on an alignment point without a structure: the window
/// puts it on that point (tied); its list names the points as the views
/// do; Display options write the points' names.
#[test]
fn a_structure_is_added_on_the_point_clicked() {
    use crate::annotations::{View, object_notes};
    use crate::editors::Form;
    use crate::tools::Tool;
    let model = synthetic_project_with("structure-on-point", |l| {
        let a = l.alignments[0].id.clone();
        let p0 = l.points_of(&l.alignments[0])[0].position;
        let p1 = l.points_of(&l.alignments[0])[1].position;
        let mid = [0, 1, 2].map(|k| 0.5 * (p0[k] + p1[k]));
        l.alignment_points
            .push(ohl_line::AlignmentPoint::free("PX", mid));
        let al = l.alignments.iter_mut().find(|x| x.id == a).unwrap();
        al.points.insert(1, "PX".into());
    });
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    use_tool(
        &mut harness,
        Tool::AddStructure,
        Selection::AlignmentPoint { id: "PX".into() },
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Structure(f)) = &r.editor else {
            panic!("{:?}", r.editor);
        };
        let (a, k) = f.on_alignment.clone().expect("on the point");
        let listed = crate::editors::alignment_points(&r.model.line);
        let p = listed.iter().find(|p| p.0 == a && p.1 == k).unwrap();
        assert_eq!(p.4, "PX");
        let at = r.model.line.alignment_point("PX").unwrap().position;
        assert!((f.position[0] - at[0]).abs() < 1e-6 && (f.position[1] - at[1]).abs() < 1e-6);
    }
    let r = harness.state_mut().ready().unwrap();
    r.model.section_display.alignment_point_label[View::Plan as usize] = true;
    let notes = object_notes(&r.model, 0, View::Plan, None);
    assert!(notes.iter().any(|n| n.text == "PX"));
}

/// Add jumper: its first end, then the suspension strings it passes
/// through (to wire points without a set) in order, then its other end —
/// a jumper with a clamp on each, given by the lengths between its clamps
/// (3 % over the straight distances), its window opened to type them; a
/// piece typed sets its length and the clamps' places.
#[test]
fn a_jumper_is_laid_through_suspension_strings() {
    use crate::editors::{Form, jumper_pieces, set_jumper_pieces};
    use crate::tools::Tool;
    let model = synthetic_project_with("jumper-through", |l| {
        let j = l.jumpers.remove(0);
        let (a, b) = (
            l.wire_point(j.from.point().unwrap()).unwrap().position,
            l.wire_point(j.to.point().unwrap()).unwrap().position,
        );
        let mid = [
            0.5 * (a[0] + b[0]),
            0.5 * (a[1] + b[1]),
            0.5 * (a[2] + b[2]) - 1.0,
        ];
        let st = l
            .structures
            .iter_mut()
            .find(|s| Some(s.id.as_str()) == j.from.structure())
            .unwrap();
        st.points.push(ohl_line::StructurePoint {
            description: String::new(),
            label: "A-JH".into(),
            position: [mid[0], mid[1], mid[2] + 2.0],
            ..Default::default()
        });
        st.wires.push(ohl_line::WirePoint {
            label: "JH".into(),
            position: mid,
            ..Default::default()
        });
        st.strings.push(ohl_line::InsulatorString {
            label: "S-JH".into(),
            top: "A-JH".into(),
            wire: "JH".into(),
            weight: 100.0,
            ..Default::default()
        });
        // A string whose wire point was given a set no section uses.
        st.wires.push(ohl_line::WirePoint {
            label: "W99".into(),
            position: [mid[0], mid[1], mid[2] - 0.5],
            set: Some(99),
            ..Default::default()
        });
        st.strings.push(ohl_line::InsulatorString {
            label: "S99".into(),
            top: "A-JH".into(),
            wire: "W99".into(),
            weight: 100.0,
            ..Default::default()
        });
        l.jumpers.insert(0, j);
    });
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let (from, to, n) = {
        let r = harness.state_mut().ready().unwrap();
        let j = r.model.line.jumpers[0].clone();
        (
            j.from.point().unwrap().clone(),
            j.to.point().unwrap().clone(),
            r.model.line.jumpers.len(),
        )
    };
    let end = |p: &ohl_line::SectionPoint| string_at(&p.structure, p.set);
    use_tool(&mut harness, Tool::AddJumper, end(&from), None);
    // A set no section uses: refused, said so; the jumper goes on.
    use_tool(
        &mut harness,
        Tool::AddJumper,
        string_at(&from.structure, 99),
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let (text, error) = r.status.clone().unwrap();
        assert!(error && text.contains("holds no section"), "{text}");
        assert_eq!(r.model.line.jumpers.len(), n);
        assert_eq!(r.jumper_from.as_ref(), Some(&from));
    }
    use_tool(
        &mut harness,
        Tool::AddJumper,
        Selection::String {
            structure: from.structure.clone(),
            label: "S-JH".into(),
        },
        None,
    );
    use_tool(&mut harness, Tool::AddJumper, end(&to), None);
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.model.line.jumpers.len(), n + 1, "{:?}", r.status);
    let j = r.model.line.jumpers.last().unwrap().clone();
    assert_eq!(j.supports().len(), 1);
    assert_eq!(j.supports()[0].wire, "JH");
    assert!(j.sag.is_none() && j.length.is_some());
    let pieces = jumper_pieces(&j);
    assert_eq!(pieces.len(), 2);
    let line = &r.model.line;
    let (a, m, b) = (
        line.wire_point(&from).unwrap().position,
        line.structure(&from.structure)
            .unwrap()
            .wire("JH")
            .unwrap()
            .position,
        line.wire_point(&to).unwrap().position,
    );
    let d = |p: [f64; 3], q: [f64; 3]| (0..3).map(|k| (p[k] - q[k]).powi(2)).sum::<f64>().sqrt();
    assert!((pieces[0] - 1.03 * d(a, m)).abs() < 1e-6, "{pieces:?}");
    assert!((pieces[1] - 1.03 * d(m, b)).abs() < 1e-6);
    assert!(matches!(&r.editor, Some(Form::Jumper(f)) if f.jumper.id == j.id));
    // Pieces typed: the length their sum, the clamp at the first's share.
    let mut k = j.clone();
    set_jumper_pieces(&mut k, &[3.0, 5.0]);
    assert!((k.length.unwrap() - 8.0).abs() < 1e-12);
    assert!((k.supports()[0].at - 3.0 / 8.0).abs() < 1e-12);
    assert_eq!(jumper_pieces(&k), vec![3.0, 5.0]);
}

/// Several weather cases drawn: a span is picked on any of its copies,
/// the case of the copy clicked reported (`None`: the case shown); the
/// app keeps it (highlight, properties) until something is selected
/// otherwise.
#[test]
fn a_span_is_picked_in_any_weather_case_drawn() {
    use crate::selection::{Snaps, pick_in_model_case};
    let mut model = synthetic();
    model.shown_cases = vec![0, 2];
    for k in 0..model.cases.len() {
        model.ensure_solved(k);
    }
    let section = model.line.sections[0].id.clone();
    let mid = |k: usize| {
        let shape = model.cases[k]
            .shapes
            .iter()
            .find(|s| s.section == section)
            .unwrap();
        let p = &shape.spans[0].points;
        p[p.len() / 2]
    };
    let (m0, m2) = (mid(0), mid(2));
    assert!(
        (m0[2] - m2[2]).abs() > 0.5,
        "the ice case sags lower: {m0:?} {m2:?}"
    );
    // Seen across the line (x, z on screen), 10 px per m.
    let project = |p: [f64; 3]| Some([p[0] * 10.0, -p[2] * 10.0]);
    let at = |p: [f64; 3]| project(p).unwrap();
    let span = Selection::Span {
        section: section.clone(),
        span: 0,
    };
    assert_eq!(
        pick_in_model_case(&model, 0, &Snaps::default(), at(m2), 3.0, project),
        (span.clone(), Some(2))
    );
    assert_eq!(
        pick_in_model_case(&model, 0, &Snaps::default(), at(m0), 3.0, project),
        (span.clone(), None)
    );
    // The app keeps the case clicked; a selection made otherwise drops it.
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    r.view3d.picked_case = Some(Some(2));
    r.selection = span.clone();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.model.selection_case, Some(2));
    r.select(Selection::Structure("T2".into()));
    assert_eq!(r.model.selection_case, None);
}

/// Several weather cases drawn: span points, spacers and wire points are
/// picked on any of their copies too (the case of the copy clicked
/// reported), as spans are.
#[test]
fn span_objects_and_wire_points_are_picked_in_any_case_drawn() {
    use crate::selection::{Snap, Snaps, pick_in_model_case};
    let mut model = synthetic_project_with("pick-any-case", |l| {
        let (a, b) = (l.sections[0].id.clone(), l.sections[1].id.clone());
        for s in &mut l.sections[..2] {
            s.span_points.push(ohl_line::SpanPoint {
                label: "M".into(),
                span: 0,
                at: ohl_line::SpanPlace::at_fraction(0.5),
                description: String::new(),
                object: Default::default(),
            });
        }
        let end = |section: &str| ohl_line::SpanPointRef {
            section: section.into(),
            point: "M".into(),
        };
        l.spacers.push(ohl_line::Spacer {
            id: "SP".into(),
            a: end(&a),
            b: end(&b),
            weight: 100.0,
            compression: Some(10_000.0),
            ..Default::default()
        });
    });
    model.shown_cases = vec![0, 2];
    for k in 0..model.cases.len() {
        model.ensure_solved(k);
    }
    let section = model.line.sections[0].id.clone();
    let shape = |k: usize| {
        model.cases[k]
            .shapes
            .iter()
            .find(|s| s.section == section)
            .unwrap()
            .clone()
    };
    let (s0, s2) = (shape(0), shape(2));
    let (p0, p2) = (s0.span_points[0].1, s2.span_points[0].1);
    assert!((p0[2] - p2[2]).abs() > 0.5, "{p0:?} {p2:?}");
    // Seen across the line (x, z on screen), 10 px per m.
    let project = |p: [f64; 3]| Some([p[0] * 10.0, -p[2] * 10.0]);
    let at = |p: [f64; 3]| project(p).unwrap();
    let snaps = Snaps::default();
    assert_eq!(
        pick_in_model_case(&model, 0, &snaps, at(p2), 3.0, project),
        (
            Selection::SpanPoint {
                section: section.clone(),
                label: "M".into()
            },
            Some(2)
        )
    );
    // The spacer, between its ends in the ice case.
    let e = s2.spacers[0].ends;
    let third = [0, 1, 2].map(|i| e[0][i] + (e[1][i] - e[0][i]) / 3.0);
    assert_eq!(
        pick_in_model_case(&model, 0, &snaps, at(third), 3.0, project).0,
        Selection::Spacer("SP".into())
    );
    // A wire point where it hangs in a wind case (the clamp swinging
    // most), seen along the line (y, z on screen).
    let windy = (0..model.cases.len())
        .max_by(|a, b| {
            let p = |k: usize| model.cases[k].case.wind_pressure.si();
            p(*a).total_cmp(&p(*b))
        })
        .unwrap();
    assert!(model.cases[windy].case.wind_pressure.si() > 0.0);
    model.shown_cases = vec![0, windy];
    let side = |p: [f64; 3]| Some([p[1] * 10.0, -p[2] * 10.0]);
    let wires = |k: usize| crate::selection::string_pieces(&model, k);
    let (w0, ww) = (wires(0), wires(windy));
    let apart = |a: [f64; 3], b: [f64; 3]| (a[1] - b[1]).hypot(a[2] - b[2]);
    let (key, clamp, still) = ww
        .iter()
        .filter_map(|(key, v)| Some((key.clone(), v.last()?.1, w0.get(key)?.last()?.1)))
        .max_by(|a, b| apart(a.1, a.2).total_cmp(&apart(b.1, b.2)))
        .unwrap();
    assert!(
        apart(clamp, still) > 0.5,
        "the clamp swings: {clamp:?} {still:?}"
    );
    let mut snaps = Snaps::default();
    snaps.0.insert(Snap::Attachment);
    let (picked, k) = pick_in_model_case(&model, 0, &snaps, side(clamp).unwrap(), 3.0, side);
    assert!(
        matches!(&picked, Selection::Attachment { structure, .. } if *structure == key.0),
        "{picked:?}"
    );
    assert_eq!(k, Some(windy));
}

/// Spread objects: picked all along their stretch on any weather case
/// drawn (the case of the copy clicked reported), highlighted on every
/// copy, listed in the table (selecting the spread, its load on each
/// subconductor said). The Delete tool asks what goes with them: a spread
/// with its bare points, a spread alone (its points stay), and a point a
/// spread ends on with its spreads (the clamp on the other end stays);
/// Cancel leaves everything. The span tool puts a spread on each
/// subconductor.
#[test]
fn spreads_are_picked_and_deleted_with_or_without_their_points() {
    use crate::selection::{Snaps, pick_in_model_case};
    use crate::span_tool::SpanWhat;
    use crate::tables::TableKind;
    use crate::tools::Tool;
    let mut model = synthetic_project_with("spreads", |l| {
        let s = &mut l.sections[0];
        for (label, f, clamp) in [
            ("XA", 0.4, false),
            ("XB", 0.6, false),
            ("XC", 0.65, true),
            ("XD", 0.7, false),
            ("XE", 0.75, false),
            ("XF", 0.85, false),
        ] {
            s.span_points.push(ohl_line::SpanPoint {
                label: label.into(),
                span: 0,
                at: ohl_line::SpanPlace::at_fraction(f),
                description: String::new(),
                object: clamp.then(|| ohl_line::LineObject::clamp(20.0)),
            });
        }
        for (label, from, to, each) in [
            ("AB", "XA", "XB", false),
            ("CD", "XC", "XD", true),
            ("EF", "XE", "XF", false),
        ] {
            s.spreads.push(ohl_line::SpreadObject {
                label: label.into(),
                from: from.into(),
                to: to.into(),
                load: 10.0,
                per_subconductor: each,
                ..Default::default()
            });
        }
    });
    model.shown_cases = vec![0, 1];
    for k in 0..model.cases.len() {
        model.ensure_solved(k);
    }
    let section = model.line.sections[0].id.clone();
    let index = |line: &ohl_line::Line, label: &str| {
        line.sections[0]
            .spreads
            .iter()
            .position(|s| s.label == label)
    };
    let ab = index(&model.line, "AB").unwrap();
    let spread = |index: usize| Selection::Spread {
        section: section.clone(),
        index,
    };
    // Picked on its copy in the hot case (sagging lower), along its
    // stretch.
    let def = model.line.section_with_fractions(&model.line.sections[0]);
    let stretch = |k: usize| {
        let shape = model.cases[k]
            .shapes
            .iter()
            .find(|s| s.section == section)
            .unwrap();
        crate::scene::spreads_drawn(&def, shape)
            .into_iter()
            .find(|(i, _)| *i == ab)
            .unwrap()
            .1
    };
    assert!(crate::selection::spread_stretch(&model, 0, &section, ab).is_some());
    let (s0, s2) = (stretch(0), stretch(1));
    let (m0, m2) = (s0[s0.len() / 2], s2[s2.len() / 2]);
    assert!((m0[2] - m2[2]).abs() > 0.5, "{m0:?} {m2:?}");
    let project = |p: [f64; 3]| Some([p[0] * 10.0, -p[2] * 10.0]);
    let at = |p: [f64; 3]| project(p).unwrap();
    assert_eq!(
        pick_in_model_case(&model, 0, &Snaps::default(), at(m2), 3.0, project),
        (spread(ab), Some(1))
    );
    assert_eq!(
        pick_in_model_case(&model, 0, &Snaps::default(), at(m0), 3.0, project),
        (spread(ab), None)
    );
    // Highlighted on every copy drawn: its stretch and its ends.
    let hi = crate::scene::layer(&model, crate::scene::Layer::Selection, 0, &spread(ab));
    let (lines, points, _) = hi.counts();
    assert!(lines > 0 && points >= 4, "{lines} {points}");
    // The table's row selects it; the load on each subconductor said.
    let rows = TableKind::SpanObjects.rows(&model, 0);
    let cd = index(&model.line, "CD").unwrap();
    let row = rows
        .iter()
        .find(|r| r.select == spread(cd))
        .expect("a row for the spread");
    assert_eq!(row.cells[TableKind::SpanObjects.column("load on")], "each");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // The other tools act on its span: Add on span opens there.
    {
        let r = harness.state_mut().ready().unwrap();
        r.hotbar.hold(Tool::OnSpan);
        r.use_tool(crate::selection::Use {
            target: spread(ab),
            at: None,
            on: None,
        });
        assert!(
            matches!(&r.editor, Some(crate::editors::Form::Span(f)) if f.span == 0 && f.section == section),
            "{:?}",
            r.status
        );
        r.editor = None;
    }
    harness.run();
    let line = |h: &mut Harness<'_, App>| h.state_mut().ready().unwrap().model.line.clone();
    let has_point =
        |l: &ohl_line::Line, p: &str| l.sections[0].span_points.iter().any(|x| x.label == p);
    // EF with its points: both bare, both go.
    let ef = index(&line(&mut harness), "EF").unwrap();
    use_tool(&mut harness, Tool::Delete, spread(ef), None);
    assert!(harness.state_mut().ready().unwrap().spread_delete.is_some());
    harness.get_by_label("Spread and its points").click();
    harness.run();
    {
        let l = line(&mut harness);
        let r = harness.state_mut().ready().unwrap();
        assert!(r.spread_delete.is_none(), "{:?}", r.status);
        assert!(index(&l, "EF").is_none());
        assert!(!has_point(&l, "XE") && !has_point(&l, "XF"));
    }
    // AB alone: its points stay.
    let ab = index(&line(&mut harness), "AB").unwrap();
    use_tool(&mut harness, Tool::Delete, spread(ab), None);
    harness.get_by_label("Spread only").click();
    harness.run();
    {
        let l = line(&mut harness);
        assert!(index(&l, "AB").is_none());
        assert!(has_point(&l, "XA") && has_point(&l, "XB"));
    }
    // A point CD ends on: asked; Cancel leaves it.
    let xd = Selection::SpanPoint {
        section: section.clone(),
        label: "XD".into(),
    };
    use_tool(&mut harness, Tool::Delete, xd.clone(), None);
    assert!(harness.state_mut().ready().unwrap().spread_delete.is_some());
    harness.get_by_label("Cancel").click();
    harness.run();
    {
        let l = line(&mut harness);
        assert!(harness.state_mut().ready().unwrap().spread_delete.is_none());
        assert!(index(&l, "CD").is_some() && has_point(&l, "XD"));
    }
    // As Delete selected: the point and its spread go, the clamp on the
    // spread's other end stays.
    {
        let r = harness.state_mut().ready().unwrap();
        r.select(xd.clone());
        assert!(r.delete_object(&xd));
    }
    harness.run();
    harness
        .get_by_label("Delete the point and its spreads")
        .click();
    harness.run();
    {
        let l = line(&mut harness);
        assert!(index(&l, "CD").is_none());
        assert!(!has_point(&l, "XD") && has_point(&l, "XC"));
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.selection, Selection::None);
        l.validate().unwrap();
    }
    // The span tool: a spread on each subconductor.
    {
        let r = harness.state_mut().ready().unwrap();
        let mut f = crate::span_tool::SpanForm::new(&r.model, r.case, &section, 0, None).unwrap();
        f.what = SpanWhat::Spread;
        (f.from, f.to, f.per_metre, f.per_subconductor) = (10.0, 30.0, 8.0, true);
        r.editor = Some(crate::editors::Form::Span(f));
    }
    harness.run();
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    let l = line(&mut harness);
    let last = l.sections[0].spreads.last().unwrap();
    assert!(last.per_subconductor && last.load == 8.0, "{last:?}");
}

/// Labels: several weather cases drawn give one label per section (its
/// first case); in 3D only those within the label distance of the eye
/// are built.
#[test]
fn labels_are_one_per_section_and_near_only() {
    use crate::annotations::{View, notes, object_notes};
    let mut model = synthetic();
    model.shown_cases = vec![0, 2];
    for k in 0..model.cases.len() {
        model.ensure_solved(k);
    }
    model.section_display.label[View::ThreeD as usize] = true;
    model.section_display.structure_label[View::ThreeD as usize] = true;
    let all = notes(&model, 0, View::ThreeD, None);
    assert_eq!(
        all.len(),
        model.line.sections.len(),
        "one label per section"
    );
    // Far from everything: only the structures' names (written from
    // afar); near the first structure: its other labels too.
    let s = model.line.structures[0].position;
    let far = Some(([s[0] + 1e6, s[1], s[2]], 1500.0));
    assert!(notes(&model, 0, View::ThreeD, far).is_empty());
    let names = object_notes(&model, 0, View::ThreeD, far);
    assert_eq!(names.len(), model.line.structures.len());
    assert!(
        names
            .iter()
            .all(|n| n.kind == crate::annotations::Kind::Structure)
    );
    model.section_display.point_label[View::ThreeD as usize] = true;
    assert_eq!(
        object_notes(&model, 0, View::ThreeD, far).len(),
        model.line.structures.len(),
        "points' labels near only"
    );
    let near = Some((s, 50.0));
    assert!(!object_notes(&model, 0, View::ThreeD, near).is_empty());
    assert_eq!(
        model.section_display.label_range(),
        crate::annotations::DEFAULT_LABEL_DISTANCE
    );
}

/// Plugin columns: a plugin's insulation level in the circuits table
/// (an invented column) — its default shown where not set; a value chosen goes to the
/// circuit's sections in one command; one not among its values refused.
/// Without the plugin the column still shows (unused) and the values stay,
/// through other edits and an OHL XML round trip.
#[test]
fn plugin_columns_in_tables() {
    use crate::tables::{PluginColumn, TableKind};
    let model = synthetic_project("plugin-columns");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let column = PluginColumn {
        table: "circuits".into(),
        key: "example.insulation_level".into(),
        title: "insulation level".into(),
        help: String::new(),
        values: vec!["low".into(), "high".into()],
        default: Some("high".into()),
        plugin: "example 0.1.0".into(),
        in_use: true,
    };
    let own = TableKind::ConductorPaths.columns().len();
    let key = {
        let r = harness.state_mut().ready().unwrap();
        r.model.plugin_columns = vec![column.clone()];
        let rows = TableKind::ConductorPaths.rows(&r.model, 0);
        let x = rows.iter().find(|x| x.cells[6].contains("S3")).unwrap();
        assert_eq!(x.cells[own], "high");
        assert!(x.defaulted[own], "the default, not set");
        // Declared for the circuits only.
        let sections = TableKind::Sections.rows(&r.model, 0);
        assert_eq!(sections[0].cells.len(), TableKind::Sections.columns().len());
        let original = x.cells[own].clone();
        r.tables
            .entry(TableKind::ConductorPaths)
            .or_default()
            .set(&x.key, own, "Low", &original);
        x.key.clone()
    };
    let before = harness
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .sections
        .len();
    apply_table(&mut harness, TableKind::ConductorPaths, false).unwrap();
    let ids = {
        let r = harness.state_mut().ready().unwrap();
        let line = &r.model.line;
        assert_eq!(line.sections.len(), before);
        let c = ohl_line::electrical::circuits(line)
            .into_iter()
            .find(|c| c.sections.contains(&"S3".to_owned()))
            .unwrap();
        for id in &c.sections {
            let s = line.sections.iter().find(|s| &s.id == id).unwrap();
            assert_eq!(s.fields.get(&column.key).map(String::as_str), Some("low"));
        }
        let others = line
            .sections
            .iter()
            .filter(|s| !c.sections.contains(&s.id))
            .count();
        assert!(others > 0);
        assert!(
            line.sections
                .iter()
                .filter(|s| !c.sections.contains(&s.id))
                .all(|s| s.fields.is_empty())
        );
        let rows = TableKind::ConductorPaths.rows(&r.model, 0);
        let x = rows.iter().find(|x| x.key == key).unwrap();
        assert_eq!(x.cells[own], "low");
        assert!(!x.defaulted[own]);
        // A value not among its values: refused.
        r.tables
            .entry(TableKind::ConductorPaths)
            .or_default()
            .set(&key, own, "medium", "low");
        c.sections
    };
    let err = apply_table(&mut harness, TableKind::ConductorPaths, false).unwrap_err();
    assert!(err.contains("not one of"), "{err}");
    harness
        .state_mut()
        .ready()
        .unwrap()
        .tables
        .remove(&TableKind::ConductorPaths);
    // Without the plugin: an unused column in the sections' tables, the
    // values kept.
    let n = TableKind::Sections.columns().len();
    {
        let r = harness.state_mut().ready().unwrap();
        r.model.plugin_columns.clear();
        let cols = TableKind::Sections.plugin_columns(&r.model);
        assert_eq!(cols.len(), 1);
        assert!(!cols[0].in_use && cols[0].key == column.key);
        let rows = TableKind::Sections.rows(&r.model, 0);
        let s3 = rows.iter().find(|x| x.key == "S3").unwrap();
        assert_eq!(s3.cells[n], "low");
        assert_eq!(TableKind::ConductorPaths.plugin_columns(&r.model).len(), 1);
    }
    set_cell(
        &mut harness,
        TableKind::Sections,
        "S3",
        "description",
        "kept its level",
    );
    apply_table(&mut harness, TableKind::Sections, false).unwrap();
    let r = harness.state_mut().ready().unwrap();
    let s3 = r.model.line.sections.iter().find(|s| s.id == "S3").unwrap();
    assert_eq!(s3.description, "kept its level");
    assert_eq!(s3.fields.get(&column.key).map(String::as_str), Some("low"));
    // The OHL XML keeps them.
    let back = ohl_io::ohlxml::from_xml(&ohl_io::ohlxml::to_xml(&r.model.line)).unwrap();
    for id in &ids {
        let s = back.sections.iter().find(|s| &s.id == id).unwrap();
        assert_eq!(s.fields.get(&column.key).map(String::as_str), Some("low"));
    }
}

/// The Swiss LeV plugin in use, when this checkout has it (kept apart
/// from the public code, at `local-data/plugins/lev-ch`) and it is built:
/// whether it is. Without the folder the test is skipped (even when the
/// plugin tests are required); with it, not built: skipped unless they
/// are.
fn lev_plugin(h: &mut Harness<'_, App>) -> bool {
    let dir = root().join("local-data/plugins/lev-ch");
    if !dir.is_dir() {
        eprintln!("skipping: no local-data/plugins/lev-ch");
        return false;
    }
    let built = dir
        .join("target/wasm32-wasip2/release/lev_ch.wasm")
        .is_file();
    assert!(
        built || ohl_core::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
        "lev-ch plugin not built"
    );
    if !built {
        return false;
    }
    let app = h.state_mut();
    app.plugins = crate::plugins::Plugins::load(None, None);
    app.plugins.folders = vec![dir];
    app.plugins.refresh();
    h.run();
    true
}

/// Checks › LeV/OLEI › Distance between conductors (the menu the lev-ch
/// plugin defines; none without it): the window opens with the
/// shipped LeV values, checks the ticked case in the background (pairs
/// worst first, every circuit), and a pair's span link selects the span
/// and draws its worst station's line with d and z.
#[test]
fn the_checks_menu_opens_the_phase_distance_window() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // Without the plugin: no LeV submenu (the plugin defines it).
    harness.get_by_label("Checks").click();
    harness.run();
    assert!(harness.query_by_label_contains("LeV").is_none());
    assert!(
        harness
            .query_by_label_contains("Distance between")
            .is_none()
    );
    harness.get_by_label("Checks").click();
    harness.run();
    if !lev_plugin(&mut harness) {
        return;
    }
    {
        let m = &harness.state_mut().ready().unwrap().model;
        assert_eq!(m.checks.len(), 1, "{:?}", m.check_problems);
    }
    harness.get_by_label("Checks").click();
    harness.run();
    harness.get_by_label_contains("LeV/OLEI").hover();
    harness.run();
    harness
        .get_by_label_contains("Distance between conductors")
        .click();
    harness.run();
    // A page of its own, floating first (it may be docked).
    let at = harness
        .state_mut()
        .dock()
        .find_tab(&crate::app::Tab::PhaseDistance)
        .expect("its page");
    assert!(!at.surface.is_main(), "floating");
    {
        let r = harness.state_mut().ready().unwrap();
        let w = &mut r.phase_distance;
        assert!(w.file.is_none(), "the plugin's values");
        assert!(w.check().is_ok(), "{:?}", w.check().err());
        assert!(w.circuits.is_none(), "every circuit");
        // A case other than the one shown: a click shows it.
        let other = (r.case + 1) % r.model.cases.len();
        assert_ne!(other, r.case);
        w.cases = std::iter::once(other).collect();
    }
    harness.get_by_label("Run").click();
    let started = std::time::Instant::now();
    let (row, below) = loop {
        harness.step();
        let r = harness.state_mut().ready().unwrap();
        if let (Some(rep), false) = r.phase_distance.report() {
            let rep = rep.as_ref().unwrap();
            assert!(!rep.rows.is_empty());
            assert!(
                rep.rows.windows(2).all(|w| w[0].margin() <= w[1].margin()),
                "worst first"
            );
            for x in &rep.rows {
                assert!(x.distance > 0.0 && x.required > x.c);
            }
            break (
                rep.rows[0].clone(),
                rep.rows.iter().filter(|x| x.margin() < 0.0).count(),
            );
        }
        assert!(started.elapsed().as_secs() < 60, "no report in 60 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    // All pairs shown, then a span link.
    harness
        .state_mut()
        .ready()
        .unwrap()
        .phase_distance
        .only_below = false;
    harness.run();
    let _ = below;
    let label = format!("{} span {}", row.span[0].0, row.span[0].1 + 1);
    harness.query_all_by_label(&label).next().unwrap().click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(
        r.selection,
        Selection::Span {
            section: row.span[0].0.clone(),
            span: row.span[0].1
        }
    );
    let (a, b, text) = r.view3d.measure.clone().expect("the line drawn");
    assert_eq!([a, b], row.at);
    assert!(text.contains("d ") && text.contains("z "), "{text}");
    // In its weather case.
    assert_eq!(r.model.cases[r.case].case.name, row.case);
    // The page closed: the line gone.
    let dock = harness.state_mut().dock();
    let at = dock.find_tab(&crate::app::Tab::PhaseDistance).unwrap();
    dock.remove_tab(at);
    harness.run();
    assert!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .view3d
            .measure
            .is_none()
    );
}

/// The library of performed checks: a run of the LeV distance between
/// conductors saved (kept with the line, logged); the line edited: the
/// report is stale; opened again: its parameters and results; run again:
/// compared with it, then replaced on request; deleted after a
/// confirmation, undo brings it back.
#[test]
fn performed_checks_are_saved_and_run_again() {
    let model = synthetic_project("performed-checks");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    if !lev_plugin(&mut harness) {
        return;
    }
    let run = |h: &mut Harness<'_, App>| {
        let started = std::time::Instant::now();
        loop {
            h.step();
            let r = h.state_mut().ready().unwrap();
            if let (Some(rep), false) = r.phase_distance.report() {
                assert!(rep.is_ok(), "{rep:?}");
                break;
            }
            assert!(started.elapsed().as_secs() < 60, "no report in 60 s");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        h.run();
    };
    {
        let r = harness.state_mut().ready().unwrap();
        r.phase_distance.show(&r.model);
        if r.phase_distance.cases.is_empty() {
            r.phase_distance.cases.insert(0);
        }
        let model = &r.model;
        let ctx = egui::Context::default();
        r.phase_distance.start(model, &ctx);
    }
    run(&mut harness);
    harness.get_by_label("Save").click();
    harness.run();
    let saved = {
        let r = harness.state_mut().ready().unwrap();
        let s = &r.model.editing.as_ref().unwrap().session;
        let reports = s.reports();
        assert_eq!(reports.len(), 1);
        let x = reports[0].clone();
        assert_eq!(x.id, "Distance between conductors (LeV)");
        assert_eq!(x.check, crate::phase_distance::CHECK_ID);
        assert_eq!(x.status, ohl_session::reports::ReportStatus::Draft);
        assert!(x.summary.contains("pairs"), "{}", x.summary);
        assert!(
            x.parameters["values_text"]
                .as_str()
                .unwrap()
                .contains("[[voltage]]")
        );
        assert!(
            ohl_session::reports::changed_since(&x.state, &s.check_state().unwrap()).is_empty()
        );
        x
    };
    // The line edited: stale.
    {
        let r = harness.state_mut().ready().unwrap();
        let p = r.model.line.structure("T2").unwrap().position;
        r.perform(crate::edit::Action::Command {
            what: "T2 up".into(),
            command: ohl_session::Command::MoveStructure {
                id: "T2".into(),
                position: [p[0], p[1], p[2] + 3.0],
            },
        });
        let s = &r.model.editing.as_ref().unwrap().session;
        let c = ohl_session::reports::changed_since(&saved.state, &s.check_state().unwrap());
        assert!(c.changed.contains(&"structure:T2".to_owned()), "{c:?}");
        r.check_library.show();
    }
    harness.run();
    // In the library: what changed since.
    harness.get_by_label_contains("changed since:").hover();
    // Opened again from the library: its parameters and results.
    harness.get_by_label("Open").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let w = &r.phase_distance;
        assert_eq!(
            w.loaded.as_ref().map(|x| x.id.as_str()),
            Some(saved.id.as_str())
        );
        assert!(
            w.values.is_some(),
            "the values as saved; log {:?}",
            r.model.log.iter().rev().take(3).collect::<Vec<_>>()
        );
        let (rep, _) = w.report();
        let rep = rep.unwrap().as_ref().unwrap();
        let kept: crate::phase_distance::Saved =
            serde_json::from_value(saved.results.clone()).unwrap();
        assert_eq!(rep.rows, kept.rows);
        assert!(
            rep.rows
                .iter()
                .all(|x| x.margin() < crate::phase_distance::KEPT_WITHIN)
        );
    }
    harness.get_by_label_contains("stale").hover();
    // Run again: compared, then replaced on request.
    {
        let r = harness.state_mut().ready().unwrap();
        let ctx = egui::Context::default();
        r.phase_distance.start(&r.model, &ctx);
    }
    run(&mut harness);
    harness.run();
    assert!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .phase_distance
            .comparison
            .is_some()
    );
    harness
        .get_by_label_contains("Compared with the saved report")
        .hover();
    harness.get_by_label("Replace").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let s = &r.model.editing.as_ref().unwrap().session;
        let reports = s.reports();
        assert_eq!(reports.len(), 1, "replaced, not added");
        assert!(
            ohl_session::reports::changed_since(&reports[0].state, &s.check_state().unwrap())
                .is_empty(),
            "made on the line as it is now"
        );
        assert_ne!(reports[0].state, saved.state);
    }
    // Deleted after a confirmation; undo brings it back (the check's
    // window closed: it lies over the library).
    let dock = harness.state_mut().dock();
    let at = dock.find_tab(&crate::app::Tab::PhaseDistance).unwrap();
    dock.remove_tab(at);
    harness.run();
    harness.get_by_label("Delete…").click();
    harness.run();
    assert_eq!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .model
            .editing
            .as_ref()
            .unwrap()
            .session
            .reports()
            .len(),
        1,
        "not before the confirmation"
    );
    harness.get_by_label("Delete").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(
        r.model
            .editing
            .as_ref()
            .unwrap()
            .session
            .reports()
            .is_empty()
    );
    r.perform(crate::edit::Action::Undo);
    assert_eq!(r.model.editing.as_ref().unwrap().session.reports().len(), 1);
}

/// The LeV values in the window: shown as tables, locked; an admin edits
/// them (Edit values), saved as the project's copy and used by the next run;
/// back to the file; a designer may not edit them.
#[test]
fn phase_distance_values_are_locked_but_an_admin_edits_them() {
    let model = synthetic_project("values-locked");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    if !lev_plugin(&mut harness) {
        return;
    }
    {
        let r = harness.state_mut().ready().unwrap();
        r.phase_distance.show(&r.model);
    }
    harness.run();
    // Shown: the 380 kV row.
    harness.get_by_label("380");
    harness.get_by_label("3.64");
    harness.get_by_label("Edit values").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let w = &mut r.phase_distance;
        let text = w.editing.clone().expect("editing");
        // 380 kV high, C₂: the last line of the first table.
        let tables = w.tables_of(&text).unwrap();
        let last = tables[0].rows.len() as u32 - 1;
        assert_eq!(tables[0].rows[last as usize][4], "3.64");
        w.editing = Some(w.edit_cell(&text, [0, last, 4], "3.70").unwrap());
        assert!(w.edit_cell(&text, [0, last, 4], "-1").is_err());
    }
    harness.run();
    harness.get_by_label("Save in the project").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.phase_distance.editing.is_none());
        let s = &r.model.editing.as_ref().unwrap().session;
        let files = s.library_files("phase-distance").unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].0, "lev-ch.ohlpd");
        assert!(files[0].1.contains("3.7 m"), "{}", files[0].1);
        assert!(files[0].1.starts_with("# Edited in OHL-Design by"));
        // The next run uses them.
        if r.phase_distance.cases.is_empty() {
            r.phase_distance.cases.insert(0);
        }
        let ctx = egui::Context::default();
        r.phase_distance.start(&r.model, &ctx);
    }
    let started = std::time::Instant::now();
    loop {
        harness.step();
        let r = harness.state_mut().ready().unwrap();
        if let (Some(rep), false) = r.phase_distance.report() {
            let rep = rep.as_ref().unwrap();
            assert!(rep.params.values_text.contains("3.7 m"));
            break;
        }
        assert!(started.elapsed().as_secs() < 60);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    harness.run();
    harness.get_by_label_contains("the project's copy");
    harness.get_by_label("Back to the plugin's").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let s = &r.model.editing.as_ref().unwrap().session;
        assert!(s.library_files("phase-distance").unwrap().is_empty());
        // A designer: locked.
        let admin = r.model.editing.as_ref().unwrap().user.clone();
        s.set_user(&admin, "dee", ohl_session::Role::Designer)
            .unwrap();
        r.model.editing.as_mut().unwrap().user = "dee".into();
    }
    harness.run();
    assert!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .phase_distance
            .editing
            .is_none()
    );
    harness.get_by_label("Edit values").click();
    harness.run();
    assert!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .phase_distance
            .editing
            .is_none(),
        "locked for a designer"
    );
}

/// Sections table: the bundle angle after the bundle and its spacing —
/// 0 shown (dimmed) for a bundle not turned, typed in, emptied: not
/// turned again.
#[test]
fn sections_table_turns_the_bundle() {
    use crate::tables::TableKind;
    let model = synthetic_project("bundle-angle");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let col = TableKind::Sections.column("bundle angle °");
    assert_eq!(col, TableKind::Sections.column("spacing m") + 1);
    set_cell(&mut harness, TableKind::Sections, "S1", "bundle", "3");
    set_cell(&mut harness, TableKind::Sections, "S1", "spacing m", "0.45");
    apply_table(&mut harness, TableKind::Sections, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let rows = TableKind::Sections.rows(&r.model, r.case);
        let s1 = rows.iter().find(|x| x.key == "S1").unwrap();
        assert_eq!(s1.cells[col], "0");
        assert!(s1.defaulted[col]);
    }
    set_cell(
        &mut harness,
        TableKind::Sections,
        "S1",
        "bundle angle °",
        "60",
    );
    apply_table(&mut harness, TableKind::Sections, false).unwrap();
    let rot = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        let s = r.model.line.sections.iter().find(|s| s.id == "S1").unwrap();
        (s.bundle, s.bundle_rotation_deg)
    };
    assert_eq!(rot(&mut harness), (3, Some(60.0)));
    set_cell(
        &mut harness,
        TableKind::Sections,
        "S1",
        "bundle angle °",
        "",
    );
    apply_table(&mut harness, TableKind::Sections, false).unwrap();
    assert_eq!(rot(&mut harness), (3, None));
}

/// The LeV page lists the circuits checked with their insulation level
/// (not set: high); one chosen there goes to all the circuit's sections.
#[test]
fn phase_distance_page_sets_the_insulation_levels() {
    let model = synthetic_project("levels-in-page");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    if !lev_plugin(&mut harness) {
        return;
    }
    {
        let r = harness.state_mut().ready().unwrap();
        r.phase_distance.show(&r.model);
    }
    harness.run();
    harness
        .query_all_by_value("high (not set)")
        .next()
        .expect("a circuit's level")
        .click();
    harness.run();
    harness
        .query_all_by_label("low")
        .last()
        .expect("the low choice")
        .click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    let line = &r.model.line;
    let low: Vec<&ohl_line::Section> = line
        .sections
        .iter()
        .filter(|s| s.fields.get("lev-ch.insulation_level").map(String::as_str) == Some("low"))
        .collect();
    assert!(!low.is_empty());
    let circuit = &low[0].circuit_name;
    assert!(
        line.sections
            .iter()
            .filter(|s| &s.circuit_name == circuit)
            .all(|s| low.iter().any(|l| l.id == s.id)),
        "all the circuit's sections"
    );
    assert!(
        line.sections
            .iter()
            .any(|s| &s.circuit_name != circuit && s.fields.is_empty())
    );
}

/// View › Profile aspect ratio, a ratio of your own: "1:" chooses it (its
/// last value, 3 at first), the value is typed in — 10, also a value of
/// the list, stays the typed choice; a ratio of the list chosen after.
#[test]
fn the_profile_takes_a_ratio_of_your_own() {
    use egui_kittest::kittest::NodeT as _;
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let open = |h: &mut Harness<'_, App>| {
        h.get_by_label("View").click();
        h.run();
        h.get_by_label_contains("Profile aspect ratio").hover();
        h.run();
    };
    let ratio = |h: &mut Harness<'_, App>| h.state_mut().ready().unwrap().model.profile_ratio;
    open(&mut harness);
    harness.get_by_label("1:").click();
    harness.run();
    assert_eq!(ratio(&mut harness), Some(3.0));
    // Typed: 10.
    // The menu's value field: the spin button on the row of "1:".
    let row = harness.get_by_label("1:").rect().center().y;
    let at = harness
        .query_all_by_role(egui::accesskit::Role::SpinButton)
        .map(|x| x.rect().center())
        .min_by(|a, b| (a.y - row).abs().total_cmp(&(b.y - row).abs()))
        .expect("the value field");
    assert!((at.y - row).abs() < 5.0, "{at:?} {row}");
    harness
        .query_all_by_role(egui::accesskit::Role::SpinButton)
        .find(|x| x.rect().center() == at)
        .unwrap()
        .click();
    harness.run();
    assert!(
        harness.query_by_label("1:").is_some(),
        "the menu stays open"
    );
    // Editing: the text selected, typed over.
    harness.event(egui::Event::Text("10".into()));
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert_eq!(ratio(&mut harness), Some(10.0));
    // Still the typed choice (the list's 1:10 not ticked).
    let typed = harness.get_by_label("1:").accesskit_node().toggled();
    assert_eq!(typed, Some(egui::accesskit::Toggled::True));
    let listed = harness
        .get_by_label("1:10 (heights ×10)")
        .accesskit_node()
        .toggled();
    assert_eq!(listed, Some(egui::accesskit::Toggled::False));
    // One of the list.
    harness.get_by_label("1:5 (heights ×5)").click();
    harness.run();
    assert_eq!(ratio(&mut harness), Some(5.0));
    assert_eq!(
        harness.get_by_label("1:").accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::False)
    );
}

/// Strings: a tension string may hang from the wire point of a suspension
/// string without a set (a TAK) though that one is not of its group: the
/// editor lists every wire point of the structure as "hangs from", and
/// takes the other group in when one is chosen.
#[test]
fn a_string_hangs_from_another_groups_wire_point() {
    let model = synthetic_project_with("tak-top", |line| {
        let s = line.structures.iter_mut().find(|s| s.id == "T3").unwrap();
        let a = s.points[0].position;
        s.points.push(ohl_line::StructurePoint {
            description: String::new(),
            label: "B".into(),
            position: [a[0] + 2.0, a[1], a[2]],
            ..Default::default()
        });
        s.wires.push(ohl_line::WirePoint {
            label: "WS".into(),
            position: [a[0] + 2.0, a[1], a[2] - 3.0],
            ..Default::default()
        });
        s.strings.push(ohl_line::InsulatorString {
            label: "SUS".into(),
            top: "B".into(),
            wire: "WS".into(),
            ..Default::default()
        });
    });
    let mut f = crate::editors::StringForm::of(&model, "T3", 2).expect("set 2 of T3");
    assert!(!f.strings.iter().any(|x| x.label == "SUS"), "another group");
    let k = f.strings.iter().position(|x| x.wire == "W2").unwrap();
    assert!(f.top_choices(&model, k).contains(&"WS".to_owned()));
    f.hang_from(&model, k, "WS").unwrap();
    let x = &f.strings[k];
    assert_eq!(x.top, "WS");
    assert!(f.strings.iter().any(|x| x.label == "SUS"), "taken in");
    // Applied: the line holds.
    let mut line = model.line.clone();
    let resolved = model.line.structure("T3").cloned();
    let s = line.structures.iter_mut().find(|s| s.id == "T3").unwrap();
    f.apply_to(s, resolved.as_ref());
    line.validate().unwrap();
    let s = line.structure("T3").unwrap();
    assert_eq!(s.strings.iter().find(|y| y.wire == "W2").unwrap().top, "WS");
    assert!(s.strings.iter().any(|y| y.label == "SUS"));
}

/// Criteria › Calculation options: the jumpers' own pull chosen — one
/// command, undoable; the line's settings say so and the sections at the
/// jumper are solved with it.
#[test]
fn calculation_options_choose_the_jumpers_pull() {
    let model = synthetic_project("solver-options");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.get_by_label("Criteria").click();
    harness.run();
    harness.get_by_label("Calculation options…").click();
    harness.run();
    harness.get_by_label("The jumper's own pull").click();
    harness.run();
    harness.get_by_label("OK").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(
        r.model.line.solver.jumper_loads,
        ohl_line::JumperLoads::Pull
    );
    let stored = r.model.stored_line().unwrap();
    assert_eq!(stored.solver.jumper_loads, ohl_line::JumperLoads::Pull);
    r.perform(crate::edit::Action::Undo);
    assert_eq!(
        r.model.line.solver.jumper_loads,
        ohl_line::JumperLoads::Estimate
    );
}

/// Criteria › Calculation options: the ice and wind per unstretched
/// metre chosen (2026-10-21, as other programs) — warned about in the
/// window, one command, undoable, kept in the line's settings, and every
/// result made since is out of date naming the option.
#[test]
fn calculation_options_choose_the_ice_per_unstretched_metre() {
    let model = synthetic_project("loads-per");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let before = harness.state_mut().ready().unwrap().model.revision();
    harness.get_by_label("Criteria").click();
    harness.run();
    harness.get_by_label("Calculation options…").click();
    harness.run();
    harness
        .get_by_label("Per unstretched metre (as other programs)")
        .click();
    harness.run();
    assert!(
        harness
            .query_all_by_label_contains("slightly less sag and swing (unsafe side)")
            .next()
            .is_some()
    );
    harness.get_by_label("OK").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(
        r.model.line.solver.loads_per,
        ohl_line::LoadsPer::Unstretched
    );
    let stored = r.model.stored_line().unwrap();
    assert_eq!(stored.solver.loads_per, ohl_line::LoadsPer::Unstretched);
    let said = before
        .changed(&r.model.revision(), crate::revision::Uses::LINE)
        .unwrap();
    assert_eq!(
        said,
        "the calculation options (now ice and wind per unstretched metre (as other programs; a \
         simplification, about T/EA ≈ 0.1 % less, unsafe side))"
    );
    r.perform(crate::edit::Action::Undo);
    assert_eq!(r.model.line.solver.loads_per, ohl_line::LoadsPer::Stretched);
    assert_eq!(r.model.revision(), before, "back as it was");
}

/// The span tool: a span right-clicked where its wire is clicked gives a
/// window placing a span point there (by distance, as clicked); two span
/// points, one on each conductor of a span, joined with Add string make a
/// spacer (the project's default compression limit), solved and drawn;
/// regular point loads and a spread load by station; the tables list them
/// and change them; deleting a span point takes its spacer.
#[test]
fn spacers_are_laid_between_span_points() {
    use crate::editors::Form;
    use crate::span_tool::{PlaceBy, SpanWhat};
    use crate::tables::TableKind;
    use crate::tools::Tool;
    let model = synthetic_project_with("spacers", |_| {});
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let (s0, s1, on) = {
        let r = harness.state_mut().ready().unwrap();
        let l = &r.model.line;
        let (s0, s1) = (l.sections[0].id.clone(), l.sections[1].id.clone());
        let at = |k: usize| {
            l.sections[k].points[..2]
                .iter()
                .map(|p| p.structure.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(at(0), at(1));
        let points = &r.model.shape(&s0, r.case).unwrap().spans[0].points;
        // A point of the wire a third of the way along.
        let on = crate::scene::at_fraction(points, 1.0 / 3.0).unwrap();
        (s0, s1, on)
    };
    // Its window: a span point where the span was clicked.
    let add_point = |h: &mut Harness<'_, App>, section: &str| {
        let r = h.state_mut().ready().unwrap();
        r.hotbar.hold(Tool::OnSpan);
        r.use_tool(crate::selection::Use {
            target: Selection::Span {
                section: section.into(),
                span: 0,
            },
            at: None,
            on: Some(on),
        });
        let Some(Form::Span(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        assert_eq!(f.what, SpanWhat::Point);
        assert!(
            (f.from / f.horizontal - 1.0 / 3.0).abs() < 0.01,
            "{}",
            f.from
        );
        f.label = "M".into();
        h.run();
        press(h, egui::Key::Enter, egui::Modifiers::NONE);
        let r = h.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
    };
    add_point(&mut harness, &s0);
    add_point(&mut harness, &s1);
    {
        let r = harness.state_mut().ready().unwrap();
        let p = r.model.line.sections[0]
            .span_points
            .iter()
            .find(|p| p.label == "M")
            .unwrap();
        assert!(p.at.distance().is_some());
        // Solved where it is: on the wire.
        let shape = r.model.shape(&s0, r.case).unwrap();
        assert!(shape.span_points.iter().any(|(l, _)| l == "M"));
    }
    // Add string: one span point, then the other — the spacer's window.
    let point = |section: &str| Selection::SpanPoint {
        section: section.into(),
        label: "M".into(),
    };
    harness
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .defaults
        .spacer_compression = 7000.0;
    use_tool(&mut harness, Tool::AddString, point(&s0), None);
    assert!(harness.state_mut().ready().unwrap().spacer_from.is_some());
    use_tool(&mut harness, Tool::AddString, point(&s1), None);
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Spacer(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        assert_eq!(f.spacer.compression, Some(7000.0));
        f.spacer.weight = 150.0;
    }
    harness.run();
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        assert_eq!(r.model.line.spacers.len(), 1);
        let id = r.model.line.spacers[0].id.clone();
        let x = crate::selection::spacer_state(&r.model, r.case, &id).expect("solved");
        assert!(x.force.is_finite());
        // Drawn and selectable.
        let wires = crate::scene::prims(&r.model, crate::scene::Layer::Wires, r.case);
        assert!(wires.iter().any(|p| p.points == x.ends.to_vec()));
        let rows = TableKind::Spacers.rows(&r.model, r.case);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].cells[TableKind::Spacers.column("weight N")],
            "150.0"
        );
        assert!(!rows[0].cells[TableKind::Spacers.column("force N")].is_empty());
    }
    // The table changes it.
    let id = harness.state_mut().ready().unwrap().model.line.spacers[0]
        .id
        .clone();
    set_cell(&mut harness, TableKind::Spacers, &id, "compression N", "");
    apply_table(&mut harness, TableKind::Spacers, false).unwrap();
    assert_eq!(
        harness.state_mut().ready().unwrap().model.line.spacers[0].compression,
        None
    );
    // Point loads every 50 m and a spread load by station.
    let before = harness.state_mut().ready().unwrap().model.line.sections[0]
        .objects()
        .count();
    {
        let r = harness.state_mut().ready().unwrap();
        let mut f = crate::span_tool::SpanForm::new(&r.model, r.case, &s0, 0, None).unwrap();
        f.what = SpanWhat::Load;
        (f.regular, f.from, f.to, f.spacing) = (true, 50.0, 150.0, 50.0);
        assert_eq!(f.distances(), vec![50.0, 100.0, 150.0]);
        r.editor = Some(Form::Span(f));
    }
    harness.run();
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        assert_eq!(r.model.line.sections[0].objects().count(), before + 3);
        let mut f = crate::span_tool::SpanForm::new(&r.model, r.case, &s0, 0, None).unwrap();
        f.what = SpanWhat::Spread;
        if f.stations.is_some() {
            f.by = PlaceBy::Station;
        }
        (f.from, f.to, f.per_metre) = (10.0, 30.0, 25.0);
        r.editor = Some(Form::Span(f));
    }
    harness.run();
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        let l = r.model.line.sections[0].spreads.last().unwrap();
        assert_eq!(l.load, 25.0);
        let rows = TableKind::SpanObjects.rows(&r.model, r.case);
        // Every point and spread of every section.
        let all: usize = r
            .model
            .line
            .sections
            .iter()
            .map(|s| s.span_points.len() + s.spreads.len())
            .sum();
        assert_eq!(rows.len(), all);
    }
    // A load's place typed in the table: the first point added with a
    // body on S0.
    let k = {
        let r = harness.state_mut().ready().unwrap();
        let s = &r.model.line.sections[0];
        let first = s.objects().nth(before).unwrap().0.label.clone();
        s.span_points.iter().position(|p| p.label == first).unwrap()
    };
    let key = format!("{s0}\tp\t{k}");
    set_cell(&mut harness, TableKind::SpanObjects, &key, "at", "0.2");
    apply_table(&mut harness, TableKind::SpanObjects, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let p = &r.model.line.sections[0].span_points[k];
        assert_eq!(p.at, ohl_line::SpanPlace::Fraction(0.2));
    }
    // The Edit tool on a point opens its own window (not the section's):
    // relabelled and moved there.
    let old = harness.state_mut().ready().unwrap().model.line.sections[0].span_points[k]
        .label
        .clone();
    use_tool(
        &mut harness,
        Tool::Edit,
        Selection::SpanPoint {
            section: s0.clone(),
            label: old.clone(),
        },
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Span(f)) = &mut r.editor else {
            panic!("no point window: {:?}", r.editor.as_ref().map(Form::title));
        };
        assert_eq!(f.editing.as_deref(), Some(old.as_str()));
        assert_eq!(f.what, SpanWhat::Load);
        f.label = "EDITED".into();
        f.by = PlaceBy::Fraction;
        f.from = 0.3 * f.horizontal;
    }
    harness.run();
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        let p = &r.model.line.sections[0].span_points[k];
        assert_eq!(p.label, "EDITED");
        assert!(matches!(p.at, ohl_line::SpanPlace::Fraction(f) if (f - 0.3).abs() < 1e-9));
        assert!(p.object.is_some());
    }
    // Deleting a span point takes its spacer.
    {
        let r = harness.state_mut().ready().unwrap();
        r.hotbar.hold(Tool::Delete);
        r.use_tool(crate::selection::Use {
            target: point(&s1),
            at: None,
            on: None,
        });
    }
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.line.spacers.is_empty(), "{:?}", r.status);
    assert!(r.model.line.sections[1].span_points.is_empty());
    assert!(
        r.model.line.sections[0]
            .span_points
            .iter()
            .any(|p| p.label == "M")
    );
}

/// Criteria › Default values: every value of the project, changed in the
/// window and kept with OK (one command); the weather table's velocities
/// from the project's Q where a case gives none, a vertical clearance
/// window's zone from the project's width; the program's values are those
/// of each part of OpenOHL.
#[test]
fn the_project_default_values_are_used() {
    use crate::tables::TableKind;
    let d = ohl_line::Defaults::default();
    assert_eq!(
        d.jumper_exit_angle_deg,
        ohl_line::jumper::DEFAULT_TERMINAL_ANGLE_DEG
    );
    assert_eq!(
        d.insulator_stiffness,
        ohl_line::solve::DEFAULT_INSULATOR_STIFFNESS
    );
    assert_eq!(
        d.air_density_factor,
        ohl_weather::DEFAULT_AIR_DENSITY_FACTOR
    );
    assert_eq!(d.thermal_emissivity, ohl_thermal::DEFAULT_EMISSIVITY);
    assert_eq!(d.thermal_time_step, ohl_thermal::DEFAULT_TIME_STEP);
    assert_eq!(
        d.clearance_search_margin,
        ohl_clearance::rules::DEFAULT_SEARCH_MARGIN
    );
    assert_eq!(d.clearance_zone_width, ohl_clearance::zone::DEFAULT_WIDTH);
    let model = synthetic_project("default-values");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let mut d = r.model.line.defaults;
        d.air_density_factor = 0.5;
        d.clearance_zone_width = 2.5;
        r.defaults_window = Some((d, None));
    }
    harness.run();
    harness.get_by_label("air density factor Q");
    harness.get_by_label("OK").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.defaults_window.is_none());
    assert_eq!(r.model.line.defaults.air_density_factor, 0.5);
    let s = &r.model.editing.as_ref().unwrap().session;
    assert_eq!(s.line().defaults.clearance_zone_width, 2.5);
    // A weather case without its own Q: V = √(P / 0.5).
    let rows = TableKind::WeatherCases.rows(&r.model, r.case);
    let (k, w) = r
        .model
        .cases
        .iter()
        .map(|c| &c.case)
        .enumerate()
        .find(|(_, c)| c.air_density_factor.is_none() && c.wind_pressure.si() > 0.0)
        .expect("a windy case without its own Q");
    let v = (w.wind_pressure.si() / 0.5).sqrt();
    assert_eq!(rows[k].cells[crate::tables::W_VELOCITY], format!("{v:.2}"));
    // A vertical clearance window's zone.
    r.vertical_clearances.show(&r.model);
    assert_eq!(r.vertical_clearances.zone.width, 2.5);
}

/// Points on a jumper: the span tool on a jumper puts a point with a tap
/// clamp (a template) there; it is picked like any point; Add jumper from
/// that clamp to a string makes a jumper on the tap (given by its length),
/// solved; the table edits its shape (advanced: its own values then);
/// deleting the point takes the jumper on it.
#[test]
fn a_jumper_starts_from_a_tap_on_another_jumper() {
    use crate::editors::Form;
    use crate::span_tool::SpanWhat;
    use crate::tables::TableKind;
    use crate::tools::Tool;
    let model = synthetic_project("jumper-tap");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let (host, end) = {
        let r = harness.state_mut().ready().unwrap();
        let j = &r.model.line.jumpers[0];
        (j.id.clone(), j.from.point().unwrap().clone())
    };
    {
        let r = harness.state_mut().ready().unwrap();
        let mut f = crate::span_tool::SpanForm::on_jumper(&r.model, r.case, &host, None).unwrap();
        f.what = SpanWhat::Load;
        f.object = f.library.get("tap-clamp").unwrap().object().unwrap();
        f.label = "K".into();
        r.editor = Some(Form::Span(f));
    }
    harness.run();
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    let point = Selection::JumperPoint {
        jumper: host.clone(),
        label: "K".into(),
    };
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        let j = &r.model.line.jumpers[0];
        let p = j.points.iter().find(|p| p.label == "K").unwrap();
        assert_eq!(p.object.as_ref().unwrap().kind, ohl_line::ObjectKind::Clamp);
        // Solved there, a candidate to click.
        r.model.ensure_solved(r.case);
        assert!(point.focus(&r.model, r.case).is_some());
        let snaps = r.effective_snaps();
        assert!(
            crate::selection::candidates(&r.model, r.case, &snaps)
                .iter()
                .any(|c| c.0 == point)
        );
    }
    // Add jumper: the clamp, then a string.
    let n = harness
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .jumpers
        .len();
    use_tool(&mut harness, Tool::AddJumper, point.clone(), None);
    assert!(harness.state_mut().ready().unwrap().jumper_tap.is_some());
    use_tool(
        &mut harness,
        Tool::AddJumper,
        string_at(&end.structure, end.set),
        None,
    );
    let tapped = {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.model.line.jumpers.len(), n + 1, "{:?}", r.status);
        let j = r.model.line.jumpers.last().unwrap().clone();
        assert!(
            matches!(j.from.as_tap(), Some(ohl_line::Tap::Jumper { jumper, point }) if *jumper == host && point == "K")
        );
        assert!(j.length.is_some() && j.sag.is_none());
        r.model.ensure_solved(r.case);
        assert!(r.model.jumper_shape(&j.id, r.case).is_some(), "solved");
        j.id
    };
    // The table: its shape typed in.
    let key = {
        let r = harness.state_mut().ready().unwrap();
        let k = r.model.line.jumpers[0]
            .points
            .iter()
            .position(|p| p.label == "K")
            .unwrap();
        format!("{host}\tj\t{k}")
    };
    set_cell(&mut harness, TableKind::SpanObjects, &key, "shape", "box");
    set_cell(&mut harness, TableKind::SpanObjects, &key, "size 1", "0.3");
    apply_table(&mut harness, TableKind::SpanObjects, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let o = r.model.line.jumpers[0]
            .points
            .iter()
            .find(|p| p.label == "K")
            .unwrap()
            .object
            .clone()
            .unwrap();
        assert!(matches!(o.shape, ohl_line::Shape::Box { along, .. } if along == 0.3));
        assert_eq!(o.type_id, None, "its own values now");
        assert_eq!(o.kind, ohl_line::ObjectKind::Clamp);
    }
    // Deleting the point takes the jumper on it.
    {
        let r = harness.state_mut().ready().unwrap();
        r.hotbar.hold(Tool::Delete);
        r.use_tool(crate::selection::Use {
            target: point,
            at: None,
            on: None,
        });
    }
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(
        r.model.line.jumpers.iter().all(|j| j.id != tapped),
        "{:?}",
        r.status
    );
    assert!(
        r.model.line.jumpers[0]
            .points
            .iter()
            .all(|p| p.label != "K")
    );
}

/// Bodies drawn by their shape: a box and a cylinder as closed solids
/// facing out, oriented along the wire; a point nothing.
#[test]
fn bodies_are_drawn_by_their_shape() {
    use crate::scene::body_mesh;
    let at = [10.0, 20.0, 30.0];
    let solid = |s: &ohl_line::Shape| {
        let t = body_mesh(s, at, [0.0, 1.0]);
        assert!(!t.is_empty() && t.len().is_multiple_of(3));
        for tri in t.as_chunks::<3>().0 {
            let u = [0, 1, 2].map(|k| tri[1][k] - tri[0][k]);
            let v = [0, 1, 2].map(|k| tri[2][k] - tri[0][k]);
            let n = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            let m = [0, 1, 2].map(|k| (tri[0][k] + tri[1][k] + tri[2][k]) / 3.0 - at[k]);
            assert!(n[0] * m[0] + n[1] * m[1] + n[2] * m[2] >= -1e-12);
        }
        t
    };
    let b = solid(&ohl_line::Shape::Box {
        along: 2.0,
        across: 0.5,
        height: 0.3,
    });
    // Along the wire (y here): 2 m.
    let ys = b.iter().map(|p| p[1] - at[1]);
    let span = ys.clone().fold(f64::MIN, f64::max) - ys.fold(f64::MAX, f64::min);
    assert!((span - 2.0).abs() < 1e-9, "{span}");
    let c = solid(&ohl_line::Shape::Cylinder {
        diameter: 0.2,
        length: 1.0,
        axis: ohl_line::Axis::Vertical,
    });
    let zs = c.iter().map(|p| p[2] - at[2]);
    let h = zs.clone().fold(f64::MIN, f64::max) - zs.fold(f64::MAX, f64::min);
    assert!((h - 1.0).abs() < 1e-9);
    assert!(body_mesh(&ohl_line::Shape::Point, at, [1.0, 0.0]).is_empty());
}

/// The point and spread load types: the templates listed; one changed
/// becomes the project's own (kept in the project, used by the span
/// tool); a new type added; the project's copy deleted brings the
/// template back; a template cannot be deleted.
#[test]
fn load_types_are_the_templates_and_the_projects_own() {
    use crate::tables::TableKind;
    let model = synthetic_project("load-types");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let col = |n: &str| TableKind::LoadTypes.column(n);
    {
        let r = harness.state_mut().ready().unwrap();
        let rows = TableKind::LoadTypes.rows(&r.model, r.case);
        let ball = rows.iter().find(|x| x.key == "marker-ball-600").unwrap();
        assert_eq!(ball.cells[col("source")], "template");
        assert_eq!(ball.cells[col("shape")], "sphere");
    }
    set_cell(
        &mut harness,
        TableKind::LoadTypes,
        "marker-ball-600",
        "weight N",
        "80",
    );
    apply_table(&mut harness, TableKind::LoadTypes, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let lib = r.model.object_library();
        assert_eq!(lib.get("marker-ball-600").unwrap().weight, 80.0);
        let rows = TableKind::LoadTypes.rows(&r.model, r.case);
        let ball = rows.iter().find(|x| x.key == "marker-ball-600").unwrap();
        assert_eq!(ball.cells[col("source")], "project");
        let files = r
            .model
            .editing
            .as_ref()
            .unwrap()
            .session
            .library_files("objects")
            .unwrap();
        assert_eq!(files.len(), 1);
        // A new type.
        let mut row = TableKind::LoadTypes.new_row(&r.model, r.case, &[]);
        row.resize(TableKind::LoadTypes.columns().len(), String::new());
        row[col("id")] = "lamp".into();
        row[col("name")] = "Lamp".into();
        row[col("kind")] = "body".into();
        row[col("weight N")] = "60".into();
        row[col("shape")] = "box".into();
        row[col("size 1")] = "0.3".into();
        row[col("size 2")] = "0.2".into();
        row[col("size 3")] = "0.4".into();
        r.tables
            .entry(TableKind::LoadTypes)
            .or_default()
            .added
            .push(row);
    }
    apply_table(&mut harness, TableKind::LoadTypes, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let lamp = r.model.object_library().get("lamp").cloned().unwrap();
        assert!(matches!(lamp.shape, Some(ohl_line::Shape::Box { height, .. }) if height == 0.4));
        // The span tool offers it.
        let s0 = r.model.line.sections[0].id.clone();
        let f = crate::span_tool::SpanForm::new(&r.model, r.case, &s0, 0, None).unwrap();
        assert!(f.library.get("lamp").is_some());
        // A template cannot be deleted; the project's copy can.
        r.tables
            .entry(TableKind::LoadTypes)
            .or_default()
            .deleted
            .insert("weight-50kg".into());
    }
    assert!(apply_table(&mut harness, TableKind::LoadTypes, false).is_err());
    {
        let r = harness.state_mut().ready().unwrap();
        let page = r.tables.entry(TableKind::LoadTypes).or_default();
        page.clear();
        page.deleted.insert("marker-ball-600".into());
    }
    apply_table(&mut harness, TableKind::LoadTypes, false).unwrap();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(
        r.model
            .object_library()
            .get("marker-ball-600")
            .unwrap()
            .weight,
        70.0
    );
}

/// The example's typed line as a project, its types (and one more, a copy
/// of type-02 with two extensions, `ext-02`) copied into it.
pub(crate) fn typed_project(name: &str) -> model::Model {
    typed_project_with(name, |_| {})
}

/// [`typed_project`] with its structure files (the folder given) changed
/// first.
pub(crate) fn typed_project_with(name: &str, edit: impl FnOnce(&Path)) -> model::Model {
    typed_project_edited(name, edit, |_| {})
}

/// [`typed_project_with`], its line changed first too (`line`).
pub(crate) fn typed_project_edited(
    name: &str,
    edit: impl FnOnce(&Path),
    line: impl FnOnce(&mut ohl_line::Line),
) -> model::Model {
    let edit_line = line;
    let ex = root().join("examples/synthetic-line");
    let dir = std::env::temp_dir().join(format!("ohl-app-{}-{name}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(dir.join("Structures")).unwrap();
    for e in std::fs::read_dir(ex.join("Structures")).unwrap() {
        let p = e.unwrap().path();
        std::fs::copy(&p, dir.join("Structures").join(p.file_name().unwrap())).unwrap();
    }
    let text = std::fs::read_to_string(ex.join("Structures/type-02.ohls")).unwrap();
    let body = text
        .lines()
        .skip_while(|l| l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
        .replacen("id = \"type-02\"", "id = \"ext-02\"", 1)
        .replacen(
            "name = \"type-02 (2 structures, e.g. T2)\"",
            "name = \"Tall suspension tower\"",
            1,
        );
    std::fs::write(
        dir.join("Structures/ext-02.ohls"),
        format!(
            "# A taller copy of type-02 for road crossings.\n{body}\n\n\
             [[extension]]\nname = \"+3\"\ndz = \"3 m\"\n\n\
             [[extension]]\nname = \"+6\"\ndz = \"6 m\"\n"
        ),
    )
    .unwrap();
    edit(&dir.join("Structures"));
    let file = dir.join("p.ohl");
    let mut line = ohl_line::Line::load(&ex.join("synthetic-line-typed.ohl.json")).unwrap();
    edit_line(&mut line);
    ohl_session::Session::open(&file)
        .unwrap()
        .import(&line, "tester")
        .unwrap();
    solved_all(
        model::load(
            Sources {
                file,
                library: Some(root().join("libraries/cables")),
                weather: Some(ex.join("weather-cases.ohlwc")),
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

/// The structure picker lists the library's types with what they are,
/// filters on any words of their id, name, kind or description and sorts
/// by a column; the description comes from the notes at the top of the
/// file, the level and what the next one lacks from the content.
#[test]
fn the_structure_picker_lists_filters_and_sorts_the_types() {
    use crate::structure_picker::{Column, Picker, TypeRow};
    let m = typed_project("picker-list");
    let mut p = Picker::new("t", Some("type-02"), None, true, true);
    let all = p.shown_ids(&m, "");
    assert_eq!(all.len(), 7, "{all:?}");
    assert_eq!(all[0], "ext-02");
    assert_eq!(p.shown_ids(&m, "taller ROAD"), ["ext-02"]);
    assert_eq!(p.shown_ids(&m, "suspension tower"), ["ext-02"]);
    assert_eq!(p.shown_ids(&m, "e.g. T3"), ["type-03"]);
    assert!(p.shown_ids(&m, "nothing like this").is_empty());
    // The most used first.
    p.sort = (Column::Used, false);
    assert_eq!(p.shown_ids(&m, "")[0], "type-02");
    let (types, insulators) = m.types.as_ref().unwrap();
    let t = types.get("ext-02").unwrap();
    let row = TypeRow::of(t, insulators, types.path("ext-02"));
    assert_eq!(
        row.description,
        "A taller copy of type-02 for road crossings."
    );
    assert_eq!(row.extensions.len(), 2);
    assert_eq!(row.level, t.detail().level());
    assert!(row.height > 10.0, "{}", row.height);
    let t1 = types.get("type-01").unwrap();
    let row = TypeRow::of(t1, insulators, types.path("type-01"));
    assert_eq!(row.level, 1);
    assert_eq!(row.lacks.len(), 1, "{:?}", row.lacks);
    assert!(row.sets > 0 && row.wire_points >= row.sets);
}

/// Add structure: the type's button opens the structure picker; a filter
/// typed narrows the table, a row clicked shows it, Choose puts it (and
/// the extension ticked) in the window, and the structure is added with
/// that type. Escape closes the picker, not the window (OHL_RENDER_PNG: an
/// image of the picker).
#[test]
fn add_structure_chooses_its_type_in_the_structure_picker() {
    use crate::editors::Form;
    use crate::tools::Tool;
    let model = typed_project("picker-add");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    crate::structure_picker::Picker::set_filter(&harness.ctx, "");
    let (t4, t5) = {
        let r = harness.state_mut().ready().unwrap();
        let l = &r.model.line;
        (
            l.structure("T4").unwrap().position,
            l.structure("T5").unwrap().position,
        )
    };
    let at = [2.0 * t5[0] - t4[0], 2.0 * t5[1] - t4[1], t5[2]];
    use_tool(&mut harness, Tool::AddStructure, Selection::None, Some(at));
    let button = {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Structure(f)) = &r.editor else {
            panic!("{:?}", r.editor);
        };
        let id = f
            .structure_type
            .clone()
            .expect("the nearest structure's type");
        let name = r
            .model
            .types
            .as_ref()
            .unwrap()
            .0
            .get(&id)
            .unwrap()
            .name
            .clone();
        format!("{id} — {name} …")
    };
    harness.get_by_label(&button).click();
    harness.run();
    assert!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .structure_picker
            .is_some()
    );
    harness.get_by_label("Choose");
    // Escape: the picker closes, the structure window stays.
    press(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.structure_picker.is_none());
        assert!(matches!(r.editor, Some(Form::Structure(_))));
    }
    harness.get_by_label(&button).click();
    harness.run();
    // The filter has the keyboard: typed, the table narrows.
    harness
        .input_mut()
        .events
        .push(egui::Event::Text("crossings".into()));
    harness.run();
    assert_eq!(
        crate::structure_picker::Picker::filter(&harness.ctx),
        "crossings"
    );
    harness.get_by_label("1 of 7 types");
    assert!(harness.query_by_label("type-03").is_none());
    harness.get_by_label("ext-02").click();
    harness.run();
    // Its details under the preview; the extension chosen here.
    assert_eq!(
        harness.query_all_by_label("Tall suspension tower").count(),
        2
    );
    assert_eq!(
        harness
            .query_all_by_label("A taller copy of type-02 for road crossings.")
            .count(),
        2
    );
    harness.get_by_label("+6").click();
    harness.run();
    if let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG")
        && let Ok(image) = harness.render()
    {
        let path = PathBuf::from(dir).join("ohl-design-structure-picker.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
    harness.get_by_label("Choose").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.structure_picker.is_none(), "Choose closes it");
        let Some(Form::Structure(f)) = &r.editor else {
            panic!("{:?}", r.editor);
        };
        assert_eq!(f.structure_type.as_deref(), Some("ext-02"));
        assert_eq!(f.extension.as_deref(), Some("+6"));
    }
    harness.get_by_label("ext-02 — Tall suspension tower …");
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    let r = harness.state_mut().ready().unwrap();
    assert!(r.editor.is_none(), "{:?}", r.status);
    let s = r.model.line.structure("T8").unwrap();
    assert_eq!(s.structure_type.as_deref(), Some("ext-02"));
    assert_eq!(s.extension.as_deref(), Some("+6"));
    // The filter is remembered for the session.
    assert_eq!(
        crate::structure_picker::Picker::filter(&harness.ctx),
        "crossings"
    );
}

/// The Structures table's type cell opens the structure picker; the type
/// (and extension) chosen are pending edits of the row, applied as one
/// change that one undo takes back.
#[test]
fn the_structures_table_chooses_a_type_in_the_structure_picker() {
    use crate::tables::{TableKind, TypeCell};
    let model = typed_project("picker-table");
    // Short steps: two clicks a double-click.
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 1000.0))
        .with_step_dt(0.02)
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    crate::structure_picker::Picker::set_filter(&harness.ctx, "");
    harness.state_mut().ready().unwrap().open_page =
        Some((crate::app::Tab::Table(TableKind::Structures), None));
    harness.run();
    harness.run();
    // T1 alone has type-01: its cell opens the picker on it.
    harness.get_by_label("type-01 …").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let (cell, picker) = r.tables[&TableKind::Structures]
            .type_picker
            .as_ref()
            .expect("the picker");
        assert!(matches!(cell, TypeCell::Row { key, .. } if key == "T1"));
        assert_eq!(picker.selected.as_deref(), Some("type-01"));
    }
    harness.get_by_label("Cancel").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let page = &r.tables[&TableKind::Structures];
        assert!(page.type_picker.is_none(), "Cancel closes it");
        assert_eq!(page.pending(), 0, "{:?}", page.edits);
    }
    // T4 (type-02): ext-02 with +3, chosen by a double-click on its row.
    {
        let r = harness.state_mut().ready().unwrap();
        r.tables
            .get_mut(&TableKind::Structures)
            .unwrap()
            .open_type_picker(
                TypeCell::Row {
                    key: "T4".into(),
                    structure_type: "type-02".into(),
                    extension: String::new(),
                },
                "type-02",
                "",
            );
    }
    harness.run();
    harness.get_by_label("ext-02").click();
    harness.run();
    harness.get_by_label("+3").click();
    harness.run();
    harness.get_by_label("ext-02").click();
    harness.get_by_label("ext-02").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let page = &r.tables[&TableKind::Structures];
        assert!(
            page.type_picker.is_none(),
            "a double-click chooses: {:?} {:?}",
            page.type_picker
                .as_ref()
                .map(|p| (&p.1.selected, &p.1.extension)),
            page.edits
        );
        let e = &page.edits["T4"];
        assert_eq!(e[&TableKind::Structures.column("type")], "ext-02");
        assert_eq!(e[&TableKind::Structures.column("extension")], "+3");
    }
    apply_table(&mut harness, TableKind::Structures, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let s = r.model.line.structure("T4").unwrap();
        assert_eq!(s.structure_type.as_deref(), Some("ext-02"));
        assert_eq!(s.extension.as_deref(), Some("+3"));
    }
    press(&mut harness, egui::Key::Z, egui::Modifiers::COMMAND);
    let r = harness.state_mut().ready().unwrap();
    let s = r.model.line.structure("T4").unwrap();
    assert_eq!(s.structure_type.as_deref(), Some("type-02"), "one undo");
    assert_eq!(s.extension, None);
}

/// Replace structure: the tool on T2 (type-02, sets 1 and 11) opens the
/// structure picker; type-04 has both sets and a joint at each of its
/// attachment points, so Choose replaces it at once and the status line
/// says how many wires were rewired; one undo takes it back.
#[test]
fn replace_structure_with_every_set_matching_rewires_at_once() {
    use crate::tools::Tool;
    let model = typed_project("replace-match");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    crate::structure_picker::Picker::set_filter(&harness.ctx, "type-04");
    use_tool(
        &mut harness,
        Tool::Replace,
        Selection::Structure("T2".into()),
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let rep = r.replace.as_ref().expect("the picker");
        let crate::replace::Stage::Pick(p) = &rep.stage else {
            panic!("the picker first");
        };
        assert_eq!(p.selected.as_deref(), Some("type-02"), "its type");
    }
    harness.get_by_label("type-04").click();
    harness.run();
    harness.get_by_label("Choose").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.replace.is_none(), "no mapping needed");
    let (status, error) = r.status.clone().unwrap();
    assert!(!error && status.contains("rewired 2 wires"), "{status}");
    let s = r.model.line.structure("T2").unwrap();
    assert_eq!(s.structure_type.as_deref(), Some("type-04"));
    // Resolved from its new type: its points linked there.
    assert!(s.points.iter().all(ohl_line::StructurePoint::is_linked));
    press(&mut harness, egui::Key::Z, egui::Modifiers::COMMAND);
    let r = harness.state_mut().ready().unwrap();
    let s = r.model.line.structure("T2").unwrap();
    assert_eq!(s.structure_type.as_deref(), Some("type-02"), "one undo");
}

/// Replace structure on T3 (type-03, sets 1, 11 and 2) by type-02 (sets
/// 1 and 11), from its Properties' action: the mapping window lists the
/// old wire points with what ends there, proposes none for set 2, lists
/// S3 and its jumper as removed (drawn red in the 3D view); Escape
/// changes nothing; Replace removes them as one undoable change.
#[test]
fn replace_structure_maps_wire_points_and_lists_what_goes() {
    use crate::replace::Stage;
    let model = typed_project("replace-map");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    crate::structure_picker::Picker::set_filter(&harness.ctx, "type-02");
    let open = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        r.perform(crate::edit::Action::ReplaceStructure { id: "T3".into() });
        h.run();
        h.get_by_label("type-02").click();
        h.run();
        h.get_by_label("Choose").click();
        h.run();
    };
    open(&mut harness);
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Stage::Map(m)) = r.replace.as_ref().map(|x| &x.stage) else {
            panic!("the mapping window: {:?}", r.status);
        };
        assert_eq!(
            m.choice.iter().map(|(a, b)| (*a, *b)).collect::<Vec<_>>(),
            vec![(1, Some(1)), (2, None), (11, Some(11))]
        );
        let i = m.impact.as_ref().unwrap();
        assert_eq!(i.sections, vec!["S3"]);
        assert_eq!(i.jumpers, vec!["J-T3-branch"]);
        assert!(!r.view3d.removed.is_empty(), "drawn red");
    }
    harness.get_by_label("Replace structure T3 by type-02");
    harness.get_by_label("S3, jumper J-T3-branch");
    harness.get_by_label("  S3");
    if let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG")
        && let Ok(image) = harness.render()
    {
        let path = PathBuf::from(dir).join("ohl-design-replace-structure.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
    // Escape: closed, nothing changed, nothing drawn red.
    press(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.replace.is_none());
        assert!(r.view3d.removed.is_empty());
        assert!(r.model.line.sections.iter().any(|s| s.id == "S3"));
    }
    open(&mut harness);
    // Its attachment points 2.5 cm off type-02's joints (A2 far from
    // any): Replace waits until each is chosen — those type-02 has by
    // label as it has them, A2 removed.
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Stage::Map(m)) = r.replace.as_mut().map(|x| &mut x.stage) else {
            panic!("the mapping window");
        };
        assert!(!m.points_chosen());
        for (k, row) in m.points.clone().iter().enumerate() {
            m.point_choice[k] = Some(match &row.by_type {
                Some((joint, offset)) => ohl_line::links::PointChoice::Link {
                    joint: joint.clone(),
                    offset: *offset,
                },
                None => ohl_line::links::PointChoice::Remove,
            });
        }
    }
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Stage::Map(m)) = r.replace.as_ref().map(|x| &x.stage) else {
            panic!("the mapping window");
        };
        assert!(m.impact.is_ok(), "{:?}", m.impact);
        assert!(m.points_chosen(), "{:?}", m.point_choice);
    }
    harness.get_by_label("Replace").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.replace.is_none(), "{:?}", r.status);
        let l = &r.model.line;
        assert_eq!(
            l.structure("T3").unwrap().structure_type.as_deref(),
            Some("type-02")
        );
        assert!(l.sections.iter().all(|s| s.id != "S3"));
        assert!(l.jumpers.is_empty());
        let (status, _) = r.status.clone().unwrap();
        assert!(status.contains("rewired 2 wires"), "{status}");
    }
    press(&mut harness, egui::Key::Z, egui::Modifiers::COMMAND);
    let r = harness.state_mut().ready().unwrap();
    assert!(
        r.model.line.sections.iter().any(|s| s.id == "S3"),
        "one undo"
    );
    assert_eq!(r.model.line.jumpers.len(), 1);
}

/// A section that stops solving (after creep, with no creep case) is
/// drawn in each case in its last solved shape, dashed red, and a click
/// on that copy picks its span (the copy's case remembered); a section
/// never solved anywhere is drawn as the straight chords.
#[test]
fn a_section_not_solved_is_drawn_dashed_where_it_was() {
    use crate::scene::{Layer, UNSOLVED_COLOR, prims};
    use crate::selection::{Snaps, pick_in_model_case};
    use ohl_line::condition::Calculation;
    let mut m = synthetic();
    let before = m.cases[0]
        .shapes
        .iter()
        .find(|s| s.section == "S1")
        .unwrap()
        .clone();
    m.line.sections[0].calculation = Calculation::CREEP_RS;
    let (line, library) = (m.line.clone(), m.library.clone());
    for c in &mut m.cases {
        c.update(&line, &library, &["S1".to_owned()]);
    }
    let c = &m.cases[0];
    assert!(c.shapes.iter().all(|s| s.section != "S1"));
    assert_eq!(c.unsolved.len(), 1, "{:?}", c.unsolved);
    assert_eq!(c.unsolved[0].0, "S1");
    let d = m.unsolved_drawn("S1", 0, 0).unwrap();
    assert!(d.from.contains("last solved"), "{}", d.from);
    assert!(d.reason.contains("Creep-Stretch"), "{}", d.reason);
    assert_eq!(d.spans[0], before.spans[0].points);
    // Drawn: red dashes (more pieces than the span has points apart).
    let red: Vec<_> = prims(&m, Layer::Wires, 0)
        .into_iter()
        .filter(|p| p.color[..3] == UNSOLVED_COLOR)
        .collect();
    assert!(red.len() > before.spans.len() * 2, "{}", red.len());
    // Solved ones are not.
    assert!(m.unsolved_drawn("S2", 0, 0).is_none());
    // A click on the dashed copy (seen from above) picks its span.
    let pts = &d.spans[1];
    let mid = pts[pts.len() / 2];
    let (picked, _) = pick_in_model_case(&m, 0, &Snaps::default(), [mid[0], mid[1]], 0.5, |p| {
        Some([p[0], p[1]])
    });
    assert_eq!(
        picked,
        Selection::Span {
            section: "S1".into(),
            span: 1
        }
    );
    // Shown in two cases: each copy drawn and picked, with its case.
    m.shown_cases = vec![0, 1];
    let d1 = m.unsolved_drawn("S1", 1, 0).unwrap();
    let mid1 = d1.spans[1][d1.spans[1].len() / 2];
    let (picked, k) = pick_in_model_case(&m, 0, &Snaps::default(), [mid1[0], mid1[1]], 0.05, |p| {
        Some([p[0], p[1]])
    });
    assert!(matches!(picked, Selection::Span { ref section, .. } if section == "S1"));
    if d1.spans[1] != d.spans[1] {
        assert_eq!(k, Some(1));
    }
    // Solved from scratch in every case (nothing before): the chords.
    for k in 0..m.cases.len() {
        m.cases[k] = model::Solved::new(&m.line, &m.library, m.cases[k].case.clone());
    }
    let d = m.unsolved_drawn("S1", 0, 0).unwrap();
    assert!(d.from.contains("chords"), "{}", d.from);
    assert!(d.spans.iter().all(|s| s.len() == 2));
}

/// Checks › Structure loads with the ice case (after creep) not solved:
/// the Usage page shows the structures as "not checked" in ice (grey,
/// the sections not solved named), and coloured by usage they are
/// marked so in the views.
#[test]
fn the_usage_page_shows_structures_not_checked() {
    use crate::loads::UsageMark;
    let model = synthetic_project("incomplete");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.get_by_label("Checks").click();
    harness.run();
    harness.get_by_label("Compute structure loads").click();
    run_until(&mut harness, |a| {
        let r = a.ready().unwrap();
        r.loads.poll();
        r.loads.results.is_some()
    });
    harness.step();
    {
        let r = harness.state_mut().ready().unwrap();
        let results = r.loads.results.as_ref().unwrap().as_ref().unwrap();
        let ice = results.iter().position(|c| c.case.name == "ice").unwrap();
        assert!(
            results[ice].is_incomplete("T1"),
            "{:?}",
            results[ice].failed
        );
        assert!(!results[0].is_incomplete("T1"));
        // The usage of ice: T1 incomplete, the everyday case's not.
        assert!(
            r.loads.usage[ice]
                .structures
                .iter()
                .all(|s| s.is_incomplete())
        );
        assert!(
            r.loads.usage[0]
                .structures
                .iter()
                .all(|s| !s.is_incomplete())
        );
        r.loads.show_usage = true;
        r.loads.usage_case = ice + 1;
    }
    harness.step();
    harness.step();
    assert!(harness.query_all_by_label("not checked").next().is_some());
    assert!(
        harness
            .query_all_by_label_contains("not checked: S1 not solved, S2 not solved in ice")
            .next()
            .is_some()
    );
    // Coloured by usage: incomplete, the legend's neutral colour.
    harness.get_by_label("Colour structures by usage").click();
    harness.step();
    harness.step();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.loads.colour_structures);
    assert_eq!(r.model.usage_marks.get("T1"), Some(&UsageMark::Incomplete));
    assert_eq!(
        UsageMark::Incomplete.label(),
        "not checked: a wire not solved"
    );
}

/// The sheet preview: off by default; ticked in the Profile page's Sheets
/// menu (which stays open), it shows the export's sheets over the profile
/// and the plan — exactly those File › Export › Drawings… writes, warnings
/// included — and follows the export settings as they change.
#[test]
fn the_sheet_preview_shows_the_exports_sheets_over_the_profile_and_plan() {
    let model = synthetic();
    // On the GPU when there is one (images with OHL_RENDER_PNG).
    let mut builder = Harness::builder().with_size(egui::vec2(1400.0, 900.0));
    if ohl_render::headless_device().is_ok() {
        builder = builder.wgpu();
    }
    let mut harness = builder.build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.state_mut().show_tab(crate::app::Tab::Profile);
    harness.run_steps(3);
    // Off by default: nothing drawn, no preview made.
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(!r.drawings.preview_on);
        assert!(r.profile.sheets_drawn.is_empty());
    }
    assert!(harness.query_by_label_contains("Sheet preview:").is_none());
    // Ticked in the Profile page's Sheets menu: the menu stays open.
    harness.get_by_label("Sheets").click();
    harness.run();
    harness.get_by_label(crate::drawings::PREVIEW_TICK).click();
    harness.run_steps(3);
    harness.get_by_label(crate::drawings::PREVIEW_TICK);
    assert!(harness.state_mut().ready().unwrap().drawings.preview_on);
    crate::ux_tests::shot(&mut harness, "sheet-preview-profile");
    // The sheets of the alignments shown, as the export lays them out (A3
    // at 1:2000 / 1:200 by default).
    let (shown, case) = {
        let r = harness.state_mut().ready().unwrap();
        let shown: Vec<(String, usize, f64, f64, bool)> = r.profile.sheets_drawn.clone();
        (shown, r.case)
    };
    assert!(shown.len() >= 2, "{shown:?}");
    let id = shown[0].0.clone();
    harness.get_by_label_contains(&format!("Sheet preview: {} sheet(s) A3", shown.len()));
    // The export writes the same sheets, with the same warnings.
    let dir = std::env::temp_dir().join(format!("ohl-app-sheet-preview-{}", std::process::id()));
    let (written, preview) = {
        let r = harness.state_mut().ready().unwrap();
        let routes = r.model.line.alignment_views();
        let k = routes.iter().position(|a| a.id == id).unwrap();
        let w = crate::drawings::DrawingsWindow {
            route: Some(k),
            formats: vec![ohl_drawing::export::Format::Svg],
            out: Some(dir.clone()),
            ..r.drawings.settings().clone()
        };
        let input = crate::drawings::Input::new(&r.model, case, &w).unwrap();
        let report = crate::drawings::run(&input, &|_, _| {}).unwrap();
        let preview = crate::drawings::preview(&r.model, case, &w).unwrap();
        (report, preview)
    };
    let route = preview.route(&id).unwrap();
    let sheet_files = std::fs::read_dir(&dir)
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("-sheet-")
        })
        .count();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(sheet_files, route.sheets.len());
    let profile_sheets: Vec<_> = shown.iter().filter(|s| s.0 == id).collect();
    assert_eq!(profile_sheets.len(), route.sheets.len());
    for (s, l) in profile_sheets.iter().zip(&route.sheets) {
        // (One alignment shown first: no shift.)
        assert_eq!((s.1, s.4), (l.number, !l.warnings.is_empty()));
    }
    let exported: Vec<&str> = written
        .lines()
        .filter_map(|l| l.strip_prefix("⚠ "))
        .collect();
    let previewed: Vec<String> = preview
        .warnings()
        .into_iter()
        .filter(|w| w.starts_with(&format!("route {id}: ")))
        .collect();
    assert_eq!(exported, previewed);
    // The settings change (A4, window closed: those it had): the
    // preview follows.
    harness.state_mut().ready().unwrap().drawings.last.page = "A4".into();
    // (Drawn again once the settings have settled.)
    harness.run_steps(1);
    std::thread::sleep(crate::drawings::PREVIEW_SETTLE);
    harness.run_steps(3);
    let a4 = harness
        .state_mut()
        .ready()
        .unwrap()
        .profile
        .sheets_drawn
        .iter()
        .filter(|s| s.0 == id)
        .count();
    assert!(a4 > profile_sheets.len(), "{a4}");
    // The window opened again starts from the settings kept.
    harness.state_mut().ready().unwrap().drawings.open();
    harness.run();
    assert_eq!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .drawings
            .window
            .as_ref()
            .unwrap()
            .page,
        "A4"
    );
    // The plan shows them too.
    harness.state_mut().show_tab(crate::app::Tab::Plan);
    harness.run_steps(3);
    assert!(harness.state_mut().ready().unwrap().plan.sheets_drawn > 0);
    crate::ux_tests::shot(&mut harness, "sheet-preview-plan");
    // One continuous drawing: no sheets, said so.
    harness
        .state_mut()
        .ready()
        .unwrap()
        .drawings
        .window
        .as_mut()
        .unwrap()
        .page = String::new();
    harness.state_mut().show_tab(crate::app::Tab::Profile);
    harness.run_steps(1);
    std::thread::sleep(crate::drawings::PREVIEW_SETTLE);
    harness.run_steps(3);
    assert!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .profile
            .sheets_drawn
            .is_empty()
    );
    harness.get_by_label_contains("the export writes one continuous drawing");
}

/// File › Export › Drawings…: the line width and the break clearance are
/// export settings, checked before anything is written.
#[test]
fn the_export_window_takes_the_line_width_and_break_clearance() {
    let m = synthetic();
    let w = crate::drawings::DrawingsWindow {
        out: Some(std::env::temp_dir()),
        line_width: 25.0,
        break_clearance: 3.5,
        ..Default::default()
    };
    let input = crate::drawings::Input::new(&m, 0, &w).unwrap();
    assert_eq!(input.options.plan_half_width, 25.0);
    assert_eq!(input.options.break_clearance, 3.5);
    for bad in [
        crate::drawings::DrawingsWindow {
            break_clearance: 25.0,
            ..w.clone()
        },
        crate::drawings::DrawingsWindow {
            break_clearance: -1.0,
            ..w.clone()
        },
        crate::drawings::DrawingsWindow {
            line_width: 0.0,
            ..w.clone()
        },
    ] {
        assert!(crate::drawings::Input::new(&m, 0, &bad).is_err());
    }
    // The defaults: 2 mm, ±40 m.
    let d = crate::drawings::DrawingsWindow::default();
    assert_eq!((d.break_clearance, d.line_width), (2.0, 40.0));
}

/// File › Export › Drawings… with a sheet template: a template file loaded
/// is kept in the project (its images embedded) and chosen; its page
/// taken; a required field left empty refuses the export, naming it;
/// filled, the sheets carry it; another project line is drawn in its
/// role; the settings (template, fields, revisions, versions) are kept;
/// a sheet is previewed as the export draws it.
#[test]
fn drawings_take_a_sheet_template_its_fields_and_versions() {
    use crate::drawings::{DrawingSettings, DrawingsWindow, Input};
    use crate::sheet_ui::VersionChoice;
    let mut m = synthetic_project("sheet-template");
    let user = m.editing.as_ref().unwrap().user.clone();
    m.editing
        .as_mut()
        .unwrap()
        .session
        .create_line(&user, "as built", "main")
        .unwrap();
    // The example template, kept in the project.
    let file = root().join("examples/sheet-templates/example-a1.ohlsheet");
    let t = ohl_drawing::SheetTemplate::load(&file).unwrap();
    m.project_op(
        "sheet template",
        ohl_server::Op::PutLibrary {
            kind: crate::sheet_ui::KIND.into(),
            name: "example-a1.ohlsheet".into(),
            text: t.embedded(),
        },
    )
    .unwrap();
    let offers = crate::sheet_ui::offers(&m, &Default::default());
    assert!(offers.iter().any(|o| o.key == "project:example-a1" && o.ok));
    let dir = std::env::temp_dir().join(format!("ohl-app-sheet-template-{}", std::process::id()));
    let mut w = DrawingsWindow {
        route: Some(0),
        formats: vec![ohl_drawing::export::Format::Svg],
        page: "template".into(),
        all_cases: true,
        out: Some(dir.clone()),
        ..DrawingsWindow::default()
    };
    w.sheet.template = "project:example-a1".into();
    w.sheet.versions.push(VersionChoice {
        line: "as built".into(),
        role: "existing".into(),
        label: "{name}".into(),
    });
    w.sheet.role = "project".into();
    // Without the project number: refused, named.
    let input = Input::new(&m, 0, &w).unwrap();
    assert_eq!(input.export.page.as_ref().unwrap().name, "A1");
    let err = crate::drawings::run(&input, &|_, _| {}).unwrap_err();
    assert!(err.contains("Project number (project_no)"), "{err}");
    w.sheet
        .fields
        .values
        .insert("project_no".into(), "P-42".into());
    w.sheet.fields.revisions.push(ohl_drawing::Revision::parse(
        "A;2026-10-20;first issue;AB;CD",
    ));
    let input = Input::new(&m, 0, &w).unwrap();
    assert_eq!(input.versions.len(), 1);
    let report = crate::drawings::run(&input, &|_, _| {}).unwrap();
    assert!(report.contains("files written"), "{report}");
    let svg = std::fs::read_to_string(dir.join("main-sheet-01.svg")).unwrap();
    assert!(svg.contains("P-42-main-001") && svg.contains("first issue"));
    assert!(
        svg.contains("as built: existing"),
        "the version in the legend"
    );
    assert!(svg.contains("OHL_VERSIONS") || svg.contains("as built"));
    // `{height_datum}`: none given, the template's text for an empty
    // field; the project's height datum when given (2026-10-21).
    assert!(
        svg.contains("(not given). Coordinates"),
        "the [if_empty] text"
    );
    m.line.height_datum = Some(ohl_line::HeightDatum {
        name: "LN02".into(),
        epsg: Some(5728),
    });
    let input = Input::new(&m, 0, &w).unwrap();
    crate::drawings::run(&input, &|_, _| {}).unwrap();
    let svg = std::fs::read_to_string(dir.join("main-sheet-01.svg")).unwrap();
    assert!(svg.contains("Heights above LN02 (EPSG:5728). Coordinates"));
    m.line.height_datum = None;
    let _ = std::fs::remove_dir_all(&dir);
    // The lines below the lowest conductor, from the ground rules: in the
    // sheet preview over the profile too (a rule naming no case: the
    // largest sag drawn).
    w.ground_rules = std::sync::Arc::new(
        ohl_clearance::RuleSet::parse(
            "format = \"ohl-clearance-rules\"\nversion = 1\n[[rule]]\nname = \"G\"\ntarget = \"ground\"\nvertical = \"6 m\"",
        )
        .unwrap()
        .rules,
    );
    w.sheet.conductor_clearance = true;
    let p = crate::drawings::preview(&m, 0, &w).unwrap();
    assert!(!p.routes[0].curves.is_empty());
    assert!(
        p.routes[0].curves[0]
            .label
            .starts_with("6 m below the lowest conductor")
    );
    // Kept with the drawing settings.
    let text = serde_json::to_string(&DrawingSettings::of(&w, false)).unwrap();
    let back: DrawingSettings = serde_json::from_str(&text).unwrap();
    let mut again = DrawingsWindow::default();
    back.apply(&mut again);
    assert_eq!(again.sheet, w.sheet);
    assert_eq!(again.page, "template");
    // A sheet previewed in the window, as the export draws it.
    let ctx = egui::Context::default();
    let mut d = crate::drawings::Drawings::default();
    w.sheet_preview = Some(1);
    d.window = Some(w);
    // Drawn in the background: frames until it is there (exit clause: a
    // minute).
    let shown = |d: &mut crate::drawings::Drawings, m: &mut model::Model| {
        let start = std::time::Instant::now();
        loop {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| d.show(ui.ctx(), m, 0));
            out.textures_delta.clear();
            if !d.sheet_drawing() {
                break d.sheet_previewed().unwrap().unwrap();
            }
            assert!(
                start.elapsed().as_secs() < 60,
                "the sheet preview took over a minute"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    };
    let (sheets, warnings, number) = shown(&mut d, &mut m);
    assert!(sheets >= 1, "{warnings:?}");
    assert_eq!(number, 1);
    // A sheet past the route's last: the last shown (not an empty window).
    d.window.as_mut().unwrap().sheet_preview = Some(99);
    let (again, _, number) = shown(&mut d, &mut m);
    assert_eq!((again, number), (sheets, sheets));
}

/// The drawing settings are kept in the project, per project line: a
/// project without them opens with the defaults (±40 m, 2 mm, A3, the
/// preview off); changed (in the window or the profile), they are
/// written as a project setting — not into the undo log — and are there
/// when the project is opened again; another project line has its own.
#[test]
fn the_drawing_settings_are_kept_in_the_project_per_line() {
    use crate::drawings::{DrawingSettings, Drawings, setting_key};
    let model = synthetic_project("drawing-settings");
    let sources = model.sources.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // An old project: the defaults.
    let seq = {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.drawings.stored(), DrawingSettings::default());
        let d = r.drawings.settings();
        assert_eq!(
            (d.line_width, d.break_clearance, d.page.as_str()),
            (40.0, 2.0, "A3")
        );
        assert!(!r.drawings.preview_on);
        assert_eq!(
            crate::drawings::DEFAULT_LINE_WIDTH,
            ohl_drawing::Options::default().plan_half_width
        );
        let s = r.model.session().unwrap();
        assert_eq!(s.setting(&setting_key("main")).unwrap(), None);
        s.last_seq().unwrap()
    };
    // Changed: written into the project (the window closed, then the
    // line width from the profile).
    {
        let r = harness.state_mut().ready().unwrap();
        let w = &mut r.drawings.last;
        w.page = "A1".into();
        w.hscale = 1000.0;
        w.vscale = 100.0;
        w.break_clearance = 3.5;
        w.profile_only = true;
        w.formats = vec![
            ohl_drawing::export::Format::Dxf,
            ohl_drawing::export::Format::Csv,
        ];
        r.drawings.preview_on = true;
        r.drawings.set_line_width(25.0);
    }
    harness.run_steps(2);
    let kept = {
        let r = harness.state_mut().ready().unwrap();
        let s = r.model.session().unwrap();
        let text = s.setting(&setting_key("main")).unwrap().unwrap();
        // A setting, not an edit: nothing to undo.
        assert_eq!(s.last_seq().unwrap(), seq);
        serde_json::from_str::<DrawingSettings>(&text).unwrap()
    };
    assert_eq!(
        (
            kept.page.as_str(),
            kept.hscale,
            kept.vscale,
            kept.line_width,
            kept.break_clearance,
            kept.profile_only,
            kept.preview
        ),
        ("A1", 1000.0, 100.0, 25.0, 3.5, true, true)
    );
    assert_eq!(kept.formats, ["dxf", "csv"]);
    // Opened again: there.
    let again = model::load(sources.clone(), &|_| {}).unwrap();
    let d = Drawings::loaded(&again);
    assert_eq!(d.stored(), kept);
    assert_eq!(d.settings().line_width, 25.0);
    assert!(d.preview_on);
    drop(again);
    // Another project line has its own (the defaults at first); back on
    // main, main's.
    {
        let r = harness.state_mut().ready().unwrap();
        r.project_op(
            "phase",
            ohl_server::Op::CreateLine {
                name: "phase 1".into(),
                from: "main".into(),
            },
        );
        r.switch_line("phase 1");
    }
    harness.run_steps(2);
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.model.session().unwrap().current_line(), "phase 1");
        assert_eq!(r.drawings.stored(), DrawingSettings::default());
        r.drawings.set_line_width(60.0);
    }
    harness.run_steps(2);
    {
        let r = harness.state_mut().ready().unwrap();
        let s = r.model.session().unwrap();
        assert!(s.setting(&setting_key("phase 1")).unwrap().is_some());
        r.switch_line("main");
    }
    harness.run_steps(2);
    {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.drawings.stored(), kept);
        assert_eq!(r.drawings.line_width(), 25.0);
    }
    // Values out of range (a file edited by hand) take the defaults.
    let odd = DrawingSettings {
        line_width: -5.0,
        break_clearance: 50.0,
        page: "B9".into(),
        hscale: f64::NAN,
        formats: vec!["svg".into(), "doc".into()],
        ..DrawingSettings::default()
    };
    let mut w = crate::drawings::DrawingsWindow::default();
    odd.apply(&mut w);
    assert_eq!(
        (w.line_width, w.break_clearance, w.page.as_str(), w.hscale),
        (40.0, 2.0, "A3", 2000.0)
    );
    assert_eq!(w.formats, [ohl_drawing::export::Format::Svg]);
}

/// Each circuit drawn in a weather case of its own: the earth wires in
/// another case than the one shown take that case's shapes; a stringing
/// case gives the stringing band its sections; a case no name or
/// temperature matches is an error naming it.
#[test]
fn each_circuit_is_drawn_in_its_own_weather_case() {
    let m = synthetic();
    assert!(m.cases.len() >= 2 && m.cases.iter().all(|c| c.ready));
    let other = &m.cases[1];
    let earth: Vec<&str> = m
        .line
        .sections
        .iter()
        .filter(|s| s.system.is_none())
        .map(|s| s.id.as_str())
        .collect();
    assert!(!earth.is_empty());
    let mut w = crate::drawings::DrawingsWindow {
        route: Some(0),
        out: Some(std::env::temp_dir()),
        ..Default::default()
    };
    w.sheet.circuit_cases.insert(
        String::new(),
        crate::sheet_ui::CircuitCases {
            drawn: Some(other.case.name.clone()),
            stringing: Some(other.case.name.clone()),
        },
    );
    let input = crate::drawings::Input::new(&m, 0, &w).unwrap();
    let (name, shapes, _) = &input.cases[0];
    assert!(name.contains(&other.case.name), "{name}");
    for id in &earth {
        let drawn = shapes.iter().find(|s| s.section == *id).unwrap();
        let want = other.shapes.iter().find(|s| s.section == *id).unwrap();
        assert_eq!(drawn, want, "{id}");
    }
    let (case, stringing) = input.stringing.as_ref().unwrap();
    assert_eq!(case, &other.case.name);
    assert_eq!(stringing.len(), earth.len());
    // No such case: refused, named.
    w.sheet.circuit_cases.get_mut("").unwrap().drawn = Some("no such case".into());
    let e = crate::drawings::Input::new(&m, 0, &w).err().unwrap();
    assert!(e.contains("no such case"), "{e}");
}

/// A project line shown is solved from the line worked on's solutions,
/// only what differs solved again, in the background: a line just
/// started differs in nothing; one with a structure moved differs in it
/// and its sections, and its solution in each case equals a full solve.
#[test]
fn a_project_line_shown_shares_the_solutions_of_the_line_worked_on() {
    let mut m = synthetic_project("lines-shared");
    m.project_op(
        "phase",
        ohl_server::Op::CreateLine {
            name: "phase 1".into(),
            from: "main".into(),
        },
    )
    .unwrap();
    m.show_line("phase 1").unwrap();
    let o = &m.other_lines[0];
    assert_eq!(o.differs, model::Differences::default());
    assert!(o.cases.iter().all(|c| !c.ready), "solved when wanted only");
    // Moved on phase 1: it differs there.
    switch(&mut m, "phase 1");
    let id = m.line.structures[1].id.clone();
    let p = m.line.structure(&id).unwrap().position;
    m.apply(
        "shift",
        ohl_session::Command::MoveStructure {
            id: id.clone(),
            position: [p[0] + 3.0, p[1], p[2]],
        },
    )
    .unwrap();
    switch(&mut m, "main");
    m.other_lines.clear();
    m.show_line("phase 1").unwrap();
    let ctx = egui::Context::default();
    let all: Vec<usize> = (0..m.cases.len()).collect();
    for k in &all {
        m.ensure_solved(*k);
    }
    m.want_others(&all, &ctx);
    let start = std::time::Instant::now();
    while m.other_lines[0].cases.iter().any(|c| !c.ready) {
        m.poll_others();
        assert!(start.elapsed().as_secs() < 60, "not solved in a minute");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let o = &m.other_lines[0];
    assert!(o.differs.structures.contains(&id));
    let through: Vec<&String> = o
        .line
        .sections
        .iter()
        .filter(|s| s.points.iter().any(|p| p.structure == id))
        .map(|s| &s.id)
        .collect();
    assert!(!through.is_empty());
    assert!(through.iter().all(|s| o.differs.sections.contains(*s)));
    for (k, c) in o.cases.iter().enumerate() {
        let full = model::Solved::new(&o.line, &m.library, m.cases[k].case.clone());
        assert_eq!(c.shapes, full.shapes, "{}", c.case.name);
        assert_eq!(c.jumpers, full.jumpers, "{}", c.case.name);
    }
    // Drawn: only what differs, unless whole lines are asked for.
    let part = o.differs.of_line(&o.line);
    assert_eq!(part.sections.len(), o.differs.sections.len());
    assert!(part.sections.len() < o.line.sections.len());
}
