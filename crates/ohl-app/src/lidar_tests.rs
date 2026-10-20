// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! LiDAR points: every point drawn can be clicked (and no other), fast;
//! the Edit tool's window changes a point's coordinates and class as one
//! undoable command, the views and the ground follow; viewers see it
//! read-only.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use ohl_render::Camera;

use crate::app::App;
use crate::model::{self, LidarPoint, Model, Sources};
use crate::scene::Layer;
use crate::selection::{LidarGroup, LidarPoints, LidarShown, Selection, Snap, Snaps};

fn lidar_only() -> Snaps {
    Snaps([Snap::LidarPoint].into_iter().collect())
}

/// A camera looking straight down at `target` from `distance` m.
fn above(target: [f64; 3], distance: f64) -> Camera {
    Camera {
        target,
        distance,
        yaw: 0.3,
        pitch: 89f64.to_radians(),
        fov_y: 45f64.to_radians(),
        exaggeration: 1.0,
    }
}

/// Columns of points: a grid of `n` × `n` points `step` m apart from
/// `(x0, y0)` m off the model's origin, `z` m above it, every 3rd class 5.
fn grid(n: i32, step: f64, x0: f64, y0: f64, z: f64) -> (Vec<[i32; 3]>, Vec<u8>) {
    let mut xyz = Vec::new();
    let mut class = Vec::new();
    for i in 0..n {
        for j in 0..n {
            let mm = |v: f64| (v * 1000.0).round() as i32;
            xyz.push([
                mm(x0 + f64::from(i) * step),
                mm(y0 + f64::from(j) * step),
                mm(z + 0.01 * f64::from((i * 7 + j * 3) % 5)),
            ]);
            class.push(if (i + j) % 3 == 0 { 5 } else { 2 });
        }
    }
    (xyz, class)
}

/// A group's points, classes and the bounds of its runs of
/// [`crate::selection::LIDAR_CELL`] points (as a node read keeps them).
type Part = (Vec<[i32; 3]>, Vec<u8>, Vec<[[f64; 3]; 2]>);

/// Bounds of points (project coordinates).
fn bounds_of(model: &Model, xyz: &[[i32; 3]]) -> [[f64; 3]; 2] {
    let at: Vec<[f64; 3]> = xyz
        .iter()
        .map(|q| crate::selection::lidar_xyz(model, q))
        .collect();
    [
        [0, 1, 2].map(|k| at.iter().map(|p| p[k]).fold(f64::INFINITY, f64::min)),
        [0, 1, 2].map(|k| at.iter().map(|p| p[k]).fold(f64::NEG_INFINITY, f64::max)),
    ]
}

/// The grids as drawn groups: with the bounds of their runs (`cells`) or
/// without (each looked at whole within its bounds).
fn parts(model: &Model, grids: Vec<(Vec<[i32; 3]>, Vec<u8>)>, cells: bool) -> Vec<Part> {
    grids
        .into_iter()
        .map(|(xyz, class)| {
            let runs = if cells {
                xyz.chunks(crate::selection::LIDAR_CELL)
                    .map(|run| bounds_of(model, run))
                    .collect()
            } else {
                Vec::new()
            };
            (xyz, class, runs)
        })
        .collect()
}

/// The points drawn, in groups of a grid each (bounds from their points).
fn groups<'a>(model: &Model, parts: &'a [Part]) -> LidarShown<'a> {
    LidarShown::Groups(
        parts
            .iter()
            .map(|(xyz, class, cells)| LidarGroup {
                bounds: Some(bounds_of(model, xyz)),
                points: LidarPoints::Columns(xyz, class),
                cells,
            })
            .collect(),
    )
}

