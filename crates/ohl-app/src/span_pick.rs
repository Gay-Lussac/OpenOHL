// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The spans a field window computes in, and what it computes in each
//! (owner, 2026-10-30: "a list of all the spans … select and deselect
//! those we want …, the ones on the alignment first and the other ones
//! after; by default the ones outside the alignment are not wanted";
//! 2026-10-31, validated: one table, a tick dragged across rows to flip
//! them, per span the stations of its 2D cuts, its volume
//! and its isolines from above): every span ticked or not, listed by
//! alignment (its spans in station order), then the spans on none; a span
//! on an alignment ticked unless unticked, one on none unticked unless
//! ticked. Shared by the Magnetic and the Electric field windows.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use eframe::egui;

use crate::magnetic_field::Spans;

/// A span by its first and last structure.
type SpanIds = (String, String);

/// The spans of one alignment (or of none): its name and the spans'
/// indices into the line's spans, in order along it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Group {
    /// The alignment's name; `None`: the other spans (on no alignment,
    /// another line's, connections; [`groups`]).
    pub alignment: Option<String>,
    pub spans: Vec<usize>,
}

/// The line's spans by alignment: each alignment's spans in station
/// order, a span on several in the first only; then the others, in the
/// line's order. A span is an alignment's when it follows the line's run
/// along it ([`ohl_line::AlignmentView::consecutive`]), or leaves the
/// run's first or last structure for one off the run (the span into a
/// substation): another line's spans under or beside it (a crossing
/// line) and connections from a structure in the middle of the run go
/// last (owner, 2026-10-31: "I don't really want to have the main line
/// span constantly interrupted by cross spans").
pub(crate) fn groups(line: &ohl_line::Line, spans: &[SpanIds]) -> Vec<Group> {
    let mut taken = vec![false; spans.len()];
    let mut out = Vec::new();
    for view in line.alignment_views() {
        let at: BTreeMap<&str, usize> = view
            .run()
            .enumerate()
            .map(|(k, s)| (s.as_str(), k))
            .collect();
        let last = at.len().saturating_sub(1);
        let mut on: Vec<(usize, usize)> = spans
            .iter()
            .enumerate()
            .filter(|(k, _)| !taken[*k])
            .filter_map(|(k, (a, b))| {
                let place = match (at.get(a.as_str()), at.get(b.as_str())) {
                    (Some(&i), Some(&j)) if view.consecutive(a, b) => i.min(j),
                    // Out of the run's end to a structure off the run.
                    (Some(&i), None) | (None, Some(&i)) if i == 0 || i == last => i,
                    _ => return None,
                };
                Some((place, k))
            })
            .collect();
        if on.is_empty() {
            continue;
        }
        on.sort();
        for &(_, k) in &on {
            taken[k] = true;
        }
        let name = if view.name.is_empty() {
            view.id.clone()
        } else {
            view.name.clone()
        };
        out.push(Group {
            alignment: Some(name),
            spans: on.into_iter().map(|(_, k)| k).collect(),
        });
    }
    let rest: Vec<usize> = (0..spans.len()).filter(|k| !taken[*k]).collect();
    if !rest.is_empty() {
        out.push(Group {
            alignment: None,
            spans: rest,
        });
    }
    out
}

/// Where a 2D cut is asked in a span (owner, 2026-10-31: "for the 2D cuts
/// it should be asked at which stations we want one"): at its middle, at
/// its lowest point, or this far from its first structure along it, m.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CutAt {
    Mid,
    Lowest,
    From(f64),
}

impl CutAt {
    /// As its chip says it.
    pub fn label(self) -> String {
        match self {
            CutAt::Mid => "mid".into(),
            CutAt::Lowest => "lowest".into(),
            CutAt::From(m) => format!("{m:.0} m"),
        }
    }

