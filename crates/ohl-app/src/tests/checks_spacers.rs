// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: labels, plugin columns, the phase distance check, calculation
//! options, spacers, defaults, load types.

use super::*;

/// Labels: several weather cases drawn give one label per section (its
/// first case); in 3D only those within the label distance of the eye
/// are built.
#[test]
fn labels_are_one_per_section_and_near_only() {
    use crate::annotations::{View, notes, object_notes};
    let mut model = synthetic();
    model.shown_cases = vec![0, 2];
    for k in 0..model.cases.len() {
        model.ensure_solved(k);
    }
    model.section_display.label[View::ThreeD as usize] = true;
    model.section_display.structure_label[View::ThreeD as usize] = true;
    let all = notes(&model, 0, View::ThreeD, None);
    assert_eq!(
        all.len(),
        model.line.sections.len(),
        "one label per section"
    );
    // Far from everything: only the structures' names (written from
    // afar); near the first structure: its other labels too.
    let s = model.line.structures[0].position;
    let far = Some(([s[0] + 1e6, s[1], s[2]], 1500.0));
    assert!(notes(&model, 0, View::ThreeD, far).is_empty());
    let names = object_notes(&model, 0, View::ThreeD, far);
    assert_eq!(names.len(), model.line.structures.len());
    assert!(
        names
            .iter()
            .all(|n| n.kind == crate::annotations::Kind::Structure)
    );
    model.section_display.point_label[View::ThreeD as usize] = true;
    assert_eq!(
        object_notes(&model, 0, View::ThreeD, far).len(),
        model.line.structures.len(),
        "points' labels near only"
    );
    let near = Some((s, 50.0));
    assert!(!object_notes(&model, 0, View::ThreeD, near).is_empty());
    assert_eq!(
        model.section_display.label_range(),
        crate::annotations::DEFAULT_LABEL_DISTANCE
    );
}

/// Plugin columns: a plugin's insulation level in the circuits table
/// (an invented column) — its default shown where not set; a value chosen goes to the
/// circuit's sections in one command; one not among its values refused.
/// Without the plugin the column still shows (unused) and the values stay,
/// through other edits and an OHL XML round trip.
#[test]
fn plugin_columns_in_tables() {
    use crate::tables::{PluginColumn, TableKind};
    let model = synthetic_project("plugin-columns");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let column = PluginColumn {
        table: "circuits".into(),
        key: "example.insulation_level".into(),
        title: "insulation level".into(),
        help: String::new(),
        values: vec!["low".into(), "high".into()],
        default: Some("high".into()),
        plugin: "example 0.1.0".into(),
        in_use: true,
    };
    let own = TableKind::ConductorPaths.columns().len();
    let key = {
        let r = harness.state_mut().ready().unwrap();
        r.model.plugin_columns = vec![column.clone()];
        let rows = TableKind::ConductorPaths.rows(&r.model, 0);
        let x = rows.iter().find(|x| x.cells[6].contains("S3")).unwrap();
        assert_eq!(x.cells[own], "high");
        assert!(x.defaulted[own], "the default, not set");
        // Declared for the circuits only.
        let sections = TableKind::Sections.rows(&r.model, 0);
        assert_eq!(sections[0].cells.len(), TableKind::Sections.columns().len());
        let original = x.cells[own].clone();
        r.tables
            .entry(TableKind::ConductorPaths)
            .or_default()
            .set(&x.key, own, "Low", &original);
        x.key.clone()
    };
    let before = harness
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .sections
        .len();
    apply_table(&mut harness, TableKind::ConductorPaths, false).unwrap();
    let ids = {
        let r = harness.state_mut().ready().unwrap();
        let line = &r.model.line;
        assert_eq!(line.sections.len(), before);
        let c = ohl_line::electrical::circuits(line)
            .into_iter()
            .find(|c| c.sections.contains(&"S3".to_owned()))
            .unwrap();
        for id in &c.sections {
            let s = line.sections.iter().find(|s| &s.id == id).unwrap();
            assert_eq!(s.fields.get(&column.key).map(String::as_str), Some("low"));
        }
        let others = line
            .sections
            .iter()
            .filter(|s| !c.sections.contains(&s.id))
            .count();
        assert!(others > 0);
        assert!(
            line.sections
                .iter()
                .filter(|s| !c.sections.contains(&s.id))
                .all(|s| s.fields.is_empty())
        );
        let rows = TableKind::ConductorPaths.rows(&r.model, 0);
        let x = rows.iter().find(|x| x.key == key).unwrap();
        assert_eq!(x.cells[own], "low");
        assert!(!x.defaulted[own]);
        // A value not among its values: refused.
        r.tables
            .entry(TableKind::ConductorPaths)
            .or_default()
            .set(&key, own, "medium", "low");
        c.sections
    };
    let err = apply_table(&mut harness, TableKind::ConductorPaths, false).unwrap_err();
    assert!(err.contains("not one of"), "{err}");
    harness
        .state_mut()
        .ready()
        .unwrap()
        .tables
        .remove(&TableKind::ConductorPaths);
    // Without the plugin: an unused column in the sections' tables, the
    // values kept.
    let n = TableKind::Sections.columns().len();
    {
        let r = harness.state_mut().ready().unwrap();
        r.model.plugin_columns.clear();
        let cols = TableKind::Sections.plugin_columns(&r.model);
        assert_eq!(cols.len(), 1);
        assert!(!cols[0].in_use && cols[0].key == column.key);
        let rows = TableKind::Sections.rows(&r.model, 0);
        let s3 = rows.iter().find(|x| x.key == "S3").unwrap();
        assert_eq!(s3.cells[n], "low");
        assert_eq!(TableKind::ConductorPaths.plugin_columns(&r.model).len(), 1);
    }
    set_cell(
        &mut harness,
        TableKind::Sections,
        "S3",
        "description",
        "kept its level",
    );
    apply_table(&mut harness, TableKind::Sections, false).unwrap();
    let r = harness.state_mut().ready().unwrap();
    let s3 = r.model.line.sections.iter().find(|s| s.id == "S3").unwrap();
    assert_eq!(s3.description, "kept its level");
    assert_eq!(s3.fields.get(&column.key).map(String::as_str), Some("low"));
    // The OHL XML keeps them.
    let back = ohl_io::ohlxml::from_xml(&ohl_io::ohlxml::to_xml(&r.model.line)).unwrap();
    for id in &ids {
        let s = back.sections.iter().find(|s| &s.id == id).unwrap();
        assert_eq!(s.fields.get(&column.key).map(String::as_str), Some("low"));
    }
}