/// Every point of three groups drawn is hit by a click at its projected
/// position; a point loaded but not drawn (thinned out) is not, nor one of
/// a group not drawn.
#[test]
fn every_point_drawn_is_picked_and_no_other() {
    let mut model = crate::bench::synthetic();
    let parts = parts(
        &model,
        vec![
            grid(20, 2.0, -20.0, -20.0, 5.0),
            grid(20, 2.0, 20.0, -20.0, 6.0),
            grid(20, 2.0, -20.0, 20.0, 7.0),
        ],
        false,
    );
    // Loaded but not drawn: between the grid's points.
    let thinned: LidarPoint = ([1000, 1000, 5000], 2);
    model.points = vec![thinned];
    let size = [1200.0, 900.0];
    let cam = above(
        [
            model.origin[0] + 10.0,
            model.origin[1] + 10.0,
            model.origin[2],
        ],
        90.0,
    );
    let project = cam.projector(size);
    let shown = groups(&model, &parts);
    let snaps = lidar_only();
    let mut hits = 0;
    for (xyz, class, _) in &parts {
        for (q, c) in xyz.iter().zip(class) {
            let (at, _) = project(crate::selection::lidar_xyz(&model, q)).unwrap();
            let (picked, _) =
                crate::selection::pick_shown(&model, 0, &snaps, at, 14.0, &project, &shown);
            assert_eq!(picked, Selection::LidarPoint((*q, *c)), "at {at:?}");
            hits += 1;
        }
    }
    assert_eq!(hits, 1200);
    let (at, _) = project(crate::selection::lidar_xyz(&model, &thinned.0)).unwrap();
    let (picked, _) = crate::selection::pick_shown(&model, 0, &snaps, at, 10.0, &project, &shown);
    assert_ne!(
        picked,
        Selection::LidarPoint(thinned),
        "not drawn: not picked"
    );
    // Nothing drawn (the plan view, the layer hidden): nothing picked.
    let (picked, _) =
        crate::selection::pick_shown(&model, 0, &snaps, at, 10.0, &project, &LidarShown::Nothing);
    assert_eq!(picked, Selection::None);
}

/// Two points on one pixel: the one nearer the eye.
#[test]
fn of_points_on_one_pixel_the_nearer_is_picked() {
    let model = crate::bench::synthetic();
    let size = [1200.0, 900.0];
    let cam = above(model.origin, 50.0);
    let project = cam.projector(size);
    let low: LidarPoint = ([0, 0, 0], 2);
    let (at, _) = project(crate::selection::lidar_xyz(&model, &low.0)).unwrap();
    // 10 m nearer the eye along the ray through it.
    let (_, d) = cam.ray(at, size).unwrap();
    let high: LidarPoint = ([0, 1, 2].map(|k| (-10_000.0 * d[k]).round() as i32), 5);
    let parts = vec![(vec![low.0, high.0], vec![low.1, high.1], Vec::new())];
    let shown = groups(&model, &parts);
    let (on, _) = project(crate::selection::lidar_xyz(&model, &high.0)).unwrap();
    assert!((on[0] - at[0]).hypot(on[1] - at[1]) < 0.5, "{on:?} {at:?}");
    let (picked, _) =
        crate::selection::pick_shown(&model, 0, &lidar_only(), at, 14.0, &project, &shown);
    assert_eq!(picked, Selection::LidarPoint(high));
}

/// A click among 10 million points drawn (100 nodes of 10⁵, twice the
/// default point budget): the nodes away from the click are skipped by
/// their bounds. Under 50 ms in release (`cargo test --release -p
/// ohl-app click_among_ten_million -- --nocapture`); debug builds are
/// slower and only bounded loosely.
#[test]
fn a_click_among_ten_million_points_is_quick() {
    let model = crate::bench::synthetic();
    // 10 × 10 nodes, each 100 m square of 316 × 316 points (~0.32 m).
    let mut grids = Vec::new();
    for a in 0..10 {
        for b in 0..10 {
            grids.push(grid(
                316,
                0.316,
                -500.0 + 100.0 * f64::from(a),
                -500.0 + 100.0 * f64::from(b),
                5.0,
            ));
        }
    }
    let parts = parts(&model, grids, true);
    let total: usize = parts.iter().map(|p| p.0.len()).sum();
    assert!(total >= 9_900_000, "{total}");
    let shown = groups(&model, &parts);
    let size = [1400.0, 900.0];
    let snaps = lidar_only();
    let mut slowest = Duration::ZERO;
    for (k, cam) in [
        above(model.origin, 150.0),
        above(model.origin, 1500.0),
        Camera {
            yaw: 0.7,
            pitch: 15f64.to_radians(),
            ..above(model.origin, 400.0)
        },
    ]
    .iter()
    .enumerate()
    {
        let project = cam.projector(size);
        let at = [700.0, 450.0];
        let t = Instant::now();
        let (picked, _) =
            crate::selection::pick_shown(&model, 0, &snaps, at, 14.0, &project, &shown);
        let took = t.elapsed();
        slowest = slowest.max(took);
        println!("view {k}: click among {total} points drawn: {took:.1?}, {picked:?}");
        assert!(
            matches!(picked, Selection::LidarPoint(_)),
            "a point under the centre"
        );
    }
    let limit = if cfg!(debug_assertions) {
        Duration::from_secs(5)
    } else {
        Duration::from_millis(50)
    };
    assert!(slowest < limit, "slowest click {slowest:.1?}");
}

