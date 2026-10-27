// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Clearance rules by voltage in the application: the Clearances window's
//! rule editor (a table "up to kV", a base plus an amount per kV, their
//! preview, Undo), the clearance windows' rule rows, and a design-code
//! plugin's rule preset loaded into the rule file; the voltage filter
//! (no voltage needed with a table or a formula), Merge rules by voltage
//! and Remove all rules.

use eframe::egui;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use ohl_clearance::RuleSet;
use ohl_clearance::rules::Requirement;

use crate::app::App;
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

/// A project with rule file `test.ohlr`, the Clearances window open and its
/// editor open; the file's path.
fn harness_with_rules(name: &str) -> (Harness<'static, App>, std::path::PathBuf) {
    harness_with(name, RULES)
}

/// [`harness_with_rules`] with the rule file `text`.
fn harness_with(name: &str, text: &str) -> (Harness<'static, App>, std::path::PathBuf) {
    let mut model = synthetic_project(name);
    // The weather cases a preset adds go to a copy, never the example's.
    let weather = model.sources.file.with_file_name("weather.ohlwc");
    std::fs::copy(model.sources.weather.as_ref().unwrap(), &weather).unwrap();
    model.sources.weather = Some(weather);
    let work = model.library_dir.clone().unwrap();
    std::fs::create_dir_all(work.join("rules")).unwrap();
    let file = work.join("rules/test.ohlr");
    std::fs::write(&file, text).unwrap();
    model.store_libraries().unwrap();
    let mut app = App::with_model(model, None);
    {
        let r = app.ready().unwrap();
        r.clearances.refresh(&r.model, None);
        r.clearances.open = true;
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

/// The Clearances window's editor: the ground's distance turned into a table
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
    // Ground: one distance → a table (the form selector's button; the
    // Wires rule's is the second).
    harness
        .query_all_by_label("Table by voltage")
        .next()
        .unwrap()
        .click();
    harness.run();
    harness.run();
    assert_eq!(
        rules_of(&file).rules[0].vertical,
        Some(Requirement::ByVoltage(vec![(220.0, 7.0)]))
    );
    // Its rows in place: a row added above the line's voltages, the live
    // line.
    harness.get_by_label("voltage up to");
    harness.get_by_label("+ Add row").click();
    harness.run();
    harness.get_by_label("At 220 kV: 7.00 m");
    harness.run();
    assert_eq!(
        rules_of(&file).rules[0].vertical,
        Some(Requirement::ByVoltage(vec![(220.0, 7.0), (320.0, 7.0)]))
    );
    // Wires: one distance → base + per kV.
    harness
        .query_all_by_label("Base + per kV")
        .nth(1)
        .unwrap()
        .click_accesskit();
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
    harness.run();
    harness.get_by_label("3 m + 0.01 m/kV above 50 kV — At 220 kV: 4.70 m");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("# The ground: kept as written."), "{text}");
    assert!(text.contains("# phase to phase"), "{text}");
    assert!(
        text.contains("radial = { base = \"3 m\", per_kv = \"0.01 m\", above_kv = 50.0 }"),
        "{text}"
    );
    // The Clearances window's check reads the new rules.
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

/// The vertical clearances window's rule, edited in its row: the distance
/// as a base plus an amount per kV, the warning until it is set, the rule
/// written with its formula.
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
    // The first rule's row (the only one): its form selector.
    harness.get_by_label("Base + per kV").click();
    harness.run();
    assert!(
        harness
            .query_all_by_label_contains("set the base distance")
            .next()
            .is_some()
    );
    {
        let w = &mut harness.state_mut().ready().unwrap().vertical_clearances;
        w.rules[0].distance = Requirement::Formula {
            base: 6.0,
            per_kv: 0.01,
            above_kv: 0.0,
        };
        // (As a field typed: written once the mouse is up.)
        w.mark_dirty();
    }
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

/// The Swiss LeV plugin's rule presets (when this checkout has
/// `local-data/plugins/lev-ch`, built with the `rule-presets` export), read
/// as Load preset… reads them and merged into a rule file: the law's span
/// line ground clearance, 7.5 m + 0.01 m/kV (9.7 m at 220 kV).
#[test]
fn the_lev_rule_presets_load_into_the_rule_file() {
    let dir = root().join("local-data/plugins/lev-ch");
    let Ok(m) = ohl_plugins::Manifest::load(&dir) else {
        eprintln!("skipping: no local-data/plugins/lev-ch");
        return;
    };
    if !m.entry_path().is_file() {
        eprintln!("skipping: lev-ch not built");
        return;
    }
    let host = ohl_plugins::WasmHost::new().unwrap();
    if !host.load_code(&dir).unwrap().has_rule_presets() {
        eprintln!("skipping: lev-ch built without rule-presets");
        return;
    }
    let (mut harness, file) = harness_with_rules("lev-rule-preset");
    let params = dir.join("example-parameters.toml");
    {
        let r = harness.state_mut().ready().unwrap();
        r.rule_editor
            .presets
            .read(vec![("lev-ch".into(), dir.clone())], Some(params));
        assert!(
            r.rule_editor
                .presets
                .poll(std::time::Duration::from_secs(60))
        );
        let list = r.rule_editor.presets.list[0].presets.as_ref().unwrap();
        // The preset holding the ground rules (the plugin offers its
        // general presets; their names are the plugin's).
        let k = list
            .iter()
            .position(|p| {
                p.rules
                    .rules
                    .iter()
                    .any(|r| r.name.starts_with("Ground vertical"))
            })
            .unwrap();
        r.rule_editor.presets.chosen = Some((0, k));
        crate::rule_editor::apply_preset(r, crate::rule_editor::Load::Merge);
    }
    harness.run();
    let set = rules_of(&file);
    let ground = set
        .rules
        .iter()
        .find(|r| r.name.starts_with("Ground vertical"))
        .unwrap();
    assert!((ground.required(Some(220.0)).vertical.unwrap() - 9.7).abs() < 1e-9);
    let r = harness.state_mut().ready().unwrap();
    assert!(
        r.model
            .cases
            .iter()
            .any(|c| c.case.name.starts_with("LeV 40")),
        "{:?}",
        r.rule_editor.status
    );
}

/// A rule preset whose weather case the project has under another name
/// (owner, 2026-10-21): the preset's "Utility 80 °C" is the project's
/// "max temperature" (80 °C, still air). Merged: no case created, its rule
/// names "max temperature", the window's preview and the status say so;
/// one Undo takes the rules back.
#[test]
fn a_rule_preset_uses_the_projects_case_of_the_same_weather() {
    let mut model = synthetic_project("rule-preset-same");
    // The project's own weather case file (kept in the project: undone).
    let work = model.library_dir.clone().unwrap();
    assert!(model.sources.weather.as_ref().unwrap().starts_with(&work));
    std::fs::create_dir_all(work.join("rules")).unwrap();
    let file = work.join("rules/test.ohlr");
    std::fs::write(&file, RULES).unwrap();
    model.store_libraries().unwrap();
    let mut app = App::with_model(model, None);
    app.plugins = crate::plugins::Plugins::load(None, None);
    {
        let r = app.ready().unwrap();
        r.clearances.refresh(&r.model, None);
        r.clearances.open = true;
    }
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| app);
    harness.run();
    harness.get_by_label("Edit rules").click();
    harness.run();
    let rules = "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"p\"\n\
                 [[rule]]\nname = \"Preset ground\"\ntarget = \"ground\"\nvertical = \"8 m\"\n\
                 cases = [\"Utility 80 °C\"]\n";
    let cases = "format = \"ohl-weather-cases\"\nversion = 1\n\
                 [[case]]\nname = \"Utility 80 °C\"\ntemperature = \"80 °C\"\n";
    let (names, entries) = {
        let r = harness.state_mut().ready().unwrap();
        let w = &mut r.rule_editor.presets;
        w.list = vec![crate::rule_editor::PluginPresets {
            plugin: "code-a".into(),
            presets: ohl_io::codes::check_rule_presets(
                "code-a",
                vec![ohl_plugins::RulePreset {
                    name: "ground".into(),
                    description: "invented test values".into(),
                    rules: rules.into(),
                    weather_cases: cases.into(),
                }],
            ),
        }];
        // As the plugins gave them (the project's edits applied to them).
        w.original = w.list.clone();
        w.asked = true;
        w.open = true;
        w.chosen = Some((0, 0));
        let names: Vec<String> = r.model.cases.iter().map(|c| c.case.name.clone()).collect();
        (names, r.model.session().unwrap().log().unwrap().len())
    };
    harness.run();
    harness.get_by_label_contains(
        "weather case \"Utility 80 °C\": uses your case \"max temperature\"",
    );
    harness.get_by_label("Merge into test.ohlr").click();
    harness.run();
    harness.run();
    let set = rules_of(&file);
    let ground = set
        .rules
        .iter()
        .find(|r| r.name == "Preset ground")
        .unwrap();
    assert_eq!(ground.cases, ["max temperature"]);
    let r = harness.state_mut().ready().unwrap();
    let now: Vec<String> = r.model.cases.iter().map(|c| c.case.name.clone()).collect();
    assert_eq!(now, names, "{:?}", r.rule_editor.status);
    let status = r.rule_editor.status.clone().unwrap().0;
    assert!(
        status.contains("uses your case \"max temperature\""),
        "{status}"
    );
    // One change (the rule file): Undo takes the rules back.
    assert_eq!(r.model.session().unwrap().log().unwrap().len(), entries + 1);
    r.perform(crate::edit::Action::Undo);
    harness.run();
    assert_eq!(rules_of(&file).rules.len(), 2);
    std::fs::remove_dir_all(file.parent().unwrap()).ok();
}

