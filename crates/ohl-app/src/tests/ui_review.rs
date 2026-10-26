// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Tests of the OHL-Design UI review (owner, 2026-10-26): units in the
//! headers, notices, the short status sentence, the layer groups.

use super::*;
use crate::app::Tab;

fn window(model: model::Model) -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(|_| App::with_model(model, None));
    h.run();
    h
}

/// The units are in the column headers: a span's weather case table in
/// Properties ("H [kN]", "Sag [m]"), the Structures table ("E [m]").
#[test]
fn units_are_in_the_headers() {
    let mut h = window(synthetic());
    h.state_mut().ready().unwrap().selection = Selection::Span {
        section: "S1".into(),
        span: 0,
    };
    h.run();
    h.get_by_label("H [kN]");
    h.get_by_label("Sag [m]");
    h.get_by_label("Low z [m]");
    assert!(h.query_by_label("H (kN)").is_none());
    h.state_mut()
        .ready()
        .unwrap()
        .tables
        .entry(crate::tables::TableKind::Structures)
        .or_default();
    h.state_mut()
        .dock()
        .push_to_focused_leaf(Tab::Table(crate::tables::TableKind::Structures));
    h.run();
    assert!(
        h.query_all_by_label_contains("E [m]").next().is_some(),
        "the Structures table's easting with its unit"
    );
}

/// A message is also a notice: information fades after a few seconds, an
/// error stays until its × is clicked; the status line keeps it.
#[test]
fn an_error_notice_stays_until_closed() {
    let mut h = window(synthetic());
    h.state_mut().notices.hidden = false;
    h.state_mut().ready().unwrap().status = Some(("it went well".into(), false));
    h.run();
    // The status line and the notice.
    assert_eq!(h.query_all_by_label("it went well").count(), 2);
    for _ in 0..24 {
        h.step();
    }
    assert_eq!(
        h.query_all_by_label("it went well").count(),
        1,
        "the information faded (the status line keeps it)"
    );
    h.state_mut().ready().unwrap().status = Some(("it went wrong".into(), true));
    h.run();
    for _ in 0..40 {
        h.step();
    }
    assert_eq!(
        h.query_all_by_label("it went wrong").count(),
        2,
        "an error stays"
    );
    h.get_by_label("\u{00d7}").click();
    h.run();
    assert_eq!(h.query_all_by_label("it went wrong").count(), 1);
    assert!(h.state_mut().notices.items.is_empty());
}

/// An edit's message reads as a short sentence; the whole text is kept
/// (on hover).
#[test]
fn an_edit_is_reported_in_a_short_sentence() {
    let mut h = window(synthetic_project("ui-review-status"));
    let r = h.state_mut().ready().unwrap();
    let mut s = r.model.line.sections[0].clone();
    assert_eq!(s.id, "S1");
    (s.bundle, s.bundle_spacing) = (2, Some(0.4));
    let (id, value) = (s.id.clone(), serde_json::to_value(&s).unwrap());
    r.perform(crate::edit::Action::Command {
        what: "changed section S1".into(),
        command: ohl_session::Command::Put {
            kind: ohl_session::Kind::Section,
            id,
            value,
            ord: None,
        },
    });
    let (text, _) = r.status.clone().unwrap();
    assert!(text.contains("(log entry"), "{text}");
    h.run();
    h.get_by_label_contains("Section S1 changed");
    assert!(
        h.query_by_label(&text).is_none(),
        "the detail only on hover"
    );
}

/// The Layers list is in groups; a group folds, and stays folded.
#[test]
fn layer_groups_fold() {
    let mut h = window(synthetic());
    h.state_mut().show_tab(Tab::Model);
    h.run();
    h.get_by_label("Ground");
    h.get_by_label_contains("Ground & survey").click();
    h.run();
    assert!(h.query_by_label("Ground").is_none(), "folded");
    assert!(
        h.state_mut()
            .layer_groups
            .folded
            .contains("Ground & survey")
    );
    // The other groups stay open.
    h.get_by_label("Wires (selected case)");
    h.get_by_label_contains("Ground & survey").click();
    h.run();
    h.get_by_label("Ground");
    // Every layer in exactly one group.
    for layer in crate::scene::Layer::ALL {
        let n = crate::scene::LAYER_GROUPS
            .iter()
            .filter(|(_, ls)| ls.contains(&layer))
            .count();
        assert_eq!(n, 1, "{layer:?}");
    }
}
