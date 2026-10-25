// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The frame benchmark (docs/development/performance.md): the whole
//! window drawn headless (egui_kittest with wgpu) with the 3D view, then
//! the plan view, at several distances; per view the time of a frame
//! split by phase ([`crate::perf`]), what the 3D view drew, and whether an
//! idle frame asks for another.
//!
//! ```bash
//! OHL_REQUIRE_GPU_TESTS=1 cargo test --release -p ohl-app frame_benchmark -- --ignored --nocapture
//! # a project of your own (a copy: it is opened for editing):
//! OHL_BENCH_FILE=/path/to/copy.ohl OHL_BENCH_PREFS=prefs.json cargo test --release …
//! ```

use std::time::{Duration, Instant};

use eframe::egui;
use egui_kittest::Harness;

use crate::app::{App, Tab};
use crate::model::{self, Model, Sources};

/// Where a view looks.
#[derive(Debug, Clone, Copy)]
pub enum Look {
    /// The 3D view fitted to the line.
    Whole3d,
    /// The 3D view at this distance from the middle structure's top, m.
    Near3d(f64),
    /// The 3D view this many metres behind the middle structure, looking
    /// along the x axis past it, low (a line along x seen down its length).
    Along3d(f64),
    /// The plan view fitted to the line.
    WholePlan,
    /// The plan view this many metres wide around the middle structure.
    Plan(f64),
}

impl Look {
    fn name(self) -> String {
        match self {
            Look::Whole3d => "3D whole line".into(),
            Look::Near3d(d) => format!("3D {d:.0} m from a structure"),
            Look::Along3d(d) => format!("3D {d:.0} m behind, along the line"),
            Look::WholePlan => "plan whole line".into(),
            Look::Plan(w) => format!("plan {w:.0} m wide"),
        }
    }
}

/// The views measured.
pub const LOOKS: [Look; 8] = [
    Look::Whole3d,
    Look::Near3d(3000.0),
    Look::Near3d(600.0),
    Look::Near3d(80.0),
    Look::WholePlan,
    Look::Plan(3000.0),
    Look::Plan(400.0),
    Look::Plan(60.0),
];

