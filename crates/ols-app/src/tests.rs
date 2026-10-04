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

fn synthetic() -> model::Model {
    let ex = root().join("examples/synthetic-line");
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
    .unwrap()
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
    let ex = root().join("examples/synthetic-line");
    let dir = std::env::temp_dir().join(format!("ols-app-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.ols");
    std::fs::remove_file(&path).ok();
    let line = ols_line::Line::load(&ex.join("synthetic-line.ols.json")).unwrap();
    ols_session::Session::open(&path)
        .unwrap()
        .import(&line, "tester")
        .unwrap();
    model::load(
        Sources {
            file: path,
            library: Some(root().join("libraries/cables")),
            weather: Some(ex.join("weather-cases.olswc")),
            point_budget: 1000,
            corridor: 60.0,
            user: Some("tester".into()),
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap()
}

/// Every case equals a full solve of the current line.
fn assert_solved_fully(m: &model::Model) {
    for c in &m.cases {
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

/// Libraries › Weather cases opens its page in a floating window; a saved
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
    set_cell(&mut harness, kind, "2", "Wind from\n ", "left");
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
    assert_eq!(glaze.case.wind_from, ols_weather::WindSide::Left);
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

/// Display weather cases (quick bar "Weather: …"): two cases ticked, applied to every
/// section; the strings are drawn to their clamp in each.
#[test]
fn several_weather_cases_are_shown_for_the_whole_line() {
    let model = synthetic();
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
    harness.get_by_label_contains("Weather: ").click();
    harness.run();
    harness.get_by_label("Display weather cases");
    harness.get_by_label("high wind").click();
    harness.run();
    harness.get_by_label("Apply to all sections").click();
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
        .unwrap();
    let c = s1.clamps[1];
    assert!((c[0] - a.wire_point[0]).hypot(c[1] - a.wire_point[1]) > 0.3);
}

/// The project keeps its libraries: opened the first time, the wire
/// library and weather case file it reads are copied into it; opened again
/// without any library given, it reads its own (and the weather case file
/// used last); a library file saved goes into it; Export writes them out.
#[test]
fn the_project_keeps_its_libraries() {
    let first = synthetic_project("libraries-kept");
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

/// Lines: the working line saved as a project line, shown next to the
/// working line after a change, then made the working line again (one
/// command: ⌘Z brings the change back).
#[test]
fn project_lines_are_created_shown_and_made_the_working_line() {
    use crate::app::LinesWindow;
    let model = synthetic_project("lines");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        // A twin bundle, to see it drawn in the project line too.
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
    harness.get_by_label("Lines").click();
    harness.run();
    harness.get_by_label("Create line…").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(LinesWindow::Create(name, note)) = &mut r.lines_window else {
            panic!("{:?}", r.lines_window);
        };
        *name = "as built".into();
        *note = "before the works".into();
    }
    harness.run();
    harness.get_by_label("Create").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.lines_window.is_none(), "{:?}", r.status);
        // Shown as it is, it is drawn exactly as the working line: every
        // structure, string, subconductor, yoke and jumper.
        r.model.show_line("as built").unwrap();
        let count = |l: crate::scene::Layer| {
            crate::scene::layer(&r.model, l, 0, &Selection::None)
                .counts()
                .0
        };
        let working = count(crate::scene::Layer::Structures) + count(crate::scene::Layer::Wires);
        assert_eq!(count(crate::scene::Layer::OtherLines), working);
        r.model.other_lines.clear();
        r.perform(crate::edit::Action::Delete {
            kind: ols_session::Kind::Structure,
            id: "T7".into(),
            confirm: true,
        });
        assert!(r.model.line.structure("T7").is_none());
        // Shown next to the working line: it still has T7.
        r.model.show_line("as built").unwrap();
        let other = &r.model.other_lines[0];
        assert!(other.line.structure("T7").is_some());
        assert_eq!(other.cases.len(), r.model.cases.len());
        let drawn = crate::scene::layer(
            &r.model,
            crate::scene::Layer::OtherLines,
            0,
            &Selection::None,
        );
        assert!(drawn.counts().0 > 0);
        r.lines_window = Some(LinesWindow::Restore("as built".into()));
    }
    harness.run();
    harness.get_by_label("Make it the working line").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.model.line.structure("T7").is_some(), "{:?}", r.status);
        assert_solved_fully(&r.model);
    }
    press(&mut harness, egui::Key::Z, egui::Modifiers::COMMAND);
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.line.structure("T7").is_none(), "one undo");
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
        let ei = ols_line::jumper::estimated_bending_stiffness(cable);
        assert_eq!(row.cells[c("EI N·m²")], format!("{ei:.1}"));
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
    harness.get_by_label("TIN (ground surface)");
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

/// A survey stored in the project: its points are read by tile as they
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
        // Every tile at full detail, the survey seen from above.
        let mut row = harness.state().lod.row(Layer::Points);
        row.render_distance = 1e6;
        row.from = [1e6, 1e6];
        harness.state_mut().lod.set(Layer::Points, row).unwrap();
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
                .lod_stats(Layer::Points)
                .unwrap();
            if s.pending == 0 && s.drawn[0] > 0 {
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
        .get_by_label(crate::library::BIMETALLIC_BEHAVIORS[0].1)
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
        .get_by_label(crate::library::BIMETALLIC_BEHAVIORS[1].1)
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
            .any(|e| e.id == "geojson")
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
