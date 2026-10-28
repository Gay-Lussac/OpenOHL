// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: display options, ratings, design codes, drawings, versions, the
//! coordinate system.

use super::*;

/// Terrain › Survey data › Class colours: a class's colour picked, the
/// scheme saved to a file and used for the project (this user's choice);
/// back to the cloud's own.
#[test]
fn class_colours_come_from_a_scheme_file() {
    use crate::class_colors::{ClassAction, set_color};
    let model = synthetic_project("class-colours");
    let dir = model.sources.file.parent().unwrap().to_owned();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // (Terrain › Survey data › Class colours…, with a point cloud.)
    harness.state_mut().ready().unwrap().class_window = Some(Default::default());
    harness.run();
    harness.get_by_label_contains("Scheme: ");
    let file = dir.join("mine.ohlc");
    {
        let r = harness.state_mut().ready().unwrap();
        set_color(&mut r.model.scheme, 2, Some("#123456".into()));
        set_color(&mut r.model.scheme, 77, Some("#abcdef".into()));
        assert_eq!(
            crate::scene::class_color(&r.model.scheme, 2),
            Some([0x12, 0x34, 0x56])
        );
        std::fs::write(&file, r.model.scheme.to_toml()).unwrap();
        r.model.use_class_scheme(Some(file.clone())).unwrap();
        assert_eq!(
            r.model.view_prefs().class_scheme.as_deref(),
            Some(file.as_path())
        );
    }
    // The file holds the colours and the class added.
    let s = ohl_lidar::ClassScheme::load(&file).unwrap();
    assert_eq!(crate::scene::class_color(&s, 77), Some([0xab, 0xcd, 0xef]));
    assert_eq!(s.class(77).unwrap().name, "Class 77");
    // Back to the cloud's own: no file; remembered as such (an empty path).
    let r = harness.state_mut().ready().unwrap();
    r.model.use_class_scheme(None).unwrap();
    assert!(r.model.scheme.class(77).is_none());
    assert_eq!(
        r.model.view_prefs().class_scheme,
        Some(std::path::PathBuf::new())
    );
    let _ = ClassAction::Recolor;
}

/// Section display options: nothing by default; a label per section and a
/// sag per span in the views ticked only; kept in the view preferences.
#[test]
fn section_labels_and_sags_per_view() {
    use crate::annotations::{Kind, View, notes};
    let mut m = synthetic();
    for v in View::ALL {
        assert!(notes(&m, 0, v, None).is_empty());
    }
    m.section_display.label[View::Plan as usize] = true;
    m.section_display.sag[View::ThreeD as usize] = true;
    let plan = notes(&m, 0, View::Plan, None);
    assert_eq!(plan.len(), m.line.sections.len());
    assert!(plan.iter().all(|n| n.kind == Kind::Label));
    assert!(notes(&m, 0, View::Profile, None).is_empty());
    let sags = notes(&m, 0, View::ThreeD, None);
    let spans: usize = m.cases[0].shapes.iter().map(|s| s.spans.len()).sum();
    assert_eq!(sags.len(), spans);
    let first = &m.cases[0].shapes[0].spans[0];
    assert_eq!(sags[0].text, format!("{:.2} m", first.sag));
    // Remembered with this user's view of the project.
    let prefs = m.view_prefs();
    assert_eq!(prefs.section_display, m.section_display);
    let back: crate::model::ViewPrefs =
        serde_json::from_str(&serde_json::to_string(&prefs).unwrap()).unwrap();
    assert!(back.section_display.sag[View::ThreeD as usize]);
}

/// Menus of display options stay open while boxes are ticked: several are
/// ticked in a row.
#[test]
fn display_options_stay_open_while_ticking() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    crate::tests::line_menu(&mut harness, "Sections");
    harness.run();
    harness.get_by_label_contains("Display options").hover();
    harness.run();
    harness.get_by_label("Sag in Plan").click();
    harness.run();
    // Still open: the next box can be ticked.
    harness.get_by_label("Section label in 3D").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.section_display.sag[crate::annotations::View::Plan as usize]);
    assert!(r.model.section_display.label[crate::annotations::View::ThreeD as usize]);
}

/// Checks › Thermal rating: the ambient cases next to the model, an
/// ampacity per section and case; the clearance-limited temperature in the
/// background with the rule file next to the model.
#[test]
fn thermal_rating_window() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.get_by_label("Checks").click();
    harness.run();
    harness.get_by_label("Thermal rating…").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.thermal.open);
        assert!(matches!(&r.thermal.ambient, Some(Ok(set)) if set.cases.len() == 3));
    }
    // The ampacities at once, in the Results tab.
    harness.get_by_label("Results").click();
    harness.run();
    harness.get_by_label("summer noon at 80 °C");
    harness.get_by_label("Run").click();
    harness.step();
    run_until(&mut harness, |a| {
        let r = a.ready().unwrap();
        r.thermal.poll();
        !r.thermal.running() && r.thermal.limits.is_some()
    });
    harness.step();
    harness.step();
    let r = harness.state_mut().ready().unwrap();
    let limits = r.thermal.limits.as_ref().unwrap().as_ref().unwrap();
    assert_eq!(limits.len(), r.model.line.sections.len());
    // S2 over the hill misses the 7 m ground rule even when cold.
    let s2 = limits.iter().find(|l| l.section == "S2").unwrap();
    assert_eq!(s2.result, Ok(ohl_thermal::limit::Limit::BelowRange));
    harness.get_by_label("clearance limit");
}