/// What a view measured.
#[derive(Debug, Clone)]
pub struct Measured {
    /// The view.
    pub look: Look,
    /// Median time of a frame (egui's frame, the app's drawing), ms.
    pub frame_ms: f64,
    /// Slowest frame, ms.
    pub max_ms: f64,
    /// Tessellating the frame's shapes, ms (median).
    pub tessellate_ms: f64,
    /// Mean time per frame of each phase, ms.
    pub phases: Vec<(&'static str, f64)>,
    /// Segments, points, triangles the 3D view drew.
    pub drawn: [u64; 3],
    /// Shapes egui was given.
    pub shapes: usize,
    /// The repaint delay asked for by an idle frame (MAX: none).
    pub idle_repaint: Duration,
    /// Frames until nothing was left to build.
    pub settle_frames: usize,
    /// A click at the view's centre: the time to pick, ms.
    pub pick_ms: f64,
}

/// The synthetic example, every case solved.
pub fn synthetic() -> Model {
    let root = std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let ex = root.join("examples/synthetic-line");
    let mut m = model::load(
        Sources {
            file: ex.join("synthetic-line.ohl.json"),
            library: Some(root.join("libraries/cables")),
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

/// The project of `OHL_BENCH_FILE` (with this user's view preferences
/// from the JSON file `OHL_BENCH_PREFS`), the cases shown solved.
fn from_env() -> Option<Model> {
    let file = ohl_core::env::var_os("OHL_BENCH_FILE")?;
    let view = ohl_core::env::var_os("OHL_BENCH_PREFS")
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<model::ViewPrefs>(&t).ok());
    let started = Instant::now();
    let mut m = model::load(
        Sources {
            file: file.into(),
            point_budget: 2_000_000,
            corridor: 60.0,
            view,
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    let mut need = m.shown(0);
    need.push(0);
    need.extend(m.display_cases.values().flatten().copied());
    for k in need {
        m.ensure_solved(k);
    }
    eprintln!("bench: loaded and solved in {:.1?}", started.elapsed());
    Some(m)
}

/// Sets the view of `look`; the tab it is on.
fn set_look(app: &mut App, look: Look) -> Tab {
    let r = app.ready().unwrap();
    let st = &r.model.line.structures;
    let mid = st.get(st.len() / 2).map_or(r.model.origin, |s| s.position);
    let (lo, hi) = st.iter().fold(
        ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]),
        |(lo, hi), s| {
            (
                [0, 1, 2].map(|k| lo[k].min(s.position[k])),
                [0, 1, 2].map(|k| hi[k].max(s.position[k] + 40.0)),
            )
        },
    );
    match look {
        Look::Whole3d => {
            r.view3d.camera = Some(ohl_render::Camera::fit(&ohl_render::Bounds {
                min: lo,
                max: hi,
            }));
            Tab::View3d
        }
        Look::Along3d(d) => {
            r.view3d.camera = Some(ohl_render::Camera {
                target: [mid[0], mid[1], mid[2] + 20.0],
                distance: d,
                yaw: std::f64::consts::PI,
                pitch: 5f64.to_radians(),
                fov_y: 45f64.to_radians(),
                exaggeration: 1.0,
                ortho: false,
            });
            Tab::View3d
        }
        Look::Near3d(d) => {
            r.view3d.camera = Some(ohl_render::Camera {
                target: [mid[0], mid[1], mid[2] + 30.0],
                distance: d,
                yaw: (-120f64).to_radians(),
                pitch: 20f64.to_radians(),
                fov_y: 45f64.to_radians(),
                exaggeration: 1.0,
                ortho: false,
            });
            Tab::View3d
        }
        Look::WholePlan => {
            r.plan.refit();
            Tab::Plan
        }
        Look::Plan(w) => {
            // The plan view is about 1000 px wide in the bench window.
            r.plan.set_view([mid[0], mid[1]], 1000.0 / w);
            Tab::Plan
        }
    }
}

/// The time to pick at the centre of the view of `tab` (a click), ms.
fn pick_time(app: &mut App, tab: Tab) -> f64 {
    let r = app.ready().unwrap();
    let snaps = r.effective_snaps();
    let started = Instant::now();
    match tab {
        Tab::View3d => {
            let (Some(cam), Some(rect)) = (r.view3d.camera, r.view3d.rect) else {
                return 0.0;
            };
            let size = [f64::from(rect.width()), f64::from(rect.height())];
            let at = [0.5 * size[0], 0.5 * size[1]];
            // What the view drew can be clicked (the LiDAR points drawn).
            let _ = r
                .view3d
                .pick(&cam, size, &r.model, r.case, &snaps, &r.visible, at, 14.0);
        }
        _ => {
            let (Some(rect), Some(c)) = (r.plan.drawn, r.plan.centre()) else {
                return 0.0;
            };
            let (mid, scale) = (rect.center(), r.plan.scale());
            let to_screen = |p: [f64; 3]| {
                Some([
                    f64::from(mid.x) + (p[0] - c[0]) * scale,
                    f64::from(mid.y) - (p[1] - c[1]) * scale,
                ])
            };
            let at = [f64::from(mid.x), f64::from(mid.y)];
            let _ = crate::selection::pick_shown(
                &r.model,
                r.case,
                &snaps,
                at,
                12.0,
                &|p| to_screen(p).map(|q| (q, 0.0)),
                &crate::selection::LidarShown::Nothing,
            );
        }
    }
    started.elapsed().as_secs_f64() * 1e3
}

/// A little motion, as a drag does (every frame is drawn again).
fn nudge(app: &mut App, tab: Tab, k: usize) {
    let r = app.ready().unwrap();
    let s = if k.is_multiple_of(2) { 1.0 } else { -1.0 };
    match tab {
        Tab::View3d => {
            if let Some(c) = r.view3d.camera.as_mut() {
                c.orbit(0.002 * s, 0.0);
            }
        }
        _ => {
            if let Some(c) = r.plan.centre() {
                let d = 2.0 / r.plan.scale();
                r.plan.set_view([c[0] + d * s, c[1]], r.plan.scale());
            }
        }
    }
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    v.get(v.len() / 2).copied().unwrap_or(0.0)
}

/// Measures `looks` on `model`, `frames` frames each.
pub fn measure(model: Model, looks: &[Look], frames: usize) -> Vec<Measured> {
    measure_with(model, looks, frames, |_| {})
}

/// [`measure`] with the app set up by `setup` first (its level-of-detail
/// settings, …).
pub fn measure_with(
    model: Model,
    looks: &[Look],
    frames: usize,
    setup: impl FnOnce(&mut App),
) -> Vec<Measured> {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 1000.0))
        .with_max_steps(10_000)
        .wgpu()
        .build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    setup(harness.state_mut());
    harness.step();
    let mut out = Vec::new();
    for &look in looks {
        let tab = set_look(harness.state_mut(), look);
        harness.state_mut().show_tab(tab);
        // Settle: everything built and read (the point cloud's readers
        // in the background), within a minute.
        let started = Instant::now();
        let mut settle_frames = 0;
        loop {
            harness.step();
            settle_frames += 1;
            let pending = harness.state_mut().ready().unwrap().view3d.pending();
            if (pending == 0 && settle_frames > 3) || started.elapsed() > Duration::from_secs(60) {
                break;
            }
            if pending > 0 {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        // Idle: a frame with nothing changed.
        harness.step();
        harness.step();
        let idle_repaint = harness
            .output()
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(Duration::MAX, |v| v.repaint_delay);
        crate::perf::enable(true);
        let _ = crate::perf::take();
        let (mut times, mut tess) = (Vec::new(), Vec::new());
        let mut shapes = 0;
        for k in 0..frames {
            nudge(harness.state_mut(), tab, k);
            let t = Instant::now();
            harness.step();
            times.push(t.elapsed().as_secs_f64() * 1e3);
            let shapes_now = harness.output().shapes.clone();
            shapes = shapes_now.len();
            let ppp = harness.output().pixels_per_point;
            let t = Instant::now();
            let _ = harness.ctx.tessellate(shapes_now, ppp);
            tess.push(t.elapsed().as_secs_f64() * 1e3);
        }
        crate::perf::enable(false);
        let pick_ms = pick_time(harness.state_mut(), tab);
        let phases = crate::perf::take()
            .into_iter()
            .map(|(n, d, _)| (n, d.as_secs_f64() * 1e3 / frames as f64))
            .collect();
        let r = harness.state_mut().ready().unwrap();
        let drawn = if matches!(tab, Tab::View3d) {
            r.view3d.drawn_counts(&r.visible)
        } else {
            [0; 3]
        };
        let max_ms = times.iter().copied().fold(0.0, f64::max);
        out.push(Measured {
            look,
            frame_ms: median(&mut times),
            max_ms,
            tessellate_ms: median(&mut tess),
            phases,
            drawn,
            shapes,
            idle_repaint,
            settle_frames,
            pick_ms,
        });
    }
    out
}

/// The measurements as a table.
pub fn report(rows: &[Measured]) -> String {
    let mut s = String::new();
    for m in rows {
        s += &format!(
            "{:<32} frame {:>7.2} ms (max {:>7.2}), tessellate {:>6.2} ms, {} shapes, \
             3D drew {} seg / {} pts / {} tri, idle repaint {}, settled in {} frames, \
             pick {:.2} ms\n",
            m.look.name(),
            m.frame_ms,
            m.max_ms,
            m.tessellate_ms,
            m.shapes,
            m.drawn[0],
            m.drawn[1],
            m.drawn[2],
            if m.idle_repaint == Duration::MAX {
                "none".to_string()
            } else {
                format!("after {:?}", m.idle_repaint)
            },
            m.settle_frames,
            m.pick_ms,
        );
        for (n, ms) in &m.phases {
            if *ms >= 0.01 {
                s += &format!("    {n:<28} {ms:>8.2} ms\n");
            }
        }
    }
    s
}

/// The frame benchmark: the synthetic example, or `OHL_BENCH_FILE`.
/// Ignored by default (run it in release, see the module's docs).
#[test]
#[ignore = "benchmark: run in release with --ignored --nocapture"]
fn frame_benchmark() {
    if ohl_render::headless_device().is_err() {
        assert!(
            ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
            "no GPU"
        );
        return;
    }
    let frames = ohl_core::env::var("OHL_BENCH_FRAMES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let model = from_env().unwrap_or_else(synthetic);
    let rows = measure(model, &LOOKS, frames);
    println!("{}", report(&rows));
}

/// A GPU, else (unless one is required) nothing to test.
pub(crate) fn gpu() -> bool {
    if ohl_render::headless_device().is_ok() {
        return true;
    }
    assert!(
        ohl_core::env::var_os("OHL_REQUIRE_GPU_TESTS").is_none(),
        "no GPU"
    );
    false
}

/// The synthetic example's window, `tab` in front, everything built.
fn settled(model: Model, tab: Tab) -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .wgpu()
        .build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    h.state_mut().show_tab(tab);
    for _ in 0..200 {
        h.step();
        if h.state_mut().ready().unwrap().view3d.pending() == 0 {
            break;
        }
    }
    h.step();
    h
}

/// Frames with nothing changed ask for no other frame, build no chunk
/// and draw no new 3D image (a frame for another panel shows the same
/// image); a camera move draws one.
#[test]
fn idle_frames_neither_repaint_nor_rebuild() {
    if !gpu() {
        return;
    }
    let mut h = settled(synthetic(), Tab::View3d);
    let counts = |h: &mut Harness<'_, App>| {
        let v = &h.state_mut().ready().unwrap().view3d;
        (v.chunks_built, v.images_drawn)
    };
    let before = counts(&mut h);
    assert!(before.0 > 0 && before.1 > 0, "{before:?}");
    for _ in 0..5 {
        h.step();
        let delay = h
            .output()
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map(|v| v.repaint_delay);
        assert_eq!(delay, Some(Duration::MAX), "an idle frame asks for another");
    }
    assert_eq!(counts(&mut h), before);
    nudge(h.state_mut(), Tab::View3d, 0);
    h.step();
    let after = counts(&mut h);
    assert_eq!(after.0, before.0, "a small turn builds nothing");
    assert_eq!(after.1, before.1 + 1, "a camera move draws the image again");
}

/// Every view of the frame benchmark draws the synthetic example well
/// within a frame (lenient: debug builds, shared machines).
#[test]
fn frames_stay_within_a_lenient_budget() {
    if !gpu() {
        return;
    }
    let rows = measure(synthetic(), &LOOKS, 3);
    for m in &rows {
        assert!(m.frame_ms < 250.0, "{}", report(&rows));
        assert_eq!(m.idle_repaint, Duration::MAX, "{}", report(&rows));
    }
}

/// The texts painted in the last frame.
fn texts(h: &Harness<'_, App>) -> Vec<String> {
    h.output()
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

/// The structures' names stay written at the farthest zoom (decluttered,
/// not hidden): in plan always, in 3D when ticked, beyond the label
/// distance too.
#[test]
fn structure_names_show_from_afar() {
    if !gpu() {
        return;
    }
    let mut h = settled(synthetic(), Tab::Plan);
    let ids: Vec<String> = h
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .structures
        .iter()
        .map(|s| s.id.clone())
        .collect();
    // The whole line in a few pixels: the first name written, the others
    // on top of it left out.
    let r = h.state_mut().ready().unwrap();
    let c = r.model.line.structures[3].position;
    r.plan.set_view([c[0], c[1]], 1e-3);
    h.step();
    let shown = texts(&h);
    assert!(shown.contains(&ids[0]), "{shown:?}");
    assert!(shown.iter().filter(|t| ids.contains(t)).count() < ids.len());
    // 3D, 30 km away (labels within 1.5 km): the names.
    h.state_mut().show_tab(Tab::View3d);
    let r = h.state_mut().ready().unwrap();
    r.model.section_display.structure_label[crate::annotations::View::ThreeD as usize] = true;
    if let Some(cam) = r.view3d.camera.as_mut() {
        cam.distance = 30_000.0;
    }
    h.step();
    h.step();
    let shown = texts(&h);
    assert!(ids.iter().any(|id| shown.contains(id)), "{shown:?}");
}

/// The plan's culling and thinning, the profile's dots: what is in view
/// kept, the ends of a line kept, one dot per pixel.
#[test]
fn plan_culls_and_thins_what_is_drawn() {
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 100.0));
    let cull = crate::plan::Cull::new(rect, [1000.0, 2000.0], 1.0, 0.0);
    assert!(cull.sees(&[[1000.0, 2000.0, 0.0]]));
    assert!(
        cull.sees(&[[900.0, 2000.0, 0.0], [1100.0, 2000.0, 0.0]]),
        "across"
    );
    assert!(!cull.sees(&[[1200.0, 2000.0, 0.0], [1300.0, 2100.0, 0.0]]));
    // 1000 points along 10 px: those half a pixel apart, and the last.
    let pts: Vec<egui::Pos2> = (0..1000)
        .map(|i| egui::pos2(i as f32 * 0.01, 0.0))
        .collect();
    let t = crate::plan::thin_px(pts.iter().copied());
    assert_eq!(t.first(), pts.first());
    assert_eq!(t.last(), pts.last());
    assert!(t.len() <= 22, "{}", t.len());
    // Spread out: every point kept.
    let wide: Vec<egui::Pos2> = (0..50).map(|i| egui::pos2(i as f32 * 2.0, 0.0)).collect();
    assert_eq!(crate::plan::thin_px(wide.iter().copied()), wide);
    let mut dots = crate::profile::PixelSet::new(rect);
    assert!(dots.insert(egui::pos2(10.2, 10.2)));
    assert!(!dots.insert(egui::pos2(9.9, 10.4)), "the same pixel");
    assert!(dots.insert(egui::pos2(11.0, 10.0)));
    assert!(!dots.insert(egui::pos2(-50.0, 10.0)), "off the view");
}

/// A drawing as large as the background of a long line (invented, the
/// same every time): over an area 28 km long along x through the
/// synthetic example and `width` m wide, parcels of 40 m (eight corners
/// and a number each), 300 buildings (with a name), 600 boundary marks
/// (small circles of 73 points) and 300 points a km², contour lines 30 m
/// apart with a vertex every 2 m. The corridor of a line (1.2 km): 51 000
/// lines with 2.2 million vertices, 31 000 texts.
pub fn large_drawing(width: f64) -> crate::backgrounds::LoadedDrawing {
    use ohl_background::dxf::{DrawingLayer, Label, Mark, Polyline};
    // A small linear congruential generator: the same drawing every time.
    let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
    let mut rand = move || {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (seed >> 11) as f64 / (1u64 << 53) as f64
    };
    let (x0, x1) = (-13_400.0, 14_600.0);
    let (y0, y1) = (280.0 - width / 2.0, 280.0 + width / 2.0);
    let km2 = (x1 - x0) * (y1 - y0) / 1e6;
    let per_km2 = |n: f64| (n * km2) as usize;
    let z = 500.0;
    let layer = |name: &str, color: [u8; 3]| DrawingLayer {
        name: name.into(),
        color,
        off: false,
    };
    let mut d = ohl_background::Drawing {
        layers: vec![
            layer("Parcels", [200, 200, 200]),
            layer("Buildings", [230, 120, 90]),
            layer("Marks", [120, 220, 120]),
            layer("Contours", [160, 120, 60]),
            layer("Texts", [240, 240, 240]),
        ],
        ..Default::default()
    };
    // Parcels: 40 m cells, eight corners each, a little jittered; a number
    // in each.
    const CORNERS: [(f64, f64); 8] = [
        (0.0, 0.0),
        (0.5, 0.0),
        (1.0, 0.0),
        (1.0, 0.5),
        (1.0, 1.0),
        (0.5, 1.0),
        (0.0, 1.0),
        (0.0, 0.5),
    ];
    let cell = 40.0;
    let (nx, ny) = (((x1 - x0) / cell) as usize, ((y1 - y0) / cell) as usize);
    for i in 0..nx {
        for j in 0..ny {
            let (cx, cy) = (x0 + i as f64 * cell, y0 + j as f64 * cell);
            let points = CORNERS
                .iter()
                .map(|(u, v)| {
                    [
                        cx + u * cell + rand() - 0.5,
                        cy + v * cell + rand() - 0.5,
                        z,
                    ]
                })
                .collect();
            d.lines.push(Polyline {
                layer: 0,
                color: [200, 200, 200],
                points,
                closed: true,
            });
            d.texts.push(Label {
                layer: 4,
                color: [240, 240, 240],
                at: [cx + 0.4 * cell, cy + 0.5 * cell, z],
                height: 1.5,
                angle_deg: 0.0,
                text: format!("{}", 1000 + i * ny + j),
            });
        }
    }
    let mut at = || [x0 + rand() * (x1 - x0), y0 + rand() * (y1 - y0), z];
    for _ in 0..per_km2(300.0) {
        let c = at();
        let s = at();
        // Sides of 8 to 18 m by 6 to 14 m.
        let (w, h) = (
            8.0 + 10.0 * (s[0] - x0) / (x1 - x0),
            6.0 + 8.0 * (s[1] - y0) / (y1 - y0),
        );
        d.lines.push(Polyline {
            layer: 1,
            color: [230, 120, 90],
            points: vec![
                c,
                [c[0] + w, c[1], z],
                [c[0] + w, c[1] + h, z],
                [c[0], c[1] + h, z],
            ],
            closed: true,
        });
        d.texts.push(Label {
            layer: 4,
            color: [240, 240, 240],
            at: [c[0] + 1.0, c[1] + 1.0, z],
            height: 1.0,
            angle_deg: 30.0,
            text: "Building".into(),
        });
    }
    for _ in 0..per_km2(600.0) {
        let c = at();
        d.lines.push(Polyline {
            layer: 2,
            color: [120, 220, 120],
            points: (0..=72)
                .map(|k| {
                    let a = f64::from(k) * 5f64.to_radians();
                    [c[0] + 0.3 * a.cos(), c[1] + 0.3 * a.sin(), z]
                })
                .collect(),
            closed: false,
        });
    }
    for _ in 0..per_km2(300.0) {
        let c = at();
        d.points.push(Mark {
            layer: 2,
            color: [120, 220, 120],
            at: c,
        });
    }
    let contours = ((y1 - y0) / 30.0) as u32;
    for k in 0..contours {
        let y = y0 + (f64::from(k) + 0.5) * 30.0;
        let n = ((x1 - x0) / 2.0) as usize;
        d.lines.push(Polyline {
            layer: 3,
            color: [160, 120, 60],
            points: (0..=n)
                .map(|i| {
                    let x = x0 + i as f64 * 2.0;
                    [x, y + 8.0 * (x / 150.0).sin(), z + f64::from(k % 40)]
                })
                .collect(),
            closed: false,
        });
    }
    let info = ohl_background::DrawingInfo {
        name: "large".into(),
        source: "invented".into(),
        crs: String::new(),
        items: (d.lines.len() + d.points.len() + d.texts.len()) as u64,
        bounds: d.bounds(),
    };
    crate::backgrounds::LoadedDrawing::new(info, d)
}

/// What a flight measured ([`fly`]).
struct Flight {
    /// Median frame, ms.
    median: f64,
    /// Slowest frame, ms.
    max: f64,
    /// Mean time per frame of each phase, ms.
    phases: Vec<(&'static str, f64)>,
    /// Segments, points, triangles drawn in the last frame.
    drawn: [u64; 3],
    /// Segments, points, triangles the chunks hold on the GPU.
    held: [u64; 3],
}

/// Frame times while flying along the line, the eye `distance` m behind
/// its target: the camera moved `step` m along x every frame (chunks change level and are built as they come
/// near), `frames` frames: the median and slowest frame, ms, the mean
/// time per frame of each phase, what the 3D view drew last and what its
/// chunks hold on the GPU then. A frame is timed until the GPU has drawn
/// it.
fn fly(model: Model, distance: f64, step: f64, frames: usize) -> Flight {
    let state: std::rc::Rc<std::cell::RefCell<Option<eframe::egui_wgpu::RenderState>>> =
        Default::default();
    let keep = state.clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 1000.0))
        .with_max_steps(10_000)
        .wgpu()
        .build_eframe(move |cc| {
            *keep.borrow_mut() = cc.wgpu_render_state.clone();
            App::with_model(model, cc.wgpu_render_state.clone())
        });
    let device = state.borrow().as_ref().expect("a GPU").device.clone();
    h.step();
    h.state_mut().show_tab(Tab::View3d);
    let start = {
        let r = h.state_mut().ready().unwrap();
        let st = &r.model.line.structures;
        st[st.len() / 2].position
    };
    let camera = |x: f64| ohl_render::Camera {
        target: [x, start[1], start[2] + 30.0],
        distance,
        yaw: std::f64::consts::PI,
        pitch: 15f64.to_radians(),
        fov_y: 45f64.to_radians(),
        exaggeration: 1.0,
        ortho: false,
    };
    // From 6 km east, flying west.
    let x0 = start[0] + 6_000.0;
    h.state_mut().ready().unwrap().view3d.camera = Some(camera(x0));
    for _ in 0..200 {
        h.step();
        if h.state_mut().ready().unwrap().view3d.pending() == 0 {
            break;
        }
    }
    crate::perf::enable(true);
    let _ = crate::perf::take();
    let mut times = Vec::new();
    for k in 0..frames {
        h.state_mut().ready().unwrap().view3d.camera = Some(camera(x0 - step * k as f64));
        let t = Instant::now();
        h.step();
        // The frame's image drawn by the GPU too.
        let _ = device.poll(eframe::wgpu::PollType::wait_indefinitely());
        times.push(t.elapsed().as_secs_f64() * 1e3);
    }
    crate::perf::enable(false);
    let phases = crate::perf::take()
        .into_iter()
        .map(|(n, d, _)| (n, d.as_secs_f64() * 1e3 / frames as f64))
        .collect();
    let r = h.state_mut().ready().unwrap();
    let drawn = r.view3d.drawn_counts(&r.visible);
    let held = r.view3d.held_counts();
    let max = times.iter().copied().fold(0.0, f64::max);
    Flight {
        median: median(&mut times),
        max,
        phases,
        drawn,
        held,
    }
}