/// The Swiss LeV plugin in use, when this checkout has it (kept apart
/// from the public code, at `local-data/plugins/lev-ch`) and it is built:
/// whether it is. Without the folder the test is skipped (even when the
/// plugin tests are required); with it, not built: skipped unless they
/// are.
pub(super) fn lev_plugin(h: &mut Harness<'_, App>) -> bool {
    let dir = root().join("local-data/plugins/lev-ch");
    if !dir.is_dir() {
        eprintln!("skipping: no local-data/plugins/lev-ch");
        return false;
    }
    let built = dir
        .join("target/wasm32-wasip2/release/lev_ch.wasm")
        .is_file();
    assert!(
        built || ohl_core::env::var_os("OHL_REQUIRE_PLUGIN_TESTS").is_none(),
        "lev-ch plugin not built"
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

/// Checks › LeV/OLEI › Distance between conductors under unequal wind:
/// its own page, one weather case with wind (the windiest), run in the
/// background with a coarse step; each pair's two winds 60 … 100 % apart;
/// a result clicked draws its line and the pair's two conductors as
/// measured (at their winds, labelled; the case shown kept, nothing
/// solved); the page closed: both gone.
#[test]
fn the_unequal_wind_check_draws_the_pair_at_its_winds() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    if !lev_plugin(&mut harness) {
        return;
    }
    harness.get_by_label("Checks").click();
    harness.run();
    harness.get_by_label_contains("LeV/OLEI").hover();
    harness.run();
    harness
        .get_by_label("Distance between conductors under unequal wind (LeV)…")
        .click();
    harness.run();
    assert!(
        harness
            .state_mut()
            .dock()
            .find_tab(&crate::app::Tab::UnequalWind)
            .is_some(),
        "its page"
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let w = &mut r.unequal_wind;
        assert!(w.check().is_ok(), "{:?}", w.check().err());
        assert_eq!(w.cases.len(), 1, "one case");
        let k = *w.cases.first().unwrap();
        assert!(r.model.cases[k].case.wind_pressure.si() > 0.0, "with wind");
        w.wind_step = 1000.0;
    }
    harness.get_by_label("Run").click();
    let started = std::time::Instant::now();
    let row = loop {
        harness.step();
        let r = harness.state_mut().ready().unwrap();
        if let (Some(rep), false) = r.unequal_wind.report() {
            let rep = rep.as_ref().unwrap();
            assert!(!rep.rows.is_empty());
            for x in &rep.rows {
                let [a, b] = x.pressure.expect("the winds");
                assert!(a.min(b) >= 0.6 * a.max(b) - 1e-6, "{a} {b}");
                assert!(x.side.is_some());
                assert!(x.rule.contains("unequal wind"), "{}", x.rule);
            }
            break rep.rows[0].clone();
        }
        assert!(started.elapsed().as_secs() < 120, "no report in 120 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    harness.state_mut().ready().unwrap().unequal_wind.only_below = false;
    harness.run();
    let shown = harness.state_mut().ready().unwrap().case;
    let label = format!("{} span {}", row.span[0].0, row.span[0].1 + 1);
    harness.query_all_by_label(&label).next().unwrap().click();
    harness.run();
    assert_eq!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .view3d
            .measure
            .clone()
            .map(|m| [m.0, m.1]),
        Some(row.at)
    );
    // The two conductors as measured, drawn at once (nothing solved);
    // the case shown kept.
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.case, shown);
    let w = &r.view3d.measure_wires;
    assert_eq!(w.len(), 2);
    assert_eq!(w[0].0, row.wires[0]);
    let [a, _] = row.pressure.unwrap();
    if a > 0.0 {
        assert!(w[0].1.contains(&format!("{a:.0} Pa")), "{}", w[0].1);
    }
    let dock = harness.state_mut().dock();
    let at = dock.find_tab(&crate::app::Tab::UnequalWind).unwrap();
    dock.remove_tab(at);
    harness.run();
    let v = &harness.state_mut().ready().unwrap().view3d;
    assert!(v.measure.is_none() && v.measure_wires.is_empty());
}

/// Checks › LeV/OLEI › Distance between conductors (the menu the lev-ch
/// plugin defines; none without it): the window opens with the
/// shipped LeV values, checks the ticked case in the background (pairs
/// worst first, every circuit), and a pair's span link selects the span
/// and draws its worst station's line with d and z.
#[test]
fn the_checks_menu_opens_the_phase_distance_window() {
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    // Without the plugin: no LeV submenu (the plugin defines it).
    harness.get_by_label("Checks").click();
    harness.run();
    assert!(harness.query_by_label_contains("LeV").is_none());
    assert!(
        harness
            .query_by_label_contains("Distance between")
            .is_none()
    );
    harness.get_by_label("Checks").click();
    harness.run();
    if !lev_plugin(&mut harness) {
        return;
    }
    let original;
    {
        let m = &harness.state_mut().ready().unwrap().model;
        // In the span and under unequal wind.
        assert_eq!(m.checks.len(), 2, "{:?}", m.check_problems);
    }
    harness.get_by_label("Checks").click();
    harness.run();
    harness.get_by_label_contains("LeV/OLEI").hover();
    harness.run();
    harness
        .get_by_label("Distance between conductors (LeV)…")
        .click();
    harness.run();
    // A page of its own, floating first (it may be docked).
    let at = harness
        .state_mut()
        .dock()
        .find_tab(&crate::app::Tab::PhaseDistance)
        .expect("its page");
    assert!(!at.surface.is_main(), "floating");
    {
        let r = harness.state_mut().ready().unwrap();
        let w = &mut r.phase_distance;
        assert!(w.file.is_none(), "the plugin's values");
        assert!(w.check().is_ok(), "{:?}", w.check().err());
        assert!(w.circuits.is_none(), "every circuit");
        // A case other than the one shown: a click shows it.
        original = r.case;
        let other = (r.case + 1) % r.model.cases.len();
        assert_ne!(other, r.case);
        w.cases = std::iter::once(other).collect();
    }
    harness.get_by_label("Run").click();
    let started = std::time::Instant::now();
    let (row, below) = loop {
        harness.step();
        let r = harness.state_mut().ready().unwrap();
        if let (Some(rep), false) = r.phase_distance.report() {
            let rep = rep.as_ref().unwrap();
            assert!(!rep.rows.is_empty());
            assert!(
                rep.rows.windows(2).all(|w| w[0].margin() <= w[1].margin()),
                "worst first"
            );
            for x in &rep.rows {
                assert!(x.distance > 0.0 && x.required > x.c);
            }
            break (
                rep.rows[0].clone(),
                rep.rows.iter().filter(|x| x.margin() < 0.0).count(),
            );
        }
        assert!(started.elapsed().as_secs() < 60, "no report in 60 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    // All pairs shown, then a span link.
    harness
        .state_mut()
        .ready()
        .unwrap()
        .phase_distance
        .only_below = false;
    harness.run();
    let _ = below;
    let label = format!("{} span {}", row.span[0].0, row.span[0].1 + 1);
    harness.query_all_by_label(&label).next().unwrap().click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(
        r.selection,
        Selection::Span {
            section: row.span[0].0.clone(),
            span: row.span[0].1
        }
    );
    let (a, b, text) = r.view3d.measure.clone().expect("the line drawn");
    assert_eq!([a, b], row.at);
    assert!(text.contains("d ") && text.contains("z "), "{text}");
    // In its weather case.
    assert_eq!(r.model.cases[r.case].case.name, row.case);
    // The page closed: the line gone.
    let dock = harness.state_mut().dock();
    let at = dock.find_tab(&crate::app::Tab::PhaseDistance).unwrap();
    dock.remove_tab(at);
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.view3d.measure.is_none());
    // The case shown before the check's, back.
    assert_eq!(r.case, original);
}

/// The library of performed checks: a run of the LeV distance between
/// conductors saved (kept with the line, logged); the line edited: the
/// report is stale; opened again: its parameters and results; run again:
/// compared with it, then replaced on request; deleted after a
/// confirmation, undo brings it back.
#[test]
fn performed_checks_are_saved_and_run_again() {
    let model = synthetic_project("performed-checks");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    if !lev_plugin(&mut harness) {
        return;
    }
    let run = |h: &mut Harness<'_, App>| {
        let started = std::time::Instant::now();
        loop {
            h.step();
            let r = h.state_mut().ready().unwrap();
            if let (Some(rep), false) = r.phase_distance.report() {
                assert!(rep.is_ok(), "{rep:?}");
                break;
            }
            assert!(started.elapsed().as_secs() < 60, "no report in 60 s");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        h.run();
    };
    {
        let r = harness.state_mut().ready().unwrap();
        r.phase_distance.show(&r.model);
        if r.phase_distance.cases.is_empty() {
            r.phase_distance.cases.insert(0);
        }
        let model = &r.model;
        let ctx = egui::Context::default();
        r.phase_distance.start(model, &ctx);
    }
    run(&mut harness);
    harness.get_by_label("Save").click();
    harness.run();
    let saved = {
        let r = harness.state_mut().ready().unwrap();
        let s = &r.model.editing.as_ref().unwrap().session;
        let reports = s.reports();
        assert_eq!(reports.len(), 1);
        let x = reports[0].clone();
        assert_eq!(x.id, "Distance between conductors (LeV)");
        assert_eq!(x.check, crate::phase_distance::CHECK_ID);
        assert_eq!(x.status, ohl_session::reports::ReportStatus::Draft);
        assert!(x.summary.contains("pairs"), "{}", x.summary);
        assert!(
            x.parameters["values_text"]
                .as_str()
                .unwrap()
                .contains("[[voltage]]")
        );
        assert!(
            ohl_session::reports::changed_since(&x.state, &s.check_state().unwrap()).is_empty()
        );
        x
    };
    // The line edited: stale.
    {
        let r = harness.state_mut().ready().unwrap();
        let p = r.model.line.structure("T2").unwrap().position;
        r.perform(crate::edit::Action::Command {
            what: "T2 up".into(),
            command: ohl_session::Command::MoveStructure {
                id: "T2".into(),
                position: [p[0], p[1], p[2] + 3.0],
            },
        });
        let s = &r.model.editing.as_ref().unwrap().session;
        let c = ohl_session::reports::changed_since(&saved.state, &s.check_state().unwrap());
        assert!(c.changed.contains(&"structure:T2".to_owned()), "{c:?}");
        r.check_library.show();
    }
    harness.run();
    // In the library: what changed since.
    harness.get_by_label_contains("changed since:").hover();
    // Opened again from the library: its parameters and results.
    harness.get_by_label("Open").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let w = &r.phase_distance;
        assert_eq!(
            w.loaded.as_ref().map(|x| x.id.as_str()),
            Some(saved.id.as_str())
        );
        assert!(
            w.values.is_some(),
            "the values as saved; log {:?}",
            r.model.log.iter().rev().take(3).collect::<Vec<_>>()
        );
        let (rep, _) = w.report();
        let rep = rep.unwrap().as_ref().unwrap();
        let kept: crate::phase_distance::Saved =
            serde_json::from_value(saved.results.clone()).unwrap();
        assert_eq!(rep.rows, kept.rows);
        assert!(
            rep.rows
                .iter()
                .all(|x| x.margin() < crate::phase_distance::KEPT_WITHIN)
        );
    }
    harness.get_by_label_contains("stale").hover();
    // Run again: compared, then replaced on request.
    {
        let r = harness.state_mut().ready().unwrap();
        let ctx = egui::Context::default();
        r.phase_distance.start(&r.model, &ctx);
    }
    run(&mut harness);
    harness.run();
    assert!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .phase_distance
            .comparison
            .is_some()
    );
    harness
        .get_by_label_contains("Compared with the saved report")
        .hover();
    harness.get_by_label("Replace").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let s = &r.model.editing.as_ref().unwrap().session;
        let reports = s.reports();
        assert_eq!(reports.len(), 1, "replaced, not added");
        assert!(
            ohl_session::reports::changed_since(&reports[0].state, &s.check_state().unwrap())
                .is_empty(),
            "made on the line as it is now"
        );
        assert_ne!(reports[0].state, saved.state);
    }
    // Deleted after a confirmation; undo brings it back (the check's
    // window closed: it lies over the library).
    let dock = harness.state_mut().dock();
    let at = dock.find_tab(&crate::app::Tab::PhaseDistance).unwrap();
    dock.remove_tab(at);
    harness.run();
    harness.get_by_label("Delete…").click();
    harness.run();
    assert_eq!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .model
            .editing
            .as_ref()
            .unwrap()
            .session
            .reports()
            .len(),
        1,
        "not before the confirmation"
    );
    harness.get_by_label("Delete").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(
        r.model
            .editing
            .as_ref()
            .unwrap()
            .session
            .reports()
            .is_empty()
    );
    r.perform(crate::edit::Action::Undo);
    assert_eq!(r.model.editing.as_ref().unwrap().session.reports().len(), 1);
}