    /// The place in a span of horizontal length `len` (0–1); none: its
    /// lowest point (found by the run).
    pub fn fraction(self, len: f64) -> Option<f64> {
        match self {
            CutAt::Mid => Some(0.5),
            CutAt::Lowest => None,
            CutAt::From(m) => Some((m / len.max(1e-9)).clamp(0.0, 1.0)),
        }
    }

    /// The cut of this chip in span `ids` as the engine takes it (owner,
    /// 2026-10-31: "cuts are made on the alignment"): the middle and a
    /// distance at the alignment station of that place (between the
    /// stations of the span's structures on its alignment); the lowest
    /// point, found by the run, placed by it at its alignment station
    /// likewise. A span on no alignment: why no cut can be placed.
    pub fn place(
        self,
        line: &ohl_line::Line,
        views: &[ohl_line::route::AlignmentView],
        ids: &SpanIds,
        len: f64,
    ) -> Result<ohl_electrical::magnetic::CutAt, String> {
        use ohl_electrical::magnetic::CutAt as At;
        match self.fraction(len) {
            Some(f) => {
                let (alignment, station) = line.span_station(views, &ids.0, &ids.1, f)?;
                Ok(At::Station { alignment, station })
            }
            None => {
                line.span_alignment(views, &ids.0, &ids.1)?;
                Ok(At::InSpan {
                    from: ids.0.clone(),
                    to: ids.1.clone(),
                    fraction: None,
                })
            }
        }
    }
}

/// A column of ticks that a drag runs along ([`SpanPick::table`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Col {
    /// The span computed.
    Span,
    /// Its volume.
    Volume,
    /// Its isolines from above.
    Above,
}

/// Which columns the table offers besides the spans' ticks.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Columns {
    /// Isolines from above (the magnetic field).
    pub above: bool,
    /// Cuts and volumes (3D only).
    pub three_d: bool,
}

/// Which spans are ticked — the changes from the default (on an
/// alignment: ticked), kept in the project by the spans' structures —
/// and per span what is computed in it (owner, 2026-10-31: the stations of
/// its 2D cuts, its volume, its isolines from above).
#[derive(Debug, Clone, Default)]
pub(crate) struct SpanPick {
    /// Spans on an alignment unticked.
    pub off: BTreeSet<SpanIds>,
    /// Spans of no alignment's group ticked ([`groups`]).
    pub on: BTreeSet<SpanIds>,
    /// The 2D cuts asked per span.
    pub cuts: BTreeMap<SpanIds, Vec<CutAt>>,
    /// The spans with a volume.
    pub volume: BTreeSet<SpanIds>,
    /// The spans with isolines from above.
    pub above: BTreeSet<SpanIds>,
    /// The groups, by line version.
    cache: Option<(u64, Arc<Vec<Group>>)>,
    /// A press on a tick dragged across rows: its column and the state it
    /// gives every row it passes (owner: "you drag the mouse across while
    /// holding the click to flip the choices").
    drag: Option<(Col, bool)>,
    /// The row pressed last (shift-click: the range from it).
    anchor: Option<usize>,
    /// Only the spans whose name holds this.
    filter: String,
    /// The distance of a station being added, m.
    at: f64,
}

impl SpanPick {
    /// The groups of the line's spans (listed once per version).
    pub fn groups(&mut self, model: &crate::model::Model, spans: &Spans) -> Arc<Vec<Group>> {
        if let Some((v, g)) = &self.cache
            && *v == model.line_version
        {
            return g.clone();
        }
        let g = Arc::new(groups(&model.line, spans));
        self.cache = Some((model.line_version, g.clone()));
        g
    }

    /// Whether span `ids` is ticked, `aligned`: on an alignment.
    pub fn ticked(&self, ids: &SpanIds, aligned: bool) -> bool {
        if aligned {
            !self.off.contains(ids)
        } else {
            self.on.contains(ids)
        }
    }

    fn set(&mut self, ids: &SpanIds, aligned: bool, tick: bool) {
        let (set, add) = if aligned {
            (&mut self.off, !tick)
        } else {
            (&mut self.on, tick)
        };
        if add {
            set.insert(ids.clone());
        } else {
            set.remove(ids);
        }
    }