/// A project of the synthetic line with an invented cloud "c" around the
/// first structure: a grid of `n` × `n` points 1 m apart, 2 m above it,
/// classes 2 (ground) and every 3rd 5.
fn project_with_cloud(name: &str, n: u32) -> (PathBuf, [f64; 3]) {
    let first = crate::tests::synthetic_project(name);
    let file = first.sources.file.clone();
    let o = first.origin;
    drop(first);
    let half = f64::from(n) / 2.0;
    let points = (0..n).flat_map(move |i| {
        (0..n).map(move |j| ohl_lidar::LidarPoint {
            x: (o[0] - half + f64::from(i)).round(),
            y: (o[1] - half + f64::from(j)).round(),
            z: (o[2] + 2.0 + 0.1 * f64::from((i + 2 * j) % 4)).round(),
            class: if (i + j) % 3 == 0 { 5 } else { 2 },
            intensity: (i * n + j) as u16,
            return_number: 1,
            number_of_returns: 1,
        })
    });
    ohl_pointcloud::Store::open(&file)
        .unwrap()
        .import_points(
            "c",
            "",
            None,
            64.0,
            0.001,
            points,
            &ohl_core::budget::Budget::new(),
        )
        .unwrap();
    (file, o)
}

fn load(file: &Path) -> Model {
    let root = crate::tests::root();
    let ex = root.join("examples/synthetic-line");
    let mut m = model::load(
        Sources {
            file: file.to_path_buf(),
            library: Some(root.join("libraries/cables")),
            weather: Some(ex.join("weather-cases.ohlwc")),
            point_budget: 1_000_000,
            corridor: 60.0,
            user: Some("tester".into()),
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    for k in 0..m.cases.len() {
        m.ensure_solved(k);
    }
    m.points_percent = 100.0;
    m
}

/// The cloud's blocks and levels as stored, by what they are.
fn stored(file: &Path) -> Vec<String> {
    let conn = rusqlite::Connection::open(file).unwrap();
    conn.prepare(
        "SELECT cx || ',' || cy || ',' || class || ',' || count || ',' || z0 || ',' ||
                hex(xyz) || ',' || COALESCE(hex(intensity), '-') || ',' ||
                COALESCE(hex(returns), '-') || ',' ||
                COALESCE((SELECT group_concat(level || ':' || count || ':' || z1 || ':' ||
                hex(xyz), ';') FROM pointcloud_lod WHERE block = b.id), '')
         FROM pointcloud_block b ORDER BY cx, cy, class",
    )
    .unwrap()
    .query_map([], |r| r.get(0))
    .unwrap()
    .collect::<Result<Vec<String>, _>>()
    .unwrap()
}

/// The stored point at `at` (m), if any.
fn stored_at(file: &Path, at: [f64; 3]) -> Option<ohl_pointcloud::edit::StoredPoint> {
    ohl_pointcloud::Store::open(file)
        .unwrap()
        .find_point("c", at, None, 0.0005)
        .unwrap()
        .map(|p| p.0)
}

fn harness(model: Model, gpu: bool) -> Harness<'static, App> {
    let mut builder = Harness::builder().with_size(egui::vec2(1400.0, 900.0));
    if gpu {
        builder = builder.wgpu();
    }
    builder.build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()))
}

