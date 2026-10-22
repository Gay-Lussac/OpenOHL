// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Box and lasso selection of LiDAR points: Alt + drag (box) and
//! Alt + Shift + drag (lasso) in the profile select the points dotted
//! inside, the Edit window changes their class as one command, undone bit
//! for bit; viewers see it read-only; 10⁶ points selected and changed in
//! seconds.

use std::time::{Duration, Instant};

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

use crate::app::App;
use crate::editors::Form;
use crate::lidar_tests::{harness, load, project_with_cloud, stored};

/// The profile shown, its dots drawn.
fn profile(h: &mut Harness<'_, App>) {
    h.state_mut().show_tab(crate::app::Tab::Profile);
    h.run();
    // Near the first structure, where the cloud is: a point a dot.
    h.state_mut()
        .ready()
        .unwrap()
        .profile
        .set_range([-25.0, 25.0]);
    h.run();
    h.run();
    assert!(
        !h.state_mut()
            .ready()
            .unwrap()
            .profile
            .lidar_dots()
            .is_empty(),
        "LiDAR points dotted in the profile"
    );
}

/// A drag of the left button along `path` with `modifiers` held.
fn drag(h: &mut Harness<'_, App>, path: &[egui::Pos2], modifiers: egui::Modifiers) {
    h.event(egui::Event::PointerMoved(path[0]));
    h.step();
    h.event(egui::Event::ModifiersChanged(modifiers));
    h.event(egui::Event::PointerButton {
        pos: path[0],
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers,
    });
    h.step();
    for p in &path[1..] {
        h.event(egui::Event::PointerMoved(*p));
        h.step();
    }
    let last = *path.last().unwrap();
    h.event(egui::Event::PointerButton {
        pos: last,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers,
    });
    h.step();
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    h.run();
}

/// The dots of the profile inside `area` (pixels from the view's top
/// left).
fn dots_in(h: &mut Harness<'_, App>, area: &crate::area::Area) -> Vec<crate::model::LidarPoint> {
    let r = h.state_mut().ready().unwrap();
    let o = r.profile.rect.unwrap().min;
    r.profile
        .lidar_dots()
        .iter()
        .filter(|(q, _)| area.contains([f64::from(q.x - o.x), f64::from(q.y - o.y)]))
        .map(|d| d.1)
        .collect()
}

/// The points in the Edit window (as drawn, sorted).
fn in_window(h: &mut Harness<'_, App>) -> Vec<crate::model::LidarPoint> {
    let r = h.state_mut().ready().unwrap();
    let Some(Form::Points(f)) = &r.editor else {
        panic!("{:?} {:?}", r.editor, r.status);
    };
    let o = r.model.origin;
    let mut v: Vec<_> = f
        .points
        .iter()
        .map(|p| {
            (
                [0, 1, 2].map(|k| ((p.xyz[k] - o[k]) * 1000.0).round() as i32),
                p.stored.class,
            )
        })
        .collect();
    v.sort_unstable();
    v
}

fn sorted(mut v: Vec<crate::model::LidarPoint>) -> Vec<crate::model::LidarPoint> {
    v.sort_unstable();
    v
}

