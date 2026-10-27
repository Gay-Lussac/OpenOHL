// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: the clearance checks and their windows, vegetation from a point
//! cloud.

use super::*;

pub(super) fn clearance_input(
    m: &model::Model,
    rules: Vec<ohl_clearance::Rule>,
) -> crate::clearance::Input {
    crate::clearance::Input {
        line: m.line.clone(),
        case: m.cases[0].case.name.clone(),
        shapes: m.cases[0].shapes.clone(),
        jumpers: m.cases[0].jumpers.clone(),
        rules,
        scheme: m.scheme.clone(),
        cloud: m.cloud.clone(),
        ground: m.ground.clone(),
        surfaces: Default::default(),
        corridor: 60.0,
        check: Default::default(),
        progress: Default::default(),
        cancel: Default::default(),
        points: 0,
    }
}

/// A ground rule far above the wires finds every span; one far below,
/// none.
#[test]
fn clearance_violations_of_the_shown_case() {
    let m = synthetic();
    let high = crate::clearance::run(&clearance_input(
        &m,
        ohl_clearance::RuleSet::ground(100.0).rules,
    ))
    .unwrap();
    let spans: usize = m.cases[0].shapes.iter().map(|s| s.spans.len()).sum();
    let wires: std::collections::BTreeSet<String> =
        high.findings.iter().map(|f| f.wire.describe()).collect();
    assert!(wires.len() >= spans, "{} of {spans}", wires.len());
    for f in &high.findings {
        assert!(f.distance < f.required);
        assert_eq!(f.measure, "vertical");
        // The target is the ground under the wire.
        assert!(f.target[2] < f.at[2]);
    }
    let low = crate::clearance::run(&clearance_input(
        &m,
        ohl_clearance::RuleSet::ground(0.5).rules,
    ))
    .unwrap();
    assert!(low.findings.is_empty(), "{:?}", low.findings);
    assert_eq!(low.rules.len(), 1);
    // The circuits chosen (owner, 2026-10-27: as in the windows): only
    // their wires; one the line has not: refused.
    let circuit = m.line.sections[0].circuit_name.clone();
    let mut input = clearance_input(&m, ohl_clearance::RuleSet::ground(100.0).rules);
    input.check.circuits = vec![circuit.clone()];
    let one = crate::clearance::run(&input).unwrap();
    assert!(!one.findings.is_empty() && one.findings.len() <= high.findings.len());
    input.check.circuits = vec!["no such circuit".into()];
    assert!(crate::clearance::run(&input).is_err());
}