/// Steps until the cloud streamed into the 3D view is read (10 s at
/// most).
fn streamed(h: &mut Harness<'_, App>) {
    let started = Instant::now();
    loop {
        h.step();
        let s = h.state_mut().ready().unwrap().view3d.stream_stats();
        if s.is_some_and(|s| s.pending == 0 && s.drawn > 0) {
            h.step();
            return;
        }
        assert!(started.elapsed() < Duration::from_secs(10), "{s:?}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// The real 3D view with a project's cloud streamed by node: every point
/// drawn is picked by a click at its place on the screen, quickly; a class
/// hidden is not drawn and not picked.
#[test]
fn every_point_streamed_into_the_3d_view_is_clickable() {
    if ohl_render::headless_device().is_err() {
        assert!(
            ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
            "no GPU"
        );
        return;
    }
    let (file, o) = project_with_cloud("lidar-click", 40);
    let mut h = harness(load(&file), true);
    h.state_mut().show_tab(crate::app::Tab::View3d);
    h.step();
    h.state_mut().ready().unwrap().view3d.camera = Some(above([o[0], o[1], o[2] + 2.0], 45.0));
    streamed(&mut h);
    let r = h.state_mut().ready().unwrap();
    let visible: HashSet<Layer> = r.visible.clone();
    let drawn = r.view3d.drawn_lidar(&r.model, &visible);
    assert_eq!(drawn.len(), 1600, "every point drawn (100 %)");
    let cam = r.view3d.camera.unwrap();
    let rect = r.view3d.rect.unwrap();
    let size = [f64::from(rect.width()), f64::from(rect.height())];
    let project = cam.projector(size);
    let snaps = lidar_only();
    let (mut tried, mut slowest) = (0, Duration::ZERO);
    for p in &drawn {
        let Some((at, _)) = project(crate::selection::lidar_xyz(&r.model, &p.0)) else {
            continue;
        };
        if at[0] < 0.0 || at[1] < 0.0 || at[0] > size[0] || at[1] > size[1] {
            continue;
        }
        let t = Instant::now();
        let (picked, _) = r
            .view3d
            .pick(&cam, size, &r.model, 0, &snaps, &visible, at, 14.0);
        slowest = slowest.max(t.elapsed());
        assert_eq!(picked, Selection::LidarPoint(*p));
        tried += 1;
    }
    assert!(tried > 1000, "{tried} points on the screen");
    println!("{tried} points clicked, slowest {slowest:.1?}");
    // Class 5 hidden: read again without it; its points are not picked.
    let hidden = *drawn.iter().find(|p| p.1 == 5).unwrap();
    r.model.hidden_classes.insert(5);
    streamed(&mut h);
    let r = h.state_mut().ready().unwrap();
    let now = r.view3d.drawn_lidar(&r.model, &visible);
    assert!(now.iter().all(|p| p.1 != 5) && !now.is_empty());
    let (at, _) = project(crate::selection::lidar_xyz(&r.model, &hidden.0)).unwrap();
    let (picked, _) = r
        .view3d
        .pick(&cam, size, &r.model, 0, &snaps, &visible, at, 10.0);
    assert_ne!(picked, Selection::LidarPoint(hidden));
    // The layer hidden: nothing.
    let mut none = visible.clone();
    none.remove(&Layer::Points);
    let (picked, _) = r
        .view3d
        .pick(&cam, size, &r.model, 0, &snaps, &none, at, 14.0);
    assert_eq!(picked, Selection::None);
}

/// The Edit tool on a LiDAR point: its window, the height and class
/// changed and applied (one command, logged), read back exactly; the
/// ground built from the cloud follows; undo gives every stored byte
/// back.
#[test]
fn the_edit_tool_changes_a_lidar_point_and_undo_restores_it() {
    use crate::editors::Form;
    use crate::tools::Tool;
    let (file, _) = project_with_cloud("lidar-edit", 30);
    let before = stored(&file);
    let model = load(&file);
    assert!(model.ground_from_lidar, "no surface: the ground from LiDAR");
    let mut h = harness(model, false);
    h.run();
    // A ground point near the first structure.
    let (target, at) = {
        let r = h.state_mut().ready().unwrap();
        let p = *r
            .model
            .points
            .iter()
            .filter(|p| p.1 == 2)
            .min_by_key(|p| (p.0[0] - 3000).abs() + (p.0[1] - 2000).abs())
            .unwrap();
        (p, crate::selection::lidar_xyz(&r.model, &p.0))
    };
    let ground_before = h.state_mut().ready().unwrap().model.ground_at(at[0], at[1]);
    assert!(ground_before.is_some_and(|z| (z - at[2]).abs() < 0.2));
    let edit = |h: &mut Harness<'_, App>, p: LidarPoint| {
        let r = h.state_mut().ready().unwrap();
        r.hotbar.hold(Tool::Edit);
        r.use_tool(crate::selection::Use {
            target: Selection::LidarPoint(p),
            at: None,
            on: None,
        });
        h.run();
    };
    edit(&mut h, target);
    {
        let r = h.state_mut().ready().unwrap();
        let Some(Form::Points(f)) = &mut r.editor else {
            panic!("{:?} {:?}", r.editor, r.status);
        };
        assert!(f.read_only.is_none());
        assert_eq!(f.points.len(), 1);
        assert_eq!(f.points[0].xyz, at, "the stored point behind it");
        f.points[0].xyz[2] += 1.5;
    }
    h.run();
    assert!(
        h.query_all_by_label("LiDAR point").count() >= 1,
        "its window"
    );
    h.get_by_label("as stored");
    h.get_by_label("Apply").click();
    h.run();
    let raised = [at[0], at[1], at[2] + 1.5];
    {
        let r = h.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        let p = stored_at(&file, raised).expect("read back where it was put");
        assert_eq!(p.class, 2);
        assert!(stored_at(&file, at).is_none());
        // The point in memory and the selection follow.
        let moved = ([target.0[0], target.0[1], target.0[2] + 1500], target.1);
        assert!(r.model.points.contains(&moved));
        assert_eq!(r.selection, Selection::LidarPoint(moved));
        let log = r.model.session().unwrap().log().unwrap();
        let last = log.last().unwrap();
        assert_eq!(
            last.objects,
            vec![(ohl_session::Kind::PointCloud, "c".to_owned())]
        );
    }
    // The ground built from the cloud follows (in the background).
    let started = Instant::now();
    loop {
        h.run();
        let z = h.state_mut().ready().unwrap().model.ground_at(at[0], at[1]);
        if z.is_some_and(|z| (z - raised[2]).abs() < 0.05) {
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(20), "{z:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_ne!(stored(&file), before);
    // Undo: every stored byte back.
    h.state_mut()
        .ready()
        .unwrap()
        .perform(crate::edit::Action::Undo);
    h.run();
    assert_eq!(stored(&file), before);
    {
        let r = h.state_mut().ready().unwrap();
        assert!(r.model.points.contains(&target));
        // The selection follows the point back.
        assert_eq!(r.selection, Selection::LidarPoint(target));
    }
    // Its class: vegetation (5), counted.
    let n5 = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        r.model
            .class_counts
            .iter()
            .find(|c| c.0 == 5)
            .map_or(0, |c| c.1)
    };
    let five = n5(&mut h);
    edit(&mut h, target);
    {
        let r = h.state_mut().ready().unwrap();
        let Some(Form::Points(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        f.points[0].class = 5;
    }
    crate::tests::press(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
    assert_eq!(stored_at(&file, at).map(|p| p.class), Some(5));
    assert_eq!(n5(&mut h), five + 1);
    assert_eq!(
        h.state_mut().ready().unwrap().selection,
        Selection::LidarPoint((target.0, 5))
    );
    h.state_mut()
        .ready()
        .unwrap()
        .perform(crate::edit::Action::Undo);
    h.run();
    assert_eq!(stored(&file), before);
    assert_eq!(n5(&mut h), five);
}

/// A viewer's Edit tool on a LiDAR point: the window, read-only — Apply
/// greyed, Enter changes nothing; the window closes.
#[test]
fn a_viewer_sees_the_lidar_point_read_only() {
    use crate::editors::Form;
    use crate::tools::Tool;
    let (file, _) = project_with_cloud("lidar-viewer", 10);
    let before = stored(&file);
    let mut model = load(&file);
    model.editing = Err("tester is a viewer of this project".into());
    let target = model.points[0];
    let mut h = harness(model, false);
    h.run();
    {
        let r = h.state_mut().ready().unwrap();
        r.hotbar.hold(Tool::Edit);
        r.use_tool(crate::selection::Use {
            target: Selection::LidarPoint(target),
            at: None,
            on: None,
        });
    }
    h.run();
    {
        let r = h.state_mut().ready().unwrap();
        let Some(Form::Points(f)) = &mut r.editor else {
            panic!("{:?} {:?}", r.editor, r.status);
        };
        assert!(f.read_only.as_deref().unwrap().contains("viewer"));
        f.points[0].xyz[2] += 3.0;
    }
    h.run();
    h.get_by_label_contains("Read-only: tester is a viewer");
    assert!(h.get_by_label("Apply").accesskit_node().is_disabled());
    crate::tests::press(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
    assert_eq!(stored(&file), before);
    {
        let r = h.state_mut().ready().unwrap();
        assert!(r.editor.is_some(), "the window stays, its reason shown");
        assert!(r.status.as_ref().is_some_and(|s| s.0.contains("read-only")));
    }
    // Closable: Cancel.
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.state_mut().ready().unwrap().editor.is_none());
}