/// The LeV values in the window: shown as tables, locked; an admin edits
/// them (Edit values), saved as the project's copy and used by the next run;
/// back to the file; a designer may not edit them.
#[test]
fn phase_distance_values_are_locked_but_an_admin_edits_them() {
    let model = synthetic_project("values-locked");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    if !lev_plugin(&mut harness) {
        return;
    }
    {
        let r = harness.state_mut().ready().unwrap();
        r.phase_distance.show(&r.model);
    }
    harness.run();
    // Shown: the 380 kV row.
    harness.get_by_label("380");
    harness.get_by_label("3.64");
    harness.get_by_label("Edit values").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let w = &mut r.phase_distance;
        let text = w.editing.clone().expect("editing");
        // 380 kV high, C₂: the last line of the first table.
        let tables = w.tables_of(&text).unwrap();
        let last = tables[0].rows.len() as u32 - 1;
        assert_eq!(tables[0].rows[last as usize][4], "3.64");
        w.editing = Some(w.edit_cell(&text, [0, last, 4], "3.70").unwrap());
        assert!(w.edit_cell(&text, [0, last, 4], "-1").is_err());
    }
    harness.run();
    harness.get_by_label("Save in the project").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.phase_distance.editing.is_none());
        let s = &r.model.editing.as_ref().unwrap().session;
        let files = s.library_files("phase-distance").unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].0, "lev-ch.ohlpd");
        assert!(files[0].1.contains("3.7 m"), "{}", files[0].1);
        assert!(files[0].1.starts_with("# Edited in OHL-Design by"));
        // The next run uses them.
        if r.phase_distance.cases.is_empty() {
            r.phase_distance.cases.insert(0);
        }
        let ctx = egui::Context::default();
        r.phase_distance.start(&r.model, &ctx);
    }
    let started = std::time::Instant::now();
    loop {
        harness.step();
        let r = harness.state_mut().ready().unwrap();
        if let (Some(rep), false) = r.phase_distance.report() {
            let rep = rep.as_ref().unwrap();
            assert!(rep.params.values_text.contains("3.7 m"));
            break;
        }
        assert!(started.elapsed().as_secs() < 60);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    harness.run();
    harness.get_by_label_contains("the project's copy");
    harness.get_by_label("Back to the plugin's").click();
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let s = &r.model.editing.as_ref().unwrap().session;
        assert!(s.library_files("phase-distance").unwrap().is_empty());
        // A designer: locked.
        let admin = r.model.editing.as_ref().unwrap().user.clone();
        s.set_user(&admin, "dee", ohl_session::Role::Designer)
            .unwrap();
        r.model.editing.as_mut().unwrap().user = "dee".into();
    }
    harness.run();
    assert!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .phase_distance
            .editing
            .is_none()
    );
    harness.get_by_label("Edit values").click();
    harness.run();
    assert!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .phase_distance
            .editing
            .is_none(),
        "locked for a designer"
    );
}

