// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Long result lists drawn row by row, only the rows in view: the shared
//! table ([`ohl_ui::rows`]).

pub use ohl_ui::rows::Table;

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui;
    use egui::vec2;

    /// A long list in a scroll area draws only the rows in view; a row
    /// asked for is scrolled to and drawn.
    #[test]
    fn a_long_list_draws_the_rows_in_view_and_scrolls_to_a_row() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;
        let mut harness = Harness::builder()
            .with_size(vec2(400.0, 300.0))
            .build_ui_state(
                |ui, target: &mut Option<usize>| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let mut t = Table::new(ui, "test", 2).spacing(12.0, 3.0);
                        t.header(ui, &[("row", ""), ("value", "")]);
                        t.rows(ui, 5000, target.take(), |c, i| {
                            c.cell(|ui| ui.label(format!("row {i}")));
                            c.cell(|ui| ui.label(format!("{}", i * 2)));
                        });
                    });
                },
                None,
            );
        harness.run();
        assert!(harness.query_by_label("row 0").is_some());
        assert!(harness.query_by_label("row 4999").is_none());
        let drawn = (0..200)
            .filter(|i| harness.query_by_label(&format!("row {i}")).is_some())
            .count();
        assert!(drawn < 30, "{drawn} rows drawn");
        *harness.state_mut() = Some(4000);
        harness.run();
        harness.run();
        assert!(harness.query_by_label("row 4000").is_some());
        assert!(harness.query_by_label("row 0").is_none());
    }
}
