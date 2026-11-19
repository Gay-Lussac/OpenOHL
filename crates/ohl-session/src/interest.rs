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
}
