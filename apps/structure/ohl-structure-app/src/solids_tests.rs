// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The 3D view draws the members' steel as the Detail view does (owner
//! 2026-11-05): the same corners at each member end, picked and coloured
//! as before; worked out once per revision. With `OHL_RENDER_PNG` set,
//! images of the 3D view and the Detail view of the same joint.

use egui_kittest::Harness;
use ohl_structure_edit::Handle;

use crate::app::StructureApp;
use crate::joint_ui_tests::ANGLES;
use crate::tests::{copy_of, harness};

fn library() -> ohl_structure_link::SectionLibrary {
    ohl_structure_link::section_library(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/sections/example-sections.ohlsec"),
    )
    .unwrap()
}

/// At joint N2 of the angle tower, every member end the close-up draws
/// has its steel's corners (where it is cut) among the 3D solid's end
/// corners, within 1e-9 m — the close-up's cap and the solid's end are
/// the same polygon.
#[test]
fn the_3d_solids_end_where_the_close_up_draws_them() {
    let text = crate::close_up::timing::heavy();
    let d = ohl_structure_link::joint_details(&text, &library(), &Default::default()).unwrap();
    let solids = ohl_structure_link::member_solids(&d);
    let j = d.joint("N2").unwrap();
    // Uncut pieces: the close-up's own faces.
    let pieces = crate::close_up::pieces_with(j, false, f64::INFINITY, f64::INFINITY);
    let corners: Vec<[f64; 3]> = pieces
        .iter()
        .filter(|p| p.bolt.is_none())
        .flat_map(|p| p.p)
        .map(|q| [0, 1, 2].map(|k| q[k] + j.position[k]))
        .collect();
    let mut checked = 0;
    for a in j.arrivals.iter().filter(|a| a.profile.is_some()) {
        let s = solids.iter().find(|s| s.member == a.member).unwrap();
        let ring = &s.ends[a.end];
        for c in ring {
            assert!(
                corners
                    .iter()
                    .any(|q| (0..3).all(|k| (q[k] - c[k]).abs() < 1e-9)),
                "{} at N2: corner {c:?} not in the close-up",
                a.member
            );
            checked += 1;
        }
        // Seated where the close-up seats it.
        assert!((s.cut[a.end] - a.cut.value.unwrap_or(0.0)).abs() < 1e-12);
    }
    assert!(checked >= 4 * 6, "{checked} corners");
}

/// The scene draws each solid's triangles; a solid whose work line has
/// moved (the file changed since) is left out, its member a stick.
#[test]
fn the_scene_draws_the_solids_that_fit() {
    let d = ohl_structure_link::joint_details(ANGLES, &library(), &Default::default()).unwrap();
    let solids = ohl_structure_link::member_solids(&d);
    assert_eq!(solids.len(), 5);
    let t = <ohl_structures::StructureType as ohl_structures::LibraryItem>::parse(
        std::path::Path::new("a.ohls"),
        ANGLES,
    )
    .unwrap();
    let g = ohl_structure_edit::geometry(&t, &Default::default());
    let count =
        |solids: &[ohl_structure_link::MemberSolid]| crate::view::scene_for_tests(&g, solids, &[]);
    let (lines0, _, tris0) = count(&[]);
    let (lines, _, tris) = count(&solids);
    let want: usize = solids.iter().map(|s| s.triangle_count()).sum();
    assert_eq!(tris - tris0, want);
    assert!(
        lines > lines0,
        "each member's work line still drawn, and the solids' edges"
    );
    // One moved: a stick again.
    let mut moved = solids.clone();
    moved[0].work[1][2] += 0.5;
    let (_, _, tris) = count(&moved);
    assert_eq!(tris - tris0, want - solids[0].triangle_count());
}

/// The angle tower of the joint tests in a folder with the example
/// sections.
fn angle_tower(tag: &str, text: &str) -> std::path::PathBuf {
    let path = copy_of("type-02.ohls", tag);
    let dir = path.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(dir.join("Sections")).unwrap();
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../libraries/sections/example-sections.ohlsec"),
        dir.join("Sections/example-sections.ohlsec"),
    )
    .unwrap();
    let tower = dir.join("angles.ohls");
    std::fs::write(&tower, text).unwrap();
    tower
}

