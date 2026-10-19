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

/// The typed example in a folder of its own (`dir`), with a section
/// library holding a profile for type-02's `leg` section when `profile`.
fn typed_in(dir: &Path, profile: bool) -> Model {
    let ex = root().join("examples/synthetic-line");
    let structures = dir.join("Structures");
    std::fs::create_dir_all(&structures).unwrap();
    for e in std::fs::read_dir(ex.join("Structures")).unwrap() {
        let p = e.unwrap().path();
        std::fs::copy(&p, structures.join(p.file_name().unwrap())).unwrap();
    }
    let file = dir.join("synthetic-line-typed.ohl.json");
    std::fs::copy(ex.join("synthetic-line-typed.ohl.json"), &file).unwrap();
    if profile {
        // An invented angle 100 × 10 for the legs (the members' section
        // is looked up in the section library first).
        std::fs::create_dir_all(dir.join("Sections")).unwrap();
        std::fs::write(
            dir.join("Sections/legs.ohlsec"),
            "format = \"ohl-sections\"\nversion = 1\nname = \"t\"\n\
             default_material = \"s\"\n\
             [[material]]\nid = \"s\"\nelasticity = \"210 GPa\"\ndensity = \"7850 kg/m3\"\n\
             yield = \"355 MPa\"\n\
             [[section]]\nid = \"leg\"\nshape = \"equal_angle\"\nh = \"100 mm\"\n\
             t = \"10 mm\"\nr = \"0 mm\"\n",
        )
        .unwrap();
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
    // body drawn at all.
    let mut bare = typed_in(&dir, true);
    bare.bodies = Default::default();
    let before = scene::prims(&bare, Layer::Structures, 0);
    for id in ["T1", "T3", "T5", "T6", "T7"] {
        let a: Vec<&Prim> = prims_at(&m, &prims, id);
        let b: Vec<&Prim> = prims_at(&bare, &before, id);
        assert_eq!(a, b, "{id}");
        assert!(a.iter().all(|p| !p.filled), "{id}");
    }
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
