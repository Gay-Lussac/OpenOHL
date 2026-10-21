// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks › Safety volumes: run on the example line, drawn in the 3D view
//! and the plan while the window is open, saved in the performed checks
//! and read back, out of date after an edit of the line, exported.

use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use crate::app::App;
use crate::lidar_tests::harness;

/// Steps until `done` (600 s at most: a debug build on a loaded computer;
/// the swing arc's band, 2026-10-21, meshes more surfaces).
fn until(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&mut App) -> bool) {
    let started = Instant::now();
    loop {
        h.step();
        if done(h.state_mut()) {
            h.step();
            return;
        }
        assert!(started.elapsed() < Duration::from_secs(600), "{what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn png(h: &mut Harness<'_, App>, name: &str) {
    if let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG")
        && let Ok(image) = h.render()
    {
        let path = std::path::PathBuf::from(dir).join(name);
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
}

/// The example line in high wind with its swing arc, 4 m: three volumes
/// (one per wire), drawn while the window is open and taken off when it
/// closes; saved and read back (meshes to the millimetre); out of date
/// once the line changes; exported as DXF, IFC and (with a coordinate
/// system) KMZ.
#[test]
fn safety_volumes_are_built_drawn_saved_and_exported() {
    let model = crate::tests::synthetic_project("safety-volume");
    let gpu =
        ohl_render::headless_device().is_ok() && ohl_core::env::var_os("OHL_RENDER_PNG").is_some();
    let mut h = harness(model, gpu);
    h.run_steps(3);
    {
        let r = h.state_mut().ready().unwrap();
        let wind = r
            .model
            .cases
            .iter()
            .position(|c| c.case.name == "high wind")
            .unwrap();
        r.safety_volume.show(wind);
        let s = &mut r.safety_volume.settings;
        // No distance is built in.
        assert_eq!(s.distance, ohl_clearance::rules::Requirement::Fixed(0.0));
        s.distance = ohl_clearance::rules::Requirement::Fixed(4.0);
        s.wind = ohl_clearance::safety_volume::WindMode::Arc;
        s.tolerance = 0.2;
    }
    h.run_steps(3);
    h.get_by_label_contains("No safety distance is built in");
    h.get_by_label("Run").click();
    until(&mut h, "a safety-volume run", |a| {
        matches!(a.ready().unwrap().safety_volume.result(), (Some(_), false))
    });
    h.run_steps(3);
    let triangles = {
        let r = h.state_mut().ready().unwrap();
        let rep = match r.safety_volume.result().0 {
            Some(Ok(rep)) => rep.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(rep.volumes.volumes.len(), 3);
        for v in &rep.volumes.volumes {
            assert_eq!(v.mesh.open_edges(), 0);
            assert!(v.simplified.checked);
        }
        let drawn = r
            .view3d
            .safety_volume
            .volumes
            .as_ref()
            .expect("drawn in 3D");
        assert_eq!(drawn.volumes.len(), 3);
        assert!(r.plan.safety_volume.outlines.iter().all(|o| !o.is_empty()));
        assert!(!r.view3d.safety_volume.stale);
        rep.volumes
            .volumes
            .iter()
            .map(|v| v.mesh.triangles.len())
            .sum::<usize>()
    };
    h.get_by_label_contains("volumes from");
    png(&mut h, "safety-volume-window-3d.png");
    // The 3D layer alone with the line, headless: in perspective and from
    // above (the outline the plan draws).
    if let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG")
        && ohl_render::headless_device().is_ok()
    {
        let r = h.state_mut().ready().unwrap();
        let sel = crate::selection::Selection::None;
        let mut scenes: Vec<ohl_render::Scene> = [
            crate::scene::Layer::Ground,
            crate::scene::Layer::Structures,
            crate::scene::Layer::Wires,
        ]
        .into_iter()
        .map(|l| crate::scene::layer(&r.model, l, 0, &sel))
        .collect();
        scenes.push(crate::safety_volume::scene(
            r.model.origin,
            &r.view3d.safety_volume,
        ));
        let bounds = scenes.last().unwrap().bounds().unwrap();
        let refs: Vec<&ohl_render::Scene> = scenes.iter().collect();
        for (name, pitch, zoom) in [
            ("safety-volume-3d.png", 25.0, 0.45),
            ("safety-volume-from-above.png", 89.0, 0.75),
        ] {
            let mut camera = ohl_render::Camera::fit(&bounds);
            camera.pitch = f64::to_radians(pitch);
            camera.distance *= zoom;
            let rgba = ohl_render::render_rgba(&refs, &camera, 1600, 1000).unwrap();
            crate::write_png(
                &std::path::PathBuf::from(&dir).join(name),
                1600,
                1000,
                &rgba,
            )
            .unwrap();
        }
    }
    // The plan: the outlines.
    {
        let r = h.state_mut().ready().unwrap();
        r.safety_volume.open = false;
    }
    h.run_steps(2);
    {
        let r = h.state_mut().ready().unwrap();
        assert!(
            r.view3d.safety_volume.volumes.is_none(),
            "taken off when closed"
        );
        r.safety_volume.open = true;
    }
    h.run_steps(2);
    assert!(
        h.state_mut()
            .ready()
            .unwrap()
            .view3d
            .safety_volume
            .volumes
            .is_some()
    );
    // Saved, read back.
    h.state_mut()
        .ready()
        .unwrap()
        .safety_volume
        .set_save_name("work 1");
    h.run_steps(2);
    h.get_by_label("Save in Performed checks").click_accesskit();
    h.run_steps(3);
    let saved = {
        let r = h.state_mut().ready().unwrap();
        r.model.session().unwrap().report("work 1").expect("saved")
    };
    assert_eq!(saved.check, crate::safety_volume::CHECK_ID);
    {
        let r = h.state_mut().ready().unwrap();
        let before = r.safety_volume.report().unwrap().clone();
        r.safety_volume = Default::default();
        r.check_library_out(crate::check_library::Out {
            open: Some(saved.clone()),
            ..Default::default()
        });
        let back = r.safety_volume.report().expect("read back").clone();
        assert_eq!(back.settings, before.settings);
        assert_eq!(back.volumes.cases, before.volumes.cases);
        for (a, b) in back.volumes.volumes.iter().zip(&before.volumes.volumes) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.mesh.triangles, b.mesh.triangles);
            for (p, q) in a.mesh.vertices.iter().zip(&b.mesh.vertices) {
                for k in 0..3 {
                    assert!((p[k] - q[k]).abs() <= 0.0005);
                }
            }
        }
        assert_eq!(r.safety_volume.out_of_date(&r.model), None);
    }
    h.run_steps(3);
    // Exported.
    let dir = std::env::temp_dir().join(format!("ohl-sv-export-{}", std::process::id()));
    {
        let r = h.state_mut().ready().unwrap();
        let model = &r.model;
        let text = r.safety_volume.export_to(model, &dir).unwrap();
        assert!(text.contains(".dxf") && text.contains(".ifc"), "{text}");
        // The example line's system is informative: no .prj, KMZ refused.
        assert!(
            text.contains("no .prj") && text.contains("no KMZ"),
            "{text}"
        );
    }
    let dxf = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .find(|e| e.path().extension().is_some_and(|x| x == "dxf"))
        .expect("a DXF");
    let read = ohl_background::dxf::read_dxf(&std::fs::read(dxf.path()).unwrap()).unwrap();
    assert_eq!(read.skipped.get("3DFACE"), Some(&triangles));
    std::fs::remove_dir_all(&dir).ok();
    // The line changed: out of date, dimmed, Run again offered.
    {
        let r = h.state_mut().ready().unwrap();
        let mut line = r.model.line.clone();
        line.structures[1].position[2] += 1.0;
        assert!(r.perform(crate::edit::Action::Command {
            what: "raised a structure".into(),
            command: ohl_session::Command::ReplaceLine {
                line: Box::new(line)
            },
        }));
    }
    h.run_steps(3);
    {
        let r = h.state_mut().ready().unwrap();
        assert_eq!(
            r.safety_volume.out_of_date(&r.model).as_deref(),
            Some("the line")
        );
        assert!(r.view3d.safety_volume.stale);
    }
    h.get_by_label("Run again");
    png(&mut h, "safety-volume-out-of-date.png");
}

/// Chosen by system, then by span (2026-10-21): the toggle shows the
/// line's systems with their voltage; None ticked refuses to run; the
/// system ticked and one volume per system give the example's one system
/// (both circuits) as one volume, named after it; the run records the
/// choice and reads it back; a run saved before reads as every circuit.
/// By span, the search and All tick the span found only, and the run
/// takes that span alone.
#[test]
fn safety_volumes_by_system() {
    use crate::safety_volume::{Scope, Settings};
    use ohl_clearance::safety_volume::Grouping;
    let model = crate::tests::synthetic_project("safety-volume-system");
    let mut h = harness(model, false);
    h.run_steps(3);
    {
        let r = h.state_mut().ready().unwrap();
        let hot = r
            .model
            .cases
            .iter()
            .position(|c| c.case.name == "max temperature")
            .unwrap();
        r.safety_volume.show(hot);
        let s = &mut r.safety_volume.settings;
        s.distance = ohl_clearance::rules::Requirement::Fixed(3.0);
        s.tolerance = 0.3;
    }
    h.run_steps(3);
    // The groups and their user-guide links.
    for title in [
        "Wire positions",
        "Distance",
        "What is included",
        "Mesh",
        "Export",
    ] {
        h.get_by_label(&format!("{title}: user guide ↗"));
    }
    h.get_by_label("Systems").click_accesskit();
    h.run_steps(2);
    h.get_by_label("220 kV example system — 220 kV");
    // The wires without a system, a row of their own (2026-10-21); the
    // example line has none.
    h.get_by_label("(no system) — earthed wires (none on this line)");
    h.get_by_label("None").click_accesskit();
    h.run_steps(2);
    h.get_by_label("Run").click();
    h.run_steps(2);
    match h.state_mut().ready().unwrap().safety_volume.result().0 {
        Some(Err(e)) => assert!(e.contains("tick a system"), "{e}"),
        other => panic!("{other:?}"),
    }
    h.get_by_label("220 kV example system — 220 kV")
        .click_accesskit();
    h.run_steps(2);
    h.get_by_label("system").click_accesskit();
    h.run_steps(2);
    {
        let s = &h.state_mut().ready().unwrap().safety_volume.settings;
        assert_eq!(s.scope, Scope::Systems);
        assert_eq!(s.systems.as_deref(), Some(&["ex220".to_owned()][..]));
        assert_eq!(s.grouping, Grouping::System);
    }
    h.get_by_label("Run").click();
    until(&mut h, "a run by system", |a| {
        matches!(a.ready().unwrap().safety_volume.result(), (Some(_), false))
    });
    let settings = {
        let r = h.state_mut().ready().unwrap();
        let rep = match r.safety_volume.result().0 {
            Some(Ok(rep)) => rep.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(rep.volumes.volumes.len(), 1);
        let v = &rep.volumes.volumes[0];
        assert_eq!(v.name, "220 kV example system");
        // Both circuits' sections: every section of the line, and its
        // jumpers (`jumper:<id>`, part of the volumes since 2026-10-21).
        let (jumpers, sections): (Vec<_>, Vec<_>) =
            v.sections.iter().partition(|s| s.starts_with("jumper:"));
        assert_eq!(sections.len(), r.model.line.sections.len());
        assert_eq!(jumpers.len(), r.model.line.jumpers.len());
        rep.settings
    };
    // Recorded with the run, read back.
    let saved = settings.to_json(&["max temperature".into()]);
    assert_eq!(saved["scope"], "systems");
    assert_eq!(saved["per"], "system");
    assert_eq!(saved["systems"][0], "ex220");
    let (back, cases) = Settings::from_json(&saved).unwrap();
    assert_eq!(back, settings);
    assert_eq!(cases, ["max temperature"]);
    // The band's margin recorded (2026-10-21); a run saved before reads
    // the margin then fixed; another margin with the arc: out of date.
    assert_eq!(saved["band_margin"], 0.15);
    let mut old = saved.clone();
    old.as_object_mut().unwrap().remove("band_margin");
    old.as_object_mut().unwrap().remove("jumper_band_margin");
    let (back, _) = Settings::from_json(&old).unwrap();
    assert_eq!(
        back.steps.band_margin,
        ohl_clearance::positions::BAND_MARGIN
    );
    // The jumpers' own margin (2026-10-21): recorded, default 0; a run
    // saved before it read as its jumpers were then (the spans' margin).
    assert_eq!(saved["jumper_band_margin"], 0.0);
    assert_eq!(
        back.steps.jumper_band_margin,
        ohl_clearance::positions::BAND_MARGIN
    );
    let mut arc = settings.clone();
    arc.wind = ohl_clearance::safety_volume::WindMode::Arc;
    let mut now = arc.clone();
    now.steps.band_margin = 0.3;
    assert_eq!(now.changed_since(&arc).as_deref(), Some("the band margin"));
    assert_eq!(now.changed_since(&settings), None, "no arc: no band");
    let mut jumper = arc.clone();
    jumper.steps.jumper_band_margin = 0.35;
    assert_eq!(
        jumper.changed_since(&arc).as_deref(),
        Some("the jumper band margin")
    );
    now.steps.jumper_band_margin = 0.35;
    assert_eq!(now.changed_since(&arc).as_deref(), Some("the band margins"));
    assert_eq!(
        arc.wind_text(),
        "swing arc (band margin 0.15 m, jumpers 0.00 m)"
    );
    // A run saved before: by circuit, every circuit.
    let mut old = saved.clone();
    old.as_object_mut().unwrap().remove("scope");
    old.as_object_mut().unwrap().remove("systems");
    old["circuits"] = serde_json::json!([]);
    let (back, _) = Settings::from_json(&old).unwrap();
    assert_eq!(
        (back.scope, back.circuits, back.systems),
        (Scope::Circuits, None, None)
    );
    // By span: the search finds S1's first span, All ticks it alone; the
    // run takes that span's wire only (one volume, of S1), and records it.
    h.get_by_label("Spans").click_accesskit();
    h.run_steps(2);
    h.get_by_label("10 of 10 ticked");
    h.get_by_label("None").click_accesskit();
    h.run_steps(2);
    h.get_by_role_and_label(eframe::egui::accesskit::Role::TextInput, "search")
        .focus();
    h.run_steps(1);
    h.get_by_role_and_label(eframe::egui::accesskit::Role::TextInput, "search")
        .type_text("s1 SPAN 1:");
    h.run_steps(2);
    h.get_by_label("0 of 10 ticked (1 shown)");
    h.get_by_label("All").click_accesskit();
    h.run_steps(2);
    h.get_by_label("wire").click_accesskit();
    h.run_steps(2);
    assert_eq!(
        h.state_mut().ready().unwrap().safety_volume.settings.spans,
        Some(vec!["S1:1".to_owned()])
    );
    h.get_by_label("Run").click();
    until(&mut h, "a run by span", |a| {
        matches!(a.ready().unwrap().safety_volume.result(), (Some(_), false))
    });
    let r = h.state_mut().ready().unwrap();
    let rep = match r.safety_volume.result().0 {
        Some(Ok(rep)) => rep.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(rep.volumes.volumes.len(), 1);
    assert_eq!(rep.volumes.volumes[0].sections, ["S1"]);
    assert_eq!(rep.volumes.volumes[0].positions, 1);
    let saved = rep.settings.to_json(&[]);
    assert_eq!(saved["scope"], "spans");
    assert_eq!(saved["spans"][0], "S1:1");
}
