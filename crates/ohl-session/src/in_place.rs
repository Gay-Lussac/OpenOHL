// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Commands applied in place (country server, phase 3, ADR-0013): the
//! objects, the line and the spatial index of the line worked on are
//! changed where the command changes them, the line is validated locally
//! ([`ohl_line::Line::validate_local`]: the objects changed and those
//! referring to them, found by the reference index
//! [`ohl_line::index::LineIndex`]), the sections to solve again and the
//! index entries are worked out from the objects changed — nothing is
//! copied, nothing is rebuilt from every object's JSON. An edit costs what
//! it touches, not the size of the line.
//!
//! The result is the same as the whole path's (the line made again from
//! every object and validated whole, the index and the dirty sections
//! from the two lines): the same line, refusals, log entries, index rows
//! and file (tested on random command sequences, `in_place_tests`, and
//! by the synthetic country's tests). Where the local path cannot be sure
//! of that, the command goes the whole way, as before:
//!
//! - a change of the line's settings (`meta` rows: systems, circuits,
//!   groups, defaults…) — every object may read them;
//! - an object in a form of older files that the line reads into another
//!   (routes of version 2, alignments of version 3, a structure's own
//!   load case sets of before 2026-11-15, a section's circuit name no
//!   circuit has, or a line holding such names): reading them changes
//!   other objects too;
//! - the import, and commands changing a large part of the line (more
//!   than [`WHOLE_ABOVE`] objects plus an eighth of the line): there the
//!   whole path is the faster one.
//!
//! A command applied in place keeps a journal (the objects, the line's
//! objects and index entries as they were) to put everything back when it
//! is refused half-way, and for the unit of work it is part of.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ohl_line::index::{Item, LineIndex, Of};
use ohl_line::validate::Scope;
use ohl_line::{Line, SectionPoint};
use serde_json::Value;

use crate::{Applied, Command, Kind, Object, Session, SessionError, refused, spatial};

/// Commands changing more line objects than this (plus an eighth of the
/// line) go the whole way (faster there; the same result).
pub const WHOLE_ABOVE: usize = 2000;

/// The objects of a line, as the session keeps them.
type Objects = BTreeMap<(Kind, String), Object>;
/// A line object as it was and its place (none: not there).
type Was = Option<(Item, usize)>;
/// The spatial index entries of a line.
type Spatial = BTreeMap<(Kind, String), spatial::Entry>;

#[cfg(test)]
thread_local! {
    /// Commands applied in place, sent the whole way, refused (tests).
    pub(crate) static COUNTS: std::cell::RefCell<[usize; 3]> = const { std::cell::RefCell::new([0; 3]) };
}

/// The kind of line object of a session kind.
pub(crate) fn of_kind(kind: Kind) -> Option<Of> {
    Some(match kind {
        Kind::Route => Of::Route,
        Kind::Alignment => Of::Alignment,
        Kind::AlignmentPoint => Of::AlignmentPoint,
        Kind::Structure => Of::Structure,
        Kind::Section => Of::Section,
        Kind::Jumper => Of::Jumper,
        Kind::Spacer => Of::Spacer,
        Kind::Meta | Kind::Report | Kind::Library | Kind::PointCloud | Kind::Annotation => {
            return None;
        }
    })
}

/// The session kind of a line object's kind.
pub(crate) fn kind_of(of: Of) -> Kind {
    match of {
        Of::Route => Kind::Route,
        Of::Alignment => Kind::Alignment,
        Of::AlignmentPoint => Kind::AlignmentPoint,
        Of::Structure => Kind::Structure,
        Of::Section => Kind::Section,
        Of::Jumper => Kind::Jumper,
        Of::Spacer => Kind::Spacer,
    }
}

