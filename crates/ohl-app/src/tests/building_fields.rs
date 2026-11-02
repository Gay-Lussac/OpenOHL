// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: Checks › Electrical › Buildings (fields) (owner, 2026-11-02):
//! the test plugin's presets run from the window, both fields at its
//! places, its report written one cut at a time into the folder; a field
//! plan template made from the window's New, saved beside the line and
//! handed to the plugin; the setup kept.

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
fn buildings_fields_from_a_plugin() {
    // The synthetic double circuit (its alignment `main`, the test
    // plugin's cuts on it), copied: the window writes beside it.
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
    r.building_fields.show(r.case, |_| Some(1000.0), &r.model);
    h.run();
    // The plugin found and its parameters as a form.
    h.get_by_label("Buildings (fields)");
    h.get_by_label("Plugin");
    h.get_by_label("Parameters");
    run_in(&h, "Buildings (fields)").click();
    wait(&mut h);
    let out = dir.join("buildings-fields");
    {
        let r = h.state_mut().ready().unwrap();
        let report = r.building_fields.report().0.unwrap().as_ref().unwrap();
        assert_eq!(report.preset, "default");
        let names: Vec<&str> = report.places.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["house-A", "house-B"]);
        assert!(report.places.iter().all(|p| p.b.is_some() && p.e.is_some()));
        assert_eq!(report.limits, (None, None));
        assert_eq!(report.out, out);
    }
    // One cut at a time, then the rest.
    for f in ["cut-house-A.txt", "cut-house-B.txt", "summary.txt"] {
        assert!(out.join(f).is_file(), "{f}");
    }
    let summary = std::fs::read_to_string(out.join("summary.txt")).unwrap();
    assert!(summary.contains("cut reports 2\nplan false\n"), "{summary}");
    let cut = std::fs::read_to_string(out.join("cut-house-A.txt")).unwrap();
    assert!(cut.contains("fields 2\nvalues 2\n"), "{cut}");
    // A field plan: New, saved (beside the line file), chosen.
    h.get_by_label("Setup").click();
    h.run();
    h.get_by_label("New").click();
    h.run();
    h.get_by_label("Save into the project").click();
    h.run();
    h.run();
    assert!(dir.join("field-plans.ohlfp").is_file());
    run_in(&h, "Buildings (fields)").click();
    {
        let r = h.state_mut().ready().unwrap();
        r.building_fields.job.result = None;
    }
    wait(&mut h);
    {
        let r = h.state_mut().ready().unwrap();
        let report = r.building_fields.report().0.unwrap().as_ref().unwrap();
        // The template's limits (the NISV's, as an example).
        assert_eq!(report.limits, (Some(1.0), Some(5.0)));
    }
    let summary = std::fs::read_to_string(out.join("summary.txt")).unwrap();
    assert!(summary.contains("plan true\n"), "{summary}");
    // The results: the verdict against the plan's limits, one row per
    // place.
    assert!(
        h.query_all_by_label_contains("over a limit")
            .next()
            .is_some()
            || h.query_all_by_label_contains("within the limits")
                .next()
                .is_some()
    );
    h.get_by_label("house-A");
    if let Some(d) = ohl_core::env::var_os("OHL_FIELD_SCREENSHOTS") {
        h.render()
            .unwrap()
            .save(std::path::Path::new(&d).join("buildings-fields-results.png"))
            .unwrap();
    }
    let _ = std::fs::remove_dir_all(&dir);
}
