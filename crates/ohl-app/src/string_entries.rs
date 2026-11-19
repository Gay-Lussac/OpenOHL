// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Strings written inline (their length, weight… on the string, no
//! library entry named) made entries of the project's insulator library
//! (owner, 2026-11-18): one function for **Make a library entry from this
//! string** (a placed structure's Edit window, [`crate::placed_strings`])
//! and **Put the project's strings in a library** (every inline string of
//! the project, [`crate::string_library_window`]).
//!
//! - **Grouped exactly**: strings of the very same values — kind, length,
//!   weight, wind area, axial stiffness, rated load, swings, the legs of
//!   their wire point (a V's) — share one entry; values are compared as
//!   stored (to the last bit), never rounded. An entry of the project's
//!   library with exactly those values is reused.
//! - **Results unchanged**: a string keeps every value it has and only
//!   names its entry (`insulator`), whose values are its own — so sags,
//!   tensions, loads and swings stay the same.
//! - **One command**: the new `.ohli` files, the structures and the
//!   structure type files changed go as one command of the session (one
//!   log entry, one undo).
//! - **Left inline**, each with its reason: a string without a weight (an
//!   entry would give it one), a post insulator (its entry is made in the
//!   library, with its ratings), a string with volumes of its own, a type
//!   string without a length.

use std::collections::BTreeMap;
use std::path::Path;

use ohl_core::units::{Angle, Area, Force, Length};
use ohl_line::{InsulatorString, Line, Structure};
use ohl_session::{Command, Kind};
use ohl_structures::{
    AssemblyKind, InsulatorAssembly, InsulatorLibrary, LibraryItem, StructureType,
};

use crate::model::Model;

/// The values of a string an entry holds, as stored (compared exactly).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Values {
    /// Suspension or strain.
    pub strain: bool,
    /// Length, m.
    pub length: f64,
    /// Weight, N.
    pub weight: f64,
    /// Wind area, m².
    pub wind_area: f64,
    /// Axial stiffness EA, N.
    pub stiffness: f64,
    /// Rated load, N.
    pub max_load: Option<f64>,
    /// Swing limits, degrees: any direction, across, along.
    pub swings: [Option<f64>; 3],
    /// The strings to its wire point (2: a V's leg).
    pub legs: usize,
}

impl Values {
    /// Of a string of the line (`resolved`: the structure as resolved;
    /// `stiffness`: the line's default where it gives none). Why not, if
    /// it is left inline.
    pub fn of_line(
        x: &InsulatorString,
        resolved: &Structure,
        stiffness: f64,
    ) -> Result<Self, String> {
        if x.post {
            return Err(
                "a post insulator: make its entry in the insulator library (its ratings)".into(),
            );
        }
        if !x.shapes.is_empty() {
            return Err("it has volumes of its own".into());
        }
        let r = resolved.string(&x.label).unwrap_or(x);
        if r.weight == 0.0 {
            return Err("no weight given (0 N): an entry would give it one".into());
        }
        let length = if r.length > 0.0 {
            r.length
        } else {
            resolved.string_length(r).ok_or("its length is not known")?
        };
        Ok(Values {
            strain: r.strain,
            length,
            weight: r.weight,
            wind_area: r.wind_area,
            stiffness: r.stiffness.unwrap_or(stiffness),
            max_load: r.max_load,
            swings: [
                r.max_swing_deg,
                r.max_swing_across_deg,
                r.max_swing_along_deg,
            ],
            legs: resolved.strings_to(&r.wire).count(),
        })
    }

