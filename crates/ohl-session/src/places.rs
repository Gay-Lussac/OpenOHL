// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The index by place of the line worked on (country server, ADR-0013,
//! O17's exports' thread, 2026-11-20).
//!
//! A working set's extent ([`crate::working_set`]) used to work out the
//! routes of every object of the model and to test every section and
//! structure against an area: on a model of a whole country that was
//! most of an offline export's decision on the authority (0.6 s of 0.7 s).
//! The place index keeps, for the line worked on:
//!
//! - **each structure's routes** (the same function as the rules of shared
//!   towers: its own routes, the alignment points tied to it, the
//!   alignments passing near it) and **the structures of each route**;
//! - **the structures by cell** of a [`CELL`] m grid in plan;
//! - **the longest span** between two points of a section in plan (an
//!   upper bound: it only grows with the edits and is worked out again
//!   when the index is made).
//!
//! The index only **narrows the candidates**: every object it gives is
//! tested exactly as before (its routes worked out, its place tested
//! against the area), so an extent is the same with it as by the whole
//! scan (tested on every route and on areas, before and after edits). A
//! candidate set can only be too large, never too small:
//!
//! - an object on a route has a structure on that route: a section's
//!   routes are those of its structures, a spacer's those of its
//!   sections, a jumper's those of its end sections or of its structures
//!   (an end, a span it taps, a support) or of the jumper it taps;
//! - a section touching an area has a point inside it, or a span crossing
//!   it — both ends of such a span lie within the area's bounds grown by
//!   the span's length, at most the longest.
//!
//! It is kept up to date by the commands applied in place (the structures
//! changed placed again, the spans of the sections changed or through a
//! structure changed measured); a change of routes, alignments, alignment
//! points or the line's settings, a line made again whole, a unit of work
//! put back or another project line worked on drop it, and it is made
//! again when next needed (one pass over the structures and sections).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use ohl_line::Line;
use ohl_line::index::{LineIndex, Of};

use crate::Kind;

/// The place index's cell, m.
pub const CELL: f64 = 1000.0;

/// A cell of the grid.
type Cell = (i64, i64);

#[cfg(test)]
thread_local! {
    /// Tests: the cell of the indexes made on this thread (a line of a
    /// few hundred metres spread over many cells).
    pub(crate) static TEST_CELL: std::cell::Cell<f64> = const { std::cell::Cell::new(CELL) };
}

/// The cell of an index made now.
fn cell_size() -> f64 {
    #[cfg(test)]
    {
        TEST_CELL.with(std::cell::Cell::get)
    }
    #[cfg(not(test))]
    {
        CELL
    }
}

/// The index by place of a line (see the module's notes).
#[derive(Debug, Clone, Default)]
pub(crate) struct PlaceIndex {
    /// The cell, m.
    cell: f64,
    /// The structures by cell.
    cells: HashMap<Cell, BTreeSet<String>>,
    /// Each structure's cell and routes.
    of: HashMap<String, (Cell, BTreeSet<String>)>,
    /// The structures of each route.
    on: BTreeMap<String, BTreeSet<String>>,
    /// The longest span in plan, m (an upper bound).
    longest: f64,
}

/// The longest span in plan of section `id` (its points' structures'
/// places; points without a structure skipped as the area's test does).
fn spans_of(line: &Line, index: &LineIndex, id: &str) -> f64 {
    let Some(s) = index.section(line, id) else {
        return 0.0;
    };
    let at: Vec<Option<[f64; 2]>> = s
        .points
        .iter()
        .map(|p| {
            index
                .structure(line, &p.structure)
                .map(|t| [t.position[0], t.position[1]])
        })
        .collect();
    at.windows(2)
        .filter_map(|w| match (w[0], w[1]) {
            (Some(a), Some(b)) => Some((a[0] - b[0]).hypot(a[1] - b[1])),
            _ => None,
        })
        .fold(0.0, f64::max)
}

impl PlaceIndex {
    /// The index of `line`, the routes of its structures given (`routes`).
    pub(crate) fn of(
        line: &Line,
        index: &LineIndex,
        routes: HashMap<String, BTreeSet<String>>,
    ) -> Self {
        let mut p = Self {
            cell: cell_size(),
            ..Self::default()
        };
        for s in &line.structures {
            if index.position(Of::Structure, &s.id).is_none() {
                continue;
            }
            let r = routes.get(&s.id).cloned().unwrap_or_default();
            p.put(&s.id, [s.position[0], s.position[1]], r);
        }
        for s in &line.sections {
            p.longest = p.longest.max(spans_of(line, index, &s.id));
        }
        p
    }

