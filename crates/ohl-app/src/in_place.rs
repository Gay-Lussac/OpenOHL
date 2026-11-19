// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The desktop's line kept in step with the session in place (O42, owner
//! 2026-11-19: "per element modified instead of validating the whole
//! thing"): after a command the session applied in place, only the
//! objects it changed are taken from the session's line — a structure
//! resolved with its type alone ([`ohl_line::library::resolve_structure`])
//! — put into the desktop's line where the session has them, and the line
//! validated locally ([`ohl_line::Line::validate_local`], the desktop's own
//! reference index kept with it). The result is the line
//! [`Model`](crate::model::Model) would make again whole (the session's
//! line copied, every structure resolved, validated whole): proved on
//! random command sequences (`tests::in_place_line`).
//!
//! The line is made again whole, as before, where in place cannot be sure
//! of that ([`Model::sync`](crate::model::Model) says which, in the edit's
//! report): the session went the whole way ([`ohl_session::Applied::whole`]:
//! objects of older forms read into others, an import…); the line's
//! settings changed (a `meta` object: the creep-stretch, bimetallic or
//! reference-state switches, systems, circuits, the solver…: every object
//! may read them); a library changed (read again whole, as before); the
//! line was replaced meanwhile (another project line, a library read
//! again, a cancelled drag); or the objects did not land in the session's
//! order (a check; never seen).

use std::collections::{BTreeMap, BTreeSet};

use ohl_line::index::{Item, LineIndex, Of, count};
use ohl_line::validate::Scope;
use ohl_line::{Line, LineError};
use ohl_session::{Applied, Kind};

/// The desktop's line kept in step with the session's: the line version
/// and project line it is in step at, its reference index (made when
/// first needed), the structures moved in memory since (a drag previewed:
/// taken from the session again at the next edit).
#[derive(Debug, Default)]
pub struct InStep {
    /// The line version at which the line is the session's line resolved.
    pub version: u64,
    /// The project line it is of.
    pub line: String,
    /// The line's reference index, kept with it.
    pub index: Option<LineIndex>,
    /// Structures moved in memory since (previewed).
    pub previewed: BTreeSet<String>,
}

impl InStep {
    /// In step at `version`, project line `line` (the index made when
    /// first needed).
    pub fn at(version: u64, line: &str) -> Self {
        Self {
            version,
            line: line.to_owned(),
            index: None,
            previewed: BTreeSet::new(),
        }
    }
}

/// The line object kind of a session kind (none: not in the line).
fn of_kind(kind: Kind) -> Option<Of> {
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

/// Why `applied` cannot be taken in place (none: it can): a change of
/// the line's settings or libraries, or the session's whole way.
pub fn whole_reason(applied: &Applied) -> Option<&'static str> {
    if applied.objects.iter().any(|(k, _)| *k == Kind::Meta) {
        return Some("the line's settings changed");
    }
    if applied.objects.iter().any(|(k, _)| *k == Kind::Library) {
        return Some("a library changed");
    }
    if applied.whole {
        return Some("the session made its line again whole");
    }
    None
}

/// The ids of kind `of` of `line`, in order.
fn ids(line: &Line, of: Of) -> Vec<&str> {
    match of {
        Of::Route => line.routes.iter().map(|x| x.id.as_str()).collect(),
        Of::Alignment => line.alignments.iter().map(|x| x.id.as_str()).collect(),
        Of::AlignmentPoint => line
            .alignment_points
            .iter()
            .map(|x| x.id.as_str())
            .collect(),
        Of::Structure => line.structures.iter().map(|x| x.id.as_str()).collect(),
        Of::Section => line.sections.iter().map(|x| x.id.as_str()).collect(),
        Of::Jumper => line.jumpers.iter().map(|x| x.id.as_str()).collect(),
        Of::Spacer => line.spacers.iter().map(|x| x.id.as_str()).collect(),
    }
}