/// Checks › Bill of materials prices the typed example with the cost file
/// next to it; Tools › Spot structures proposes structures along the main
/// route on the survey, in the background.
#[test]
fn bill_of_materials_and_spotting_windows() {
    let ex = root().join("examples/synthetic-line");
    let model = model::load(
        Sources {
            file: ex.join("synthetic-line-typed.ohl.json"),
            library: Some(root().join("libraries/cables")),
            structures: Some(ex.join("Structures")),
            terrain: Some(ex.join("synthetic-survey.csv")),
            point_budget: 1000,
            corridor: 60.0,
            ..Sources::default()
        },
        &|_| {},
    )
    .unwrap();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.get_by_label("Plans & sheets").click();
    harness.run();
    harness.get_by_label_contains("Bill of materials").hover();
    harness.run();
    harness.get_by_label("Wires…").click();
    run_until(&mut harness, |a| {
        let r = a.ready().unwrap();
        r.bom.result.is_some() && !r.bom.running()
    });
    harness.step();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(
            r.bom
                .cost_path
                .as_ref()
                .is_some_and(|p| p.ends_with("unit-costs.ohlcost"))
        );
        let e = r.bom.result.as_ref().unwrap().as_ref().unwrap();
        assert!(e.priced && e.missing.is_empty(), "{:?}", e.missing);
        assert_eq!(e.bom.structures.iter().map(|s| s.count).sum::<usize>(), 7);
        // Only the cables are offered yet.
        assert!(!e.lines.is_empty());
        assert!(
            e.lines
                .iter()
                .all(|l| crate::bom::CABLES.contains(&l.category.as_str()))
        );
    }
    assert!(
        harness
            .query_all_by_label_contains("Total: ")
            .next()
            .is_some()
    );

    // The menu: Line › Structures.
    crate::tests::line_menu(&mut harness, "Structures");
    harness.get_by_label("Spot structures…").click();
    harness.run();
    // The bill's window is open too: the spotting window's Run.
    run_in(&harness, "Spot structures").click_accesskit();
    run_until(&mut harness, |a| {
        let r = a.ready().unwrap();
        r.spot.result.is_some() && !r.spot.running()
    });
    harness.step();
    let r = harness.state_mut().ready().unwrap();
    let (_, s, line) = r.spot.result.as_ref().unwrap().as_ref().unwrap();
    assert!(s.structures.len() >= 3);
    assert_eq!(line.structures.len(), s.structures.len());
    assert!(s.spans.iter().all(|x| x.clearance >= 7.0 - 1e-9));
}

/// Criteria › Design code: a design-code plugin's cases written into the
/// project's libraries, kept in the project and used for the model.
#[test]
fn design_code_cases_go_into_the_project() {
    let dir = root().join("plugins/codes/en50341");
    let built = dir
        .join("target/wasm32-wasip2/release/en50341.wasm")
        .exists();
    assert!(
        built || ohl_core::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
        "en50341 plugin not built"
    );
    if !built {
        return;
    }
    let mut r = crate::app::Ready::new(synthetic_project("design-code"));
    let mut plugins = crate::plugins::Plugins::load(None, None);
    plugins.folders = vec![root().join("plugins/codes")];
    plugins.refresh();
    let choices = crate::codes::choices(&plugins);
    assert!(
        choices
            .iter()
            .any(|c| c.name == "en50341" && c.problem.is_none()),
        "{choices:?}"
    );
    let host = ohl_plugins::WasmHost::new().unwrap();
    let params = std::fs::read_to_string(dir.join("example-parameters.toml")).unwrap();
    let g = ohl_io::codes::generate(&host, &dir, &params).unwrap();
    let before: Vec<ohl_weather::WeatherCase> =
        r.model.cases.iter().map(|c| c.case.clone()).collect();
    let report = crate::codes::apply(&mut r, &g, "en50341").unwrap();
    assert!(report.contains("6 weather cases ("), "{report}");
    // The project's cases kept, first; only the code's new ones added
    // (those of the same physics as one of the project's not).
    let names: Vec<String> = r.model.cases.iter().map(|c| c.case.name.clone()).collect();
    let kept: Vec<String> = before.iter().map(|c| c.name.clone()).collect();
    assert_eq!(names[..kept.len()], kept[..]);
    let new = g
        .weather
        .cases
        .iter()
        .filter(|c| !before.iter().any(|b| b.same_physics(c)))
        .count();
    assert!(new > 0);
    assert_eq!(names.len(), kept.len() + new, "{names:?}");
    // The load cases name cases of the model.
    let lc_text = std::fs::read_to_string(r.model.sources.load_cases.clone().unwrap()).unwrap();
    for c in ohl_loads::LoadCases::parse(&lc_text).unwrap().cases {
        assert!(names.contains(&c.weather), "{}", c.weather);
    }
    // Generated again: every case already the project's, none added.
    let report = crate::codes::apply(&mut r, &g, "en50341").unwrap();
    assert!(
        report.contains("6 already yours, used; 0 added"),
        "{report}"
    );
    assert_eq!(r.model.cases.len(), names.len());
    let lc = r.model.sources.load_cases.clone().unwrap();
    assert!(lc.ends_with("loadcases/en50341.ohllc"), "{}", lc.display());
    let session = &r.model.editing.as_ref().unwrap().session;
    for (kind, name) in [("weather", "en50341.ohlwc"), ("loadcases", "en50341.ohllc")] {
        let files = session.library_files(kind).unwrap();
        assert!(files.iter().any(|(n, _)| n == name), "{kind}: {name}");
    }
}