/// Toggled state of a checkbox node.
fn ticked(node: &egui_kittest::Node<'_>) -> bool {
    node.accesskit_node().toggled() == Some(egui::accesskit::Toggled::True)
}

/// A table rule needs no voltage: its filter hides behind "Only for some
/// voltages…", off. An older file's table rule with a filter keeps it
/// (shown ticked, it applies); unticked, the filter is cleared from the
/// file, and Undo brings it back. A rule of one distance offers the
/// filter as it is (every voltage when none is ticked).
#[test]
fn a_table_rule_needs_no_voltage() {
    const TEXT: &str = r#"format = "ohl-clearance-rules"
version = 1

[[rule]]
name = "Table"
target = "ground"
vertical = { by_voltage = [[0, "5 m"], [400, "7 m"]] }

[[rule]]
name = "Older table"
target = "ground"
vertical = { by_voltage = [[0, "5 m"], [400, "7 m"]] }
voltages = [220]

[[rule]]
name = "One distance"
target = "ground"
vertical = "6 m"
"#;
    let (mut harness, file) = harness_with("voltage-filter", TEXT);
    // The old filter read, and applied by the check.
    let set = rules_of(&file);
    assert_eq!(set.rules[1].voltages, vec![220.0]);
    assert!(!set.rules[1].checks_voltage(None));
    assert_eq!(
        harness.state_mut().ready().unwrap().rule_editor.rules[1].voltages,
        vec![220.0]
    );
    // Two toggles (the table rules), the first off, the older one on.
    let toggles: Vec<bool> = harness
        .query_all_by_label(crate::requirement_ui::ONLY_SOME_VOLTAGES)
        .map(|n| ticked(&n))
        .collect();
    assert_eq!(toggles, vec![false, true]);
    // The older rule's voltages shown; the plain rule's filter: every
    // voltage.
    harness.get_by_label("220 kV");
    harness.get_by_label("every voltage");
    // Unticked: the filter cleared in the file.
    harness
        .query_all_by_label(crate::requirement_ui::ONLY_SOME_VOLTAGES)
        .nth(1)
        .unwrap()
        .click_accesskit();
    harness.run();
    harness.run();
    assert!(rules_of(&file).rules[1].voltages.is_empty());
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(!text.contains("voltages"), "{text}");
    // Undo: back.
    harness
        .state_mut()
        .ready()
        .unwrap()
        .perform(crate::edit::Action::Undo);
    harness.run();
    harness.run();
    assert_eq!(rules_of(&file).rules[1].voltages, vec![220.0]);
}

