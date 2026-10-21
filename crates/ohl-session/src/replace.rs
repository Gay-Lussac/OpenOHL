// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Replace structure ([`Command::ReplaceStructure`], structure building
//! decision 8): a placed structure is replaced by another — a structure
//! type of the library or one given point by point — at the same place,
//! orientation and height adjustment, keeping its id (sections, jumpers,
//! routes and alignment points tied to it stay with it).
//!
//! The wires follow by their sets: every old set something ends at
//! (sections, jumper ends, circuit starts) goes to the new set the user
//! gives ([`SetMove`]), else to the new set of the same number when the
//! new structure has it, else nowhere — then the sections ending there go,
//! with the jumpers at them (or hanging on taps on them) and the spacers
//! on them, after a confirmation listing everything
//! ([`SessionError::NeedsConfirmation`]). The old structure's own points,
//! strings and wire points (overrides of its type) are listed and dropped,
//! as are the jumper support clamps on wire points the new structure does
//! not have. One log entry: undo brings everything back.
//!
//! The session does not read the structure library: for a typed new
//! structure the caller gives its wire points as placed ([`NewWire`]).

use std::collections::{BTreeMap, BTreeSet};

use ohl_line::{SectionPoint, Structure};
use serde::{Deserialize, Serialize};

use crate::{Command, Impact, Kind, Session, SessionError, put, refused};

/// A wire point of the new structure (as placed from the library).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewWire {
    /// Its label.
    pub label: String,
    /// Its set (none: strings only meet there).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set: Option<u32>,
}

/// Where what ends at an old set goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetMove {
    /// The old structure's set.
    pub from: u32,
    /// The new structure's set; none: what ends there is removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<u32>,
}

/// What becomes of an old attachment point
/// ([`Command::ReplaceStructure`]'s `points`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointMove {
    /// The old point's label.
    pub label: String,
    /// The new structure's joint it is linked to (the new structure
    /// carries the point, linked so); none: removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub joint: Option<String>,
    /// Its offset from that joint, structure axes, m.
    #[serde(default, skip_serializing_if = "crate::is_zero_point")]
    pub offset: ohl_line::Point,
}

impl Session {
    /// Structure `id`'s own attachment point `label`, if it has one;
    /// refused when there is no such structure or it has no type (its
    /// points are their own joints).
    pub(crate) fn own_point(
        &self,
        id: &str,
        label: &str,
    ) -> Result<Option<ohl_line::StructurePoint>, SessionError> {
        let s = self
            .line
            .structure(id)
            .ok_or_else(|| refused(format!("no structure {id}")))?;
        if s.structure_type.is_none() {
            return Err(refused(format!(
                "{id} has no type: its attachment points are their own joints"
            )));
        }
        Ok(s.points.iter().find(|p| p.label == label).cloned())
    }

    /// The put of typed structure `id` with `points` as its own (each
    /// replacing its own point of the label).
    pub(crate) fn with_points(
        &self,
        id: &str,
        points: Vec<ohl_line::StructurePoint>,
    ) -> Result<Command, SessionError> {
        let mut s = self
            .line
            .structure(id)
            .ok_or_else(|| refused(format!("no structure {id}")))?
            .clone();
        if s.structure_type.is_none() {
            return Err(refused(format!(
                "{id} has no type: its attachment points are their own joints"
            )));
        }
        for p in points {
            if p.label.is_empty() || s.wires.iter().any(|w| w.label == p.label) {
                return Err(refused(format!(
                    "{id}: attachment point \"{}\": empty, or the label of a wire point",
                    p.label
                )));
            }
            match s.points.iter_mut().find(|q| q.label == p.label) {
                Some(q) => *q = p,
                None => s.points.push(p),
            }
        }
        Ok(put(Kind::Structure, id, &s))
    }
}

/// What replacing a structure does (nothing is changed).
#[derive(Debug, Clone, PartialEq)]
pub struct Replacement {
    /// The new structure as stored: id, position, orientation, height
    /// adjustment, description, routes and fields of the old one.
    pub structure: Structure,
    /// Every old set something ends at, and the new set it goes to (none:
    /// removed), in set order.
    pub moves: Vec<(u32, Option<u32>)>,
    /// What is removed or dropped (sections, jumpers, spacers, the old
    /// structure's overrides, jumper supports).
    pub impact: Impact,
    /// The jumper support clamps dropped: (jumper, clamp label).
    pub clamps: Vec<(String, String)>,
}

impl Replacement {
    /// The sets rewired to another set or the same one.
    pub fn rewired(&self) -> usize {
        self.moves.iter().filter(|(_, to)| to.is_some()).count()
    }
}

/// The type of `s` in words.
fn type_name(s: &Structure) -> String {
    match (&s.structure_type, &s.extension) {
        (Some(t), Some(e)) => format!("{t} ({e})"),
        (Some(t), None) => t.clone(),
        (None, _) => "no type".into(),
    }
}