    fn cell_of(&self, p: [f64; 2]) -> Cell {
        (
            (p[0] / self.cell).floor() as i64,
            (p[1] / self.cell).floor() as i64,
        )
    }

    fn put(&mut self, id: &str, at: [f64; 2], routes: BTreeSet<String>) {
        self.take(id);
        let c = self.cell_of(at);
        self.cells.entry(c).or_default().insert(id.to_owned());
        for r in &routes {
            self.on.entry(r.clone()).or_default().insert(id.to_owned());
        }
        self.of.insert(id.to_owned(), (c, routes));
    }

    fn take(&mut self, id: &str) {
        if let Some((c, routes)) = self.of.remove(id) {
            if let Some(set) = self.cells.get_mut(&c) {
                set.remove(id);
                if set.is_empty() {
                    self.cells.remove(&c);
                }
            }
            for r in routes {
                if let Some(set) = self.on.get_mut(&r) {
                    set.remove(id);
                    if set.is_empty() {
                        self.on.remove(&r);
                    }
                }
            }
        }
    }

    /// The routes of structure `id`, if the line has it.
    pub(crate) fn routes(&self, id: &str) -> Option<&BTreeSet<String>> {
        self.of.get(id).map(|(_, r)| r)
    }

    /// The structures of route `route`.
    pub(crate) fn on_route(&self, route: &str) -> impl Iterator<Item = &String> {
        self.on.get(route).into_iter().flatten()
    }

    /// The structures in the cells meeting bounds `b` (x0, y0, x1, y1)
    /// grown by `grow` m: candidates, each tested by the caller.
    pub(crate) fn structures_near(&self, b: [f64; 4], grow: f64) -> BTreeSet<&String> {
        let (lo, hi) = (
            self.cell_of([b[0] - grow, b[1] - grow]),
            self.cell_of([b[2] + grow, b[3] + grow]),
        );
        let mut out = BTreeSet::new();
        let cells = (hi.0 - lo.0 + 1).saturating_mul(hi.1 - lo.1 + 1);
        if cells < 0 || cells as usize > self.cells.len() {
            // Fewer cells held than asked: every held cell looked at.
            for (c, ids) in &self.cells {
                if (lo.0..=hi.0).contains(&c.0) && (lo.1..=hi.1).contains(&c.1) {
                    out.extend(ids.iter());
                }
            }
            return out;
        }
        for x in lo.0..=hi.0 {
            for y in lo.1..=hi.1 {
                if let Some(ids) = self.cells.get(&(x, y)) {
                    out.extend(ids.iter());
                }
            }
        }
        out
    }

    /// The longest span in plan, m (an upper bound).
    pub(crate) fn longest(&self) -> f64 {
        self.longest
    }

    /// Kept up to date after a command applied in place changed
    /// `changed` (the line and its `index` as they are now; `routes` the
    /// routes of a structure as it is now). `false`: the index cannot
    /// follow (routes, alignments, their points or the line's settings
    /// changed) — drop it.
    pub(crate) fn changed(
        &mut self,
        line: &Line,
        index: &LineIndex,
        changed: &[(Kind, String)],
        routes: impl Fn(&str) -> BTreeSet<String>,
    ) -> bool {
        if changed.iter().any(|(k, _)| {
            matches!(
                k,
                Kind::Route | Kind::Alignment | Kind::AlignmentPoint | Kind::Meta
            )
        }) {
            return false;
        }
        let mut sections: BTreeSet<&String> = BTreeSet::new();
        for (k, id) in changed {
            match k {
                Kind::Structure => match index.structure(line, id) {
                    Some(s) => {
                        let at = [s.position[0], s.position[1]];
                        self.put(id, at, routes(id));
                        sections.extend(index.referrers_of(Of::Structure, id, Of::Section));
                    }
                    None => self.take(id),
                },
                Kind::Section => {
                    sections.insert(id);
                }
                _ => {}
            }
        }
        for id in sections {
            self.longest = self.longest.max(spans_of(line, index, id));
        }
        true
    }
}

/// The candidates of an extent's picks (see the module's notes): every
/// one is tested by the caller.
pub(crate) struct Candidates<'a> {
    pub(crate) line: &'a Line,
    pub(crate) index: &'a LineIndex,
    pub(crate) places: &'a PlaceIndex,
    /// Every object a candidate (the whole scan).
    pub(crate) scan: bool,
}