/// Sections table: the bundle angle after the bundle and its spacing —
/// 0 shown (dimmed) for a bundle not turned, typed in, emptied: not
/// turned again.
#[test]
fn sections_table_turns_the_bundle() {
    use crate::tables::TableKind;
    let model = synthetic_project("bundle-angle");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let col = TableKind::Sections.column("bundle angle °");
    assert_eq!(col, TableKind::Sections.column("spacing m") + 1);
    set_cell(&mut harness, TableKind::Sections, "S1", "bundle", "3");
    set_cell(&mut harness, TableKind::Sections, "S1", "spacing m", "0.45");
    apply_table(&mut harness, TableKind::Sections, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let rows = TableKind::Sections.rows(&r.model, r.case);
        let s1 = rows.iter().find(|x| x.key == "S1").unwrap();
        assert_eq!(s1.cells[col], "0");
        assert!(s1.defaulted[col]);
    }
    set_cell(
        &mut harness,
        TableKind::Sections,
        "S1",
        "bundle angle °",
        "60",
    );
    apply_table(&mut harness, TableKind::Sections, false).unwrap();
    let rot = |h: &mut Harness<'_, App>| {
        let r = h.state_mut().ready().unwrap();
        let s = r.model.line.sections.iter().find(|s| s.id == "S1").unwrap();
        (s.bundle, s.bundle_rotation_deg)
    };
    assert_eq!(rot(&mut harness), (3, Some(60.0)));
    set_cell(
        &mut harness,
        TableKind::Sections,
        "S1",
        "bundle angle °",
        "",
    );
    apply_table(&mut harness, TableKind::Sections, false).unwrap();
    assert_eq!(rot(&mut harness), (3, None));
}

/// The LeV page lists the circuits checked with their insulation level
/// (not set: high); one chosen there goes to all the circuit's sections.
#[test]
fn phase_distance_page_sets_the_insulation_levels() {
    let model = synthetic_project("levels-in-page");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1600.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    if !lev_plugin(&mut harness) {
        return;
    }
    {
        let r = harness.state_mut().ready().unwrap();
        r.phase_distance.show(&r.model);
    }
    harness.run();
    harness
        .query_all_by_value("high (not set)")
        .next()
        .expect("a circuit's level")
        .click();
    harness.run();
    harness
        .query_all_by_label("low")
        .last()
        .expect("the low choice")
        .click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    let line = &r.model.line;
    let low: Vec<&ohl_line::Section> = line
        .sections
        .iter()
        .filter(|s| s.fields.get("lev-ch.insulation_level").map(String::as_str) == Some("low"))
        .collect();
    assert!(!low.is_empty());
    let circuit = &low[0].circuit_name;
    assert!(
        line.sections
            .iter()
            .filter(|s| &s.circuit_name == circuit)
            .all(|s| low.iter().any(|l| l.id == s.id)),
        "all the circuit's sections"
    );
    assert!(
        line.sections
            .iter()
            .any(|s| &s.circuit_name != circuit && s.fields.is_empty())
    );
}

/// View › Profile aspect ratio, a ratio of your own: "1:" chooses it (its
/// last value, 3 at first), the value is typed in — 10, also a value of
/// the list, stays the typed choice; a ratio of the list chosen after.
#[test]
fn the_profile_takes_a_ratio_of_your_own() {
    use egui_kittest::kittest::NodeT as _;
    let model = synthetic();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let open = |h: &mut Harness<'_, App>| {
        h.get_by_label("View").click();
        h.run();
        h.get_by_label_contains("Profile aspect ratio").hover();
        h.run();
    };
    let ratio = |h: &mut Harness<'_, App>| h.state_mut().ready().unwrap().model.profile_ratio;
    open(&mut harness);
    harness.get_by_label("1:").click();
    harness.run();
    assert_eq!(ratio(&mut harness), Some(3.0));
    // Typed: 10.
    // The menu's value field: the spin button on the row of "1:".
    let row = harness.get_by_label("1:").rect().center().y;
    let at = harness
        .query_all_by_role(egui::accesskit::Role::SpinButton)
        .map(|x| x.rect().center())
        .min_by(|a, b| (a.y - row).abs().total_cmp(&(b.y - row).abs()))
        .expect("the value field");
    assert!((at.y - row).abs() < 5.0, "{at:?} {row}");
    harness
        .query_all_by_role(egui::accesskit::Role::SpinButton)
        .find(|x| x.rect().center() == at)
        .unwrap()
        .click();
    harness.run();
    assert!(
        harness.query_by_label("1:").is_some(),
        "the menu stays open"
    );
    // Editing: the text selected, typed over.
    harness.event(egui::Event::Text("10".into()));
    harness.run();
    harness.key_press(egui::Key::Enter);
    harness.run();
    assert_eq!(ratio(&mut harness), Some(10.0));
    // Still the typed choice (the list's 1:10 not ticked).
    let typed = harness.get_by_label("1:").accesskit_node().toggled();
    assert_eq!(typed, Some(egui::accesskit::Toggled::True));
    let listed = harness
        .get_by_label("1:10 (heights ×10)")
        .accesskit_node()
        .toggled();
    assert_eq!(listed, Some(egui::accesskit::Toggled::False));
    // One of the list.
    harness.get_by_label("1:5 (heights ×5)").click();
    harness.run();
    assert_eq!(ratio(&mut harness), Some(5.0));
    assert_eq!(
        harness.get_by_label("1:").accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::False)
    );
}