    /// Of a string of structure type `t` (`stiffness`: the line's
    /// default).
    pub fn of_type(
        x: &ohl_structures::structures::TypeString,
        t: &StructureType,
        stiffness: f64,
    ) -> Result<Self, String> {
        if x.post {
            return Err(
                "a post insulator: make its entry in the insulator library (its ratings)".into(),
            );
        }
        if !x.shapes.is_empty() {
            return Err("it has volumes of its own".into());
        }
        let length = x.length.ok_or("no length given")?.si();
        let weight = x.weight.map_or(0.0, |w| w.si());
        if weight == 0.0 {
            return Err("no weight given (0 N): an entry would give it one".into());
        }
        let l = x.limits();
        Ok(Values {
            strain: x.strain,
            length,
            weight,
            wind_area: x.wind_area.map_or(0.0, |a| a.si()),
            stiffness,
            max_load: l.max_load,
            swings: [
                l.max_swing_deg,
                l.max_swing_across_deg,
                l.max_swing_along_deg,
            ],
            legs: t.strings.iter().filter(|y| y.wire == x.wire).count(),
        })
    }

    /// Of a library entry (`legs`: what it is reused for).
    pub fn of_entry(a: &InsulatorAssembly, legs: usize) -> Option<Self> {
        let strain = match a.kind {
            AssemblyKind::Suspension => false,
            AssemblyKind::Strain => true,
            _ => return None,
        };
        if !a.volumes.is_empty() || a.compression_load.is_some() || a.cantilever_load.is_some() {
            return None;
        }
        let l = a.limits();
        Some(Values {
            strain,
            length: a.length.si(),
            weight: a.weight.si(),
            wind_area: a.wind_area.si(),
            stiffness: a.axial_stiffness.si(),
            max_load: l.max_load,
            swings: [
                l.max_swing_deg,
                l.max_swing_across_deg,
                l.max_swing_along_deg,
            ],
            legs,
        })
    }

    /// Exactly the same values (to the last bit).
    pub fn same(&self, o: &Values) -> bool {
        let eq = |a: f64, b: f64| a.to_bits() == b.to_bits();
        let eqo = |a: Option<f64>, b: Option<f64>| match (a, b) {
            (Some(a), Some(b)) => eq(a, b),
            (None, None) => true,
            _ => false,
        };
        self.strain == o.strain
            && eq(self.length, o.length)
            && eq(self.weight, o.weight)
            && eq(self.wind_area, o.wind_area)
            && eq(self.stiffness, o.stiffness)
            && eqo(self.max_load, o.max_load)
            && (0..3).all(|k| eqo(self.swings[k], o.swings[k]))
            && self.legs == o.legs
    }

    /// A readable label: kind, length, rating (`Suspension-3.500m-900N`,
    /// `V-leg-…`, `…-70kN`).
    pub fn label(&self) -> String {
        let kind = match (self.strain, self.legs >= 2) {
            (true, _) => "Strain",
            (false, true) => "V-leg",
            (false, false) => "Suspension",
        };
        let mut l = format!("{kind}-{:.3}m-{:.0}N", self.length, self.weight);
        if let Some(r) = self.max_load {
            l.push_str(&format!("-{:.0}kN", r / 1000.0));
        }
        l
    }

    /// The entry of these values, `id`.
    pub fn entry(&self, id: &str) -> InsulatorAssembly {
        let deg = |a: Option<f64>| a.map(Angle::from_degrees);
        InsulatorAssembly {
            format: None,
            version: None,
            id: id.to_owned(),
            name: format!(
                "{} {:.3} m, {:.0} N",
                match (self.strain, self.legs >= 2) {
                    (true, _) => "strain string",
                    (false, true) => "leg of a V",
                    (false, false) => "suspension string",
                },
                self.length,
                self.weight
            ),
            kind: if self.strain {
                AssemblyKind::Strain
            } else {
                AssemblyKind::Suspension
            },
            source: Some("made from the project's inline strings".into()),
            length: Length::from_meters(self.length),
            weight: Force::from_newtons(self.weight),
            axial_stiffness: Force::from_newtons(self.stiffness),
            wind_area: Area::from_square_meters(self.wind_area),
            half_width: None,
            max_load: self.max_load.map(Force::from_newtons),
            max_swing: deg(self.swings[0]),
            max_swing_across: deg(self.swings[1]),
            max_swing_along: deg(self.swings[2]),
            cantilever_load: None,
            compression_load: None,
            volumes: Default::default(),
        }
    }
}