/// A design code's clearance presets: written into the project's rule
/// and weather case files when it generates; the Vertical clearances
/// window lists the vertical one, Load preset takes its rules (with their
/// own half widths), its circuits, method and search distance, its 80 °C
/// case taken as the project's of the same physics (none created).
#[test]
fn design_code_clearance_presets_load_into_the_window() {
    use ohl_plugins::{ClearanceMeasure, ClearancePreset};
    let mut r = crate::app::Ready::new(synthetic_project("clearance-preset"));
    let w = "format = \"ohl-weather-cases\"\nversion = 1\n[[case]]\nname = \"a\"\ntemperature = \"5 °C\"\n";
    let l = "format = \"ohl-load-cases\"\nversion = 1\n[[case]]\nname = \"x\"\nweather = \"a\"\n";
    let hot = "format = \"ohl-weather-cases\"\nversion = 1\nname = \"p\"\n\
               [[case]]\nname = \"Preset 80 °C\"\ntemperature = \"80 °C\"\n";
    let rules = "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"Code vertical\"\n\
                 [wind]\nsteps = false\nsides = \"both\"\n\
                 [check]\nmethod = \"fe\"\nsearch_distance = \"30 m\"\n\
                 [[rule]]\nname = \"roads\"\ntarget = \"points\"\nclasses = [11]\n\
                 vertical = \"7.5 m\"\nvertical_band = \"1 m\"\nvoltages = [220.0]\ncases = [\"Preset 80 °C\"]\n\
                 [[rule]]\nname = \"water\"\ntarget = \"points\"\nclasses = [9]\n\
                 vertical = \"6 m\"\nvertical_band = \"3 m\"\nvoltages = [220.0]\ncases = [\"Preset 80 °C\"]\n";
    let g = ohl_io::codes::Generated::check(
        "code",
        w.into(),
        l.into(),
        vec![],
        vec![ClearancePreset {
            name: "vertical".into(),
            measure: ClearanceMeasure::Vertical,
            rules: rules.into(),
            weather_cases: hot.into(),
        }],
    )
    .unwrap();
    // Written next to the generated cases' files in the project.
    let work = r.model.library_dir.clone().unwrap();
    let report = crate::codes::apply(&mut r, &g, "code").unwrap();
    assert!(report.contains("code-vertical.ohlr"), "{report}");
    assert!(work.join("rules/code-vertical.ohlr").is_file());
    assert!(work.join("weather/code-vertical.ohlwc").is_file());
    // The model's cases: the project's and the code's "a" (none of the
    // project's at 5 °C); the preset's is not created.
    assert!(!r.model.cases.iter().any(|c| c.case.name == "Preset 80 °C"));
    assert!(r.model.cases.iter().any(|c| c.case.name == "a"));
    let mut win = crate::clearance_zones::ZoneWindow::new(ohl_clearance::Measure::Vertical);
    win.show(&r.model);
    let presets = crate::rule_table::project_presets_read(
        &r.model,
        crate::preset_cases::Part::of(ohl_clearance::Measure::Vertical),
    );
    assert_eq!(presets.len(), 1, "{presets:?}");
    assert_eq!(presets[0].name, "Code vertical");
    // The radial window has none.
    assert!(
        crate::rule_table::project_presets_read(
            &r.model,
            crate::preset_cases::Part::of(ohl_clearance::Measure::Radial)
        )
        .is_empty()
    );
    let mut out = crate::clearance_zones::Out::default();
    win.take_preset(
        &r.model,
        crate::rule_table::PresetPick {
            group: crate::rule_table::PROJECT_GROUP.into(),
            preset: presets[0].clone(),
            how: crate::rule_editor::Load::Replace,
        },
        &mut out,
    );
    assert_eq!(win.rules.len(), 2);
    // The first rule's half width is the window's, the other keeps its own.
    assert_eq!(win.zone.width, 1.0);
    assert_eq!((win.rules[0].width, win.rules[1].width), (None, Some(3.0)));
    assert_eq!(win.zone.search, Some(30.0));
    assert_eq!(win.check.method, Some(ohl_line::condition::Method::Fe));
    // Its 80 °C is the project's "max temperature" (the same physics):
    // nothing created, the rules naming the project's case.
    assert!(out.weather_saved.is_none());
    assert!(
        win.rules
            .iter()
            .all(|x| x.cases == vec!["max temperature".to_owned()]),
        "{:?}",
        win.rules.iter().map(|x| &x.cases).collect::<Vec<_>>()
    );
    let n = r.model.cases.len();
    crate::app::clearance_zone_out(&mut r, win.zone.measure, out);
    assert_eq!(r.model.cases.len(), n, "{:?}", r.status);
    // Kept: the window's rule file says it again.
    let text = win.rule_set().to_toml();
    assert!(text.contains("search_distance = \"30 m\""), "{text}");
    assert!(text.contains("vertical_band = \"3 m\""), "{text}");
}