/// The objects of kind `of` of `stored` among `wanted`, with their place.
fn taken(
    stored: &Line,
    of: Of,
    wanted: &BTreeSet<&str>,
    resolve: &dyn Fn(&ohl_line::Structure) -> Result<ohl_line::Structure, LineError>,
) -> Result<Vec<(Item, usize)>, LineError> {
    let mut out = Vec::new();
    macro_rules! scan {
        ($v:expr, $make:expr) => {
            for (k, x) in $v.iter().enumerate() {
                if wanted.contains(x.id.as_str()) {
                    out.push(($make(x)?, k));
                }
            }
        };
    }
    let ok = |i: Item| -> Result<Item, LineError> { Ok(i) };
    match of {
        Of::Route => scan!(stored.routes, |x: &ohl_line::Route| ok(Item::Route(
            x.clone()
        ))),
        Of::Alignment => scan!(stored.alignments, |x: &ohl_line::Alignment| ok(
            Item::Alignment(x.clone())
        )),
        Of::AlignmentPoint => scan!(stored.alignment_points, |x: &ohl_line::AlignmentPoint| ok(
            Item::AlignmentPoint(x.clone())
        )),
        Of::Structure => scan!(stored.structures, |x: &ohl_line::Structure| resolve(x)
            .map(|s| Item::Structure(Box::new(s)))),
        Of::Section => scan!(stored.sections, |x: &ohl_line::Section| ok(Item::Section(
            Box::new(x.clone())
        ))),
        Of::Jumper => scan!(stored.jumpers, |x: &ohl_line::Jumper| ok(Item::Jumper(
            Box::new(x.clone())
        ))),
        Of::Spacer => scan!(stored.spacers, |x: &ohl_line::Spacer| ok(Item::Spacer(
            x.clone()
        ))),
    }
    Ok(out)
}

/// A line's own fields: everything but its objects and what the program
/// works out from the libraries and cases (the reference state's check,
/// the creep-stretch cases: [`crate::edit`] does them after). Taken from
/// the session's line with the objects (the circuit starts follow the
/// sections; the others change only with a settings change, which goes
/// the whole way).
struct Settings(Line);

impl Settings {
    /// `line`'s own fields (its objects left empty).
    fn of(line: &Line) -> Self {
        // Every field named: one added to the line is a decision here.
        let Line {
            format,
            version,
            name,
            crs,
            height_datum,
            alignment,
            routes: _,
            alignments: _,
            alignment_points: _,
            structures: _,
            sections: _,
            jumpers: _,
            spacers: _,
            systems,
            circuits,
            display,
            creep_stretch,
            bimetallic,
            solver,
            defaults,
            circuit_starts,
            structure_loads,
            structure_groups,
            electrical,
            reference,
            feet_state,
            condition_cases,
        } = line;
        Self(Line {
            format: format.clone(),
            version: *version,
            name: name.clone(),
            crs: crs.clone(),
            height_datum: height_datum.clone(),
            alignment: alignment.clone(),
            routes: Vec::new(),
            alignments: Vec::new(),
            alignment_points: Vec::new(),
            structures: Vec::new(),
            sections: Vec::new(),
            jumpers: Vec::new(),
            spacers: Vec::new(),
            systems: systems.clone(),
            circuits: circuits.clone(),
            display: display.clone(),
            creep_stretch: creep_stretch.clone(),
            bimetallic: *bimetallic,
            solver: *solver,
            defaults: *defaults,
            circuit_starts: circuit_starts.clone(),
            structure_loads: structure_loads.clone(),
            structure_groups: structure_groups.clone(),
            electrical: *electrical,
            reference: reference.clone(),
            feet_state: feet_state.clone(),
            condition_cases: condition_cases.clone(),
        })
    }

    /// Puts them into `line` (the reference state and what is worked out
    /// left as they are).
    fn put(self, line: &mut Line) {
        let s = self.0;
        line.format = s.format;
        line.version = s.version;
        line.name = s.name;
        line.crs = s.crs;
        line.height_datum = s.height_datum;
        line.alignment = s.alignment;
        line.systems = s.systems;
        line.circuits = s.circuits;
        line.display = s.display;
        line.creep_stretch = s.creep_stretch;
        line.bimetallic = s.bimetallic;
        line.solver = s.solver;
        line.defaults = s.defaults;
        line.circuit_starts = s.circuit_starts;
        line.structure_loads = s.structure_loads;
        line.structure_groups = s.structure_groups;
        line.electrical = s.electrical;
    }

