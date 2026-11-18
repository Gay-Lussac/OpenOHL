// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The reference index of a line (country server, phase 3): where each
//! object is in the line, and **who refers to whom** — the sections
//! through a structure or at one of its wire points, the jumpers at a
//! structure or tapping a section or another jumper, the spacers on a
//! section, the alignment points tied to a structure, the alignments
//! through a point or of a route, the structures giving a route.
//!
//! Built once ([`LineIndex::of`], one pass over the line) and kept up to
//! date by changing the line through it ([`LineIndex::insert`],
//! [`LineIndex::replace`], [`LineIndex::remove`]): an edit then costs what
//! it touches, not the size of the line. What it answers is used by the
//! local validation ([`crate::Line::validate_local`]), the whole one
//! (lookups instead of searches) and the session's in-place edits.
//!
//! Ids: an object is found by its id at the **first** place of that id in
//! its list, as a search through the list finds it. The index is kept up
//! to date in place only for lines whose ids are unique within each list
//! (the session's: its objects are keyed by kind and id); a line with an
//! id given twice is refused by the validation anyway.

use std::collections::{BTreeSet, HashMap};

use crate::{
    Alignment, AlignmentPoint, Jumper, JumperEnd, Line, Route, Section, Spacer, Structure, Tap,
};

/// The kinds of objects of a line, in the order the line lists (and
/// writes) them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Of {
    /// [`Line::routes`].
    Route,
    /// [`Line::alignments`].
    Alignment,
    /// [`Line::alignment_points`].
    AlignmentPoint,
    /// [`Line::structures`].
    Structure,
    /// [`Line::sections`].
    Section,
    /// [`Line::jumpers`].
    Jumper,
    /// [`Line::spacers`].
    Spacer,
}

impl Of {
    /// Every kind, in the line's order.
    pub const ALL: [Of; 7] = [
        Of::Route,
        Of::Alignment,
        Of::AlignmentPoint,
        Of::Structure,
        Of::Section,
        Of::Jumper,
        Of::Spacer,
    ];

    fn slot(self) -> usize {
        self as usize
    }

    /// The line's field holding them (`structures`, …).
    pub fn field(self) -> &'static str {
        match self {
            Of::Route => "routes",
            Of::Alignment => "alignments",
            Of::AlignmentPoint => "alignment_points",
            Of::Structure => "structures",
            Of::Section => "sections",
            Of::Jumper => "jumpers",
            Of::Spacer => "spacers",
        }
    }
}

/// What an object refers to.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Target {
    /// Another object, by kind and id.
    Object(Of, String),
    /// A structure's wire point (structure id, set): a section's point or a
    /// jumper's end.
    Point(String, u32),
}

/// One object of a line, owned.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// A route.
    Route(Route),
    /// An alignment.
    Alignment(Alignment),
    /// An alignment point.
    AlignmentPoint(AlignmentPoint),
    /// A structure (boxed: much larger than the others).
    Structure(Box<Structure>),
    /// A section.
    Section(Box<Section>),
    /// A jumper.
    Jumper(Box<Jumper>),
    /// A spacer.
    Spacer(Spacer),
}

impl Item {
    /// Its kind.
    pub fn of(&self) -> Of {
        match self {
            Self::Route(_) => Of::Route,
            Self::Alignment(_) => Of::Alignment,
            Self::AlignmentPoint(_) => Of::AlignmentPoint,
            Self::Structure(_) => Of::Structure,
            Self::Section(_) => Of::Section,
            Self::Jumper(_) => Of::Jumper,
            Self::Spacer(_) => Of::Spacer,
        }
    }

    /// Its id.
    pub fn id(&self) -> &str {
        match self {
            Self::Route(x) => &x.id,
            Self::Alignment(x) => &x.id,
            Self::AlignmentPoint(x) => &x.id,
            Self::Structure(x) => &x.id,
            Self::Section(x) => &x.id,
            Self::Jumper(x) => &x.id,
            Self::Spacer(x) => &x.id,
        }
    }