/// Strings: a tension string may hang from the wire point of a suspension
/// string without a set (a TAK) though that one is not of its group: the
/// editor lists every wire point of the structure as "hangs from", and
/// takes the other group in when one is chosen.
#[test]
fn a_string_hangs_from_another_groups_wire_point() {
    let model = synthetic_project_with("tak-top", |line| {
        let s = line.structures.iter_mut().find(|s| s.id == "T3").unwrap();
        let a = s.points[0].position;
        s.points.push(ohl_line::StructurePoint {
            description: String::new(),
            label: "B".into(),
            position: [a[0] + 2.0, a[1], a[2]],
            ..Default::default()
        });
        s.wires.push(ohl_line::WirePoint {
            label: "WS".into(),
            position: [a[0] + 2.0, a[1], a[2] - 3.0],
            ..Default::default()
        });
        s.strings.push(ohl_line::InsulatorString {
            label: "SUS".into(),
            top: "B".into(),
            wire: "WS".into(),
            ..Default::default()
        });
    });
    let mut f = crate::editors::StringForm::of(&model, "T3", 2).expect("set 2 of T3");
    assert!(!f.strings.iter().any(|x| x.label == "SUS"), "another group");
    let k = f.strings.iter().position(|x| x.wire == "W2").unwrap();
    assert!(f.top_choices(&model, k).contains(&"WS".to_owned()));
    f.hang_from(&model, k, "WS").unwrap();
    let x = &f.strings[k];
    assert_eq!(x.top, "WS");
    assert!(f.strings.iter().any(|x| x.label == "SUS"), "taken in");
    // Applied: the line holds.
    let mut line = model.line.clone();
    let resolved = model.line.structure("T3").cloned();
    let s = line.structures.iter_mut().find(|s| s.id == "T3").unwrap();
    f.apply_to(s, resolved.as_ref());
    line.validate().unwrap();
    let s = line.structure("T3").unwrap();
    assert_eq!(s.strings.iter().find(|y| y.wire == "W2").unwrap().top, "WS");
    assert!(s.strings.iter().any(|y| y.label == "SUS"));
}

/// Criteria › Calculation options: the jumpers' own pull chosen — one
/// command, undoable; the line's settings say so and the sections at the
/// jumper are solved with it.
#[test]
fn calculation_options_choose_the_jumpers_pull() {
    let model = synthetic_project("solver-options");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    crate::tests::line_menu(&mut harness, "Criteria");
    harness.run();
    harness.get_by_label("Calculation options…").click();
    harness.run();
    harness.get_by_label("The jumper's own pull").click();
    harness.run();
    harness.get_by_label("OK").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(
        r.model.line.solver.jumper_loads,
        ohl_line::JumperLoads::Pull
    );
    let stored = r.model.stored_line().unwrap();
    assert_eq!(stored.solver.jumper_loads, ohl_line::JumperLoads::Pull);
    r.perform(crate::edit::Action::Undo);
    assert_eq!(
        r.model.line.solver.jumper_loads,
        ohl_line::JumperLoads::Estimate
    );
}

/// Criteria › Calculation options: the ice and wind per unstretched
/// metre chosen (2026-10-21, as other programs) — warned about in the
/// window, one command, undoable, kept in the line's settings, and every
/// result made since is out of date naming the option.
#[test]
fn calculation_options_choose_the_ice_per_unstretched_metre() {
    let model = synthetic_project("loads-per");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let before = harness.state_mut().ready().unwrap().model.revision();
    crate::tests::line_menu(&mut harness, "Criteria");
    harness.run();
    harness.get_by_label("Calculation options…").click();
    harness.run();
    harness
        .get_by_label("Per unstretched metre (as other programs)")
        .click();
    harness.run();
    assert!(
        harness
            .query_all_by_label_contains("slightly less sag and swing (unsafe side)")
            .next()
            .is_some()
    );
    harness.get_by_label("OK").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(
        r.model.line.solver.loads_per,
        ohl_line::LoadsPer::Unstretched
    );
    let stored = r.model.stored_line().unwrap();
    assert_eq!(stored.solver.loads_per, ohl_line::LoadsPer::Unstretched);
    let said = before
        .changed(&r.model.revision(), crate::revision::Uses::LINE)
        .unwrap();
    assert_eq!(
        said,
        "the calculation options (now ice and wind per unstretched metre (as other programs; a \
         simplification, about T/EA ≈ 0.1 % less, unsafe side))"
    );
    r.perform(crate::edit::Action::Undo);
    assert_eq!(r.model.line.solver.loads_per, ohl_line::LoadsPer::Stretched);
    assert_eq!(r.model.revision(), before, "back as it was");
}