/// A project preset of the Vertical clearances window whose cases the
/// project has under other names (owner, 2026-10-21): "Utility 80 °C" is
/// the project's "max temperature" (80 °C, still air): no case created,
/// the rule names "max temperature"; its "everyday" (20 °C) is not the
/// project's "everyday" (10 °C): created as "everyday (preset)", the rule
/// naming it so.
#[test]
fn a_clearance_preset_uses_the_projects_cases_of_the_same_weather() {
    let mut r = crate::app::Ready::new(synthetic_project("clearance-preset-same"));
    let work = r.model.library_dir.clone().unwrap();
    std::fs::create_dir_all(work.join("rules")).unwrap();
    std::fs::create_dir_all(work.join("weather")).unwrap();
    let rules = "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"Code vertical\"\n\
                 [[rule]]\nname = \"roads\"\ntarget = \"points\"\nclasses = [11]\n\
                 vertical = \"7.5 m\"\ncases = [\"Utility 80 °C\", \"everyday\"]\n";
    let cases = "format = \"ohl-weather-cases\"\nversion = 1\n\
                 [[case]]\nname = \"Utility 80 °C\"\ntemperature = \"80 °C\"\n\
                 [[case]]\nname = \"everyday\"\ntemperature = \"20 °C\"\n";
    std::fs::write(work.join("rules/code-same.ohlr"), rules).unwrap();
    std::fs::write(work.join("weather/code-same.ohlwc"), cases).unwrap();
    let before: Vec<String> = r.model.cases.iter().map(|c| c.case.name.clone()).collect();
    let mut win = crate::clearance_zones::ZoneWindow::new(ohl_clearance::Measure::Vertical);
    win.show(&r.model);
    let presets = crate::rule_table::project_presets_read(
        &r.model,
        crate::preset_cases::Part::of(ohl_clearance::Measure::Vertical),
    );
    let p = presets.iter().find(|p| p.name == "Code vertical").unwrap();
    let mut out = crate::clearance_zones::Out::default();
    win.take_preset(
        &r.model,
        crate::rule_table::PresetPick {
            group: crate::rule_table::PROJECT_GROUP.into(),
            preset: p.clone(),
            how: crate::rule_editor::Load::Replace,
        },
        &mut out,
    );
    assert_eq!(win.rules[0].cases, ["max temperature", "everyday (preset)"]);
    crate::app::clearance_zone_out(&mut r, win.zone.measure, out);
    let after: Vec<String> = r.model.cases.iter().map(|c| c.case.name.clone()).collect();
    assert_eq!(after.len(), before.len() + 1, "{after:?}");
    assert!(!after.iter().any(|n| n == "Utility 80 °C"), "{after:?}");
    let everyday = |n: &str| {
        r.model
            .cases
            .iter()
            .find(|c| c.case.name == n)
            .unwrap()
            .case
            .temperature
            .celsius()
    };
    assert_eq!(everyday("everyday"), 10.0);
    assert_eq!(everyday("everyday (preset)"), 20.0);
}

/// One big combined preset (40 rules of both measures, a `[vegetation]`
/// table; owner, 2026-10-21: presets are per check) in the Vertical
/// clearances window, as a project preset and as a plugin preset: only
/// its 24 vertical rules are taken, and only their cases reach the
/// project — "V hot" (80 °C) uses the project's "max temperature", "V
/// ice" and "B cold" are created; no radial or vegetation case.
#[test]
fn a_big_preset_brings_only_the_vertical_part_to_the_vertical_window() {
    let p = crate::preset_cases::tests::big_preset("");
    let mut r = crate::app::Ready::new(synthetic_project("big-preset-vertical"));
    let work = r.model.library_dir.clone().unwrap();
    std::fs::create_dir_all(work.join("rules")).unwrap();
    std::fs::create_dir_all(work.join("weather")).unwrap();
    std::fs::write(work.join("rules/code-big.ohlr"), &p.rules_text).unwrap();
    std::fs::write(work.join("weather/code-big.ohlwc"), &p.weather_text).unwrap();
    let before: Vec<String> = r.model.cases.iter().map(|c| c.case.name.clone()).collect();
    let mut win = crate::clearance_zones::ZoneWindow::new(ohl_clearance::Measure::Vertical);
    win.show(&r.model);
    let presets = crate::rule_table::project_presets_read(
        &r.model,
        crate::preset_cases::Part::of(ohl_clearance::Measure::Vertical),
    );
    let file = presets.iter().find(|x| x.name == "big").unwrap();
    let mut out = crate::clearance_zones::Out::default();
    win.take_preset(
        &r.model,
        crate::rule_table::PresetPick {
            group: crate::rule_table::PROJECT_GROUP.into(),
            preset: file.clone(),
            how: crate::rule_editor::Load::Replace,
        },
        &mut out,
    );
    assert_eq!(win.rules.len(), 24);
    assert!(
        win.rules
            .iter()
            .all(|x| x.cases.iter().all(|c| c != "V hot" && !c.starts_with('R'))),
        "{:?}",
        win.rules.iter().map(|x| &x.cases).collect::<Vec<_>>()
    );
    assert!(
        win.rules
            .iter()
            .any(|x| x.cases.contains(&"max temperature".to_string()))
    );
    crate::app::clearance_zone_out(&mut r, win.zone.measure, out);
    let after: Vec<String> = r.model.cases.iter().map(|c| c.case.name.clone()).collect();
    let added: Vec<&String> = after.iter().filter(|n| !before.contains(n)).collect();
    assert_eq!(added, ["V ice", "B cold"], "{:?}", r.status);
    // The plugin preset path takes the same part.
    let (adapted, plan) = crate::preset_cases::for_project(
        &r.model,
        &p,
        crate::preset_cases::Part::of(ohl_clearance::Measure::Vertical),
    )
    .unwrap();
    assert_eq!(adapted.rules.rules.len(), 24);
    assert_eq!(plan.names(), ["max temperature", "V ice", "B cold"]);
    // All three are the project's now: nothing more to create.
    assert!(adapted.weather.as_ref().unwrap().cases.is_empty());
}