impl Session {
    /// What replacing structure `id` by `structure` does
    /// ([`Command::ReplaceStructure`]); refused when a set given is not
    /// on the new structure, or two old sets go to one new set.
    pub fn replacement(
        &self,
        id: &str,
        structure: &Structure,
        wires: &[NewWire],
        sets: &[SetMove],
        points: &[PointMove],
    ) -> Result<Replacement, SessionError> {
        let old = self
            .line
            .structure(id)
            .ok_or_else(|| refused(format!("no structure {id}")))?;
        let mut new = structure.clone();
        new.id = old.id.clone();
        new.position = old.position;
        new.orientation_deg = old.orientation_deg;
        new.height_adjust = old.height_adjust;
        new.description = old.description.clone();
        new.routes = old.routes.clone();
        new.fields = old.fields.clone();
        // The new structure's wire points: the type's as placed (less
        // those left out), with its own over them.
        let mut placed: BTreeMap<String, Option<u32>> = BTreeMap::new();
        if new.structure_type.is_some() {
            for w in wires {
                let left_out = new.omit.contains(&w.label)
                    || w.set.is_some_and(|s| new.omit_sets.contains(&s));
                if !left_out {
                    placed.insert(w.label.clone(), w.set);
                }
            }
        }
        for w in &new.wires {
            if let Some(set) = w.set {
                placed.retain(|_, s| *s != Some(set));
            }
            placed.insert(w.label.clone(), w.set);
        }
        let new_sets: BTreeSet<u32> = placed.values().filter_map(|s| *s).collect();

        // The old sets something ends at.
        let mut used: BTreeSet<u32> = BTreeSet::new();
        for sec in &self.line.sections {
            used.extend(
                sec.points
                    .iter()
                    .filter(|p| p.structure == id)
                    .map(|p| p.set),
            );
        }
        for j in &self.line.jumpers {
            used.extend(j.points().filter(|p| p.structure == id).map(|p| p.set));
        }
        used.extend(
            self.line
                .circuit_starts
                .iter()
                .filter(|p| p.structure == id)
                .map(|p| p.set),
        );
        let given: BTreeMap<u32, Option<u32>> = sets.iter().map(|m| (m.from, m.to)).collect();
        let mut moves = Vec::new();
        let mut taken: BTreeMap<u32, u32> = BTreeMap::new();
        for &set in &used {
            let to = match given.get(&set) {
                Some(Some(to)) => {
                    if !new_sets.contains(to) {
                        return Err(refused(format!(
                            "{id}: the new structure ({}) has no wire point with set {to} \
                             (for old set {set})",
                            type_name(&new)
                        )));
                    }
                    Some(*to)
                }
                Some(None) => None,
                None => new_sets.contains(&set).then_some(set),
            };
            if let Some(to) = to
                && let Some(other) = taken.insert(to, set)
            {
                return Err(refused(format!(
                    "{id}: old sets {other} and {set} both to new set {to} — give each \
                     its own"
                )));
            }
            moves.push((set, to));
        }

        let mut impact = Impact::new(Kind::Structure, id);
        impact.part = Some(format!("replaced by {}", type_name(&new)));
        let removed: Vec<SectionPoint> = moves
            .iter()
            .filter(|(_, to)| to.is_none())
            .map(|(set, _)| SectionPoint::new(id, *set))
            .collect();
        let sections: Vec<&ohl_line::Section> = self
            .line
            .sections
            .iter()
            .filter(|sec| sec.points.iter().any(|p| removed.contains(p)))
            .collect();
        impact.sections = sections.iter().map(|sec| sec.id.clone()).collect();
        let at_ends: Vec<String> = self
            .line
            .jumpers
            .iter()
            .filter(|j| {
                j.points().any(|end| {
                    removed.contains(end) || sections.iter().any(|sec| sec.points.contains(end))
                })
            })
            .map(|j| j.id.clone())
            .collect();
        impact.jumpers = self.jumpers_hanging(&impact.sections, &at_ends);
        impact.spacers = self.spacers_on(&impact.sections);
        // Support clamps of the jumpers kept on wire points the new
        // structure does not have.
        let mut clamps = Vec::new();
        for j in &self.line.jumpers {
            if impact.jumpers.contains(&j.id) {
                continue;
            }
            for c in j.supports() {
                if c.structure_id(j) == id && !placed.contains_key(&c.wire) {
                    impact.supports.push(format!(
                        "{} clamp {} (wire point {})",
                        j.id, c.label, c.wire
                    ));
                    clamps.push((j.id.clone(), c.label.clone()));
                }
            }
        }
        // The old attachment points matched to the new structure: linked
        // (it carries them so), or removed.
        for m in points {
            match &m.joint {
                Some(j) => {
                    if !new
                        .points
                        .iter()
                        .any(|p| p.label == m.label && p.joint.as_deref() == Some(j.as_str()))
                    {
                        return Err(refused(format!(
                            "{id}: attachment point {} to joint {j}: the new structure does not \
                             carry it so",
                            m.label
                        )));
                    }
                }
                None => impact
                    .overrides
                    .push(format!("attachment point {} (removed)", m.label)),
            }
        }
        // The old structure's own items over its type, not on the new one
        // (a point carried over by its label is not dropped).
        if old.structure_type.is_some() {
            for p in &old.points {
                let moved = points.iter().any(|m| m.label == p.label);
                if !moved && !new.points.iter().any(|q| q.label == p.label) {
                    impact.overrides.push(format!("point {}", p.label));
                }
            }
            for x in &old.strings {
                if !new.strings.contains(x) {
                    impact.overrides.push(format!("string {}", x.label));
                }
            }
            for w in &old.wires {
                if !new.wires.contains(w) {
                    impact.overrides.push(match w.set {
                        Some(set) => format!("wire point {} (set {set})", w.label),
                        None => format!("wire point {}", w.label),
                    });
                }
            }
            for o in &old.omit {
                if !new.omit.contains(o) {
                    impact.overrides.push(format!("left out: {o}"));
                }
            }
            for s in &old.omit_sets {
                if !new.omit_sets.contains(s) {
                    impact.overrides.push(format!("left out: set {s}"));
                }
            }
        }
        Ok(Replacement {
            structure: new,
            moves,
            impact,
            clamps,
        })
    }