/// The Clearances tab finds the example rules next to the model; checks run
/// on demand (Checks › Run checks), not at start; the violations are drawn.
#[test]
fn the_checks_menu_runs_the_clearance_check() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.run();
    assert!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .clearances
            .report
            .is_none()
    );
    harness.get_by_label("Checks").click();
    harness.run();
    harness.get_by_label_contains("Wire clearances").hover();
    harness.run();
    // The menu's (the quick bar has one too).
    harness
        .query_all_by_label("Check wire clearances")
        .last()
        .unwrap()
        .click();
    let started = std::time::Instant::now();
    loop {
        harness.step();
        let r = harness.state_mut().ready().unwrap();
        if r.clearances.report.is_some() && !r.clearances.running() {
            break;
        }
        assert!(started.elapsed().as_secs() < 60, "no report in 60 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let r = harness.state_mut().ready().unwrap();
    assert!(r.clearances.files[0].ends_with("clearance-rules.ohlr"));
    let rep = r.clearances.report.clone().unwrap().unwrap();
    assert_eq!(r.view3d.marks.len(), rep.findings.len());
    // Rules for other lines and across cases are noted, not run.
    assert!(
        rep.notes.iter().any(|n| n.contains("Crossed lines")),
        "{:?}",
        rep.notes
    );
    // Run, in the tab's footer.
    harness.get_by_label("Run");
}

#[test]
fn the_checks_menu_opens_the_galloping_window() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    harness.get_by_label("Checks").click();
    harness.run();
    harness.get_by_label("Galloping ellipses…").click();
    harness.run();
    // The example parameters next to the line are chosen; the glaze ice
    // case is the one they name.
    let r = harness.state_mut().ready().unwrap();
    assert!(r.galloping.open);
    assert!(
        r.galloping
            .file
            .as_ref()
            .is_some_and(|f| f.ends_with("galloping-example.ohlg"))
    );
    r.galloping.case = r
        .model
        .cases
        .iter()
        .position(|c| c.case.name == "glaze ice")
        .unwrap();
    harness.run();
    harness.get_by_label("Run").click();
    let started = std::time::Instant::now();
    loop {
        harness.step();
        let r = harness.state_mut().ready().unwrap();
        if let (Some(rep), false) = r.galloping.report() {
            let rep = rep.as_ref().unwrap();
            // Two phases on T1–T5: four spans, two methods.
            assert_eq!(rep.rows.len(), 8);
            assert!(rep.notes.is_empty(), "{:?}", rep.notes);
            break;
        }
        assert!(started.elapsed().as_secs() < 60, "no report in 60 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    // A pair's span link goes to the span.
    harness
        .query_all_by_label("S1 span 1")
        .next()
        .unwrap()
        .click();
    harness.run();
    assert_eq!(
        harness.state_mut().ready().unwrap().selection,
        Selection::Span {
            section: "S1".into(),
            span: 1
        }
    );
}

/// Opens Checks › Ground clearances › `item`.
pub(super) fn open_clearance_window(harness: &mut Harness<'_, App>, item: &str) {
    harness.get_by_label("Checks").click();
    harness.run();
    harness.get_by_label_contains("Ground clearances").hover();
    harness.run();
    harness.get_by_label(item).click();
    harness.run();
}

/// Checks › Vertical clearances: the project's rule file
/// next to the model; a ground rule run with the wind stepped finds every
/// span, the violations are drawn, a span link goes to it.
#[test]
fn the_vertical_clearances_window_runs_with_the_wind_stepped() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1600.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    open_clearance_window(&mut harness, "Vertical clearances…");
    let dir = std::env::temp_dir().join(format!("ohl-app-{}-zones-v", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let w = &mut r.vertical_clearances;
        assert!(w.open && !r.radial_clearances.open);
        assert!(
            w.file
                .as_ref()
                .is_some_and(|f| f.ends_with("synthetic-line.vertical-clearances.ohlr")),
            "{:?}",
            w.file
        );
        // It starts with the ground's rule, its distance to set: not run.
        assert_eq!(w.rules, vec![crate::clearance_zones::first_rule()]);
        assert!(w.why_not_run().is_some());
        w.rules.clear();
        // Kept in a scratch folder, not next to the example.
        w.file = Some(dir.join("rules.ohlr"));
        w.wind = ohl_clearance::WindSteps {
            stepped: true,
            step: 200.0,
            sides: ohl_clearance::WindSides::Both,
            arc: false,
            ..Default::default()
        };
        w.rules.push(ohl_clearance::ZoneRule {
            name: "High ground".into(),
            voltages: vec![220.0],
            cases: vec!["high wind".into()],
            distance: ohl_clearance::Requirement::Fixed(100.0),
            classes: Vec::new(),
            ground: true,
            surface: None,
            wires: ohl_clearance::Wires::Sections,
            width: None,
            screen: false,
        });
    }
    harness.run();
    harness.get_by_label("Run").click();
    let started = std::time::Instant::now();
    let worst = loop {
        harness.step();
        let r = harness.state_mut().ready().unwrap();
        if let (Some(rep), false) = r.vertical_clearances.result() {
            let rep = rep.as_ref().unwrap();
            // 600 Pa in steps of 200 Pa, from both sides, and no wind.
            assert_eq!(rep.states.len(), 1 + 3 * 2);
            let spans: usize = r.model.cases[0].shapes.iter().map(|s| s.spans.len()).sum();
            assert_eq!(rep.zone.findings.len(), spans);
            assert!(rep.zone.findings.iter().all(|f| f.class.is_none()));
            assert_eq!(r.view3d.marks.len(), spans);
            // Drawn vertical (owner, 2026-10-22): each mark from the
            // ground point straight up to the wire's height — the same x
            // and y at both ends, in the 3D view (and so the cross-section
            // panel), the plan and the profile.
            for ((top, ground), f) in r.view3d.marks.iter().zip(&rep.zone.findings) {
                assert_eq!([top[0], top[1]], [ground[0], ground[1]]);
                assert_eq!(*ground, f.target);
                assert_eq!(top[2], f.wire[2]);
                assert!(top[2] > ground[2]);
            }
            assert_eq!(r.profile.marks, r.view3d.marks);
            let plan: Vec<[f64; 3]> = r.view3d.marks.iter().map(|m| m.0).collect();
            assert_eq!(r.plan.marks, plan);
            let f = &rep.zone.findings[0];
            break Selection::Span {
                section: f.section.clone(),
                span: f.span,
            };
        }
        assert!(started.elapsed().as_secs() < 60, "no report in 60 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    harness.run();
    // The worst first: its link goes to it.
    harness.get_by_label(&worst.describe()).click();
    harness.run();
    assert_eq!(harness.state_mut().ready().unwrap().selection, worst);
    std::fs::remove_dir_all(dir).ok();
}

/// Checks › Vertical clearances with the swing arc: the window says how
/// many solves a windy case needs (and suggests the arc for fine steps),
/// runs the arc (three positions), names the arc and its angle in the
/// results; its groups link to the user guide.
#[test]
fn the_vertical_clearances_window_runs_the_swing_arc() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1600.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    open_clearance_window(&mut harness, "Vertical clearances…");
    let dir = std::env::temp_dir().join(format!("ohl-app-{}-zones-arc", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let w = &mut r.vertical_clearances;
        w.rules.clear();
        w.file = Some(dir.join("rules.ohlr"));
        w.wind = ohl_clearance::WindSteps {
            stepped: true,
            step: 10.0,
            sides: ohl_clearance::WindSides::Both,
            arc: false,
            ..Default::default()
        };
        w.rules.push(ohl_clearance::ZoneRule {
            name: "High ground".into(),
            voltages: Vec::new(),
            cases: vec!["high wind".into()],
            distance: ohl_clearance::Requirement::Fixed(100.0),
            classes: Vec::new(),
            ground: true,
            surface: None,
            wires: ohl_clearance::Wires::Sections,
            width: None,
            screen: false,
        });
    }
    harness.run();
    // 600 Pa in 10 Pa steps: 1 + 59 × 2 + 2 solves, the arc suggested.
    let text = harness
        .query_all_by_label_contains("121 solves for a windy case of 600 Pa")
        .count();
    assert!(text > 0);
    assert!(
        harness
            .query_all_by_label_contains("The swing arc needs 3")
            .next()
            .is_some()
    );
    for g in [
        "Wind: user guide ↗",
        "Zone: user guide ↗",
        "Results: user guide ↗",
    ] {
        harness.get_by_label(g);
    }
    {
        let w = &mut harness.state_mut().ready().unwrap().vertical_clearances;
        w.wind.stepped = false;
        w.wind.arc = true;
    }
    harness.run();
    // The band's margin beside the arc (2026-10-21), 0.15 m by default.
    harness.get_by_label("band margin");
    assert!(
        harness
            .query_all_by_label_contains("3 solves for a windy case of 600 Pa")
            .next()
            .is_some()
    );
    harness.get_by_label("Run").click();
    let started = std::time::Instant::now();
    loop {
        harness.step();
        let r = harness.state_mut().ready().unwrap();
        if let (Some(rep), false) = r.vertical_clearances.result() {
            let rep = rep.as_ref().unwrap();
            // Still air and the two blowouts; every span's arc.
            assert_eq!(rep.states.len(), 3);
            assert!(rep.arc_spans > 0);
            assert!(rep.zone.findings.iter().any(|f| f.arc.is_some()));
            break;
        }
        assert!(started.elapsed().as_secs() < 120, "no report in 120 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    harness.run();
    assert!(harness.query_all_by_label_contains("arc ").next().is_some());
    std::fs::remove_dir_all(dir).ok();
}

/// The time left to solve: none before the first position, then the
/// mean time of those solved times those left.
#[test]
fn the_clearance_windows_say_the_time_left() {
    use crate::clearance_zones::time_left;
    assert_eq!(time_left(3.0, 0, 10), None);
    assert_eq!(time_left(3.0, 3, 10), Some(7.0));
    assert_eq!(time_left(3.0, 10, 10), None);
}

/// Checks › Radial clearances: a rule added is kept in the
/// project's rule file; removing it asks first.
#[test]
fn the_radial_clearances_window_keeps_its_rules() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1600.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    open_clearance_window(&mut harness, "Radial clearances…");
    let dir = std::env::temp_dir().join(format!("ohl-app-{}-zones-r", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("rules.ohlr");
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.radial_clearances.open && !r.vertical_clearances.open);
        r.radial_clearances.file = Some(file.clone());
    }
    harness.run();
    // The ground's rule to start with (its distance to set): the only
    // rule, not removed.
    assert_eq!(
        harness.state_mut().ready().unwrap().radial_clearances.rules,
        vec![crate::clearance_zones::first_rule()]
    );
    {
        use egui_kittest::kittest::NodeT as _;
        assert!(
            harness
                .get_by_label("Remove")
                .accesskit_node()
                .is_disabled()
        );
    }
    harness.get_by_label("Add rule").click();
    harness.run();
    harness.run();
    let set = ohl_clearance::RuleSet::load(&file).unwrap();
    assert_eq!(
        set.rules.len(),
        0,
        "a rule without targets or distance is not written"
    );
    assert_eq!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .radial_clearances
            .rules
            .len(),
        2
    );
    {
        let w = &mut harness.state_mut().ready().unwrap().radial_clearances;
        w.rules.truncate(1);
        w.rules[0].distance = ohl_clearance::Requirement::Fixed(5.0);
    }
    harness.run();
    harness.get_by_label("Duplicate").click();
    harness.run();
    harness.run();
    let set = ohl_clearance::RuleSet::load(&file).unwrap();
    assert_eq!(set.rules.len(), 2);
    assert!(
        set.rules
            .iter()
            .all(|r| r.radial.is_some() && r.vertical.is_none())
    );
    harness
        .query_all_by_label("Remove")
        .next()
        .unwrap()
        .scroll_to_me();
    harness.run();
    harness.query_all_by_label("Remove").next().unwrap().click();
    harness.run();
    assert_eq!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .radial_clearances
            .rules
            .len(),
        2,
        "asks first"
    );
    // The question under the rules (scrolled to: the window is long).
    harness
        .query_all_by_label("Remove")
        .last()
        .unwrap()
        .scroll_to_me();
    harness.run();
    harness.get_by_label("Keep").click();
    harness.run();
    harness
        .query_all_by_label("Remove")
        .next()
        .unwrap()
        .scroll_to_me();
    harness.run();
    harness.query_all_by_label("Remove").next().unwrap().click();
    harness.run();
    // The question under the rules (scrolled to: the window is long).
    harness
        .query_all_by_label("Remove")
        .last()
        .unwrap()
        .scroll_to_me();
    harness.run();
    harness.query_all_by_label("Remove").last().unwrap().click();
    harness.run();
    harness.run();
    assert_eq!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .radial_clearances
            .rules
            .len(),
        1
    );
    assert_eq!(ohl_clearance::RuleSet::load(&file).unwrap().rules.len(), 1);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn vegetation_and_as_built_wires_from_a_point_cloud() {
    // A scan at 35 °C: points along every span (wire class 14) and a tree
    // (class 5) 3 m below the middle of S1's first span.
    let mut model = synthetic();
    let hot = model::Solved::new(
        &model.line,
        &model.library,
        ohl_weather::WeatherCase::still("scan", ohl_core::units::Temperature::from_celsius(35.0)),
    );
    let point = |p: [f64; 3], class: u8| ohl_lidar::LidarPoint {
        x: p[0],
        y: p[1],
        z: p[2],
        class,
        intensity: 0,
        return_number: 1,
        number_of_returns: 1,
    };
    let mut points = Vec::new();
    for shape in &hot.shapes {
        for span in &shape.spans {
            for s in span.points.windows(2) {
                for k in 0..10 {
                    let t = k as f64 / 10.0;
                    let p = [0, 1, 2].map(|i| s[0][i] + (s[1][i] - s[0][i]) * t);
                    points.push(point(p, 14));
                }
            }
        }
    }
    let mid = hot.shapes[0].spans[0].mid_point;
    points.push(point([mid[0], mid[1], mid[2] - 3.0], 5));
    let dir = std::env::temp_dir().join(format!("ohl-app-{}-scan", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("scan.ohl");
    std::fs::remove_file(&file).ok();
    ohl_pointcloud::Store::open(&file)
        .unwrap()
        .import_points(
            "scan",
            "",
            None,
            64.0,
            0.001,
            points.iter().cloned(),
            &ohl_core::budget::Budget::new(),
        )
        .unwrap();
    model.cloud = Some(format!("{}#scan", file.display()));
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    r.wirefit.open = true;
    harness.run();
    harness.get_by_label("Run").click();
    harness.step();
    let r = harness.state_mut().ready().unwrap();
    r.vegetation.show(0);
    r.vegetation.settings.iso.0 = true;
    harness.step();
    harness.step();
    run_in(&harness, crate::vegetation::CHECK_TITLE).click_accesskit();
    let started = std::time::Instant::now();
    loop {
        harness.step();
        let r = harness.state_mut().ready().unwrap();
        if let ((Some(fit), false), (Some(veg), false)) =
            (r.wirefit.result(), r.vegetation.result())
        {
            let rows = fit.as_ref().unwrap();
            assert_eq!(rows.len(), 10);
            for row in rows {
                let t = row.temperature.expect("estimated");
                assert!((t - 35.0).abs() < 2.0, "{t}");
            }
            let veg = veg.as_ref().unwrap();
            assert_eq!(veg.points, 1);
            let found = &veg.spans[0];
            assert_eq!((found.section.as_str(), found.span), ("S1", 0));
            assert_eq!(found.encroaching, 1);
            // Every grow-in point listed (one here).
            assert_eq!(veg.grow.len(), 1);
            // The case shown, as it is, and the default root ball.
            assert_eq!(veg.cases.len(), 1);
            assert_eq!(
                veg.settings.root_ball,
                ohl_clearance::vegetation::DEFAULT_ROOT_BALL
            );
            break;
        }
        assert!(started.elapsed().as_secs() < 120, "no answers in 120 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    // Drawn on its layers: the tree under the span grows in.
    harness.step();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.view3d.vegetation.grow.len(), 1);
    assert_eq!(r.plan.vegetation, r.view3d.vegetation);
    assert_eq!(r.profile.vegetation, r.view3d.vegetation);
    // The isoclearance lines too, and for the drawings' plan.
    assert!(!r.view3d.vegetation.lines.is_empty());
    let lines = r.drawings.settings().isoclearance.clone().unwrap();
    assert_eq!(lines.0.len(), r.view3d.vegetation.lines.len());
    std::fs::remove_dir_all(dir).ok();
}