/// Criteria › Design code… opens the window; without plugins it says how
/// to get one.
#[test]
fn the_design_code_window_opens_from_the_criteria_menu() {
    let model = synthetic_project("design-code-window");
    let mut app = App::with_model(model, None);
    app.plugins = crate::plugins::Plugins::load(None, None);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| app);
    harness.run();
    crate::tests::line_menu(&mut harness, "Criteria");
    harness.run();
    harness.get_by_label("Design code…").click();
    harness.run();
    assert!(harness.state_mut().ready().unwrap().design_code.open);
    harness.get_by_label_contains("No design-code plugin");
    harness.get_by_label("Generate");
}

/// File › Export drawings: the main route's profile from 0+100 to 0+900
/// as PDF and DXF on A3, every solved case.
#[test]
fn drawings_export_in_the_background_job() {
    let m = synthetic();
    let dir = std::env::temp_dir().join(format!("ohl-app-drawings-{}", std::process::id()));
    let w = crate::drawings::DrawingsWindow {
        route: Some(0),
        formats: vec![
            ohl_drawing::export::Format::Pdf,
            ohl_drawing::export::Format::Dxf,
        ],
        range: true,
        from: 100.0,
        to: 900.0,
        profile_only: true,
        all_cases: true,
        out: Some(dir.clone()),
        ..Default::default()
    };
    let input = crate::drawings::Input::new(&m, 0, &w).unwrap();
    assert_eq!(input.cases.len(), 5);
    let report = crate::drawings::run(&input, &|_, _| {}).unwrap();
    // On A3 (772 m per sheet) split at T3: two DXF sheets, the profile in
    // model space, one PDF of two pages.
    assert!(report.starts_with("4 files"), "{report}");
    let pdf = std::fs::read(dir.join("main-profile.pdf")).unwrap();
    assert_eq!(ohl_drawing::pdf::read_back(&pdf).unwrap().len(), 2);
    assert!(dir.join("main-profile-model.dxf").is_file());
    let _ = std::fs::remove_dir_all(&dir);
    // Without a folder it does not start.
    let none = crate::drawings::DrawingsWindow::default();
    assert!(crate::drawings::Input::new(&m, 0, &none).is_err());
}

/// Regression (D5, D6): the export window draws each other weather case
/// from its own solution (not the case shown again), the markers of the
/// last clearance check, side profiles, the plan's own scale and logos,
/// and the clearance line of the project's ground rule.
#[test]
fn drawings_draw_every_case_from_its_own_solution() {
    let m = synthetic();
    let logo = std::env::temp_dir().join(format!("ohl-app-logo-{}.svg", std::process::id()));
    std::fs::write(
        &logo,
        "<svg xmlns='http://www.w3.org/2000/svg' width='10' height='5'/>",
    )
    .unwrap();
    let rules = ohl_clearance::RuleSet::parse(
        "format = \"ohl-clearance-rules\"\nversion = 1\n[[rule]]\nname = \"Ground\"\ntarget = \"ground\"\nvertical = \"6.5 m\"",
    )
    .unwrap()
    .rules;
    let w = crate::drawings::DrawingsWindow {
        route: Some(0),
        all_cases: true,
        out: Some(std::env::temp_dir()),
        side_offsets: vec![-10.0, 10.0],
        plan_scale: Some(1000.0),
        logos: vec![(logo.clone(), 20.0)],
        ground_rules: std::sync::Arc::new(rules),
        markers: std::sync::Arc::new(vec![([150.0, 0.0, 500.0], "test".into())]),
        ..Default::default()
    };
    let input = crate::drawings::Input::new(&m, 0, &w).unwrap();
    assert_eq!(input.options.clearance, Some(6.5));
    assert_eq!(input.options.side_offsets, [-10.0, 10.0]);
    assert_eq!(input.options.plan_offset_scale, Some(1000.0));
    assert_eq!(input.options.logos.len(), 1);
    let route = input.routes[0].clone();
    let d = input.drawing(&route).unwrap();
    assert_eq!(d.sides.len(), 2);
    assert_eq!(d.markers.len(), 1);
    // The first span of S1 in each case: the dashed ones are the other
    // cases' (their sag differs from the case shown).
    let lowest =
        |t: &ohl_drawing::Trace| t.points.iter().map(|p| p[2]).fold(f64::INFINITY, f64::min);
    let spans: Vec<&ohl_drawing::Trace> = d
        .wires
        .iter()
        .filter(|t| t.label.starts_with("S1 span 0 "))
        .collect();
    let solid: Vec<f64> = spans
        .iter()
        .filter(|t| t.dash.is_none())
        .map(|t| lowest(t))
        .collect();
    let dashed: Vec<f64> = spans
        .iter()
        .filter(|t| t.dash.is_some())
        .map(|t| lowest(t))
        .collect();
    assert_eq!(solid.len(), 1);
    assert!(dashed.len() >= 4, "{dashed:?}");
    assert!(
        dashed
            .iter()
            .filter(|z| (**z - solid[0]).abs() > 0.05)
            .count()
            >= 3,
        "{solid:?} {dashed:?}"
    );
    // Without the ground rule tick: the typed clearance.
    let typed = crate::drawings::DrawingsWindow {
        clearance_rule: false,
        clearance: Some(8.0),
        ..w
    };
    assert_eq!(
        crate::drawings::Input::new(&m, 0, &typed)
            .unwrap()
            .options
            .clearance,
        Some(8.0)
    );
    let _ = std::fs::remove_file(&logo);
}

