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
    harness.get_by_label("Weather cases");
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
    // The Snap tab (behind Model) lists every kind.
    harness.state_mut().show_tab(crate::app::Tab::Snap);
    harness.run();
    harness.get_by_label("Attachment points");
    harness.get_by_label("LiDAR points");
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
