// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The cross-section view ([`crate::xsection`]): C opens its window, a
//! cut opens at a station or a structure (depth ±10 m by default) in a 3D
//! panel of its own, the plane is perpendicular to the alignment (the
//! bisector at an angle point) or holds a structure's cross-arms, the
//! wires cross it where they hang, the panel starts in the front view
//! (orthographic, looking ahead), turns as the 3D view and comes back,
//! draws only what lies in the slab (the LiDAR points from the index),
//! picks and uses the tools as the 3D view, steps along the line with its
//! camera, and is outlined in the main 3D view.
//!
//! The example line (`examples/synthetic-line`): alignment `main` runs
//! east from (0, 0) to the Y pylon T3 at (640, 0), then turns 10° left to
//! (1250.58, 107.66). T1 (0, 0) and T2 (310, 0) stand across it
//! (orientation −90°: their x axis points south, to the right looking
//! east); phase L1 of section S1 hangs from point A1, 6 m south of their
//! axes (y = −6), so 6 m right of the line looking ahead.

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::app::{App, Tab};
use crate::model::{self, Sources};
use crate::selection::Selection;
use crate::tests::press;
use crate::xsection::{At, CutSpec, DEFAULT_DEPTH, Plane};

fn synthetic() -> model::Model {
    let ex = crate::tests::root().join("examples/synthetic-line");
    let mut m = model::load(
        Sources {
            file: ex.join("synthetic-line.ohl.json"),
            library: Some(crate::tests::root().join("libraries/cables")),
            terrain: Some(ex.join("synthetic-survey.csv")),
            weather: Some(ex.join("weather-cases.ohlwc")),
            point_budget: 1000,
            corridor: 60.0,
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    for k in 0..m.cases.len() {
        m.ensure_solved(k);
    }
    m
}

fn harness(model: model::Model) -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    h.run();
    h
}

/// A harness drawing with the GPU (none without one: the test is
/// skipped).
fn gpu_harness(model: model::Model) -> Option<Harness<'static, App>> {
    if let Err(e) = ohl_render::headless_device() {
        assert!(
            ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
            "no GPU: {e}"
        );
        eprintln!("skipping: {e}");
        return None;
    }
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .wgpu()
        .build_eframe(move |cc| App::with_model(model, cc.wgpu_render_state.clone()));
    h.run();
    Some(h)
}

fn spec(at: At) -> CutSpec {
    CutSpec {
        alignment: "main".into(),
        at,
        depth: DEFAULT_DEPTH,
        width: 40.0,
    }
}

fn placed(m: &model::Model, at: At) -> crate::xsection::Placed {
    let views = m.line.alignment_views();
    crate::xsection::place(&m.line, &views, &spec(at)).unwrap()
}

/// The one cut open: its number.
fn only_cut(h: &mut Harness<'_, App>) -> u32 {
    let r = h.state_mut().ready().unwrap();
    assert_eq!(r.xsections.views.len(), 1);
    *r.xsections.views.keys().next().unwrap()
}

/// Opens cut `at` in a panel floating in a window of its own (the whole
/// cut in view, beside the 3D view); its number.
fn open_floating(h: &mut Harness<'_, App>, at: At) -> u32 {
    h.state_mut().ready().unwrap().open_cut(spec(at));
    h.run_steps(3);
    let id = *h
        .state_mut()
        .ready()
        .unwrap()
        .xsections
        .views
        .keys()
        .last()
        .unwrap();
    crate::app::move_page(
        h.state_mut().dock(),
        Tab::CrossSection(id),
        crate::app::Place::Window,
    );
    h.run_steps(4);
    id
}

/// Where point `p` is drawn in cut `id`'s panel (none: outside its slab
/// or behind the eye).
fn on_screen(h: &mut Harness<'_, App>, id: u32, p: [f64; 3]) -> Option<egui::Pos2> {
    let r = h.state_mut().ready().unwrap();
    let v = &r.xsections.views[&id];
    let rect = v.view.rect.unwrap();
    let cam = v.view.camera.unwrap();
    let size = [f64::from(rect.width()), f64::from(rect.height())];
    let project = crate::view3d::clipped_projector(&cam, size, v.clip());
    project(p).map(|(q, _)| rect.min + egui::vec2(q[0] as f32, q[1] as f32))
}

/// The point of cut `id`'s plane `offset` right of the axis, `z` high.
fn in_plane(h: &mut Harness<'_, App>, id: u32, offset: f64, z: f64) -> [f64; 3] {
    let r = h.state_mut().ready().unwrap();
    let p = r.xsections.views[&id]
        .placed
        .clone()
        .unwrap()
        .plane
        .at(offset);
    [p[0], p[1], z]
}

