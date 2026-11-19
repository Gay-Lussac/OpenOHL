// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! What a country server keeps beside each write (ADR-0013, phase 5), and
//! what a replica of one of its models needs. Nothing here changes what a
//! project file holds or how the desktop works alone.
//!
//! - **Routes of objects** ([`Session::object_routes`]): the routes a line
//!   object is on, counted as the rules of shared towers count them
//!   ([`crate::shared`]): the server keeps them in its database with every
//!   write (`object.routes`) so that the operation stream, the working
//!   sets and the rights can ask "what of route R".
//! - **References of objects** ([`Session::object_refs`]): what a line
//!   object refers to — the reference index's targets
//!   ([`ohl_line::index`]) — kept as rows (`object_ref`).
//! - **The next log number** ([`Session::set_next_seq`]): a network numbers
//!   the log entries of all its models in one sequence, so a replica of one
//!   model sees gaps; it takes the server's number for the entry an
//!   operation makes, and the same entries then carry the same numbers on
//!   both sides.

use std::collections::BTreeSet;

use ohl_core::grid::Bounds;
use ohl_line::index::Of;
use ohl_line::{JumperEnd, Line, Tap};

use ohl_line::index::Target;

use crate::in_place::{kind_of, of_kind};
use crate::{Kind, Session};

impl Session {
    /// The number the next log entry takes (`None`: the next of the store,
    /// as always). Taken by the next entry only; a replica of a network's
    /// model sets it before it replays an operation that made entries on
    /// the server, and clears it after.
    pub fn set_next_seq(&self, seq: Option<i64>) {
        *self.next_seq.lock().expect("not poisoned") = seq;
    }

    /// The routes of line objects of the line worked on, one set per
    /// object asked: a route itself; an alignment its route; an alignment
    /// point its alignments'; a structure the routes it gives itself, else
    /// those of the alignment points tied to it and of the alignments
    /// passing near it; a section those all its structures share (else
    /// those of any); a jumper those of the sections at its ends; a spacer
    /// its sections'. Objects that are not line objects (meta, libraries,
    /// reports) and objects not in the line: none.
    ///
    /// Expected cost: a lookup per object in the reference index; the
    /// first structure without routes of its own nor tied alignment points
    /// makes the search grid of the alignments once.
    pub fn object_routes(&mut self, objects: &[(Kind, String)]) -> Vec<BTreeSet<String>> {
        self.routes_of_objects(objects)
    }

    /// What line object `kind`/`id` of the line worked on refers to: other
    /// objects (kind, id, `None`), and wire points of structures (the
    /// structure, `Some(set)`), each once. Not a line object, or not in the
    /// line: nothing.
    pub fn object_refs(&mut self, kind: Kind, id: &str) -> Vec<(Kind, String, Option<u32>)> {
        let Some(of) = of_kind(kind) else {
            return Vec::new();
        };
        self.line_index();
        let index = self.index.as_ref().expect("made above");
        let Some(item) = index.get(&self.line, of, id) else {
            return Vec::new();
        };
        item.targets()
            .into_iter()
            .map(|t| match t {
                Target::Object(of, id) => (kind_of(of), id, None),
                Target::Point(structure, set) => (Kind::Structure, structure, Some(set)),
            })
            .collect()
    }