/// The vertical window's rule rows: with one distance
/// the voltages are a tick menu (every voltage when none is ticked); with
/// a table, behind "Only for some voltages…", off.
#[test]
fn a_clearance_window_rule_needs_no_voltage_for_a_table() {
    let model = synthetic_project("zone-voltage-filter");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        r.vertical_clearances.show(&r.model);
    }
    harness.run();
    // The first rule: one distance.
    assert_eq!(harness.query_all_by_label("every voltage").count(), 1);
    assert_eq!(
        harness
            .query_all_by_label(crate::requirement_ui::ONLY_SOME_VOLTAGES)
            .count(),
        0
    );
    {
        let w = &mut harness.state_mut().ready().unwrap().vertical_clearances;
        w.rules[0].distance = Requirement::ByVoltage(vec![(400.0, 7.0)]);
    }
    harness.run();
    assert_eq!(harness.query_all_by_label("every voltage").count(), 0);
    assert!(!ticked(
        &harness.get_by_label(crate::requirement_ui::ONLY_SOME_VOLTAGES)
    ));
    assert!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .vertical_clearances
            .rules[0]
            .voltages
            .is_empty()
    );
    // Ticked: the filter's voltages offered.
    harness
        .get_by_label(crate::requirement_ui::ONLY_SOME_VOLTAGES)
        .click();
    harness.run();
    assert_eq!(harness.query_all_by_label("every voltage").count(), 1);
}