    /// What it refers to (each target once).
    pub fn targets(&self) -> Vec<Target> {
        let mut out = Vec::new();
        let obj = |of: Of, id: &str| Target::Object(of, id.to_owned());
        match self {
            Self::Route(_) => {}
            Self::Alignment(a) => {
                out.push(obj(Of::Route, &a.route));
                out.extend(a.points.iter().map(|p| obj(Of::AlignmentPoint, p)));
            }
            Self::AlignmentPoint(p) => {
                if let Some(s) = &p.structure {
                    out.push(obj(Of::Structure, s));
                }
            }
            Self::Structure(s) => out.extend(s.routes.iter().map(|r| obj(Of::Route, r))),
            Self::Section(s) => {
                for p in &s.points {
                    out.push(obj(Of::Structure, &p.structure));
                    out.push(Target::Point(p.structure.clone(), p.set));
                }
            }
            Self::Jumper(j) => {
                for end in [&j.from, &j.to] {
                    match end {
                        JumperEnd::Wire(p) => {
                            out.push(obj(Of::Structure, &p.structure));
                            out.push(Target::Point(p.structure.clone(), p.set));
                        }
                        JumperEnd::Tap { tap } => match tap {
                            Tap::Span(r) => out.push(obj(Of::Section, &r.section)),
                            Tap::Jumper { jumper, .. } => out.push(obj(Of::Jumper, jumper)),
                        },
                    }
                }
                for c in j.supports() {
                    out.push(obj(Of::Structure, c.structure_id(j)));
                }
            }
            Self::Spacer(s) => {
                out.push(obj(Of::Section, &s.a.section));
                out.push(obj(Of::Section, &s.b.section));
            }
        }
        out.sort();
        out.dedup();
        out
    }

    /// Whether a section with a span point placed by station (its place
    /// depends on the routes: [`Line::span_stations`]).
    fn by_station(&self) -> bool {
        matches!(self, Self::Section(s) if s.span_points.iter().any(|p| p.at.station().is_some()))
    }
}

/// The object of kind `of` at `pos` of `line`, borrowed.
fn object_at(line: &Line, of: Of, pos: usize) -> Option<Item> {
    Some(match of {
        Of::Route => Item::Route(line.routes.get(pos)?.clone()),
        Of::Alignment => Item::Alignment(line.alignments.get(pos)?.clone()),
        Of::AlignmentPoint => Item::AlignmentPoint(line.alignment_points.get(pos)?.clone()),
        Of::Structure => Item::Structure(Box::new(line.structures.get(pos)?.clone())),
        Of::Section => Item::Section(Box::new(line.sections.get(pos)?.clone())),
        Of::Jumper => Item::Jumper(Box::new(line.jumpers.get(pos)?.clone())),
        Of::Spacer => Item::Spacer(line.spacers.get(pos)?.clone()),
    })
}

/// The id of the object of kind `of` at `pos`.
fn id_at(line: &Line, of: Of, pos: usize) -> &str {
    match of {
        Of::Route => &line.routes[pos].id,
        Of::Alignment => &line.alignments[pos].id,
        Of::AlignmentPoint => &line.alignment_points[pos].id,
        Of::Structure => &line.structures[pos].id,
        Of::Section => &line.sections[pos].id,
        Of::Jumper => &line.jumpers[pos].id,
        Of::Spacer => &line.spacers[pos].id,
    }
}

/// The number of objects of kind `of`.
pub fn count(line: &Line, of: Of) -> usize {
    match of {
        Of::Route => line.routes.len(),
        Of::Alignment => line.alignments.len(),
        Of::AlignmentPoint => line.alignment_points.len(),
        Of::Structure => line.structures.len(),
        Of::Section => line.sections.len(),
        Of::Jumper => line.jumpers.len(),
        Of::Spacer => line.spacers.len(),
    }
}

/// The reference index of a line (see the module's notes).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LineIndex {
    /// Per kind: id → its first place in the line's list.
    pos: [HashMap<String, usize>; 7],
    /// Target → the objects referring to it.
    referrers: HashMap<Target, BTreeSet<(Of, String)>>,
    /// The sections with a span point placed by station.
    stations: BTreeSet<String>,
}

impl LineIndex {
    /// The index of `line` (one pass over it).
    pub fn of(line: &Line) -> Self {
        let mut index = Self::default();
        for of in Of::ALL {
            let n = count(line, of);
            index.pos[of.slot()].reserve(n);
            for k in 0..n {
                index.pos[of.slot()]
                    .entry(id_at(line, of, k).to_owned())
                    .or_insert(k);
            }
        }
        for of in Of::ALL {
            for k in 0..count(line, of) {
                index.add_refs_at(line, of, k);
            }
        }
        index
    }

    /// Where object `of`/`id` is in its list (the first of that id).
    pub fn position(&self, of: Of, id: &str) -> Option<usize> {
        self.pos[of.slot()].get(id).copied()
    }