    /// Column `col` of span `ids`.
    fn get_col(&self, col: Col, ids: &SpanIds, aligned: bool) -> bool {
        match col {
            Col::Span => self.ticked(ids, aligned),
            Col::Volume => self.volume.contains(ids),
            Col::Above => self.above.contains(ids),
        }
    }

    fn set_col(&mut self, col: Col, ids: &SpanIds, aligned: bool, v: bool) {
        let set = match col {
            Col::Span => return self.set(ids, aligned, v),
            Col::Volume => &mut self.volume,
            Col::Above => &mut self.above,
        };
        if v {
            set.insert(ids.clone());
        } else {
            set.remove(ids);
        }
    }

    /// The spans ticked (indices into the line's spans, listed order) and
    /// whether that is every span.
    pub fn chosen(&mut self, model: &crate::model::Model, spans: &Spans) -> (Vec<usize>, bool) {
        let groups = self.groups(model, spans);
        let this = &*self;
        let out: Vec<usize> = groups
            .iter()
            .flat_map(|g| {
                let aligned = g.alignment.is_some();
                g.spans
                    .iter()
                    .copied()
                    .filter(move |&k| this.ticked(&spans[k], aligned))
            })
            .collect();
        let all = out.len() == spans.len();
        (out, all)
    }

    /// The ticked spans in `set`, in the line's order, as runs of spans
    /// following each other: each run's first and last structure and how
    /// many spans it holds.
    fn runs_of(
        &mut self,
        model: &crate::model::Model,
        spans: &Spans,
        set: &BTreeSet<SpanIds>,
    ) -> Vec<(SpanIds, usize)> {
        let (mut chosen, _) = self.chosen(model, spans);
        chosen.sort_unstable();
        let mut out: Vec<(SpanIds, usize)> = Vec::new();
        for k in chosen.into_iter().filter(|&k| set.contains(&spans[k])) {
            let (a, b) = &spans[k];
            match out.last_mut() {
                Some(last) if last.0.1 == *a => {
                    last.0.1 = b.clone();
                    last.1 += 1;
                }
                _ => out.push(((a.clone(), b.clone()), 1)),
            }
        }
        out
    }

    /// The volumes: the runs of ticked spans with a volume ticked that
    /// follow each other, and how many spans each holds.
    pub fn volume_runs(
        &mut self,
        model: &crate::model::Model,
        spans: &Spans,
    ) -> Vec<(SpanIds, usize)> {
        let set = self.volume.clone();
        self.runs_of(model, spans, &set)
    }

    /// The grids from above: the runs of ticked spans with isolines from
    /// above, as [`SpanPick::volume_runs`].
    pub fn above_runs(
        &mut self,
        model: &crate::model::Model,
        spans: &Spans,
    ) -> Vec<(SpanIds, usize)> {
        let set = self.above.clone();
        self.runs_of(model, spans, &set)
    }

    /// The cuts asked in the ticked spans, (span, where), in the line's
    /// order.
    pub fn cuts_asked(
        &mut self,
        model: &crate::model::Model,
        spans: &Spans,
    ) -> Vec<(SpanIds, CutAt)> {
        let (mut chosen, _) = self.chosen(model, spans);
        chosen.sort_unstable();
        let mut out = Vec::new();
        for k in chosen {
            for c in self.cuts.get(&spans[k]).into_iter().flatten() {
                out.push((spans[k].clone(), *c));
            }
        }
        out
    }

    /// The cuts asked in the ticked spans as the engine places them
    /// ([`CutAt::place`]), in the line's order; and, for those that
    /// cannot be placed (a span on no alignment), why — once per span.
    pub fn cut_places(
        &mut self,
        model: &crate::model::Model,
        spans: &Spans,
    ) -> (Vec<ohl_electrical::magnetic::CutAt>, Vec<String>) {
        let asked = self.cuts_asked(model, spans);
        if asked.is_empty() {
            return (Vec::new(), Vec::new());
        }
        let views = model.line.alignment_views();
        let mut placed = Vec::new();
        let mut why: Vec<String> = Vec::new();
        for (ids, at) in asked {
            match at.place(&model.line, &views, &ids, span_length(model, &ids)) {
                Ok(c) => placed.push(c),
                Err(e) => {
                    if !why.contains(&e) {
                        why.push(e);
                    }
                }
            }
        }
        (placed, why)
    }

