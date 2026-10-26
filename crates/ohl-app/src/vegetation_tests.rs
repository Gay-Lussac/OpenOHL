// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The vegetation panel: every setting explained and drawn, grouped, with
//! links to the user guide; a dockable panel; its layers drawn while it is
//! open and taken off when it closes; colours and classes chosen; a run
//! saved in the performed checks and reopened (out of date once the
//! points change); a design-code preset (classes, distances, weather
//! cases) chosen from a dropdown grouped by plugin, previewed and applied.

use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

use crate::app::{App, Tab};
use crate::dockable::Panel;
use crate::lidar_tests::{harness, load, project_with_cloud};

/// Steps until `done` (60 s at most).
fn until(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&mut App) -> bool) {
    let started = Instant::now();
    loop {
        h.step();
        if done(h.state_mut()) {
            h.step();
            return;
        }
        assert!(started.elapsed() < Duration::from_secs(60), "{what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A project with a cloud around its first structure, the panel open on
/// the first case, its clearance so large that every vegetation point
/// grows in.
fn opened(name: &str) -> (std::path::PathBuf, Harness<'static, App>) {
    let (file, _) = project_with_cloud(name, 20);
    let mut h = harness(load(&file), false);
    h.run_steps(3);
    let r = h.state_mut().ready().unwrap();
    r.vegetation.show(0);
    r.vegetation.settings.clearance = ohl_clearance::rules::Requirement::Fixed(80.0);
    h.run_steps(3);
    (file, h)
}

fn run(h: &mut Harness<'_, App>) {
    h.get_by_label("Run").click();
    until(h, "a vegetation run", |a| {
        let r = a.ready().unwrap();
        matches!(r.vegetation.result(), (Some(_), false))
    });
    let r = h.state_mut().ready().unwrap();
    assert!(
        matches!(r.vegetation.result().0, Some(Ok(_))),
        "{:?}",
        r.vegetation.result().0.map(|x| x.as_ref().err())
    );
}

/// Every setting has its explanation beside it, its group heading with a
/// link to the user guide, and the drawings; a link opens the guide at
/// its section.
#[test]
fn every_setting_is_explained_and_drawn() {
    let (file, mut h) = opened("veg-explained");
    for heading in ["Wire positions", "Grow-in", "Fall-in", "Search"] {
        h.get_by_label(heading);
        h.get_by_label(&format!("{heading}: user guide ↗"));
    }
    for drawing in [
        "drawing: falling tree",
        "drawing: grow-in",
        "drawing: wind positions",
        "drawing: tree base",
        "drawing: unbalanced ice",
    ] {
        h.get_by_label(drawing);
    }
    for text in [
        "Radially from the conductor's surface",
        "Growth until the next cut",
        "Added to the clearance",
        "The tree turns about the edge of its root ball",
        "What a falling tree must keep from the wire",
        "Points lower than the smallest are not trees",
        "The base is taken from the ground only",
        "Half width of the corridor read",
        "None ticked: each case's own pressure",
        "The span checked carries more ice",
        "The LiDAR classes checked as vegetation",
    ] {
        h.get_by_label_contains(text);
    }
    h.get_by_label("Fall-in: user guide ↗").click();
    h.run_steps(2);
    let status = h.state_mut().ready().unwrap().status.clone();
    let opened = crate::help::OPENED.lock().unwrap().clone();
    assert!(
        opened
            .iter()
            .any(|p| p.display().to_string().contains("analyses")),
        "{opened:?} {status:?}"
    );
    assert!(
        status
            .as_ref()
            .is_some_and(|s| s.0.contains("vegetation-fall-in")),
        "{status:?}"
    );
    // The drawings follow the values typed: k and A change, still drawn.
    let r = h.state_mut().ready().unwrap();
    r.vegetation.settings.root_ball = 0.3;
    r.vegetation.settings.allowance = ohl_clearance::rules::Requirement::Fixed(8.0);
    r.vegetation.settings.steps.stepped = true;
    h.run_steps(2);
    h.get_by_label("drawing: falling tree");
    std::fs::remove_dir_all(file.parent().unwrap()).ok();
}

/// The panel docks and floats; its run's points are drawn while it is
/// open (docked or not), taken off when it closes, drawn again when it
/// opens; the colours chosen are the layers' and kept with the view
/// settings; the classes ticked are the run's.
#[test]
fn the_highlights_are_drawn_while_the_panel_is_open() {
    let (file, mut h) = opened("veg-highlights");
    run(&mut h);
    h.run_steps(2);
    let drawn = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        (r.view3d.vegetation.grow.len(), r.plan.vegetation.grow.len())
    };
    let (n, m) = drawn(&mut h);
    assert!(n > 0 && n == m, "{n} {m}");
    let r = h.state_mut().ready().unwrap();
    if let (Some(Ok(rep)), _) = r.vegetation.result() {
        assert_eq!(rep.grow.len(), n, "every grow-in point drawn");
    }
    // Docked: still drawn.
    h.get_by_label("Dock as a panel").click();
    h.run_steps(3);
    assert!(
        h.state_mut()
            .dock()
            .find_tab(&Tab::Tool(Panel::Vegetation))
            .is_some()
    );
    assert_eq!(drawn(&mut h).0, n);
    // Its tab closed: the panel closes, its points go.
    crate::dockable::host(&h.ctx, |x| x.closed.insert(Panel::Vegetation));
    h.run_steps(3);
    assert!(!h.state_mut().ready().unwrap().vegetation.open);
    assert_eq!(drawn(&mut h), (0, 0), "taken off when closed");
    // Opened again: drawn again.
    h.state_mut().ready().unwrap().vegetation.show(0);
    h.run_steps(3);
    assert_eq!(drawn(&mut h).0, n);
    // A colour chosen: the layers' (and the view settings').
    h.get_by_label("colour of the grow-in points")
        .click_accesskit();
    h.run_steps(2);
    h.get_by_label_contains("Colour of the grow-in points");
    let r = h.state_mut().ready().unwrap();
    crate::vegetation::handle(
        r,
        crate::vegetation::Out {
            colors: Some([[1, 2, 3], [4, 5, 6]]),
            ..Default::default()
        },
    );
    h.run_steps(2);
    let r = h.state_mut().ready().unwrap();
    assert_eq!(r.view3d.vegetation.colors, [[1, 2, 3], [4, 5, 6]]);
    assert_eq!(
        r.model.view_prefs().vegetation_colors,
        Some([[1, 2, 3], [4, 5, 6]])
    );
    // The classes: the menu's ticks (by role: 3, 4, 5 of the ASPRS
    // scheme), one more ticked.
    let shown = h
        .query_all_by_label_contains("by role: vegetation")
        .next()
        .expect("the classes menu")
        .accesskit_node()
        .label()
        .unwrap_or_default();
    // (By its accessibility action: the colour window may lie over it.)
    h.get_by_label(&shown).click_accesskit();
    h.run_steps(2);
    let ground = h
        .query_all_by_label_contains("2 ")
        .find(|n| {
            n.accesskit_node()
                .label()
                .is_some_and(|l| l.starts_with("2 ") && l.contains("round"))
        })
        .expect("class 2 in the menu");
    ground.click();
    h.run_steps(2);
    let r = h.state_mut().ready().unwrap();
    let mut want = r.model.scheme.codes_with_role("vegetation");
    want.push(2);
    want.sort_unstable();
    assert_eq!(r.vegetation.settings.classes, want);
    std::fs::remove_dir_all(file.parent().unwrap()).ok();
}

/// A run saved in the performed checks (its settings, cases, results and
/// every point), read back the same; reopened from the library its points
/// are drawn; out of date once the points changed since.
#[test]
fn a_saved_run_reads_back_and_tells_when_the_points_changed() {
    let (file, mut h) = opened("veg-saved");
    run(&mut h);
    let before = {
        let r = h.state_mut().ready().unwrap();
        r.vegetation.result().0.unwrap().as_ref().unwrap().clone()
    };
    assert!(!before.grow.is_empty());
    {
        let r = h.state_mut().ready().unwrap();
        r.vegetation.set_save_name("trees 1");
    }
    h.run_steps(2);
    // Under the results (above the Run footer, scrolled out of sight).
    h.get_by_label("Save in Performed checks").click_accesskit();
    h.run_steps(3);
    let saved = {
        let r = h.state_mut().ready().unwrap();
        r.model.session().unwrap().report("trees 1").expect("saved")
    };
    assert_eq!(saved.check, crate::vegetation::CHECK_ID);
    let (back, _) = crate::vegetation::saved::read(&saved).unwrap();
    assert_eq!(back.spans, before.spans);
    assert_eq!(back.labels, before.labels);
    assert_eq!(back.settings, before.settings);
    assert_eq!(back.cases, before.cases);
    assert_eq!(back.grow.len(), before.grow.len());
    for (a, b) in back.grow.iter().zip(&before.grow) {
        for k in 0..3 {
            assert!((a.point[k] - b.point[k]).abs() <= 0.0005);
        }
        assert_eq!(a.label, b.label);
    }
    // Reopened from the library, in a fresh panel: drawn, up to date.
    {
        let r = h.state_mut().ready().unwrap();
        r.vegetation = Default::default();
        r.check_library_out(crate::check_library::Out {
            open: Some(saved.clone()),
            ..Default::default()
        });
    }
    h.run_steps(3);
    {
        let r = h.state_mut().ready().unwrap();
        assert!(r.vegetation.open);
        assert_eq!(r.view3d.vegetation.grow.len(), before.grow.len());
        assert_eq!(r.vegetation.out_of_date(&r.model), None);
    }
    h.get_by_label_contains("saved run \"trees 1\"");
    // A point changed since: reopened, out of date.
    {
        let r = h.state_mut().ready().unwrap();
        let p = *r.model.points.iter().find(|p| p.1 == 2).unwrap();
        let mut f = crate::lidar_edit::PointsForm::of(&r.model, &[p]).unwrap();
        f.points[0].class = 1;
        let (what, command) = f.command().unwrap();
        assert!(r.perform(crate::edit::Action::Command { what, command }));
        r.vegetation = Default::default();
        r.check_library_out(crate::check_library::Out {
            open: Some(saved),
            ..Default::default()
        });
    }
    h.run_steps(3);
    let r = h.state_mut().ready().unwrap();
    assert_eq!(
        r.vegetation.out_of_date(&r.model).as_deref(),
        Some("the LiDAR points")
    );
    std::fs::remove_dir_all(file.parent().unwrap()).ok();
}

/// The presets of two design-code plugins (invented, as a plugin returns
/// them): the dropdown lists them grouped by plugin (a plugin without a
/// vegetation preset says so); the vegetation preset previewed — its
/// classes 42 and 43, its clearance by voltage, which weather case exists
/// and which is created — then applied: the values in, the missing case
/// created (one undo step) and both ticked.
#[test]
fn a_plugin_preset_is_chosen_previewed_and_applied() {
    let (file, mut h) = opened("veg-preset");
    let head = "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"p\"\n";
    let trees = format!(
        "{head}[wind]\nsteps = true\nstep = \"100 Pa\"\n[vegetation]\nclasses = [42, 43]\n\
         clearance = {{ by_voltage = [[0, \"2.5 m\"], [220, \"7 m\"], [380, \"9 m\"]] }}\n\
         root_ball = 0.05\nfall_allowance = \"2 m\"\nmax_base_ground_distance = \"5 m\"\n\
         search_distance = \"50 m\"\n"
    );
    let cases = "format = \"ohl-weather-cases\"\nversion = 1\n\
                 [[case]]\nname = \"max temperature\"\ndescription = \"maximum conductor temperature\"\n\
                 tags = [\"clearance\"]\ntemperature = \"80 °C\"\n\
                 [[case]]\nname = \"Test warm wind\"\ntemperature = \"60 °C\"\nwind_pressure = \"200 Pa\"\n";
    let ground = format!("{head}[[rule]]\nname = \"g\"\ntarget = \"ground\"\nvertical = \"7 m\"\n");
    let preset = |name: &str, rules: &str, weather: &str| ohl_plugins::RulePreset {
        name: name.into(),
        description: format!("{name}: invented test values"),
        rules: rules.into(),
        weather_cases: weather.into(),
    };
    let list = vec![
        crate::rule_editor::PluginPresets {
            plugin: "code-a".into(),
            presets: ohl_io::codes::check_rule_presets(
                "code-a",
                vec![
                    preset("ground", &ground, ""),
                    preset("trees", &trees, cases),
                ],
            ),
        },
        crate::rule_editor::PluginPresets {
            plugin: "code-b".into(),
            presets: ohl_io::codes::check_rule_presets(
                "code-b",
                vec![preset("ground only", &ground, "")],
            ),
        },
    ];
    {
        let w = &mut h.state_mut().ready().unwrap().rule_editor.presets;
        w.list = list;
        w.asked = true;
    }
    h.run_steps(2);
    // The dropdown: both plugins, the vegetation preset of one.
    h.get_by_value("Choose a preset…").click();
    h.run_steps(2);
    h.get_by_label("code-a");
    h.get_by_label("code-b");
    h.get_by_label("   trees").click();
    h.run_steps(2);
    assert!(
        h.query_all_by_label_contains("no preset for this").count() == 0,
        "the dropdown closed on a choice"
    );
    assert_eq!(
        h.state_mut().ready().unwrap().vegetation.preset,
        Some((0, 1))
    );
    // The preview.
    h.get_by_label_contains("weather case \"max temperature\": uses your case \"max temperature\"");
    h.get_by_label_contains("weather case \"Test warm wind\": creates \"Test warm wind\"");
    assert!(h.query_all_by_label_contains("42").count() >= 1);
    h.get_by_label("Apply preset").click();
    h.run_steps(3);
    let entries = {
        let r = h.state_mut().ready().unwrap();
        let s = &r.vegetation.settings;
        assert_eq!(s.classes, [42, 43]);
        assert_eq!(s.clearance.at(380.0), Ok(9.0));
        assert_eq!(s.clearance.at(220.0), Ok(7.0));
        assert_eq!(s.root_ball, 0.05);
        assert_eq!(s.base_limit, (true, 5.0));
        assert_eq!(s.search, 50.0);
        assert!(s.steps.stepped && s.steps.step == 100.0);
        let names: Vec<&str> = r
            .vegetation
            .cases
            .iter()
            .map(|&k| r.model.cases[k].case.name.as_str())
            .collect();
        assert_eq!(
            names,
            ["max temperature", "Test warm wind"],
            "{:?}",
            r.status
        );
        r.model.session().unwrap().log().unwrap().len()
    };
    // The table stays editable after.
    h.state_mut().ready().unwrap().vegetation.settings.clearance =
        ohl_clearance::rules::Requirement::ByVoltage(vec![(220.0, 7.5), (380.0, 9.5)]);
    // One undo step takes the created case back.
    let r = h.state_mut().ready().unwrap();
    r.perform(crate::edit::Action::Undo);
    h.run_steps(3);
    let r = h.state_mut().ready().unwrap();
    assert!(
        !r.model
            .cases
            .iter()
            .any(|c| c.case.name == "Test warm wind")
    );
    assert_eq!(r.model.session().unwrap().log().unwrap().len(), entries + 1);
    std::fs::remove_dir_all(file.parent().unwrap()).ok();
}

/// A vegetation preset whose cases the project has under other names
/// (owner, 2026-10-21): "Utility vegetation 80 °C" is the project's "max
/// temperature" (80 °C, still air) — used and ticked, nothing created;
/// its "everyday" (20 °C) is not the project's (10 °C) — created as
/// "everyday (preset)", the preview saying so. Undo takes everything
/// back.
#[test]
fn a_preset_uses_the_projects_cases_of_the_same_weather() {
    let (file, mut h) = opened("veg-preset-same");
    let rules = "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"p\"\n\
                 [vegetation]\nclasses = [5]\n";
    let cases = "format = \"ohl-weather-cases\"\nversion = 1\n\
                 [[case]]\nname = \"Utility vegetation 80 °C\"\ntemperature = \"80 °C\"\n\
                 [[case]]\nname = \"everyday\"\ntemperature = \"20 °C\"\n";
    {
        let r = h.state_mut().ready().unwrap();
        let w = &mut r.rule_editor.presets;
        w.list = vec![crate::rule_editor::PluginPresets {
            plugin: "code-a".into(),
            presets: ohl_io::codes::check_rule_presets(
                "code-a",
                vec![ohl_plugins::RulePreset {
                    name: "trees".into(),
                    description: "invented test values".into(),
                    rules: rules.into(),
                    weather_cases: cases.into(),
                }],
            ),
        }];
        w.original = w.list.clone();
        w.asked = true;
        r.vegetation.preset = Some((0, 0));
    }
    h.run_steps(2);
    let (names, entries) = {
        let r = h.state_mut().ready().unwrap();
        let names: Vec<String> = r.model.cases.iter().map(|c| c.case.name.clone()).collect();
        (names, r.model.session().unwrap().log().unwrap().len())
    };
    h.get_by_label_contains(
        "weather case \"Utility vegetation 80 °C\": uses your case \"max temperature\"",
    );
    h.get_by_label_contains(
        "weather case \"everyday\": creates \"everyday (preset)\" (your \"everyday\" has other",
    );
    h.get_by_label("Apply preset").click();
    h.run_steps(3);
    let steps = {
        let r = h.state_mut().ready().unwrap();
        let now: Vec<&str> = r.model.cases.iter().map(|c| c.case.name.as_str()).collect();
        assert_eq!(now.len(), names.len() + 1, "{now:?}");
        assert!(!now.contains(&"Utility vegetation 80 °C"), "{now:?}");
        let ticked: Vec<&str> = r
            .vegetation
            .cases
            .iter()
            .map(|&k| r.model.cases[k].case.name.as_str())
            .collect();
        assert_eq!(
            ticked,
            ["max temperature", "everyday (preset)"],
            "{:?}",
            r.status
        );
        let everyday = r
            .model
            .cases
            .iter()
            .find(|c| c.case.name == "everyday")
            .unwrap();
        assert_eq!(everyday.case.temperature.celsius(), 10.0);
        r.model.session().unwrap().log().unwrap().len() - entries
    };
    // One change: Undo gives the project's cases as before.
    assert_eq!(steps, 1);
    let r = h.state_mut().ready().unwrap();
    r.perform(crate::edit::Action::Undo);
    h.run_steps(3);
    let r = h.state_mut().ready().unwrap();
    let back: Vec<String> = r.model.cases.iter().map(|c| c.case.name.clone()).collect();
    assert_eq!(back, names);
    std::fs::remove_dir_all(file.parent().unwrap()).ok();
}

/// One big combined preset (40 clearance rules and a `[vegetation]`
/// table; owner, 2026-10-21: presets are per check): the vegetation panel
/// lists it, takes no rule and brings only its own cases — "Veg hot"
/// (80 °C) uses the project's "max temperature", "Veg wind" is created;
/// none of the clearance rules' cases.
#[test]
fn the_vegetation_panel_takes_only_its_part_of_a_big_preset() {
    let (file, mut h) = opened("veg-preset-big");
    {
        let r = h.state_mut().ready().unwrap();
        let w = &mut r.rule_editor.presets;
        w.list = vec![crate::rule_editor::PluginPresets {
            plugin: "code-big".into(),
            presets: Ok(vec![crate::preset_cases::tests::big_preset("")]),
        }];
        w.original = w.list.clone();
        w.asked = true;
        r.vegetation.preset = Some((0, 0));
    }
    h.run_steps(2);
    let names: Vec<String> = {
        let r = h.state_mut().ready().unwrap();
        r.model.cases.iter().map(|c| c.case.name.clone()).collect()
    };
    h.get_by_label_contains("weather case \"Veg hot\": uses your case \"max temperature\"");
    h.get_by_label_contains("weather case \"Veg wind\": creates \"Veg wind\"");
    assert_eq!(h.query_all_by_label_contains("\"V hot\"").count(), 0);
    h.get_by_label("Apply preset").click();
    h.run_steps(3);
    let r = h.state_mut().ready().unwrap();
    let added: Vec<String> = r
        .model
        .cases
        .iter()
        .map(|c| c.case.name.clone())
        .filter(|n| !names.contains(n))
        .collect();
    assert_eq!(added, ["Veg wind"], "{:?}", r.status);
    let ticked: Vec<&str> = r
        .vegetation
        .cases
        .iter()
        .map(|&k| r.model.cases[k].case.name.as_str())
        .collect();
    assert_eq!(ticked, ["max temperature", "Veg wind"]);
    assert_eq!(r.vegetation.settings.classes, [5]);
    std::fs::remove_dir_all(file.parent().unwrap()).ok();
}