/// Edit › Versions: commit, branch (a project line), work on it and merge
/// as project operations; the model follows (re-solved) and the window lists them.
#[test]
fn versions_commit_branch_checkout_and_merge() {
    use ohl_server::Op;
    let mut m = synthetic_project("versions");
    let p = m.line.structure("T6").unwrap().position;
    for (what, op) in [
        (
            "commit",
            Op::Commit {
                message: "as imported".into(),
            },
        ),
        (
            "branch",
            Op::CreateBranch {
                name: "alt".into(),
                at: None,
            },
        ),
    ] {
        m.project_op(what, op).unwrap();
    }
    switch(&mut m, "alt");
    m.apply(
        "moved T6",
        ohl_session::Command::MoveStructure {
            id: "T6".into(),
            position: [p[0] + 4.0, p[1], p[2]],
        },
    )
    .unwrap();
    m.project_op(
        "commit",
        Op::Commit {
            message: "T6 east".into(),
        },
    )
    .unwrap();
    switch(&mut m, "main");
    assert_eq!(m.line.structure("T6").unwrap().position, p);
    assert_solved_fully(&m);
    m.project_op(
        "merge",
        Op::Merge {
            other: "alt".into(),
            message: None,
            choices: Default::default(),
        },
    )
    .unwrap();
    assert_eq!(m.line.structure("T6").unwrap().position[0], p[0] + 4.0);
    assert_solved_fully(&m);

    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|cc| App::with_model(m, cc.wgpu_render_state.clone()));
    harness.run();
    harness.state_mut().ready().unwrap().versions.open = true;
    harness.run();
    assert!(harness.query_by_label_contains("On branch main").is_some());
    assert!(harness.query_by_label("T6 east").is_some());
    assert!(harness.query_by_label("as imported").is_some());
    // The working state against the head: the same.
    harness.get_by_label("Changes").click();
    harness.run();
    harness.get_by_label("Compare").click();
    harness.run();
    assert!(harness.query_by_label_contains("no differences").is_some());
}

/// Criteria › Bimetallic conductor model: bird cage for the line, one
/// command (undoable), every section solved again.
#[test]
fn the_bimetallic_model_is_chosen_for_the_line() {
    let model = synthetic_project("bimetallic");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    crate::tests::line_menu(&mut harness, "Criteria");
    harness.run();
    harness.get_by_label("Bimetallic conductor model…").click();
    harness.run();
    harness
        .get_by_label("The outer strands take no compression (bird cage)")
        .click();
    harness.run();
    harness.get_by_label("OK").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.bimetallic_window.is_none());
    assert_eq!(
        r.model.line.bimetallic.behavior,
        ohl_cables::BimetallicBehavior::BirdCage
    );
    // Kept in the project: the session's line has it.
    let e = r.model.editing.as_ref().unwrap();
    assert_eq!(
        e.session.line().bimetallic.behavior,
        ohl_cables::BimetallicBehavior::BirdCage
    );
    // Undone like any edit.
    r.model.undo().unwrap();
    assert!(r.model.line.bimetallic.is_default());
}

/// Criteria › Loads for structures: a tick menu that stays open while two
/// weather cases are ticked in a row; each tick
/// kept in the project (the line's `structure_loads` settings row),
/// undone like any edit.
#[test]
fn loads_for_structures_are_ticked_in_a_menu_that_stays_open() {
    let model = synthetic_project("structure-loads");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    crate::tests::line_menu(&mut harness, "Criteria");
    harness.run();
    harness
        .get_by_label_contains("Loads for structures")
        .hover();
    harness.run();
    harness.get_by_label("glaze ice").click();
    harness.run();
    // Still open: the next boxes can be ticked.
    harness.get_by_label("high wind").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    let want = ohl_line::StructureLoads {
        load_cases: None,
        weather_cases: vec!["glaze ice".into(), "high wind".into()],
        condition: None,
        ..Default::default()
    };
    assert_eq!(r.model.line.structure_loads, want);
    // Kept in the project: the session's line has it.
    let e = r.model.editing.as_ref().unwrap();
    assert_eq!(e.session.line().structure_loads, want);
    // Undone like any edit (one tick each).
    r.model.undo().unwrap();
    assert_eq!(r.model.line.structure_loads.weather_cases, ["glaze ice"]);
}

/// A transverse Mercator projection centred on the synthetic line's origin
/// (invented, for the tests).
pub(super) const LOCAL_TM: &str =
    "+proj=tmerc +lat_0=46 +lon_0=8 +k=1 +x_0=0 +y_0=0 +ellps=GRS80 +units=m +no_defs";

