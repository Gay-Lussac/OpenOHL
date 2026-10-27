//! Another program's project brought into an existing project line
//! (owner, 2026-10-25): its routes said to be the project's own or new,
//! what appears twice found and settled one by one, the rest added.
//!
//! - **Routes**: each imported route is either one of the project's
//!   (its alignments join that route) or a new route.
//! - **What appears twice** ([`Duplicate`]): an object of the same id with
//!   other values, or of another id at the same place — an alignment point
//!   or structure within the tolerance in plan, a section over the same
//!   structures and sets, a jumper between the same ends — and a library
//!   file of the same name with another text. Each is settled by a
//!   [`Resolution`]: keep the project's (the imported one left out, what
//!   refers to it then refers to the project's), take the imported one (it
//!   replaces the project's, under the project's id), or keep both (the
//!   imported one under an id of its own).
//! - **Towers in both** ([`SharedTower`], owner 2026-10-27): a structure
//!   within the tolerance of one of the project's (of its id or another)
//!   is kept once, as the project's — its id, position, orientation and
//!   type — and its sets are settled one by one ([`SetChoice`]): a set
//!   only in the import added (or left out), a set in both at the same
//!   place the same attachment, a set in both at other places kept as the
//!   project's, taken from the import, or added under a new number. Of
//!   the same id further apart, it is another structure, kept apart under
//!   an id of its own (`T1-2`) with its type; so are alignment points,
//!   and sections, jumpers and alignments of the same id elsewhere (other
//!   structures, another route).
//! - **Library files by what they hold** where that is clear: wire files
//!   cable by cable (a cable of another id added, of the same id with
//!   other data a duplicate: keep the project's or the imported one under
//!   a new id), structure type files by type id; other kinds by name.
//! - Ids are changed in a scratch session with the project's own rename
//!   commands, so whatever refers to an object follows as it does when
//!   one is renamed by hand.
//!
//! The result is the merged line and the library files to write
//! ([`ImportPlan`]); the caller writes it as one change (the session's
//! `ReplaceLine`).

use std::collections::{BTreeMap, BTreeSet};

use ohl_line::Line;
use ohl_session::{Command, Kind, Session};
use serde_json::Value;

use crate::projects::ImportedProject;

mod entries;
mod towers;

pub use towers::{SetChoice, SetState, SharedSet, SharedTower};

/// How a duplicate is settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Resolution {
    /// The project's kept; the imported one left out (what refers to it
    /// refers to the project's).
    KeepExisting,
    /// The imported one replaces the project's (under the project's id).
    TakeImported,
    /// Both kept: the imported one under an id of its own.
    KeepBoth,
}

impl Resolution {
    /// In words, for the window.
    pub fn words(self) -> &'static str {
        match self {
            Self::KeepExisting => "keep the project's",
            Self::TakeImported => "take the imported one",
            Self::KeepBoth => "keep both",
        }
    }

    /// Every resolution, in the window's order.
    pub const ALL: [Self; 3] = [Self::KeepExisting, Self::TakeImported, Self::KeepBoth];
}

/// Why two objects are taken to be the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Twice {
    /// The same id, other values.
    SameId,
    /// Another id, the same place.
    SamePlace,
}

/// An object (or library file) that appears in the project and in the
/// import.
#[derive(Debug, Clone, PartialEq)]
pub struct Duplicate {
    /// Key the choice is given by: `structure:T3` (the imported id),
    /// `library:cables/x.ohlw`.
    pub key: String,
    /// The kind, in words (`structure`, `section`, `library file`…).
    pub kind: &'static str,
    /// The project's id (or library file).
    pub existing: String,
    /// The imported one's id.
    pub imported: String,
    /// Why they are taken to be the same.
    pub twice: Twice,
    /// What differs, in words (top-level fields; a library file: "text").
    pub differences: Vec<String>,
    /// How it is settled (the choice given, else keep the project's).
    pub resolution: Resolution,
}

impl Duplicate {
    /// The resolutions it has, in the window's order: a cable is kept as
    /// the project's or both (the imported one under a new id).
    pub fn choices(&self) -> &'static [Resolution] {
        match self.kind {
            "cable" => &[Resolution::KeepExisting, Resolution::KeepBoth],
            _ => &Resolution::ALL,
        }
    }
}

/// What the import does.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportPlan {
    /// What appears twice, in the order the kinds are merged.
    pub duplicates: Vec<Duplicate>,
    /// The towers in both, their sets compared.
    pub shared: Vec<SharedTower>,
    /// Structures of the merged line whose type no type file of the
    /// project holds after the import: structure, type (to say before
    /// importing).
    pub missing_types: Vec<(String, String)>,
    /// Objects the same in both, by kind (left as they are).
    pub identical: BTreeMap<&'static str, usize>,
    /// Objects added, by kind.
    pub added: BTreeMap<&'static str, usize>,
    /// Imported objects given another id: kind, from, to.
    pub renamed: Vec<(&'static str, String, String)>,
    /// The merged line.
    pub line: Line,
    /// Library files to write: kind, name, text.
    pub libraries: Vec<(String, String, String)>,
    /// What else to know.
    pub notes: Vec<String>,
}

/// How the import is to be done.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportOptions {
    /// Per imported route (its id), the project's route it is (none: a
    /// new route); a route not given is the project's route of the same
    /// id, else new.
    pub routes: BTreeMap<String, Option<String>>,
    /// Alignment points and structures nearer than this in plan are the
    /// same place, m (0: [`DEFAULT_TOLERANCE`]).
    pub tolerance: f64,
    /// The choices, by [`Duplicate::key`].
    pub choices: BTreeMap<String, Resolution>,
    /// The choices for the sets of towers in both, by [`SharedSet::key`]
    /// (`tower:<imported id>:set:<n>`; not given: the default, the first
    /// of [`SharedSet::choices`]).
    pub sets: BTreeMap<String, SetChoice>,
}

/// Alignment points and structures nearer than this in plan, m, are taken
/// to be the same place.
pub const DEFAULT_TOLERANCE: f64 = 1.0;

const USER: &str = "import";