    /// The line objects of the line worked on referring to line object
    /// `kind`/`id` (the alignments through an alignment point, the sections
    /// on a structure, …), each once.
    pub fn object_referrers(&mut self, kind: Kind, id: &str) -> Vec<(Kind, String)> {
        let Some(of) = of_kind(kind) else {
            return Vec::new();
        };
        self.line_index();
        let index = self.index.as_ref().expect("made above");
        let mut out: Vec<(Kind, String)> = index
            .referrers(&Target::Object(of, id.to_owned()))
            .map(|(of, id)| (kind_of(*of), id.clone()))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// The part of the line worked on that drawing a tile reads (country
    /// server, phase 6: the tile packager's input instead of a copy of the
    /// whole line): the line's settings and, in the line's order,
    ///
    /// - `structures` and `alignments` (with their points and routes);
    /// - for each of `sections`, what solving it alone reads (the
    ///   packager's sub-line): its structures; every section and jumper at
    ///   them (a jumper tapped on a span of a section there too, with what
    ///   its taps lead to); its spacers and the sections they join; the
    ///   structures of every section kept; the alignments whose points'
    ///   bounds come within `near` of those structures, with their points
    ///   and routes; the structures' own routes.
    ///
    /// Taken from the session's line by its reference index: it costs what
    /// the tile holds, not the size of the line. A section's sub-line made
    /// from it is the one made from the whole line (tested in the
    /// packager, bit for bit).
    pub fn drawing_part(
        &mut self,
        structures: &[String],
        sections: &[String],
        alignments: &[String],
        near: f64,
    ) -> Line {
        self.line_index();
        let index = self.index.as_ref().expect("made above");
        let line = &self.line;
        let mut keep: BTreeSet<(Of, String)> = BTreeSet::new();
        for id in structures {
            keep.insert((Of::Structure, id.clone()));
        }
        let mut aligned: BTreeSet<String> = alignments.iter().cloned().collect();
        // The alignments' bounds, by place (their points found in the line).
        let bounds: Vec<Option<Bounds>> = line
            .alignments
            .iter()
            .map(|a| {
                Bounds::of_points(
                    a.points
                        .iter()
                        .filter_map(|p| index.alignment_point(line, p).map(|p| &p.position)),
                )
            })
            .collect();
        for id in sections {
            let Some(s) = index.section(line, id) else {
                continue;
            };
            let mut secs: BTreeSet<String> = [id.clone()].into();
            let mut at: BTreeSet<String> = BTreeSet::new();
            for p in &s.points {
                at.insert(p.structure.clone());
            }
            let mut jumpers: BTreeSet<String> = BTreeSet::new();
            for t in &at {
                secs.extend(index.referrers_of(Of::Structure, t, Of::Section).cloned());
                jumpers.extend(index.referrers_of(Of::Structure, t, Of::Jumper).cloned());
            }
            for x in &secs {
                jumpers.extend(index.referrers_of(Of::Section, x, Of::Jumper).cloned());
            }
            for x in index.referrers_of(Of::Section, id, Of::Spacer) {
                keep.insert((Of::Spacer, x.clone()));
                if let Some(k) = index.position(Of::Spacer, x) {
                    let sp = &line.spacers[k];
                    secs.insert(sp.a.section.clone());
                    secs.insert(sp.b.section.clone());
                }
            }
            // Jumpers with what their taps lead to (sections, jumpers).
            let mut todo: Vec<String> = jumpers.iter().cloned().collect();
            while let Some(j) = todo.pop() {
                let Some(jp) = index.jumper(line, &j) else {
                    continue;
                };
                keep.insert((Of::Jumper, j.clone()));
                for end in [&jp.from, &jp.to] {
                    match end {
                        JumperEnd::Tap { tap: Tap::Span(r) } => {
                            keep.insert((Of::Section, r.section.clone()));
                        }
                        JumperEnd::Tap {
                            tap: Tap::Jumper { jumper, .. },
                        } => {
                            if !keep.contains(&(Of::Jumper, jumper.clone())) {
                                todo.push(jumper.clone());
                            }
                        }
                        JumperEnd::Wire(_) => {}
                    }
                }
            }
            for x in &secs {
                keep.insert((Of::Section, x.clone()));
                if let Some(sx) = index.section(line, x) {
                    for p in &sx.points {
                        at.insert(p.structure.clone());
                    }
                }
            }
            let near_box = Bounds::of_points(
                at.iter()
                    .filter_map(|t| index.structure(line, t).map(|s| &s.position)),
            )
            .map(|b| b.grown(near));
            for t in at {
                if let Some(st) = index.structure(line, &t) {
                    for r in &st.routes {
                        keep.insert((Of::Route, r.clone()));
                    }
                }
                keep.insert((Of::Structure, t));
            }
            if let Some(near_box) = near_box {
                for (k, b) in bounds.iter().enumerate() {
                    if b.is_some_and(|b| b.intersects(&near_box)) {
                        aligned.insert(line.alignments[k].id.clone());
                    }
                }
            }
        }
        for a in &aligned {
            let Some(k) = index.position(Of::Alignment, a) else {
                continue;
            };
            let al = &line.alignments[k];
            keep.insert((Of::Alignment, a.clone()));
            keep.insert((Of::Route, al.route.clone()));
            for p in &al.points {
                keep.insert((Of::AlignmentPoint, p.clone()));
            }
        }
        let mut out = line.settings_only();
        out.alignment = line.alignment.clone();
        let places = |of: Of| -> Vec<usize> {
            let mut v: Vec<usize> = keep
                .iter()
                .filter(|(k, _)| *k == of)
                .filter_map(|(_, id)| index.position(of, id))
                .collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        out.routes = places(Of::Route)
            .into_iter()
            .map(|k| line.routes[k].clone())
            .collect();
        out.alignments = places(Of::Alignment)
            .into_iter()
            .map(|k| line.alignments[k].clone())
            .collect();
        out.alignment_points = places(Of::AlignmentPoint)
            .into_iter()
            .map(|k| line.alignment_points[k].clone())
            .collect();
        out.structures = places(Of::Structure)
            .into_iter()
            .map(|k| line.structures[k].clone())
            .collect();
        out.sections = places(Of::Section)
            .into_iter()
            .map(|k| line.sections[k].clone())
            .collect();
        out.jumpers = places(Of::Jumper)
            .into_iter()
            .map(|k| line.jumpers[k].clone())
            .collect();
        out.spacers = places(Of::Spacer)
            .into_iter()
            .map(|k| line.spacers[k].clone())
            .collect();
        out
    }

    /// Whether a structure of the line worked on has a type (its drawing
    /// then needs the structure libraries).
    pub fn any_typed(&self) -> bool {
        self.line
            .structures
            .iter()
            .any(|s| s.structure_type.is_some())
    }
}