/// The coordinate-system picker: a code typed whole is chosen, words find
/// a system by its name, a row shows code, name and unit, Enter takes
/// the first row.
#[test]
fn the_coordinate_system_picker_searches() {
    let mut h = Harness::new_ui_state(
        |ui, p: &mut crate::crs::Picker| p.ui(ui, "test"),
        crate::crs::Picker::new(""),
    );
    h.run();
    h.get_by(|n| n.role() == egui::accesskit::Role::TextInput)
        .focus();
    h.run();
    h.event(egui::Event::Text("EPSG:2056".into()));
    h.run();
    assert_eq!(h.state().spec, "EPSG:2056", "a code typed whole is chosen");
    let lv95 = ohl_crs::known_code(2056).unwrap();
    let row = crate::crs::row_text(&lv95);
    assert!(
        row.contains("2056") && row.contains(lv95.name) && row.ends_with("·  m"),
        "{row}"
    );
    h.get_by_label(&row);
    // Words of the name: a row clicked is chosen.
    let utm = ohl_crs::known_code(32632).unwrap();
    h.state_mut().filter = utm.name.to_lowercase();
    h.run();
    h.get_by_label(&crate::crs::row_text(&utm)).click();
    h.run();
    assert_eq!(h.state().spec, "EPSG:32632");
    // Enter in the search takes the first row.
    h.state_mut().filter = "lv95".into();
    h.run();
    let first = ohl_crs::search("lv95")[0].spec();
    h.get_by(|n| n.role() == egui::accesskit::Role::TextInput)
        .focus();
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(h.state().spec, first);
    // Nothing found: nothing listed, the choice stays.
    h.state_mut().filter = "no such system at all".into();
    h.run();
    h.get_by_label_contains("0 systems listed");
    assert_eq!(h.state().spec, first);
}

#[test]
fn the_coordinate_system_is_defined_converted_and_the_project_reprojected() {
    let model = synthetic_project("crs");
    let file = model.sources.file.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // Define: from the menu; the system typed as a proj4 string.
    crate::tests::line_menu(&mut harness, "Terrain");
    harness.run();
    harness.get_by_label("Coordinate system ⏵").click();
    harness.run();
    harness.get_by_label("Define…").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let d = r.crs.define.as_mut().unwrap();
        d.picker.category = crate::crs::Category::Proj4;
        d.picker.proj4 = LOCAL_TM.into();
        d.picker.spec = LOCAL_TM.into();
    }
    harness.run();
    harness.get_by_label("OK").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.crs.define.is_none());
    assert_eq!(r.model.line.crs, LOCAL_TM);
    let e = r.model.editing.as_ref().unwrap();
    assert_eq!(e.session.line().crs, LOCAL_TM, "kept in the project");
    // The calculator opens with the project's system.
    crate::tests::line_menu(&mut harness, "Terrain");
    harness.run();
    harness.get_by_label("Coordinate system ⏵").click();
    harness.run();
    harness
        .get_by_label("Coordinate system calculator…")
        .click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    let c = r.crs.calculator.as_mut().unwrap();
    assert_eq!(c.from.spec, LOCAL_TM);
    c.input = "0 0 500\n\n# a note\n1000, 2000".into();
    c.compute();
    let out: Vec<&str> = c.output.lines().collect();
    assert_eq!(out[0], "8.000000000 46.000000000 500.0000", "{}", c.output);
    assert_eq!(&out[1..3], ["", "# a note"]);
    assert!(out[3].starts_with("8.01"), "{}", c.output);
    c.input = "1 2 3 4".into();
    c.compute();
    assert!(c.output.starts_with('—'), "{}", c.output);
    r.crs.calculator = None;
    harness.run();
    // Reproject into UTM 32N: a new project, this one unchanged.
    crate::tests::line_menu(&mut harness, "Terrain");
    harness.run();
    harness.get_by_label("Coordinate system ⏵").click();
    harness.run();
    harness.get_by_label("Reproject project…").click();
    harness.run();
    assert!(harness.state_mut().ready().unwrap().crs.reproject.is_some());
    let target = file.with_file_name("moved.ohl");
    std::fs::remove_file(&target).ok();
    {
        let s = ohl_session::Session::open(&file).unwrap();
        s.put_surface(
            "tester",
            "ground",
            "test",
            &[[0.0, 0.0, 400.0], [500.0, 0.0, 410.0], [0.0, 500.0, 420.0]],
            &[[0, 1, 2]],
        )
        .unwrap();
    }
    let mut store = ohl_pointcloud::Store::open(&file).unwrap();
    let pts = (0..50).map(|k| ohl_lidar::LidarPoint {
        x: k as f64 * 3.0,
        y: 10.0,
        z: 400.0,
        class: 2,
        intensity: 7,
        return_number: 1,
        number_of_returns: 1,
    });
    store
        .import_points(
            "lidar",
            LOCAL_TM,
            None,
            64.0,
            0.001,
            pts,
            &ohl_core::budget::Budget::new(),
        )
        .unwrap();
    drop(store);
    let (from, to) = (
        ohl_crs::Crs::parse(LOCAL_TM).unwrap(),
        ohl_crs::Crs::parse("EPSG:32632").unwrap(),
    );
    let report =
        crate::crs::reproject_file(&file, &target, &from, &to, "tester", &|_, _| {}).unwrap();
    assert!(report.contains("1 surface"), "{report}");
    let t = ohl_crs::Transformation::new(&from, &to, ohl_crs::Heights::Keep);
    let old = ohl_session::Session::open(&file).unwrap();
    let new = ohl_session::Session::open(&target).unwrap();
    assert_eq!(old.line().crs, LOCAL_TM, "the old project is not changed");
    assert_eq!(new.line().crs, "EPSG:32632");
    for (a, b) in old.line().structures.iter().zip(&new.line().structures) {
        let p = t.point(a.position).unwrap();
        assert!((p[0] - b.position[0]).hypot(p[1] - b.position[1]) < 1e-6);
        assert_eq!(p[2], b.position[2]);
    }
    let (points, _) = new.surface("ground").unwrap();
    assert!((points[0][0] - t.point([0.0, 0.0, 400.0]).unwrap()[0]).abs() < 1e-6);
    let store = ohl_pointcloud::Store::open(&target).unwrap();
    let cloud = store.cloud(Some("lidar")).unwrap();
    assert_eq!((cloud.points, cloud.crs.as_str()), (50, "EPSG:32632"));
    assert!(cloud.bounds[0][0] > 400_000.0, "{:?}", cloud.bounds);
    // Not again into an existing file.
    let r = harness.state_mut().ready().unwrap();
    let err = crate::crs::reproject_check_for_tests(&r.model, "EPSG:32632", &target);
    assert!(err.unwrap_err().contains("exists already"));
}