    /// Ticks span `k` only (tests).
    #[cfg(test)]
    pub fn only(&mut self, model: &crate::model::Model, spans: &Spans, k: usize) {
        let groups = self.groups(model, spans);
        for g in groups.iter() {
            let aligned = g.alignment.is_some();
            for &j in &g.spans {
                self.set(&spans[j], aligned, j == k);
            }
        }
    }

    /// The table (owner, 2026-10-31, validated): every span a row —
    /// alignment by alignment, then those on none — with its tick (an
    /// unticked row greyed), its length, its profile, the stations of its
    /// 2D cuts, its volume and (`cols.above`) its isolines from above. A
    /// press on a tick dragged across rows gives each the state the press
    /// gave; shift-click: the range from the row pressed before; a filter;
    /// All, None and Invert on the rows shown. `id` keeps its height and
    /// scroll apart from another window's.
    pub fn table(
        &mut self,
        ui: &mut egui::Ui,
        id: &str,
        model: &crate::model::Model,
        spans: &Spans,
        cols: Columns,
    ) {
        let groups = self.groups(model, spans);
        let (chosen, _) = self.chosen(model, spans);
        let needle = self.filter.trim().to_lowercase();
        let name = |k: usize| format!("{}–{}", spans[k].0, spans[k].1);
        // The rows shown, in order: (span, on an alignment, its group).
        let rows: Vec<(usize, bool)> = groups
            .iter()
            .flat_map(|g| g.spans.iter().map(move |&k| (k, g.alignment.is_some())))
            .filter(|&(k, _)| needle.is_empty() || name(k).to_lowercase().contains(&needle))
            .collect();
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("filter: T2")
                    .desired_width(130.0),
            )
            .on_hover_text("Only the spans whose name holds this");
            let all = |this: &mut Self, v: Option<bool>| {
                for &(k, aligned) in &rows {
                    let now = this.ticked(&spans[k], aligned);
                    this.set(&spans[k], aligned, v.unwrap_or(!now));
                }
            };
            if ui
                .small_button("All")
                .on_hover_text("Tick the spans shown")
                .clicked()
            {
                all(self, Some(true));
            }
            if ui
                .small_button("None")
                .on_hover_text("Untick the spans shown")
                .clicked()
            {
                all(self, Some(false));
            }
            if ui
                .small_button("Invert")
                .on_hover_text("Flip the spans shown")
                .clicked()
            {
                all(self, None);
            }
            ui.label(
                egui::RichText::new(format!(
                    "{} of {} ticked — drag across the ticks to flip them; shift-click a range",
                    chosen.len(),
                    spans.len()
                ))
                .small()
                .weak(),
            );
        });
        // A drag ends with the button.
        if !ui.input(|i| i.pointer.primary_down()) {
            self.drag = None;
        }
        let pressed = ui.input(|i| i.pointer.primary_pressed());
        let shift = ui.input(|i| i.modifiers.shift);
        crate::check_help::list(
            ui,
            (id, "span-table"),
            260.0,
            // At least its height (owner: "the selection … window for the
            // spans is too small"; egui shrinks a list to 64 px otherwise).
            egui::ScrollArea::vertical()
                .id_salt((id, "span-table-scroll"))
                .min_scrolled_height(crate::check_help::list_height(
                    ui.ctx(),
                    (id, "span-table"),
                    260.0,
                )),
            |ui| {
                egui::Grid::new((id, "span-grid"))
                    .num_columns(if cols.above { 7 } else { 6 })
                    .striped(true)
                    .spacing([12.0, 3.0])
                    .show(ui, |ui| {
                        ui.label("");
                        ui.strong("Span");
                        ui.strong("Length");
                        ui.strong("Profile");
                        ui.strong("2D cuts at");
                        ui.strong("Volume");
                        if cols.above {
                            ui.strong("From above");
                        }
                        ui.end_row();
                        let mut group: Option<Option<String>> = None;
                        for row in 0..rows.len() {
                            let (k, aligned) = rows[row];
                            let alignment = groups
                                .iter()
                                .find(|g| g.spans.contains(&k))
                                .and_then(|g| g.alignment.clone());
                            if group.as_ref() != Some(&alignment) {
                                ui.label("");
                                ui.label(
                                    egui::RichText::new(match &alignment {
                                        Some(a) => format!("Alignment {a}"),
                                        None => "Other spans: other lines, connections, off the alignments (unticked unless ticked)".into(),
                                    })
                                    .small()
                                    .strong(),
                                );
                                ui.end_row();
                                group = Some(alignment);
                            }
                            let ids = spans[k].clone();
                            self.tick_cell(
                                ui,
                                Col::Span,
                                &rows,
                                row,
                                spans,
                                (pressed, shift),
                                true,
                            );
                            let on = self.ticked(&ids, aligned);
                            let text = |t: String| {
                                if on {
                                    egui::RichText::new(t)
                                } else {
                                    egui::RichText::new(t).weak()
                                }
                            };
                            ui.label(text(name(k)));
                            let len = span_length(model, &ids);
                            ui.label(text(format!("{len:.0} m")));
                            ui.label(text(if on { "yes".into() } else { String::new() }));
                            ui.horizontal(|ui| {
                                ui.add_enabled_ui(on && cols.three_d, |ui| {
                                    self.cuts_cell(ui, &ids, len);
                                });
                            });
                            self.tick_cell(
                                ui,
                                Col::Volume,
                                &rows,
                                row,
                                spans,
                                (pressed, shift),
                                on && cols.three_d,
                            );
                            if cols.above {
                                self.tick_cell(
                                    ui,
                                    Col::Above,
                                    &rows,
                                    row,
                                    spans,
                                    (pressed, shift),
                                    on,
                                );
                            }
                            ui.end_row();
                        }
                    });
            },
        );
    }

    /// The stations of span `ids`'s cuts as chips (a click removes one)
    /// and "+ station" (at its middle, its lowest point, or a distance
    /// from its first structure).
    fn cuts_cell(&mut self, ui: &mut egui::Ui, ids: &SpanIds, len: f64) {
        let mut remove = None;
        for (n, c) in self.cuts.get(ids).into_iter().flatten().enumerate() {
            if ui
                .small_button(format!("{} ×", c.label()))
                .on_hover_text("Remove this cut")
                .clicked()
            {
                remove = Some(n);
            }
        }
        if let Some(n) = remove
            && let Some(list) = self.cuts.get_mut(ids)
        {
            list.remove(n);
            if list.is_empty() {
                self.cuts.remove(ids);
            }
        }
        let mut add = None;
        let at = &mut self.at;
        ui.menu_button("+ station", |ui| {
            if ui.button("at mid-span").clicked() {
                add = Some(CutAt::Mid);
            }
            if ui.button("at its lowest point").clicked() {
                add = Some(CutAt::Lowest);
            }
            ui.horizontal(|ui| {
                crate::style::field(
                    ui,
                    egui::DragValue::new(at)
                        .range(0.0..=len.max(1.0))
                        .speed(1.0)
                        .suffix(" m"),
                );
                if ui
                    .button(format!("from {}", ids.0))
                    .on_hover_text("This far along the span from its first structure")
                    .clicked()
                {
                    add = Some(CutAt::From(*at));
                }
            });
        })
        .response
        .on_hover_text("A vertical cut across the span, computed with the run");
        if let Some(c) = add {
            let list = self.cuts.entry(ids.clone()).or_default();
            if !list.contains(&c) {
                list.push(c);
            }
        }
    }

    /// A tick of column `col` in row `row` of `rows`: a press flips it and
    /// starts a drag that gives every row passed the same state (with
    /// shift: the range from the row pressed before); `enabled` false:
    /// drawn faint, not changed. `(pressed, shift)`: the pointer's button
    /// pressed this frame, the shift key held.
    #[allow(clippy::too_many_arguments)]
    fn tick_cell(
        &mut self,
        ui: &mut egui::Ui,
        col: Col,
        rows: &[(usize, bool)],
        row: usize,
        spans: &Spans,
        (pressed, shift): (bool, bool),
        enabled: bool,
    ) {
        let (k, aligned) = rows[row];
        let ids = &spans[k];
        let value = self.get_col(col, ids, aligned);
        let (rect, resp) =
            ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::click_and_drag());
        let what = match col {
            Col::Span => "span",
            Col::Volume => "volume",
            Col::Above => "from above",
        };
        let label = format!("{what} {}–{}", ids.0, ids.1);
        resp.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::Checkbox, enabled, value, &label)
        });
        // Over it: by the pointer's place (a drag started on another tick
        // keeps egui's hover on that one).
        let over = ui
            .input(|i| i.pointer.latest_pos())
            .is_some_and(|p| rect.contains(p) && ui.is_rect_visible(rect));
        if enabled && over {
            if pressed {
                let v = !value;
                match self.anchor {
                    Some(a) if shift => {
                        let (lo, hi) = (a.min(row), a.max(row).min(rows.len() - 1));
                        for &(j, al) in &rows[lo..=hi] {
                            self.set_col(col, &spans[j], al, v);
                        }
                    }
                    _ => self.set_col(col, ids, aligned, v),
                }
                self.drag = Some((col, v));
                self.anchor = Some(row);
            } else if let Some((c, v)) = self.drag
                && c == col
                && value != v
            {
                self.set_col(col, ids, aligned, v);
            }
        }
        // Drawn as a check box (faint when it cannot change).
        let value = self.get_col(col, ids, aligned);
        let colour = if enabled {
            ui.style().interact(&resp).fg_stroke.color
        } else {
            ui.visuals().weak_text_color()
        };
        let r = rect.shrink(2.0);
        ui.painter().rect_stroke(
            r,
            2.0,
            egui::Stroke::new(1.0, colour),
            egui::StrokeKind::Inside,
        );
        if value {
            ui.painter().line(
                vec![
                    egui::pos2(r.left() + 2.5, r.center().y),
                    egui::pos2(r.center().x - 1.0, r.bottom() - 3.0),
                    egui::pos2(r.right() - 2.5, r.top() + 3.0),
                ],
                egui::Stroke::new(1.6, colour),
            );
        }
        resp.on_hover_text("Press and drag across the rows to give them all this state");
    }
}