    /// The objects referring to `target`, by kind and id.
    pub fn referrers<'a>(
        &'a self,
        target: &Target,
    ) -> impl Iterator<Item = &'a (Of, String)> + use<'a> {
        self.referrers.get(target).into_iter().flatten()
    }

    /// The objects of kind `kind` referring to object `of`/`id`.
    pub fn referrers_of<'a>(
        &'a self,
        of: Of,
        id: &str,
        kind: Of,
    ) -> impl Iterator<Item = &'a String> + use<'a> {
        self.referrers(&Target::Object(of, id.to_owned()))
            .filter(move |(k, _)| *k == kind)
            .map(|(_, id)| id)
    }

    /// The objects of kind `kind` at wire point `structure`/`set` (sections
    /// through it, jumpers ending there).
    pub fn at_point<'a>(
        &'a self,
        structure: &str,
        set: u32,
        kind: Of,
    ) -> impl Iterator<Item = &'a String> + use<'a> {
        self.referrers(&Target::Point(structure.to_owned(), set))
            .filter(move |(k, _)| *k == kind)
            .map(|(_, id)| id)
    }

    /// The sections with a span point placed by station.
    pub fn station_sections(&self) -> &BTreeSet<String> {
        &self.stations
    }

    /// The structure `id` of `line` (the first of the id).
    pub fn structure<'l>(&self, line: &'l Line, id: &str) -> Option<&'l Structure> {
        line.structures.get(self.position(Of::Structure, id)?)
    }

    /// The section `id` of `line` (the first of the id).
    pub fn section<'l>(&self, line: &'l Line, id: &str) -> Option<&'l Section> {
        line.sections.get(self.position(Of::Section, id)?)
    }

    /// The jumper `id` of `line` (the first of the id).
    pub fn jumper<'l>(&self, line: &'l Line, id: &str) -> Option<&'l Jumper> {
        line.jumpers.get(self.position(Of::Jumper, id)?)
    }

    /// The alignment point `id` of `line` (the first of the id).
    pub fn alignment_point<'l>(&self, line: &'l Line, id: &str) -> Option<&'l AlignmentPoint> {
        line.alignment_points
            .get(self.position(Of::AlignmentPoint, id)?)
    }

    /// The first section of `line` (in its order) through wire point
    /// `structure`/`set`.
    pub fn first_section_at<'l>(
        &self,
        line: &'l Line,
        structure: &str,
        set: u32,
    ) -> Option<&'l Section> {
        self.at_point(structure, set, Of::Section)
            .filter_map(|id| self.position(Of::Section, id))
            .min()
            .map(|k| &line.sections[k])
    }

    /// [`Line::wire_point`] by the index.
    pub fn wire_point<'l>(
        &self,
        line: &'l Line,
        p: &crate::SectionPoint,
    ) -> Option<&'l crate::WirePoint> {
        self.structure(line, &p.structure)?.wire_at_set(p.set)
    }

    /// [`Line::tap_section`] by the index (the same result).
    pub fn tap_section<'l>(&self, line: &'l Line, tap: &Tap, depth: usize) -> Option<&'l Section> {
        match tap {
            Tap::Span(r) => self.section(line, &r.section),
            Tap::Jumper { jumper, .. } if depth < 16 => {
                let host = self.jumper(line, jumper)?;
                self.end_section(line, &host.from, depth + 1)
                    .or_else(|| self.end_section(line, &host.to, depth + 1))
            }
            Tap::Jumper { .. } => None,
        }
    }

    /// The section at a jumper's end by the index (as `Line::end_section`).
    pub fn end_section<'l>(
        &self,
        line: &'l Line,
        end: &JumperEnd,
        depth: usize,
    ) -> Option<&'l Section> {
        match end {
            JumperEnd::Wire(p) => self.first_section_at(line, &p.structure, p.set),
            JumperEnd::Tap { tap } => self.tap_section(line, tap, depth),
        }
    }

    /// [`Line::jumper_section`] by the index.
    pub fn jumper_section<'l>(&self, line: &'l Line, jumper: &Jumper) -> Option<&'l Section> {
        self.end_section(line, &jumper.from, 0)
            .or_else(|| self.end_section(line, &jumper.to, 0))
    }

    fn add_refs_at(&mut self, line: &Line, of: Of, pos: usize) {
        let Some(o) = object_at(line, of, pos) else {
            return;
        };
        self.add_refs(&o);
    }

    fn add_refs(&mut self, o: &Item) {
        let me = (o.of(), o.id().to_owned());
        for t in o.targets() {
            self.referrers.entry(t).or_default().insert(me.clone());
        }
        if o.by_station() {
            self.stations.insert(o.id().to_owned());
        }
    }

    fn remove_refs(&mut self, o: &Item) {
        let me = (o.of(), o.id().to_owned());
        for t in o.targets() {
            if let Some(set) = self.referrers.get_mut(&t) {
                set.remove(&me);
                if set.is_empty() {
                    self.referrers.remove(&t);
                }
            }
        }
        if o.by_station() {
            self.stations.remove(o.id());
        }
    }

    /// Renumbers the places of kind `of` from `from` on (after an insert or
    /// a removal there).
    fn renumber(&mut self, line: &Line, of: Of, from: usize) {
        for k in from..count(line, of) {
            let id = id_at(line, of, k);
            if let Some(p) = self.pos[of.slot()].get_mut(id) {
                *p = k;
            }
        }
    }

    /// Removes object `of`/`id` from `line`; it is given back (none when
    /// there is none).
    pub fn remove(&mut self, line: &mut Line, of: Of, id: &str) -> Option<Item> {
        let k = self.pos[of.slot()].remove(id)?;
        let o = match of {
            Of::Route => Item::Route(line.routes.remove(k)),
            Of::Alignment => Item::Alignment(line.alignments.remove(k)),
            Of::AlignmentPoint => Item::AlignmentPoint(line.alignment_points.remove(k)),
            Of::Structure => Item::Structure(Box::new(line.structures.remove(k))),
            Of::Section => Item::Section(Box::new(line.sections.remove(k))),
            Of::Jumper => Item::Jumper(Box::new(line.jumpers.remove(k))),
            Of::Spacer => Item::Spacer(line.spacers.remove(k)),
        };
        self.remove_refs(&o);
        self.renumber(line, of, k);
        Some(o)
    }

    /// Inserts `o` into `line` at place `at` of its list (its id not there
    /// yet).
    pub fn insert(&mut self, line: &mut Line, o: Item, at: usize) {
        let of = o.of();
        debug_assert!(
            self.position(of, o.id()).is_none(),
            "{} given twice",
            o.id()
        );
        self.add_refs(&o);
        let id = o.id().to_owned();
        match o {
            Item::Route(x) => line.routes.insert(at, x),
            Item::Alignment(x) => line.alignments.insert(at, x),
            Item::AlignmentPoint(x) => line.alignment_points.insert(at, x),
            Item::Structure(x) => line.structures.insert(at, *x),
            Item::Section(x) => line.sections.insert(at, *x),
            Item::Jumper(x) => line.jumpers.insert(at, *x),
            Item::Spacer(x) => line.spacers.insert(at, x),
        }
        self.pos[of.slot()].insert(id, at);
        self.renumber(line, of, at + 1);
    }

    /// Replaces the object of `o`'s kind and id in `line` (in its place);
    /// the one replaced is given back (none when there was none: nothing
    /// changed).
    pub fn replace(&mut self, line: &mut Line, o: Item) -> Option<Item> {
        let of = o.of();
        let k = self.position(of, o.id())?;
        let old = object_at(line, of, k)?;
        self.remove_refs(&old);
        self.add_refs(&o);
        match o {
            Item::Route(x) => line.routes[k] = x,
            Item::Alignment(x) => line.alignments[k] = x,
            Item::AlignmentPoint(x) => line.alignment_points[k] = x,
            Item::Structure(x) => line.structures[k] = *x,
            Item::Section(x) => line.sections[k] = *x,
            Item::Jumper(x) => line.jumpers[k] = *x,
            Item::Spacer(x) => line.spacers[k] = x,
        }
        Some(old)
    }

    /// The object `of`/`id` of `line`, as an owned value.
    pub fn get(&self, line: &Line, of: Of, id: &str) -> Option<Item> {
        object_at(line, of, self.position(of, id)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kept_in_place_equals_built_again() {
        let mut line = crate::Line::from_json(include_str!(
            "../../../examples/synthetic-line/synthetic-line.ohl.json"
        ))
        .expect("example line");
        let mut index = LineIndex::of(&line);
        assert_eq!(index, LineIndex::of(&line));
        // Take each object out and put it back elsewhere and in its place.
        for of in Of::ALL {
            let n = count(&line, of);
            for k in 0..n {
                let id = id_at(&line, of, k).to_owned();
                let o = index.remove(&mut line, of, &id).expect("there");
                assert_eq!(index, LineIndex::of(&line), "{of:?} {id} removed");
                index.insert(&mut line, o.clone(), 0);
                assert_eq!(index, LineIndex::of(&line), "{of:?} {id} first");
                let o = index.remove(&mut line, of, &id).expect("there");
                index.insert(&mut line, o, k);
                assert_eq!(index, LineIndex::of(&line), "{of:?} {id} back");
                let again = index.get(&line, of, &id).expect("there");
                index.replace(&mut line, again);
                assert_eq!(index, LineIndex::of(&line), "{of:?} {id} replaced");
            }
        }
    }
}