/// The import of `imported` into `existing` (the project line's line)
/// with its library files `existing_libs` (kind, name, text), as `options`
/// say: what appears twice, settled by the choices given (else keeping
/// the project's), and the merged line. Refused when the coordinate
/// systems differ. Nothing is written.
pub fn plan(
    existing: &Line,
    existing_libs: &[(String, String, String)],
    imported: &ImportedProject,
    options: &ImportOptions,
) -> Result<ImportPlan, String> {
    if !existing.crs.is_empty()
        && !imported.line.crs.is_empty()
        && existing.crs != imported.line.crs
    {
        return Err(format!(
            "the import is in \"{}\", the project in \"{}\": reproject one of them first",
            imported.line.crs, existing.crs
        ));
    }
    let tolerance = if options.tolerance > 0.0 {
        options.tolerance
    } else {
        DEFAULT_TOLERANCE
    };
    let mut out = ImportPlan {
        duplicates: Vec::new(),
        shared: Vec::new(),
        missing_types: Vec::new(),
        identical: BTreeMap::new(),
        added: BTreeMap::new(),
        renamed: Vec::new(),
        line: existing.clone(),
        libraries: Vec::new(),
        notes: Vec::new(),
    };
    let mut scratch = Scratch::new(&imported.line)?;
    // Ids taken in the project and given out so far.
    let mut taken: BTreeMap<&'static str, BTreeSet<String>> = BTreeMap::new();
    let choose = |key: &str| {
        options
            .choices
            .get(key)
            .copied()
            .unwrap_or(Resolution::KeepExisting)
    };
    // The structures as they are in effect, each line with its own
    // library files (a tower's sets may live in its type).
    let ours_in_effect = towers::effective(existing, existing_libs);
    let theirs_in_effect = towers::effective(&imported.line, &imported.libraries);

    // Routes: the project's (renamed to it, the imported route left out)
    // or new (renamed only if its id is taken).
    let mut route_to_existing: BTreeSet<String> = BTreeSet::new();
    let ids = |v: Vec<String>| -> BTreeSet<String> { v.into_iter().collect() };
    taken.insert(
        "route",
        ids(existing.routes.iter().map(|r| r.id.clone()).collect()),
    );
    for r in scratch.line().routes.clone() {
        // Not said: the project's route of the same id, if any.
        let said = match options.routes.get(&r.id) {
            Some(x) => x.clone(),
            None => existing
                .routes
                .iter()
                .any(|x| x.id == r.id)
                .then(|| r.id.clone()),
        };
        match said {
            Some(e) if existing.routes.iter().any(|x| x.id == e) => {
                if r.id != e {
                    scratch.rename(Kind::Route, "route", &r.id, &e, &mut out)?;
                }
                route_to_existing.insert(e);
            }
            _ => {
                if taken["route"].contains(&r.id) {
                    let to = free_id(&r.id, &taken["route"], &scratch.ids(Kind::Route));
                    scratch.rename(Kind::Route, "route", &r.id, &to, &mut out)?;
                    taken.get_mut("route").unwrap().insert(to);
                }
            }
        }
    }

    // Alignment points and structures: the same id within the tolerance,
    // or another id at the same place. The same id further away is
    // another object (kept apart under an id of its own); a structure in
    // both is kept once, as the project's, its sets settled one by one.
    let mut settled: BTreeMap<(&'static str, String), Resolution> = BTreeMap::new();
    // Objects of the same id elsewhere kept apart, by kind.
    let mut apart: BTreeMap<&'static str, usize> = BTreeMap::new();
    // Imported structures kept apart (their new id → the imported id).
    let mut far: BTreeMap<String, String> = BTreeMap::new();
    for (kind, word) in [
        (Kind::AlignmentPoint, "alignment point"),
        (Kind::Structure, "structure"),
    ] {
        let theirs: Vec<(String, [f64; 2], Value)> = match kind {
            Kind::AlignmentPoint => scratch
                .line()
                .alignment_points
                .iter()
                .map(|p| (p.id.clone(), [p.position[0], p.position[1]], json(p)))
                .collect(),
            _ => scratch
                .line()
                .structures
                .iter()
                .map(|s| (s.id.clone(), [s.position[0], s.position[1]], json(s)))
                .collect(),
        };
        let ours: Vec<(String, [f64; 2], Value)> = match kind {
            Kind::AlignmentPoint => existing
                .alignment_points
                .iter()
                .map(|p| (p.id.clone(), [p.position[0], p.position[1]], json(p)))
                .collect(),
            _ => existing
                .structures
                .iter()
                .map(|s| (s.id.clone(), [s.position[0], s.position[1]], json(s)))
                .collect(),
        };
        let ours_ids: BTreeSet<String> = ours.iter().map(|o| o.0.clone()).collect();
        let mut used: BTreeSet<String> = BTreeSet::new();
        for (id, at, value) in theirs {
            let d = |o: &(String, [f64; 2], Value)| (o.1[0] - at[0]).hypot(o.1[1] - at[1]);
            let same_id = ours.iter().find(|o| o.0 == id);
            let near = || {
                ours.iter()
                    .filter(|o| !used.contains(&o.0))
                    .filter(|o| d(o) < tolerance)
                    .min_by(|a, b| d(a).total_cmp(&d(b)))
            };
            let found = same_id.filter(|o| d(o) <= tolerance).or_else(near);
            let Some((eid, _, evalue)) = found else {
                if let Some(o) = same_id {
                    // The same id elsewhere: another object.
                    let to = free_apart(&id, &ours_ids, &scratch.ids(kind));
                    scratch.rename(kind, word, &id, &to, &mut out)?;
                    if kind == Kind::Structure {
                        out.notes.push(format!(
                            "structure {id} of the import stands {:.1} m from the project's \
                             {id}: a structure of its own, now {to}",
                            d(o)
                        ));
                        far.insert(to, id.clone());
                    } else {
                        *apart.entry(word).or_default() += 1;
                    }
                }
                *out.added.entry(word).or_default() += 1;
                continue;
            };
            used.insert(eid.clone());
            if kind == Kind::Structure {
                let (ours_now, theirs_now) = (
                    ours_in_effect.get(eid).map(json),
                    theirs_in_effect.get(&id).map(json),
                );
                if *eid == id && *evalue == value && ours_now == theirs_now {
                    *out.identical.entry(word).or_default() += 1;
                    settled.insert((word, id.clone()), Resolution::KeepExisting);
                    continue;
                }
                if let (Some(e), Some(i)) = (ours_in_effect.get(eid), theirs_in_effect.get(&id)) {
                    out.shared.push(towers::compare(e, i, tolerance, |k| {
                        options.sets.get(k).copied()
                    }));
                }
                if *eid != id {
                    scratch.rename(kind, word, &id, eid, &mut out)?;
                }
                settled.insert((word, eid.clone()), Resolution::KeepExisting);
                continue;
            }
            if *eid == id && *evalue == value {
                *out.identical.entry(word).or_default() += 1;
                settled.insert((word, id.clone()), Resolution::KeepExisting);
                continue;
            }
            // At the same place, alike but for its id: the project's.
            if differences(evalue, &value).is_empty() {
                *out.identical.entry(word).or_default() += 1;
                scratch.rename(kind, word, &id, eid, &mut out)?;
                settled.insert((word, eid.clone()), Resolution::KeepExisting);
                continue;
            }
            let key = format!("{}:{id}", kind.as_str());
            let resolution = choose(&key);
            out.duplicates.push(Duplicate {
                key,
                kind: word,
                existing: eid.clone(),
                imported: id.clone(),
                twice: if *eid == id {
                    Twice::SameId
                } else {
                    Twice::SamePlace
                },
                differences: differences(evalue, &value),
                resolution,
            });
            let to = match resolution {
                Resolution::KeepExisting | Resolution::TakeImported => eid.clone(),
                Resolution::KeepBoth => {
                    if ours_ids.contains(&id) {
                        free_id(&id, &ours_ids, &scratch.ids(kind))
                    } else {
                        id.clone()
                    }
                }
            };
            if to != id {
                scratch.rename(kind, word, &id, &to, &mut out)?;
            }
            settled.insert((word, to), resolution);
        }
    }

    // What the import's line takes on before its sections are compared:
    // the structure types of its own given new ids, the sets of towers in
    // both given new numbers or left out, the cables given new ids.
    let mut line = scratch.line().clone();
    let mut handled: BTreeSet<(String, String)> = BTreeSet::new();
    let shared = out.shared.clone();
    let types = structure_types(
        existing_libs,
        imported,
        &shared,
        &far,
        &choose,
        &mut out,
        &mut handled,
    );
    for s in &mut line.structures {
        if let Some(t) = s.structure_type.as_ref().and_then(|t| types.get(t)) {
            s.structure_type = Some(t.clone());
        }
    }
    let mut gone: BTreeSet<(String, u32)> = BTreeSet::new();
    let mut value = json(&line);
    for t in &out.shared {
        for s in &t.sets {
            match (s.choice, s.new_set) {
                (Some(towers::SetChoice::NewNumber), Some(n)) => {
                    towers::renumber(&mut value, &t.existing, s.set, n);
                }
                (Some(towers::SetChoice::LeaveOut), _) => {
                    gone.insert((t.existing.clone(), s.set));
                }
                _ => {}
            }
        }
    }
    let cables = cables(existing_libs, imported, &choose, &mut out, &mut handled);
    rename_cables(&mut value, &cables);
    line = serde_json::from_value(value).map_err(|e| format!("the import's line: {e}"))?;
    // The imported tower's own wire points renumbered too, so the
    // import's line checks until it is merged.
    for t in &out.shared {
        let Some(st) = line.structures.iter_mut().find(|s| s.id == t.existing) else {
            continue;
        };
        for s in &t.sets {
            if let (Some(towers::SetChoice::NewNumber), Some(n)) = (s.choice, s.new_set) {
                for w in st.wires.iter_mut().filter(|w| w.set == Some(s.set)) {
                    w.set = Some(n);
                }
            }
        }
    }
    let left = towers::leave_out(&mut line, &gone);
    if !left.is_empty() {
        out.notes.push(format!(
            "left out with the sets left out: {}",
            left.join(", ")
        ));
    }
    if line != *scratch.line() {
        scratch.replace(line)?;
    }

    // Systems, alignments, sections, jumpers, spacers, circuits: the same
    // id, or (sections, jumpers) the same place once the structures are
    // matched. A section, jumper or alignment of the same id elsewhere
    // (other structures, another route) is another object, kept apart.
    for (kind, word) in [
        (None, "system"),
        (Some(Kind::Alignment), "alignment"),
        (Some(Kind::Section), "section"),
        (Some(Kind::Jumper), "jumper"),
        (Some(Kind::Spacer), "spacer"),
        (None, "circuit"),
    ] {
        let theirs = objects(scratch.line(), word);
        let ours = objects(existing, word);
        let ours_ids: BTreeSet<String> = ours.iter().map(|o| o.0.clone()).collect();
        let place = |v: &Value| -> Option<Value> {
            match word {
                "section" => v.get("points").cloned(),
                "jumper" => Some(Value::Array(vec![
                    v.get("from").cloned().unwrap_or(Value::Null),
                    v.get("to").cloned().unwrap_or(Value::Null),
                ])),
                _ => None,
            }
        };
        // Where it is, to tell one of the same id elsewhere.
        let whereabouts = |v: &Value| -> Option<Value> {
            match word {
                "alignment" => v.get("route").cloned(),
                _ => place(v),
            }
        };
        for (id, value) in theirs {
            // An alignment of a route said to be the project's: by id.
            let same_id = ours.iter().find(|o| o.0 == id);
            let elsewhere = same_id.is_some_and(|o| {
                whereabouts(&value).is_some() && whereabouts(&o.1) != whereabouts(&value)
            });
            let same_place = || {
                let p = place(&value)?;
                ours.iter().find(|o| place(&o.1).as_ref() == Some(&p))
            };
            let found = if elsewhere {
                same_place()
            } else {
                same_id.or_else(same_place)
            };
            let Some((eid, evalue)) = found else {
                if elsewhere {
                    let to = free_apart(&id, &ours_ids, &ids_of(scratch.line(), word));
                    if let Some(k) = kind {
                        scratch.rename(k, word, &id, &to, &mut out)?;
                        *apart.entry(word).or_default() += 1;
                    }
                }
                *out.added.entry(word).or_default() += 1;
                continue;
            };
            if *eid == id && *evalue == value {
                *out.identical.entry(word).or_default() += 1;
                settled.insert((word, id.clone()), Resolution::KeepExisting);
                continue;
            }
            let key = format!("{word}:{id}");
            let resolution = choose(&key);
            out.duplicates.push(Duplicate {
                key,
                kind: word,
                existing: eid.clone(),
                imported: id.clone(),
                twice: if *eid == id {
                    Twice::SameId
                } else {
                    Twice::SamePlace
                },
                differences: differences(evalue, &value),
                resolution,
            });
            let to = match resolution {
                Resolution::KeepExisting | Resolution::TakeImported => eid.clone(),
                Resolution::KeepBoth if ours_ids.contains(&id) => {
                    free_id(&id, &ours_ids, &ids_of(scratch.line(), word))
                }
                Resolution::KeepBoth => id.clone(),
            };
            if to != id {
                match (kind, word) {
                    (Some(k), _) => scratch.rename(k, word, &id, &to, &mut out)?,
                    (None, "system") => scratch.rename_system(&id, &to, &mut out)?,
                    _ => scratch.rename_circuit(&id, &to, &mut out)?,
                }
            }
            settled.insert((word, to), resolution);
        }
    }
    for (word, n) in &apart {
        out.notes.push(format!(
            "{n} {word}{} of the import of an id the project gives to another elsewhere: \
             kept apart under ids of their own",
            if *n == 1 { "" } else { "s" }
        ));
    }

    // The merged line: the project's, the imported objects added or
    // replacing as settled; the project's settings kept.
    let theirs = scratch.line().clone();
    let mut line = existing.clone();
    let keep = |word: &'static str, id: &str| -> bool {
        // Added, or settled by taking the imported one or keeping both.
        !matches!(
            settled.get(&(word, id.to_owned())),
            Some(Resolution::KeepExisting)
        )
    };
    merge_into(
        &mut line.routes,
        theirs.routes,
        |r| &r.id,
        |r| !route_to_existing.contains(&r.id),
    );
    merge_into(
        &mut line.alignment_points,
        theirs.alignment_points,
        |p| &p.id,
        |p| keep("alignment point", &p.id),
    );
    merge_into(
        &mut line.alignments,
        theirs.alignments,
        |a| &a.id,
        |a| keep("alignment", &a.id),
    );
    merge_into(
        &mut line.structures,
        theirs.structures,
        |s| &s.id,
        |s| keep("structure", &s.id),
    );
    merge_into(
        &mut line.systems,
        theirs.systems,
        |s| &s.id,
        |s| keep("system", &s.id),
    );
    merge_into(
        &mut line.sections,
        theirs.sections,
        |s| &s.id,
        |s| keep("section", &s.id),
    );
    merge_into(
        &mut line.jumpers,
        theirs.jumpers,
        |j| &j.id,
        |j| keep("jumper", &j.id),
    );
    merge_into(
        &mut line.spacers,
        theirs.spacers,
        |s| &s.id,
        |s| keep("spacer", &s.id),
    );
    merge_into(
        &mut line.circuits,
        theirs.circuits,
        |c| &c.id,
        |c| keep("circuit", &c.id),
    );
    // The towers in both: the project's, with the imported sets added as
    // chosen.
    for t in &out.shared {
        let (Some(ours), Some(theirs)) = (
            ours_in_effect.get(&t.existing),
            theirs_in_effect.get(&t.imported),
        ) else {
            continue;
        };
        if let Some(s) = line.structures.iter_mut().find(|s| s.id == t.existing) {
            let notes = towers::apply(t, s, ours, theirs);
            out.notes.extend(notes);
        }
    }
    // An alignment of a route said to be the project's that adds points
    // to it: kept as the import gives it (a route may hold several).
    out.line = line;

    // Library files: by kind and name (wire and structure type files
    // above, by what they hold).
    for (kind, name, text) in &imported.libraries {
        if handled.contains(&(kind.clone(), name.clone())) {
            continue;
        }
        match existing_libs
            .iter()
            .find(|(k, n, _)| k == kind && n == name)
        {
            None => {
                *out.added.entry("library file").or_default() += 1;
                out.libraries
                    .push((kind.clone(), name.clone(), text.clone()));
            }
            Some((_, _, t)) if t == text => {
                *out.identical.entry("library file").or_default() += 1;
            }
            Some(_) => {
                let key = format!("library:{kind}/{name}");
                let resolution = choose(&key);
                out.duplicates.push(Duplicate {
                    key,
                    kind: "library file",
                    existing: format!("{kind}/{name}"),
                    imported: format!("{kind}/{name}"),
                    twice: Twice::SameId,
                    differences: vec!["text".into()],
                    resolution,
                });
                match resolution {
                    Resolution::KeepExisting => {}
                    Resolution::TakeImported => {
                        out.libraries
                            .push((kind.clone(), name.clone(), text.clone()));
                    }
                    Resolution::KeepBoth => {
                        let names: BTreeSet<String> = existing_libs
                            .iter()
                            .filter(|(k, ..)| k == kind)
                            .map(|(_, n, _)| n.clone())
                            .collect();
                        let to = free_file_name(name, &names);
                        out.renamed.push(("library file", name.clone(), to.clone()));
                        out.libraries.push((kind.clone(), to, text.clone()));
                        out.notes.push(format!(
                            "library {kind}/{name}: the imported file kept as another file; \
                             what the import refers to by that name takes the project's"
                        ));
                    }
                }
            }
        }
    }
    check_libraries(existing_libs, &mut out);
    if let Err(e) = out.line.validate() {
        out.notes
            .push(format!("the merged line does not check yet: {e}"));
    }
    out.notes.extend(imported.notes.iter().cloned());
    Ok(out)
}

