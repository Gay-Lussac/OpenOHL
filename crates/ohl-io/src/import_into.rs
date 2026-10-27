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
//! - **Two modes** ([`ImportMode`], owner 2026-10-27). *Replace* (the
//!   default): the import is a variant of what it covers — what appears
//!   in both is settled by taking the imported one (a tower in both is
//!   the imported tower, the project's sets it lacks kept where what
//!   stays still hangs on them; a tower of another id at the same place
//!   takes the imported id, what refers to it following), and the
//!   project's structures, sections and jumpers in the stretch the
//!   import covers that it does not hold are removed (the stretch: see
//!   the `stretch` module; each removal listed as a [`Removed`], to keep
//!   instead). An imported route not given is the project's route of
//!   its id, else the one whose alignment points it meets; an imported
//!   alignment on one of the project's alignments (two points or more in
//!   common) replaces the part of it between the first and last of them.
//!   *Add*: what is in both kept as the project's, nothing removed.
//!
//! The result is the merged line and the library files to write
//! ([`ImportPlan`]); the caller writes it as one change (the session's
//! `ReplaceLine`).

use std::collections::{BTreeMap, BTreeSet};

use ohl_line::Line;
use ohl_session::{Command, Kind, Session};
use serde_json::Value;

use crate::projects::ImportedProject;

pub(crate) mod entries;
mod stretch;
mod towers;

pub use stretch::Removed;
pub use towers::{SetChoice, SetState, SharedSet, SharedTower};

/// How the import is done (owner, 2026-10-27).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ImportMode {
    /// A variant of what the import covers: what is in both taken from
    /// the import, what the project has in the stretch the import covers
    /// and the import has not removed.
    #[default]
    Replace,
    /// Added to the project: what is in both kept as the project's,
    /// nothing removed.
    Add,
}

impl ImportMode {
    /// In words, for the window.
    pub fn words(self) -> &'static str {
        match self {
            Self::Replace => "Replace what the import covers (a variant)",
            Self::Add => "Add to the project (keep the project's)",
        }
    }

    /// One sentence on what it does.
    pub fn sentence(self) -> &'static str {
        match self {
            Self::Replace => {
                "What is in both becomes the imported one, and what the project has between the \
                 import's structures but the import has not (structures, sections, jumpers) is \
                 removed; the line outside that stretch stays."
            }
            Self::Add => {
                "The import's objects are added; what is in both stays the project's (another \
                 choice per object), nothing is removed."
            }
        }
    }

    /// The word of the command line (`--mode`).
    pub fn word(self) -> &'static str {
        match self {
            Self::Replace => "replace",
            Self::Add => "add",
        }
    }

    /// Both, in the window's order.
    pub const ALL: [Self; 2] = [Self::Replace, Self::Add];
}

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
    /// The resolutions it has, in the window's order (a cable taken from
    /// the import replaces the project's under its id, in the project's
    /// wire file; kept both, the imported one gets a new id).
    pub fn choices(&self) -> &'static [Resolution] {
        &Resolution::ALL
    }
}