/// Where an inline string is.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Place {
    /// A structure of the line (its own string).
    Structure(String),
    /// A structure type file of the project (its file name).
    Type(String),
}

impl Place {
    /// In words.
    pub fn describe(&self) -> String {
        match self {
            Place::Structure(id) => format!("structure {id}"),
            Place::Type(f) => format!("structure type file {f}"),
        }
    }
}

/// An entry to make (or reused) and the strings it serves.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// Its label (the `.ohli` id; renamed in the preview).
    pub id: String,
    /// Its values.
    pub values: Values,
    /// An existing entry of the project's library reused.
    pub reused: bool,
    /// The strings: where, label.
    pub strings: Vec<(Place, String)>,
}

/// What putting the project's strings in a library does.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    /// The entries: new ones and those reused.
    pub groups: Vec<Group>,
    /// The strings left inline: where, label, why.
    pub left: Vec<(Place, String, String)>,
}

impl Plan {
    /// The strings it points at an entry.
    pub fn strings(&self) -> usize {
        self.groups.iter().map(|g| g.strings.len()).sum()
    }

    /// Ids used twice, or taken by another entry of `library`: why it
    /// cannot be applied.
    pub fn check(&self, library: &InsulatorLibrary) -> Result<(), String> {
        let mut seen: Vec<&str> = Vec::new();
        for g in &self.groups {
            if g.id.trim().is_empty() || !crate::placed_editor::type_id(&g.id).eq(&g.id) {
                return Err(format!(
                    "entry \"{}\": a label of letters, digits, - _ . only",
                    g.id
                ));
            }
            if seen.contains(&g.id.as_str()) {
                return Err(format!("two entries are called {}", g.id));
            }
            seen.push(&g.id);
            if !g.reused && library.get(&g.id).is_some() {
                return Err(format!("{} is an entry of the library already", g.id));
            }
        }
        Ok(())
    }
}

/// Groups `found` (where, label, values or why left inline) into entries:
/// exactly the same values share one; an entry of `library` with those
/// values is reused; new ones take a free readable label.
fn group(found: Vec<(Place, String, Result<Values, String>)>, library: &InsulatorLibrary) -> Plan {
    let mut plan = Plan::default();
    for (place, label, v) in found {
        let v = match v {
            Ok(v) => v,
            Err(why) => {
                plan.left.push((place, label, why));
                continue;
            }
        };
        if let Some(g) = plan.groups.iter_mut().find(|g| g.values.same(&v)) {
            g.strings.push((place, label));
            continue;
        }
        let reused = library
            .iter()
            .find(|a| Values::of_entry(a, v.legs).is_some_and(|e| e.same(&v)));
        let id = match reused {
            Some(a) => a.id.clone(),
            None => {
                let stem = v.label();
                let taken =
                    |id: &str| library.get(id).is_some() || plan.groups.iter().any(|g| g.id == id);
                std::iter::once(stem.clone())
                    .chain((2..).map(|n| format!("{stem}-{n}")))
                    .find(|id| !taken(id))
                    .expect("a free label")
            }
        };
        plan.groups.push(Group {
            id,
            values: v,
            reused: reused.is_some(),
            strings: vec![(place, label)],
        });
    }
    plan
}

/// The insulator library and the line's default stiffness of `model`.
fn library_of(model: &Model) -> (InsulatorLibrary, f64) {
    let library = model
        .types
        .as_ref()
        .map(|(_, i)| i.clone())
        .unwrap_or_default();
    (library, model.line.defaults.insulator_stiffness)
}

