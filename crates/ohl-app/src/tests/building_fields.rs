// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: Checks › Electrical › Buildings (fields) (owner, 2026-11-02,
//! 2026-11-03): the test plugin's presets found from the window (no field
//! run there), the buildings listed, Set up the field tools handing the
//! cuts and places to the Magnetic field and Electric field windows with
//! the field plan's limits; a plan made with New and saved.

use super::*;

/// The test plugin with field presets (`plugins/tests/field-presets`),
/// put in use; false when it is not built (and plugin tests are not
/// required).
fn test_plugin(h: &mut Harness<'_, App>) -> bool {
    let dir = root().join("plugins/tests/field-presets");
    let built = dir
        .join("target/wasm32-wasip2/release/field_presets.wasm")
        .is_file();
    assert!(
        built || ohl_core::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
        "the field-presets test plugin is not built"
    );
    if !built {
        return false;
    }
    let app = h.state_mut();
    app.plugins = crate::plugins::Plugins::load(None, None);
    app.plugins.folders = vec![dir];
    app.plugins.refresh();
    h.run();
    true
}

/// Waits for the window's run.
fn wait(h: &mut Harness<'_, App>) {
    let started = std::time::Instant::now();
    loop {
        h.step();
        let r = h.state_mut().ready().unwrap();
        if r.building_fields.job.result.is_some() {
            break;
        }
        assert!(started.elapsed().as_secs() < 180, "no run in 180 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    h.run();
}

#[test]
fn buildings_found_and_handed_to_the_field_windows() {
    let dir = std::env::temp_dir().join(format!("ohl-building-fields-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ex = root().join("examples/electrical");
    for f in ["double-circuit.ohl.json", "wires.ohlw"] {
        std::fs::copy(ex.join(f), dir.join(f)).unwrap();
    }
    let model = solved_all(
        model::load(
            Sources {
                file: dir.join("double-circuit.ohl.json"),
                library: Some(dir.join("wires.ohlw")),
                ..Sources::default()
            },
            &|_| {},
        )
        .unwrap(),
    );
    let mut h = Harness::builder()
        .with_size(egui::vec2(1600.0, 1000.0))
        .build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    h.run();
    if !test_plugin(&mut h) {
        return;
    }
    let r = h.state_mut().ready().unwrap();
    r.building_fields.show(r.case);
    h.run();
    h.get_by_label("Buildings (fields)");
    h.get_by_label("Plugin");
    h.get_by_label("Parameters");
    // A field plan: New, saved (beside the line file), chosen.
    h.get_by_label("New").click();
    h.run();
    h.get_by_label("Save into the project").click();
    h.run();
    h.run();
    assert!(dir.join("field-plans.ohlfp").is_file());
    run_in(&h, "Buildings (fields)").click();
    wait(&mut h);
    {
        let r = h.state_mut().ready().unwrap();
        let found = r.building_fields.found().0.unwrap().as_ref().unwrap();
        assert_eq!(found.presets[0].name, "default");
        // Nothing handed over before asked.
        assert!(r.magnetic_field.buildings().is_none());
    }
    h.get_by_label("Set up the field tools").click();
    h.run();
    h.run();
    {
        let r = h.state_mut().ready().unwrap();
        for p in [r.magnetic_field.buildings(), r.electric_field.buildings()] {
            let p = p.expect("the preset handed over");
            let names: Vec<&str> = p.places.iter().map(|q| q.name.as_str()).collect();
            assert_eq!(names, ["house-A", "house-B"]);
            assert!(!p.cuts.is_empty());
        }
        assert!(r.magnetic_field.open && r.electric_field.open);
        // The template's limits (the NISV's, as an example).
        assert_eq!(r.magnetic_field.limit(), Some(1.0));
        assert_eq!(r.electric_field.limit(), Some(5.0));
    }
    assert_eq!(h.query_all_by_label_contains("default: ").count(), 2);
    if let Some(d) = ohl_core::env::var_os("OHL_FIELD_SCREENSHOTS") {
        h.render()
            .unwrap()
            .save(std::path::Path::new(&d).join("buildings-fields-results.png"))
            .unwrap();
    }
    let _ = std::fs::remove_dir_all(&dir);
}
