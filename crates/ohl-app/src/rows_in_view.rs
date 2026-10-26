// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Long result lists drawn row by row, only the rows in view (performance
//! review item 7, 2026-10-26: the check windows' lists of wire tensions,
//! string checks, internal clearances, zone and vegetation findings and
//! the bill of materials were laid out whole every frame; OHL-Structure
//! has its own copy, `rows`, the two programs not depending on each
//! other). Display only.
//!
//! The rows have one height; the table takes the room of all of them in
//! the scroll area it is in, and only those its view (the clip
//! rectangle) shows are drawn, striped as a grid's. The columns keep
//! their widths from frame to frame (in egui's memory), each the widest
//! of its cells drawn so far, so they do not jump as the rows in view
//! change. A row asked for ([`Table::rows`]' `scroll_to`) is brought into
//! view by its place, as drawing it did before.

use eframe::egui;
use egui::{Align, Layout, Rect, Ui, UiBuilder, Vec2, vec2};

/// A table's columns: their widths, kept in egui's memory under its id.
pub(crate) struct Table {
    id: egui::Id,
    widths: Vec<f32>,
    grew: bool,
    /// The gaps between cells and rows (none: the style's).
    spacing: Option<Vec2>,
}

/// The cells of one row (or of the header), each in its column's width.
pub(crate) struct Cells<'a> {
    /// The row's own `Ui` (cells left to right).
    pub ui: &'a mut Ui,
    widths: &'a mut [f32],
    grew: &'a mut bool,
    k: usize,
}

impl Cells<'_> {
    /// The next cell, drawn by `add`; its column widens to it when it
    /// is wider (from the next frame).
    pub fn cell<R>(&mut self, add: impl FnOnce(&mut Ui) -> R) -> R {
        let k = self.k.min(self.widths.len().saturating_sub(1));
        self.k += 1;
        let w = self.widths.get(k).copied().unwrap_or(40.0);
        let h = self.ui.spacing().interact_size.y;
        let r = self.ui.allocate_ui_with_layout(
            vec2(w, h),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.set_min_width(w);
                add(ui)
            },
        );
        let used = r.response.rect.width().ceil();
        if let Some(width) = self.widths.get_mut(k)
            && used > *width
        {
            *width = used;
            *self.grew = true;
        }
        r.inner
    }
}

impl Table {
    /// The table of `columns` columns with this id.
    pub fn new(ui: &Ui, id: &str, columns: usize) -> Self {
        let id = ui.id().with(("rows-table", id));
        let mut widths: Vec<f32> = ui.data(|d| d.get_temp(id)).unwrap_or_default();
        widths.resize(columns, 0.0);
        Self {
            id,
            widths,
            grew: false,
            spacing: None,
        }
    }

    /// The gaps between cells (`x`) and rows (`y`), as a grid's spacing.
    pub fn spacing(mut self, x: f32, y: f32) -> Self {
        self.spacing = Some(vec2(x, y));
        self
    }

    /// The gaps in use.
    fn gaps(&self, ui: &Ui) -> Vec2 {
        self.spacing.unwrap_or(ui.spacing().item_spacing)
    }

    /// The header: each column's title (bold) with its tip.
    pub fn header(&mut self, ui: &mut Ui, columns: &[(&str, &str)]) {
        self.header_cells(ui, |cells| {
            for (title, tip) in columns {
                cells.cell(|ui| {
                    let r = ui.strong(*title);
                    if !tip.is_empty() {
                        r.on_hover_text(*tip);
                    }
                });
            }
        });
    }

    /// The header drawn by `draw`, a cell per column.
    pub fn header_cells(&mut self, ui: &mut Ui, draw: impl FnOnce(&mut Cells<'_>)) {
        let gaps = self.gaps(ui);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gaps.x;
            let mut cells = Cells {
                ui,
                widths: &mut self.widths,
                grew: &mut self.grew,
                k: 0,
            };
            draw(&mut cells);
        });
        ui.add_space((gaps.y - ui.spacing().item_spacing.y).max(0.0));
    }

    /// The width of a row, all columns.
    fn width(&self, ui: &Ui) -> f32 {
        let gap = self.gaps(ui).x;
        self.widths.iter().map(|w| w + gap).sum::<f32>()
    }

    /// `n` rows, `draw` drawing row `i`'s cells; striped. With
    /// `scroll_to`, that row is brought into view (centred), as the
    /// table's row of a pick made elsewhere is.
    pub fn rows(
        &mut self,
        ui: &mut Ui,
        n: usize,
        scroll_to: Option<usize>,
        mut draw: impl FnMut(&mut Cells<'_>, usize),
    ) {
        let gaps = self.gaps(ui);
        let height = ui.spacing().interact_size.y;
        let step = height + gaps.y;
        let width = self.width(ui).max(1.0);
        let top = ui.cursor().min;
        let total = vec2(width, (n as f32 * step - gaps.y).max(0.0));
        let (all, _) = ui.allocate_exact_size(total, egui::Sense::hover());
        let row_rect = |i: usize| {
            Rect::from_min_size(
                egui::pos2(top.x, top.y + i as f32 * step),
                vec2(width, height),
            )
        };
        if let Some(i) = scroll_to.filter(|i| *i < n) {
            ui.scroll_to_rect(row_rect(i), Some(Align::Center));
        }
        let clip = ui.clip_rect().intersect(all);
        if clip.height() <= 0.0 && n > 0 {
            return self.keep(ui);
        }
        let (first, last) = in_view(clip.min.y - top.y, clip.max.y - top.y, step, n);
        for i in first..last {
            let rect = row_rect(i);
            if i % 2 == 1 {
                ui.painter().rect_filled(
                    rect.expand2(vec2(0.0, gaps.y / 2.0)),
                    0.0,
                    ui.visuals().faint_bg_color,
                );
            }
            let mut row = ui.new_child(
                UiBuilder::new()
                    .id_salt(("row", i))
                    .max_rect(rect)
                    .layout(Layout::left_to_right(Align::Center)),
            );
            row.spacing_mut().item_spacing.x = gaps.x;
            row.set_clip_rect(rect.intersect(ui.clip_rect()));
            let mut cells = Cells {
                ui: &mut row,
                widths: &mut self.widths,
                grew: &mut self.grew,
                k: 0,
            };
            draw(&mut cells, i);
        }
        self.keep(ui);
    }

    /// Keeps the widths for the next frame (another frame when a column
    /// grew).
    fn keep(&mut self, ui: &Ui) {
        ui.data_mut(|d| d.insert_temp(self.id, self.widths.clone()));
        if self.grew {
            ui.ctx().request_repaint();
            self.grew = false;
        }
    }
}

/// The rows (first, one past the last) of `n` rows `step` apart that a
/// view from `from` to `to` (down from the first row's top) shows.
fn in_view(from: f32, to: f32, step: f32, n: usize) -> (usize, usize) {
    let first = ((from / step).floor().max(0.0) as usize).min(n);
    let last = ((to / step).ceil().max(0.0) as usize + 1).min(n);
    (first, last.max(first))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_rows_in_view() {
        assert_eq!(in_view(0.0, 100.0, 25.0, 1000), (0, 5));
        assert_eq!(in_view(250.0, 350.0, 25.0, 1000), (10, 15));
        assert_eq!(in_view(250.0, 350.0, 25.0, 12), (10, 12));
        assert_eq!(in_view(-50.0, 10.0, 25.0, 3), (0, 2));
        assert_eq!(in_view(0.0, 100.0, 25.0, 0), (0, 0));
    }

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