    /// The puts and deletes of [`Command::ReplaceStructure`].
    pub(crate) fn replace_primitives(
        &self,
        plan: Replacement,
        out: &mut Vec<Command>,
        notes: &mut Vec<String>,
    ) {
        let Replacement {
            structure,
            moves,
            impact,
            clamps: dropped,
        } = plan;
        let id = structure.id.clone();
        let old_type = self.line.structure(&id).map(type_name).unwrap_or_default();
        let del = |kind, id: &String| Command::Delete {
            kind,
            id: id.clone(),
            confirm: false,
        };
        for j in &impact.jumpers {
            out.push(del(Kind::Jumper, j));
        }
        for s in &impact.spacers {
            out.push(del(Kind::Spacer, s));
        }
        for sec in &impact.sections {
            out.push(del(Kind::Section, sec));
        }
        out.push(put(Kind::Structure, &id, &structure));
        let to: BTreeMap<u32, u32> = moves
            .iter()
            .filter_map(|(from, to)| Some((*from, (*to)?)))
            .filter(|(from, to)| from != to)
            .collect();
        let moved = |p: &SectionPoint| {
            (p.structure == id)
                .then(|| to.get(&p.set))
                .flatten()
                .map(|set| SectionPoint::new(id.clone(), *set))
        };
        for sec in &self.line.sections {
            if impact.sections.contains(&sec.id) || !sec.points.iter().any(|p| moved(p).is_some()) {
                continue;
            }
            let mut sec = sec.clone();
            for p in &mut sec.points {
                if let Some(n) = moved(p) {
                    *p = n;
                }
            }
            out.push(put(Kind::Section, &sec.id.clone(), &sec));
        }
        for j in &self.line.jumpers {
            if impact.jumpers.contains(&j.id) {
                continue;
            }
            let ends = j.points().any(|p| moved(p).is_some());
            let clamps = dropped.iter().any(|(jid, _)| *jid == j.id);
            if !ends && !clamps {
                continue;
            }
            let mut j = j.clone();
            for p in [&mut j.from, &mut j.to]
                .into_iter()
                .filter_map(ohl_line::JumperEnd::point_mut)
            {
                if let Some(n) = moved(p) {
                    *p = n;
                }
            }
            if clamps {
                let jid = j.id.clone();
                j.edit_supports(|cs| {
                    cs.retain(|c| !dropped.contains(&(jid.clone(), c.label.clone())));
                });
            }
            out.push(put(Kind::Jumper, &j.id.clone(), &j));
        }
        out.extend(self.starts_renamed(moved));
        // In words, for the log.
        let mut note = format!(
            "replace structure {id}: {old_type} by {}",
            type_name(&structure)
        );
        let rewired: Vec<String> = moves
            .iter()
            .filter_map(|(from, to)| {
                let to = (*to)?;
                Some(if *from == to {
                    format!("{from}")
                } else {
                    format!("{from}→{to}")
                })
            })
            .collect();
        if !rewired.is_empty() {
            note.push_str(&format!("; sets rewired: {}", rewired.join(", ")));
        }
        for (title, items) in impact.groups() {
            if !items.is_empty() {
                note.push_str(&format!("; {title}: {}", items.join(", ")));
            }
        }
        notes.push(note);
    }
}