/// A span's horizontal length between its structures, m.
pub(crate) fn span_length(model: &crate::model::Model, ids: &SpanIds) -> f64 {
    let at = |id: &str| model.line.structure(id).map(|s| s.position);
    match (at(&ids.0), at(&ids.1)) {
        (Some(a), Some(b)) => (b[0] - a[0]).hypot(b[1] - a[1]),
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The synthetic line: the main alignment's spans first, the
    /// branch's after; a span on no alignment last and unticked.
    #[test]
    fn spans_are_listed_by_alignment_and_ticked_on_them() {
        let line = crate::tests::synthetic().line;
        let spans: Vec<SpanIds> = ohl_electrical::cross_section::line_spans(&line)
            .into_iter()
            .map(|s| (s.from, s.to))
            .collect();
        let g = groups(&line, &spans);
        let all: Vec<usize> = g.iter().flat_map(|g| g.spans.clone()).collect();
        let mut sorted = all.clone();
        sorted.sort();
        assert_eq!(
            sorted,
            (0..spans.len()).collect::<Vec<_>>(),
            "each span once"
        );
        assert!(g[0].alignment.is_some());
        let mut pick = SpanPick::default();
        let x = ("X".to_owned(), "Y".to_owned());
        assert!(pick.ticked(&spans[g[0].spans[0]], true));
        assert!(!pick.ticked(&x, false));
        pick.set(&x, false, true);
        assert!(pick.ticked(&x, false));
        pick.set(&spans[0], true, false);
        assert!(!pick.ticked(&spans[0], true));
        assert!(pick.off.contains(&spans[0]));
    }

    /// Another line crossing under the main line's first span (two towers
    /// either side of it, near the alignment): its span goes last, after
    /// every alignment's, and the main alignment's spans stay together.
    #[test]
    fn a_crossing_line_goes_last() {
        let mut line = crate::tests::synthetic().line;
        let ids = |line: &ohl_line::Line| -> Vec<SpanIds> {
            ohl_electrical::cross_section::line_spans(line)
                .into_iter()
                .map(|s| (s.from, s.to))
                .collect()
        };
        let before = groups(&line, &ids(&line));
        let main: Vec<SpanIds> = {
            let spans = ids(&line);
            before[0].spans.iter().map(|&k| spans[k].clone()).collect()
        };
        let view = line.alignment_views().remove(0);
        let (a, b) = (view.run().next().unwrap(), view.run().nth(1).unwrap());
        let (pa, pb) = (
            line.structure(a).unwrap().position,
            line.structure(b).unwrap().position,
        );
        let mid = [0.5 * (pa[0] + pb[0]), 0.5 * (pa[1] + pb[1])];
        let n = (pb[0] - pa[0]).hypot(pb[1] - pa[1]);
        let across = [(pb[1] - pa[1]) / n, -(pb[0] - pa[0]) / n];
        for (id, side) in [("X1", 15.0), ("X2", -15.0)] {
            let mut c = line.structures[0].clone();
            c.id = id.into();
            c.position = [
                mid[0] + side * across[0],
                mid[1] + side * across[1],
                pa[2] - 10.0,
            ];
            c.routes = vec![view.route.clone()];
            line.structures.push(c);
        }
        let mut cross = line.sections[0].clone();
        cross.id = "cross".into();
        cross.points = vec![
            ohl_line::SectionPoint {
                structure: "X1".into(),
                set: cross.points[0].set,
            },
            ohl_line::SectionPoint {
                structure: "X2".into(),
                set: cross.points[0].set,
            },
        ];
        line.sections.push(cross);
        // Both stand on the main alignment (by both structures on it, the
        // span was the alignment's), off its run.
        let view = line.alignment_views().remove(0);
        assert!(
            ["X1", "X2"]
                .iter()
                .all(|x| view.structures.iter().any(|s| s == x))
        );
        assert!(!view.consecutive("X1", "X2"));
        let spans = ids(&line);
        let g = groups(&line, &spans);
        let first: Vec<SpanIds> = g[0].spans.iter().map(|&k| spans[k].clone()).collect();
        assert_eq!(first, main, "the main alignment's spans as before");
        let last = g.last().unwrap();
        assert!(last.alignment.is_none());
        assert!(
            last.spans
                .iter()
                .any(|&k| spans[k] == ("X1".to_owned(), "X2".to_owned())
                    || spans[k] == ("X2".to_owned(), "X1".to_owned())),
            "{g:?}"
        );
    }

    /// A cut's place: the middle, the lowest point (the run finds it), a
    /// distance from the first structure (held within the span).
    #[test]
    fn a_cut_station_is_a_place_in_its_span() {
        assert_eq!(CutAt::Mid.fraction(300.0), Some(0.5));
        assert_eq!(CutAt::Lowest.fraction(300.0), None);
        assert_eq!(CutAt::From(75.0).fraction(300.0), Some(0.25));
        assert_eq!(CutAt::From(500.0).fraction(300.0), Some(1.0));
        assert_eq!(CutAt::From(120.0).label(), "120 m");
    }
}
