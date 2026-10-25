// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: local winds per span (roadmap O15) in the Sections table and a
//! span's Properties; broken-wire cases sent to the structures (roadmap
//! O9) in Criteria › Loads for structures.

use super::objects::{apply_table, set_cell};
use super::*;
use crate::tables::TableKind;

fn factors(h: &mut Harness<'_, App>) -> Vec<ohl_line::SpanWind> {
    h.state_mut().ready().unwrap().model.line.sections[0]
        .wind_factors
        .clone()
}

/// The Sections table's "local winds" column: spans from 1 and runs, one
/// command on Apply (one undo), solved again; text that does not read
/// blocks Apply, saying why.
#[test]
fn local_winds_are_typed_in_the_sections_table() {
    let model = synthetic_project("local-winds-table");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let s1 = harness.state_mut().ready().unwrap().model.line.sections[0]
        .id
        .clone();
    set_cell(
        &mut harness,
        TableKind::Sections,
        &s1,
        "local winds",
        "2–3: 1.2; 4: 0.7",
    );
    apply_table(&mut harness, TableKind::Sections, false).unwrap();
    let want = vec![
        ohl_line::SpanWind {
            span: 1,
            factor: 1.2,
        },
        ohl_line::SpanWind {
            span: 2,
            factor: 1.2,
        },
        ohl_line::SpanWind {
            span: 3,
            factor: 0.7,
        },
    ];
    assert_eq!(factors(&mut harness), want);
    {
        let r = harness.state_mut().ready().unwrap();
        let rows = TableKind::Sections.rows(&r.model, r.case);
        let row = rows.iter().find(|x| x.key == s1).unwrap();
        assert_eq!(
            row.cells[TableKind::Sections.column("local winds")],
            "2–3: 1.2; 4: 0.7"
        );
        // One undo takes it back, a redo brings it again.
        r.model.undo().unwrap();
        assert!(r.model.line.sections[0].wind_factors.is_empty());
        r.model.redo().unwrap();
    }
    assert_eq!(factors(&mut harness), want);
    set_cell(
        &mut harness,
        TableKind::Sections,
        &s1,
        "local winds",
        "9: 1.2",
    );
    let e = apply_table(&mut harness, TableKind::Sections, false).unwrap_err();
    assert!(
        e.contains("local winds") && e.contains("spans 1 to 4"),
        "{e}"
    );
    assert_eq!(factors(&mut harness), want);
}

/// A span's Properties: its local wind shown, and set on a run of spans
/// with Set (one command, one undo).
#[test]
fn local_winds_are_set_in_a_spans_properties() {
    let model = synthetic_project("local-winds-span");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_eframe(|_| App::with_model(model, None));
    harness.run();
    let s1 = harness.state_mut().ready().unwrap().model.line.sections[0]
        .id
        .clone();
    harness
        .state_mut()
        .ready()
        .unwrap()
        .select(Selection::Span {
            section: s1.clone(),
            span: 1,
        });
    harness.run();
    harness.get_by_label("Local wind ×");
    // The factor and spans as typed in the row (span 2 to 3, × 1.3).
    let id = egui::Id::new(("local-wind", s1.as_str(), 1_usize, 1.0_f64.to_bits()));
    harness
        .ctx
        .data_mut(|d| d.insert_temp(id, (1.3_f64, 2_usize, 3_usize)));
    harness.run();
    harness.get_by_label("Set").click();
    harness.run();
    let got = factors(&mut harness);
    assert_eq!(
        got,
        vec![
            ohl_line::SpanWind {
                span: 1,
                factor: 1.3
            },
            ohl_line::SpanWind {
                span: 2,
                factor: 1.3
            },
        ]
    );
    harness.get_by_label("× 1.3 of the weather case's");
    harness.state_mut().ready().unwrap().model.undo().unwrap();
    assert!(factors(&mut harness).is_empty());
}

/// Criteria › Loads for structures lists the line's load cases with a
/// broken wire; a tick changes the choice (the menu sends it as one
/// undoable edit, as its other ticks).
#[test]
fn broken_wire_cases_are_ticked_for_the_structures() {
    let mut model = synthetic_project("broken-cases");
    let dir = model.sources.file.parent().unwrap().to_path_buf();
    let path = dir.join("broken.ohllc");
    std::fs::write(
        &path,
        "format = \"ohl-load-cases\"\nversion = 3\n[[case]]\nname = \"S1 broken\"\n\
         weather = \"everyday\"\n[case.broken]\nsection = \"S1\"\nspan = 2\n",
    )
    .unwrap();
    model.sources.load_cases = Some(path);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1500.0, 950.0))
        .build_ui_state(
            |ui, model: &mut crate::model::Model| {
                let lc = crate::structure_app::line_load_cases(model);
                let mut choice = model.line.structure_loads.clone();
                crate::structure_app::broken_cases_ui(ui, &lc, &mut choice);
                model.line.structure_loads = choice;
            },
            model,
        );
    harness.run();
    harness.get_by_label("Broken-wire cases sent");
    harness
        .get_by_label("S1 broken (everyday, S1 span 2 broken)")
        .click();
    harness.run();
    assert_eq!(
        harness.state().line.structure_loads.broken_cases,
        vec!["S1 broken".to_owned()]
    );
}
