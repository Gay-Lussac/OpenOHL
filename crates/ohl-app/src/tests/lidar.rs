// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: LiDAR classes and chunks, files created and opened, surveys, the
//! point budget.

use super::*;

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
pub(super) fn grid_points(
    x0: f64,
    y0: f64,
    w: u32,
    h: u32,
) -> impl Iterator<Item = ohl_lidar::LidarPoint> {
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
pub(super) fn write_survey(path: &std::path::Path, x0: f64, w: usize) {
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