/// Saves the window as `name` in `OHL_RENDER_PNG` (a GPU rendering).
fn shot(h: &mut Harness<'static, StructureApp>, name: &str) {
    if ohl_render::headless_device().is_ok()
        && let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG")
    {
        let image = h.render().unwrap();
        image.save(std::path::Path::new(&dir).join(name)).unwrap();
    }
}

/// The 3D view of the angle tower: its members' solids worked out once
/// for the revision (not again on a pick); the whole tower, the Detail
/// view of N2 and the 3D view near N2 seen from the same side at the same
/// scale (images with `OHL_RENDER_PNG`).
#[test]
fn the_3d_view_draws_the_profiles_as_placed() {
    let tower = angle_tower("solids-view", ANGLES);
    let mut h = harness(&tower);
    let ctx = h.ctx.clone();
    let solids = h.state_mut().wait_for_solids(&ctx);
    assert_eq!(solids.len(), 5, "every member a solid");
    // Its bolts with them (2026-11-05): D1's three at N2 and the
    // member's two at each of its other ends.
    assert!(!h.state().bolt_solids().is_empty(), "the bolts too");
    let generation = h.state().solids.generation;
    h.state_mut().selected = vec![Handle::Member("D1".into())];
    h.run();
    h.run();
    assert_eq!(
        h.state().solids.generation,
        generation,
        "a pick does not work them out again"
    );
    if h.state().view_camera().is_none() {
        std::fs::remove_dir_all(tower.parent().unwrap()).ok();
        return; // no GPU: no view
    }
    h.state_mut().selected.clear();
    // The whole tower.
    h.state_mut().set_view_camera(ohl_render::Camera {
        target: [2.0, 1.0, 5.0],
        distance: 14.0,
        yaw: (-60f64).to_radians(),
        pitch: 20f64.to_radians(),
        fov_y: 45f64.to_radians(),
        exaggeration: 1.0,
        ortho: false,
    });
    h.run_steps(3);
    shot(&mut h, "solids-3d-tower.png");
    // The Detail view of N2, turned (yaw 0.6, tilted 0.35 + 0.2).
    let (yaw, pitch) = (0.6_f32, 0.2_f32);
    h.state_mut().open_in_detail(Handle::Joint("N2".into()));
    h.state_mut().focus(crate::layout::Panel::JointView);
    h.state_mut().joints.yaw = yaw;
    h.state_mut().joints.pitch = pitch;
    h.state_mut().joints.zoom = 0.0;
    h.run_steps(4);
    shot(&mut h, "solids-detail-N2.png");
    // Its scale on the screen, points per metre.
    let d = h.state_mut().joint_details().unwrap();
    let d = d.as_ref().as_ref().unwrap();
    let j = d.joint("N2").unwrap();
    let rect = h.state().joints.view_rect.expect("the close-up drawn");
    let k = f64::from(rect.width().min(rect.height())) / (2.4 * ohl_structure_link::reach(j));
    // The 3D view on N2 from the same side at the same scale (parallel).
    h.state_mut().focus(crate::layout::Panel::View3d);
    h.run_steps(2);
    let view = h.state().view_rect().expect("the 3D view drawn");
    let fov = 45f64.to_radians();
    h.state_mut().set_view_camera(ohl_render::Camera {
        target: j.position,
        distance: f64::from(view.height()) / (2.0 * (0.5 * fov).tan() * k),
        yaw: f64::from(yaw) - std::f64::consts::FRAC_PI_2,
        pitch: f64::from(pitch) + 0.35,
        fov_y: fov,
        exaggeration: 1.0,
        ortho: true,
    });
    h.run_steps(3);
    shot(&mut h, "solids-3d-N2.png");
    std::fs::remove_dir_all(tower.parent().unwrap()).ok();
}