/// The project's library files as they are after the import: its own,
/// those of the plan replacing them by kind and name, added.
pub fn libraries_after(
    existing_libs: &[(String, String, String)],
    plan: &ImportPlan,
) -> Vec<(String, String, String)> {
    let mut out: Vec<(String, String, String)> = existing_libs
        .iter()
        .filter(|(k, n, _)| !plan.libraries.iter().any(|(pk, pn, _)| pk == k && pn == n))
        .cloned()
        .collect();
    out.extend(plan.libraries.iter().cloned());
    out
}

/// Notes when the wire library or the structure types would not load
/// after the import (an id twice), a structure's type is in no type file
/// ([`ImportPlan::missing_types`]), or the merged line does not resolve.
fn check_libraries(existing_libs: &[(String, String, String)], out: &mut ImportPlan) {
    let after = libraries_after(existing_libs, out);
    let mut wires = ohl_cables::Library::default();
    for (_, name, text) in after.iter().filter(|(k, ..)| k == "cables") {
        if let Err(e) = wires.add_toml(std::path::Path::new(name), text) {
            out.notes.push(format!(
                "the wire library does not load after the import: {e}"
            ));
            break;
        }
    }
    // Every structure's type in a type file of the project after the
    // import (owner, 2026-10-27), each type id in one file.
    let mut files: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (_, name, text) in after
        .iter()
        .filter(|(k, n, _)| k == "structures" && n.ends_with(".ohls"))
    {
        let id = toml::from_str::<toml::Table>(text)
            .ok()
            .and_then(|t| t.get("id").and_then(|v| v.as_str()).map(str::to_owned));
        if let Some(id) = id {
            files.entry(id).or_default().push(name.clone());
        }
    }
    for (id, names) in files.iter().filter(|(_, n)| n.len() > 1) {
        out.notes.push(format!(
            "structure type {id} in {} files after the import: {}",
            names.len(),
            names.join(", ")
        ));
    }
    out.missing_types = out
        .line
        .structures
        .iter()
        .filter_map(|s| {
            let t = s.structure_type.as_ref()?;
            (!files.contains_key(t)).then(|| (s.id.clone(), t.clone()))
        })
        .collect();
    if !out.missing_types.is_empty() {
        out.notes.push(format!(
            "structure types not in the project after the import: {}",
            out.missing_types
                .iter()
                .map(|(s, t)| format!("{s} ({t})"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let (types, insulators, _) = towers::libraries(&after);
    let typed = out
        .line
        .structures
        .iter()
        .any(|s| s.structure_type.is_some());
    if typed && let Err(e) = out.line.resolve(&types, &insulators) {
        out.notes.push(format!(
            "the merged line does not resolve with the libraries after the import: {e}"
        ));
    }
}

/// The import's structure type files compared with the project's by type
/// id: one used only at towers in both left out (the project's types are
/// kept there); one the project has (by name or id) with another text
/// settled as a duplicate (`library:structures/<name>`) — kept both, the
/// import's under a new type id — and given a new id without asking when
/// a structure of the import kept apart uses it. Returns the type ids
/// given (imported → new); the files done are added to `handled`.
fn structure_types(
    existing_libs: &[(String, String, String)],
    imported: &ImportedProject,
    shared: &[towers::SharedTower],
    far: &BTreeMap<String, String>,
    choose: &dyn Fn(&str) -> Resolution,
    out: &mut ImportPlan,
    handled: &mut BTreeSet<(String, String)>,
) -> BTreeMap<String, String> {
    let (_, _, ours) = towers::libraries(existing_libs);
    let (_, _, theirs) = towers::libraries(&imported.libraries);
    let mut ids: BTreeSet<String> = ours.keys().chain(theirs.keys()).cloned().collect();
    let mut names: BTreeSet<String> = existing_libs
        .iter()
        .chain(&imported.libraries)
        .filter(|(k, ..)| k == "structures")
        .map(|(_, n, _)| n.clone())
        .collect();
    let in_both: BTreeSet<&str> = shared.iter().map(|t| t.imported.as_str()).collect();
    let mut given = BTreeMap::new();
    for (tid, (name, text)) in &theirs {
        handled.insert(("structures".into(), name.clone()));
        let users: Vec<&str> = imported
            .line
            .structures
            .iter()
            .filter(|s| s.structure_type.as_deref() == Some(tid.as_str()))
            .map(|s| s.id.as_str())
            .collect();
        if !users.is_empty() && users.iter().all(|u| in_both.contains(u)) {
            out.notes.push(format!(
                "structure type {tid} ({name}): only at towers in both, where the project's \
                 types are kept; not added"
            ));
            continue;
        }
        let by_name = existing_libs
            .iter()
            .find(|(k, n, _)| k == "structures" && n == name)
            .map(|(_, n, t)| (n.clone(), t.clone()));
        let by_id = ours.get(tid).cloned();
        let other = by_name
            .iter()
            .chain(by_id.iter())
            .find(|(_, t)| t != text)
            .cloned();
        let Some((their_file, _)) = other else {
            if by_name.is_some() || by_id.is_some() {
                *out.identical.entry("library file").or_default() += 1;
            } else {
                *out.added.entry("library file").or_default() += 1;
                out.libraries
                    .push(("structures".into(), name.clone(), text.clone()));
            }
            continue;
        };
        // Not asked: a structure kept apart uses it, or the project has no
        // type of its id (its file of that name holds another): keeping
        // the project's, or writing over that file, would leave a type
        // missing.
        let apart = users.iter().any(|u| far.values().any(|f| f == u));
        let forced = apart || by_id.is_none();
        let key = format!("library:structures/{name}");
        let resolution = if forced {
            Resolution::KeepBoth
        } else {
            let r = choose(&key);
            out.duplicates.push(Duplicate {
                key,
                kind: "library file",
                existing: format!("structures/{their_file}"),
                imported: format!("structures/{name}"),
                twice: Twice::SameId,
                differences: vec!["text".into()],
                resolution: r,
            });
            r
        };
        match resolution {
            Resolution::KeepExisting => {}
            // In the project's file of that type (its id once).
            Resolution::TakeImported => {
                let file = by_id.map_or(name.clone(), |(n, _)| n);
                out.libraries
                    .push(("structures".into(), file, text.clone()));
            }
            Resolution::KeepBoth => {
                // A new id when the project has the type's id; the file
                // under a name of its own.
                let new = if by_id.is_some() {
                    free_apart(tid, &ids, &BTreeSet::new())
                } else {
                    tid.clone()
                };
                let t = if new == *tid {
                    text.clone()
                } else {
                    let Some(t) = entries::with_id(text, &new) else {
                        out.notes.push(format!(
                            "structure type {tid} ({name}): its id could not be changed; not \
                             added"
                        ));
                        continue;
                    };
                    t
                };
                ids.insert(new.clone());
                let wanted = format!("{new}.ohls");
                let file = if names.contains(&wanted) {
                    free_file_name(&wanted, &names)
                } else {
                    wanted
                };
                names.insert(file.clone());
                if new != *tid {
                    out.renamed
                        .push(("structure type", tid.clone(), new.clone()));
                    given.insert(tid.clone(), new.clone());
                }
                if name != &file {
                    out.renamed
                        .push(("library file", name.clone(), file.clone()));
                }
                if forced {
                    out.notes.push(format!(
                        "structure type {tid} ({name}): the project's file or type of that name \
                         is another; the import's added as {new} ({file})"
                    ));
                }
                *out.added.entry("library file").or_default() += 1;
                out.libraries.push(("structures".into(), file, t));
            }
        }
    }
    given
}

/// The import's wire files compared with the project's cable by cable
/// (owner, 2026-10-27: one of each cable in the project): a cable the
/// project has with the same data left out; a new one added into the
/// project's wire file (the first by name when it has several, said in a
/// note); one of the project's ids with other data a duplicate
/// (`cable:<id>`): the project's kept (the imported sections use it), or
/// the imported one under a new id, added likewise. A project without a
/// wire file gets the imported files as they are. Returns the cable ids
/// given (imported → new); the files done are added to `handled` (a file
/// that does not read is left to the file rules).
fn cables(
    existing_libs: &[(String, String, String)],
    imported: &ImportedProject,
    choose: &dyn Fn(&str) -> Resolution,
    out: &mut ImportPlan,
    handled: &mut BTreeSet<(String, String)>,
) -> BTreeMap<String, String> {
    let files = |libs: &[(String, String, String)]| -> Vec<(String, String)> {
        let mut f: Vec<(String, String)> = libs
            .iter()
            .filter(|(k, ..)| k == "cables")
            .map(|(_, n, t)| (n.clone(), t.clone()))
            .collect();
        f.sort();
        f
    };
    let (our_files, their_files) = (files(existing_libs), files(&imported.libraries));
    let ours = entries::cables_of(&our_files);
    let theirs = entries::cables_of(&their_files);
    let mut ids: BTreeSet<String> = ours.keys().chain(theirs.keys()).cloned().collect();
    let readable: Vec<(&String, String, Vec<entries::Block>)> = their_files
        .iter()
        .filter_map(|(n, t)| entries::cable_blocks(t).map(|(h, b)| (n, h, b)))
        .collect();
    let mut given = BTreeMap::new();
    // The cables added so far (one of each), and the text added to the
    // project's wire file.
    let mut added: BTreeMap<String, toml::Table> = BTreeMap::new();
    let mut into_ours = String::new();
    for (name, head, blocks) in readable {
        handled.insert(("cables".into(), name.clone()));
        let mut body = String::new();
        for b in blocks {
            if let Some(a) = added.get(&b.id) {
                if *a != b.data {
                    out.notes.push(format!(
                        "cable {} ({name}): another of that id came in from another wire file \
                         of the import; left out",
                        b.id
                    ));
                }
                continue;
            }
            let text = match ours.get(&b.id) {
                Some((_, e)) if e.data == b.data => {
                    *out.identical.entry("cable").or_default() += 1;
                    continue;
                }
                Some((file, e)) => {
                    let key = format!("cable:{}", b.id);
                    let resolution = match choose(&key) {
                        Resolution::KeepExisting => Resolution::KeepExisting,
                        _ => Resolution::KeepBoth,
                    };
                    out.duplicates.push(Duplicate {
                        key,
                        kind: "cable",
                        existing: format!("{} ({file})", b.id),
                        imported: format!("{} ({name})", b.id),
                        twice: Twice::SameId,
                        differences: entries::differences(&e.data, &b.data),
                        resolution,
                    });
                    if resolution == Resolution::KeepExisting {
                        continue;
                    }
                    let new = free_apart(&b.id, &ids, &BTreeSet::new());
                    let Some(t) = entries::with_id(&b.text, &new) else {
                        out.notes
                            .push(format!("cable {}: its id could not be changed", b.id));
                        continue;
                    };
                    ids.insert(new.clone());
                    out.renamed.push(("cable", b.id.clone(), new.clone()));
                    given.insert(b.id.clone(), new);
                    t
                }
                None => b.text,
            };
            // A cable the wire library would refuse left out: the
            // project's library keeps loading.
            if let Err(e) = ohl_cables::Library::default()
                .add_toml(std::path::Path::new(name), &format!("{head}{text}"))
            {
                out.notes.push(format!(
                    "cable {} ({name}) left out: it does not load ({e}); the imported \
                     sections on it find no wire",
                    b.id
                ));
                continue;
            }
            added.insert(b.id.clone(), b.data.clone());
            if !ours.contains_key(&b.id) {
                *out.added.entry("cable").or_default() += 1;
            }
            let to = if our_files.is_empty() {
                &mut body
            } else {
                &mut into_ours
            };
            if !to.is_empty() && !to.ends_with("\n\n") {
                to.push('\n');
            }
            to.push_str(&text);
        }
        // No wire file in the project: the imported one as it is.
        if !body.is_empty() {
            *out.added.entry("library file").or_default() += 1;
            out.libraries
                .push(("cables".into(), name.clone(), format!("{head}{body}")));
        }
    }
    if let Some((name, text)) = our_files.first()
        && !into_ours.is_empty()
    {
        if our_files.len() > 1 {
            out.notes.push(format!(
                "the project has {} wire files: the new cables added to the first, {name}",
                our_files.len()
            ));
        }
        let mut t = text.clone();
        if !t.ends_with('\n') {
            t.push('\n');
        }
        if !t.ends_with("\n\n") {
            t.push('\n');
        }
        t.push_str(&into_ours);
        out.libraries.push(("cables".into(), name.clone(), t));
    }
    given
}

/// Every `"cable": <id>` in `v` given as `given` says.
fn rename_cables(v: &mut Value, given: &BTreeMap<String, String>) {
    if given.is_empty() {
        return;
    }
    match v {
        Value::Object(m) => {
            for (k, x) in m.iter_mut() {
                match x {
                    Value::String(s) if k == "cable" => {
                        if let Some(n) = given.get(s.as_str()) {
                            *s = n.clone();
                        }
                    }
                    _ => rename_cables(x, given),
                }
            }
        }
        Value::Array(a) => {
            for x in a {
                rename_cables(x, given);
            }
        }
        _ => {}
    }
}

/// The objects of `line` of kind `word` (by id, as JSON).
fn objects(line: &Line, word: &str) -> Vec<(String, Value)> {
    match word {
        "system" => line
            .systems
            .iter()
            .map(|x| (x.id.clone(), json(x)))
            .collect(),
        "alignment" => line
            .alignments
            .iter()
            .map(|x| (x.id.clone(), json(x)))
            .collect(),
        "section" => line
            .sections
            .iter()
            .map(|x| (x.id.clone(), json(x)))
            .collect(),
        "jumper" => line
            .jumpers
            .iter()
            .map(|x| (x.id.clone(), json(x)))
            .collect(),
        "spacer" => line
            .spacers
            .iter()
            .map(|x| (x.id.clone(), json(x)))
            .collect(),
        "circuit" => line
            .circuits
            .iter()
            .map(|x| (x.id.clone(), json(x)))
            .collect(),
        _ => Vec::new(),
    }
}

fn ids_of(line: &Line, word: &str) -> BTreeSet<String> {
    objects(line, word).into_iter().map(|o| o.0).collect()
}

fn json<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

/// The top-level fields that differ, in words (`id` left out).
fn differences(a: &Value, b: &Value) -> Vec<String> {
    let (Some(a), Some(b)) = (a.as_object(), b.as_object()) else {
        return vec!["value".into()];
    };
    let keys: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
    keys.into_iter()
        .filter(|k| *k != "id" && a.get(*k) != b.get(*k))
        .map(|k| k.replace('_', " "))
        .collect()
}

/// `id (2)`, `id (3)`… the first in neither set.
fn free_id(id: &str, a: &BTreeSet<String>, b: &BTreeSet<String>) -> String {
    (2..)
        .map(|k| format!("{id} ({k})"))
        .find(|c| !a.contains(c) && !b.contains(c))
        .expect("an id is free")
}

/// `id-2`, `id-3`… the first in neither set (an object of the same id
/// elsewhere, kept apart).
fn free_apart(id: &str, a: &BTreeSet<String>, b: &BTreeSet<String>) -> String {
    (2..)
        .map(|k| format!("{id}-{k}"))
        .find(|c| !a.contains(c) && !b.contains(c))
        .expect("an id is free")
}

/// `name (2).ext`… not in `names`.
fn free_file_name(name: &str, names: &BTreeSet<String>) -> String {
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s, format!(".{e}")),
        None => (name, String::new()),
    };
    (2..)
        .map(|k| format!("{stem} ({k}){ext}"))
        .find(|c| !names.contains(c))
        .expect("a name is free")
}

/// `ours` with each of `theirs` that `take` takes added, or replacing the
/// one of the same id (in its place).
fn merge_into<T>(
    ours: &mut Vec<T>,
    theirs: Vec<T>,
    id: impl Fn(&T) -> &String,
    take: impl Fn(&T) -> bool,
) {
    for t in theirs {
        if !take(&t) {
            continue;
        }
        match ours.iter().position(|o| id(o) == id(&t)) {
            Some(k) => ours[k] = t,
            None => ours.push(t),
        }
    }
}

/// The imported line in a session of its own (a file in the temporary
/// folder, removed when done), to rename with the project's commands.
struct Scratch {
    session: Session,
    path: std::path::PathBuf,
}

impl Scratch {
    fn new(line: &Line) -> Result<Self, String> {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ohl-import-{}-{}.{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            ohl_core::files::PROJECT_EXTENSION
        ));
        let _ = std::fs::remove_file(&path);
        let mut session = Session::open(&path).map_err(|e| e.to_string())?;
        session.import(line, USER).map_err(|e| e.to_string())?;
        Ok(Self { session, path })
    }

    fn line(&self) -> &Line {
        self.session.line()
    }

    fn ids(&self, kind: Kind) -> BTreeSet<String> {
        let l = self.line();
        match kind {
            Kind::Route => l.routes.iter().map(|x| x.id.clone()).collect(),
            Kind::AlignmentPoint => l.alignment_points.iter().map(|x| x.id.clone()).collect(),
            Kind::Structure => l.structures.iter().map(|x| x.id.clone()).collect(),
            _ => BTreeSet::new(),
        }
    }

    fn rename(
        &mut self,
        kind: Kind,
        word: &'static str,
        from: &str,
        to: &str,
        out: &mut ImportPlan,
    ) -> Result<(), String> {
        self.session
            .apply(
                USER,
                Command::Rename {
                    kind,
                    id: from.to_owned(),
                    new_id: to.to_owned(),
                },
            )
            .map_err(|e| format!("{word} {from} → {to}: {e}"))?;
        out.renamed.push((word, from.to_owned(), to.to_owned()));
        Ok(())
    }

    fn rename_system(&mut self, from: &str, to: &str, out: &mut ImportPlan) -> Result<(), String> {
        self.session
            .apply(
                USER,
                Command::RenameSystem {
                    id: from.to_owned(),
                    new_id: to.to_owned(),
                },
            )
            .map_err(|e| format!("system {from} → {to}: {e}"))?;
        out.renamed.push(("system", from.to_owned(), to.to_owned()));
        Ok(())
    }

    /// The line replaced by `line` (the same objects, other values).
    fn replace(&mut self, line: Line) -> Result<(), String> {
        self.session
            .apply(
                USER,
                Command::ReplaceLine {
                    line: Box::new(line),
                },
            )
            .map_err(|e| format!("the import's line: {e}"))?;
        Ok(())
    }

    /// A circuit's id changed (nothing else refers to a circuit by id).
    fn rename_circuit(&mut self, from: &str, to: &str, out: &mut ImportPlan) -> Result<(), String> {
        let mut line = self.line().clone();
        if let Some(c) = line.circuits.iter_mut().find(|c| c.id == from) {
            c.id = to.to_owned();
        }
        self.session
            .apply(
                USER,
                Command::ReplaceLine {
                    line: Box::new(line),
                },
            )
            .map_err(|e| format!("circuit {from} → {to}: {e}"))?;
        out.renamed
            .push(("circuit", from.to_owned(), to.to_owned()));
        Ok(())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example() -> Line {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        Line::load(&root.join("examples/synthetic-line/synthetic-line.ohl.json")).unwrap()
    }

    fn imported(line: Line) -> ImportedProject {
        ImportedProject {
            line,
            libraries: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// The same line imported again: everything the same, nothing twice,
    /// the line unchanged.
    #[test]
    fn the_same_line_changes_nothing() {
        let line = example();
        let p = plan(
            &line,
            &[],
            &imported(line.clone()),
            &ImportOptions::default(),
        )
        .unwrap();
        assert!(p.duplicates.is_empty(), "{:?}", p.duplicates);
        assert!(p.added.is_empty(), "{:?}", p.added);
        assert_eq!(p.identical["structure"], line.structures.len());
        assert_eq!(p.line, line);
    }

    /// A structure of another id 0.3 m away is the same structure: kept
    /// once, as the project's (its sets the same: nothing to choose); the
    /// import's sections then refer to it and are the project's.
    #[test]
    fn a_structure_at_the_same_place_is_found() {
        let line = example();
        let id = line.structures[1].id.clone();
        let mut other = line.clone();
        let mut ses = Scratch::new(&other).unwrap();
        let mut sink = plan(
            &line,
            &[],
            &imported(line.clone()),
            &ImportOptions::default(),
        )
        .unwrap();
        ses.rename(Kind::Structure, "structure", &id, "X2", &mut sink)
            .unwrap();
        other = ses.line().clone();
        drop(ses);
        let s = other.structures.iter_mut().find(|s| s.id == "X2").unwrap();
        s.position[0] += 0.3;
        let p = plan(
            &line,
            &[],
            &imported(other.clone()),
            &ImportOptions::default(),
        )
        .unwrap();
        assert!(p.duplicates.is_empty(), "{:?}", p.duplicates);
        assert_eq!(p.shared.len(), 1);
        let t = &p.shared[0];
        assert_eq!(
            (t.existing.as_str(), t.imported.as_str()),
            (id.as_str(), "X2")
        );
        assert!((t.distance - 0.3).abs() < 1e-9);
        assert!(t.sets.iter().all(|s| s.state == SetState::Same));
        assert_eq!(p.line, line);
    }

    /// The same id further away than the tolerance: another structure,
    /// kept apart as "id-2" (a note gives the distance); its sections,
    /// over it, are of ids the project gives to sections elsewhere: kept
    /// apart too, naming the new id.
    #[test]
    fn the_same_id_far_away_is_another_structure() {
        let line = example();
        let id = line.structures[1].id.clone();
        let mut other = line.clone();
        other.structures[1].position[0] += 50.0;
        let p = plan(&line, &[], &imported(other), &ImportOptions::default()).unwrap();
        assert!(p.shared.iter().all(|t| t.existing != id));
        assert!(
            p.duplicates.iter().all(|d| d.kind != "structure"),
            "{:?}",
            p.duplicates
        );
        let new = format!("{id}-2");
        assert!(p.line.structures.iter().any(|s| s.id == new));
        assert!(p.renamed.contains(&("structure", id.clone(), new.clone())));
        assert!(
            p.notes.iter().any(|n| n.contains("50.0 m")),
            "{:?}",
            p.notes
        );
        assert!(
            p.line
                .sections
                .iter()
                .any(|s| s.id.ends_with("-2") && s.points.iter().any(|q| q.structure == new))
        );
        // The branch's section, not over it, is the same.
        assert_eq!(p.line.sections.len(), line.sections.len() + 2);
    }

    /// The example line split at its junction tower T3: the project, the
    /// main route with T3 as a stub (its sets 1 and 11, not the branch's
    /// set 2); the import, the branch with T3 in full, turned half round
    /// (its strings turned with it: the same in the world), its section
    /// of an id the project gives to another ("S1"), its jumper from set 2
    /// to set 1.
    fn junction() -> (Line, Line) {
        let full = example();
        let mut ours = full.clone();
        ours.routes.retain(|r| r.id == "main");
        ours.alignments.retain(|a| a.route == "main");
        let main: Vec<String> = ours.alignments[0].points.clone();
        ours.alignment_points.retain(|p| main.contains(&p.id));
        ours.structures
            .retain(|s| !["T6", "T7"].contains(&s.id.as_str()));
        ours.sections.retain(|s| s.id != "S3");
        ours.jumpers.clear();
        let t3 = ours.structures.iter_mut().find(|s| s.id == "T3").unwrap();
        t3.wires.retain(|w| w.set != Some(2));
        t3.strings.retain(|s| s.wire != "W2");
        t3.points.retain(|p| p.label != "A2");
        let mut theirs = full;
        theirs.routes.retain(|r| r.id == "branch");
        theirs.alignments.retain(|a| a.route == "branch");
        let branch: Vec<String> = theirs.alignments[0].points.clone();
        theirs.alignment_points.retain(|p| branch.contains(&p.id));
        theirs
            .structures
            .retain(|s| ["T3", "T6", "T7"].contains(&s.id.as_str()));
        theirs.sections.retain(|s| s.id == "S3");
        theirs.sections[0].id = "S1".into();
        let t3 = theirs.structures.iter_mut().find(|s| s.id == "T3").unwrap();
        t3.orientation_deg += 180.0;
        for s in &mut t3.strings {
            s.direction_deg = (s.direction_deg + 180.0).rem_euclid(360.0);
        }
        ours.validate().unwrap();
        theirs.validate().unwrap();
        (ours, theirs)
    }

    /// The line placed and solved in still air (the example's cables).
    fn solves(line: &Line) {
        let line = line
            .resolve(&Default::default(), &Default::default())
            .unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let lib = ohl_cables::Library::load(&root.join("libraries/cables")).unwrap();
        let case = ohl_weather::WeatherCase::still(
            "15C",
            ohl_core::units::Temperature::from_celsius(15.0),
        );
        for r in ohl_line::solve::solve_line(&line, &lib, &case) {
            r.unwrap();
        }
    }

    /// The junction tower in both: kept once as the project's (its
    /// orientation), sets 1 and 11 the same, the branch's set 2 only in
    /// the import and added by default — its wire point, string and
    /// structure point as the tower's own, the string's direction turned
    /// into the project's axes, the wire point where the import has it.
    /// The branch's section, of an id the project gives to another, comes
    /// in apart (S1-2) on the added set; the line checks and solves. Left
    /// out, the set goes with the section and the jumper at it.
    #[test]
    fn a_set_only_in_the_import_is_added() {
        let (ours, theirs) = junction();
        let p = plan(
            &ours,
            &[],
            &imported(theirs.clone()),
            &ImportOptions::default(),
        )
        .unwrap();
        assert!(p.duplicates.is_empty(), "{:?}", p.duplicates);
        assert_eq!(p.shared.len(), 1, "{:?}", p.shared);
        let t = &p.shared[0];
        assert_eq!(t.existing, "T3");
        let state = |n: u32| t.sets.iter().find(|s| s.set == n).unwrap();
        assert_eq!(state(1).state, SetState::Same);
        assert_eq!(state(11).state, SetState::Same);
        assert_eq!(state(2).state, SetState::OnlyImported);
        assert_eq!(state(2).choice, Some(SetChoice::Add));
        assert_eq!(state(2).key, "tower:T3:set:2");
        let t3 = p.line.structure("T3").unwrap();
        let o = ours.structure("T3").unwrap().orientation_deg;
        assert_eq!(t3.orientation_deg, o);
        let w = t3.wire_at_set(2).unwrap();
        let s = t3.strings.iter().find(|s| s.wire == w.label).unwrap();
        assert!(
            (s.direction_deg - 274.76).abs() < 1e-6,
            "{}",
            s.direction_deg
        );
        assert!(t3.point(&s.top).is_some());
        let s3 = p.line.sections.iter().find(|s| s.id == "S1-2").unwrap();
        assert_eq!(s3.points[0].structure, "T3");
        assert_eq!(s3.points[0].set, 2);
        assert!(p.line.jumpers.iter().any(|j| j.id == "J-T3-branch"));
        p.line.validate().unwrap();
        let placed = p
            .line
            .resolve(&Default::default(), &Default::default())
            .unwrap();
        let want = theirs
            .structure("T3")
            .unwrap()
            .wire_at_set(2)
            .unwrap()
            .position;
        let got = placed
            .structure("T3")
            .unwrap()
            .wire_at_set(2)
            .unwrap()
            .position;
        assert!(
            (0..3).all(|k| (want[k] - got[k]).abs() < 1e-6),
            "{got:?} {want:?}"
        );
        solves(&p.line);
        assert_eq!(p.line.routes.len(), 2);

        let leave = ImportOptions {
            sets: [("tower:T3:set:2".to_owned(), SetChoice::LeaveOut)].into(),
            ..Default::default()
        };
        let p = plan(&ours, &[], &imported(theirs), &leave).unwrap();
        assert!(p.line.structure("T3").unwrap().wire_at_set(2).is_none());
        assert_eq!(p.line.sections.len(), ours.sections.len());
        assert!(p.line.jumpers.is_empty());
        p.line.validate().unwrap();
    }

    /// A set in both at another place (the import's set 11 three metres
    /// off, its jumper to it): kept as the project's by default (the
    /// jumper hangs on the project's), taken from the import (the
    /// project's set 11 replaced), or added under a new number (12, the
    /// jumper following); each line checks and solves.
    #[test]
    fn a_set_at_another_place_is_settled() {
        let (ours, mut theirs) = junction();
        let t3 = theirs.structures.iter_mut().find(|s| s.id == "T3").unwrap();
        for p in t3.points.iter_mut().filter(|p| p.label == "A11") {
            p.position[0] += 3.0;
        }
        for w in t3.wires.iter_mut().filter(|w| w.label == "W11") {
            w.position[0] += 3.0;
        }
        theirs.jumpers[0].to = ohl_line::JumperEnd::wire("T3", 11);
        let ours_w11 = ours
            .structure("T3")
            .unwrap()
            .wire_at_set(11)
            .unwrap()
            .position;
        let at = |line: &Line, n: u32| {
            line.resolve(&Default::default(), &Default::default())
                .unwrap()
                .structure("T3")
                .unwrap()
                .wire_at_set(n)
                .map(|w| w.position)
        };
        let with = |c: Option<SetChoice>| {
            let o = ImportOptions {
                sets: c
                    .map(|c| ("tower:T3:set:11".to_owned(), c))
                    .into_iter()
                    .collect(),
                ..Default::default()
            };
            let p = plan(&ours, &[], &imported(theirs.clone()), &o).unwrap();
            p.line.validate().unwrap();
            solves(&p.line);
            p
        };
        let p = with(None);
        let s11 = p.shared[0].sets.iter().find(|s| s.set == 11).unwrap();
        assert_eq!(s11.state, SetState::Differs);
        assert!((s11.distance().unwrap() - 3.0).abs() < 1e-6);
        assert_eq!(s11.choice, Some(SetChoice::KeepExisting));
        assert_eq!(at(&p.line, 11), Some(ours_w11));
        assert_eq!(p.line.jumpers[0].to, ohl_line::JumperEnd::wire("T3", 11));

        let p = with(Some(SetChoice::TakeImported));
        let w = at(&p.line, 11).unwrap();
        assert!((w[0] - ours_w11[0] - 3.0).abs() < 1e-6, "{w:?}");
        assert_eq!(
            p.line
                .structure("T3")
                .unwrap()
                .sets()
                .filter(|n| *n == 11)
                .count(),
            1
        );

        let p = with(Some(SetChoice::NewNumber));
        let s11 = p.shared[0].sets.iter().find(|s| s.set == 11).unwrap();
        assert_eq!(s11.new_set, Some(12));
        assert_eq!(at(&p.line, 11), Some(ours_w11));
        let w = at(&p.line, 12).unwrap();
        assert!((w[0] - ours_w11[0] - 3.0).abs() < 1e-6);
        assert_eq!(p.line.jumpers[0].to, ohl_line::JumperEnd::wire("T3", 12));
    }

    /// A wire file of cables, each a copy of the example's with an id of
    /// its own (and a name, when given).
    fn wires(cables: &[(&str, &str)]) -> String {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let text =
            std::fs::read_to_string(root.join("libraries/cables/acsr-examples.ohlw")).unwrap();
        let (head, blocks) = entries::cable_blocks(&text).unwrap();
        let mut out = head;
        for (id, name) in cables {
            let t = entries::with_id(&blocks[0].text, id).unwrap();
            if name.is_empty() {
                out.push_str(&t);
            } else {
                for l in t.lines() {
                    if l.starts_with("name = ") {
                        out.push_str(&format!("name = \"{name}\""));
                    } else {
                        out.push_str(l);
                    }
                    out.push('\n');
                }
            }
            out.push('\n');
        }
        out
    }

    /// Cables by id: one the project has alike left out, a new one added
    /// into the project's wire file (one of each cable, no second file),
    /// one of the project's ids with other data a duplicate — kept as the
    /// project's (the imported sections use it) or the imported one under
    /// a new id (they follow), added likewise. The wire library loads
    /// after either. A project without a wire file gets the import's as it
    /// is.
    #[test]
    fn cables_are_compared_by_id() {
        let mut line = example();
        line.sections.truncate(1);
        line.sections[0].cable = "y".into();
        line.jumpers.clear();
        line.structures.retain(|s| s.id != "T6" && s.id != "T7");
        line.routes.truncate(1);
        line.alignments.truncate(1);
        let main = line.alignments[0].points.clone();
        line.alignment_points.retain(|p| main.contains(&p.id));
        let ours = vec![(
            "cables".to_owned(),
            "cables.ohlw".to_owned(),
            wires(&[("x", ""), ("y", "")]),
        )];
        let mut theirs = imported(line.clone());
        theirs.line.sections[0].stringing = ohl_line::Stringing::Tension {
            horizontal_tension: 20_000.0,
            temperature: 15.0,
        };
        theirs.libraries = vec![(
            "cables".into(),
            "cables.ohlw".into(),
            wires(&[("x", ""), ("y", "other"), ("z", "")]),
        )];
        let p = plan(&line, &ours, &theirs, &ImportOptions::default()).unwrap();
        let d: Vec<&Duplicate> = p.duplicates.iter().filter(|d| d.kind == "cable").collect();
        assert_eq!(d.len(), 1, "{:?}", p.duplicates);
        assert_eq!(d[0].key, "cable:y");
        assert_eq!(d[0].differences, ["name"]);
        assert_eq!(
            d[0].choices(),
            [Resolution::KeepExisting, Resolution::KeepBoth]
        );
        assert_eq!(p.identical["cable"], 1);
        assert_eq!(p.added["cable"], 1);
        assert_eq!(p.libraries.len(), 1);
        assert_eq!(p.libraries[0].1, "cables.ohlw");
        let load = |p: &ImportPlan| {
            let mut lib = ohl_cables::Library::default();
            for (_, n, t) in libraries_after(&ours, p) {
                lib.add_toml(std::path::Path::new(&n), &t).unwrap();
            }
            lib
        };
        let lib = load(&p);
        assert!(lib.get("z").is_some() && lib.get("x").is_some());
        assert!(
            !p.notes.iter().any(|n| n.contains("wire library")),
            "{:?}",
            p.notes
        );
        // The section (a duplicate of the project's): kept, it is y.
        assert_eq!(p.line.sections[0].cable, "y");

        let both = ImportOptions {
            choices: [
                ("cable:y".to_owned(), Resolution::KeepBoth),
                (
                    format!("section:{}", line.sections[0].id),
                    Resolution::TakeImported,
                ),
            ]
            .into(),
            ..Default::default()
        };
        let p = plan(&line, &ours, &theirs, &both).unwrap();
        assert_eq!(p.libraries.len(), 1);
        assert_eq!(p.libraries[0].1, "cables.ohlw");
        let lib = load(&p);
        assert_eq!(lib.len(), 4);
        assert_eq!(lib.get("y-2").unwrap().name, "other");
        assert!(p.renamed.contains(&("cable", "y".into(), "y-2".into())));
        assert_eq!(p.line.sections[0].cable, "y-2");

        let p = plan(&line, &[], &theirs, &ImportOptions::default()).unwrap();
        assert_eq!(p.libraries, theirs.libraries);
    }

    /// The typed example line and its structure type and insulator files.
    fn typed() -> (Line, Vec<(String, String, String)>) {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let dir = root.join("examples/synthetic-line");
        let line = Line::load(&dir.join("synthetic-line-typed.ohl.json")).unwrap();
        let mut libs = Vec::new();
        for (kind, folder, ext) in [
            ("structures", "Structures", "ohls"),
            ("insulators", "Insulators", "ohli"),
        ] {
            for e in std::fs::read_dir(dir.join(folder)).unwrap() {
                let path = e.unwrap().path();
                if path.extension().is_some_and(|x| x == ext) {
                    let name = path.file_name().unwrap().to_string_lossy().into_owned();
                    libs.push((
                        kind.to_owned(),
                        name,
                        std::fs::read_to_string(&path).unwrap(),
                    ));
                }
            }
        }
        (line, libs)
    }

    /// Typed structures: the import's T1 is 5 km away (another structure,
    /// T1-2) and its type file differs from the project's of that name:
    /// the type comes in under a new id in a file of its own, T1-2 naming
    /// it; the other towers are the same (their type files left alike).
    /// The structure library loads and the merged line resolves.
    #[test]
    fn a_tower_kept_apart_brings_its_type() {
        let (line, libs) = typed();
        let t1 = line.structure("T1").unwrap();
        let tid = t1.structure_type.clone().unwrap();
        let mut theirs = imported(line.clone());
        theirs.line.structures[0].position[0] += 5000.0;
        theirs.libraries = libs.clone();
        let file = format!("{tid}.ohls");
        for l in theirs.libraries.iter_mut().filter(|l| l.1 == file) {
            l.2 = format!("# the import's own\n{}", l.2);
        }
        let p = plan(&line, &libs, &theirs, &ImportOptions::default()).unwrap();
        assert!(
            p.duplicates.iter().all(|d| d.kind != "library file"),
            "{:?}",
            p.duplicates
        );
        let new = format!("{tid}-2");
        let s = p.line.structure("T1-2").unwrap();
        assert_eq!(s.structure_type.as_deref(), Some(new.as_str()));
        assert!(
            p.renamed
                .contains(&("structure type", tid.clone(), new.clone()))
        );
        let (_, n, t) = p.libraries.iter().find(|l| l.0 == "structures").unwrap();
        assert_eq!(n, &format!("{new}.ohls"));
        assert!(t.contains(&format!("id = \"{new}\"")));
        assert!(
            !p.notes.iter().any(|n| n.contains("does not")),
            "{:?}",
            p.notes
        );
        // The project's T1 keeps its type.
        assert_eq!(p.line.structure("T1").unwrap().structure_type, Some(tid));
        assert!(p.missing_types.is_empty(), "{:?}", p.missing_types);
    }

    /// Every structure's type is in a type file of the project after the
    /// import: a new tower whose type file has the name of one of the
    /// project's holding another type brings it under a file name of its
    /// own (its id kept, nothing asked); a tower of a type no file holds
    /// is said before importing ([`ImportPlan::missing_types`]).
    #[test]
    fn every_type_is_in_the_project_after() {
        let (line, libs) = typed();
        let tid = line.structures[0].structure_type.clone().unwrap();
        let file = format!("{tid}.ohls");
        let mut theirs = imported(line.clone());
        let mut t8 = theirs.line.structures[0].clone();
        t8.id = "T8".into();
        t8.position[0] += 5000.0;
        t8.structure_type = Some("other-type".into());
        let mut t9 = t8.clone();
        t9.id = "T9".into();
        t9.position[0] += 1000.0;
        t9.structure_type = Some("nowhere".into());
        theirs.line.structures.extend([t8, t9]);
        theirs.libraries = libs.clone();
        for l in theirs.libraries.iter_mut().filter(|l| l.1 == file) {
            l.2 = entries::with_id(&l.2, "other-type").unwrap();
        }
        let p = plan(&line, &libs, &theirs, &ImportOptions::default()).unwrap();
        assert!(
            p.duplicates.iter().all(|d| d.kind != "library file"),
            "{:?}",
            p.duplicates
        );
        let (_, n, t) = p.libraries.iter().find(|l| l.0 == "structures").unwrap();
        assert_eq!(n, "other-type.ohls");
        assert!(t.contains("id = \"other-type\""));
        assert_eq!(
            p.line.structure("T8").unwrap().structure_type.as_deref(),
            Some("other-type")
        );
        assert_eq!(p.missing_types, [("T9".to_owned(), "nowhere".to_owned())]);
        assert!(
            p.notes.iter().any(|n| n.contains("T9 (nowhere)")),
            "{:?}",
            p.notes
        );
        // Without T9 the merged line resolves with the files after it.
        theirs.line.structures.retain(|s| s.id != "T9");
        let p = plan(&line, &libs, &theirs, &ImportOptions::default()).unwrap();
        assert!(p.missing_types.is_empty());
        assert!(
            !p.notes.iter().any(|n| n.contains("does not")),
            "{:?}",
            p.notes
        );
    }

    /// Another line far away: all added; its routes said to be new
    /// (renamed, their ids being taken), or left to the default — the
    /// project's route of the same id (its alignments join it).
    #[test]
    fn another_line_is_added_and_its_route_chosen() {
        let line = example();
        let mut ses = Scratch::new(&line).unwrap();
        let mut sink = plan(
            &line,
            &[],
            &imported(line.clone()),
            &ImportOptions::default(),
        )
        .unwrap();
        let ids: Vec<(Kind, &str, String)> = line
            .structures
            .iter()
            .map(|s| (Kind::Structure, "structure", s.id.clone()))
            .chain(
                line.sections
                    .iter()
                    .map(|s| (Kind::Section, "section", s.id.clone())),
            )
            .chain(
                line.jumpers
                    .iter()
                    .map(|s| (Kind::Jumper, "jumper", s.id.clone())),
            )
            .chain(
                line.alignment_points
                    .iter()
                    .map(|s| (Kind::AlignmentPoint, "alignment point", s.id.clone())),
            )
            .chain(
                line.alignments
                    .iter()
                    .map(|s| (Kind::Alignment, "alignment", s.id.clone())),
            )
            .collect();
        for (k, w, id) in ids {
            ses.rename(k, w, &id, &format!("B-{id}"), &mut sink)
                .unwrap();
        }
        let mut other = ses.line().clone();
        drop(ses);
        for s in &mut other.structures {
            s.position[0] += 5000.0;
        }
        for p in &mut other.alignment_points {
            p.position[0] += 5000.0;
        }
        other.spacers.clear();
        other.circuits.clear();
        // Its routes said to be new: added under ids of their own.
        let new = ImportOptions {
            routes: other.routes.iter().map(|r| (r.id.clone(), None)).collect(),
            ..Default::default()
        };
        let p = plan(&line, &[], &imported(other.clone()), &new).unwrap();
        assert!(
            p.duplicates.iter().all(|d| d.kind == "system"),
            "{:?}",
            p.duplicates
        );
        assert_eq!(p.line.structures.len(), 2 * line.structures.len());
        assert_eq!(p.line.routes.len(), line.routes.len() * 2);
        let p = plan(&line, &[], &imported(other), &ImportOptions::default()).unwrap();
        assert_eq!(p.line.routes.len(), line.routes.len());
        assert!(
            p.line
                .alignments
                .iter()
                .filter(|a| a.route == line.routes[0].id)
                .count()
                > line
                    .alignments
                    .iter()
                    .filter(|a| a.route == line.routes[0].id)
                    .count()
        );
    }

    /// Library files: the same text left, another text a duplicate (kept
    /// both: under "name (2).ext"); another coordinate system refused.
    #[test]
    fn library_files_and_coordinate_systems() {
        let line = example();
        let ours = vec![("cables".to_owned(), "w.ohlw".to_owned(), "a".to_owned())];
        let mut theirs = imported(line.clone());
        theirs.libraries = vec![
            ("cables".into(), "w.ohlw".into(), "b".into()),
            ("weather".into(), "c.ohlwc".into(), "c".into()),
        ];
        let both = ImportOptions {
            choices: [("library:cables/w.ohlw".to_owned(), Resolution::KeepBoth)].into(),
            ..Default::default()
        };
        let p = plan(&line, &ours, &theirs, &both).unwrap();
        assert_eq!(p.duplicates.len(), 1);
        let names: Vec<&str> = p.libraries.iter().map(|l| l.1.as_str()).collect();
        assert_eq!(names, ["w (2).ohlw", "c.ohlwc"]);
        let mut far = line.clone();
        far.crs = "EPSG:2056".into();
        let mut near = line;
        near.crs = "EPSG:21781".into();
        assert!(plan(&near, &[], &imported(far), &ImportOptions::default()).is_err());
    }
}