/// What the import does.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportPlan {
    /// The mode it was made in.
    pub mode: ImportMode,
    /// What each imported route became: the project's route, or none (a
    /// new route).
    pub routes: BTreeMap<String, Option<String>>,
    /// The project's objects the import removes (Replace), each with the
    /// choice to keep it.
    pub removed: Vec<Removed>,
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
    /// Imported objects given another id: kind, from, to; the project's
    /// structures given the id of the imported tower at their place
    /// (Replace) as kind `structure (the project's)`.
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
    /// Replace (the default) or add.
    pub mode: ImportMode,
    /// Per imported route (its id), the project's route it is (none: a
    /// new route); a route not given is the project's route of the same
    /// id, else new.
    pub routes: BTreeMap<String, Option<String>>,
    /// Alignment points and structures nearer than this in plan are the
    /// same place, m (0: [`DEFAULT_TOLERANCE`]).
    pub tolerance: f64,
    /// The choices, by [`Duplicate::key`] and [`SharedTower::key`] (a
    /// tower in both: the project's kept, or the imported one taken).
    /// Not given: the project's (Add) or the imported one (Replace).
    pub choices: BTreeMap<String, Resolution>,
    /// The removals chosen to keep, by [`Removed::key`].
    pub kept: BTreeSet<String>,
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
    let replace = options.mode == ImportMode::Replace;
    let mut out = ImportPlan {
        mode: options.mode,
        routes: BTreeMap::new(),
        removed: Vec::new(),
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
    // Not chosen: the project's (Add), the imported one (Replace).
    let default = if replace {
        Resolution::TakeImported
    } else {
        Resolution::KeepExisting
    };
    let choose = |key: &str| options.choices.get(key).copied().unwrap_or(default);
    // Replace: the stretch the import covers cleared of what the import
    // has not, the project's towers at the import's places under the
    // import's ids.
    let ours = if replace {
        variant(existing, imported, tolerance, options, &choose, &mut out)?
    } else {
        existing.clone()
    };
    let existing = &ours;
    let mut scratch = Scratch::new(&imported.line)?;
    // Ids taken in the project and given out so far.
    let mut taken: BTreeMap<&'static str, BTreeSet<String>> = BTreeMap::new();
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
        // Not said: the project's route of the same id, if any; in
        // Replace mode else the one whose alignment points it meets.
        let said = match options.routes.get(&r.id) {
            Some(x) => x.clone(),
            None => existing
                .routes
                .iter()
                .any(|x| x.id == r.id)
                .then(|| r.id.clone())
                .or_else(|| {
                    replace
                        .then(|| route_met(existing, scratch.line(), &r.id, tolerance))
                        .flatten()
                }),
        };
        out.routes.insert(
            r.id.clone(),
            said.clone()
                .filter(|e| existing.routes.iter().any(|x| x.id == *e)),
        );
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
                    let mut t = towers::compare(e, i, tolerance, |k| options.sets.get(k).copied());
                    // The project's tower kept (its sets settled one by
                    // one), or the imported one taken (as it is).
                    t.resolution = match choose(&t.key) {
                        Resolution::TakeImported => Resolution::TakeImported,
                        _ => Resolution::KeepExisting,
                    };
                    if t.resolution == Resolution::TakeImported {
                        for s in &mut t.sets {
                            s.choice = None;
                            s.new_set = None;
                        }
                    }
                    out.shared.push(t);
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
    // The towers kept as the project's (their types kept).
    let shared: Vec<SharedTower> = out
        .shared
        .iter()
        .filter(|t| t.resolution == Resolution::KeepExisting)
        .cloned()
        .collect();
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
    // Imported alignment → the project's it is spliced into (Replace).
    let mut spliced: Vec<(String, String)> = Vec::new();
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
        // An imported object moved off an id another of the import takes
        // (its original id → where it waits), and the ids settled so far.
        let mut moved: BTreeMap<String, String> = BTreeMap::new();
        let mut done: BTreeSet<String> = BTreeSet::new();
        for (id, value) in theirs {
            // Where it is now (moved aside, or as read).
            let cur = moved.remove(&id).unwrap_or_else(|| id.clone());
            // Replace: an alignment on one of the project's (two points or
            // more in common) replaces the part of it between them.
            if replace
                && word == "alignment"
                && let Some(eid) = splice_target(existing, scratch.line(), &id)
            {
                spliced.push((id.clone(), eid));
                settled.insert((word, id.clone()), Resolution::KeepExisting);
                continue;
            }
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
                if let Some(k) = kind
                    && (elsewhere || cur != id)
                {
                    let to = free_apart(&id, &ours_ids, &ids_of(scratch.line(), word));
                    scratch.rename(k, word, &cur, &to, &mut out)?;
                    if let Some(r) = out.renamed.last_mut() {
                        r.1 = id.clone();
                    }
                    *apart.entry(word).or_default() += 1;
                    done.insert(to);
                } else {
                    done.insert(cur.clone());
                }
                *out.added.entry(word).or_default() += 1;
                continue;
            };
            if *eid == id && *evalue == value {
                *out.identical.entry(word).or_default() += 1;
                settled.insert((word, cur.clone()), Resolution::KeepExisting);
                done.insert(cur);
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
            let mut to = match resolution {
                Resolution::KeepExisting | Resolution::TakeImported => eid.clone(),
                Resolution::KeepBoth if ours_ids.contains(&id) || cur != id => {
                    free_id(&id, &ours_ids, &ids_of(scratch.line(), word))
                }
                Resolution::KeepBoth => id.clone(),
            };
            if let Some(k) = kind
                && to != cur
                && ids_of(scratch.line(), word).contains(&to)
            {
                if done.contains(&to) {
                    // Another of the import is that object already (two of
                    // the import at its place): this one kept apart.
                    let apart_to = free_id(&id, &ours_ids, &ids_of(scratch.line(), word));
                    out.notes.push(format!(
                        "{word} {id} of the import is at the place of {to}, as another of the \
                         import is: kept apart as {apart_to}"
                    ));
                    to = apart_to;
                } else {
                    // One of the import not settled yet holds that id: it
                    // waits under another (owner report 2026-10-27:
                    // "section sec13 exists already").
                    let mut n = 1;
                    let mut wait = format!("{to}~{n}");
                    while ids_of(scratch.line(), word).contains(&wait) {
                        n += 1;
                        wait = format!("{to}~{n}");
                    }
                    scratch.session_rename(k, &to, &wait)?;
                    moved.insert(to.clone(), wait);
                }
            }
            if to != cur {
                match (kind, word) {
                    (Some(k), _) => scratch.rename(k, word, &cur, &to, &mut out)?,
                    (None, "system") => scratch.rename_system(&cur, &to, &mut out)?,
                    _ => scratch.rename_circuit(&cur, &to, &mut out)?,
                }
                // Said as from its id as read.
                if cur != id
                    && let Some(r) = out.renamed.last_mut()
                {
                    r.1 = id.clone();
                }
            }
            done.insert(to.clone());
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
    let (their_structures, their_alignments) =
        (theirs.structures.clone(), theirs.alignments.clone());
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
    // chosen; or the imported one taken as it is.
    for t in &out.shared {
        let (Some(ours), Some(theirs)) = (
            ours_in_effect.get(&t.existing),
            theirs_in_effect.get(&t.imported),
        ) else {
            continue;
        };
        let Some(s) = line.structures.iter_mut().find(|s| s.id == t.existing) else {
            continue;
        };
        if t.resolution == Resolution::TakeImported {
            if let Some(i) = their_structures.iter().find(|x| x.id == t.existing) {
                *s = i.clone();
            }
        } else {
            let notes = towers::apply(t, s, ours, theirs);
            out.notes.extend(notes);
        }
    }
    // An alignment of a route said to be the project's that adds points
    // to it: kept as the import gives it (a route may hold several);
    // in Replace mode one on the project's alignment spliced into it.
    let before: BTreeSet<String> = existing
        .alignment_points
        .iter()
        .map(|p| p.id.clone())
        .collect();
    for (i, e) in &spliced {
        let Some(a) = their_alignments.iter().find(|a| a.id == *i) else {
            continue;
        };
        if let Some(note) = splice(&mut line, e, &a.points, &before) {
            out.notes.push(note);
        }
    }
    // The taken towers keep the project's sets they lack where what stays
    // hangs on them.
    keep_sets(&mut line, &mut out, &ours_in_effect, &theirs_in_effect);
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
    // The import gets everything in, the rules are checked afterwards
    // (owner, 2026-10-27): a merged line that does not check is mended as
    // safe mode mends a plugin's (e.g. a variant labelling the phases of a
    // conductor the project continues otherwise), each mend said.
    // In Replace mode the variant is the new state: along a joined
    // conductor its names win (owner, 2026-10-27).
    let imported_sections: BTreeSet<String> = match options.mode {
        ImportMode::Replace => scratch
            .line()
            .sections
            .iter()
            .map(|s| s.id.clone())
            .collect(),
        ImportMode::Add => BTreeSet::new(),
    };
    crate::projects::mend_preferring(&mut out.line, &mut out.notes, &|id: &str| {
        imported_sections.contains(id)
    });
    if let Err(e) = out.line.validate() {
        out.notes
            .push(format!("the merged line does not check yet: {e}"));
    }
    out.notes.extend(imported.notes.iter().cloned());
    Ok(out)
}

/// Replace mode: the project line with the stretch the import covers
/// cleared of what the import has not (the removals listed in `out`), and
/// the project's towers at the import's places of another id given the
/// imported id (what refers to them following) unless the project has
/// that id or the tower is chosen to stay the project's.
fn variant(
    existing: &Line,
    imported: &ImportedProject,
    tolerance: f64,
    options: &ImportOptions,
    choose: &dyn Fn(&str) -> Resolution,
    out: &mut ImportPlan,
) -> Result<Line, String> {
    let found = stretch::find(existing, &imported.line, tolerance);
    let joined = stretch::joins(&imported.line, &found.matched);
    // The import's sections and jumpers by the project's ids (an imported
    // structure not matched never meets one of the project's).
    let map = |s: &str| {
        found
            .matched
            .get(s)
            .cloned()
            .unwrap_or_else(|| format!("\u{0}{s}"))
    };
    let their_sections: BTreeSet<Vec<(String, u32)>> = imported
        .line
        .sections
        .iter()
        .map(|s| {
            s.points
                .iter()
                .map(|p| (map(&p.structure), p.set))
                .collect()
        })
        .collect();
    let their_jumpers: BTreeSet<Vec<(String, u32)>> = imported
        .line
        .jumpers
        .iter()
        .map(|j| {
            let mut v: Vec<(String, u32)> =
                j.points().map(|p| (map(&p.structure), p.set)).collect();
            v.sort();
            v
        })
        .collect();
    // The import's spans from a structure in both: its project id, the
    // far end's id as imported and where it stands.
    let their_spans: Vec<stretch::TheirSpan> = imported
        .line
        .sections
        .iter()
        .flat_map(|s| s.points.windows(2))
        .flat_map(|w| [(&w[0], &w[1]), (&w[1], &w[0])])
        .filter_map(|(a, b)| {
            let from = found.matched.get(&a.structure)?.clone();
            let far = imported.line.structure(&b.structure)?;
            Some(stretch::TheirSpan {
                from,
                to: b.structure.clone(),
                to_position: far.position,
            })
        })
        .collect();
    out.removed = stretch::removals(
        existing,
        &found,
        &joined,
        &their_sections,
        &their_jumpers,
        &their_spans,
        &options.kept,
    );
    if joined.is_empty() && !found.matched.is_empty() {
        out.notes.push(
            "the import's structures in both are not joined by its sections or alignments: it \
             covers no stretch of the project's line, nothing is removed"
                .into(),
        );
    }
    for r in out
        .removed
        .iter()
        .filter(|r| r.kind == "section" && !r.keep && r.why.contains("crosses"))
    {
        out.notes.push(format!(
            "section {} of the project ran out of the stretch the import covers through a \
             removed structure: split at the stretch's end, {} kept outside it as {}",
            r.id,
            if r.pieces.len() == 1 {
                "its part"
            } else {
                "its parts"
            },
            if r.pieces.len() == 1 {
                "a section of its own (same id)".to_owned()
            } else {
                format!("{} sections (the first under its id)", r.pieces.len())
            }
        ));
    }
    let mut line = existing.clone();
    stretch::remove(&mut line, &out.removed);
    let mut ids: BTreeSet<String> = line.structures.iter().map(|s| s.id.clone()).collect();
    let mut renames = Vec::new();
    for (i, e) in &found.matched {
        if i == e || ids.contains(i) || choose(&format!("tower:{i}")) != Resolution::TakeImported {
            continue;
        }
        ids.remove(e);
        ids.insert(i.clone());
        renames.push((e.clone(), i.clone()));
    }
    if renames.is_empty() {
        return Ok(line);
    }
    let mut s = Scratch::new(&line)?;
    for (e, i) in renames {
        s.session
            .apply(
                USER,
                Command::Rename {
                    kind: Kind::Structure,
                    id: e.clone(),
                    new_id: i.clone(),
                },
            )
            .map_err(|x| format!("the project's structure {e} → {i}: {x}"))?;
        out.renamed.push(("structure (the project's)", e, i));
    }
    Ok(s.line().clone())
}

/// Replace mode: the project's route the imported route `id` meets — the
/// one with the most alignment points within `tolerance` of the imported
/// route's (two at least).
fn route_met(existing: &Line, theirs: &Line, id: &str, tolerance: f64) -> Option<String> {
    let points = |line: &Line, route: &str| -> Vec<[f64; 2]> {
        line.alignments
            .iter()
            .filter(|a| a.route == route)
            .flat_map(|a| &a.points)
            .filter_map(|p| line.alignment_point(p))
            .map(|p| [p.position[0], p.position[1]])
            .collect()
    };
    let theirs = points(theirs, id);
    existing
        .routes
        .iter()
        .map(|r| {
            let ours = points(existing, &r.id);
            let n = theirs
                .iter()
                .filter(|p| {
                    ours.iter()
                        .any(|q| (q[0] - p[0]).hypot(q[1] - p[1]) < tolerance)
                })
                .count();
            (n, r.id.clone())
        })
        .filter(|(n, _)| *n >= 2)
        .max_by_key(|(n, _)| *n)
        .map(|(_, r)| r)
}

/// Replace mode: the project's alignment of the same route the imported
/// alignment `id` has the most points in common with (two at least; its
/// points already given the project's ids where at the same place).
fn splice_target(existing: &Line, theirs: &Line, id: &str) -> Option<String> {
    let a = theirs.alignment_by_id(id)?;
    existing
        .alignments
        .iter()
        .filter(|e| e.route == a.route)
        .map(|e| {
            let n = a.points.iter().filter(|p| e.points.contains(p)).count();
            (n, e.id.clone())
        })
        .filter(|(n, _)| *n >= 2)
        .max_by_key(|(n, _)| *n)
        .map(|(_, e)| e)
}

/// The points of the project's alignment `id` from the first to the last
/// it has in common with the imported points `theirs` replaced by those
/// of the import between them (and the import's beyond them where the
/// project's alignment ends there). Points that then are on no alignment
/// leave the line: the project's taken off, the import's not used
/// (`before`: the project's points before the import). A note when
/// anything changed.
fn splice(
    line: &mut Line,
    id: &str,
    theirs: &[String],
    before: &BTreeSet<String>,
) -> Option<String> {
    let k = line.alignments.iter().position(|a| a.id == id)?;
    let ours = line.alignments[k].points.clone();
    let common = |q: &[String]| -> Vec<(usize, usize)> {
        q.iter()
            .enumerate()
            .filter_map(|(a, p)| ours.iter().position(|x| x == p).map(|i| (a, i)))
            .collect()
    };
    let mut q = theirs.to_vec();
    let mut c = common(&q);
    if c.len() < 2 {
        return None;
    }
    if c[0].1 > c[c.len() - 1].1 {
        q.reverse();
        c = common(&q);
    }
    let ((a0, i0), (a1, i1)) = (c[0], c[c.len() - 1]);
    let mut new: Vec<String> = Vec::new();
    if i0 == 0 {
        new.extend(q[..a0].iter().cloned());
    }
    new.extend(ours[..i0].iter().cloned());
    new.extend(q[a0..=a1].iter().cloned());
    new.extend(ours[i1 + 1..].iter().cloned());
    if i1 + 1 == ours.len() {
        new.extend(q[a1 + 1..].iter().cloned());
    }
    let mut seen = BTreeSet::new();
    new.retain(|p| seen.insert(p.clone()));
    if new == ours {
        return None;
    }
    let dropped: Vec<String> = ours[i0..=i1]
        .iter()
        .filter(|p| !new.contains(p))
        .cloned()
        .collect();
    line.alignments[k].points = new;
    let on: BTreeSet<String> = line
        .alignments
        .iter()
        .flat_map(|a| a.points.iter().cloned())
        .collect();
    line.alignment_points
        .retain(|p| on.contains(&p.id) || (before.contains(&p.id) && !dropped.contains(&p.id)));
    Some(format!(
        "alignment {id}: from {} to {} the import's points ({}){}",
        ours[i0],
        ours[i1],
        a1 - a0 + 1,
        if dropped.is_empty() {
            String::new()
        } else {
            format!("; the project's {} taken off", dropped.join(", "))
        }
    ))
}

/// The towers taken from the import keep the project's sets they lack
/// where what stays in the merged `line` (a section, jumper or circuit
/// start) still hangs on them; each set only the project's says what
/// hangs on it ([`SharedSet::kept_for`], none: it went with the
/// project's tower).
fn keep_sets(
    line: &mut Line,
    out: &mut ImportPlan,
    ours_in_effect: &BTreeMap<String, ohl_line::Structure>,
    theirs_in_effect: &BTreeMap<String, ohl_line::Structure>,
) {
    for k in 0..out.shared.len() {
        let t = &out.shared[k];
        if t.resolution != Resolution::TakeImported {
            continue;
        }
        let (Some(o), Some(i)) = (
            ours_in_effect.get(&t.existing),
            theirs_in_effect.get(&t.imported),
        ) else {
            continue;
        };
        let mut users: BTreeMap<u32, Vec<String>> = BTreeMap::new();
        let mut hang = |set: u32, what: String| {
            let u = users.entry(set).or_default();
            if !u.contains(&what) {
                u.push(what);
            }
        };
        for s in &line.sections {
            for p in s.points.iter().filter(|p| p.structure == t.existing) {
                hang(p.set, format!("section {}", s.id));
            }
        }
        for j in &line.jumpers {
            for p in j.points().filter(|p| p.structure == t.existing) {
                hang(p.set, format!("jumper {}", j.id));
            }
        }
        for p in line
            .circuit_starts
            .iter()
            .filter(|p| p.structure == t.existing)
        {
            hang(p.set, "a circuit start".into());
        }
        let missing: Vec<u32> = users
            .keys()
            .filter(|n| i.wire_at_set(**n).is_none() && o.wire_at_set(**n).is_some())
            .copied()
            .collect();
        let carry = SharedTower {
            sets: t
                .sets
                .iter()
                .filter(|s| missing.contains(&s.set))
                .map(|s| SharedSet {
                    choice: Some(SetChoice::Add),
                    new_set: None,
                    ..s.clone()
                })
                .collect(),
            ..t.clone()
        };
        if let Some(st) = line.structures.iter_mut().find(|s| s.id == carry.existing)
            && !carry.sets.is_empty()
        {
            let notes = towers::apply(&carry, st, i, o);
            out.notes.extend(notes);
        }
        for s in out.shared[k]
            .sets
            .iter_mut()
            .filter(|s| s.state == SetState::OnlyExisting)
        {
            s.kept_for = users.get(&s.set).cloned().unwrap_or_default();
        }
    }
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
                "structure type {tid} ({name}): only at structures in both, where the project's \
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
    // The project's wire files changed (a cable taken from the import).
    let mut edited: BTreeMap<String, String> = BTreeMap::new();
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
                    let resolution = choose(&key);
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
                    if resolution == Resolution::TakeImported {
                        // In the project's file, in the place of its own.
                        let Some(t) = edited.get(file).cloned().or_else(|| {
                            our_files.iter().find(|f| f.0 == *file).map(|f| f.1.clone())
                        }) else {
                            continue;
                        };
                        let mut new = b.text.clone();
                        while e.text.ends_with("\n\n") && !new.ends_with("\n\n") {
                            new.push('\n');
                        }
                        edited.insert(file.clone(), t.replacen(&e.text, &new, 1));
                        added.insert(b.id.clone(), b.data.clone());
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
        let mut t = edited.remove(name).unwrap_or_else(|| text.clone());
        if !t.ends_with('\n') {
            t.push('\n');
        }
        if !t.ends_with("\n\n") {
            t.push('\n');
        }
        t.push_str(&into_ours);
        out.libraries.push(("cables".into(), name.clone(), t));
    }
    for (name, text) in edited {
        out.libraries.push(("cables".into(), name, text));
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

    /// Renames without recording it (an object of the import moved aside
    /// for a moment, renamed again when its turn comes).
    fn session_rename(&mut self, kind: Kind, from: &str, to: &str) -> Result<(), String> {
        self.session
            .apply(
                USER,
                Command::Rename {
                    kind,
                    id: from.to_owned(),
                    new_id: to.to_owned(),
                },
            )
            .map(drop)
            .map_err(|e| format!("{from} → {to}: {e}"))
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

    /// Add mode, nothing chosen.
    fn add() -> ImportOptions {
        ImportOptions {
            mode: ImportMode::Add,
            ..Default::default()
        }
    }

    /// The same line imported again: everything the same, nothing twice,
    /// the line unchanged.
    #[test]
    fn the_same_line_changes_nothing() {
        let line = example();
        let p = plan(&line, &[], &imported(line.clone()), &add()).unwrap();
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
        let mut sink = plan(&line, &[], &imported(line.clone()), &add()).unwrap();
        ses.rename(Kind::Structure, "structure", &id, "X2", &mut sink)
            .unwrap();
        other = ses.line().clone();
        drop(ses);
        let s = other.structures.iter_mut().find(|s| s.id == "X2").unwrap();
        s.position[0] += 0.3;
        let p = plan(&line, &[], &imported(other.clone()), &add()).unwrap();
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
        let p = plan(&line, &[], &imported(other), &add()).unwrap();
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
        let p = plan(&ours, &[], &imported(theirs.clone()), &add()).unwrap();
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
            ..add()
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
                ..add()
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
        let p = plan(&line, &ours, &theirs, &add()).unwrap();
        let d: Vec<&Duplicate> = p.duplicates.iter().filter(|d| d.kind == "cable").collect();
        assert_eq!(d.len(), 1, "{:?}", p.duplicates);
        assert_eq!(d[0].key, "cable:y");
        assert_eq!(d[0].differences, ["name"]);
        assert_eq!(d[0].choices(), Resolution::ALL);
        assert_eq!(d[0].resolution, Resolution::KeepExisting);
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
            ..add()
        };
        let p = plan(&line, &ours, &theirs, &both).unwrap();
        assert_eq!(p.libraries.len(), 1);
        assert_eq!(p.libraries[0].1, "cables.ohlw");
        let lib = load(&p);
        assert_eq!(lib.len(), 4);
        assert_eq!(lib.get("y-2").unwrap().name, "other");
        assert!(p.renamed.contains(&("cable", "y".into(), "y-2".into())));
        assert_eq!(p.line.sections[0].cable, "y-2");

        // Replace: the imported y takes the place of the project's in its
        // file, under its id; one of each cable.
        let p = plan(&line, &ours, &theirs, &ImportOptions::default()).unwrap();
        let d = p.duplicates.iter().find(|d| d.kind == "cable").unwrap();
        assert_eq!(d.resolution, Resolution::TakeImported);
        assert_eq!(p.libraries.len(), 1);
        let lib = load(&p);
        assert_eq!(lib.len(), 3);
        assert_eq!(lib.get("y").unwrap().name, "other");
        assert_eq!(p.line.sections[0].cable, "y");

        let p = plan(&line, &[], &theirs, &add()).unwrap();
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
        let p = plan(&line, &libs, &theirs, &add()).unwrap();
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
        let p = plan(&line, &libs, &theirs, &add()).unwrap();
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
        let p = plan(&line, &libs, &theirs, &add()).unwrap();
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
        let mut sink = plan(&line, &[], &imported(line.clone()), &add()).unwrap();
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
            ..add()
        };
        let p = plan(&line, &[], &imported(other.clone()), &new).unwrap();
        assert!(
            p.duplicates.iter().all(|d| d.kind == "system"),
            "{:?}",
            p.duplicates
        );
        assert_eq!(p.line.structures.len(), 2 * line.structures.len());
        assert_eq!(p.line.routes.len(), line.routes.len() * 2);
        let p = plan(&line, &[], &imported(other), &add()).unwrap();
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

    /// A tower of the synthetic straight line: one string per set (sets
    /// 1 and 21 on the left, 11 and 31 on the right; 21 and 31 half a
    /// metre ahead), world positions, turned across the line.
    fn tower(id: &str, x: f64, y: f64, z: f64, sets: &[u32]) -> Value {
        let mut points = Vec::new();
        let mut strings = Vec::new();
        let mut wires = Vec::new();
        for &n in sets {
            let side = if n % 20 == 1 { -6.0 } else { 6.0 };
            let ahead = if n > 20 { 0.5 } else { 0.0 };
            points.push(serde_json::json!({
                "label": format!("A{n}"), "position": [x + ahead, y + side, z + 30.0]
            }));
            strings.push(serde_json::json!({
                "label": format!("S{n}"), "top": format!("A{n}"), "wire": format!("W{n}"),
                "length": 3.5, "weight": 900.0
            }));
            wires.push(serde_json::json!({
                "label": format!("W{n}"), "position": [x + ahead, y + side, z + 26.5], "set": n
            }));
        }
        serde_json::json!({
            "id": id, "position": [x, y, z], "orientation_deg": -90.0,
            "points": points, "strings": strings, "wires": wires
        })
    }

    /// A section of circuit `c` (A: phase L1, B: L2) over (structure,
    /// set).
    fn section(id: &str, c: &str, points: &[(&str, u32)], tension: f64) -> Value {
        serde_json::json!({
            "id": id, "cable": "acsr-drake-26-7", "circuit_name": c,
            "phase_label": if c == "A" { "L1" } else { "L2" }, "system": "ex220",
            "points": points.iter().map(|(s, n)| serde_json::json!({"structure": s, "set": n}))
                .collect::<Vec<_>>(),
            "stringing": {"kind": "tension", "horizontal_tension": tension, "temperature": 15.0}
        })
    }

    fn jumper(id: &str, at: &str, from: u32, to: u32) -> Value {
        serde_json::json!({
            "id": id, "cable": "acsr-drake-26-7", "sag": 1.5, "temperature": 15.0,
            "from": {"structure": at, "set": from}, "to": {"structure": at, "set": to}
        })
    }

    /// A line of the example's header and system: one route `route`, one
    /// alignment through a point tied to each structure.
    fn straight(
        route: &str,
        prefix: &str,
        structures: Vec<Value>,
        sections: Vec<Value>,
        jumpers: Vec<Value>,
    ) -> Line {
        let mut v = json(&example());
        let points: Vec<Value> = structures
            .iter()
            .map(|s| {
                serde_json::json!({
                    "id": format!("{prefix}{}", s["id"].as_str().unwrap()),
                    "position": s["position"], "structure": s["id"]
                })
            })
            .collect();
        v["routes"] = serde_json::json!([{"id": route, "name": route}]);
        v["alignments"] = serde_json::json!([{
            "id": route, "name": route, "route": route,
            "points": points.iter().map(|p| p["id"].clone()).collect::<Vec<_>>()
        }]);
        v["alignment_points"] = Value::Array(points);
        v["structures"] = Value::Array(structures);
        v["sections"] = Value::Array(sections);
        v["jumpers"] = Value::Array(jumpers);
        let line: Line = serde_json::from_value(v).unwrap();
        line.validate().unwrap();
        line
    }

    /// The project: towers T1…T10 300 m apart along a straight route and
    /// T6b between T6 and T7, strain towers T3, T5 and T8 (sets 1/11 back,
    /// 21/31 ahead), two circuits (A on sets 1/21, B on 11/31) in four
    /// sections each, jumpers at each strain tower. The variant covers T3
    /// to T8: T3 (its ahead sets only), T4 (its wires 2 m higher), T5 and
    /// T8 (its back sets only) the same towers, V6 at T6's place, V7 13 m
    /// beside T7; T6b and T7 not in it; its sections T3–T5 (A strung
    /// harder) and T5–V6–V7–T8, the jumper of circuit A at T5 only.
    fn project_and_variant() -> (Line, Line) {
        let x = |k: f64| 300.0 * (k - 1.0);
        let all = [1, 11, 21, 31];
        let ours = straight(
            "main",
            "P",
            vec![
                tower("T1", x(1.0), 0.0, 500.0, &[1, 11]),
                tower("T2", x(2.0), 0.0, 500.0, &[1, 11]),
                tower("T3", x(3.0), 0.0, 500.0, &all),
                tower("T4", x(4.0), 0.0, 500.0, &[1, 11]),
                tower("T5", x(5.0), 0.0, 500.0, &all),
                tower("T6", x(6.0), 0.0, 500.0, &[1, 11]),
                tower("T6b", x(6.5), 0.0, 500.0, &[1, 11]),
                tower("T7", x(7.0), 0.0, 500.0, &[1, 11]),
                tower("T8", x(8.0), 0.0, 500.0, &all),
                tower("T9", x(9.0), 0.0, 500.0, &[1, 11]),
                tower("T10", x(10.0), 0.0, 500.0, &[1, 11]),
            ],
            ["A", "B"]
                .iter()
                .flat_map(|c| {
                    let (b, a) = if *c == "A" { (1, 21) } else { (11, 31) };
                    [
                        section(
                            &format!("{c}1"),
                            c,
                            &[("T1", b), ("T2", b), ("T3", b)],
                            25e3,
                        ),
                        section(
                            &format!("{c}2"),
                            c,
                            &[("T3", a), ("T4", b), ("T5", b)],
                            25e3,
                        ),
                        section(
                            &format!("{c}3"),
                            c,
                            &[("T5", a), ("T6", b), ("T6b", b), ("T7", b), ("T8", b)],
                            25e3,
                        ),
                        section(
                            &format!("{c}4"),
                            c,
                            &[("T8", a), ("T9", b), ("T10", b)],
                            25e3,
                        ),
                    ]
                })
                .collect(),
            ["3", "5", "8"]
                .iter()
                .flat_map(|t| {
                    [
                        jumper(&format!("JA{t}"), &format!("T{t}"), 1, 21),
                        jumper(&format!("JB{t}"), &format!("T{t}"), 11, 31),
                    ]
                })
                .collect(),
        );
        let theirs = straight(
            "variant",
            "Q",
            vec![
                tower("T3", x(3.0), 0.0, 500.0, &[21, 31]),
                tower("T4", x(4.0), 0.0, 502.0, &[1, 11]),
                tower("T5", x(5.0), 0.0, 500.0, &all),
                tower("V6", x(6.0), 0.0, 500.0, &[1, 11]),
                tower("V7", x(7.0), 13.0, 500.0, &[1, 11]),
                tower("T8", x(8.0), 0.0, 500.0, &[1, 11]),
            ],
            ["A", "B"]
                .iter()
                .flat_map(|c| {
                    let (b, a) = if *c == "A" { (1, 21) } else { (11, 31) };
                    let t = if *c == "A" { 26e3 } else { 25e3 };
                    [
                        section(&format!("{c}2"), c, &[("T3", a), ("T4", b), ("T5", b)], t),
                        section(
                            &format!("{c}3"),
                            c,
                            &[("T5", a), ("V6", b), ("V7", b), ("T8", b)],
                            25e3,
                        ),
                    ]
                })
                .collect(),
            vec![jumper("JA5", "T5", 1, 21)],
        );
        (ours, theirs)
    }

    fn ids<T>(v: &[T], id: impl Fn(&T) -> &String) -> Vec<String> {
        let mut out: Vec<String> = v.iter().map(|x| id(x).clone()).collect();
        out.sort();
        out
    }

    /// An imported section at the place of the project's A2 whose id the
    /// import gives to another of its sections (here: its B2 is named
    /// A2): the one holding the id waits under another until its turn
    /// (owner report 2026-10-27: "section sec13 exists already"); each
    /// matches the project's at its place and the line checks.
    #[test]
    fn an_id_the_import_uses_twice_over_is_untangled() {
        let (ours, mut theirs) = project_and_variant();
        for s in &mut theirs.sections {
            match s.id.as_str() {
                "A2" => s.id = "S9".into(),
                "B2" => s.id = "A2".into(),
                _ => {}
            }
        }
        let p = plan(&ours, &[], &imported(theirs), &ImportOptions::default()).unwrap();
        let at = |id: &str| {
            p.line
                .sections
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.circuit_name.clone())
        };
        // A2 is circuit A's again, B2 circuit B's: each matched by place.
        assert_eq!(at("A2").as_deref(), Some("A"));
        assert_eq!(at("B2").as_deref(), Some("B"));
        assert!(
            p.renamed
                .iter()
                .all(|r| !r.1.contains('~') && !r.2.contains('~')),
            "{:?}",
            p.renamed
        );
        assert!(!p.line.sections.iter().any(|s| s.id.contains('~')));
        p.line.validate().unwrap();
    }

    /// A project section running through a removed tower and on beyond
    /// the stretch's end (circuit B strung from T5 to T10 in one, through
    /// T8) is split at the end (owner, 2026-10-27): its part T8–T10 stays,
    /// under its id, the part inside goes; the line checks.
    #[test]
    fn a_section_crossing_the_stretch_is_split_at_its_end() {
        let (mut ours, theirs) = project_and_variant();
        let through = section(
            "B34",
            "B",
            &[
                ("T5", 31),
                ("T6", 11),
                ("T6b", 11),
                ("T7", 11),
                ("T8", 11),
                ("T9", 11),
                ("T10", 11),
            ],
            25e3,
        );
        ours.sections.retain(|s| s.id != "B3" && s.id != "B4");
        ours.sections.push(serde_json::from_value(through).unwrap());
        ours.jumpers.retain(|j| j.id != "JB8");
        ours.validate().unwrap();
        let p = plan(&ours, &[], &imported(theirs), &ImportOptions::default()).unwrap();
        let r = p.removed.iter().find(|r| r.id == "B34").unwrap();
        assert_eq!(r.pieces, [(4, 6)], "{r:?}");
        assert!(r.why.contains("split"), "{}", r.why);
        let kept = p.line.sections.iter().find(|s| s.id == "B34").unwrap();
        let at: Vec<&str> = kept.points.iter().map(|q| q.structure.as_str()).collect();
        assert_eq!(at, ["T8", "T9", "T10"]);
        assert!(
            p.notes
                .iter()
                .any(|n| n.contains("split at the stretch's end")),
            "{:?}",
            p.notes
        );
        p.line.validate().unwrap();
    }

    /// A spur the variant draws again to a structure the project has
    /// twice (owner report 2026-10-27, a substation 20 m from its older
    /// copy): the project's spur from T8 to the older copy `Sold`, 20 m
    /// from the variant's `S`, is replaced — its section removed, and
    /// `Sold`, left with nothing, too; a spur ending far from any of the
    /// import's (`Far`, 80 m off a 300 m span) stays.
    #[test]
    fn a_span_to_an_older_copy_of_the_far_end_is_replaced() {
        let (mut ours, mut theirs) = project_and_variant();
        let x8 = 300.0 * 7.0;
        let add = |line: &mut Line, t: Value, sec: Value| {
            line.structures.push(serde_json::from_value(t).unwrap());
            line.sections.push(serde_json::from_value(sec).unwrap());
        };
        add(
            &mut ours,
            tower("Sold", x8 + 20.0, 300.0, 500.0, &[1]),
            section("E1", "A", &[("T8", 21), ("Sold", 1)], 10e3),
        );
        add(
            &mut ours,
            tower("Far", x8 - 80.0, -300.0, 500.0, &[1]),
            section("E2", "B", &[("T8", 31), ("Far", 1)], 10e3),
        );
        ours.validate().unwrap();
        // The variant's T8 with its ahead sets, the spur to S and another
        // spur away from Far.
        let t8 = theirs.structures.iter().position(|s| s.id == "T8").unwrap();
        theirs.structures[t8] =
            serde_json::from_value(tower("T8", x8, 0.0, 500.0, &[1, 11, 21, 31])).unwrap();
        add(
            &mut theirs,
            tower("S", x8, 300.0, 500.0, &[1]),
            section("E1", "A", &[("T8", 21), ("S", 1)], 10e3),
        );
        add(
            &mut theirs,
            tower("U", x8, -300.0, 500.0, &[1]),
            section("E3", "B", &[("T8", 31), ("U", 1)], 10e3),
        );
        theirs.validate().unwrap();
        let p = plan(
            &ours,
            &[],
            &imported(theirs.clone()),
            &ImportOptions::default(),
        )
        .unwrap();
        let r = p.removed.iter().find(|r| r.id == "E1").unwrap();
        assert!(r.why.contains("20.0 m from the import's S"), "{}", r.why);
        assert!(!r.keep && !r.fixed);
        let old = p.removed.iter().find(|r| r.id == "Sold").unwrap();
        assert_eq!(old.kind, "structure");
        assert!(!p.removed.iter().any(|r| r.id == "E2" || r.id == "Far"));
        assert!(p.line.structure("Sold").is_none());
        assert!(p.line.structure("Far").is_some());
        assert!(p.line.sections.iter().any(|s| s.id == "E2"));
        p.line.validate().unwrap();
        // Chosen to keep: the section stays, and so does Sold.
        let mut keep = ImportOptions::default();
        keep.kept.insert("remove:section:E1".into());
        let p = plan(&ours, &[], &imported(theirs), &keep).unwrap();
        assert!(
            p.line
                .sections
                .iter()
                .any(|s| s.id == "E1" && s.points[1].structure == "Sold")
        );
        assert!(p.line.structure("Sold").is_some());
    }

    /// Replace (the default): the variant replaces T3–T8. Outside it the
    /// line stays (T1, T2, T9, T10, the sections there, the jumpers at
    /// the stretch's ends T3 and T8); T3, T4, T5, T8 are the imported
    /// towers (T4 2 m higher), T3 and T8 keeping the project's back and
    /// ahead sets the sections outside hang on; T6 becomes V6 (the
    /// imported tower at its place); T6b and T7 are removed with the
    /// sections over them and the jumper of circuit B at T5 (inside, not
    /// in the import); V7 is added; the import's sections replace the
    /// project's inside; the alignment runs through V7. The line checks
    /// and solves; every removal is listed.
    #[test]
    fn a_variant_replaces_its_stretch() {
        let (ours, theirs) = project_and_variant();
        let p = plan(&ours, &[], &imported(theirs), &ImportOptions::default()).unwrap();
        assert_eq!(p.mode, ImportMode::Replace);
        let removed: Vec<(&str, &str, bool)> = p
            .removed
            .iter()
            .map(|r| (r.kind, r.id.as_str(), r.fixed))
            .collect();
        assert_eq!(
            removed,
            [
                ("structure", "T6b", false),
                ("structure", "T7", false),
                ("section", "A3", true),
                ("section", "B3", true),
                ("jumper", "JB5", false),
            ],
            "{:?}",
            p.removed
        );
        assert!(
            p.removed
                .iter()
                .all(|r| r.key == format!("remove:{}:{}", r.kind, r.id))
        );
        assert!(
            p.renamed
                .contains(&("structure (the project's)", "T6".into(), "V6".into()))
        );
        assert_eq!(p.routes["variant"], Some("main".to_owned()));
        let l = &p.line;
        assert_eq!(
            ids(&l.structures, |s| &s.id),
            ["T1", "T10", "T2", "T3", "T4", "T5", "T8", "T9", "V6", "V7"]
        );
        assert_eq!(
            ids(&l.sections, |s| &s.id),
            ["A1", "A2", "A3", "A4", "B1", "B2", "B3", "B4"]
        );
        assert_eq!(
            ids(&l.jumpers, |j| &j.id),
            ["JA3", "JA5", "JA8", "JB3", "JB8"]
        );
        // The import's sections inside: A2 strung harder, A3 over V7.
        let a2 = l.sections.iter().find(|s| s.id == "A2").unwrap();
        assert!(matches!(
            a2.stringing,
            ohl_line::Stringing::Tension { horizontal_tension, .. } if horizontal_tension == 26e3
        ));
        let a3 = l.sections.iter().find(|s| s.id == "A3").unwrap();
        assert_eq!(a3.points[2].structure, "V7");
        // The imported towers; the ends keep the project's sets the
        // sections and jumpers outside hang on.
        let t4 = l.structure("T4").unwrap();
        assert_eq!(t4.wire_at_set(1).unwrap().position[2], 528.5);
        for t in ["T3", "T8"] {
            let s = l.structure(t).unwrap();
            for n in [1, 11, 21, 31] {
                assert!(s.wire_at_set(n).is_some(), "{t} set {n}");
            }
        }
        let t3 = p.shared.iter().find(|t| t.existing == "T3").unwrap();
        assert_eq!(t3.resolution, Resolution::TakeImported);
        let s1 = t3.sets.iter().find(|s| s.set == 1).unwrap();
        assert_eq!(s1.state, SetState::OnlyExisting);
        assert_eq!(s1.kept_for, ["section A1", "jumper JA3"]);
        // The alignment through the import's points between T3 and T8.
        let a = &l.alignments[0];
        assert_eq!(
            a.points,
            [
                "PT1", "PT2", "PT3", "PT4", "PT5", "PT6", "QV7", "PT8", "PT9", "PT10"
            ]
        );
        assert_eq!(l.alignment_points.len(), 10);
        assert_eq!(l.routes.len(), 1);
        l.validate().unwrap();
        solves(l);
        assert!(
            !p.notes.iter().any(|n| n.contains("does not")),
            "{:?}",
            p.notes
        );
    }

    /// Add: the same variant only added — nothing removed, the project's
    /// towers kept (T6 keeps its id), V7 and the import's sections over it
    /// added apart; the route a new one.
    #[test]
    fn a_variant_added_removes_nothing() {
        let (ours, theirs) = project_and_variant();
        let p = plan(&ours, &[], &imported(theirs), &add()).unwrap();
        assert!(p.removed.is_empty());
        assert!(p.line.structure("T6b").is_some() && p.line.structure("T7").is_some());
        assert!(p.line.structure("V6").is_none() && p.line.structure("V7").is_some());
        assert!(
            p.shared
                .iter()
                .all(|t| t.resolution == Resolution::KeepExisting)
        );
        assert_eq!(
            p.line
                .structure("T4")
                .unwrap()
                .wire_at_set(1)
                .unwrap()
                .position[2],
            526.5
        );
        assert!(p.line.sections.iter().any(|s| s.id == "A3-2"));
        assert_eq!(p.routes["variant"], None);
        assert_eq!(p.line.routes.len(), 2);
    }

    /// Replace with choices: T7 and the jumper JB5 kept instead of
    /// removed (the sections over T6b still go with it), T4 kept as the
    /// project's tower. The line checks.
    #[test]
    fn a_removal_can_be_kept() {
        let (ours, theirs) = project_and_variant();
        let o = ImportOptions {
            kept: ["remove:structure:T7", "remove:jumper:JB5"]
                .map(String::from)
                .into(),
            choices: [("tower:T4".to_owned(), Resolution::KeepExisting)].into(),
            ..Default::default()
        };
        let p = plan(&ours, &[], &imported(theirs), &o).unwrap();
        let keep: Vec<&str> = p
            .removed
            .iter()
            .filter(|r| r.keep)
            .map(|r| r.id.as_str())
            .collect();
        assert_eq!(keep, ["T7", "JB5"]);
        assert!(p.line.structure("T7").is_some());
        assert!(p.line.structure("T6b").is_none());
        assert!(p.line.jumpers.iter().any(|j| j.id == "JB5"));
        assert!(
            !p.line
                .sections
                .iter()
                .any(|s| s.points.iter().any(|q| q.structure == "T6b"))
        );
        assert_eq!(
            p.line
                .structure("T4")
                .unwrap()
                .wire_at_set(1)
                .unwrap()
                .position[2],
            526.5
        );
        p.line.validate().unwrap();
    }

    /// Replace with a branch meeting the line at one tower: the imported
    /// junction tower taken (its turned axes too), the project's sets the
    /// main line hangs on the same attachments; one tower in both covers
    /// no stretch, nothing is removed. The line checks and solves.
    #[test]
    fn a_branch_in_replace_mode_removes_nothing() {
        let (ours, theirs) = junction();
        let p = plan(
            &ours,
            &[],
            &imported(theirs.clone()),
            &ImportOptions::default(),
        )
        .unwrap();
        assert!(p.removed.is_empty(), "{:?}", p.removed);
        assert_eq!(p.shared[0].resolution, Resolution::TakeImported);
        let t3 = p.line.structure("T3").unwrap();
        assert_eq!(
            t3.orientation_deg,
            theirs.structure("T3").unwrap().orientation_deg
        );
        assert_eq!(p.line.sections.len(), ours.sections.len() + 1);
        p.line.validate().unwrap();
        solves(&p.line);
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
            ..add()
        };
        let p = plan(&line, &ours, &theirs, &both).unwrap();
        assert_eq!(p.duplicates.len(), 1);
        let names: Vec<&str> = p.libraries.iter().map(|l| l.1.as_str()).collect();
        assert_eq!(names, ["w (2).ohlw", "c.ohlwc"]);
        let mut far = line.clone();
        far.crs = "EPSG:2056".into();
        let mut near = line;
        near.crs = "EPSG:21781".into();
        assert!(plan(&near, &[], &imported(far), &add()).is_err());
    }
}
