// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Structures at levels 2 and 3 drawn with their members: the 3D view's
//! scene, picking a member, the plan and profile drawings, and a
//! performance check on a long line.

use std::path::{Path, PathBuf};
use std::time::Instant;

use eframe::egui;
use egui_kittest::Harness;

use crate::app::App;
use crate::model::{self, Model, Sources};
use crate::scene::{self, Layer, Prim};

fn root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// Invented angles for type-02's legs, braces and arms (the members'
/// sections are looked up in the section library first).
const ALL_ANGLES: &str = "format = \"ohl-sections\"\nversion = 1\nname = \"t\"\n\
     default_material = \"s\"\n\
     [[material]]\nid = \"s\"\nelasticity = \"210 GPa\"\ndensity = \"7850 kg/m3\"\n\
     yield = \"355 MPa\"\n\
     [[section]]\nid = \"leg\"\nshape = \"equal_angle\"\nh = \"100 mm\"\nt = \"10 mm\"\nr = \"0 mm\"\n\
     [[section]]\nid = \"brace\"\nshape = \"equal_angle\"\nh = \"60 mm\"\nt = \"6 mm\"\nr = \"0 mm\"\n\
     [[section]]\nid = \"arm\"\nshape = \"equal_angle\"\nh = \"80 mm\"\nt = \"8 mm\"\nr = \"0 mm\"\n";

/// The typed example in a folder of its own (`dir`), with a section
/// library holding a profile for type-02's `leg` section when `profile`.
fn typed_in(dir: &Path, profile: bool) -> Model {
    typed_with(dir, profile.then_some(LEGS))
}

/// An invented angle 100 × 10 for the legs.
const LEGS: &str = "format = \"ohl-sections\"\nversion = 1\nname = \"t\"\n\
     default_material = \"s\"\n\
     [[material]]\nid = \"s\"\nelasticity = \"210 GPa\"\ndensity = \"7850 kg/m3\"\n\
     yield = \"355 MPa\"\n\
     [[section]]\nid = \"leg\"\nshape = \"equal_angle\"\nh = \"100 mm\"\n\
     t = \"10 mm\"\nr = \"0 mm\"\n";