/// What a command applied in place changed in memory, as it was.
#[derive(Default)]
pub(crate) struct Journal {
    /// The objects as they were (none: not there), in the order changed.
    objects: Vec<((Kind, String), Option<Object>)>,
    /// The line's objects made again, as they were (value and place;
    /// none: not there).
    items: Vec<(Of, String, Was)>,
    /// The circuit starts as they were, when they changed.
    starts: Option<Vec<SectionPoint>>,
    /// The spatial index entries as they were, in the order changed.
    spatial: Vec<((Kind, String), Option<spatial::Entry>)>,
    /// The wind sides kept were brought up to date (put back: dropped,
    /// worked out again when next needed).
    pub(crate) sides: bool,
}

/// What the line's update gives the rest of the command.
struct LineChange {
    /// The line's objects made again, as they were.
    before: Vec<(Of, String, Option<Item>)>,
    /// The sections whose wind side turned, when routes changed.
    sides: Option<BTreeSet<String>>,
    /// The objects whose index entry may differ.
    placed: BTreeSet<(Kind, String)>,
}

/// Puts back what `j` changed, in `objects`, `line` (with its `index`)
/// and `spatial`.
pub(crate) fn revert(
    objects: &mut Objects,
    line: &mut Line,
    index: &mut Option<LineIndex>,
    spatial: &mut Spatial,
    j: Journal,
) {
    for (key, old) in j.objects.into_iter().rev() {
        match old {
            Some(o) => {
                objects.insert(key, o);
            }
            None => {
                objects.remove(&key);
            }
        }
    }
    if !j.items.is_empty() {
        let index = index.get_or_insert_with(|| LineIndex::of(line));
        for (of, id, _) in &j.items {
            index.remove(line, *of, id);
        }
        let mut back: Vec<(Item, usize)> = j.items.into_iter().filter_map(|x| x.2).collect();
        back.sort_by_key(|a| (a.0.of(), a.1));
        for (item, at) in back {
            index.insert(line, item, at);
        }
    }
    if let Some(s) = j.starts {
        line.circuit_starts = s;
    }
    for (key, e) in j.spatial.into_iter().rev() {
        match e {
            Some(e) => {
                spatial.insert(key, e);
            }
            None => {
                spatial.remove(&key);
            }
        }
    }
}

/// The next place among the objects of `kind` (after the last).
pub(crate) fn next_ord(objects: &Objects, kind: Kind) -> i64 {
    objects
        .range((kind, String::new())..)
        .take_while(|((k, _), _)| *k == kind)
        .map(|(_, o)| o.ord)
        .max()
        .map_or(0, |m| m + 1)
}

/// Parses `value` as a line object of kind `of`.
fn parse(of: Of, value: &Value) -> Result<Item, serde_json::Error> {
    let v = value.clone();
    Ok(match of {
        Of::Route => Item::Route(serde_json::from_value(v)?),
        Of::Alignment => Item::Alignment(serde_json::from_value(v)?),
        Of::AlignmentPoint => Item::AlignmentPoint(serde_json::from_value(v)?),
        Of::Structure => Item::Structure(Box::new(serde_json::from_value(v)?)),
        Of::Section => Item::Section(Box::new(serde_json::from_value(v)?)),
        Of::Jumper => Item::Jumper(Box::new(serde_json::from_value(v)?)),
        Of::Spacer => Item::Spacer(serde_json::from_value(v)?),
    })
}

impl Session {
    /// The current line's reference index (made when first needed).
    pub(crate) fn line_index(&mut self) -> &LineIndex {
        if self.index.is_none() {
            self.index = Some(LineIndex::of(&self.line));
        }
        self.index.as_ref().expect("made above")
    }

    /// Checks what the session keeps in memory against what the whole way
    /// makes of its objects: the line (made again from every object and
    /// validated whole), the spatial index, the reference index. For tests
    /// and measurements (cost: a whole reading of the line).
    #[doc(hidden)]
    pub fn check_kept(&self) -> Result<(), String> {
        if self.objects.is_empty() {
            return Ok(());
        }
        let again = crate::line_of(&self.objects).map_err(|e| e.to_string())?;
        if again != self.line {
            return Err("the line kept differs from the line made again".into());
        }
        if spatial::entries(&self.line, &self.grid) != self.spatial {
            return Err("the spatial index kept differs from the one made again".into());
        }
        if let Some(index) = &self.index
            && *index != LineIndex::of(&self.line)
        {
            return Err("the reference index kept differs from the one made again".into());
        }
        if let Some(sides) = &self.sides
            && sides.against() != ohl_line::solve::sections_against_the_line(&self.line)
        {
            return Err("the wind sides kept differ from those worked out again".into());
        }
        Ok(())
    }