/// The span tool: a span right-clicked where its wire is clicked gives a
/// window placing a span point there (by distance, as clicked); two span
/// points, one on each conductor of a span, joined with Add string make a
/// spacer (the project's default compression limit), solved and drawn;
/// regular point loads and a spread load by station; the tables list them
/// and change them; deleting a span point takes its spacer.
#[test]
fn spacers_are_laid_between_span_points() {
    use crate::editors::Form;
    use crate::span_tool::{PlaceBy, SpanWhat};
    use crate::tables::TableKind;
    use crate::tools::Tool;
    let model = synthetic_project_with("spacers", |_| {});
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let (s0, s1, on) = {
        let r = harness.state_mut().ready().unwrap();
        let l = &r.model.line;
        let (s0, s1) = (l.sections[0].id.clone(), l.sections[1].id.clone());
        let at = |k: usize| {
            l.sections[k].points[..2]
                .iter()
                .map(|p| p.structure.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(at(0), at(1));
        let points = &r.model.shape(&s0, r.case).unwrap().spans[0].points;
        // A point of the wire a third of the way along.
        let on = crate::scene::at_fraction(points, 1.0 / 3.0).unwrap();
        (s0, s1, on)
    };
    // Its window: a span point where the span was clicked.
    let add_point = |h: &mut Harness<'_, App>, section: &str| {
        let r = h.state_mut().ready().unwrap();
        r.hotbar.hold(Tool::OnSpan);
        r.use_tool(crate::selection::Use {
            target: Selection::Span {
                section: section.into(),
                span: 0,
            },
            at: None,
            on: Some(on),
        });
        let Some(Form::Span(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        assert_eq!(f.what, SpanWhat::Point);
        assert!(
            (f.from / f.horizontal - 1.0 / 3.0).abs() < 0.01,
            "{}",
            f.from
        );
        f.label = "M".into();
        h.run();
        press(h, egui::Key::Enter, egui::Modifiers::NONE);
        let r = h.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
    };
    add_point(&mut harness, &s0);
    add_point(&mut harness, &s1);
    {
        let r = harness.state_mut().ready().unwrap();
        let p = r.model.line.sections[0]
            .span_points
            .iter()
            .find(|p| p.label == "M")
            .unwrap();
        assert!(p.at.distance().is_some());
        // Solved where it is: on the wire.
        let shape = r.model.shape(&s0, r.case).unwrap();
        assert!(shape.span_points.iter().any(|(l, _)| l == "M"));
    }
    // Add string: one span point, then the other — the spacer's window.
    let point = |section: &str| Selection::SpanPoint {
        section: section.into(),
        label: "M".into(),
    };
    harness
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .defaults
        .spacer_compression = 7000.0;
    use_tool(&mut harness, Tool::AddString, point(&s0), None);
    assert!(harness.state_mut().ready().unwrap().spacer_from.is_some());
    use_tool(&mut harness, Tool::AddString, point(&s1), None);
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Spacer(f)) = &mut r.editor else {
            panic!("{:?}", r.editor);
        };
        assert_eq!(f.spacer.compression, Some(7000.0));
        f.spacer.weight = 150.0;
    }
    harness.run();
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        assert_eq!(r.model.line.spacers.len(), 1);
        let id = r.model.line.spacers[0].id.clone();
        let x = crate::selection::spacer_state(&r.model, r.case, &id).expect("solved");
        assert!(x.force.is_finite());
        // Drawn and selectable.
        let wires = crate::scene::prims(&r.model, crate::scene::Layer::Wires, r.case);
        assert!(wires.iter().any(|p| p.points == x.ends.to_vec()));
        let rows = TableKind::Spacers.rows(&r.model, r.case);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].cells[TableKind::Spacers.column("weight N")],
            "150.0"
        );
        assert!(!rows[0].cells[TableKind::Spacers.column("force N")].is_empty());
    }
    // The table changes it.
    let id = harness.state_mut().ready().unwrap().model.line.spacers[0]
        .id
        .clone();
    set_cell(&mut harness, TableKind::Spacers, &id, "compression N", "");
    apply_table(&mut harness, TableKind::Spacers, false).unwrap();
    assert_eq!(
        harness.state_mut().ready().unwrap().model.line.spacers[0].compression,
        None
    );
    // Point loads every 50 m and a spread load by station.
    let before = harness.state_mut().ready().unwrap().model.line.sections[0]
        .objects()
        .count();
    {
        let r = harness.state_mut().ready().unwrap();
        let mut f = crate::span_tool::SpanForm::new(&r.model, r.case, &s0, 0, None).unwrap();
        f.what = SpanWhat::Load;
        (f.regular, f.from, f.to, f.spacing) = (true, 50.0, 150.0, 50.0);
        assert_eq!(f.distances(), vec![50.0, 100.0, 150.0]);
        r.editor = Some(Form::Span(f));
    }
    harness.run();
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        assert_eq!(r.model.line.sections[0].objects().count(), before + 3);
        let mut f = crate::span_tool::SpanForm::new(&r.model, r.case, &s0, 0, None).unwrap();
        f.what = SpanWhat::Spread;
        if f.stations.is_some() {
            f.by = PlaceBy::Station;
        }
        (f.from, f.to, f.per_metre) = (10.0, 30.0, 25.0);
        r.editor = Some(Form::Span(f));
    }
    harness.run();
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        let l = r.model.line.sections[0].spreads.last().unwrap();
        assert_eq!(l.load, 25.0);
        let rows = TableKind::SpanObjects.rows(&r.model, r.case);
        // Every point and spread of every section.
        let all: usize = r
            .model
            .line
            .sections
            .iter()
            .map(|s| s.span_points.len() + s.spreads.len())
            .sum();
        assert_eq!(rows.len(), all);
    }
    // A load's place typed in the table: the first point added with a
    // body on S0.
    let k = {
        let r = harness.state_mut().ready().unwrap();
        let s = &r.model.line.sections[0];
        let first = s.objects().nth(before).unwrap().0.label.clone();
        s.span_points.iter().position(|p| p.label == first).unwrap()
    };
    let key = format!("{s0}\tp\t{k}");
    set_cell(&mut harness, TableKind::SpanObjects, &key, "at", "0.2");
    apply_table(&mut harness, TableKind::SpanObjects, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let p = &r.model.line.sections[0].span_points[k];
        assert_eq!(p.at, ohl_line::SpanPlace::Fraction(0.2));
    }
    // The Edit tool on a point opens its own window (not the section's):
    // relabelled and moved there.
    let old = harness.state_mut().ready().unwrap().model.line.sections[0].span_points[k]
        .label
        .clone();
    use_tool(
        &mut harness,
        Tool::Edit,
        Selection::SpanPoint {
            section: s0.clone(),
            label: old.clone(),
        },
        None,
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let Some(Form::Span(f)) = &mut r.editor else {
            panic!("no point window: {:?}", r.editor.as_ref().map(Form::title));
        };
        assert_eq!(f.editing.as_deref(), Some(old.as_str()));
        assert_eq!(f.what, SpanWhat::Load);
        f.label = "EDITED".into();
        f.by = PlaceBy::Fraction;
        f.from = 0.3 * f.horizontal;
    }
    harness.run();
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        let p = &r.model.line.sections[0].span_points[k];
        assert_eq!(p.label, "EDITED");
        assert!(matches!(p.at, ohl_line::SpanPlace::Fraction(f) if (f - 0.3).abs() < 1e-9));
        assert!(p.object.is_some());
    }
    // Deleting a span point takes its spacer.
    {
        let r = harness.state_mut().ready().unwrap();
        r.hotbar.hold(Tool::Delete);
        r.use_tool(crate::selection::Use {
            target: point(&s1),
            at: None,
            on: None,
        });
    }
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.model.line.spacers.is_empty(), "{:?}", r.status);
    assert!(r.model.line.sections[1].span_points.is_empty());
    assert!(
        r.model.line.sections[0]
            .span_points
            .iter()
            .any(|p| p.label == "M")
    );
}

/// Criteria › Default values: every value of the project, changed in the
/// window and kept with OK (one command); the weather table's velocities
/// from the project's Q where a case gives none, a vertical clearance
/// window's zone from the project's width; the program's values are those
/// of each part of OpenOHL.
#[test]
fn the_project_default_values_are_used() {
    use crate::tables::TableKind;
    let d = ohl_line::Defaults::default();
    assert_eq!(
        d.jumper_exit_angle_deg,
        ohl_line::jumper::DEFAULT_TERMINAL_ANGLE_DEG
    );
    assert_eq!(
        d.insulator_stiffness,
        ohl_line::solve::DEFAULT_INSULATOR_STIFFNESS
    );
    assert_eq!(
        d.air_density_factor,
        ohl_weather::DEFAULT_AIR_DENSITY_FACTOR
    );
    assert_eq!(d.thermal_emissivity, ohl_thermal::DEFAULT_EMISSIVITY);
    assert_eq!(d.thermal_time_step, ohl_thermal::DEFAULT_TIME_STEP);
    assert_eq!(
        d.clearance_search_margin,
        ohl_clearance::rules::DEFAULT_SEARCH_MARGIN
    );
    assert_eq!(d.clearance_zone_width, ohl_clearance::zone::DEFAULT_WIDTH);
    let model = synthetic_project("default-values");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    {
        let r = harness.state_mut().ready().unwrap();
        let mut d = r.model.line.defaults;
        d.air_density_factor = 0.5;
        d.clearance_zone_width = 2.5;
        r.defaults_window = Some((d, None));
    }
    harness.run();
    harness.get_by_label("air density factor Q");
    harness.get_by_label("OK").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(r.defaults_window.is_none());
    assert_eq!(r.model.line.defaults.air_density_factor, 0.5);
    let s = &r.model.editing.as_ref().unwrap().session;
    assert_eq!(s.line().defaults.clearance_zone_width, 2.5);
    // A weather case without its own Q: V = √(P / 0.5).
    let rows = TableKind::WeatherCases.rows(&r.model, r.case);
    let (k, w) = r
        .model
        .cases
        .iter()
        .map(|c| &c.case)
        .enumerate()
        .find(|(_, c)| c.air_density_factor.is_none() && c.wind_pressure.si() > 0.0)
        .expect("a windy case without its own Q");
    let v = (w.wind_pressure.si() / 0.5).sqrt();
    assert_eq!(rows[k].cells[crate::tables::W_VELOCITY], format!("{v:.2}"));
    // A vertical clearance window's zone.
    r.vertical_clearances.show(&r.model);
    assert_eq!(r.vertical_clearances.zone.width, 2.5);
}