/// The frame benchmark with a large drawing ([`large_drawing`]) over the
/// synthetic example, 1.2 km and 9 km wide: indexing and building its
/// chunks, the views of [`LOOKS`], then flying along the line.
///
/// ```bash
/// OHL_REQUIRE_GPU_TESTS=1 cargo test --release -p ohl-app drawing_benchmark -- --ignored --nocapture
/// ```
#[test]
#[ignore = "benchmark: run in release with --ignored --nocapture"]
fn drawing_benchmark() {
    use crate::scene::Layer;
    if !gpu() {
        return;
    }
    let frames = ohl_core::env::var("OHL_BENCH_FRAMES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    // The corridor of a line, then a map sheet as wide as a third of its
    // length.
    for width in [1_200.0, 9_000.0] {
        println!("--- a drawing {width} m wide");
        let with_drawing = || {
            let mut model = synthetic();
            model.backgrounds.drawings.push(large_drawing(width));
            model
        };
        let started = Instant::now();
        let model = with_drawing();
        let d = &model.backgrounds.drawings[0].drawing;
        println!(
            "drawing: {} lines ({} vertices), {} points, {} texts, made in {:.1?}",
            d.lines.len(),
            d.lines.iter().map(|l| l.points.len()).sum::<usize>(),
            d.points.len(),
            d.texts.len(),
            started.elapsed()
        );
        let started = Instant::now();
        let index = crate::chunks::ChunkIndex::new(&model, Layer::Drawings, 0);
        println!(
            "index: {} chunks in {:.1?}",
            index.keys().count(),
            started.elapsed()
        );
        let row = crate::lod::LodSettings::default().row(Layer::Drawings);
        for level in 0..3 {
            let started = Instant::now();
            let segments: usize = index
                .keys()
                .map(|(t, _)| {
                    index
                        .build(&model, Layer::Drawings, &t, level, &row)
                        .counts()
                        .0
                })
                .sum();
            let n = index.keys().count().max(1);
            println!(
                "level {level}: every chunk built in {:.1?} ({:.2} ms a chunk), {segments} segments",
                started.elapsed(),
                started.elapsed().as_secs_f64() * 1e3 / n as f64
            );
        }
        let rows = measure(model, &LOOKS, frames);
        println!("{}", report(&rows));
        // 10 km each.
        for (distance, step) in [(400.0, 50.0), (2_500.0, 100.0)] {
            let frames = (10_000.0 / step) as usize;
            let Flight {
                median,
                max,
                phases,
                drawn,
                held,
            } = fly(with_drawing(), distance, step, frames);
            println!(
                "flying {step} m a frame, {distance} m behind: frame {median:.2} ms (max \
                 {max:.2}), 3D drew {} seg / {} pts / {} tri; chunks hold {:.0} MB",
                drawn[0],
                drawn[1],
                drawn[2],
                // Segments 32 bytes, points 20, triangles 3 × 16.
                (held[0] * 32 + held[1] * 20 + held[2] * 48) as f64 / 1e6
            );
            for (n, ms) in &phases {
                if *ms >= 0.01 {
                    println!("    {n:<28} {ms:>8.2} ms");
                }
            }
        }
    }
}

/// The edit benchmark: one section's stringing changed (as the span
/// tool or the section editor does) on `OHL_BENCH_FILE` (a copy: it is
/// edited), with the shown cases solved, then with every case solved;
/// the time of the whole edit and of its parts (one case's re-solve, all
/// cases'). Ignored by default; prints a table.
///
/// ```bash
/// OHL_BENCH_FILE=/path/to/copy.ohl cargo test --release -p ohl-app edit_benchmark -- --ignored --nocapture
/// ```
#[test]
#[ignore = "benchmark: run in release with --ignored --nocapture"]
fn edit_benchmark() {
    let Some(mut m) = from_env() else {
        eprintln!("edit benchmark: give OHL_BENCH_FILE");
        return;
    };
    // A section in the middle with several spans.
    let line = m.line.clone();
    let section = line
        .sections
        .iter()
        .filter(|s| s.points.len() > 3)
        .nth(line.sections.len() / 4)
        .or_else(|| line.sections.first())
        .unwrap()
        .clone();
    let changed = |s: &ohl_line::Stringing, k: f64| match s {
        ohl_line::Stringing::Tension {
            horizontal_tension,
            temperature,
        } => ohl_line::Stringing::Tension {
            horizontal_tension: horizontal_tension + k,
            temperature: *temperature,
        },
        other => other.clone(),
    };
    let mut k = 0.0;
    let mut edit = |m: &mut Model, label: &str| {
        k += 1.0;
        let solved = m.cases.iter().filter(|c| c.ready).count();
        let command = ohl_session::Command::SetStringing {
            section: section.id.clone(),
            stringing: changed(&section.stringing, k),
        };
        let t = Instant::now();
        let report = m.apply("bench", command).unwrap();
        let whole = t.elapsed();
        // One case's re-solve alone, and the jumpers' part of it.
        let dirty = vec![section.id.clone()];
        let first = m.cases.iter().position(|c| c.ready).unwrap();
        let t = Instant::now();
        m.cases[first].update(&m.line, &m.library, &dirty);
        let one = t.elapsed();
        let t = Instant::now();
        m.cases[first].update(&m.line, &m.library, &[]);
        let nothing = t.elapsed();
        println!(
            "{label}: {solved} cases solved; edit {whole:.1?} ({report}); one case's update \
             {one:.1?}, of it with no section dirty (jumpers, shapes, bundles) {nothing:.1?}"
        );
    };
    edit(&mut m, "shown cases");
    edit(&mut m, "shown cases, again");
    // More edits to profile them (OHL_BENCH_EDITS).
    let more: usize = ohl_core::env::var("OHL_BENCH_EDITS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    for _ in 0..more {
        edit(&mut m, "again");
    }
    // The results' revision (what the check pages compare): once per edit.
    let t = Instant::now();
    m.line_version += 1;
    let _ = m.revision();
    println!("the results' revision, first: {:.1?}", t.elapsed());
    let t = Instant::now();
    m.line_version += 1;
    let _ = m.revision();
    println!("the results' revision after an edit: {:.1?}", t.elapsed());
    {
        let stored = m.stored_line().unwrap().clone();
        let t = Instant::now();
        let _ = crate::revision::hash_debug(&stored);
        let a = t.elapsed();
        let t = Instant::now();
        let _ = crate::revision::hash_debug(&m.library);
        let b = t.elapsed();
        let t = Instant::now();
        let _ = stored.clone();
        let c = t.elapsed();
        struct W(std::collections::hash_map::DefaultHasher);
        impl std::io::Write for W {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                std::hash::Hasher::write(&mut self.0, b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let t = Instant::now();
        let mut w = W(Default::default());
        serde_json::to_writer(&mut w, &stored).unwrap();
        let d = t.elapsed();
        println!(
            "  of it: line written out {a:.1?}, wire library {b:.1?}, line copied {c:.1?}; line as JSON {d:.1?}"
        );
    }
    // The case brought up to date edit by edit against a full solve of
    // the line as it is now: the largest difference of a clamp and of a
    // jumper point, m.
    {
        let first = m.cases.iter().position(|c| c.ready).unwrap();
        let c = &m.cases[first];
        let full = crate::model::Solved::new(&m.line, &m.library, c.case.clone());
        let mut clamp: f64 = 0.0;
        let mut worst = Vec::new();
        for (a, b) in c.shapes.iter().zip(&full.shapes) {
            assert_eq!(a.section, b.section);
            for (p, q) in a.clamps.iter().zip(&b.clamps) {
                let d = (0..3).map(|i| (p[i] - q[i]).abs()).fold(0.0, f64::max);
                clamp = clamp.max(d);
                if d > 1e-3 {
                    worst.push((d, a.section.clone()));
                }
            }
        }
        let mut jumper: f64 = 0.0;
        for (a, b) in c.jumpers.iter().zip(&full.jumpers) {
            for (p, q) in a.points.iter().zip(&b.points) {
                jumper = jumper.max((0..3).map(|i| (p[i] - q[i]).abs()).fold(0.0, f64::max));
            }
        }
        worst.sort_by(|a, b| b.0.total_cmp(&a.0));
        worst.dedup_by(|a, b| a.1 == b.1);
        println!(
            "case {}: sections differing by more than 1 mm: {:?}",
            c.case.name,
            &worst[..worst.len().min(12)]
        );
        println!(
            "incremental vs full solve: {} / {} sections, {} / {} jumpers; largest clamp \
             difference {clamp:.2e} m, jumper point {jumper:.2e} m",
            c.shapes.len(),
            full.shapes.len(),
            c.jumpers.len(),
            full.jumpers.len()
        );
    }
    // The jumpers' loads as an estimate and as their pull: one case
    // solved, then updated with nothing changed and with the section.
    for loads in [ohl_line::JumperLoads::Estimate, ohl_line::JumperLoads::Pull] {
        let mut line = m.line.clone();
        line.solver.jumper_loads = loads;
        let first = m.cases.iter().position(|c| c.ready).unwrap();
        let case = m.cases[first].case.clone();
        let t = Instant::now();
        let mut s = ohl_line::incremental::SolvedLine::solve(&line, &m.library, &case);
        let solve = t.elapsed();
        // (Repeated with OHL_BENCH_REPEAT, to profile it.)
        let repeat: usize = ohl_core::env::var("OHL_BENCH_REPEAT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);
        let t = Instant::now();
        for _ in 0..repeat {
            s.update(&line, &m.library, &[]);
        }
        let nothing = t.elapsed() / repeat as u32;
        let t = Instant::now();
        s.update(&line, &m.library, std::slice::from_ref(&section.id));
        let one = t.elapsed();
        println!(
            "jumper loads {loads:?}: whole case {solve:.1?}; update, nothing changed \
             {nothing:.1?}; update, the section {one:.1?}"
        );
    }
    // Every case solved (minutes on a real line): with OHL_BENCH_ALL.
    if ohl_core::env::var_os("OHL_BENCH_ALL").is_none() {
        return;
    }
    let t = Instant::now();
    for k in 0..m.cases.len() {
        m.ensure_solved(k);
    }
    println!("every case solved in {:.1?}", t.elapsed());
    edit(&mut m, "every case");
}

/// The distance under unequal wind on a project of your own: the line
/// solved at the levels of the windiest case (step `OHL_BENCH_STEP`,
/// default 50 Pa), then measured with the check plugin in the folder
/// `OHL_BENCH_PLUGIN`; each part timed.
///
/// ```bash
/// OHL_BENCH_FILE=copy.ohl OHL_BENCH_PLUGIN=path/to/plugin \
///   cargo test --release -p ohl-app unequal_wind_benchmark -- --ignored --nocapture
/// ```
#[test]
#[ignore = "a benchmark on a project of your own"]
fn unequal_wind_benchmark() {
    let (Some(mut m), Some(dir)) = (from_env(), ohl_core::env::var_os("OHL_BENCH_PLUGIN")) else {
        eprintln!("skipping: set OHL_BENCH_FILE and OHL_BENCH_PLUGIN");
        return;
    };
    let step: f64 = ohl_core::env::var("OHL_BENCH_STEP")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(crate::phase_distance::DEFAULT_WIND_STEP);
    let host = ohl_plugins::WasmHost::new().unwrap();
    let (checks, problems) = ohl_io::checks::list(&host, &[dir.into()]);
    assert!(problems.is_empty(), "{problems:?}");
    let check = checks
        .iter()
        .find(|c| c.info.id == ohl_io::checks::UNEQUAL_WIND)
        .expect("the plugin runs the unequal wind check");
    let k = (0..m.cases.len())
        .max_by(|a, b| {
            let p = |k: usize| m.cases[k].case.wind_pressure.si();
            p(*a).total_cmp(&p(*b))
        })
        .unwrap();
    m.ensure_solved(k);
    let case = m.cases[k].case.clone();
    let t = Instant::now();
    let (levels, notes) =
        ohl_clearance::unequal_wind::solve_levels(&m.line, &m.library, &case, step, 0.6, |c| {
            let s = model::Solved::new(&m.line, &m.library, c.clone());
            (s.shapes, s.failed)
        })
        .unwrap();
    let solved = t.elapsed();
    let t = Instant::now();
    let pairs = ohl_io::checks::unequal_wind(
        &host,
        check,
        &check.info.values,
        &[],
        &Default::default(),
        &levels,
        &ohl_core::budget::Budget::new(),
    )
    .unwrap();
    let measured = t.elapsed();
    let below = pairs.iter().filter(|p| p.margin() < 0.0).count();
    println!(
        "unequal wind, {:.0} Pa by {step} Pa: {} levels solved in {solved:.1?} ({} notes); {} \
         conductors, {} pairs ({below} below) measured in {measured:.1?}",
        case.wind_pressure.si(),
        levels.len(),
        notes.len(),
        levels[0].conductors.len(),
        pairs.len()
    );
}

/// The measuring tools' cost while the pointer moves over the 3D view
/// (owner, 2026-10-25: responsive first): frame times with Select held,
/// then with Distance held after a first pick (each frame finds what is
/// under the cursor and measures to it). The project of `OHL_BENCH_FILE`,
/// else the synthetic line. Prints the median and slowest frame of each.
#[test]
#[ignore = "benchmark: run in release with --ignored --nocapture"]
fn measuring_benchmark() {
    let model = from_env().unwrap_or_else(synthetic);
    let state: std::rc::Rc<std::cell::RefCell<Option<eframe::egui_wgpu::RenderState>>> =
        Default::default();
    let keep = state.clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 1000.0))
        .with_max_steps(10_000)
        .wgpu()
        .build_eframe(move |cc| {
            *keep.borrow_mut() = cc.wgpu_render_state.clone();
            App::with_model(model, cc.wgpu_render_state.clone())
        });
    let device = state.borrow().as_ref().expect("a GPU").device.clone();
    h.step();
    h.state_mut().show_tab(Tab::View3d);
    let (mid, first) = {
        let r = h.state_mut().ready().unwrap();
        let st = &r.model.line.structures;
        (st[st.len() / 2].position, st[st.len() / 2].id.clone())
    };
    h.state_mut().ready().unwrap().view3d.camera = Some(ohl_render::Camera {
        target: [mid[0], mid[1], mid[2] + 20.0],
        distance: 250.0,
        yaw: std::f64::consts::PI,
        pitch: 20f64.to_radians(),
        fov_y: 45f64.to_radians(),
        exaggeration: 1.0,
        ortho: false,
    });
    for _ in 0..300 {
        h.step();
        if h.state_mut().ready().unwrap().view3d.pending() == 0 {
            break;
        }
    }
    let frames = 120;
    let run = |h: &mut Harness<'_, App>| {
        let mut times = Vec::new();
        for k in 0..frames {
            let x = 500.0 + 600.0 * (k as f32 / frames as f32);
            h.input_mut()
                .events
                .push(egui::Event::PointerMoved(egui::pos2(x, 520.0)));
            let t = Instant::now();
            h.step();
            let _ = device.poll(eframe::wgpu::PollType::wait_indefinitely());
            times.push(t.elapsed().as_secs_f64() * 1e3);
        }
        let max = times.iter().copied().fold(0.0, f64::max);
        (median(&mut times), max)
    };
    let (select, select_max) = run(&mut h);
    {
        let r = h.state_mut().ready().unwrap();
        r.hotbar.hold(crate::tools::Tool::MeasureDistance);
        r.use_tool(crate::selection::Use {
            target: crate::selection::Selection::Structure(first),
            at: None,
            on: None,
        });
    }
    let (measuring, measuring_max) = run(&mut h);
    let r = h.state_mut().ready().unwrap();
    println!(
        "measuring: frames with Select {select:.1} ms (slowest {select_max:.1}), with Distance \
         held {measuring:.1} ms (slowest {measuring_max:.1}); live value {}",
        r.measuring
            .live
            .as_ref()
            .map_or("none".into(), |m| m.label())
    );
}

/// Follow and snap on a project of your own (`OHL_BENCH_FILE`): a span in
/// the middle of the line, the classes shown, the default radius — how
/// long its surroundings take to read, a cursor move (the nearest point
/// found and measured), and N (the closest spot of the whole span).
#[test]
#[ignore = "a benchmark on a project of your own"]
fn follow_benchmark() {
    let Some(m) = from_env() else {
        eprintln!("skipping: set OHL_BENCH_FILE");
        return;
    };
    let ctx = egui::Context::default();
    let case = 0;
    let sec = m.line.sections[m.line.sections.len() / 2].id.clone();
    let points = m.shape(&sec, case).unwrap().spans[0].points.clone();
    let lines = crate::measure::Centrelines::of(&m);
    let mut f = crate::follow::Follow::default();
    f.subject = Some(crate::follow::Subject::Span {
        section: sec.clone(),
        span: 0,
    });
    let at = |k: usize, n: usize| crate::selection::Use {
        target: crate::selection::Selection::Span {
            section: sec.clone(),
            span: 0,
        },
        at: None,
        on: crate::measure::point_along(&points, k as f64 / n as f64),
    };
    let t = Instant::now();
    loop {
        f.hover(&m, case, &lines, "c", Some(&at(1, 2)), &ctx);
        if !f.status.starts_with("reading") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert!(t.elapsed().as_secs() < 120, "read within two minutes");
    }
    let read = t.elapsed();
    let n = 400;
    let t = Instant::now();
    let mut found = 0;
    for k in 0..n {
        f.hover(&m, case, &lines, "c", Some(&at(k, n)), &ctx);
        found += usize::from(f.live.is_some());
    }
    let hover = t.elapsed() / n as u32;
    let t = Instant::now();
    f.closest(&m, case, &lines, "c");
    let closest = t.elapsed();
    println!(
        "follow and snap on {sec} span 1: surroundings read in {read:.1?} ({}); a cursor move \
         {hover:.1?} ({found} of {n} with a point within reach); N {closest:.1?}: {}",
        f.status,
        f.live.as_ref().map_or("none".into(), |m| m.label())
    );
}

/// The wire tools on a project of your own (`OHL_BENCH_FILE`): Clearance
/// at a point (with its sweep along the span) and Wire to wire on two
/// spans in the middle of the line — each computed live under the cursor,
/// so each should take well under a frame.
#[test]
#[ignore = "a benchmark on a project of your own"]
fn wire_tools_benchmark() {
    let Some(m) = from_env() else {
        eprintln!("skipping: set OHL_BENCH_FILE");
        return;
    };
    let lines = crate::measure::Centrelines::of(&m);
    let case = m.cases[0].case.name.clone();
    let k = m.line.sections.len() / 2;
    let pick = |s: usize| {
        crate::measure::pick_of(
            &m,
            0,
            &ohl_session::annotations::Anchor::Wire {
                section: m.line.sections[s].id.clone(),
                span: 0,
                fraction: 0.5,
            },
        )
        .unwrap()
    };
    let (a, b) = (pick(k), pick(k + 1));
    let n = 20;
    let t = Instant::now();
    let mut c = None;
    for _ in 0..n {
        c = Some(crate::measure::measure(
            &m,
            &lines,
            crate::measure::Kind::Clearance,
            std::slice::from_ref(&a),
            &case,
        ));
    }
    let clearance = t.elapsed() / n;
    let t = Instant::now();
    let mut w = None;
    for _ in 0..n {
        w = Some(crate::measure::measure(
            &m,
            &lines,
            crate::measure::Kind::WireToWire,
            &[a.clone(), b.clone()],
            &case,
        ));
    }
    let wires = t.elapsed() / n;
    println!(
        "wire tools: clearance {clearance:.1?} ({}), wire to wire {wires:.1?} ({})",
        c.unwrap().label(),
        w.unwrap().label()
    );
}