/// The height datum in Terrain › Coordinate system › Define…, next to
/// the coordinate system (kept as it is: the project's local system is
/// no cartographic one, and OK still takes the datum): a code that is no
/// number is said and OK is off; a name and a code set the project's
/// datum (one edit, kept in the project, undone as any edit), shown in
/// the menu, the window and the properties of the project.
#[test]
fn the_height_datum_is_defined_with_the_coordinate_system() {
    let model = synthetic_project("height-datum");
    let crs = model.line.crs.clone();
    assert!(ohl_crs::Crs::parse(&crs).is_err(), "{crs}");
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    h.run();
    h.get_by_label("height datum");
    h.get_by_label("not given");
    let open_define = |h: &mut Harness<'_, App>| {
        crate::tests::line_menu(h, "Terrain");
        h.run();
        h.get_by_label("Coordinate system ⏵").click();
        h.run();
        h.get_by_label("Define…").click();
        h.run();
    };
    open_define(&mut h);
    {
        let d = h.state_mut().ready().unwrap().crs.define.as_mut().unwrap();
        assert_eq!((d.datum_name.as_str(), d.datum_epsg.as_str()), ("", ""));
        d.datum_name = "LN02".into();
        d.datum_epsg = "LN".into();
    }
    h.run();
    h.get_by_label_contains("is not an EPSG code");
    h.get_by_label("OK").click();
    h.run();
    assert!(
        h.state_mut().ready().unwrap().crs.define.is_some(),
        "OK is off"
    );
    h.state_mut()
        .ready()
        .unwrap()
        .crs
        .define
        .as_mut()
        .unwrap()
        .datum_epsg = "EPSG:5728".into();
    h.run();
    h.get_by_label("On the sheets: LN02 (EPSG:5728)");
    h.get_by_label("OK").click();
    h.run();
    let r = h.state_mut().ready().unwrap();
    assert!(r.crs.define.is_none());
    let want = Some(ohl_line::HeightDatum {
        name: "LN02".into(),
        epsg: Some(5728),
    });
    assert_eq!(r.model.line.height_datum, want);
    assert_eq!(r.model.line.crs, crs, "the system kept");
    let e = r.model.editing.as_ref().unwrap();
    assert_eq!(e.session.line().height_datum, want, "kept in the project");
    h.run();
    h.get_by_label("LN02 (EPSG:5728)");
    // Shown again when the window opens.
    open_define(&mut h);
    h.get_by_label("Height datum now: LN02 (EPSG:5728)");
    {
        let r = h.state_mut().ready().unwrap();
        let d = r.crs.define.as_ref().unwrap();
        assert_eq!(
            (d.datum_name.as_str(), d.datum_epsg.as_str()),
            ("LN02", "5728")
        );
        r.crs.define = None;
    }
    // Undone as any edit.
    let r = h.state_mut().ready().unwrap();
    assert!(r.perform(crate::edit::Action::Undo));
    assert_eq!(r.model.line.height_datum, None);
}

#[test]
#[ignore = "writes images of the coordinate system windows to OHL_SHOTS (a look by eye)"]
fn coordinate_system_window_images() {
    let Ok(dir) = ohl_core::env::var("OHL_SHOTS") else {
        return;
    };
    let model = synthetic_project("crs-shots");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let shot = |h: &mut Harness<'_, App>, name: &str| {
        h.run();
        h.render()
            .unwrap()
            .save(format!("{dir}/{name}.png"))
            .unwrap();
    };
    {
        let r = harness.state_mut().ready().unwrap();
        let mut p = crate::crs::Picker::new("EPSG:2056");
        p.filter = "LV95".into();
        let mut d = crate::crs::Define::new(&r.model);
        d.picker = p;
        d.datum_name = "LN02".into();
        d.datum_epsg = "5728".into();
        r.crs.define = Some(d);
    }
    shot(&mut harness, "define");
    {
        let r = harness.state_mut().ready().unwrap();
        r.crs.define = None;
        let mut c = crate::crs::Calculator::new(&r.model);
        c.from = crate::crs::Picker::new(LOCAL_TM);
        c.input = "0 0 500\n1000 2000".into();
        c.compute();
        r.crs.calculator = Some(c);
    }
    shot(&mut harness, "calculator");
    {
        let r = harness.state_mut().ready().unwrap();
        r.crs.calculator = None;
        r.crs.reproject = Some(crate::crs::Reproject::new());
    }
    shot(&mut harness, "reproject");
}
