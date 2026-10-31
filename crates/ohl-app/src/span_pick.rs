// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The spans a field window computes its profiles in (owner, 2026-10-30:
//! "a list of all the spans … select and deselect those we want …, the
//! ones on the alignment first and the other ones after; by default the
//! ones outside the alignment are not wanted"): every span ticked or not,
//! listed by alignment (its spans in station order), then the spans on
//! none; a span on an alignment ticked unless unticked, one on none
//! unticked unless ticked. Shared by the Magnetic and the Electric field
//! windows.

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
    /// The alignment's name; `None`: the spans on no alignment.
    pub alignment: Option<String>,
    pub spans: Vec<usize>,
}

/// The line's spans by alignment: each alignment's spans (both
/// structures on it) in station order, a span on several in the first
/// only; then the spans on none, in the line's order.
pub(crate) fn groups(line: &ohl_line::Line, spans: &[SpanIds]) -> Vec<Group> {
    let mut taken = vec![false; spans.len()];
    let mut out = Vec::new();
    for view in line.alignment_views() {
        let at: BTreeMap<&str, usize> = view
            .structures
            .iter()
            .enumerate()
            .map(|(k, s)| (s.as_str(), k))
            .collect();
        let mut on: Vec<(usize, usize)> = spans
            .iter()
            .enumerate()
            .filter(|(k, _)| !taken[*k])
            .filter_map(|(k, (a, b))| {
                let (i, j) = (at.get(a.as_str())?, at.get(b.as_str())?);
                Some(((*i).min(*j), k))
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

/// Which spans are ticked: the changes from the default (on an
/// alignment: ticked), kept in the project by the spans' structures.
#[derive(Debug, Clone, Default)]
pub(crate) struct SpanPick {
    /// Spans on an alignment unticked.
    pub off: BTreeSet<SpanIds>,
    /// Spans on no alignment ticked.
    pub on: BTreeSet<SpanIds>,
    /// The groups, by line version.
    cache: Option<(u64, Arc<Vec<Group>>)>,
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

    /// The spans ticked in the line's order and as runs of spans
    /// following each other (each run's first and last structure): what
    /// the isolines from above and the volumes are computed over.
    pub fn runs(&mut self, model: &crate::model::Model, spans: &Spans) -> Vec<SpanIds> {
        let (mut chosen, _) = self.chosen(model, spans);
        chosen.sort_unstable();
        let ticked: Vec<SpanIds> = chosen.into_iter().map(|k| spans[k].clone()).collect();
        crate::magnetic_field::runs(&ticked)
    }

    /// How many spans each run of ticked spans holds ([`SpanPick::runs`]).
    pub fn run_lengths(&mut self, model: &crate::model::Model, spans: &Spans) -> Vec<usize> {
        let (mut chosen, _) = self.chosen(model, spans);
        chosen.sort_unstable();
        let mut out: Vec<(String, usize)> = Vec::new();
        for k in chosen {
            let (a, b) = &spans[k];
            match out.last_mut() {
                Some(last) if last.0 == *a => {
                    last.0 = b.clone();
                    last.1 += 1;
                }
                _ => out.push((b.clone(), 1)),
            }
        }
        out.into_iter().map(|r| r.1).collect()
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

    /// The list: per alignment its spans with a tick each and *all* /
    /// *none*; `id` keeps the list's scroll apart from another window's.
    pub fn ui(&mut self, ui: &mut egui::Ui, id: &str, model: &crate::model::Model, spans: &Spans) {
        let groups = self.groups(model, spans);
        let (chosen, _) = self.chosen(model, spans);
        egui::CollapsingHeader::new(format!(
            "{} of {} ticked: the profiles, isolines and volumes over them",
            chosen.len(),
            spans.len()
        ))
        .id_salt((id, "spans"))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt((id, "span-list"))
                .max_height(220.0)
                .show(ui, |ui| {
                    for g in groups.iter() {
                        let aligned = g.alignment.is_some();
                        ui.horizontal(|ui| {
                            match &g.alignment {
                                Some(name) => ui.strong(format!("Alignment {name}")),
                                None => ui.strong("On no alignment").on_hover_text(
                                    "Spans whose structures are not both on one alignment: \
                                     unticked unless you tick them",
                                ),
                            };
                            if ui.small_button("all").clicked() {
                                for &k in &g.spans {
                                    self.set(&spans[k], aligned, true);
                                }
                            }
                            if ui.small_button("none").clicked() {
                                for &k in &g.spans {
                                    self.set(&spans[k], aligned, false);
                                }
                            }
                        });
                        ui.horizontal_wrapped(|ui| {
                            for &k in &g.spans {
                                let ids = &spans[k];
                                let mut tick = self.ticked(ids, aligned);
                                if ui
                                    .checkbox(&mut tick, format!("{}–{}", ids.0, ids.1))
                                    .changed()
                                {
                                    self.set(ids, aligned, tick);
                                }
                            }
                        });
                    }
                });
        });
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
}