/// "Merge rules by voltage": offered only with rules alike but for one
/// voltage and a plain distance; previewed (what changes at the line's
/// voltages); merged into one table rule without a filter, in the first
/// one's place; Undo takes it back.
#[test]
fn rules_by_voltage_merge_into_one_table_rule() {
    const TEXT: &str = r#"format = "ohl-clearance-rules"
version = 1

# Earthed wires.
[[rule]]
name = "Ground"
target = "ground"
vertical = "5 m"
voltages = [0]

[[rule]]
name = "Ground 220"
target = "ground"
vertical = "7 m"
voltages = [220]

[[rule]]
name = "Wires"
target = "same_line"
radial = "3 m"
"#;
    let (mut harness, file) = harness_with("merge-by-voltage", TEXT);
    harness.get_by_label("Merge rules by voltage…").click();
    harness.run();
    harness.get_by_label_contains(
        "\"Ground\" (earthed (0 kV)), \"Ground 220\" (220 kV) become one rule",
    );
    harness.get_by_label("vertical, 220 kV: 7.00 m, merged 7.00 m");
    harness.get_by_label("vertical, earthed (0 kV): 5.00 m, merged 5.00 m");
    harness.get_by_label("Merge").click();
    harness.run();
    harness.run();
    let set = rules_of(&file);
    assert_eq!(set.rules.len(), 2);
    assert_eq!(set.rules[0].name, "Ground");
    assert_eq!(
        set.rules[0].vertical,
        Some(Requirement::ByVoltage(vec![(0.0, 5.0), (220.0, 7.0)]))
    );
    assert!(set.rules[0].voltages.is_empty());
    assert_eq!(set.rules[1].name, "Wires");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("# Earthed wires."), "{text}");
    // Nothing left to merge: not offered.
    assert!(
        harness
            .query_all_by_label("Merge rules by voltage…")
            .next()
            .is_none()
    );
    harness
        .state_mut()
        .ready()
        .unwrap()
        .perform(crate::edit::Action::Undo);
    harness.run();
    harness.run();
    assert_eq!(rules_of(&file).rules.len(), 3);
    assert_eq!(
        harness.state_mut().ready().unwrap().rule_editor.rules.len(),
        3
    );
}

