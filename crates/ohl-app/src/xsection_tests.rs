// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The cross-section view ([`crate::xsection`]): C opens its window, a
//! cut opens at a station or a structure (depth ±10 m by default), the
//! plane is perpendicular to the alignment (the bisector at an angle
//! point) or holds a structure's cross-arms, the wires cross it where
//! they hang, only the LiDAR points of the slab are shown (from the
//! index), nothing rotates it, it steps along the line, and its clicks
//! select in every view.
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

/// Where (offset, height) of cut `id` was drawn on the screen.
fn on_screen(h: &mut Harness<'_, App>, id: u32, p: [f64; 2]) -> egui::Pos2 {
    let r = h.state_mut().ready().unwrap();
    let v = &r.xsections.views[&id];
    let (rect, c, s, ex) = (
        v.rect.unwrap(),
        v.centre.unwrap(),
        v.scale.unwrap(),
        v.exaggeration,
    );
    egui::pos2(
        rect.center().x + ((p[0] - c[0]) / s) as f32,
        rect.center().y - ((p[1] - c[1]) * ex / s) as f32,
    )
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
/// into the span) and 310.00 m, 5.998 m and 5.901 m right of the axis
/// (the suspension string at T2 swings a little in the line's plan). In
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
        (a[0] - 5.0).abs() < 0.01 && (b[0] - 310.0).abs() < 0.01,
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
}

/// Only the LiDAR points within the slab are shown, and they come from
/// the index: of 10 000 points far away none is looked at.
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
    // In the view: dotted, none of the others.
    let mut h = harness(m);
    {
        let r = h.state_mut().ready().unwrap();
        r.open_cut(spec(At::Station(155.0)));
    }
    h.run_steps(3);
    let id = only_cut(&mut h);
    let r = h.state_mut().ready().unwrap();
    let v = &r.xsections.views[&id];
    assert_eq!(v.lidar_dots.len(), 2);
}