/// Every inline string of the project: the structures' own strings
/// (`stored`: the line as stored, `resolved`: as resolved) and those of the
/// project's structure type files (`types`: file name, text).
fn inline_strings(
    stored: &Line,
    resolved: &Line,
    types: &[(String, String)],
    stiffness: f64,
) -> Vec<(Place, String, Result<Values, String>)> {
    let mut out = Vec::new();
    for s in &stored.structures {
        let Some(r) = resolved.structure(&s.id) else {
            continue;
        };
        for x in s.strings.iter().filter(|x| x.insulator.is_none()) {
            out.push((
                Place::Structure(s.id.clone()),
                x.label.clone(),
                Values::of_line(x, r, stiffness),
            ));
        }
    }
    for (name, text) in types {
        let Ok(t) = StructureType::parse(Path::new(name), text) else {
            continue;
        };
        for x in t.strings.iter().filter(|x| x.insulator.is_none()) {
            out.push((
                Place::Type(name.clone()),
                x.label.clone(),
                Values::of_type(x, &t, stiffness),
            ));
        }
    }
    out
}

/// The project's structure type files kept in the session (name, text).
fn type_files(model: &Model) -> Vec<(String, String)> {
    model
        .session()
        .and_then(|s| s.library_files("structures").ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|(n, _)| n.ends_with(".ohls"))
        .collect()
}

/// What putting every inline string of the project in its library would
/// do (the preview).
pub fn plan(model: &Model) -> Result<Plan, String> {
    let stored = model
        .stored_line()
        .ok_or("read-only: open a project to edit")?;
    let (library, stiffness) = library_of(model);
    let found = inline_strings(stored, &model.line, &type_files(model), stiffness);
    Ok(group(found, &library))
}

/// The plan for string `label` of structure `id` alone.
pub fn plan_one(model: &Model, id: &str, label: &str) -> Result<Plan, String> {
    let stored = model
        .stored_line()
        .ok_or("read-only: open a project to edit")?;
    let s = stored
        .structure(id)
        .ok_or_else(|| format!("no structure {id}"))?;
    let r = model
        .line
        .structure(id)
        .ok_or_else(|| format!("no structure {id}"))?;
    let x = s
        .string(label)
        .ok_or_else(|| format!("string {label} is not the structure's own"))?;
    if let Some(i) = &x.insulator {
        return Err(format!("string {label} is library entry {i} already"));
    }
    let (library, stiffness) = library_of(model);
    let plan = group(
        vec![(
            Place::Structure(id.to_owned()),
            label.to_owned(),
            Values::of_line(x, r, stiffness),
        )],
        &library,
    );
    if let Some((_, _, why)) = plan.left.first() {
        return Err(format!("string {label} is left as it is: {why}"));
    }
    Ok(plan)
}