/// "Remove all rules", beside Add rule and Load preset…: asks first
/// (Keep them leaves them), then removes every rule of the file (its
/// other tables kept); Undo brings them back.
#[test]
fn remove_all_rules_asks_and_undoes() {
    let (mut harness, file) = harness_with_rules("remove-all-rules");
    // After Add rule and Load preset…, on their row (wrapped to the next
    // one in the narrow tab).
    let (add, load, all) = (
        harness.get_by_label("Add rule").rect(),
        harness.get_by_label("Load preset…").rect(),
        harness.get_by_label("Remove all rules").rect(),
    );
    assert!(add.right() < load.left());
    assert!(
        (load.right() < all.left() && (load.center().y - all.center().y).abs() < 2.0)
            || (all.top() >= load.bottom() && all.top() - load.bottom() < 12.0),
        "{load:?} {all:?}"
    );
    harness.get_by_label("Remove all rules").click();
    harness.run();
    harness.get_by_label("Remove all 2 rules of this file?");
    harness.get_by_label("Keep them").click();
    harness.run();
    assert_eq!(rules_of(&file).rules.len(), 2);
    harness.get_by_label("Remove all rules").click();
    harness.run();
    harness.get_by_label("Remove them").click();
    harness.run();
    harness.run();
    assert!(rules_of(&file).rules.is_empty());
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("name = \"Test rules\""), "{text}");
    harness
        .state_mut()
        .ready()
        .unwrap()
        .perform(crate::edit::Action::Undo);
    harness.run();
    harness.run();
    assert_eq!(rules_of(&file).rules.len(), 2);
    assert_eq!(
        harness.state_mut().ready().unwrap().rule_editor.rules.len(),
        2
    );
}

/// The Clearances window (usability pass 2026-10-27): the rule file named,
/// "not checked yet" before a run, the results above the rule editor, the
/// ground and LiDAR clearance windows reachable from it.
#[test]
fn the_clearances_window_puts_the_results_first() {
    let (mut harness, _file) = harness_with_rules("clearances-order");
    harness.get_by_label("Rule file");
    let hint = harness.get_by_label_contains("Not checked yet").rect();
    let edit = harness.get_by_label("Edit rules").rect();
    assert!(hint.bottom() <= edit.top());
    harness.get_by_label("Run").click();
    for _ in 0..200 {
        harness.step();
        if harness
            .state_mut()
            .ready()
            .unwrap()
            .clearances
            .report
            .is_some()
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    harness.run();
    let head = harness.get_by_label_contains("violations (").rect();
    assert!(head.bottom() <= harness.get_by_label("Edit rules").rect().top());
    // Below the rule editor (scrolled to: the page's items have more
    // room since 2026-10-27).
    harness.get_by_label("Vertical…").scroll_to_me();
    harness.run();
    harness.get_by_label("Vertical…").click();
    harness.run_steps(3);
    assert!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .vertical_clearances
            .open
    );
}

/// The Clearances window's Run: in its footer, under the rules, running the
/// check of the rule file.
#[test]
fn the_clearances_window_runs_from_its_footer() {
    let (mut harness, _file) = harness_with_rules("clearances-footer");
    let run = harness.get_by_label("Run").rect();
    let edit = harness.get_by_label("Edit rules").rect();
    assert!(run.top() > edit.bottom());
    harness.get_by_label("Run").click();
    harness.step();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.clearances.running() || r.clearances.report.is_some());
}