#[test]
fn box_and_lasso_select_the_points_drawn_and_change_their_class_in_one_step() {
    let (file, _) = project_with_cloud("area", 30);
    let before = stored(&file);
    let mut h = harness(load(&file), false);
    h.run();
    profile(&mut h);
    let (rect, dots) = {
        let r = h.state_mut().ready().unwrap();
        (r.profile.rect.unwrap(), r.profile.lidar_dots().to_vec())
    };
    // A box around the dots (a margin of 6 px).
    let (mut lo, mut hi) = (dots[0].0, dots[0].0);
    for (q, _) in &dots {
        lo = lo.min(*q);
        hi = hi.max(*q);
    }
    let inner = rect.shrink(1.0);
    let (a, b) = (
        inner.clamp(lo - egui::vec2(6.0, 6.0)),
        inner.clamp(hi + egui::vec2(6.0, 6.0)),
    );
    let local = |p: egui::Pos2| [f64::from(p.x - rect.min.x), f64::from(p.y - rect.min.y)];
    let area = crate::area::Area::Box(local(a), local(b));
    let expected = sorted(dots_in(&mut h, &area));
    assert!(
        expected.len() * 2 > dots.len(),
        "{} of {}",
        expected.len(),
        dots.len()
    );
    let camera_range = h.state_mut().ready().unwrap().profile.rect;
    drag(
        &mut h,
        &[a, a + (b - a) * 0.3, a + (b - a) * 0.7, b],
        egui::Modifiers::ALT,
    );
    // The view did not move; the window lists exactly the dots inside.
    assert_eq!(h.state_mut().ready().unwrap().profile.rect, camera_range);
    assert_eq!(in_window(&mut h), expected);
    h.get_by_label_contains("points selected — new class");
    h.get_by_label_contains("change the points of class:");
    // Class 5 left out (filter), the others to 6, applied: one command.
    // The class filter: the first point's class left out — the points of
    // other classes only (none: nothing to apply).
    let c0 = expected[0].1;
    let mut others = expected.iter().filter(|p| p.1 != c0).count();
    let entries = |h: &mut Harness<'_, App>| {
        h.state_mut()
            .ready()
            .unwrap()
            .model
            .session()
            .unwrap()
            .log()
            .unwrap()
            .len()
    };
    let n0 = entries(&mut h);
    {
        let r = h.state_mut().ready().unwrap();
        let Some(Form::Points(f)) = &mut r.editor else {
            panic!()
        };
        f.skip.insert(c0);
        for p in &mut f.points {
            p.class = 6;
        }
        if others == 0 {
            assert_eq!(f.command().unwrap_err(), "nothing changed");
        }
    }
    h.run();
    h.get_by_label_contains(&format!("{others} points change class"));
    if others == 0 {
        let r = h.state_mut().ready().unwrap();
        let Some(Form::Points(f)) = &mut r.editor else {
            panic!()
        };
        f.skip.clear();
        others = expected.len();
    }
    h.run();
    h.get_by_label_contains(&format!("{others} points change class"));
    assert!(!h.get_by_label("Apply").accesskit_node().is_disabled());
    crate::tests::press(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
    h.run();
    assert_eq!(entries(&mut h), n0 + 1, "one command");
    let count = |h: &mut Harness<'_, App>, c: u8| {
        let r = h.state_mut().ready().unwrap();
        r.model
            .class_counts
            .iter()
            .find(|n| n.0 == c)
            .map_or(0, |n| n.1)
    };
    assert_eq!(count(&mut h, 6) as usize, others);
    assert_ne!(stored(&file), before);
    // One undo step: every stored byte back.
    h.state_mut()
        .ready()
        .unwrap()
        .perform(crate::edit::Action::Undo);
    h.run();
    assert_eq!(stored(&file), before);
    assert_eq!(count(&mut h, 6), 0);

    // A lasso: a triangle over the left half of the dots.
    h.state_mut().ready().unwrap().editor = None;
    h.run();
    let mid = egui::pos2(0.5 * (a.x + b.x), b.y);
    let tri = [a, egui::pos2(mid.x, a.y), mid, a + egui::vec2(0.0, 1.0)];
    let path: Vec<egui::Pos2> = (0..=30)
        .map(|k| {
            let t = f32::from(k as u8) / 30.0 * 3.0;
            let (i, f) = (t.floor() as usize % 3, t.fract());
            let (p, q) = (tri[i], tri[(i + 1) % 3]);
            p + (q - p) * f
        })
        .collect();
    let lasso = crate::area::Area::Lasso(path.iter().map(|p| local(*p)).collect());
    let expected = sorted(dots_in(&mut h, &lasso));
    drag(&mut h, &path, egui::Modifiers::ALT | egui::Modifiers::SHIFT);
    if expected.is_empty() {
        assert!(h.state_mut().ready().unwrap().editor.is_none());
    } else {
        assert_eq!(in_window(&mut h), expected);
    }
    assert!(expected.len() < dots.len(), "a part only");
    let _ = &dots;

    // Without Alt a drag moves the view and selects nothing.
    h.state_mut().ready().unwrap().editor = None;
    drag(&mut h, &[a, b], egui::Modifiers::NONE);
    assert!(h.state_mut().ready().unwrap().editor.is_none());
    std::fs::remove_dir_all(file.parent().unwrap()).ok();
}