/// The one command applying `plan`: the new entries as `.ohli` files of
/// the project, each string pointing at its entry (`insulator`), every
/// other value as it was; with what it does in words.
pub fn command(model: &Model, plan: &Plan) -> Result<(String, Command), String> {
    let (library, _) = library_of(model);
    plan.check(&library)?;
    let stored = model
        .stored_line()
        .ok_or("read-only: open a project to edit")?;
    let e = model.editing.as_ref().map_err(Clone::clone)?;
    // Which entry each string takes.
    let mut to: BTreeMap<(Place, String), String> = BTreeMap::new();
    for g in &plan.groups {
        for (p, l) in &g.strings {
            to.insert((p.clone(), l.clone()), g.id.clone());
        }
    }
    let mut files: Vec<(String, String, String)> = plan
        .groups
        .iter()
        .filter(|g| !g.reused)
        .map(|g| {
            (
                "insulators".to_owned(),
                format!(
                    "{}.{}",
                    g.id,
                    ohl_structures::insulators::INSULATOR_FILE_EXTENSION
                ),
                g.values.entry(&g.id).to_toml(),
            )
        })
        .collect();
    // The structure type files: `insulator = "…"` added to their strings.
    for (name, text) in type_files(model) {
        let labels: Vec<(&String, &String)> = to
            .iter()
            .filter_map(|((p, l), id)| (*p == Place::Type(name.clone())).then_some((l, id)))
            .collect();
        if labels.is_empty() {
            continue;
        }
        let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| format!("{name}: {e}"))?;
        if let Some(a) = doc
            .get_mut("string")
            .and_then(toml_edit::Item::as_array_of_tables_mut)
        {
            for t in a.iter_mut() {
                let l = t
                    .get("label")
                    .and_then(toml_edit::Item::as_str)
                    .map(str::to_owned);
                if let Some((_, id)) = labels
                    .iter()
                    .find(|(x, _)| Some(x.as_str()) == l.as_deref())
                {
                    t.insert("insulator", toml_edit::value(id.as_str()));
                    // A type's string gives an insulator or a length, not
                    // both: its entry's length is its own, exactly.
                    t.remove("length");
                }
            }
        }
        files.push(("structures".to_owned(), name, doc.to_string()));
    }
    let mut commands = e
        .session
        .library_puts(&e.user, &files)
        .map_err(|e| e.to_string())?;
    // The structures: their strings name their entry.
    let mut structures = 0;
    for s in &stored.structures {
        let mut s = s.clone();
        let mut any = false;
        for x in s.strings.iter_mut() {
            if let Some(id) = to.get(&(Place::Structure(s.id.clone()), x.label.clone())) {
                x.insulator = Some(id.clone());
                any = true;
            }
        }
        if any {
            structures += 1;
            commands.push(Command::Put {
                kind: Kind::Structure,
                id: s.id.clone(),
                value: serde_json::to_value(&s).map_err(|e| e.to_string())?,
                ord: None,
            });
        }
    }
    let made = plan.groups.iter().filter(|g| !g.reused).count();
    let what = format!(
        "strings put in the library: {} string(s) of {structures} structure(s) and {} type \
         file(s) point at {} entr{} ({made} new, {} reused)",
        plan.strings(),
        files.len() - made,
        plan.groups.len(),
        if plan.groups.len() == 1 { "y" } else { "ies" },
        plan.groups.len() - made
    );
    Ok((what, Command::Batch { commands }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(length: f64, weight: f64) -> Values {
        Values {
            strain: false,
            length,
            weight,
            wind_area: 0.1,
            stiffness: 5e7,
            max_load: None,
            swings: [None; 3],
            legs: 1,
        }
    }

    /// Exactly the same values share an entry; one bit apart they do not;
    /// a library entry of the same values is reused; labels are free.
    #[test]
    fn strings_group_exactly() {
        let p = |s: &str| Place::Structure(s.into());
        let next = f64::from_bits(3.5f64.to_bits() + 1);
        let found = vec![
            (p("T1"), "S1".into(), Ok(v(3.5, 900.0))),
            (p("T2"), "S1".into(), Ok(v(3.5, 900.0))),
            (p("T3"), "S1".into(), Ok(v(next, 900.0))),
            (p("T4"), "S1".into(), Err("no weight".into())),
            (p("T5"), "S1".into(), Ok(v(2.0, 500.0))),
        ];
        let mut library = InsulatorLibrary::default();
        library
            .insert(Path::new("mine.ohli"), v(2.0, 500.0).entry("mine"))
            .unwrap();
        let plan = group(found, &library);
        assert_eq!(plan.groups.len(), 3);
        assert_eq!(plan.groups[0].id, "Suspension-3.500m-900N");
        assert_eq!(plan.groups[0].strings.len(), 2);
        assert_eq!(plan.groups[1].id, "Suspension-3.500m-900N-2");
        assert_eq!(
            (plan.groups[2].id.as_str(), plan.groups[2].reused),
            ("mine", true)
        );
        assert_eq!(plan.left.len(), 1);
        assert!(plan.check(&library).is_ok());
        // An entry's values read back exactly from its file.
        let a = v(1.0 / 3.0, 912.345_678_9).entry("x");
        let back = InsulatorAssembly::parse(Path::new("x.ohli"), &a.to_toml()).unwrap();
        assert!(
            Values::of_entry(&back, 1)
                .unwrap()
                .same(&v(1.0 / 3.0, 912.345_678_9))
        );
    }
}
