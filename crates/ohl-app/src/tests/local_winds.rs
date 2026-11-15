// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests: local winds per span (roadmap O15) in the Sections table and a
//! span's Properties. (The broken-wire cases sent to the structures of
//! Criteria › Loads for structures are gone since 2026-11-15: a
//! broken-wire case reaches a structure through its groups.)

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