/// A viewer's box: the window, read-only (Apply greyed).
#[test]
fn a_viewer_sees_the_points_of_a_box_read_only() {
    let (file, _) = project_with_cloud("area-viewer", 12);
    let before = stored(&file);
    let mut model = load(&file);
    model.editing = Err("tester is a viewer of this project".into());
    let mut h = harness(model, false);
    h.run();
    profile(&mut h);
    let rect = h.state_mut().ready().unwrap().profile.rect.unwrap();
    drag(
        &mut h,
        &[
            rect.min + egui::vec2(2.0, 2.0),
            rect.max - egui::vec2(2.0, 2.0),
        ],
        egui::Modifiers::ALT,
    );
    assert!(!in_window(&mut h).is_empty());
    h.get_by_label_contains("Read-only: tester is a viewer");
    assert!(h.get_by_label("Apply").accesskit_node().is_disabled());
    assert_eq!(stored(&file), before);
    std::fs::remove_dir_all(file.parent().unwrap()).ok();
}

/// 10⁶ points (10⁵ in a debug build) of a 1 km square seen from above,
/// all selected by a box (as drawn), found as stored, given another class
/// by one command and undone bit for bit; each step timed.
#[test]
fn a_million_points_are_selected_and_changed_in_seconds() {
    let n: u32 = if cfg!(debug_assertions) { 316 } else { 1000 };
    let (file, o) = project_with_cloud("area-million", n);
    let before = stored(&file);
    let mut model = load(&file);
    let total = (n * n) as usize;
    assert!(
        model.points.len() >= total * 9 / 10,
        "{}",
        model.points.len()
    );
    let camera = ohl_render::Camera {
        target: [o[0], o[1], o[2]],
        distance: 4.0 * f64::from(n),
        yaw: 0.0,
        pitch: 89.9f64.to_radians(),
        fov_y: 45f64.to_radians(),
        exaggeration: 1.0,
        ortho: false,
    };
    let size = [1400.0, 900.0];
    let project = camera.projector(size);
    let area = crate::area::Area::Box([0.0, 0.0], size);
    let t = Instant::now();
    let (picked, inside) = crate::selection::lidar_in_area(
        &model,
        &crate::selection::LidarShown::Loaded,
        &area,
        &project,
        None,
        ohl_session::points::MAX_POINTS + 1,
    );
    let select = t.elapsed();
    assert_eq!(inside, model.points.len(), "every point drawn");
    let t = Instant::now();
    let mut f = crate::lidar_edit::PointsForm::of(&model, &picked).unwrap();
    let find = t.elapsed();
    assert_eq!(f.points.len(), picked.len());
    for p in &mut f.points {
        p.class = 7;
    }
    let (what, command) = f.command().unwrap();
    let t = Instant::now();
    model.apply(&what, command).unwrap();
    let apply = t.elapsed();
    let t = Instant::now();
    model.undo().unwrap();
    let undo = t.elapsed();
    println!(
        "{} points: selected in {select:.1?}, found as stored in {find:.1?}, class changed \
         (one command) in {apply:.1?}, undone in {undo:.1?}",
        picked.len()
    );
    assert_eq!(stored(&file), before, "undo gives every byte back");
    let limit = if cfg!(debug_assertions) {
        Duration::from_secs(120)
    } else {
        Duration::from_secs(30)
    };
    assert!(select + find + apply + undo < limit);
    drop(model);
    std::fs::remove_dir_all(file.parent().unwrap()).ok();
}