/// The 3D view of the example structure type-02 (sections of values
/// only: squares of their area, as the Detail view draws them) and of
/// every profile of the example library (examples/exports).
#[test]
fn the_examples_in_3d() {
    let path = copy_of("type-02.ohls", "solids-type-02");
    let mut h = harness(&path);
    let ctx = h.ctx.clone();
    let solids = h.state_mut().wait_for_solids(&ctx);
    let members = h.state().file.as_ref().unwrap().structure().members.len();
    assert_eq!(solids.len(), members);
    assert!(solids.iter().all(|s| !s.shaped), "values only");
    h.run_steps(3);
    shot(&mut h, "solids-type-02.png");
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
    // Every profile.
    let dir =
        std::env::temp_dir().join(format!("ohl-structure-solids-every-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("Sections")).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    std::fs::copy(
        root.join("libraries/sections/example-sections.ohlsec"),
        dir.join("Sections/example-sections.ohlsec"),
    )
    .unwrap();
    let file = dir.join("every-profile.ohls");
    std::fs::copy(root.join("examples/exports/every-profile.ohls"), &file).unwrap();
    let mut h = harness(&file);
    let ctx = h.ctx.clone();
    let solids = h.state_mut().wait_for_solids(&ctx);
    assert_eq!(solids.len(), 10, "every member a solid");
    if h.state().view_camera().is_some() {
        h.state_mut().set_view_camera(ohl_render::Camera {
            target: [2.0, 1.5, 3.0],
            distance: 11.0,
            yaw: (-70f64).to_radians(),
            pitch: 25f64.to_radians(),
            fov_y: 45f64.to_radians(),
            exaggeration: 1.0,
            ortho: false,
        });
        h.run_steps(3);
        shot(&mut h, "solids-every-profile.png");
    }
    std::fs::remove_dir_all(dir).ok();
}

/// A large tower stays interactive, run by hand: `OHL_SOLIDS_FILE` (a
/// structure file) with its section library in a `Sections` folder
/// beside it. Times the solids once and the 3D view's frames after
/// (picks, turns).
#[test]
#[ignore = "timing, run by hand"]
fn large_tower_frames() {
    let Some(file) = ohl_core::env::var_os("OHL_SOLIDS_FILE") else {
        return;
    };
    let mut h = harness(std::path::Path::new(&file));
    let ctx = h.ctx.clone();
    let t0 = std::time::Instant::now();
    let solids = h.state_mut().wait_for_solids(&ctx);
    let tris: usize = solids.iter().map(|s| s.triangle_count()).sum();
    eprintln!(
        "{} solids, {tris} triangles, worked out in {:.0} ms",
        solids.len(),
        t0.elapsed().as_secs_f64() * 1e3
    );
    let bolts = h.state().bolt_solids();
    let bolt_tris: usize = bolts
        .iter()
        .flat_map(|b| &b.faces)
        .map(|f| f.triangles.len())
        .sum();
    eprintln!("{} bolts, {bolt_tris} triangles", bolts.len());
    h.run();
    let ids: Vec<String> = solids.iter().take(10).map(|s| s.member.clone()).collect();
    let t0 = std::time::Instant::now();
    for id in &ids {
        h.state_mut().selected = vec![Handle::Member(id.clone())];
        h.step();
    }
    eprintln!(
        "a pick (the scene built again): {:.1} ms per frame",
        t0.elapsed().as_secs_f64() * 1e3 / ids.len() as f64
    );
    let t0 = std::time::Instant::now();
    let n = 20;
    for k in 0..n {
        if let Some(mut c) = h.state().view_camera() {
            c.yaw += 0.05 * f64::from(k);
            h.state_mut().set_view_camera(c);
        }
        h.step();
    }
    eprintln!(
        "a turn: {:.1} ms per frame",
        t0.elapsed().as_secs_f64() * 1e3 / f64::from(n)
    );
    // The same without the profiles (sticks only, as before).
    h.state_mut().profiles = false;
    h.run();
    let t0 = std::time::Instant::now();
    for id in &ids {
        h.state_mut().selected = vec![Handle::Member(id.clone())];
        h.step();
    }
    eprintln!(
        "sticks only: a pick {:.1} ms per frame",
        t0.elapsed().as_secs_f64() * 1e3 / ids.len() as f64
    );
    h.state_mut().profiles = true;
    h.run();
    shot(&mut h, "solids-large-tower.png");
    // Near a joint of its legs: the profiles with their edges.
    if let Some(s) = solids.first() {
        h.state_mut().set_view_camera(ohl_render::Camera {
            target: s.work[1],
            distance: 3.0,
            yaw: (-60f64).to_radians(),
            pitch: 20f64.to_radians(),
            fov_y: 45f64.to_radians(),
            exaggeration: 1.0,
            ortho: false,
        });
        h.run();
        shot(&mut h, "solids-large-tower-near.png");
    }
}
