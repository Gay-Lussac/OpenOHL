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

/// What the import does.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportPlan {
    /// What appears twice, in the order the kinds are merged.
    pub duplicates: Vec<Duplicate>,
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

    // Alignment points and structures: the same id, or the same place.
    let mut settled: BTreeMap<(&'static str, String), Resolution> = BTreeMap::new();
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
            let same_id = ours.iter().find(|o| o.0 == id);
            let near = || {
                ours.iter()
                    .filter(|o| !used.contains(&o.0))
                    .filter(|o| (o.1[0] - at[0]).hypot(o.1[1] - at[1]) < tolerance)
                    .min_by(|a, b| {
                        let d =
                            |o: &&(String, [f64; 2], Value)| (o.1[0] - at[0]).hypot(o.1[1] - at[1]);
                        d(a).total_cmp(&d(b))
                    })
            };
            let found = same_id.or_else(near);
            let Some((eid, _, evalue)) = found else {
                *out.added.entry(word).or_default() += 1;
                continue;
            };
            used.insert(eid.clone());
            if *eid == id && *evalue == value {
                *out.identical.entry(word).or_default() += 1;
                settled.insert((word, id.clone()), Resolution::KeepExisting);
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

    // Systems, alignments, sections, jumpers, spacers, circuits: the same
    // id, or (sections, jumpers) the same place once the structures are
    // matched.
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
        for (id, value) in theirs {
            // An alignment of a route said to be the project's: by id.
            let same_id = ours.iter().find(|o| o.0 == id);
            let same_place = || {
                let p = place(&value)?;
                ours.iter().find(|o| place(&o.1).as_ref() == Some(&p))
            };
            let Some((eid, evalue)) = same_id.or_else(same_place) else {
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
    // An alignment of a route said to be the project's that adds points
    // to it: kept as the import gives it (a route may hold several).
    out.line = line;

    // Library files: by kind and name.
    for (kind, name, text) in &imported.libraries {
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
    if let Err(e) = out.line.validate() {
        out.notes
            .push(format!("the merged line does not check yet: {e}"));
    }
    out.notes.extend(imported.notes.iter().cloned());
    Ok(out)
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
    /// as the project's by default (the import's sections then refer to
    /// it and are the project's); taken, it replaces the project's under
    /// its id; both kept, the imported one stays apart with its sections.
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
        let d: Vec<&Duplicate> = p
            .duplicates
            .iter()
            .filter(|d| d.kind == "structure")
            .collect();
        assert_eq!(d.len(), 1);
        assert_eq!(
            (d[0].existing.as_str(), d[0].imported.as_str()),
            (id.as_str(), "X2")
        );
        assert_eq!(d[0].twice, Twice::SamePlace);
        assert_eq!(d[0].differences, ["position"]);
        // Kept as the project's: the line as it was.
        assert_eq!(p.line, line);
        // Taken: the project's structure moved, under its id.
        let take = ImportOptions {
            choices: [("structure:X2".to_owned(), Resolution::TakeImported)].into(),
            ..Default::default()
        };
        let p = plan(&line, &[], &imported(other.clone()), &take).unwrap();
        let st = p.line.structures.iter().find(|s| s.id == id).unwrap();
        assert!((st.position[0] - line.structures[1].position[0] - 0.3).abs() < 1e-9);
        assert_eq!(p.line.structures.len(), line.structures.len());
        // Both: the imported one added as X2, its sections apart.
        let both = ImportOptions {
            choices: [("structure:X2".to_owned(), Resolution::KeepBoth)].into(),
            ..Default::default()
        };
        let p = plan(&line, &[], &imported(other), &both).unwrap();
        assert_eq!(p.line.structures.len(), line.structures.len() + 1);
        assert!(p.duplicates.iter().any(|d| d.kind == "section"));
    }

    /// The same id with other values, kept both: the imported one under
    /// "id (2)", what refers to it following.
    #[test]
    fn the_same_id_kept_both_is_renamed() {
        let line = example();
        let id = line.structures[1].id.clone();
        let mut other = line.clone();
        other.structures[1].position[0] += 50.0;
        let both = ImportOptions {
            choices: [(format!("structure:{id}"), Resolution::KeepBoth)].into(),
            ..Default::default()
        };
        let p = plan(&line, &[], &imported(other.clone()), &both).unwrap();
        // Its sections differ from the project's of the same ids (they
        // name the new id): twice too; kept both, they come in renamed.
        let mut both = both;
        for d in p.duplicates.iter().filter(|d| d.kind == "section") {
            assert_eq!(d.differences, ["points"]);
            both.choices.insert(d.key.clone(), Resolution::KeepBoth);
        }
        let p = plan(&line, &[], &imported(other), &both).unwrap();
        let new = format!("{id} (2)");
        assert!(p.line.structures.iter().any(|s| s.id == new));
        assert!(p.renamed.contains(&("structure", id.clone(), new.clone())));
        // A section over it in the import now names the new id.
        assert!(
            p.line
                .sections
                .iter()
                .any(|s| s.points.iter().any(|q| q.structure == new))
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