/// The typed example in a folder of its own (`dir`), with `sections` as
/// its section library (none: no library).
fn typed_with(dir: &Path, sections: Option<&str>) -> Model {
    let ex = root().join("examples/synthetic-line");
    let structures = dir.join("Structures");
    std::fs::create_dir_all(&structures).unwrap();
    for e in std::fs::read_dir(ex.join("Structures")).unwrap() {
        let p = e.unwrap().path();
        std::fs::copy(&p, structures.join(p.file_name().unwrap())).unwrap();
    }
    let file = dir.join("synthetic-line-typed.ohl.json");
    std::fs::copy(ex.join("synthetic-line-typed.ohl.json"), &file).unwrap();
    if let Some(text) = sections {
        std::fs::create_dir_all(dir.join("Sections")).unwrap();
        std::fs::write(dir.join("Sections/legs.ohlsec"), text).unwrap();
    }
    model::load(
        Sources {
            file,
            library: Some(root().join("libraries/cables")),
            structures: Some(structures),
            point_budget: 1000,
            corridor: 60.0,
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap()
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("ohl-design-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// The structures layer's primitives anchored at structure `id`.
fn prims_at<'a>(m: &Model, prims: &'a [Prim], id: &str) -> Vec<&'a Prim> {
    let p = m.line.structure(id).unwrap().position;
    prims.iter().filter(|x| x.anchor == [p[0], p[1]]).collect()
}

#[test]
fn the_3d_view_draws_the_members_of_levels_2_and_3() {
    let dir = scratch("members-3d");
    let m = typed_in(&dir, true);
    let prims = scene::prims(&m, Layer::Structures, 0);
    // T2 (type-02, 178 members): each member a line at level 0, as the line
    // places the type (the resolved stick drawing).
    let t2 = m.line.structure("T2").unwrap();
    let at = prims_at(&m, &prims, "T2");
    let lines: Vec<&Vec<[f64; 3]>> = at
        .iter()
        .filter(|p| !p.filled && p.levels == (0, 0) && p.width == 1.5)
        .map(|p| &p.points)
        .collect();
    assert_eq!(lines.len(), 178);
    for (l, s) in lines.iter().zip(&t2.members) {
        assert_eq!(*l, s);
    }
    // Its 40 legs (4 per panel, 10 panels) with the library angle: solids
    // at level 0 only, 20 triangles each (6 sides, two ends of 4).
    let solids: Vec<&&Prim> = at.iter().filter(|p| p.filled).collect();
    assert_eq!(solids.len(), 1);
    assert_eq!(solids[0].levels, (0, 0));
    // Drawn by the solids distance, not by the structures' levels.
    assert!(solids[0].near);
    assert!(at.iter().filter(|p| !p.filled).all(|p| !p.near));
    assert_eq!(solids[0].points.len(), 40 * 20 * 3);
    // Within the tower: 2.5 m and the angle from the centre at most, from
    // the ground to the top of the legs (30 m).
    let foot = [t2.position[0], t2.position[1], t2.position[2]];
    for p in &solids[0].points {
        assert!((p[0] - foot[0]).hypot(p[1] - foot[1]) < 2.5 * 2f64.sqrt() + 0.1);
        assert!(p[2] > foot[2] - 0.1 && p[2] < foot[2] + 30.1);
    }
    // From afar the mast line (levels 1 and 2), as before.
    assert!(at.iter().any(|p| p.levels == (1, 2)));
    // T1 (type-01, level 1): drawn as before — the same primitives with no
    // body drawn at all. (T6's pole: below.)
    let mut bare = typed_in(&dir, true);
    bare.bodies = Default::default();
    let before = scene::prims(&bare, Layer::Structures, 0);
    for id in ["T1", "T3", "T5", "T7"] {
        let a: Vec<&Prim> = prims_at(&m, &prims, id);
        let b: Vec<&Prim> = prims_at(&bare, &before, id);
        assert_eq!(a, b, "{id}");
        assert!(a.iter().all(|p| !p.filled), "{id}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// T6 (type-05) names the example's `.ohlp` pole (700 mm at the ground
/// line, 300 mm at 27 m, wall 8 mm): drawn as a hollow tube by the solids
/// distance, its lines as before (the mast and arm of its stick drawing);
/// a click inside it on the screen selects T6; the drawings get its
/// outline.
#[test]
fn a_pole_at_level_3_is_drawn_with_its_outline() {
    let dir = scratch("members-pole");
    let m = typed_in(&dir, false);
    let prims = scene::prims(&m, Layer::Structures, 0);
    let t6 = m.line.structure("T6").unwrap().clone();
    let at = prims_at(&m, &prims, "T6");
    let solids: Vec<&&Prim> = at.iter().filter(|p| p.filled).collect();
    assert_eq!(solids.len(), 1);
    assert!(solids[0].near);
    // 4 × 32 triangles (outer and inner faces, the rings at both ends).
    assert_eq!(solids[0].points.len(), 128 * 3);
    // At T6, from its foot (raised by its height adjustment) up 27 m,
    // within 350 mm of its axis.
    let o = scene::type_origin(&t6);
    for p in &solids[0].points {
        assert!((p[0] - o[0]).hypot(p[1] - o[1]) <= 0.35 + 1e-9);
        assert!(p[2] >= o[2] - 1e-9 && p[2] <= o[2] + 27.0 + 1e-9);
    }
    // Its lines unchanged.
    let mut bare = typed_in(&dir, false);
    bare.bodies = Default::default();
    let before = scene::prims(&bare, Layer::Structures, 0);
    let lines: Vec<&Prim> = at.iter().copied().filter(|p| !p.filled).collect();
    assert_eq!(lines, prims_at(&bare, &before, "T6"));
    // A click inside the pole, away from its axis and lines: T6. Looking
    // from the side (x across, z up), 100 px per metre: 0.25 m from the
    // axis at 2 m up.
    let project = |p: [f64; 3]| Some([100.0 * (p[0] - o[0]), -100.0 * (p[2] - o[2])]);
    let inside = [25.0, -200.0];
    assert_eq!(
        crate::selection::pole_at(&m, inside, &project).as_deref(),
        Some("T6")
    );
    let snaps = crate::selection::Snaps::default();
    assert_eq!(
        crate::selection::pick_in_model(&m, 0, &snaps, inside, 3.0, project),
        crate::selection::Selection::Structure("T6".into())
    );
    // Outside it (0.5 m off the axis at 2 m): not the pole.
    assert_eq!(
        crate::selection::pole_at(&m, [50.0, -200.0], &project),
        None
    );
    // The drawings: the pole's two outlines along the branch route.
    let m = solved(m);
    let routes = m.line.alignment_views();
    let branch = routes.iter().find(|r| r.id == "branch").unwrap();
    let mut d = ohl_drawing::build(
        &m.line,
        branch,
        &m.shapes_shown(0),
        &m.cases[0].jumpers,
        None,
        &[],
        &ohl_drawing::Options::default(),
    )
    .unwrap();
    scene::set_drawn_members(&mut d, &m.line, &m.bodies);
    let mark = d.structures.iter().find(|s| s.id == "T6").unwrap();
    assert_eq!(mark.pole.len(), 2);
    assert!(mark.members.is_empty() && mark.has_body());
    let _ = std::fs::remove_dir_all(&dir);
}

/// The structures layer's chunks: the solids apart (built by the solids
/// distance), the lines in the levels.
#[test]
fn solids_are_chunked_apart_from_the_levels() {
    let dir = scratch("members-near");
    let m = typed_in(&dir, true);
    let index = crate::chunks::ChunkIndex::new(&m, Layer::Structures, 0);
    let row = crate::lod::LodRow::default_for(Layer::Structures);
    let near: Vec<_> = index.near_keys().collect();
    // T2 and T4 (profiled legs) and T6 (its pole).
    assert!(!near.is_empty());
    let (mut solid, mut level0) = (0, 0);
    for (tile, _) in index.keys() {
        level0 += index
            .build(&m, Layer::Structures, &tile, 0, &row)
            .counts()
            .2;
        solid += index.build_near(&m, &tile).counts().2;
    }
    assert_eq!(level0, 0, "no solid in the levels");
    // Two towers' 40 legs of 20 triangles, and the pole's 128.
    assert_eq!(solid, 2 * 40 * 20 + 128);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_click_on_a_member_selects_the_structure_and_the_member() {
    let dir = scratch("members-pick");
    let m = typed_in(&dir, false);
    let t2 = m.line.structure("T2").unwrap().clone();
    // An oblique view, metres to pixels ×100: the arm m171 picked at its
    // middle.
    let project = |p: [f64; 3]| Some([100.0 * (p[0] + 0.37 * p[1]), 100.0 * (p[2] + 0.21 * p[1])]);
    let members = scene::placed_members(&m.bodies, &t2);
    let (_, arm) = members
        .iter()
        .find(|(id, _)| id.as_deref() == Some("m171"))
        .unwrap();
    let mid = [0, 1, 2].map(|k| 0.5 * (arm[0][k] + arm[1][k]));
    let at = project(mid).unwrap();
    let snaps = crate::selection::Snaps::default();
    assert_eq!(
        crate::selection::pick_in_model(&m, 0, &snaps, at, 3.0, project),
        crate::selection::Selection::Structure("T2".into())
    );
    assert_eq!(
        crate::selection::picked_member(&m, "T2", at, 3.0, project).as_deref(),
        Some("m171")
    );
    // A structure at level 1 has no member to pick.
    let p1 = m.line.structure("T1").unwrap().position;
    let at1 = project([p1[0], p1[1], p1[2] + 5.0]).unwrap();
    assert_eq!(
        crate::selection::picked_member(&m, "T1", at1, 3.0, project),
        None
    );
    // The structure screen open on type-02 selects the member.
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(m, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(!r.select_member("T2", "m171"), "no structure screen open");
        r.edit_type("type-02");
    }
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.select_member("T2", "m171"));
    let page = &r.libraries[&crate::library::LibraryKind::StructureTypes];
    assert_eq!(
        page.workspace.selected,
        vec![crate::type_workspace::Handle::Member("m171".into())]
    );
    // Not for a structure of another type.
    assert!(!r.select_member("T1", "m171"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_drawings_draw_the_members_on_their_layer() {
    use ohl_drawing::model::{Item, Layer as Sheet};
    let dir = scratch("members-drawing");
    let m = solved(typed_in(&dir, false));
    let routes = m.line.alignment_views();
    let mut d = ohl_drawing::build(
        &m.line,
        &routes[0],
        &m.shapes_shown(0),
        &m.cases[0].jumpers,
        None,
        &[],
        &ohl_drawing::Options::default(),
    )
    .unwrap();
    let plain = d.clone();
    scene::set_drawn_members(&mut d, &m.line, &m.bodies);
    let mark = |id: &str| d.structures.iter().find(|s| s.id == id).unwrap();
    assert_eq!(mark("T2").members.len(), 178);
    // Level 1: no members, drawn as before.
    assert!(mark("T1").members.is_empty());
    assert_eq!(
        plain.structures.iter().find(|s| s.id == "T1"),
        Some(mark("T1"))
    );
    let sheet = ohl_drawing::drawing(&d, &ohl_drawing::Options::default());
    let mut on = 0;
    let mut stack = sheet.items.clone();
    while let Some(i) = stack.pop() {
        match i {
            Item::Clip(c) => stack.extend(c.items),
            Item::Path(p) if p.layer == Sheet::Members => on += 1,
            _ => {}
        }
    }
    // The members of every level-2/3 structure on the route, in plan and
    // profile.
    let members: usize = d.structures.iter().map(|s| s.members.len()).sum();
    assert!(members >= 178);
    assert_eq!(on, 2 * members);
    let dxf = ohl_drawing::dxf::write(&sheet);
    assert!(dxf.contains("OHL_MEMBERS"));
    assert!(sheet.svg().contains(r#"class="OHL_MEMBERS""#));
    let _ = std::fs::remove_dir_all(&dir);
}

fn solved(mut m: Model) -> Model {
    for k in 0..m.cases.len() {
        m.ensure_solved(k);
    }
    m
}

/// A line of 400 type-02 towers with profiled legs: the structures layer
/// built (lines and solids from the shared geometry) well within a lenient
/// budget, debug builds included.
#[test]
fn many_lattice_towers_stay_fast() {
    let dir = scratch("members-perf");
    let mut m = typed_in(&dir, true);
    let t2 = m.line.structure("T2").unwrap().clone();
    let mut line = m.line.clone();
    line.structures.clear();
    line.sections.clear();
    line.jumpers.clear();
    for k in 0..400 {
        let mut s = t2.clone();
        s.id = format!("X{k}");
        s.position = [400.0 * f64::from(k), 0.0, 500.0];
        s.strings.clear();
        line.structures.push(s);
    }
    m.line = line;
    m.cases.clear();
    let started = Instant::now();
    let prims = scene::prims(&m, Layer::Structures, 0);
    let took = started.elapsed();
    let triangles: usize = prims
        .iter()
        .filter(|p| p.filled)
        .map(|p| p.points.len() / 3)
        .sum();
    assert_eq!(triangles, 400 * 40 * 20);
    assert!(
        took.as_secs_f64() < 5.0,
        "400 towers built in {took:.1?} ({triangles} triangles)"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A line of `n` type-02 towers `spacing` m apart, every member a library
/// angle (178 solids of 20 triangles each per tower).
fn tower_line(dir: &Path, n: u32, spacing: f64) -> Model {
    let mut m = typed_with(dir, Some(ALL_ANGLES));
    let t2 = m.line.structure("T2").unwrap().clone();
    let mut line = m.line.clone();
    line.structures.clear();
    line.sections.clear();
    line.jumpers.clear();
    for k in 0..n {
        let mut s = t2.clone();
        s.id = format!("X{k}");
        s.position = [spacing * f64::from(k), 0.0, 500.0];
        s.strings.clear();
        line.structures.push(s);
    }
    m.line = line;
    m.cases.clear();
    m
}

/// The 3D view draws the solids within the solids distance only: 80 m from
/// a tower of a line of towers 400 m apart, the default (400 m) draws its
/// solids and its neighbours' at most; 0 draws none, the lines the same.
#[test]
fn the_3d_view_draws_solids_within_their_distance() {
    if !crate::bench::gpu() {
        return;
    }
    let dir = scratch("members-solids-3d");
    let looks = [crate::bench::Look::Near3d(80.0)];
    let with = |d: f64| {
        crate::bench::measure_with(tower_line(&dir, 21, 400.0), &looks, 1, |app| {
            app.lod.set_solids(d).unwrap();
        })[0]
            .drawn
    };
    let (none, near) = (with(0.0), with(crate::lod::DEFAULT_SOLIDS_DISTANCE));
    let per_tower = 178 * 20;
    let solids = near[2] - none[2];
    assert!(
        solids >= per_tower && solids <= 3 * per_tower && solids % per_tower == 0,
        "{none:?} {near:?}"
    );
    assert_eq!(none[0], near[0], "the lines drawn the same");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The solids distance's benchmark (docs/development/performance.md):
/// 400 towers 400 m apart, every member a library angle (1.42 million
/// triangles in all), the 3D view looking down the line from 80 m and
/// 600 m behind the middle tower, at 80 m and 600 m from it obliquely and
/// the whole line, for solids distances from 0 to 2 km.
///
/// ```bash
/// OHL_REQUIRE_GPU_TESTS=1 cargo test --release -p ohl-app solids_distance_benchmark -- --ignored --nocapture
/// ```
#[test]
#[ignore = "benchmark: run in release with --ignored --nocapture"]
fn solids_distance_benchmark() {
    use crate::bench::Look;
    if !crate::bench::gpu() {
        return;
    }
    let dir = scratch("members-solids-bench");
    let looks = [
        Look::Along3d(80.0),
        Look::Along3d(600.0),
        Look::Near3d(80.0),
        Look::Near3d(600.0),
        Look::Whole3d,
    ];
    for d in [0.0, 200.0, 400.0, 800.0, 1200.0, 2000.0] {
        let rows = crate::bench::measure_with(tower_line(&dir, 400, 400.0), &looks, 20, |app| {
            app.lod.set_solids(d).unwrap();
        });
        for r in &rows {
            let build: f64 = r
                .phases
                .iter()
                .filter(|(n, _)| *n == "3d chunk build")
                .map(|(_, ms)| ms)
                .sum();
            eprintln!(
                "solids {d:>6.0} m  {:<28} frame {:>7.2} ms (max {:>7.2}), {:>8} triangles, \
                 settled in {} frames, chunk build {build:.2} ms/frame",
                format!("{:?}", r.look),
                r.frame_ms,
                r.max_ms,
                r.drawn[2],
                r.settle_frames,
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The elevation options are kept with the drawing settings (settings
/// written before them read the defaults, odd values too), and the sheet
/// preview carries the elevations the sheets draw (the export's own
/// layout): every structure drawn along the route, none when off.
#[test]
fn elevation_settings_are_kept_and_the_preview_shows_them() {
    use crate::drawings::{DrawingSettings, DrawingsWindow};
    use ohl_drawing::{ElevationPlace, Elevations};
    let w = DrawingsWindow {
        elevations: Elevations {
            place: ElevationPlace::Below,
            offset_mm: 14.0,
            rotation_deg: 80.0,
        },
        ..DrawingsWindow::default()
    };
    let text = serde_json::to_string(&DrawingSettings::of(&w, false)).unwrap();
    let back: DrawingSettings = serde_json::from_str(&text).unwrap();
    let mut again = DrawingsWindow::default();
    back.apply(&mut again);
    assert_eq!(again.elevations, w.elevations);
    let old: DrawingSettings = serde_json::from_str(r#"{"page":"A1"}"#).unwrap();
    let mut from_old = DrawingsWindow::default();
    old.apply(&mut from_old);
    assert_eq!(from_old.elevations, Elevations::default());
    let odd = DrawingSettings {
        elevations: "sideways".into(),
        elevation_offset: -3.0,
        elevation_rotation: 999.0,
        ..DrawingSettings::default()
    };
    let mut from_odd = DrawingsWindow::default();
    odd.apply(&mut from_odd);
    assert_eq!(from_odd.elevations, Elevations::default());
    // The preview (A3 sheets).
    let dir = scratch("members-elevations-preview");
    let m = solved(typed_in(&dir, false));
    let mut w = DrawingsWindow::default();
    let off = crate::drawings::preview(&m, 0, &w).unwrap();
    assert!(off.routes.iter().all(|r| r.elevations.is_empty()));
    w.elevations.place = ElevationPlace::Above;
    let p = crate::drawings::preview(&m, 0, &w).unwrap();
    let main = p.route("main").unwrap();
    let t2 = main.elevations.iter().find(|e| e.id == "T2").unwrap();
    assert_eq!(t2.lines.len(), 178);
    assert!(t2.leader.is_some());
    // Above the line: left of it (negative offsets), the base 10 mm on
    // paper away (20 m at 1:2000).
    let base = t2.leader.unwrap()[1];
    assert!((base[1] + 20.0).abs() < 1e-9, "{base:?}");
    assert!(t2.lines.iter().flat_map(|(l, _)| l).all(|q| q[1] < 0.0));
    let _ = std::fs::remove_dir_all(&dir);
}