/// Nothing rotates the view: drags of every button, a two-finger turn,
/// the keys that turn the 3D view; the plane stays, the view only pans
/// (the axes stay horizontal and vertical: one scale across, the same
/// times the exaggeration up).
#[test]
fn no_rotation_input_has_any_effect() {
    let mut h = harness(synthetic());
    h.state_mut()
        .ready()
        .unwrap()
        .open_cut(spec(At::Structure("T2".into())));
    h.run_steps(3);
    let id = only_cut(&mut h);
    let before = {
        let r = h.state_mut().ready().unwrap();
        let v = &r.xsections.views[&id];
        (v.placed.clone().unwrap(), v.scale.unwrap(), v.rect.unwrap())
    };
    let c = before.2.center();
    for button in [
        egui::PointerButton::Primary,
        egui::PointerButton::Secondary,
        egui::PointerButton::Middle,
    ] {
        h.input_mut().events.push(egui::Event::PointerMoved(c));
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: c,
            button,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        h.run();
        for k in 1..=5 {
            h.input_mut().events.push(egui::Event::PointerMoved(
                c + egui::vec2(10.0 * k as f32, 6.0 * k as f32),
            ));
            h.run();
        }
        h.input_mut().events.push(egui::Event::PointerButton {
            pos: c + egui::vec2(50.0, 30.0),
            button,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.run();
    }
    h.input_mut().events.push(egui::Event::PointerMoved(c));
    h.input_mut().events.push(egui::Event::Rotate(0.8));
    h.run();
    for key in [
        egui::Key::Q,
        egui::Key::E,
        egui::Key::ArrowLeft,
        egui::Key::ArrowRight,
    ] {
        press(&mut h, key, egui::Modifiers::NONE);
    }
    let r = h.state_mut().ready().unwrap();
    let v = &r.xsections.views[&id];
    assert_eq!(
        v.placed.clone().unwrap(),
        before.0,
        "the plane did not turn"
    );
    assert_eq!(v.scale.unwrap(), before.1, "drags pan, never zoom or turn");
    // Panned by the drags: three drags of 50 points across and 30 down.
    let centre = v.centre.unwrap();
    assert!(centre[0].abs() > 1.0, "panned: {centre:?}");
    // Points of one height are drawn level; of one offset, plumb.
    let a = on_screen(&mut h, id, [-5.0, 520.0]);
    let b = on_screen(&mut h, id, [5.0, 520.0]);
    let up = on_screen(&mut h, id, [-5.0, 530.0]);
    assert!((a.y - b.y).abs() < 1e-3 && (a.x - up.x).abs() < 1e-3);
}

/// ▶ steps ahead by twice the depth, ◀ back, the full stop and comma
/// keys over the view the same, ⏭ goes to the next structure.
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
    h.get_by_label("▶").click();
    h.run_steps(2);
    assert!((station(&mut h) - 175.0).abs() < 1e-9);
    h.get_by_label("◀").click();
    h.run_steps(2);
    h.get_by_label("◀").click();
    h.run_steps(2);
    assert!((station(&mut h) - 135.0).abs() < 1e-9);
    // Keys, with the pointer over the view.
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
    h.get_by_label("⏮").click();
    h.run_steps(2);
    assert_eq!(
        h.state_mut().ready().unwrap().xsections.views[&id].spec.at,
        At::Structure("T2".into())
    );
}

/// A click on a wire's crossing in the cut selects its span (the other
/// views share the selection); a click on the structure selects it; a
/// structure selected elsewhere is drawn selected (white) in the cut.
/// "Cross-section here" from a structure's menu opens a cut at it.
#[test]
fn selection_syncs_with_the_other_views() {
    let mut h = harness(synthetic());
    h.state_mut().ready().unwrap().set_case(0);
    h.run();
    h.state_mut()
        .ready()
        .unwrap()
        .open_cut(spec(At::Station(155.0)));
    h.run_steps(3);
    let id = only_cut(&mut h);
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
    let at = on_screen(&mut h, id, crossing);
    click(&mut h, at, egui::PointerButton::Primary);
    assert_eq!(
        h.state_mut().ready().unwrap().selection,
        Selection::Span {
            section: "S1".into(),
            span: 0
        }
    );
    // At T2, its mast clicked.
    h.state_mut().ready().unwrap().cross_section_at("T2");
    h.run_steps(3);
    let r = h.state_mut().ready().unwrap();
    let id2 = *r
        .xsections
        .views
        .iter()
        .find(|(_, v)| v.spec.at == At::Structure("T2".into()))
        .unwrap()
        .0;
    assert_eq!(r.xsections.views[&id2].spec.depth, DEFAULT_DEPTH);
    let mast = r.xsections.views[&id2]
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
    let at = on_screen(&mut h, id2, mid);
    click(&mut h, at, egui::PointerButton::Primary);
    assert_eq!(
        h.state_mut().ready().unwrap().selection,
        Selection::Structure("T2".into())
    );
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

/// A right-click on the profile (Select held) offers "Cross-section
/// here": a cut at that station of the alignment under it.
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
    h.get_by_label_contains("Cross-section here").click();
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

/// Pictures of the window and of cuts at a structure and mid-span, with
/// LiDAR points around (`OHL_RENDER_PNG`: the folder; a GPU needed).
#[test]
fn pictures_of_the_cross_section() {
    let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG") else {
        return;
    };
    if ohl_render::headless_device().is_err() {
        return;
    }
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
    let mut h = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .wgpu()
        .build_eframe(move |cc| App::with_model(m, cc.wgpu_render_state.clone()));
    h.run();
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
    h.state_mut().ready().unwrap().selection = Selection::Structure("T3".into());
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
    h.run_steps(4);
    shot(&mut h, "cut-at-structure-T3");
    h.state_mut().ready().unwrap().selection = Selection::None;
    h.state_mut()
        .ready()
        .unwrap()
        .open_cut(spec(At::Station(157.5)));
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
    let rect = h.state_mut().ready().unwrap().xsections.views[&id]
        .rect
        .unwrap();
    h.hover_at(rect.center() + egui::vec2(60.0, -40.0));
    h.run_steps(2);
    shot(&mut h, "cut-at-mid-span");
}