    /// Whether `a` and `b` have the same own fields (but the reference
    /// state and what is worked out).
    fn same(a: &Line, b: &Line) -> bool {
        a.format == b.format
            && a.version == b.version
            && a.name == b.name
            && a.crs == b.crs
            && a.height_datum == b.height_datum
            && a.alignment == b.alignment
            && a.systems == b.systems
            && a.circuits == b.circuits
            && a.display == b.display
            && a.creep_stretch == b.creep_stretch
            && a.bimetallic == b.bimetallic
            && a.solver == b.solver
            && a.defaults == b.defaults
            && a.circuit_starts == b.circuit_starts
            && a.structure_loads == b.structure_loads
            && a.structure_groups == b.structure_groups
            && a.electrical == b.electrical
    }
}

/// A line object as it was and its place (none: not there).
type Was = Option<(Item, usize)>;

/// What an update in place did.
#[derive(Debug, Default)]
pub struct Updated {
    /// Line objects taken from the session.
    pub objects: usize,
}

/// Why an update in place did not happen.
#[derive(Debug)]
pub enum NotInPlace {
    /// The line would not be valid (or a structure not resolve): the line
    /// is left as it was (the same refusal as the whole way's).
    Invalid(LineError),
    /// The objects did not land in the session's order: the line is left
    /// as it was, to be made again whole.
    Order(Of),
}

