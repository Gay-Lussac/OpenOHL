// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! View › Lines on the ground beside the line, in the application: on
//! the example line, the lines in still air and in high wind (moved by
//! the outermost conductor's blowout), on the ground; the option from the
//! menu (which stays open), shown and hidden; the weather case switched.

use std::path::PathBuf;

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::app::App;
use crate::ground_lines::{self, Settings, Side};
use crate::model::{self, Sources};

fn root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// The example line with every weather case solved.
fn example() -> model::Model {
    let ex = root().join("examples/synthetic-line");
    let mut m = model::load(
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
    .unwrap();
    for k in 0..m.cases.len() {
        m.ensure_solved(k);
    }
    m
}

fn case(m: &model::Model, name: &str) -> usize {
    m.cases.iter().position(|c| c.case.name == name).unwrap()
}

/// The first span of the first alignment: its chord's ends in plan.
fn first_chord(m: &model::Model) -> ([f64; 2], [f64; 2]) {
    let view = &m.line.alignment_views()[0];
    let run: Vec<&String> = view.run().collect();
    let p = |id: &str| {
        let s = m.line.structure(id).unwrap();
        [s.position[0], s.position[1]]
    };
    (p(run[0]), p(run[1]))
}

/// The outermost conductor's offset to the left of the chord `(a, b)` at
/// its middle (left positive; the largest for the left side, the
/// smallest for the right): the wires of that span in `case` (each
/// subconductor), interpolated where they cross the perpendicular there.
fn outermost_at_middle(m: &model::Model, k: usize, (a, b): ([f64; 2], [f64; 2])) -> (f64, f64) {
    let l = (b[0] - a[0]).hypot(b[1] - a[1]);
    let u = [(b[0] - a[0]) / l, (b[1] - a[1]) / l];
    let n = [-u[1], u[0]];
    let mid = l / 2.0;
    let (mut most, mut least) = (f64::NEG_INFINITY, f64::INFINITY);
    let solved = &m.cases[k];
    for subs in solved.bundles.spans.values() {
        for pl in subs {
            for w in pl.windows(2) {
                let along = |p: [f64; 3]| (p[0] - a[0]) * u[0] + (p[1] - a[1]) * u[1];
                let off = |p: [f64; 3]| (p[0] - a[0]) * n[0] + (p[1] - a[1]) * n[1];
                let (s0, s1) = (along(w[0]), along(w[1]));
                if (s0 - mid) * (s1 - mid) > 0.0 || s0 == s1 {
                    continue;
                }
                // Only the first span's wires (between its structures).
                let f = (mid - s0) / (s1 - s0);
                let o = off(w[0]) + (off(w[1]) - off(w[0])) * f;
                if o.abs() > 60.0 {
                    continue;
                }
                most = most.max(o);
                least = least.min(o);
            }
        }
    }
    (most, least)
}

/// The line on `side` where it crosses the perpendicular at the middle
/// of the chord: its offset to the left, and its height.
fn line_at_middle(d: &ground_lines::Drawn, side: Side, (a, b): ([f64; 2], [f64; 2])) -> (f64, f64) {
    let l = (b[0] - a[0]).hypot(b[1] - a[1]);
    let u = [(b[0] - a[0]) / l, (b[1] - a[1]) / l];
    let n = [-u[1], u[0]];
    let mid = l / 2.0;
    let line = d.lines.iter().find(|x| x.side == side).unwrap();
    for run in &line.runs {
        for w in run.windows(2) {
            let along = |p: [f64; 3]| (p[0] - a[0]) * u[0] + (p[1] - a[1]) * u[1];
            let (s0, s1) = (along(w[0]), along(w[1]));
            if (s0 - mid) * (s1 - mid) <= 0.0 && s0 != s1 {
                let f = (mid - s0) / (s1 - s0);
                let p = [0, 1, 2].map(|i| w[0][i] + (w[1][i] - w[0][i]) * f);
                // On this span (not a far part of the line).
                let off = (p[0] - a[0]) * n[0] + (p[1] - a[1]) * n[1];
                if off.abs() < 100.0 {
                    return (off, p[2]);
                }
            }
        }
    }
    panic!("no line at the middle of the span on the {side:?}");
}

/// On the example line, 10 m from the outermost conductor: in still air
/// (everyday) and in high wind (600 Pa from the right), each line at the
/// middle of the first span is 10 m beyond the outermost conductor on its
/// side there — so the wind moves it by that conductor's blowout — and
/// on the ground surface (its height there).
#[test]
fn the_lines_follow_the_outermost_conductor_and_lie_on_the_ground() {
    let m = example();
    let settings = Settings {
        on: true,
        ..Settings::default()
    };
    let chord = first_chord(&m);
    let g = m.ground.as_deref().unwrap();
    let mut shift = Vec::new();
    for name in ["everyday", "high wind"] {
        let k = case(&m, name);
        let d = ground_lines::compute(&m, k, &settings);
        assert_eq!(d.ground, ground_lines::Ground::Surface);
        // Two lines per alignment.
        assert_eq!(d.lines.len(), 2 * m.line.alignment_views().len());
        let (most, least) = outermost_at_middle(&m, k, chord);
        let (left, zl) = line_at_middle(&d, Side::Left, chord);
        let (right, zr) = line_at_middle(&d, Side::Right, chord);
        assert!(
            (left - (most + 10.0)).abs() < 0.05,
            "{name}: {left} vs {most} + 10"
        );
        assert!(
            (right - (least - 10.0)).abs() < 0.05,
            "{name}: {right} vs {least} − 10"
        );
        // Draped: every point on the surface.
        for l in &d.lines {
            for run in &l.runs {
                for p in run {
                    let z = g.height(p[0], p[1]).unwrap();
                    assert!((p[2] - z).abs() < 1e-6, "{p:?} {z}");
                }
            }
        }
        assert!(zl.is_finite() && zr.is_finite());
        shift.push((left, most, right, least));
    }
    // The wind from the right blows the wires to the left: both lines
    // move left by the outermost conductors' blowout.
    let (still, windy) = (shift[0], shift[1]);
    assert!(windy.1 - still.1 > 1.0, "blown out: {still:?} {windy:?}");
    assert!(((windy.0 - still.0) - (windy.1 - still.1)).abs() < 0.1);
    assert!(((windy.2 - still.2) - (windy.3 - still.3)).abs() < 0.1);
}

/// From the axis: the lines 25 m from the alignment, on their side (the
/// chords of the round outer corners, in 5° steps, 25·(1 − cos 2.5°) =
/// 24 mm nearer at most).
#[test]
fn from_the_axis_on_the_example() {
    let m = example();
    let settings = Settings {
        on: true,
        from: ground_lines::From::Axis,
        distance: 25.0,
        ..Settings::default()
    };
    let d = ground_lines::compute(&m, 0, &settings);
    for l in &d.lines {
        let mut n = 0;
        let mut exact = 0;
        for run in &l.runs {
            for p in run {
                let near = l
                    .axis
                    .windows(2)
                    .map(|w| {
                        let e = [w[1][0] - w[0][0], w[1][1] - w[0][1]];
                        let q = [p[0] - w[0][0], p[1] - w[0][1]];
                        let t = ((q[0] * e[0] + q[1] * e[1]) / (e[0] * e[0] + e[1] * e[1]))
                            .clamp(0.0, 1.0);
                        (q[0] - t * e[0]).hypot(q[1] - t * e[1])
                    })
                    .fold(f64::INFINITY, f64::min);
                assert!(near > 25.0 - 0.025 && near < 25.0 + 1e-6, "{p:?}: {near}");
                if (near - 25.0).abs() < 1e-6 {
                    exact += 1;
                }
                // On its side (right of the direction positive).
                let (_, off) = ohl_line::route::project(&l.axis, *p);
                assert_eq!(off > 0.0, l.side == Side::Right, "{p:?}");
                n += 1;
            }
        }
        assert!(n > 10 && exact * 10 > n * 9, "{exact} of {n}");
    }
}

/// The option from View › Lines on the ground beside the line: ticked,
/// the menu stays open and the views get the lines (the 3D layer made
/// again); another weather case shown makes them for it, and back again
/// they are the ones kept; unticked, they are gone. Remembered with the
/// view settings, not in the project.
#[test]
fn the_option_shows_and_hides_and_follows_the_weather_case() {
    let model = example();
    let windy = case(&model, "high wind");
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    h.run();
    h.get_by_label("View").click();
    h.run();
    h.get_by_label_contains("Lines on the ground beside the line")
        .hover();
    h.run();
    h.get_by_label("Show the lines").click();
    h.run();
    // Still open: a tick keeps the menu.
    assert!(h.query_by_label("Show the lines").is_some());
    let r = h.state_mut().ready().unwrap();
    assert!(r.model.ground_lines.on);
    assert!(r.model.view_prefs().ground_lines.on);
    let first = r.ground_lines.drawn.clone().expect("drawn");
    assert!(!first.lines.is_empty());
    assert!(r.view3d.ground_lines.is_some() && r.plan.ground_lines.is_some());
    assert_eq!(r.ground_lines.made, 1);
    // Another case shown.
    r.case = windy;
    h.run();
    let r = h.state_mut().ready().unwrap();
    let second = r.ground_lines.drawn.clone().unwrap();
    assert!(!std::sync::Arc::ptr_eq(&first, &second));
    assert_ne!(first.lines, second.lines);
    assert!(std::sync::Arc::ptr_eq(
        r.view3d.ground_lines.as_ref().unwrap(),
        &second
    ));
    assert_eq!(r.ground_lines.made, 2);
    // Back: kept, not made again.
    r.case = 0;
    h.run();
    let r = h.state_mut().ready().unwrap();
    assert!(std::sync::Arc::ptr_eq(
        r.ground_lines.drawn.as_ref().unwrap(),
        &first
    ));
    assert_eq!(r.ground_lines.made, 2);
    // Unticked: gone.
    h.get_by_label("Show the lines").click();
    h.run();
    let r = h.state_mut().ready().unwrap();
    assert!(r.ground_lines.drawn.is_none() && r.view3d.ground_lines.is_none());
    assert!(r.plan.ground_lines.is_none() && r.profile.ground_lines.is_none());
    assert!(!r.model.ground_lines.on);
}

/// An edit of the line makes them again. Without a ground surface they
/// take the alignment's heights.
#[test]
fn an_edit_makes_them_again() {
    let mut model = crate::tests::synthetic_project("ground-lines-edit");
    model.ground_lines.on = true;
    let mut h = Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .build_eframe(|_| App::with_model(model, None));
    h.run();
    let r = h.state_mut().ready().unwrap();
    let before = r.ground_lines.drawn.clone().unwrap();
    assert_eq!(before.ground, ground_lines::Ground::Alignment);
    assert!(before.lines.iter().any(|l| !l.runs.is_empty()));
    // A structure moved 3 m sideways (perpendicular to its span).
    let (a, b) = first_chord(&r.model);
    let l = (b[0] - a[0]).hypot(b[1] - a[1]);
    let n = [-(b[1] - a[1]) / l, (b[0] - a[0]) / l];
    let id = r.model.line.alignment_views()[0]
        .run()
        .nth(1)
        .unwrap()
        .clone();
    let mut p = r.model.line.structure(&id).unwrap().position;
    p[0] += 3.0 * n[0];
    p[1] += 3.0 * n[1];
    assert!(r.perform(crate::edit::Action::Move { id, position: p }));
    for _ in 0..50 {
        h.run();
        let r = h.state_mut().ready().unwrap();
        if r.model.solving.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    h.run();
    let r = h.state_mut().ready().unwrap();
    let after = r.ground_lines.drawn.clone().unwrap();
    assert!(!std::sync::Arc::ptr_eq(&before, &after));
    assert_ne!(before.lines, after.lines);
}

/// In the profile too when ticked: the ground along the lines of both
/// sides; the 3D view, the plan and the profile drawn with them
/// (OHL_RENDER_PNG: images of the window with the 3D view, then the plan,
/// in front).
#[test]
fn the_profile_shows_both_sides_and_the_views_draw_them() {
    let gpu = ohl_render::headless_device().is_ok();
    assert!(
        gpu || ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
        "no GPU"
    );
    let mut model = example();
    model.ground_lines = Settings {
        on: true,
        distance: 15.0,
        profile: true,
        ..Settings::default()
    };
    let windy = case(&model, "high wind");
    let mut builder = Harness::builder().with_size(egui::vec2(1600.0, 1000.0));
    if gpu {
        builder = builder.wgpu();
    }
    let mut h = builder.build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    h.state_mut().ready().unwrap().case = windy;
    h.run_steps(4);
    let [left, right] = h.state_mut().ready().unwrap().profile.ground_profiles();
    assert!(left > 0 && right > 0, "{left} {right}");
    let dir = ohl_core::env::var_os("OHL_RENDER_PNG").filter(|_| gpu);
    if let Some(dir) = &dir {
        let image = h.render().unwrap();
        let path = PathBuf::from(dir).join("ground-lines-3d.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
    h.state_mut().show_tab(crate::app::Tab::Plan);
    h.run_steps(3);
    if let Some(dir) = &dir {
        let image = h.render().unwrap();
        let path = PathBuf::from(dir).join("ground-lines-plan.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
    // Not in the profile when unticked.
    h.state_mut().ready().unwrap().model.ground_lines.profile = false;
    h.run_steps(2);
    assert_eq!(
        h.state_mut().ready().unwrap().profile.ground_profiles(),
        [0, 0]
    );
}
