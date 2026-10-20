// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Clearance rules by voltage in the application: the Clearances tab's
//! rule editor (a table "up to kV", a base plus an amount per kV, their
//! preview, Undo), the clearance windows' "Add a rule", and a design-code
//! plugin's rule preset loaded into the rule file.

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use ohl_clearance::RuleSet;
use ohl_clearance::rules::Requirement;

use crate::app::{App, Tab};
use crate::tests::{root, synthetic_project};

const RULES: &str = r#"format = "ohl-clearance-rules"
version = 1
name = "Test rules"

# The ground: kept as written.
[[rule]]
name = "Ground"
target = "ground"
vertical = "7 m"

[[rule]]
name = "Wires"
target = "same_line"
radial = "3 m" # phase to phase
"#;

/// A project with rule file `test.ohlr`, the Clearances tab shown and its
/// editor open; the file's path.
fn harness_with_rules(name: &str) -> (Harness<'static, App>, std::path::PathBuf) {
    let mut model = synthetic_project(name);
    // The weather cases a preset adds go to a copy, never the example's.
    let weather = model.sources.file.with_file_name("weather.ohlwc");
    std::fs::copy(model.sources.weather.as_ref().unwrap(), &weather).unwrap();
    model.sources.weather = Some(weather);
    let work = model.library_dir.clone().unwrap();
    std::fs::create_dir_all(work.join("rules")).unwrap();
    let file = work.join("rules/test.ohlr");
    std::fs::write(&file, RULES).unwrap();
    model.store_libraries().unwrap();
    let mut app = App::with_model(model, None);
    {
        let r = app.ready().unwrap();
        r.clearances.refresh(&r.model, None);
        r.open_page = Some((Tab::Clearances, None));
    }
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| app);
    harness.run();
    harness.get_by_label("Edit rules").click();
    harness.run();
    (harness, file)
}

fn rules_of(file: &std::path::Path) -> RuleSet {
    RuleSet::load(file).unwrap()
}

/// The Clearances tab's editor: the ground's distance turned into a table
/// (a row added for the voltage above it), the wires' into base + per kV;
/// the preview gives the distance at 0 and 220 kV (the synthetic line's);
/// each change written in place (comments kept) as a logged command that
/// Undo takes back.
#[test]
fn the_rule_editor_sets_distances_by_voltage() {
    let (mut harness, file) = harness_with_rules("rule-editor");
    assert_eq!(
        harness.state_mut().ready().unwrap().rule_editor.rules.len(),
        2
    );
    // Ground: one distance → a table.
    harness
        .query_all_by_value("one distance")
        .next()
        .unwrap()
        .click();
    harness.run();
    harness.get_by_label("table by voltage").click();
    harness.run();
    harness.run();
    assert_eq!(
        rules_of(&file).rules[0].vertical,
        Some(Requirement::ByVoltage(vec![(220.0, 7.0)]))
    );
    // Its menu: a row added above the line's voltages, the preview.
    harness.get_by_label("1 row up to 220 kV…").click();
    harness.run();
    harness.run();
    harness.get_by_label("Add row").click();
    harness.run();
    harness.get_by_label("0 kV (earthed): 7.00 m");
    harness.get_by_label("220 kV: 7.00 m");
    harness.run();
    assert_eq!(
        rules_of(&file).rules[0].vertical,
        Some(Requirement::ByVoltage(vec![(220.0, 7.0), (320.0, 7.0)]))
    );
    harness.key_press(egui::Key::Escape);
    harness.run();
    // Wires: one distance → base + per kV.
    harness.get_by_value("one distance").click();
    harness.run();
    harness.get_by_label("base + per kV").click();
    harness.run();
    harness.run();
    assert_eq!(
        rules_of(&file).rules[1].radial,
        Some(Requirement::Formula {
            base: 3.0,
            per_kv: 0.0,
            above_kv: 0.0
        })
    );
    // 0.01 m per kV above 50 kV: 3 + 0.01 · (220 − 50) = 4.7 m at 220 kV.
    {
        let e = &mut harness.state_mut().ready().unwrap().rule_editor;
        e.rules[1].radial = Some(Requirement::Formula {
            base: 3.0,
            per_kv: 0.01,
            above_kv: 50.0,
        });
        e.dirty = true;
    }
    harness.run();
    harness.get_by_label("3 m + 0.01 m/kV above 50 kV…").click();
    harness.run();
    harness.run();
    harness.get_by_label("0 kV (earthed): 3.00 m");
    harness.get_by_label("220 kV: 4.70 m");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("# The ground: kept as written."), "{text}");
    assert!(text.contains("# phase to phase"), "{text}");
    assert!(
        text.contains("radial = { base = \"3 m\", per_kv = \"0.01 m\", above_kv = 50.0 }"),
        "{text}"
    );
    // The Clearances tab's check reads the new rules.
    let r = harness.state_mut().ready().unwrap();
    let rules = r.clearances.rules.clone().unwrap().unwrap();
    assert_eq!(
        rules[1].required(Some(220.0)).radial,
        Some(3.0 + 0.01 * 170.0)
    );
    // Undo: the change before (per kV 0).
    r.perform(crate::edit::Action::Undo);
    harness.run();
    harness.run();
    assert_eq!(
        rules_of(&file).rules[1].radial,
        Some(Requirement::Formula {
            base: 3.0,
            per_kv: 0.0,
            above_kv: 0.0
        })
    );
    assert_eq!(
        harness.state_mut().ready().unwrap().rule_editor.rules[1].radial,
        rules_of(&file).rules[1].radial,
        "the editor follows the file"
    );
}

