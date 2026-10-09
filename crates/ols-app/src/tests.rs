// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::path::PathBuf;

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::app::App;
use crate::model::{self, Sources};
use crate::selection::Selection;

fn root() -> PathBuf {
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

fn synthetic() -> model::Model {
    let ex = root().join("examples/synthetic-line");
    solved_all(
        model::load(
            Sources {
                file: ex.join("synthetic-line.ols.json"),
                library: Some(root().join("libraries/cables")),
                terrain: Some(ex.join("synthetic-survey.csv")),
                weather: Some(ex.join("weather-cases.olswc")),
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
/// the case list. With a GPU the 3D view renders too; set OLS_RENDER_PNG
/// to a directory to save the window as an image.
#[test]
fn the_window_shows_the_model_and_the_selection() {
    let gpu = ols_render::headless_device().is_ok();
    assert!(
        gpu || std::env::var_os("OLS_REQUIRE_GPU_TESTS").is_none(),
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
    if gpu && let Some(dir) = std::env::var_os("OLS_RENDER_PNG") {
        let image = harness.render().unwrap();
        let path = PathBuf::from(dir).join("ols-cad-window.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
    // Top bar and properties.
    assert_eq!(harness.query_all_by_label("S1 span 2").count(), 2);
    assert!(harness.query_all_by_label("glaze ice").count() >= 1);
}

/// Holding W over the 3D view flies forward; Space climbs.
#[test]
fn keys_fly_the_camera() {
    if ols_render::headless_device().is_err() {
        assert!(
            std::env::var_os("OLS_REQUIRE_GPU_TESTS").is_none(),
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
    if ols_render::headless_device().is_err() {
        assert!(
            std::env::var_os("OLS_REQUIRE_GPU_TESTS").is_none(),
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
    assert!(yaw1 < yaw0, "{yaw0} → {yaw1}");
    // Aim at T3's first insulator point and click anywhere: T3 selected.
    {
        let r = harness.state_mut().ready().unwrap();
        let p = r.model.line.structure("T3").unwrap().attachments[0].insulator_point;
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
    if ols_render::headless_device().is_err() {
        assert!(
            std::env::var_os("OLS_REQUIRE_GPU_TESTS").is_none(),
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
    if ols_render::headless_device().is_err() {
        assert!(
            std::env::var_os("OLS_REQUIRE_GPU_TESTS").is_none(),
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
    assert!((s1 / s0 - 0.3f64.exp()).abs() < 1e-9, "{s0} → {s1}");
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
    let gpu = ols_render::headless_device().is_ok();
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
    assert_eq!(
        kinds(&Snaps::default()),
        [Snap::Structure, Snap::Span, Snap::Jumper]
            .into_iter()
            .collect()
    );
    let all = Snaps(Snap::ALL.into_iter().collect());
    // The example has no LiDAR points; every other kind is there.
    let mut expected: std::collections::BTreeSet<Snap> = Snap::ALL.into_iter().collect();
    expected.remove(&Snap::LidarPoint);
    assert_eq!(kinds(&all), expected);
    assert!(candidates(&model, 0, &Snaps(Default::default())).is_empty());
    // A plan projection (1 px per m): the clamp of T2's first attachment.
    let a = model.line.structure("T2").unwrap().attachments[0].clone();
    let project = |p: [f64; 3]| Some([p[0], -p[1]]);
    let at = [a.wire_point[0], -a.wire_point[1]];
    let with = |snaps: &Snaps| pick(&candidates(&model, 0, snaps), at, 5.0, project);
    assert_eq!(with(&Snaps::default()), Selection::Structure("T2".into()));
    assert_eq!(
        with(&all),
        Selection::Attachment {
            structure: "T2".into(),
            set: a.set,
            phase: a.phase
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
        phase: 1,
    };
    harness.run();
    // Top bar and properties.
    assert_eq!(harness.query_all_by_label("T3 set 2 phase 1").count(), 2);
    assert!(
        harness
            .query_all_by_label_contains("jumper J-T3-branch")
            .next()
            .is_some()
    );
    harness.state_mut().ready().unwrap().selection = Selection::AlignmentPoint {
        route: "main".into(),
        index: 1,
    };
    harness.run();
    assert_eq!(
        harness
            .query_all_by_label("Route main, alignment point 2")
            .count(),
        1
    );
    harness.get_by_label("free (not tied to a structure)");
    // Edit › Snap lists every kind.
    harness.get_by_label("Edit").click();
    harness.run();
    harness
        .get_by_label_contains("Snap: what can be clicked")
        .click();
    harness.run();
    harness.get_by_label("Attachment points");
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
        Selection::LidarPoint(0)
    );
}

/// G opens "Go to structure"; Enter goes there (selection, 3D camera),
/// Escape closes without going.
#[test]
fn g_goes_to_a_structure() {
    let model = synthetic();
    let gpu = ols_render::headless_device().is_ok();
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
fn synthetic_project(name: &str) -> model::Model {
    synthetic_project_with(name, |_| {})
}

/// [`synthetic_project`] with its line changed first.
fn synthetic_project_with(name: &str, edit: impl FnOnce(&mut ols_line::Line)) -> model::Model {
    let ex = root().join("examples/synthetic-line");
    let dir = std::env::temp_dir().join(format!("ols-app-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ols");
    std::fs::remove_file(&path).ok();
    let mut line = ols_line::Line::load(&ex.join("synthetic-line.ols.json")).unwrap();
    edit(&mut line);
    ols_session::Session::open(&path)
        .unwrap()
        .import(&line, "tester")
        .unwrap();
    solved_all(
        model::load(
            Sources {
                file: path,
                library: Some(root().join("libraries/cables")),
                weather: Some(ex.join("weather-cases.olswc")),
                load_cases: Some(ex.join("load-cases.olslc")),
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
    use ols_session::Command;
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
        kind: ols_session::Kind::Structure,
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
            ols_session::Command::MoveStructure {
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

fn press(h: &mut Harness<'_, App>, key: egui::Key, modifiers: egui::Modifiers) {
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
    if ols_render::headless_device().is_err() {
        assert!(
            std::env::var_os("OLS_REQUIRE_GPU_TESTS").is_none(),
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
    if ols_render::headless_device().is_ok() {
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
        assert_eq!(log.len(), 1, "only the import so far");
    }
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.move_dialog.is_none());
        let log = r.model.editing.as_ref().unwrap().session.log().unwrap();
        assert_eq!(log.len(), 2, "one move command");
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
    if ols_render::headless_device().is_ok()
        && let Some(dir) = std::env::var_os("OLS_RENDER_PNG")
    {
        let image = harness.render().unwrap();
        let path = PathBuf::from(dir).join("ols-cad-inventory.png");
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

/// Choosing a slot only holds the tool; a right-click uses it.
#[test]
fn tools_act_on_a_right_click_only() {
    use crate::tools::Tool;
    if ols_render::headless_device().is_err() {
        assert!(
            std::env::var_os("OLS_REQUIRE_GPU_TESTS").is_none(),
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
        let p = r.model.line.structure("T3").unwrap().attachments[0].insulator_point;
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

fn clearance_input(m: &model::Model, rules: Vec<ols_clearance::Rule>) -> crate::clearance::Input {
    crate::clearance::Input {
        line: m.line.clone(),
        case: m.cases[0].case.name.clone(),
        shapes: m.cases[0].shapes.clone(),
        jumpers: m.cases[0].jumpers.clone(),
        rules,
        scheme: m.scheme.clone(),
        cloud: m.cloud.clone(),
        ground: m.ground.clone(),
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
        ols_clearance::RuleSet::ground(100.0).rules,
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
        ols_clearance::RuleSet::ground(0.5).rules,
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
    // The menu's (the quick bar has one too).
    harness
        .query_all_by_label("Run checks")
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
    assert!(r.clearances.files[0].ends_with("clearance-rules.olsr"));
    let rep = r.clearances.report.clone().unwrap().unwrap();
    assert_eq!(r.view3d.marks.len(), rep.findings.len());
    // Rules for other lines and across cases are noted, not run.
    assert!(
        rep.notes.iter().any(|n| n.contains("Crossed lines")),
        "{:?}",
        rep.notes
    );
    harness.get_by_label_contains("Check now");
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
            .is_some_and(|f| f.ends_with("galloping-example.olsg"))
    );
    r.galloping.case = r
        .model
        .cases
        .iter()
        .position(|c| c.case.name == "glaze ice")
        .unwrap();
    harness.run();
    harness.get_by_label("Check").click();
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

#[test]
fn vegetation_and_as_built_wires_from_a_point_cloud() {
    // A scan at 35 °C: points along every span (wire class 14) and a tree
    // (class 5) 3 m below the middle of S1's first span.
    let mut model = synthetic();
    let hot = model::Solved::new(
        &model.line,
        &model.library,
        ols_weather::WeatherCase::still("scan", ols_core::units::Temperature::from_celsius(35.0)),
    );
    let point = |p: [f64; 3], class: u8| ols_lidar::LidarPoint {
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
    let dir = std::env::temp_dir().join(format!("ols-app-{}-scan", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("scan.ols");
    std::fs::remove_file(&file).ok();
    ols_pointcloud::Store::open(&file)
        .unwrap()
        .import_points(
            "scan",
            "",
            None,
            64.0,
            0.001,
            points.iter().cloned(),
            &ols_core::budget::Budget::new(),
        )
        .unwrap();
    model.cloud = Some(format!("{}#scan", file.display()));
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    r.wirefit.open = true;
    r.vegetation.show(0);
    harness.run();
    harness.get_by_label("Fit").click();
    harness.get_by_label("Check").click();
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
            let (w, found) = &veg.wires[0];
            assert_eq!((w.section.as_str(), w.span), ("S1", 0));
            assert_eq!(found.encroaching, 1);
            break;
        }
        assert!(started.elapsed().as_secs() < 120, "no answers in 120 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
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
    r.use_tool(crate::selection::Use { target, at });
    h.run();
}

fn string_at(structure: &str, set: u32, phase: u32) -> Selection {
    Selection::Attachment {
        structure: structure.into(),
        set,
        phase,
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
    // Add string: T5's 1:1 copied in its own axes.
    let t5_string =
        crate::editors::StringForm::of(&harness.state_mut().ready().unwrap().model, "T5", 1, 1)
            .unwrap();
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
        assert_eq!((f.set, f.phase), (1, 1));
        f.top = t5_string.top;
        f.bottom = t5_string.bottom;
        f.weight = t5_string.weight;
    }
    enter(&mut harness);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        let a = r
            .model
            .line
            .attachment(&ols_line::SectionPoint {
                structure: "T8".into(),
                set: 1,
                phase: 1,
            })
            .unwrap();
        assert!((a.wire_point[2] - (at[2] + t5_string.bottom[2])).abs() < 1e-9);
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
    use_tool(&mut harness, Tool::StringWire, string_at("T5", 1, 1), None);
    use_tool(&mut harness, Tool::StringWire, string_at("T8", 1, 1), None);
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
        let ols_line::Stringing::Tension {
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
    use_tool(&mut harness, Tool::Edit, string_at("T8", 1, 1), None);
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::String(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        f.bottom[2] += 1.0;
    }
    enter(&mut harness);
    {
        let r = harness.state_mut().ready().unwrap();
        let a = r
            .model
            .line
            .attachment(&ols_line::SectionPoint {
                structure: "T8".into(),
                set: 1,
                phase: 1,
            })
            .unwrap();
        assert!((a.wire_point[2] - (at[2] + t5_string.bottom[2] + 1.0)).abs() < 1e-9);
        assert_solved_fully(&r.model);
    }
    // Delete the string: S4 goes with it, after the confirmation.
    use_tool(&mut harness, Tool::Delete, string_at("T8", 1, 1), None);
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
        assert!(r.model.line.structure("T8").unwrap().attachments.is_empty());
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
    use_tool(&mut harness, Tool::StringWire, string_at("T3", 2, 1), None);
    use_tool(&mut harness, Tool::StringWire, string_at("T7", 1, 1), None);
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
    use ols_line::Stringing;
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
    let weather = dir.join("cases.olswc");
    std::fs::write(
        &weather,
        "format = \"ols-weather-cases\"\nversion = 1\n\n[[case]]\nname = \"cold\"\ntemperature = \"-10 °C\"\n",
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
        "format = \"ols-weather-cases\"\nversion = 1\n\n[[case]]\nname = \"cold\"\ntemperature = \"-10 °C\"\n\n[[case]]\nname = \"hot\"\ntemperature = \"60 °C\"\n",
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
    let theirs = dir.join("theirs.olsw");
    std::fs::write(&theirs, LibraryKind::Cables.template("theirs")).unwrap();
    r.library_saved(LibraryKind::Cables, PageAction::UseForModel(theirs));
    assert!(r.status.as_ref().unwrap().1, "{:?}", r.status);
    assert_eq!(r.model.sources.library, before, "kept");
    let good = dir.join("good.olsw");
    std::fs::copy(root().join("libraries/cables/acsr-examples.olsw"), &good).unwrap();
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
    // Sections: condition by sag, with its value; a string renumbered with
    // a new weight; a jumper's sag; a system's voltage and phases.
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
        ols_line::Stringing::Sag {
            sag: 8.0,
            temperature: 15.0,
            span: Some(1)
        }
    );
    set_cell(&mut harness, TableKind::Strings, "Y3\t2:1", "string", "5:1");
    set_cell(
        &mut harness,
        TableKind::Strings,
        "Y3\t2:1",
        "weight N",
        "1234",
    );
    apply_table(&mut harness, TableKind::Strings, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let a = r
            .model
            .line
            .structure("Y3")
            .unwrap()
            .attachments
            .iter()
            .find(|a| (a.set, a.phase) == (5, 1))
            .unwrap();
        assert_eq!(a.insulator_weight, 1234.0);
        assert_eq!(r.model.line.sections[2].points[0].set, 5);
        assert_eq!(r.model.line.jumpers[0].from.set, 5);
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
    add(
        &mut harness,
        TableKind::Strings,
        &[
            ("structure", "T8"),
            ("string", "1:1"),
            ("top up", "30"),
            ("length m", "3"),
        ],
    );
    apply_table(&mut harness, TableKind::Strings, false).unwrap();
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

/// Alignment points: moved, tied to a structure, added between two,
/// deleted — one put of the route; the profile follows.
#[test]
fn alignment_points_are_edited_in_their_table() {
    use crate::tables::TableKind;
    let model = synthetic_project("table-alignment");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let (route, n, p0) = {
        let r = harness.state_mut().ready().unwrap();
        let rows = TableKind::Alignments.rows(&r.model, 0);
        let (route, _) = rows[0].key.split_once('\t').unwrap();
        let n = rows
            .iter()
            .filter(|x| x.key.starts_with(&format!("{route}\t")))
            .count();
        (route.to_owned(), n, rows[0].cells.clone())
    };
    assert!(n >= 2, "{n}");
    let key0 = format!("{route}\t0");
    let key1 = format!("{route}\t1");
    let e = p0[2].parse::<f64>().unwrap() + 3.0;
    set_cell(
        &mut harness,
        TableKind::Alignments,
        &key0,
        "E",
        &format!("{e}"),
    );
    set_cell(&mut harness, TableKind::Alignments, &key0, "structure", "");
    {
        let r = harness.state_mut().ready().unwrap();
        let mut row = TableKind::Alignments.new_row(&r.model, 0, &[]);
        row[0] = route.clone();
        row[1] = "1.5".into();
        row[2] = format!("{e}");
        row[3] = p0[3].clone();
        let page = r.tables.entry(TableKind::Alignments).or_default();
        page.added.push(row);
        page.deleted.insert(key1);
    }
    apply_table(&mut harness, TableKind::Alignments, false).unwrap();
    let r = harness.state_mut().ready().unwrap();
    let rows = TableKind::Alignments.rows(&r.model, 0);
    let mine: Vec<&crate::tables::Row> = rows
        .iter()
        .filter(|x| x.key.starts_with(&format!("{route}\t")))
        .collect();
    assert_eq!(mine.len(), n, "one added, one deleted");
    assert_eq!(mine[0].cells[5], "", "detached");
    assert!((mine[0].cells[2].parse::<f64>().unwrap() - e).abs() < 1e-6);
    assert!(
        (mine[1].cells[2].parse::<f64>().unwrap() - e).abs() < 1e-6,
        "the new one second"
    );
}

/// Weather cases: changed, added (between two), deleted — written to the
/// case file (comments kept), the model solved again.
#[test]
fn the_weather_case_table_writes_its_file() {
    use crate::tables::TableKind;
    let mut model = synthetic_project("weather-table");
    let path = model.sources.file.parent().unwrap().join("cases.olswc");
    std::fs::copy(
        root().join("examples/synthetic-line/weather-cases.olswc"),
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
    // Between T1 and T2, 5 m aside.
    let at = [0.5 * (t1[0] + t2[0]), 0.5 * (t1[1] + t2[1]) + 5.0, t1[2]];
    use_tool(&mut harness, Tool::AddAlignment, Selection::None, Some(at));
    let (route, index) = {
        let r = harness.state_mut().ready().unwrap();
        let Selection::AlignmentPoint { route, index } = r.selection.clone() else {
            panic!("{:?} {:?}", r.selection, r.status);
        };
        assert_eq!(index, 1, "between the first two");
        (route, index)
    };
    let points = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        r.model
            .line
            .effective_routes()
            .into_iter()
            .find(|x| x.id == route)
            .unwrap()
            .alignment
    };
    let before = points(&mut harness);
    assert_eq!(before[index].position, at);
    assert!(before.len() >= 3);
    // Moved 2 m east in its window.
    use_tool(
        &mut harness,
        Tool::Edit,
        Selection::AlignmentPoint {
            route: route.clone(),
            index,
        },
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Alignment(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        f.position[0] += 2.0;
    }
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    assert_eq!(points(&mut harness)[index].position[0], at[0] + 2.0);
    // Deleted.
    use_tool(
        &mut harness,
        Tool::Delete,
        Selection::AlignmentPoint {
            route: route.clone(),
            index,
        },
        None,
    );
    assert_eq!(points(&mut harness).len(), before.len() - 1);
}

/// Criteria › Display weather cases: two cases ticked for every section
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
    // No weather button on the quick bar any more: the Criteria menu.
    assert!(harness.query_by_label_contains("Weather: ").is_none());
    // Only in the Criteria menu.
    for menu in ["Sections", "View"] {
        harness.get_by_label(menu).click();
        harness.run();
        assert!(
            harness.query_by_label("Display weather cases…").is_none(),
            "{menu}"
        );
    }
    harness.get_by_label("Criteria").click();
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
        .attachment(&r.model.line.sections[0].points[1])
        .unwrap()
        .clone();
    let c = s1.clamps[1];
    assert!((c[0] - a.wire_point[0]).hypot(c[1] - a.wire_point[1]) > 0.3);
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
    assert_eq!(solved.case.wind_from, ols_weather::WindSide::Right);
    assert_eq!(mirror.case.wind_from, ols_weather::WindSide::Left);
    let side = |s: &crate::model::Solved| {
        let c = s.shapes[0].clamps[1];
        (c[0] - a.wire_point[0], c[1] - a.wire_point[1])
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
        ols_weather::WindSide::Left
    );
}

/// Weather case files of earlier versions: the wind side is dropped.
#[test]
fn old_weather_files_lose_the_wind_side() {
    let old = "format = \"ols-weather-cases\"\nversion = 1\n\n[[case]]\nname = \"w\"\ntemperature = \"15 °C\"\nwind_pressure = \"400 Pa\"\nwind_from = \"left\"\n";
    let new = crate::model::without_wind_side(old).unwrap();
    assert!(!new.contains("wind_from") && new.contains("wind_pressure = \"400 Pa\""));
    assert!(crate::model::without_wind_side(&new).is_none());
    ols_weather::WeatherCases::parse(&new).unwrap();
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
    let export = std::env::temp_dir().join(format!("ols-app-export-{}", std::process::id()));
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
            command: ols_session::Command::Put {
                kind: ols_session::Kind::Section,
                id,
                value,
                ord: None,
            },
        });
        assert_eq!(r.model.line.sections[0].bundle, 2, "{:?}", r.status);
    }
    // Two phases from the main line, through the menu.
    for phase in ["phase 1", "phase 2"] {
        harness.get_by_label("Lines").click();
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
    harness.get_by_label("Lines").click();
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
        let count = |l: crate::scene::Layer| {
            crate::scene::layer(&r.model, l, 0, &Selection::None)
                .counts()
                .0
        };
        let working = count(crate::scene::Layer::Structures) + count(crate::scene::Layer::Wires);
        assert_eq!(count(crate::scene::Layer::OtherLines), working);
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
            kind: ols_session::Kind::Structure,
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
            kind: ols_session::Kind::Structure,
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
            command: ols_session::Command::MoveStructure {
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
            ols_server::Op::CreateLine {
                name: "phase 1".into(),
                from: "main".into(),
            },
        );
    }
    harness.get_by_label("Lines").click();
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
        s2.circuit = s1.circuit.clone();
        s2.phase_label = s1.phase_label.clone();
        assert!(r.perform(crate::edit::Action::Command {
            what: "same phase".into(),
            command: ols_session::Command::Put {
                kind: ols_session::Kind::Section,
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
        assert_eq!(j.from.structure, "T1");
        assert_eq!(j.to.structure, "T1");
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
    use_tool(&mut harness, Tool::AddJumper, string_at("T5", 1, 1), None);
    use_tool(&mut harness, Tool::AddJumper, string_at("T5", 1, 2), None);
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.model.line.jumpers.len(), n + 2, "{:?}", r.status);
    assert!(matches!(r.selection, Selection::Jumper(_)));
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
        harness.get_by_label("Parameters").click();
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
        let ei = ols_line::jumper::estimated_bending_stiffness(
            cable,
            ols_line::jumper::DEFAULT_STIFFNESS_FACTOR,
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

/// The 3D view draws the chunked layers chunk by chunk: they load over a
/// few frames; a render distance of 0 draws none; other details rebuild.
#[test]
fn the_3d_view_draws_chunks_by_the_lod_table() {
    if ols_render::headless_device().is_err() {
        assert!(
            std::env::var_os("OLS_REQUIRE_GPU_TESTS").is_none(),
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
    let scheme = ols_lidar::ClassScheme::asprs();
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
    let dir = std::env::temp_dir().join(format!("ols-app-{}-files", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("fresh.ols");
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
    harness.get_by_label("Ground (active surface) shown");
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
    if ols_render::headless_device().is_err() {
        assert!(
            std::env::var_os("OLS_REQUIRE_GPU_TESTS").is_none(),
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
    let mut store = ols_pointcloud::Store::open(&file).unwrap();
    store
        .import_tiles(
            "survey",
            &[laz],
            "EPSG:2056",
            None,
            64.0,
            0.001,
            &ols_core::budget::Budget::new(),
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
fn grid_points(x0: f64, y0: f64, w: u32, h: u32) -> impl Iterator<Item = ols_lidar::LidarPoint> {
    (0..2 * w).flat_map(move |i| {
        (0..2 * h).map(move |j| ols_lidar::LidarPoint {
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
    if ols_render::headless_device().is_err() {
        assert!(
            std::env::var_os("OLS_REQUIRE_GPU_TESTS").is_none(),
            "no GPU"
        );
        return;
    }
    use crate::scene::Layer;
    let first = synthetic_project("budget");
    let file = first.sources.file.clone();
    drop(first);
    let mut store = ols_pointcloud::Store::open(&file).unwrap();
    store
        .import_points(
            "big",
            "",
            None,
            64.0,
            0.001,
            grid_points(-200.0, -500.0, 1000, 1000),
            &ols_core::budget::Budget::new(),
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
/// ols-app -- --ignored --nocapture point_budget_frame_times`): 25 million
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
    let mut store = ols_pointcloud::Store::open(&file).unwrap();
    store
        .import_points(
            "big",
            "",
            None,
            64.0,
            0.001,
            grid_points(-1000.0, -1250.0, 2500, 2500),
            &ols_core::budget::Budget::new(),
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
        // A fine error: the budget (10 million, or OLS_BENCH_BUDGET), not
        // the error, limits the view.
        harness.state_mut().lod.points.budget = std::env::var("OLS_BENCH_BUDGET")
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
    ols_pointcloud::copc::write_copc(&copc, &points, 0.001, &ols_core::budget::Budget::new())
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
    if ols_render::headless_device().is_err() {
        assert!(
            std::env::var_os("OLS_REQUIRE_GPU_TESTS").is_none(),
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

/// Survey data › Import LAS/LAZ and Export LAS/LAZ: tiles imported into
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
            &std::fs::read_to_string(root().join("libraries/lidar-classes/asprs-las-1.4.olsc"))
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
    let olsc = std::fs::read_to_string(out.with_extension("olsc")).unwrap();
    assert!(ols_lidar::ClassScheme::parse(&olsc).is_ok());
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
    assert!(!only.with_extension("olsc").exists());
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
    harness.get_by_label("Terrain").click();
    harness.run();
    harness.get_by_label_contains("Survey data").click();
    harness.run();
    assert!(
        harness
            .get_by_label("Export LAS/LAZ…")
            .accesskit_node()
            .is_disabled()
    );
    harness.get_by_label("Import LAS/LAZ…").click();
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
    use ols_line::condition::{Calculation, CreepStretch};
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
        assert_eq!(r.model.line.sections[0].calculation, Calculation::CreepRs);
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
format = "ols-wire"
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
        .check(std::path::Path::new("x.olsw"), text)
        .unwrap();
    let mut lib = ols_cables::Library::default();
    lib.add_toml(std::path::Path::new("x.olsw"), text).unwrap();
    let b = lib.get("bimetallic").unwrap();
    assert!(b.outer.stress_strain.is_some() && b.core.as_ref().unwrap().stress_strain.is_some());
    assert_eq!(
        lib.get("linear-shift").unwrap().model,
        ols_cables::CableModel::LinearCreepTemperature
    );
    // A linear model without its shift is refused.
    let bad = text.replace("creep_temperature_shift = 15.0\n", "");
    assert!(
        crate::library::LibraryKind::Cables
            .check(std::path::Path::new("x.olsw"), &bad)
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
    let user = std::env::temp_dir().join(format!("ols-app-plugins-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&user);
    let examples = root().join("plugins/examples");
    let geojson_built = examples
        .join("geojson/target/wasm32-wasip2/release/geojson.wasm")
        .is_file();
    assert!(
        geojson_built || std::env::var_os("OLS_REQUIRE_PLUGIN_TESTS").is_none(),
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
        "len(ols.line()['structures']), ols.case(), len(ols.results()['sections']) > 0".into(),
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
            "p = [s for s in ols.line()['structures'] if s['id'] == '{id}'][0]['position']\n\
             p[0] += 3.0\n\
             ols.apply({{'op': 'move_structure', 'id': '{id}', 'position': p}}, 'moved by a script')"
        ),
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let s = r.model.line.structures.iter().find(|s| s.id == id).unwrap();
        assert!((s.position[0] - x[0] - 3.0).abs() < 1e-9);
        assert_solved_fully(&r.model);
    }
    submit(&mut harness, "ols.undo()".into());
    {
        let r = harness.state_mut().ready().unwrap();
        let s = r.model.line.structures.iter().find(|s| s.id == id).unwrap();
        assert_eq!(s.position, x, "undone");
    }
    // Refused: raised in the script.
    let out = submit(
        &mut harness,
        "ols.apply({'op': 'move_structure', 'id': 'no-such', 'position': [0, 0, 0]})".into(),
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
        let s = ols_session::Session::open(&file).unwrap();
        s.set_user("tester", "bob", ols_session::Role::Viewer)
            .unwrap();
    }
    let server = ols_server::server::Server::start(
        &file,
        ols_server::server::Options {
            bind: "127.0.0.1:0".parse().unwrap(),
            advertise: None,
            code: None,
            newcomers: None,
            web: None,
        },
    )
    .unwrap();
    let dir = std::env::temp_dir().join(format!("ols-app-shared-{}", std::process::id()));
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
        ols_server::Op::SetUser {
            name: "bob".into(),
            role: ols_session::Role::Designer,
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
    let target = ols_server::lock_target("main", ols_session::Kind::Structure, &id);
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
    let s = ols_session::Session::open(&file).unwrap();
    assert_eq!(s.line().structure(&id).unwrap().position, p0);
    assert_eq!(s.role("bob").unwrap(), Some(ols_session::Role::Designer));
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
            .is_some_and(|p| p.ancestors().any(|a| a.ends_with("ols-cad-libraries"))),
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
            file: ex.join("synthetic-line.ols.json"),
            library: Some(root().join("libraries/cables")),
            weather: Some(ex.join("weather-cases.olswc")),
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
        let raised = ols_terrain::Terrain::from_mesh(
            g.points()
                .iter()
                .map(|p| [p[0], p[1], p[2] + 1.0])
                .collect(),
            g.triangles().to_vec(),
        )
        .unwrap();
        std::fs::write(
            &xml,
            ols_terrain::formats::write_landxml(&[("raised", &raised)]),
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
        let s = ols_session::Session::open(&file).unwrap();
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
        let s = ols_session::Session::open(&file).unwrap();
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
        built || std::env::var_os("OLS_REQUIRE_PLUGIN_TESTS").is_none(),
        "project-bundle plugin not built"
    );
    if !built {
        return;
    }
    let tmp = std::env::temp_dir().join(format!("ols-app-import-project-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let ex = root().join("examples/synthetic-line");
    let bundle = serde_json::json!({
        "line": serde_json::from_str::<serde_json::Value>(
            &std::fs::read_to_string(ex.join("synthetic-line.ols.json")).unwrap()).unwrap(),
        "libraries": [
            {"kind": "cables", "name": "acsr-examples.olsw",
             "text": std::fs::read_to_string(root().join("libraries/cables/acsr-examples.olsw")).unwrap()},
            {"kind": "weather", "name": "weather-cases.olswc",
             "text": std::fs::read_to_string(ex.join("weather-cases.olswc")).unwrap()},
        ],
    });
    let file = tmp.join("line.olsb");
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
    let into = tmp.join("imported.ols");
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
    let file = dir.join("mine.olsc");
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
    let s = ols_lidar::ClassScheme::load(&file).unwrap();
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
        assert!(notes(&m, 0, v).is_empty());
    }
    m.section_display.label[View::Plan as usize] = true;
    m.section_display.sag[View::ThreeD as usize] = true;
    let plan = notes(&m, 0, View::Plan);
    assert_eq!(plan.len(), m.line.sections.len());
    assert!(plan.iter().all(|n| n.kind == Kind::Label));
    assert!(notes(&m, 0, View::Profile).is_empty());
    let sags = notes(&m, 0, View::ThreeD);
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
    harness
        .get_by_label("Clearance-limited temperature")
        .click();
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
    assert_eq!(s2.result, Ok(ols_thermal::limit::Limit::BelowRange));
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
            file: ex.join("synthetic-line-typed.ols.json"),
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
    harness.get_by_label("Checks").click();
    harness.run();
    harness.get_by_label("Bill of materials…").click();
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
                .is_some_and(|p| p.ends_with("unit-costs.olscost"))
        );
        let e = r.bom.result.as_ref().unwrap().as_ref().unwrap();
        assert!(e.priced && e.missing.is_empty(), "{:?}", e.missing);
        assert_eq!(e.bom.structures.iter().map(|s| s.count).sum::<usize>(), 7);
    }
    assert!(
        harness
            .query_all_by_label_contains("Total: ")
            .next()
            .is_some()
    );

    harness.get_by_label("Tools").click();
    harness.run();
    harness.get_by_label("Spot structures…").click();
    harness.run();
    harness.get_by_label("Spot").click();
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
        built || std::env::var_os("OLS_REQUIRE_PLUGIN_TESTS").is_none(),
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
    let host = ols_plugins::WasmHost::new().unwrap();
    let params = std::fs::read_to_string(dir.join("example-parameters.toml")).unwrap();
    let g = ols_io::codes::generate(&host, &dir, &params).unwrap();
    let report = crate::codes::apply(&mut r, &g, "en50341").unwrap();
    assert!(
        report.contains("6 weather cases and 7 load cases"),
        "{report}"
    );
    let names: Vec<&str> = r.model.cases.iter().map(|c| c.case.name.as_str()).collect();
    assert_eq!(names[0], "EN extreme wind");
    assert_eq!(names.len(), 6);
    let lc = r.model.sources.load_cases.clone().unwrap();
    assert!(lc.ends_with("loadcases/en50341.olslc"), "{}", lc.display());
    let session = &r.model.editing.as_ref().unwrap().session;
    for (kind, name) in [("weather", "en50341.olswc"), ("loadcases", "en50341.olslc")] {
        let files = session.library_files(kind).unwrap();
        assert!(files.iter().any(|(n, _)| n == name), "{kind}: {name}");
    }
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
    let dir = std::env::temp_dir().join(format!("ols-app-drawings-{}", std::process::id()));
    let w = crate::drawings::DrawingsWindow {
        route: Some(0),
        formats: vec![
            ols_drawing::export::Format::Pdf,
            ols_drawing::export::Format::Dxf,
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
    assert_eq!(ols_drawing::pdf::read_back(&pdf).unwrap().len(), 2);
    assert!(dir.join("main-profile-model.dxf").is_file());
    let _ = std::fs::remove_dir_all(&dir);
    // Without a folder it does not start.
    let none = crate::drawings::DrawingsWindow::default();
    assert!(crate::drawings::Input::new(&m, 0, &none).is_err());
}

/// Edit › Versions: commit, branch (a project line), work on it and merge
/// as project operations; the model follows (re-solved) and the window lists them.
#[test]
fn versions_commit_branch_checkout_and_merge() {
    use ols_server::Op;
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
        ols_session::Command::MoveStructure {
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
        ols_cables::BimetallicBehavior::BirdCage
    );
    // Kept in the project: the session's line has it.
    let e = r.model.editing.as_ref().unwrap();
    assert_eq!(
        e.session.line().bimetallic.behavior,
        ols_cables::BimetallicBehavior::BirdCage
    );
    // Undone like any edit.
    r.model.undo().unwrap();
    assert!(r.model.line.bimetallic.is_default());
}

/// A transverse Mercator projection centred on the synthetic line's origin
/// (invented, for the tests).
const LOCAL_TM: &str =
    "+proj=tmerc +lat_0=46 +lon_0=8 +k=1 +x_0=0 +y_0=0 +ellps=GRS80 +units=m +no_defs";

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
    let target = file.with_file_name("moved.ols");
    std::fs::remove_file(&target).ok();
    {
        let s = ols_session::Session::open(&file).unwrap();
        s.put_surface(
            "tester",
            "ground",
            "test",
            &[[0.0, 0.0, 400.0], [500.0, 0.0, 410.0], [0.0, 500.0, 420.0]],
            &[[0, 1, 2]],
        )
        .unwrap();
    }
    let mut store = ols_pointcloud::Store::open(&file).unwrap();
    let pts = (0..50).map(|k| ols_lidar::LidarPoint {
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
            &ols_core::budget::Budget::new(),
        )
        .unwrap();
    drop(store);
    let (from, to) = (
        ols_crs::Crs::parse(LOCAL_TM).unwrap(),
        ols_crs::Crs::parse("EPSG:32632").unwrap(),
    );
    let report =
        crate::crs::reproject_file(&file, &target, &from, &to, "tester", &|_, _| {}).unwrap();
    assert!(report.contains("1 surface"), "{report}");
    let t = ols_crs::Transformation::new(&from, &to, ols_crs::Heights::Keep);
    let old = ols_session::Session::open(&file).unwrap();
    let new = ols_session::Session::open(&target).unwrap();
    assert_eq!(old.line().crs, LOCAL_TM, "the old project is not changed");
    assert_eq!(new.line().crs, "EPSG:32632");
    for (a, b) in old.line().structures.iter().zip(&new.line().structures) {
        let p = t.point(a.position).unwrap();
        assert!((p[0] - b.position[0]).hypot(p[1] - b.position[1]) < 1e-6);
        assert_eq!(p[2], b.position[2]);
    }
    let (points, _) = new.surface("ground").unwrap();
    assert!((points[0][0] - t.point([0.0, 0.0, 400.0]).unwrap()[0]).abs() < 1e-6);
    let store = ols_pointcloud::Store::open(&target).unwrap();
    let cloud = store.cloud(Some("lidar")).unwrap();
    assert_eq!((cloud.points, cloud.crs.as_str()), (50, "EPSG:32632"));
    assert!(cloud.bounds[0][0] > 400_000.0, "{:?}", cloud.bounds);
    // Not again into an existing file.
    let r = harness.state_mut().ready().unwrap();
    let err = crate::crs::reproject_check_for_tests(&r.model, "EPSG:32632", &target);
    assert!(err.unwrap_err().contains("exists already"));
}

#[test]
#[ignore = "writes images of the coordinate system windows to OLS_SHOTS (a look by eye)"]
fn coordinate_system_window_images() {
    let Ok(dir) = std::env::var("OLS_SHOTS") else {
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
        r.crs.define = Some(crate::crs::Define { picker: p });
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
    ols_background::Store::open(&file)
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
}

#[test]
#[ignore = "writes images of the backgrounds to OLS_SHOTS (a look by eye)"]
fn background_images() {
    let Ok(dir) = std::env::var("OLS_SHOTS") else {
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
    model.cloud = Some("survey.ols".into());
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
#[ignore = "writes an image of the surfaces window to OLS_SHOTS (a look by eye)"]
fn surfaces_window_image() {
    let Ok(dir) = std::env::var("OLS_SHOTS") else {
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
    model.cloud = Some("survey.ols".into());
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
    assert_eq!((row.render_distance, row.detail), (10_000.0, [0.5, 2.0]));
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
    // A 1 m line: drawn at level 0, left out at level 2 (2 m tolerance).
    let d = std::sync::Arc::make_mut(&mut model.backgrounds.drawings[0].drawing);
    d.lines.push(ols_background::dxf::Polyline {
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
#[ignore = "writes images of the Lines menu and windows to OLS_SHOTS (a look by eye)"]
fn lines_images() {
    use crate::project_lines::LinesWindow;
    let Ok(dir) = std::env::var("OLS_SHOTS") else {
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
                ols_server::Op::CreateLine {
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
            command: ols_session::Command::MoveStructure {
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
            command: ols_session::Command::MoveStructure {
                id: "T3".into(),
                position: [p[0] - 3.0, p[1], p[2]],
            },
        });
    }
    harness.run();
    harness.get_by_label("Lines").click();
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
        ols_server::Op::CreateLine {
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
    use ols_server::Op;
    let mut m = synthetic_project("tree");
    let shift = |m: &mut model::Model, id: &str, dx: f64| {
        let p = m.line.structure(id).unwrap().position;
        m.apply(
            "shift",
            ols_session::Command::MoveStructure {
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
    harness.get_by_label("Lines").click();
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
                .set_user("tester", "ben", ols_session::Role::Designer)
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
#[ignore = "writes an image of the working tree to OLS_SHOTS (a look by eye)"]
fn working_tree_image() {
    use ols_server::Op;
    let Ok(dir) = std::env::var("OLS_SHOTS") else {
        return;
    };
    let mut m = synthetic_project("tree-shot");
    let shift = |m: &mut model::Model, id: &str, dx: f64| {
        let p = m.line.structure(id).unwrap().position;
        m.apply(
            "shift",
            ols_session::Command::MoveStructure {
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
    use ols_server::Op;
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
        ols_session::Command::MoveStructure {
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
    if let Ok(dir) = std::env::var("OLS_SHOTS") {
        harness
            .render()
            .unwrap()
            .save(format!("{dir}/new-line-from-revision.png"))
            .unwrap();
    }
}

#[test]
#[ignore = "times switching project lines on the project OLS_PROJECT (a copy: it is changed)"]
fn line_switch_timing() {
    let Ok(file) = std::env::var("OLS_PROJECT") else {
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
                ols_server::Op::CreateLine {
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
        ols_server::Op::CreateLine {
            name: "phase 1".into(),
            from: "main".into(),
        },
    )
    .unwrap();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(m, None));
    harness.run();
    harness.get_by_label("Lines").click();
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
    if let Ok(dir) = std::env::var("OLS_SHOTS") {
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
                .set_user("tester", "ben", ols_session::Role::Designer)
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
#[ignore = "imports OLS_XML with the project plugins of OLS_PLUGINS through the app (local check)"]
fn local_project_import_through_the_app() {
    let (Ok(xml), Ok(plugins), Ok(out)) = (
        std::env::var("OLS_XML"),
        std::env::var("OLS_PLUGINS"),
        std::env::var("OLS_OUT"),
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
    // Then a TIN into the new project (OLS_TIN): drawn without reopening.
    let Ok(tin) = std::env::var("OLS_TIN") else {
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
    if let Ok(dir) = std::env::var("OLS_SHOTS") {
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
#[ignore = "writes an image of the Terrain surfaces window to OLS_SHOTS (a look by eye)"]
fn surfaces_sections_image() {
    let Ok(dir) = std::env::var("OLS_SHOTS") else {
        return;
    };
    let mut model = synthetic_project("surfaces-sections");
    model.points = vec![([0, 0, 0], 2), ([1000, 0, 0], 5)];
    model.class_counts = vec![(2, 40), (5, 7)];
    model.cloud = Some("survey.ols".into());
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
            command: ols_session::Command::MoveStructure {
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
    let s = ols_session::Session::open(&file).unwrap();
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
        r.model
            .line
            .routes
            .iter()
            .find(|x| x.id == "main")
            .unwrap()
            .alignment
            .clone()
    };
    let before = main(&mut harness);
    assert!(before.len() >= 3);
    let p0 = before[0].position;
    let p2 = before[2].position;
    let free = [p0[0] + 40.0, p0[1] + 60.0, p0[2]];
    let at = |route: &str, index| Selection::AlignmentPoint {
        route: route.into(),
        index,
    };
    use_tool(&mut harness, Tool::NewAlignment, at("main", 0), None);
    use_tool(
        &mut harness,
        Tool::NewAlignment,
        Selection::None,
        Some(free),
    );
    use_tool(&mut harness, Tool::NewAlignment, at("main", 2), None);
    assert_eq!(harness.state_mut().ready().unwrap().view3d.draft.len(), 3);
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    let new = {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.new_alignment.is_empty(), "{:?}", r.status);
        let n = r
            .model
            .line
            .routes
            .iter()
            .find(|x| x.id == "alignment 2")
            .unwrap_or_else(|| panic!("{:?}", r.status))
            .clone();
        assert!(n.structures.is_empty());
        n.alignment
    };
    assert_eq!(new.len(), 3);
    assert_eq!(new[0].position, p0);
    assert_eq!(new[1].position, free);
    assert_eq!(new[2].position, p2);
    // The junction moved with the Edit tool: both alignments.
    use_tool(&mut harness, Tool::Edit, at("main", 0), None);
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Alignment(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        f.structure = None;
        f.position[0] += 2.0;
    }
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        let line = &r.model.line;
        let get = |id: &str| line.routes.iter().find(|x| x.id == id).unwrap().alignment[0].position;
        assert!((get("main")[0] - (p0[0] + 2.0)).abs() < 1e-9);
        assert_eq!(get("main"), get("alignment 2"));
    }
    // Point 2 of main to alignment 2, as its second point.
    let moved = main(&mut harness)[1].position;
    use_tool(&mut harness, Tool::Edit, at("main", 1), None);
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Alignment(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        f.target = "alignment 2".into();
        f.order = 2;
    }
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        let line = &r.model.line;
        let a2 = &line
            .routes
            .iter()
            .find(|x| x.id == "alignment 2")
            .unwrap()
            .alignment;
        assert_eq!(a2.len(), 4);
        assert_eq!(a2[1].position, moved);
        assert_eq!(main(&mut harness).len(), before.len() - 1);
    }
    // An alignment without structures: its profile is drawn.
    {
        let r = harness.state_mut().ready().unwrap();
        r.profile.route = r
            .model
            .line
            .routes
            .iter()
            .position(|x| x.id == "alignment 2")
            .unwrap();
        r.profile.chosen = true;
    }
    harness.state_mut().show_tab(crate::app::Tab::Profile);
    harness.run_steps(3);
    // Escape gives a new alignment up; the tool stays held.
    use_tool(
        &mut harness,
        Tool::NewAlignment,
        Selection::None,
        Some(free),
    );
    press(&mut harness, egui::Key::Escape, egui::Modifiers::NONE);
    let r = harness.state_mut().ready().unwrap();
    assert!(r.new_alignment.is_empty() && r.view3d.draft.is_empty());
    assert_eq!(r.hotbar.tool(), Tool::NewAlignment);
}

/// A line with its own alignment only: a point given another alignment
/// name in the table makes that alignment — and the line's own becomes
/// the route "main" with all its structures.
#[test]
fn a_new_alignment_name_in_the_table_makes_the_alignment() {
    use crate::tables::TableKind;
    let model = synthetic_project_with("table-new-alignment", |line| {
        let main = line.routes.iter().find(|r| r.id == "main").unwrap().clone();
        line.alignment = main.alignment;
        line.routes.clear();
    });
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let n = {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.model.line.routes.is_empty());
        let rows = TableKind::Alignments.rows(&r.model, 0);
        assert!(rows.iter().all(|x| x.cells[0] == "main"));
        rows.len()
    };
    assert!(n >= 3);
    set_cell(
        &mut harness,
        TableKind::Alignments,
        "main\t2",
        "alignment",
        "spur",
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let mut row = TableKind::Alignments.new_row(&r.model, 0, &[]);
        let rows = TableKind::Alignments.rows(&r.model, 0);
        row[0] = "spur".into();
        row[1] = "2".into();
        row[2] = format!("{}", rows[2].cells[2].parse::<f64>().unwrap() + 50.0);
        row[3] = rows[2].cells[3].clone();
        r.tables
            .entry(TableKind::Alignments)
            .or_default()
            .added
            .push(row);
    }
    apply_table(&mut harness, TableKind::Alignments, false).unwrap();
    let r = harness.state_mut().ready().unwrap();
    let line = &r.model.line;
    assert!(line.alignment.is_empty());
    let ids: Vec<&str> = line.routes.iter().map(|x| x.id.as_str()).collect();
    assert_eq!(ids, ["main", "spur"]);
    assert_eq!(line.routes[0].structures.len(), line.structures.len());
    assert_eq!(line.routes[0].alignment.len(), n - 1);
    assert_eq!(line.routes[1].alignment.len(), 2);
    assert!(line.routes[1].structures.is_empty());
}

/// A project whose library has a crafted name ("../…": written by another
/// program, or a crafted copy): exporting the libraries refuses it, and
/// nothing is written outside the folder chosen.
#[test]
fn exported_libraries_stay_in_their_folder() {
    let model = synthetic_project("export-names");
    let file = model.sources.file.clone();
    {
        let db = rusqlite::Connection::open(&file).unwrap();
        db.execute(
            "INSERT INTO library_file (kind, name, user, text) VALUES ('cables', '../escaped.olsw', 'tester', 'x')",
            [],
        )
        .unwrap();
    }
    let dir = file.parent().unwrap().join("export");
    let err = model.export_libraries(&dir).unwrap_err();
    assert!(err.contains("not a plain file name"), "{err}");
    assert!(
        !dir.join("escaped.olsw").exists() && !dir.parent().unwrap().join("escaped.olsw").exists()
    );
}

/// In the 3D view in first person: Escape while stringing cancels the
/// wire and keeps the mouse; a second Escape frees it.
#[test]
fn escape_twice_in_first_person() {
    use crate::tools::Tool;
    if ols_render::headless_device().is_err() {
        assert!(
            std::env::var_os("OLS_REQUIRE_GPU_TESTS").is_none(),
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
    use_tool(&mut harness, Tool::StringWire, string_at("T2", 1, 1), None);
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
    use_tool(&mut harness, Tool::StringWire, string_at("T2", 1, 1), None);
    use_tool(&mut harness, Tool::StringWire, string_at("T3", 1, 1), None);
    harness.state_mut().ready().unwrap().view3d.captured = true;
    escape(&mut harness);
    cancelled(&mut harness, Tool::StringWire);
    assert!(harness.state_mut().ready().unwrap().view3d.captured);
    // The section window, a field focused: closed, String wire held.
    use_tool(&mut harness, Tool::StringWire, string_at("T2", 1, 1), None);
    use_tool(&mut harness, Tool::StringWire, string_at("T3", 1, 1), None);
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
    let at = harness.state_mut().ready().unwrap().model.line.structures[0].position;
    use_tool(&mut harness, Tool::NewAlignment, Selection::None, Some(at));
    harness.state_mut().ready().unwrap().view3d.captured = true;
    escape(&mut harness);
    cancelled(&mut harness, Tool::NewAlignment);
    assert!(harness.state_mut().ready().unwrap().view3d.captured);
}

/// Several routes, a short spur listed first: the profile opens on the
/// route with most structures; a route chosen in the list stays.
#[test]
fn the_profile_opens_on_the_main_run() {
    let mut model = synthetic_project("profile-main-run");
    let ids: Vec<String> = model.line.structures.iter().map(|s| s.id.clone()).collect();
    assert!(ids.len() >= 3);
    let route = |id: &str, s: &[String]| ols_line::Route {
        id: id.into(),
        name: id.into(),
        alignment: Vec::new(),
        structures: s.to_vec(),
    };
    model.line.routes = vec![route("spur", &ids[..2]), route("run", &ids)];
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    assert_eq!(harness.state_mut().ready().unwrap().profile.route, 1);
}

/// The plan view of a local project, drawn to a file (local checks only:
/// `OLS_PROJECT`, `OLS_SHOTS`).
#[test]
#[ignore = "local data"]
fn local_plan_image() {
    let (Ok(file), Ok(dir)) = (std::env::var("OLS_PROJECT"), std::env::var("OLS_SHOTS")) else {
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

/// Where an edit's re-solve goes, on a local project (`OLS_PROJECT`):
/// one section re-solved in a solved case, its parts timed, with the
/// bundles as they are and without their spacing.
#[test]
#[ignore = "local data"]
fn local_resolve_timing() {
    let Ok(file) = std::env::var("OLS_PROJECT") else {
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
        let shapes: Vec<_> = ols_line::solve::solve_line(&model.line, &library, &c.case)
            .into_iter()
            .filter_map(Result::ok)
            .collect();
        let t = std::time::Instant::now();
        let r = ols_line::jumper::solve_jumpers(&model.line, &library, &c.case, &shapes);
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
            ols_line::incremental::SolvedLine::solve(&line, &library, &model.cases[k].case),
        );
        let dirty = vec![line.sections[10].id.clone()];
        let t = std::time::Instant::now();
        let shapes = solved.live.shapes(&line);
        let t_shapes = t.elapsed();
        let t = std::time::Instant::now();
        let _ = ols_line::jumper::solve_jumpers(&line, &library, &solved.case, &shapes);
        let t_jumpers = t.elapsed();
        let t = std::time::Instant::now();
        let _ = ols_line::bundles::BundleLines::new(&line, &solved.shapes, &solved.jumpers);
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
#[ignore = "writes images of the wire library page scrolled right to OLS_SHOTS (a look by eye)"]
fn wire_library_scrolled_right_image() {
    let Ok(dir) = std::env::var("OLS_SHOTS") else {
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

/// The jumpers failing in a local project's first cases (`OLS_PROJECT`):
/// how each is defined, its bundle, its clamps' distance in each case.
#[test]
#[ignore = "local data"]
fn local_jumper_failures() {
    let Ok(file) = std::env::var("OLS_PROJECT") else {
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
        let shapes: Vec<_> = ols_line::solve::solve_line(line, lib, &c.case)
            .into_iter()
            .filter_map(Result::ok)
            .collect();
        for (j, r) in line
            .jumpers
            .iter()
            .zip(ols_line::jumper::solve_jumpers(line, lib, &c.case, &shapes))
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
        let ends = |p: &ols_line::SectionPoint| {
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
            j.from.structure,
            j.length,
            j.sag,
            j.temperature,
            ends(&j.from),
            ends(&j.to)
        );
        for (case, _, m) in &chords[id] {
            println!("    {case}: {m}");
        }
    }
}