/// Points on a jumper: the span tool on a jumper puts a point with a tap
/// clamp (a template) there; it is picked like any point; Add jumper from
/// that clamp to a string makes a jumper on the tap (given by its length),
/// solved; the table edits its shape (advanced: its own values then);
/// deleting the point takes the jumper on it.
#[test]
fn a_jumper_starts_from_a_tap_on_another_jumper() {
    use crate::editors::Form;
    use crate::span_tool::SpanWhat;
    use crate::tables::TableKind;
    use crate::tools::Tool;
    let model = synthetic_project("jumper-tap");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let (host, end) = {
        let r = harness.state_mut().ready().unwrap();
        let j = &r.model.line.jumpers[0];
        (j.id.clone(), j.from.point().unwrap().clone())
    };
    {
        let r = harness.state_mut().ready().unwrap();
        let mut f = crate::span_tool::SpanForm::on_jumper(&r.model, r.case, &host, None).unwrap();
        f.what = SpanWhat::Load;
        f.object = f.library.get("tap-clamp").unwrap().object().unwrap();
        f.label = "K".into();
        r.editor = Some(Form::Span(f));
    }
    harness.run();
    press(&mut harness, egui::Key::Enter, egui::Modifiers::NONE);
    let point = Selection::JumperPoint {
        jumper: host.clone(),
        label: "K".into(),
    };
    {
        let r = harness.state_mut().ready().unwrap();
        assert!(r.editor.is_none(), "{:?}", r.status);
        let j = &r.model.line.jumpers[0];
        let p = j.points.iter().find(|p| p.label == "K").unwrap();
        assert_eq!(p.object.as_ref().unwrap().kind, ohl_line::ObjectKind::Clamp);
        // Solved there, a candidate to click.
        r.model.ensure_solved(r.case);
        assert!(point.focus(&r.model, r.case).is_some());
        let snaps = r.effective_snaps();
        assert!(
            crate::selection::candidates(&r.model, r.case, &snaps)
                .iter()
                .any(|c| c.0 == point)
        );
    }
    // Add jumper: the clamp, then a string.
    let n = harness
        .state_mut()
        .ready()
        .unwrap()
        .model
        .line
        .jumpers
        .len();
    use_tool(&mut harness, Tool::AddJumper, point.clone(), None);
    assert!(harness.state_mut().ready().unwrap().jumper_tap.is_some());
    use_tool(
        &mut harness,
        Tool::AddJumper,
        string_at(&end.structure, end.set),
        None,
    );
    let tapped = {
        let r = harness.state_mut().ready().unwrap();
        assert_eq!(r.model.line.jumpers.len(), n + 1, "{:?}", r.status);
        let j = r.model.line.jumpers.last().unwrap().clone();
        assert!(
            matches!(j.from.as_tap(), Some(ohl_line::Tap::Jumper { jumper, point }) if *jumper == host && point == "K")
        );
        assert!(j.length.is_some() && j.sag.is_none());
        r.model.ensure_solved(r.case);
        assert!(r.model.jumper_shape(&j.id, r.case).is_some(), "solved");
        j.id
    };
    // The table: its shape typed in.
    let key = {
        let r = harness.state_mut().ready().unwrap();
        let k = r.model.line.jumpers[0]
            .points
            .iter()
            .position(|p| p.label == "K")
            .unwrap();
        format!("{host}\tj\t{k}")
    };
    set_cell(&mut harness, TableKind::SpanObjects, &key, "shape", "box");
    set_cell(&mut harness, TableKind::SpanObjects, &key, "size 1", "0.3");
    apply_table(&mut harness, TableKind::SpanObjects, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let o = r.model.line.jumpers[0]
            .points
            .iter()
            .find(|p| p.label == "K")
            .unwrap()
            .object
            .clone()
            .unwrap();
        assert!(matches!(o.shape, ohl_line::Shape::Box { along, .. } if along == 0.3));
        assert_eq!(o.type_id, None, "its own values now");
        assert_eq!(o.kind, ohl_line::ObjectKind::Clamp);
    }
    // Deleting the point takes the jumper on it.
    {
        let r = harness.state_mut().ready().unwrap();
        r.hotbar.hold(Tool::Delete);
        r.use_tool(crate::selection::Use {
            target: point,
            at: None,
            on: None,
        });
    }
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert!(
        r.model.line.jumpers.iter().all(|j| j.id != tapped),
        "{:?}",
        r.status
    );
    assert!(
        r.model.line.jumpers[0]
            .points
            .iter()
            .all(|p| p.label != "K")
    );
}

/// Bodies drawn by their shape: a box and a cylinder as closed solids
/// facing out, oriented along the wire; a point nothing.
#[test]
fn bodies_are_drawn_by_their_shape() {
    use crate::scene::body_mesh;
    let at = [10.0, 20.0, 30.0];
    let solid = |s: &ohl_line::Shape| {
        let t = body_mesh(s, at, [0.0, 1.0]);
        assert!(!t.is_empty() && t.len().is_multiple_of(3));
        for tri in t.as_chunks::<3>().0 {
            let u = [0, 1, 2].map(|k| tri[1][k] - tri[0][k]);
            let v = [0, 1, 2].map(|k| tri[2][k] - tri[0][k]);
            let n = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            let m = [0, 1, 2].map(|k| (tri[0][k] + tri[1][k] + tri[2][k]) / 3.0 - at[k]);
            assert!(n[0] * m[0] + n[1] * m[1] + n[2] * m[2] >= -1e-12);
        }
        t
    };
    let b = solid(&ohl_line::Shape::Box {
        along: 2.0,
        across: 0.5,
        height: 0.3,
    });
    // Along the wire (y here): 2 m.
    let ys = b.iter().map(|p| p[1] - at[1]);
    let span = ys.clone().fold(f64::MIN, f64::max) - ys.fold(f64::MAX, f64::min);
    assert!((span - 2.0).abs() < 1e-9, "{span}");
    let c = solid(&ohl_line::Shape::Cylinder {
        diameter: 0.2,
        length: 1.0,
        axis: ohl_line::Axis::Vertical,
    });
    let zs = c.iter().map(|p| p[2] - at[2]);
    let h = zs.clone().fold(f64::MIN, f64::max) - zs.fold(f64::MAX, f64::min);
    assert!((h - 1.0).abs() < 1e-9);
    assert!(body_mesh(&ohl_line::Shape::Point, at, [1.0, 0.0]).is_empty());
}

