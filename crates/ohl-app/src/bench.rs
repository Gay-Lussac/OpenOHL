// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

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
        Look::Near3d(d) => {
            r.view3d.camera = Some(ohl_render::Camera {
                target: [mid[0], mid[1], mid[2] + 30.0],
                distance: d,
                yaw: (-120f64).to_radians(),
                pitch: 20f64.to_radians(),
                fov_y: 45f64.to_radians(),
                exaggeration: 1.0,
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
            let project = cam.projector(size);
            let at = [0.5 * size[0], 0.5 * size[1]];
            let _ = crate::selection::pick_in_model_case(&r.model, r.case, &snaps, at, 14.0, |p| {
                project(p).map(|(q, _)| q)
            });
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
            let _ =
                crate::selection::pick_in_model_case(&r.model, r.case, &snaps, at, 12.0, to_screen);
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
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 1000.0))
        .with_max_steps(10_000)
        .wgpu()
        .build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
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
fn gpu() -> bool {
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