fn click(h: &mut Harness<'_, App>, at: egui::Pos2, button: egui::PointerButton) {
    h.input_mut().events.push(egui::Event::PointerMoved(at));
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

/// A drag of `button` from `from` by `by` points, in five moves.
fn drag(h: &mut Harness<'_, App>, button: egui::PointerButton, from: egui::Pos2, by: egui::Vec2) {
    h.input_mut().events.push(egui::Event::PointerMoved(from));
    h.input_mut().events.push(egui::Event::PointerButton {
        pos: from,
        button,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.run();
    for k in 1..=5 {
        h.input_mut()
            .events
            .push(egui::Event::PointerMoved(from + by * (k as f32 / 5.0)));
        h.run();
    }
    h.input_mut().events.push(egui::Event::PointerButton {
        pos: from + by,
        button,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run();
}

/// The camera of cut `id`'s panel.
fn camera(h: &mut Harness<'_, App>, id: u32) -> ohl_render::Camera {
    h.state_mut().ready().unwrap().xsections.views[&id]
        .view
        .camera
        .unwrap()
}

/// Whether cut `id`'s panel shows its front view.
fn in_front_view(h: &mut Harness<'_, App>, id: u32) -> bool {
    let r = h.state_mut().ready().unwrap();
    let v = &r.xsections.views[&id];
    crate::xsection::is_front(&v.view.camera.unwrap(), &v.placed.clone().unwrap().plane)
}

/// C opens the window (⌘C / Ctrl+C does not), with a depth of ±10 m and
/// the line width across; Escape closes it.
#[test]
fn c_opens_the_window_with_a_depth_of_ten_metres() {
    let mut h = harness(synthetic());
    press(&mut h, egui::Key::C, egui::Modifiers::COMMAND);
    assert!(h.state_mut().ready().unwrap().xsections.dialog.is_none());
    press(&mut h, egui::Key::C, egui::Modifiers::NONE);
    let r = h.state_mut().ready().unwrap();
    let d = r.xsections.dialog.clone().expect("the window is open");
    assert_eq!(d.depth, DEFAULT_DEPTH);
    assert_eq!(d.depth, 10.0);
    assert_eq!(d.width, r.drawings.line_width());
    // The route the profile shows by default: main (most structures).
    assert_eq!(d.alignment, "main");
    assert!(!d.at_structure);
    h.get_by_label("Open cut");
    h.get_by_label("At a structure");
    press(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(h.state_mut().ready().unwrap().xsections.dialog.is_none());
    // A structure selected: the window opens at it.
    h.state_mut().ready().unwrap().selection = Selection::Structure("T6".into());
    press(&mut h, egui::Key::C, egui::Modifiers::NONE);
    let d = h
        .state_mut()
        .ready()
        .unwrap()
        .xsections
        .dialog
        .clone()
        .unwrap();
    assert!(d.at_structure);
    assert_eq!(d.structure.as_deref(), Some("T6"));
    // T6 stands on the branch, not on main.
    assert_eq!(d.alignment, "branch");
}

/// A station typed opens a cut there; a structure picked in the list
/// (found by searching) opens one at it; each its own tab beside the
/// profile; both open at once.
#[test]
fn station_and_structure_modes_open_a_cut() {
    let mut h = harness(synthetic());
    press(&mut h, egui::Key::C, egui::Modifiers::NONE);
    h.state_mut()
        .ready()
        .unwrap()
        .xsections
        .dialog
        .as_mut()
        .unwrap()
        .station = 155.0;
    h.run();
    h.get_by_label("Open cut").click();
    h.run_steps(3);
    let id = only_cut(&mut h);
    {
        let r = h.state_mut().ready().unwrap();
        assert!(r.xsections.dialog.is_none());
        let v = &r.xsections.views[&id];
        assert_eq!(v.spec.at, At::Station(155.0));
        assert_eq!(v.spec.depth, 10.0);
        assert!(v.placed.is_some(), "{:?}", v.error);
    }
    assert!(
        h.state_mut()
            .dock()
            .find_tab(&Tab::CrossSection(id))
            .is_some()
    );
    // The tab's title says where.
    assert_eq!(
        h.state_mut().ready().unwrap().xsections.title(id),
        "Cut station 0+155.0"
    );
    // At a structure: searched, picked in the list.
    press(&mut h, egui::Key::C, egui::Modifiers::NONE);
    h.get_by_label("At a structure").click();
    h.run();
    h.state_mut()
        .ready()
        .unwrap()
        .xsections
        .dialog
        .as_mut()
        .unwrap()
        .search = "T3".into();
    h.run();
    h.get_by_label("T3").click();
    h.run();
    h.get_by_label("Open cut").click();
    h.run_steps(3);
    let r = h.state_mut().ready().unwrap();
    assert_eq!(r.xsections.views.len(), 2);
    let at: Vec<At> = r
        .xsections
        .views
        .values()
        .map(|v| v.spec.at.clone())
        .collect();
    assert!(at.contains(&At::Structure("T3".into())), "{at:?}");
}

/// A structure clicked in a view while the window is open is taken.
#[test]
fn a_structure_clicked_elsewhere_is_taken_by_the_window() {
    let mut h = harness(synthetic());
    press(&mut h, egui::Key::C, egui::Modifiers::NONE);
    h.state_mut().ready().unwrap().selection = Selection::Structure("T4".into());
    h.run();
    let d = h
        .state_mut()
        .ready()
        .unwrap()
        .xsections
        .dialog
        .clone()
        .unwrap();
    assert!(d.at_structure);
    assert_eq!(d.structure.as_deref(), Some("T4"));
}

/// Mid-span (station 155 m on the straight first leg): the plane is
/// perpendicular to the alignment, looking east. At T3's station (640 m,
/// the angle point; the legs run at 0° and 10°): along the bisector, 5°.
/// At structure T3: its cross-arms (orientation −85.24°) lie in the
/// plane, so it looks along 4.76°; at T1 (−90°), east.
#[test]
fn the_orientation_at_a_station_and_at_an_angle_structure() {
    let m = synthetic();
    let angle = |p: &Plane| p.ahead[1].atan2(p.ahead[0]).to_degrees();
    let mid = placed(&m, At::Station(155.0));
    assert!(angle(&mid.plane).abs() < 1e-9);
    assert!(!mid.angle_point);
    assert!((mid.plane.origin[0] - 155.0).abs() < 1e-9 && mid.plane.origin[1].abs() < 1e-9);
    let vertex = placed(&m, At::Station(640.0));
    assert!(vertex.angle_point);
    // The second leg: atan(107.662 / 610.581) = 10.000°.
    let leg = 107.661_870_153_496_8f64
        .atan2(610.580_806_867_569)
        .to_degrees();
    assert!((leg - 10.0).abs() < 1e-3, "{leg}");
    assert!((angle(&vertex.plane) - leg / 2.0).abs() < 1e-9);
    let t3 = placed(&m, At::Structure("T3".into()));
    assert!((angle(&t3.plane) - (90.0 - 85.24)).abs() < 1e-9);
    assert!((t3.station - 640.0).abs() < 1e-6);
    // Its x axis (cross-arms) is the plane's right.
    let th = (-85.24f64).to_radians();
    let r = t3.plane.right();
    assert!((r[0] - th.cos()).abs() < 1e-12 && (r[1] - th.sin()).abs() < 1e-12);
    let t1 = placed(&m, At::Structure("T1".into()));
    assert!(angle(&t1.plane).abs() < 1e-9);
}

/// Mid-span T1–T2 in the case "everyday" (no wind): phase L1 of S1
/// hangs from A1, 6 m right of the axis. Its clamps (the span's ends)
/// lie at x = 5.00 m (T1 is a dead end: the strain string reaches 5 m
/// into the span) and about 310.0 m (the suspension string at T2 swung
/// 3 cm along the line over the sheaves, 2026-10-25), 5.998 m and
/// 5.901 m right of the axis (it swings a little in the line's plan). In
/// still air the wire lies in the vertical plane through its clamps, so
/// at mid-span (x = 157.5 m) it crosses the cut at the mean of the two
/// offsets, 5.95 m right; its height there is the chord's middle less
/// the sag (a span this level — ends 2 m apart over 305 m — sags most
/// at mid-span within millimetres). L2 (A11) crosses about 5.95 m left.
#[test]
fn wires_cross_the_plane_where_they_hang() {
    let m = synthetic();
    let case = m
        .cases
        .iter()
        .position(|c| c.case.name == "everyday")
        .unwrap();
    let shape = m.cases[case]
        .shapes
        .iter()
        .find(|s| s.section == "S1")
        .unwrap();
    let span = &shape.spans[0];
    let (a, b) = (span.points[0], *span.points.last().unwrap());
    assert!(
        (a[0] - 5.0).abs() < 0.01 && (b[0] - 310.0).abs() < 0.05,
        "{a:?} {b:?}"
    );
    let mid = 0.5 * (a[0] + b[0]);
    let p = placed(&m, At::Station(mid));
    let c = crate::xsection::contents(&m, case, &p.plane, 10.0, 40.0);
    let hand_offset = -0.5 * (a[1] + b[1]);
    assert!((hand_offset - 5.95).abs() < 0.01, "{hand_offset}");
    let hand_height = 0.5 * (a[2] + b[2]) - span.sag;
    let l1 = c
        .wires
        .iter()
        .find(|w| {
            w.select
                == Selection::Span {
                    section: "S1".into(),
                    span: 0,
                }
        })
        .expect("S1's first span in the cut");
    assert_eq!(l1.crossings.len(), 1);
    let [off, z] = l1.crossings[0];
    assert!((off - hand_offset).abs() < 0.005, "offset {off}");
    assert!(
        (z - hand_height).abs() < 0.02 + span.polyline_error,
        "height {z}, hand {hand_height}"
    );
    // The solver's own point at half the horizontal distance.
    assert!((z - span.mid_point[2]).abs() < 0.01 + span.polyline_error);
    assert!(l1.label.contains("L1"), "{}", l1.label);
    // Its curve within ±10 m.
    assert!(l1.curve[0].len() >= 2);
    let l2 = c
        .wires
        .iter()
        .find(|w| {
            w.select
                == Selection::Span {
                    section: "S2".into(),
                    span: 0,
                }
        })
        .unwrap();
    let l2_off = l2.crossings[0][0];
    assert!(l2_off < -5.85 && l2_off > -6.05, "{l2_off}");
    // No structure within ±10 m of mid-span; T1 at 0 m within ±160 m.
    assert!(c.structures.is_empty());
    let wide = crate::xsection::contents(&m, case, &p.plane, 160.0, 40.0);
    assert!(wide.structures.iter().any(|s| s.id == "T1"));
    // In the front view (1:1, orthographic) the crossing is drawn the
    // hand offset right of the axis, the hand height above the ground's
    // level, at one scale.
    let cam = crate::xsection::front_camera(&p.plane, [z - 30.0, z + 10.0], 40.0, 1.5);
    let size = [900.0, 600.0];
    let m_px = cam.metres_per_pixel(size[1]);
    let q = |o: f64, h: f64| {
        let a = p.plane.at(o);
        cam.project([a[0], a[1], h], size).unwrap().0
    };
    let (axis, wire) = (q(0.0, hand_height - 10.0), q(hand_offset, hand_height));
    assert!(((wire[0] - axis[0]) * m_px - hand_offset).abs() < 1e-6);
    assert!(((axis[1] - wire[1]) * m_px - 10.0).abs() < 1e-6);
}

/// The cut opens in its front view: orthographic, level, looking ahead
/// (at station 155 m, east: the eye west of the plane), its target on
/// the plane's axis. A point 6 m right of the axis is drawn right of it,
/// one 6 m left as far left, both at the axis's height on the screen; 10
/// m up is as many points as 10 m across (1:1); the whole width (±40 m)
/// is in view.
#[test]
fn a_cut_opens_in_its_front_view() {
    let mut h = harness(synthetic());
    h.state_mut()
        .ready()
        .unwrap()
        .open_cut(spec(At::Station(155.0)));
    h.run_steps(3);
    let id = only_cut(&mut h);
    assert!(in_front_view(&mut h, id));
    let cam = camera(&mut h, id);
    assert!(cam.ortho && cam.pitch == 0.0 && cam.exaggeration == 1.0);
    // From the target to the eye: west.
    assert!((cam.yaw.cos() + 1.0).abs() < 1e-12, "{}", cam.yaw);
    assert!((cam.target[0] - 155.0).abs() < 1e-9 && cam.target[1].abs() < 1e-9);
    let z = cam.target[2];
    let mut at = |o: f64, dz: f64| {
        let p = in_plane(&mut h, id, o, z + dz);
        on_screen(&mut h, id, p).unwrap()
    };
    let (c, r, l, up, across) = (
        at(0.0, 0.0),
        at(6.0, 0.0),
        at(-6.0, 0.0),
        at(0.0, 10.0),
        at(10.0, 0.0),
    );
    assert!(r.x > c.x && l.x < c.x);
    assert!(((r.x - c.x) - (c.x - l.x)).abs() < 1e-3);
    assert!((r.y - c.y).abs() < 1e-3 && (l.y - c.y).abs() < 1e-3);
    assert!(((c.y - up.y) - (across.x - c.x)).abs() < 1e-3);
    let (w0, w1) = (at(-40.0, 0.0), at(40.0, 0.0));
    let rect = h.state_mut().ready().unwrap().xsections.views[&id]
        .rect
        .unwrap();
    for p in [w0, w1] {
        assert!(rect.contains(p), "{p:?} in {rect:?}");
    }
}

/// The panel turns, pans and zooms as the 3D view (a drag of the button
/// that orbits there turns it here); "Back to the cut view" and Home
/// bring the front view back as it was; it can be shown in perspective.
#[test]
fn the_panel_turns_as_the_3d_view_and_comes_back() {
    let Some(mut h) = gpu_harness(synthetic()) else {
        return;
    };
    let id = open_floating(&mut h, At::Structure("T2".into()));
    assert!(in_front_view(&mut h, id));
    // Fitted again to the floating panel.
    h.get_by_label("Back to the cut view").click();
    h.run_steps(2);
    let front = camera(&mut h, id);
    let (rect, controls) = {
        let r = h.state_mut().ready().unwrap();
        (
            r.xsections.views[&id].rect.unwrap(),
            r.view3d.controls.clone(),
        )
    };
    let orbit = [
        egui::PointerButton::Primary,
        egui::PointerButton::Secondary,
        egui::PointerButton::Middle,
    ]
    .into_iter()
    .find(|b| controls.drag(*b) == crate::controls::Drag::Orbit)
    .expect("a button orbits");
    drag(&mut h, orbit, rect.center(), egui::vec2(80.0, 40.0));
    let turned = camera(&mut h, id);
    assert!(
        (turned.yaw - front.yaw).abs() > 0.1 && (turned.pitch - front.pitch).abs() > 0.1,
        "{turned:?}"
    );
    assert!(!in_front_view(&mut h, id));
    // The wheel zooms.
    h.input_mut()
        .events
        .push(egui::Event::PointerMoved(rect.center()));
    h.input_mut().events.push(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: egui::vec2(0.0, 3.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    h.run_steps(10);
    assert!(camera(&mut h, id).distance < turned.distance);
    h.get_by_label("Back to the cut view").click();
    h.run_steps(2);
    assert!(in_front_view(&mut h, id));
    assert_eq!(camera(&mut h, id), front);
    // Home, the pointer over the panel.
    drag(&mut h, orbit, rect.center(), egui::vec2(-60.0, 30.0));
    assert!(!in_front_view(&mut h, id));
    h.hover_at(rect.center());
    h.run();
    press(&mut h, egui::Key::Home, egui::Modifiers::NONE);
    h.run_steps(2);
    assert!(in_front_view(&mut h, id));
    // Perspective, and back.
    h.get_by_label("Perspective").click();
    h.run_steps(2);
    assert!(!camera(&mut h, id).ortho);
    h.get_by_label("Orthographic").click();
    h.run_steps(2);
    assert!(camera(&mut h, id).ortho);
}

/// Idle, the panel draws nothing new: frames for other reasons show the
/// image already drawn (no chunk built, no image rendered).
#[test]
fn an_idle_panel_draws_nothing_new() {
    let Some(mut h) = gpu_harness(synthetic()) else {
        return;
    };
    let id = open_floating(&mut h, At::Station(155.0));
    h.run_steps(6);
    let count = |h: &mut Harness<'_, App>| {
        let v = &h.state_mut().ready().unwrap().xsections.views[&id].view;
        (v.images_drawn, v.chunks_built)
    };
    let before = count(&mut h);
    assert!(before.0 > 0);
    h.run_steps(5);
    assert_eq!(count(&mut h), before);
}

/// Only what lies in the slab is drawn. Mid-span T1–T2 (station 155 m,
/// ±10 m): T2 stands 155 m ahead, right behind the plane in the front
/// view, yet nothing is drawn where its attachment point A1 (6 m right)
/// and its mast would be; in a cut at T2 they are. The structures' chunks
/// built are only those meeting the slab.
#[test]
fn only_what_lies_in_the_slab_is_drawn() {
    let Some(mut h) = gpu_harness(synthetic()) else {
        return;
    };
    let (a1, base) = {
        let r = h.state_mut().ready().unwrap();
        let t2 = r.model.line.structure("T2").unwrap();
        let a1 = t2.points.iter().find(|p| p.label == "A1").unwrap().position;
        (a1, t2.position)
    };
    // The cross-arm's middle (the line's axis is drawn dashed at offset 0
    // in the panel).
    let arm = [0.5 * (base[0] + a1[0]), 0.5 * (base[1] + a1[1]), a1[2]];
    let drawn_at = |h: &mut Harness<'_, App>, id: u32, p: [f64; 3]| -> bool {
        let (rect, cam) = {
            let r = h.state_mut().ready().unwrap();
            let v = &r.xsections.views[&id];
            (v.rect.unwrap(), v.view.camera.unwrap())
        };
        let size = [f64::from(rect.width()), f64::from(rect.height())];
        let (q, _) = cam.project(p, size).unwrap();
        let image = h.render().unwrap();
        let k = image.width() as f32 / 1400.0;
        let (x, y) = (rect.min.x + q[0] as f32, rect.min.y + q[1] as f32);
        let mut any = false;
        for dx in -2..=2 {
            for dy in -2..=2 {
                let px = image
                    .get_pixel(
                        ((x + dx as f32) * k).round() as u32,
                        ((y + dy as f32) * k).round() as u32,
                    )
                    .0;
                any |= [px[0], px[1], px[2]] != ohl_render::BACKGROUND;
            }
        }
        any
    };
    let mid = open_floating(&mut h, At::Station(155.0));
    h.run_steps(4);
    assert!(!drawn_at(&mut h, mid, a1), "T2's A1 is not in the slab");
    assert!(
        !drawn_at(&mut h, mid, arm),
        "T2's cross-arm is not in the slab"
    );
    let (built, all) = {
        let r = h.state_mut().ready().unwrap();
        let stats = r.xsections.views[&mid]
            .view
            .lod_stats(crate::scene::Layer::Structures)
            .unwrap();
        (stats.drawn.iter().sum::<usize>(), stats.chunks)
    };
    assert!(built < all, "{built} of {all} chunks drawn");
    // Closed; at T2 they are drawn.
    h.state_mut().ready().unwrap().xsections.views.remove(&mid);
    h.run_steps(2);
    let at_t2 = open_floating(&mut h, At::Structure("T2".into()));
    h.run_steps(4);
    assert!(drawn_at(&mut h, at_t2, a1), "T2's A1 in its own cut");
    assert!(
        drawn_at(&mut h, at_t2, arm),
        "T2's cross-arm in its own cut"
    );
}

/// A click on a wire where it crosses the plane selects its span (the
/// selection the other views show); a click on the mast of T2 in a cut
/// at T2 selects T2; a structure outside the slab is never picked.
#[test]
fn picking_in_the_panel_selects_in_every_view() {
    let Some(mut h) = gpu_harness(synthetic()) else {
        return;
    };
    h.state_mut().ready().unwrap().set_case(0);
    h.run();
    let id = open_floating(&mut h, At::Station(155.0));
    let crossing = {
        let r = h.state_mut().ready().unwrap();
        r.xsections.views[&id]
            .contents
            .wires
            .iter()
            .find(|w| {
                w.select
                    == Selection::Span {
                        section: "S1".into(),
                        span: 0,
                    }
            })
            .unwrap()
            .crossings[0]
    };
    let p = in_plane(&mut h, id, crossing[0], crossing[1]);
    let at = on_screen(&mut h, id, p).unwrap();
    click(&mut h, at, egui::PointerButton::Primary);
    let span = Selection::Span {
        section: "S1".into(),
        span: 0,
    };
    assert_eq!(h.state_mut().ready().unwrap().selection, span);
    // Where T2's mast would be (behind the plane): nothing of T2.
    let base = h
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .structure("T2")
        .unwrap()
        .position;
    let behind = {
        let r = h.state_mut().ready().unwrap();
        let v = &r.xsections.views[&id];
        let rect = v.rect.unwrap();
        let cam = v.view.camera.unwrap();
        let size = [f64::from(rect.width()), f64::from(rect.height())];
        let (q, _) = cam
            .project([base[0], base[1], base[2] + 12.0], size)
            .unwrap();
        rect.min + egui::vec2(q[0] as f32, q[1] as f32)
    };
    click(&mut h, behind, egui::PointerButton::Primary);
    assert_ne!(
        h.state_mut().ready().unwrap().selection,
        Selection::Structure("T2".into())
    );
    // At T2, its mast clicked.
    let id2 = open_floating(&mut h, At::Structure("T2".into()));
    let mast = h.state_mut().ready().unwrap().xsections.views[&id2]
        .contents
        .structures
        .iter()
        .find(|s| s.id == "T2")
        .unwrap()
        .lines[0]
        .clone();
    let mid = [
        0.5 * (mast[0][0] + mast[1][0]),
        0.5 * (mast[0][1] + mast[1][1]) - 5.0,
    ];
    let p = in_plane(&mut h, id2, mid[0], mid[1]);
    let at = on_screen(&mut h, id2, p).unwrap();
    click(&mut h, at, egui::PointerButton::Primary);
    assert_eq!(
        h.state_mut().ready().unwrap().selection,
        Selection::Structure("T2".into())
    );
}

/// The tool held works in the panel as in the 3D view: Go to, its
/// button clicked on T2's mast in a cut at T2, opens its window at T2
/// (the example is a read-only line file: the editing tools would only
/// say so); Select, clicked with the same button, selects.
#[test]
fn the_tool_held_works_in_the_panel() {
    let Some(mut h) = gpu_harness(synthetic()) else {
        return;
    };
    let id = open_floating(&mut h, At::Structure("T2".into()));
    let mast = h.state_mut().ready().unwrap().xsections.views[&id]
        .contents
        .structures
        .iter()
        .find(|s| s.id == "T2")
        .unwrap()
        .lines[0]
        .clone();
    let mid = [
        0.5 * (mast[0][0] + mast[1][0]),
        0.5 * (mast[0][1] + mast[1][1]) - 5.0,
    ];
    let p = in_plane(&mut h, id, mid[0], mid[1]);
    let at = on_screen(&mut h, id, p).unwrap();
    let tool = {
        let r = h.state_mut().ready().unwrap();
        r.hotbar.hold(crate::tools::Tool::GoTo);
        crate::controls::click_buttons(false).1
    };
    click(&mut h, at, tool);
    assert_eq!(
        h.state_mut().ready().unwrap().goto.as_deref(),
        Some("T2"),
        "Go to opened at T2"
    );
    {
        let r = h.state_mut().ready().unwrap();
        r.goto = None;
        r.selection = Selection::None;
        r.hotbar.hold(crate::tools::Tool::Select);
    }
    h.run();
    click(&mut h, at, tool);
    assert_eq!(
        h.state_mut().ready().unwrap().selection,
        Selection::Structure("T2".into())
    );
}

/// ▶ steps ahead by twice the depth, ◀ back, the full stop and comma
/// keys over the panel the same, ⏭ goes to the next structure. The
/// camera, turned, keeps its pose relative to the plane: 20 m further
/// east, as high above the ground, turned the same.
#[test]
fn the_cut_steps_along_the_line() {
    let mut h = harness(synthetic());
    h.state_mut()
        .ready()
        .unwrap()
        .open_cut(spec(At::Station(155.0)));
    h.run_steps(3);
    let id = only_cut(&mut h);
    let station = |h: &mut Harness<'_, App>| {
        h.state_mut().ready().unwrap().xsections.views[&id]
            .placed
            .clone()
            .unwrap()
            .station
    };
    // Turned: from the south-west, above, 3 m right of the axis.
    let turned = {
        let v = h
            .state_mut()
            .ready()
            .unwrap()
            .xsections
            .views
            .get_mut(&id)
            .unwrap();
        let mut c = v.view.camera.unwrap();
        c.yaw += 0.5;
        c.pitch = 0.3;
        c.target[1] -= 3.0;
        v.view.set_camera(c);
        c
    };
    h.get_by_label("▶").click();
    h.run_steps(2);
    assert!((station(&mut h) - 175.0).abs() < 1e-9);
    let moved = camera(&mut h, id);
    let dz = {
        let g = h.state_mut().ready().unwrap().model.ground.clone().unwrap();
        g.height(175.0, 0.0).unwrap() - g.height(155.0, 0.0).unwrap()
    };
    assert!((moved.target[0] - turned.target[0] - 20.0).abs() < 1e-9);
    assert!((moved.target[1] - turned.target[1]).abs() < 1e-9);
    assert!((moved.target[2] - turned.target[2] - dz).abs() < 1e-9);
    assert!((moved.yaw - turned.yaw).abs() < 1e-12 && moved.pitch == turned.pitch);
    h.get_by_label("◀").click();
    h.run_steps(2);
    h.get_by_label("◀").click();
    h.run_steps(2);
    assert!((station(&mut h) - 135.0).abs() < 1e-9);
    // Keys, with the pointer over the panel.
    let rect = h.state_mut().ready().unwrap().xsections.views[&id]
        .rect
        .unwrap();
    h.hover_at(rect.center());
    h.run();
    press(&mut h, egui::Key::Period, egui::Modifiers::NONE);
    h.run();
    assert!((station(&mut h) - 155.0).abs() < 1e-9);
    press(&mut h, egui::Key::Comma, egui::Modifiers::NONE);
    h.run();
    assert!((station(&mut h) - 135.0).abs() < 1e-9);
    // The next structure: T2 at 310 m; then T3.
    h.get_by_label("⏭").click();
    h.run_steps(2);
    assert_eq!(
        h.state_mut().ready().unwrap().xsections.views[&id].spec.at,
        At::Structure("T2".into())
    );
    assert!((station(&mut h) - 310.0).abs() < 1e-6);
    h.get_by_label("⏭").click();
    h.run_steps(2);
    assert_eq!(
        h.state_mut().ready().unwrap().xsections.views[&id].spec.at,
        At::Structure("T3".into())
    );
    // At T3 the plane turned by 4.76° (its cross-arms): the camera too.
    let at_t3 = camera(&mut h, id);
    assert!((at_t3.yaw - turned.yaw - 4.76f64.to_radians()).abs() < 1e-9);
    h.get_by_label("⏮").click();
    h.run_steps(2);
    assert_eq!(
        h.state_mut().ready().unwrap().xsections.views[&id].spec.at,
        At::Structure("T2".into())
    );
}

/// The main 3D view outlines the slab of every cut open (its box from
/// the ground to the top, ±10 m along, ±40 m across); a click on the
/// outline brings the cut's panel to the front.
#[test]
fn the_main_3d_view_outlines_the_slab() {
    let Some(mut h) = gpu_harness(synthetic()) else {
        return;
    };
    h.state_mut()
        .ready()
        .unwrap()
        .open_cut(spec(At::Station(155.0)));
    h.run_steps(3);
    let id = only_cut(&mut h);
    let outline = {
        let r = h.state_mut().ready().unwrap();
        assert_eq!(r.view3d.cut_outlines.len(), 1);
        r.view3d.cut_outlines[0].clone()
    };
    assert_eq!(outline.id, id);
    // Behind left (west, north), ahead right (east, south).
    let c = outline.corners;
    assert!((c[0][0] - 145.0).abs() < 1e-9 && (c[0][1] - 40.0).abs() < 1e-9);
    assert!((c[2][0] - 165.0).abs() < 1e-9 && (c[2][1] + 40.0).abs() < 1e-9);
    assert!(c[4][2] > c[0][2]);
    // The cut hidden behind the profile, the 3D view in front.
    h.state_mut().show_tab(Tab::Profile);
    h.state_mut().show_tab(Tab::View3d);
    h.run_steps(3);
    let hidden = h.state_mut().ready().unwrap().xsections.views[&id].drawn_frame;
    // Looking at the slab from the south-west.
    {
        let r = h.state_mut().ready().unwrap();
        let cam = r.view3d.camera.as_mut().unwrap();
        cam.target = [155.0, 0.0, 0.5 * (c[0][2] + c[4][2])];
        cam.distance = 300.0;
    }
    h.run_steps(2);
    let at = {
        let r = h.state_mut().ready().unwrap();
        let rect = r.view3d.rect.unwrap();
        let cam = r.view3d.camera.unwrap();
        let size = [f64::from(rect.width()), f64::from(rect.height())];
        let mid = [0, 1, 2].map(|k| 0.5 * (c[2][k] + c[6][k]));
        let (q, _) = cam.project(mid, size).unwrap();
        rect.min + egui::vec2(q[0] as f32, q[1] as f32)
    };
    click(&mut h, at, egui::PointerButton::Primary);
    h.run_steps(3);
    let r = h.state_mut().ready().unwrap();
    assert!(r.view3d.picked_cut.is_none());
    let shown = r.xsections.views[&id].drawn_frame;
    assert!(
        shown > hidden,
        "the cut's panel came forward: {hidden:?} {shown:?}"
    );
}

/// Only the LiDAR points within the slab are found, from the index: of
/// 10 000 points far away none is looked at; the panel draws none of
/// them (their chunks are not built).
#[test]
fn lidar_points_within_the_slab_only() {
    let mut m = synthetic();
    let mm = |v: f64| (v * 1000.0).round() as i32;
    let o = m.origin;
    let at = |x: f64, y: f64, z: f64| [mm(x - o[0]), mm(y - o[1]), mm(z - o[2])];
    // At station 155 (x = 155): inside (5 m ahead, 3 m left), beyond the
    // depth (12 m ahead), beyond the width (45 m right), inside at the
    // edge (−9.9 m behind, 39 m right).
    let mut points = vec![
        (at(160.0, 3.0, 505.0), 5u8),
        (at(167.0, 0.0, 505.0), 5),
        (at(155.0, -45.0, 505.0), 2),
        (at(145.1, -39.0, 504.0), 2),
    ];
    for k in 0..10_000 {
        points.push((
            at(
                1000.0 + f64::from(k % 100),
                500.0 + f64::from(k / 100),
                510.0,
            ),
            5,
        ));
    }
    m.points = points;
    let p = placed(&m, At::Station(155.0));
    let grid = crate::xsection::PointGrid::new(&m);
    let s = crate::xsection::slab(&m, &grid, &p.plane, 10.0, 40.0);
    let mut got: Vec<[i64; 2]> = s
        .lidar
        .iter()
        .map(|(q, _, _)| [(q[0] * 10.0).round() as i64, (q[1] * 10.0).round() as i64])
        .collect();
    got.sort_unstable();
    assert_eq!(got, vec![[-30, 5050], [390, 5040]]);
    assert!(s.tested <= 4, "looked at {} points", s.tested);
    // In the panel: the far points' chunks are not drawn.
    let Some(mut h) = gpu_harness(m) else {
        return;
    };
    let id = open_floating(&mut h, At::Station(155.0));
    h.run_steps(4);
    let r = h.state_mut().ready().unwrap();
    let drawn = r.xsections.views[&id]
        .view
        .drawn_lidar(&r.model, &r.visible);
    let far = at(1000.0, 500.0, 510.0);
    assert!(!drawn.is_empty(), "the slab's points drawn");
    assert!(
        drawn.iter().all(|(q, _)| q[0] < far[0] - 100_000),
        "no far point drawn"
    );
    assert!(drawn.iter().any(|(q, _)| *q == at(160.0, 3.0, 505.0)));
}

/// Pictures of the window, of a cut's front view and turned, and of the
/// slab outlined in the main 3D view, with LiDAR points around
/// (`OHL_RENDER_PNG`: the folder; a GPU needed).
#[test]
fn pictures_of_the_cross_section() {
    let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let mut m = synthetic();
    // Ground (class 2) and trees (class 5) around the first span.
    let ground = m.ground.clone().unwrap();
    let o = m.origin;
    let mm = |v: f64| (v * 1000.0).round() as i32;
    let mut points = Vec::new();
    for i in 0..400 {
        for j in 0..90 {
            let (x, y) = (f64::from(i) * 0.8, -45.0 + f64::from(j));
            let Some(z) = ground.height(x, y) else {
                continue;
            };
            let k = (i * 7 + j * 13) % 11;
            points.push(([mm(x - o[0]), mm(y - o[1]), mm(z - o[2])], 2u8));
            if (y > 14.0 || y < -20.0) && k < 4 {
                let top = 6.0 + f64::from((i * 3 + j * 5) % 9);
                let zt = z + top * f64::from(k + 1) / 4.0;
                points.push(([mm(x - o[0]), mm(y - o[1]), mm(zt - o[2])], 5));
            }
        }
    }
    m.points = points;
    let Some(mut h) = gpu_harness(m) else {
        return;
    };
    let shot = |h: &mut Harness<'_, App>, name: &str| {
        let image = h.render().unwrap();
        crate::write_png(
            &dir.join(format!("{name}.png")),
            image.width(),
            image.height(),
            image.as_raw(),
        )
        .unwrap();
    };
    h.state_mut().ready().unwrap().selection = Selection::Structure("T2".into());
    press(&mut h, egui::Key::C, egui::Modifiers::NONE);
    h.run_steps(3);
    shot(&mut h, "window");
    h.get_by_label("Open cut").click();
    h.run_steps(3);
    let id = only_cut(&mut h);
    crate::app::move_page(
        h.state_mut().dock(),
        Tab::CrossSection(id),
        crate::app::Place::Window,
    );
    h.run_steps(6);
    shot(&mut h, "cut-front-view");
    let rect = h.state_mut().ready().unwrap().xsections.views[&id]
        .rect
        .unwrap();
    drag(
        &mut h,
        egui::PointerButton::Primary,
        rect.center(),
        egui::vec2(120.0, 50.0),
    );
    h.run_steps(6);
    shot(&mut h, "cut-orbited");
    // The main 3D view on the slab.
    let c = h.state_mut().ready().unwrap().view3d.cut_outlines[0].corners;
    {
        let r = h.state_mut().ready().unwrap();
        r.selection = Selection::None;
        let cam = r.view3d.camera.as_mut().unwrap();
        cam.target = [310.0, 0.0, 0.5 * (c[0][2] + c[4][2])];
        cam.distance = 260.0;
    }
    crate::app::move_page(
        h.state_mut().dock(),
        Tab::CrossSection(id),
        crate::app::Place::Left,
    );
    h.state_mut().show_tab(Tab::View3d);
    h.run_steps(8);
    shot(&mut h, "main-3d-slab-outline");
}

/// Help › Controls has a Cross-section page; C is listed.
#[test]
fn the_controls_list_the_cross_section() {
    let names: Vec<&str> = crate::app::ControlsPage::ALL
        .iter()
        .map(|p| p.name())
        .collect();
    assert!(names.contains(&"Cross-section"));
}

/// A right-click on the profile offers "Cross-section here (station …)"
/// in its menu: a cut at that station of the alignment under it.
#[test]
fn the_profile_right_click_opens_a_cut_at_its_station() {
    let mut h = harness(synthetic());
    h.state_mut().show_tab(Tab::Profile);
    h.run_steps(3);
    let (rect, shown) = {
        let r = h.state_mut().ready().unwrap();
        (r.profile.rect.unwrap(), r.profile.shown.unwrap())
    };
    let at = rect.center();
    click(&mut h, at, egui::PointerButton::Secondary);
    h.run();
    h.get_by_label_contains("Cross-section here (station")
        .click();
    h.run_steps(3);
    let id = only_cut(&mut h);
    let r = h.state_mut().ready().unwrap();
    let v = &r.xsections.views[&id];
    assert_eq!(v.spec.alignment, "main");
    let [s0, s1] = shown.0;
    let expected = s0 + f64::from(at.x - rect.min.x) / f64::from(rect.width()) * (s1 - s0);
    match v.spec.at {
        At::Station(s) => assert!((s - expected).abs() < 1e-6, "{s} {expected}"),
        ref other => panic!("{other:?}"),
    }
}