impl Candidates<'_> {
    /// The structures that may be on `route`.
    pub(crate) fn structures_on(&self, route: &str) -> Vec<String> {
        if self.scan {
            return self.line.structures.iter().map(|s| s.id.clone()).collect();
        }
        self.places.on_route(route).cloned().collect()
    }

    /// The sections that may be on `route`: those through a structure on
    /// it.
    pub(crate) fn sections_on(&self, route: &str) -> BTreeSet<String> {
        if self.scan {
            return self.line.sections.iter().map(|s| s.id.clone()).collect();
        }
        self.places
            .on_route(route)
            .flat_map(|t| self.index.referrers_of(Of::Structure, t, Of::Section))
            .cloned()
            .collect()
    }

    /// The line objects that may be on `route`: the route, its
    /// alignments and their points, its structures, the sections through
    /// them, their spacers, and the jumpers at those structures and
    /// sections' structures, tapping those sections, or tapping such a
    /// jumper.
    pub(crate) fn on_route(&self, route: &str) -> Vec<(Kind, String)> {
        let l = self.line;
        let mut v = Vec::new();
        if self.scan {
            v.extend(l.routes.iter().map(|x| (Kind::Route, x.id.clone())));
            v.extend(l.alignments.iter().map(|x| (Kind::Alignment, x.id.clone())));
            v.extend(
                l.alignment_points
                    .iter()
                    .map(|x| (Kind::AlignmentPoint, x.id.clone())),
            );
            v.extend(l.structures.iter().map(|x| (Kind::Structure, x.id.clone())));
            v.extend(l.sections.iter().map(|x| (Kind::Section, x.id.clone())));
            v.extend(l.jumpers.iter().map(|x| (Kind::Jumper, x.id.clone())));
            v.extend(l.spacers.iter().map(|x| (Kind::Spacer, x.id.clone())));
            return v;
        }
        let index = self.index;
        v.push((Kind::Route, route.to_owned()));
        let mut points: BTreeSet<&String> = BTreeSet::new();
        for a in l.alignments.iter().filter(|a| a.route == route) {
            v.push((Kind::Alignment, a.id.clone()));
            points.extend(a.points.iter());
        }
        v.extend(
            points
                .into_iter()
                .map(|p| (Kind::AlignmentPoint, p.clone())),
        );
        let structures: BTreeSet<&String> = self.places.on_route(route).collect();
        let sections: BTreeSet<&String> = structures
            .iter()
            .flat_map(|t| index.referrers_of(Of::Structure, t, Of::Section))
            .collect();
        let mut at: BTreeSet<&String> = structures.clone();
        for id in &sections {
            if let Some(s) = index.section(l, id) {
                at.extend(s.points.iter().map(|p| &p.structure));
            }
        }
        let mut jumpers: BTreeSet<&String> = at
            .iter()
            .flat_map(|t| index.referrers_of(Of::Structure, t, Of::Jumper))
            .collect();
        jumpers.extend(
            sections
                .iter()
                .flat_map(|x| index.referrers_of(Of::Section, x, Of::Jumper)),
        );
        let mut todo: Vec<&String> = jumpers.iter().copied().collect();
        while let Some(j) = todo.pop() {
            for t in index.referrers_of(Of::Jumper, j, Of::Jumper) {
                if jumpers.insert(t) {
                    todo.push(t);
                }
            }
        }
        let spacers: BTreeSet<&String> = sections
            .iter()
            .flat_map(|x| index.referrers_of(Of::Section, x, Of::Spacer))
            .collect();
        v.extend(structures.into_iter().map(|x| (Kind::Structure, x.clone())));
        v.extend(sections.into_iter().map(|x| (Kind::Section, x.clone())));
        v.extend(jumpers.into_iter().map(|x| (Kind::Jumper, x.clone())));
        v.extend(spacers.into_iter().map(|x| (Kind::Spacer, x.clone())));
        v
    }

    /// The sections and the structures that may touch area `polygon`:
    /// the structures in its bounds, the sections through a structure in
    /// its bounds grown by the longest span.
    pub(crate) fn in_area(&self, polygon: &[[f64; 2]]) -> (BTreeSet<String>, BTreeSet<String>) {
        if self.scan {
            return (
                self.line.sections.iter().map(|s| s.id.clone()).collect(),
                self.line.structures.iter().map(|s| s.id.clone()).collect(),
            );
        }
        let mut b = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for p in polygon {
            b = [
                b[0].min(p[0]),
                b[1].min(p[1]),
                b[2].max(p[0]),
                b[3].max(p[1]),
            ];
        }
        let sections = self
            .places
            .structures_near(b, self.places.longest() + 1.0)
            .into_iter()
            .flat_map(|t| self.index.referrers_of(Of::Structure, t, Of::Section))
            .cloned()
            .collect();
        let structures = self
            .places
            .structures_near(b, 1.0)
            .into_iter()
            .cloned()
            .collect();
        (sections, structures)
    }
}