/// The point and spread load types: the templates listed; one changed
/// becomes the project's own (kept in the project, used by the span
/// tool); a new type added; the project's copy deleted brings the
/// template back; a template cannot be deleted.
#[test]
fn load_types_are_the_templates_and_the_projects_own() {
    use crate::tables::TableKind;
    let model = synthetic_project("load-types");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let col = |n: &str| TableKind::LoadTypes.column(n);
    {
        let r = harness.state_mut().ready().unwrap();
        let rows = TableKind::LoadTypes.rows(&r.model, r.case);
        let ball = rows.iter().find(|x| x.key == "marker-ball-600").unwrap();
        assert_eq!(ball.cells[col("source")], "template");
        assert_eq!(ball.cells[col("shape")], "sphere");
    }
    set_cell(
        &mut harness,
        TableKind::LoadTypes,
        "marker-ball-600",
        "weight N",
        "80",
    );
    apply_table(&mut harness, TableKind::LoadTypes, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let lib = r.model.object_library();
        assert_eq!(lib.get("marker-ball-600").unwrap().weight, 80.0);
        let rows = TableKind::LoadTypes.rows(&r.model, r.case);
        let ball = rows.iter().find(|x| x.key == "marker-ball-600").unwrap();
        assert_eq!(ball.cells[col("source")], "project");
        let files = r
            .model
            .editing
            .as_ref()
            .unwrap()
            .session
            .library_files("objects")
            .unwrap();
        assert_eq!(files.len(), 1);
        // A new type.
        let mut row = TableKind::LoadTypes.new_row(&r.model, r.case, &[]);
        row.resize(TableKind::LoadTypes.columns().len(), String::new());
        row[col("id")] = "lamp".into();
        row[col("name")] = "Lamp".into();
        row[col("kind")] = "body".into();
        row[col("weight N")] = "60".into();
        row[col("shape")] = "box".into();
        row[col("size 1")] = "0.3".into();
        row[col("size 2")] = "0.2".into();
        row[col("size 3")] = "0.4".into();
        r.tables
            .entry(TableKind::LoadTypes)
            .or_default()
            .added
            .push(row);
    }
    apply_table(&mut harness, TableKind::LoadTypes, false).unwrap();
    {
        let r = harness.state_mut().ready().unwrap();
        let lamp = r.model.object_library().get("lamp").cloned().unwrap();
        assert!(matches!(lamp.shape, Some(ohl_line::Shape::Box { height, .. }) if height == 0.4));
        // The span tool offers it.
        let s0 = r.model.line.sections[0].id.clone();
        let f = crate::span_tool::SpanForm::new(&r.model, r.case, &s0, 0, None).unwrap();
        assert!(f.library.get("lamp").is_some());
        // A template cannot be deleted; the project's copy can.
        r.tables
            .entry(TableKind::LoadTypes)
            .or_default()
            .deleted
            .insert("weight-50kg".into());
    }
    assert!(apply_table(&mut harness, TableKind::LoadTypes, false).is_err());
    {
        let r = harness.state_mut().ready().unwrap();
        let page = r.tables.entry(TableKind::LoadTypes).or_default();
        page.clear();
        page.deleted.insert("marker-ball-600".into());
    }
    apply_table(&mut harness, TableKind::LoadTypes, false).unwrap();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(
        r.model
            .object_library()
            .get("marker-ball-600")
            .unwrap()
            .weight,
        70.0
    );
}

/// Criteria › Calculation options: the simplified in-the-blocks model
/// (equal H) chosen (2026-10-25; over sheaves the default) — one command,
/// undoable and redoable, kept in the line's settings.
#[test]
fn calculation_options_choose_the_simplified_blocks() {
    let model = synthetic_project("blocks");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 1000.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    assert_eq!(
        harness
            .state_mut()
            .ready()
            .unwrap()
            .model
            .line
            .solver
            .blocks,
        ohl_line::InTheBlocks::Sheaves
    );
    crate::tests::line_menu(&mut harness, "Criteria");
    harness.run();
    harness.get_by_label("Calculation options…").click();
    harness.run();
    harness.get_by_label("Simplified (equal H)").click();
    harness.run();
    harness.get_by_label("OK").click();
    harness.run();
    let r = harness.state_mut().ready().unwrap();
    assert_eq!(r.model.line.solver.blocks, ohl_line::InTheBlocks::EqualH);
    let stored = r.model.stored_line().unwrap();
    assert_eq!(stored.solver.blocks, ohl_line::InTheBlocks::EqualH);
    r.perform(crate::edit::Action::Undo);
    assert_eq!(r.model.line.solver.blocks, ohl_line::InTheBlocks::Sheaves);
    r.perform(crate::edit::Action::Redo);
    assert_eq!(r.model.line.solver.blocks, ohl_line::InTheBlocks::EqualH);
}

/// Checks › Electrical › Magnetic field…: Run is greyed until a circuit
/// has a current; then a profile per span with a limit's line; the line
/// edited, the results are out of date.
#[test]
fn the_magnetic_field_window_runs_and_goes_out_of_date() {
    use egui_kittest::kittest::NodeT;
    let model = synthetic();
    let png =
        ohl_core::env::var_os("OHL_RENDER_PNG").filter(|_| ohl_render::headless_device().is_ok());
    let mut builder = Harness::builder().with_size(egui::vec2(1600.0, 1000.0));
    if png.is_some() {
        builder = builder.wgpu();
    }
    let mut harness =
        builder.build_eframe(|cc| App::with_model(model, cc.wgpu_render_state.clone()));
    harness.run();
    submenu(&mut harness, "Checks", "Electrical");
    harness.get_by_label("Magnetic field…").click();
    harness.run();
    assert!(harness.state_mut().ready().unwrap().magnetic_field.open);
    assert!(
        run_in(&harness, "Magnetic field")
            .accesskit_node()
            .is_disabled(),
        "no current yet"
    );
    {
        let r = harness.state_mut().ready().unwrap();
        let circuits = ohl_electrical::magnetic::circuits(&r.model.line);
        assert!(!circuits.is_empty());
        r.magnetic_field.set_current(&circuits[0].label(), 800.0);
        r.magnetic_field.set_limit(Some(1.0));
    }
    harness.run();
    run_in(&harness, "Magnetic field").click();
    let started = std::time::Instant::now();
    loop {
        harness.step();
        let r = harness.state_mut().ready().unwrap();
        if let Some(res) = &r.magnetic_field.job.result {
            let a = res.as_ref().expect("a field");
            assert!(!a.field.profiles.is_empty());
            assert!(a.field.profiles.iter().all(|p| p.max_ut > 0.0));
            break;
        }
        assert!(started.elapsed().as_secs() < 60, "no field in 60 s");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    harness.run();
    assert!(harness.query_by_label_contains("Export CSV").is_some());
    if let Some(dir) = png {
        let image = harness.render().unwrap();
        let path = std::path::PathBuf::from(dir).join("ohl-design-magnetic-field.png");
        crate::write_png(&path, image.width(), image.height(), image.as_raw()).unwrap();
    }
    let r = harness.state_mut().ready().unwrap();
    assert!(r.magnetic_field.out_of_date(&r.model).is_none());
    r.model.line.structures[1].position[2] += 1.0;
    r.model.line_version += 1;
    assert!(r.magnetic_field.out_of_date(&r.model).is_some());
}