/// A plugin's preset value changed in the project (Plugin manager ›
/// Presets…): the edit kept in the project's library is taken by every
/// window's presets (the preview at 220 kV: 7 m + 0.01 m/kV → 9.20 m
/// instead of 8.20 m), listed in the Changes tab as applied, and Reset all
/// takes the plugin's value back.
#[test]
fn a_preset_value_changed_in_the_project() {
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
    let (mut harness, _file) = harness_with_rules("preset-edit");
    {
        let app = harness.state_mut();
        app.plugins = crate::plugins::Plugins::load(None, None);
        app.plugins.folders = vec![dir];
        app.plugins.refresh();
        app.plugins.open = true;
    }
    // The manager repaints until the plugins' formats are loaded: waited
    // for (under a minute).
    let started = std::time::Instant::now();
    while harness.state_mut().plugins.formats.is_none() {
        harness.step();
        assert!(started.elapsed().as_secs() < 60, "no formats in 60 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    harness.run();
    harness.get_by_label("Presets…").click();
    let started = std::time::Instant::now();
    loop {
        harness.step();
        let r = harness.state_mut().ready().unwrap();
        if !r.rule_editor.presets.busy()
            && !r.rule_editor.presets.original.is_empty()
            && !r.preset_editor.reading()
        {
            break;
        }
        assert!(started.elapsed().as_secs() < 60, "no presets in 60 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    harness.run();
    harness.get_by_label("ground — rules");
    let edit = crate::preset_edits::Edit {
        item: "rules:ground".into(),
        path: vec!["rule".into(), "name=Test ground".into(), "vertical".into()],
        original: "{ base = \"6 m\", per_kv = \"0.01 m\", above_kv = 0.0 }".into(),
        value: "{ base = \"7 m\", per_kv = \"0.01 m\", above_kv = 0.0 }".into(),
        note: "the utility's".into(),
    };
    {
        let r = harness.state_mut().ready().unwrap();
        r.model
            .project_op(
                "preset values",
                ohl_server::Op::PutLibrary {
                    kind: crate::preset_edits::KIND.into(),
                    name: crate::preset_edits::file_name("code-presets"),
                    text: crate::preset_edits::to_text("code-presets", &[edit]),
                },
            )
            .unwrap();
    }
    harness.run();
    harness.run();
    shot(&mut harness, "preset-editor-presets");
    harness.get_by_label("Changes (1)").click();
    harness.run();
    harness.get_by_label("applied");
    shot(&mut harness, "preset-editor-changes");
    {
        let r = harness.state_mut().ready().unwrap();
        let p = &r.rule_editor.presets.list[0].presets.as_ref().unwrap()[0];
        let g = p
            .rules
            .rules
            .iter()
            .find(|x| x.name == "Test ground")
            .unwrap();
        assert!((g.required(Some(220.0)).vertical.unwrap() - 9.2).abs() < 1e-9);
        assert!(
            p.description.contains("1 value changed"),
            "{}",
            p.description
        );
        let o = &r.rule_editor.presets.original[0].presets.as_ref().unwrap()[0];
        let g = o
            .rules
            .rules
            .iter()
            .find(|x| x.name == "Test ground")
            .unwrap();
        assert!((g.required(Some(220.0)).vertical.unwrap() - 8.2).abs() < 1e-9);
    }
    harness.get_by_label("Reset all").click();
    harness.run();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(crate::preset_edits::of_model(&r.model).0.is_empty());
    let p = &r.rule_editor.presets.list[0].presets.as_ref().unwrap()[0];
    let g = p
        .rules
        .rules
        .iter()
        .find(|x| x.name == "Test ground")
        .unwrap();
    assert!((g.required(Some(220.0)).vertical.unwrap() - 8.2).abs() < 1e-9);
}

/// Saves the window as `name`.png in `OHL_RENDER_PNG` (when set and the
/// harness renders).
fn shot(h: &mut Harness<'_, App>, name: &str) {
    if let Some(dir) = ohl_core::env::var_os("OHL_RENDER_PNG")
        && let Ok(image) = h.render()
    {
        let path = std::path::PathBuf::from(dir).join(format!("{name}.png"));
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
}