    /// Whether the command's puts and deletes `prims` may go in place.
    pub(crate) fn suits_in_place(&self, prims: &[Command]) -> bool {
        if self.whole_only {
            return false;
        }
        let mut line_objects = 0;
        for p in prims {
            let kind = match p {
                Command::Put { kind, .. } | Command::Delete { kind, .. } => *kind,
                _ => continue,
            };
            if kind == Kind::Meta {
                return false;
            }
            if of_kind(kind).is_some() {
                line_objects += 1;
            }
        }
        let size: usize = ohl_line::index::Of::ALL
            .iter()
            .map(|&of| ohl_line::index::count(&self.line, of))
            .sum();
        line_objects <= WHOLE_ABOVE + size / 8
    }

    /// Applies the puts and deletes `prims` (and the point changes
    /// `point_prims`) in place, validated locally, logged as
    /// `entry_kind`. `Ok(None)`: it cannot go in place (nothing changed:
    /// the whole path is to be taken). A refusal leaves everything as it
    /// was.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_in_place(
        &mut self,
        user: &str,
        mut prims: Vec<Command>,
        point_prims: Vec<Command>,
        entry_kind: &str,
        undoes: Option<i64>,
        cascade: bool,
        note: Option<String>,
    ) -> Result<Option<Applied>, SessionError> {
        let mut j = Journal::default();
        let out = self.in_place_steps(
            user,
            &mut prims,
            point_prims,
            entry_kind,
            undoes,
            cascade,
            note,
            &mut j,
        );
        #[cfg(test)]
        COUNTS.with(|c| {
            let mut c = c.borrow_mut();
            match &out {
                Ok(Some(_)) => c[0] += 1,
                Ok(None) => c[1] += 1,
                Err(_) => c[2] += 1,
            }
        });
        match out {
            Ok(Some(applied)) => {
                self.places_changed(&applied.objects);
                if let Some(kept) = &mut self.unit {
                    kept.replaced
                        .push((self.current.clone(), crate::Kept::Journal(j)));
                }
                Ok(Some(applied))
            }
            other => {
                if j.sides {
                    self.sides = None;
                }
                revert(
                    &mut self.objects,
                    &mut self.line,
                    &mut self.index,
                    &mut self.spatial,
                    j,
                );
                other
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn in_place_steps(
        &mut self,
        user: &str,
        prims: &mut Vec<Command>,
        point_prims: Vec<Command>,
        entry_kind: &str,
        undoes: Option<i64>,
        cascade: bool,
        note: Option<String>,
        j: &mut Journal,
    ) -> Result<Option<Applied>, SessionError> {
        self.line_index();
        // The objects, in place; the inverse (in reverse order).
        let mut inverse = Vec::new();
        let mut changed: Vec<(Kind, String)> = Vec::new();
        for p in prims.iter() {
            match p {
                Command::Put {
                    kind,
                    id,
                    value,
                    ord: wanted,
                } => {
                    let key = (*kind, id.clone());
                    let next = if self.objects.contains_key(&key) {
                        0
                    } else {
                        next_ord(&self.objects, *kind)
                    };
                    match self.objects.get_mut(&key) {
                        Some(o) => {
                            j.objects.push((key.clone(), Some(o.clone())));
                            let moved = wanted.filter(|w| *w != o.ord);
                            inverse.push(Command::Put {
                                kind: *kind,
                                id: id.clone(),
                                value: (*o.value).clone(),
                                ord: moved.map(|_| o.ord),
                            });
                            o.value = Arc::new(value.clone());
                            o.version += 1;
                            if let Some(w) = moved {
                                o.ord = w;
                            }
                        }
                        None => {
                            j.objects.push((key.clone(), None));
                            inverse.push(Command::Delete {
                                kind: *kind,
                                id: id.clone(),
                                confirm: false,
                            });
                            self.objects.insert(
                                key,
                                Object {
                                    ord: wanted.unwrap_or(next),
                                    version: 1,
                                    value: Arc::new(value.clone()),
                                },
                            );
                        }
                    }
                    changed.push((*kind, id.clone()));
                }
                Command::Delete { kind, id, .. } => {
                    let key = (*kind, id.clone());
                    let old = self
                        .objects
                        .remove(&key)
                        .ok_or_else(|| refused(format!("no {} {id}", kind.as_str())))?;
                    inverse.push(Command::Put {
                        kind: *kind,
                        id: id.clone(),
                        value: (*old.value).clone(),
                        ord: Some(old.ord),
                    });
                    j.objects.push((key, Some(old)));
                    changed.push((*kind, id.clone()));
                }
                _ => unreachable!("expanded before"),
            }
        }
        inverse.reverse();
        let objects_changed = !changed.is_empty();
        for p in &point_prims {
            if let Command::ChangePoints { cloud, .. } = p {
                changed.push((Kind::PointCloud, cloud.clone()));
            }
        }
        for (kind, id) in &changed {
            if *kind == Kind::Report
                && let Some(o) = self.objects.get(&(*kind, id.clone()))
            {
                crate::reports::check_value(id, &o.value)?;
            }
            if *kind == Kind::Annotation
                && let Some(o) = self.objects.get(&(*kind, id.clone()))
            {
                crate::annotations::check_value(id, &o.value)?;
            }
        }
        let change = if objects_changed {
            match self.update_line(&changed, j)? {
                Some(c) => Some(c),
                None => return Ok(None),
            }
        } else {
            None
        };
        // Annotations attached to what the command deleted go with it.
        if cascade && objects_changed {
            let gone = crate::annotations::orphaned(
                self.objects
                    .iter()
                    .filter(|((k, _), _)| *k == Kind::Annotation)
                    .map(|((_, id), o)| (id, &*o.value)),
                &self.line,
            );
            for id in gone {
                let key = (Kind::Annotation, id.clone());
                if let Some(old) = self.objects.remove(&key) {
                    inverse.insert(
                        0,
                        Command::Put {
                            kind: Kind::Annotation,
                            id: id.clone(),
                            value: (*old.value).clone(),
                            ord: Some(old.ord),
                        },
                    );
                    j.objects.push((key.clone(), Some(old)));
                    prims.push(Command::Delete {
                        kind: Kind::Annotation,
                        id: id.clone(),
                        confirm: false,
                    });
                    changed.push(key);
                }
            }
        }
        changed.sort();
        changed.dedup();
        let index = self.index.as_ref().expect("made above");
        let sections: Vec<String> = match &change {
            Some(c) => ohl_line::diff::sections_to_solve_again_local(
                &c.before,
                &self.line,
                index,
                c.sides.as_ref(),
            )
            .into_iter()
            .collect(),
            None => Vec::new(),
        };
        // The index entries that may differ: before and after.
        let mut placed: BTreeSet<(Kind, String)> = change
            .as_ref()
            .map(|c| c.placed.clone())
            .unwrap_or_default();
        placed.extend(
            changed
                .iter()
                .filter(|(k, _)| of_kind(*k).is_some())
                .cloned(),
        );
        let mut old_entries: Spatial = BTreeMap::new();
        let mut new_entries: Spatial = BTreeMap::new();
        for (kind, id) in &placed {
            if let Some(e) = self.spatial.get(&(*kind, id.clone())) {
                old_entries.insert((*kind, id.clone()), *e);
            }
            if let Some(e) = spatial::entry_of(&self.line, index, &self.grid, *kind, id) {
                new_entries.insert((*kind, id.clone()), e);
            }
        }
        // Edit rights.
        let objects_only: Vec<(Kind, String)> = changed
            .iter()
            .filter(|(k, _)| *k != Kind::PointCloud)
            .cloned()
            .collect();
        let points = change.as_ref().map(|c| {
            let mut before = BTreeMap::new();
            for (of, id, item) in &c.before {
                if let (Of::AlignmentPoint, Some(Item::AlignmentPoint(p))) = (of, item) {
                    before.insert(id.clone(), p.clone());
                }
            }
            let after: BTreeMap<String, ohl_line::AlignmentPoint> = before
                .keys()
                .chain(
                    changed
                        .iter()
                        .filter(|(k, _)| *k == Kind::AlignmentPoint)
                        .map(|(_, id)| id),
                )
                .filter_map(|id| Some((id.clone(), index.alignment_point(&self.line, id)?.clone())))
                .collect();
            (before, after)
        });
        self.check_edit(
            user,
            &objects_only,
            &old_entries,
            &new_entries,
            points.as_ref().map(|(a, b)| (a, b)),
        )?;
        for p in &point_prims {
            if let Command::ChangePoints { cloud, points } = p {
                let key = (Kind::PointCloud, cloud.clone());
                let area: Spatial = self
                    .conn
                    .points_entry(&self.grid, cloud, points)
                    .map(|e| (key.clone(), e))
                    .into_iter()
                    .collect();
                self.check_edit(user, &[key], &area, &area, None)?;
            }
        }
        let (seq, points_changed) = self.persist(
            user,
            entry_kind,
            undoes,
            note,
            point_prims,
            std::mem::take(prims),
            inverse,
            &changed,
            &self.objects,
            &sections,
            &old_entries,
            &new_entries,
        )?;
        // The index entries kept in memory.
        for key in &placed {
            let new = new_entries.get(key).copied();
            let old = self.spatial.get(key).copied();
            if new != old {
                j.spatial.push((key.clone(), old));
                match new {
                    Some(e) => self.spatial.insert(key.clone(), e),
                    None => self.spatial.remove(key),
                };
            }
        }
        Ok(Some(Applied {
            seq,
            objects: changed,
            sections,
            points: points_changed,
            notice: None,
            whole: false,
        }))
    }

    /// Makes the line's objects changed (`changed`) again from their rows,
    /// in place, and validates the line locally. `Ok(None)`: not possible
    /// in place (see the module's notes). Everything changed is in `j`.
    fn update_line(
        &mut self,
        changed: &[(Kind, String)],
        j: &mut Journal,
    ) -> Result<Option<LineChange>, SessionError> {
        if self.index.is_none() {
            self.index = Some(LineIndex::of(&self.line));
        }
        // The line's objects changed, read in the order the line is read
        // (kind, place): the first that does not read is the one said.
        let mut keys: Vec<(Kind, i64, String)> = Vec::new();
        let mut gone: Vec<(Of, String)> = Vec::new();
        for (kind, id) in changed {
            let Some(of) = of_kind(*kind) else { continue };
            match self.objects.get(&(*kind, id.clone())) {
                Some(o) => keys.push((*kind, o.ord, id.clone())),
                None => gone.push((of, id.clone())),
            }
        }
        keys.sort();
        keys.dedup();
        gone.sort();
        gone.dedup();
        let mut new: BTreeMap<(Of, String), Item> = BTreeMap::new();
        for (kind, _, id) in &keys {
            let of = of_kind(*kind).expect("a line object");
            let o = &self.objects[&(*kind, id.clone())];
            let item =
                parse(of, &o.value).map_err(|e| refused(format!("{} {id}: {e}", kind.as_str())))?;
            new.insert((of, id.clone()), item);
        }
        // Ids inside the JSON must match the object ids (in the order the
        // whole way checks them: by kind and id).
        let mut by_key: Vec<&(Kind, String)> = changed.iter().collect();
        by_key.sort();
        by_key.dedup();
        for (kind, id) in by_key {
            if *kind == Kind::Meta {
                continue;
            }
            if let Some(o) = self.objects.get(&(*kind, id.clone()))
                && o.value.get("id").and_then(Value::as_str) != Some(id)
            {
                return Err(refused(format!(
                    "{} {id}: its \"id\" field differs",
                    kind.as_str()
                )));
            }
        }
        // What only the whole path reads right: forms of older files,
        // circuit names given circuits when the line is read.
        for item in new.values() {
            let whole = match item {
                Item::Route(r) => r.is_legacy(),
                Item::Alignment(a) => a.legacy.is_some(),
                Item::Structure(s) => !s.load_cases.is_empty() || s.no_load_cases,
                Item::Section(s) => {
                    !s.circuit_name.is_empty()
                        && !self.line.circuits.iter().any(|c| c.name == s.circuit_name)
                }
                _ => false,
            };
            if whole {
                return Ok(None);
            }
        }
        let sections_change = new
            .keys()
            .chain(gone.iter())
            .any(|(of, _)| *of == Of::Section);
        if sections_change {
            // A line with circuits given when it was read keeps them only
            // while the names are on its sections.
            let stored = crate::meta_of(&self.objects).map_err(refused)?;
            let adopted = stored.is_some_and(|m| m.circuits.len() != self.line.circuits.len());
            if adopted {
                return Ok(None);
            }
        }
        // The structures' strings placed (in the line's order).
        for (kind, _, id) in &keys {
            if *kind != Kind::Structure {
                continue;
            }
            if let Some(Item::Structure(s)) = new.get_mut(&(Of::Structure, id.clone())) {
                s.take_string_geometry();
                s.place_wires()
                    .map_err(|m| refused(ohl_line::LineError::Invalid(m).to_string()))?;
            }
        }
        // The sections a spacer changed ends on are made again (their bare
        // end points get a clamp: `Line::clamp_spacer_ends`).
        let index = self.index.as_ref().expect("made above");
        let mut again: BTreeSet<String> = BTreeSet::new();
        for (of, id) in new.keys().chain(gone.iter()) {
            if *of != Of::Spacer {
                continue;
            }
            let old = index.get(&self.line, Of::Spacer, id);
            for item in [old.as_ref(), new.get(&(Of::Spacer, id.clone()))]
                .into_iter()
                .flatten()
            {
                if let Item::Spacer(s) = item {
                    again.insert(s.a.section.clone());
                    again.insert(s.b.section.clone());
                }
            }
        }
        for id in &again {
            let key = (Of::Section, id.clone());
            if new.contains_key(&key) || gone.contains(&key) {
                continue;
            }
            if let Some(o) = self.objects.get(&(Kind::Section, id.clone())) {
                // Read before: it reads.
                let Ok(item) = parse(Of::Section, &o.value) else {
                    return Ok(None);
                };
                new.insert(key, item);
            }
        }
        // Every line object made again: changed, added, deleted.
        let mut made: Vec<(Of, String)> = new.keys().cloned().collect();
        made.extend(gone.iter().cloned());
        made.extend(
            again
                .iter()
                .filter(|id| index.position(Of::Section, id).is_some())
                .map(|id| (Of::Section, id.clone())),
        );
        made.sort();
        made.dedup();
        // As they were.
        let before: Vec<(Of, String, Was)> = made
            .iter()
            .map(|(of, id)| {
                let at = index.position(*of, id);
                (
                    *of,
                    id.clone(),
                    at.and_then(|k| index.get(&self.line, *of, id).map(|x| (x, k))),
                )
            })
            .collect();
        let routes = made
            .iter()
            .any(|(of, _)| matches!(of, Of::Route | Of::Alignment | Of::AlignmentPoint));
        // The sections whose ends may move (their wind side worked out
        // again): changed, or through a structure changed.
        let mut moved: BTreeSet<String> = BTreeSet::new();
        let mut moved_by = |index: &LineIndex| {
            for (of, id) in &made {
                match of {
                    Of::Section => {
                        moved.insert(id.clone());
                    }
                    Of::Structure => {
                        moved.extend(index.referrers_of(Of::Structure, id, Of::Section).cloned())
                    }
                    _ => {}
                }
            }
        };
        moved_by(index);
        if routes && self.sides.is_none() {
            self.sides = Some(ohl_line::solve::Sides::of(&self.line));
        }
        let mut scope = Scope::of_change(&self.line, index, &made);
        let mut placed = placed_by(&self.line, index, &made);
        j.items.extend(before.iter().cloned());
        // Into the line: first everything but the sections, then the
        // sections (their spacers' clamps read from the line).
        let mut index = self.index.take().expect("made above");
        let line = &mut self.line;
        let objects = &self.objects;
        let ord_of = |of: Of, id: &str| objects[&(kind_of(of), id.to_owned())].ord;
        for pass in [false, true] {
            let ours = |of: Of| (of == Of::Section) == pass;
            // Those whose place changes (or that come or go) are taken out
            // first: the others stay in their order.
            let mut placing: Vec<(Of, String, Item)> = Vec::new();
            for (of, id, was) in &before {
                if !ours(*of) {
                    continue;
                }
                let key = (*of, id.clone());
                let Some(mut item) = new.remove(&key) else {
                    index.remove(line, *of, id);
                    continue;
                };
                if let Item::Section(s) = &mut item {
                    clamp_spacer_ends(&index, line, s);
                }
                let same_place = was.is_some()
                    && j.objects
                        .iter()
                        .find(|(k, _)| k.0 == kind_of(*of) && k.1 == *id)
                        .and_then(|(_, o)| o.as_ref())
                        .is_none_or(|o| o.ord == ord_of(*of, id));
                if same_place {
                    index.replace(line, item);
                } else {
                    index.remove(line, *of, id);
                    placing.push((*of, id.clone(), item));
                }
            }
            for (of, id, item) in placing {
                let ord = ord_of(of, &id);
                let n = ohl_line::index::count(line, of);
                let at = partition(n, |k| {
                    let other = item_id(line, of, k);
                    (ord_of(of, other), other) < (ord, id.as_str())
                });
                index.insert(line, item, at);
            }
        }
        // The circuit starts: those a section's point (once each).
        if made.iter().any(|(of, _)| *of == Of::Section) {
            let stored = crate::meta_of(objects)
                .map_err(refused)?
                .map(|m| m.circuit_starts)
                .unwrap_or_default();
            let mut seen: Vec<&SectionPoint> = Vec::new();
            let mut starts = Vec::new();
            for p in &stored {
                let keep = !seen.contains(&p)
                    && index
                        .at_point(&p.structure, p.set, Of::Section)
                        .next()
                        .is_some();
                seen.push(p);
                if keep {
                    starts.push(p.clone());
                }
            }
            if starts != line.circuit_starts {
                j.starts = Some(std::mem::replace(&mut line.circuit_starts, starts));
            }
        }
        scope.merge(Scope::of_change(line, &index, &made));
        placed.extend(placed_by(line, &index, &made));
        let checked = line.validate_local(&index, &scope);
        moved_by(&index);
        self.index = Some(index);
        checked.map_err(|e| refused(e.to_string()))?;
        // The wind sides kept (when kept): those that may turn worked out.
        let mut sides = None;
        if let Some(kept) = &mut self.sides {
            j.sides = true;
            let index = self.index.as_ref().expect("put back above");
            let turned = kept.update(&self.line, index, &moved, routes);
            sides = routes.then_some(turned);
        }
        Ok(Some(LineChange {
            before: before
                .into_iter()
                .map(|(of, id, x)| (of, id, x.map(|x| x.0)))
                .collect(),
            sides,
            placed,
        }))
    }
}

/// Gives the bare span points of `s` that spacers of `line` end on a clamp
/// of no weight (as [`Line::clamp_spacer_ends`] does for the whole line).
fn clamp_spacer_ends(index: &LineIndex, line: &Line, s: &mut ohl_line::Section) {
    let mut spacers: Vec<usize> = index
        .referrers_of(Of::Section, &s.id, Of::Spacer)
        .filter_map(|id| index.position(Of::Spacer, id))
        .collect();
    spacers.sort_unstable();
    for k in spacers {
        let x = &line.spacers[k];
        for end in [&x.a, &x.b] {
            if end.section != s.id {
                continue;
            }
            if let Some(p) = s.span_points.iter_mut().find(|p| p.label == end.point)
                && p.object.is_none()
            {
                p.object = Some(ohl_line::LineObject::clamp(0.0));
            }
        }
    }
}

/// The id of the object of kind `of` at place `k`.
fn item_id(line: &Line, of: Of, k: usize) -> &str {
    match of {
        Of::Route => &line.routes[k].id,
        Of::Alignment => &line.alignments[k].id,
        Of::AlignmentPoint => &line.alignment_points[k].id,
        Of::Structure => &line.structures[k].id,
        Of::Section => &line.sections[k].id,
        Of::Jumper => &line.jumpers[k].id,
        Of::Spacer => &line.spacers[k].id,
    }
}

/// The first of `0..n` for which `before` is false (it is true, then
/// false).
fn partition(n: usize, before: impl Fn(usize) -> bool) -> usize {
    let (mut lo, mut hi) = (0, n);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if before(mid) {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo
}

/// The objects of `line` whose plan bounds (index entries) may change when
/// the objects `made` change: those, the sections through a structure
/// among them, the jumpers ending or clamped at one, the jumpers and
/// spacers on those sections (all those on a section with a span point by
/// station, when the routes or a structure change), the jumpers tapping
/// those jumpers, the alignments through a point among them and the
/// routes of those alignments. Called on the line before and after.
fn placed_by(line: &Line, index: &LineIndex, made: &[(Of, String)]) -> BTreeSet<(Kind, String)> {
    use ohl_line::index::Target;
    let mut out: BTreeSet<(Kind, String)> = BTreeSet::new();
    let mut sections: BTreeSet<String> = BTreeSet::new();
    let mut jumpers: Vec<String> = Vec::new();
    let mut alignments: BTreeSet<String> = BTreeSet::new();
    let mut geometry = false;
    for (of, id) in made {
        out.insert((kind_of(*of), id.clone()));
        match of {
            Of::Structure => {
                geometry = true;
                for (k, r) in index.referrers(&Target::Object(Of::Structure, id.clone())) {
                    match k {
                        Of::Section => {
                            sections.insert(r.clone());
                        }
                        Of::Jumper => jumpers.push(r.clone()),
                        _ => {}
                    }
                }
            }
            Of::Section => {
                sections.insert(id.clone());
            }
            Of::Jumper => jumpers.push(id.clone()),
            Of::AlignmentPoint => {
                geometry = true;
                alignments.extend(
                    index
                        .referrers_of(Of::AlignmentPoint, id, Of::Alignment)
                        .cloned(),
                );
            }
            Of::Alignment => {
                geometry = true;
                alignments.insert(id.clone());
            }
            Of::Route => geometry = true,
            Of::Spacer => {}
        }
    }
    if geometry {
        sections.extend(index.station_sections().iter().cloned());
    }
    for s in &sections {
        out.insert((Kind::Section, s.clone()));
        jumpers.extend(index.referrers_of(Of::Section, s, Of::Jumper).cloned());
        for x in index.referrers_of(Of::Section, s, Of::Spacer) {
            out.insert((Kind::Spacer, x.clone()));
        }
    }
    let mut seen: BTreeSet<String> = BTreeSet::new();
    while let Some(x) = jumpers.pop() {
        if !seen.insert(x.clone()) {
            continue;
        }
        out.insert((Kind::Jumper, x.clone()));
        jumpers.extend(index.referrers_of(Of::Jumper, &x, Of::Jumper).cloned());
    }
    for a in alignments {
        out.insert((Kind::Alignment, a.clone()));
        if let Some(k) = index.position(Of::Alignment, &a) {
            out.insert((Kind::Route, line.alignments[k].route.clone()));
        }
    }
    out
}