/// Takes the line objects `applied` changed (and the structures
/// `step.previewed`) from `stored`, the session's line, into `line`, the
/// desktop's (in step with the session before the command: the session's
/// line resolved), resolving structures with `resolve`, and validates the
/// line locally. The line's own fields (its settings, the circuit starts
/// worked out from the sections) are the session's too ([`Settings`]). On
/// an error the line is left as it was.
pub fn update(
    line: &mut Line,
    step: &mut InStep,
    stored: &Line,
    applied: &Applied,
    resolve: &dyn Fn(&ohl_line::Structure) -> Result<ohl_line::Structure, LineError>,
    check_strings: bool,
) -> Result<Updated, NotInPlace> {
    let index = step.index.get_or_insert_with(|| LineIndex::of(line));
    // What is made again: the objects changed, the structures previewed,
    // and the sections the spacers changed end on (their bare end points
    // given a clamp: the session makes them again too).
    let mut made: BTreeSet<(Of, String)> = applied
        .objects
        .iter()
        .filter_map(|(k, id)| of_kind(*k).map(|of| (of, id.clone())))
        .collect();
    made.extend(
        std::mem::take(&mut step.previewed)
            .into_iter()
            .map(|id| (Of::Structure, id)),
    );
    let spacers: Vec<String> = made
        .iter()
        .filter(|(of, _)| *of == Of::Spacer)
        .map(|(_, id)| id.clone())
        .collect();
    for id in &spacers {
        let old = index.get(line, Of::Spacer, id);
        let new = stored.spacers.iter().find(|s| &s.id == id);
        for s in old
            .iter()
            .filter_map(|i| match i {
                Item::Spacer(s) => Some(s),
                _ => None,
            })
            .chain(new)
        {
            made.insert((Of::Section, s.a.section.clone()));
            made.insert((Of::Section, s.b.section.clone()));
        }
    }
    let made: Vec<(Of, String)> = made.into_iter().collect();
    if made.is_empty() && Settings::same(line, stored) {
        return Ok(Updated::default());
    }
    // The objects as the session has them now, resolved, before anything
    // changes (an error leaves the line as it was).
    let mut new: BTreeMap<(Of, String), (Item, usize)> = BTreeMap::new();
    for of in Of::ALL {
        let wanted: BTreeSet<&str> = made
            .iter()
            .filter(|(o, _)| *o == of)
            .map(|(_, id)| id.as_str())
            .collect();
        if wanted.is_empty() {
            continue;
        }
        for (item, at) in taken(stored, of, &wanted, resolve).map_err(NotInPlace::Invalid)? {
            new.insert((of, item.id().to_owned()), (item, at));
        }
    }
    // As they were, with their place.
    let before: Vec<(Of, String, Was)> = made
        .iter()
        .map(|(of, id)| {
            let at = index.position(*of, id);
            (
                *of,
                id.clone(),
                at.and_then(|k| index.get(line, *of, id).map(|x| (x, k))),
            )
        })
        .collect();
    let mut scope = Scope::of_change(line, index, &made);
    // Those in the same place replaced; the others taken out, then put
    // in at the session's places (in order).
    let mut placing: Vec<(Item, usize)> = Vec::new();
    for (of, id, was) in &before {
        let key = (*of, id.clone());
        match (new.remove(&key), was) {
            (Some((item, at)), Some((_, k))) if at == *k => {
                index.replace(line, item);
            }
            (Some(x), Some(_)) => {
                index.remove(line, *of, id);
                placing.push(x);
            }
            (Some(x), None) => placing.push(x),
            (None, Some(_)) => {
                index.remove(line, *of, id);
            }
            (None, None) => {}
        }
    }
    placing.sort_by_key(|(item, at)| (item.of(), *at));
    for (item, at) in placing {
        let at = at.min(count(line, item.of()));
        index.insert(line, item, at);
    }
    let settings = (!Settings::same(line, stored)).then(|| {
        let was = Settings::of(line);
        Settings::of(stored).put(line);
        was
    });
    // Every kind changed in the session's order (a check, cheap).
    let kinds: BTreeSet<Of> = made.iter().map(|(of, _)| *of).collect();
    let order = kinds
        .iter()
        .copied()
        .find(|of| ids(line, *of) != ids(stored, *of));
    let checked = match order {
        Some(of) => Err(NotInPlace::Order(of)),
        None => {
            scope.merge(Scope::of_change(line, index, &made));
            line.validate_local(index, &scope)
                .and_then(|()| {
                    // Merged with their types: every string's top is there
                    // (as the whole way checks every structure).
                    if !check_strings {
                        return Ok(());
                    }
                    for (of, id) in &made {
                        if *of == Of::Structure
                            && let Some(s) = index.structure(line, id)
                        {
                            s.check_strings().map_err(LineError::Invalid)?;
                        }
                    }
                    Ok(())
                })
                .map_err(NotInPlace::Invalid)
        }
    };
    if let Err(e) = checked {
        // Put back as it was.
        for (of, id) in &made {
            index.remove(line, *of, id);
        }
        let mut back: Vec<(Item, usize)> = before.into_iter().filter_map(|x| x.2).collect();
        back.sort_by_key(|(item, at)| (item.of(), *at));
        for (item, at) in back {
            index.insert(line, item, at);
        }
        if let Some(s) = settings {
            s.put(line);
        }
        return Err(e);
    }
    Ok(Updated {
        objects: made.len(),
    })
}

/// Where two lines first differ, in words (the tests' messages).
pub fn first_difference(a: &Line, b: &Line) -> String {
    let (Ok(x), Ok(y)) = (serde_json::to_value(a), serde_json::to_value(b)) else {
        return "not written".into();
    };
    let (Some(x), Some(y)) = (x.as_object(), y.as_object()) else {
        return "not objects".into();
    };
    for (k, v) in x {
        let w = y.get(k);
        if Some(v) == w {
            continue;
        }
        if let (Some(p), Some(q)) = (v.as_array(), w.and_then(|w| w.as_array())) {
            for (i, (e, f)) in p.iter().zip(q).enumerate() {
                if e != f {
                    return format!("{k}[{i}] ({}): kept {e} — made again {f}", e["id"]);
                }
            }
            return format!("{k}: {} kept, {} made again", p.len(), q.len());
        }
        return format!("{k}: kept {v} — made again {w:?}");
    }
    if a.condition_cases != b.condition_cases {
        return "the condition cases".into();
    }
    "nothing written differs".into()
}