/// The vertical clearances window's "Add a rule": the distance as a base
/// plus an amount per kV, the warning until it is set, the rule written
/// with its formula.
#[test]
fn add_a_rule_takes_a_distance_by_voltage() {
    let model = synthetic_project("add-rule-by-voltage");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.get_by_label("Checks").click();
    harness.run();
    harness.get_by_label_contains("Ground clearances").hover();
    harness.run();
    harness.get_by_label("Vertical clearances…").click();
    harness.run();
    // The helper's form chooser is the first; the grid's row the second.
    harness
        .query_all_by_value("one distance")
        .next()
        .unwrap()
        .click();
    harness.run();
    harness.get_by_label("base + per kV").click();
    harness.run();
    assert!(
        harness
            .query_all_by_label_contains("set the base distance")
            .next()
            .is_some()
    );
    {
        let w = &mut harness.state_mut().ready().unwrap().vertical_clearances;
        w.draft.distance = Requirement::Formula {
            base: 6.0,
            per_kv: 0.01,
            above_kv: 0.0,
        };
    }
    harness.run();
    harness.get_by_label("Add this rule").click();
    harness.run();
    harness.run();
    let w = &harness.state_mut().ready().unwrap().vertical_clearances;
    assert_eq!(w.rules.len(), 1);
    let set = w.rule_set();
    // 6 m + 0.01 m/kV: 8.2 m at 220 kV.
    assert!((set.rules[0].required(Some(220.0)).vertical.unwrap() - 8.2).abs() < 1e-12);
    let file = w.file.clone().unwrap();
    let text = std::fs::read_to_string(file).unwrap();
    assert!(
        text.contains("vertical = { base = \"6 m\", per_kv = \"0.01 m\""),
        "{text}"
    );
}

/// A design-code plugin's rule preset (the test plugin
/// `plugins/tests/code-presets`, when built): listed in Load preset…,
/// previewed with its distances at the line's voltages, merged into the
/// rule file as one command (its weather case added to the model).
#[test]
fn a_plugin_rule_preset_loads_into_the_rule_file() {
    let dir = root().join("plugins/tests/code-presets");
    let built = ohl_plugins::Manifest::load(&dir)
        .unwrap()
        .entry_path()
        .is_file();
    assert!(
        built || ohl_core::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
        "code-presets test plugin not built"
    );
    if !built {
        eprintln!("skipping: plugins/tests/code-presets not built");
        return;
    }
    let (mut harness, file) = harness_with_rules("rule-preset");
    {
        let app = harness.state_mut();
        app.plugins = crate::plugins::Plugins::load(None, None);
        app.plugins.folders = vec![dir];
        app.plugins.refresh();
    }
    harness.run();
    harness.get_by_label("Load preset…").click();
    let started = std::time::Instant::now();
    while harness
        .state_mut()
        .ready()
        .unwrap()
        .rule_editor
        .presets
        .busy()
        || harness
            .state_mut()
            .ready()
            .unwrap()
            .rule_editor
            .presets
            .list
            .is_empty()
    {
        harness.step();
        assert!(started.elapsed().as_secs() < 60, "no presets in 60 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    harness.run();
    // The plugin's one preset ("ground": a label other pages use too).
    {
        let w = &mut harness.state_mut().ready().unwrap().rule_editor.presets;
        assert_eq!(w.list.len(), 1);
        assert_eq!(w.list[0].plugin, "code-presets");
        w.chosen = Some((0, 0));
    }
    harness.run();
    // The preview: 6 m + 0.01 m/kV → 8.20 m at 220 kV; crossings: the
    // 420 kV row (4.5 m) at 220 kV.
    harness.get_by_label_contains("220 kV: 8.20 m");
    harness.get_by_label_contains("220 kV: 4.50 m");
    harness.get_by_label("Merge into test.ohlr").click();
    harness.run();
    harness.run();
    let set = rules_of(&file);
    let names: Vec<&str> = set.rules.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["Ground", "Wires", "Test ground", "Test crossings"]);
    let r = harness.state_mut().ready().unwrap();
    assert!(
        r.model.cases.iter().any(|c| c.case.name == "Test hot"),
        "{:?}",
        r.rule_editor.status
    );
    // One command: Undo takes the rules back.
    r.perform(crate::edit::Action::Undo);
    harness.run();
    assert_eq!(rules_of(&file).rules.len(), 2);
}
