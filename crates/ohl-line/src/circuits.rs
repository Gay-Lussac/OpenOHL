// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Circuits: the conductors that form one circuit, defined by the user.
//!
//! A [`Circuit`] is an object of the line (`circuits` in the line file):
//! an id, a name, optionally an electrical system, a description. Its
//! **members** are conductor paths ([`crate::electrical::conductor_paths`]):
//! a conductor path belongs to the circuit whose name its sections give
//! (`circuit_name`), so that a section split, joined or renamed stays in
//! its circuit without anything else to update. A circuit may have none
//! (allowed, noted).
//!
//! **Circuits follow the physical connections** (owner decision of
//! 2026-10-21): the connections make the conductor paths (sections joined
//! by jumpers or a shared wire point), and what the user defines never
//! contradicts them —
//!
//! - a conductor path is in one circuit at most: its sections are joined,
//!   they are one conductor;
//! - a circuit has at most one conductor path per phase (of a system, by
//!   its label): two paths of one phase that are not connected are two
//!   conductors, so two circuits. There is no merge: two circuits become
//!   one only when they are connected (a jumper, a wire strung through),
//!   which makes their paths one — the jumper's window then asks which
//!   circuit's name stays (the other keeps its paths not connected, or is
//!   left with none, noted).
//!
//! Files written before circuits were objects gave only the names on the
//! sections: they are read into one circuit per distinct name, its id the
//! name, with the same members ([`Line::adopt_circuits`]) — everything
//! grouped by circuit (safety volumes per circuit, circuit-to-circuit
//! rules, phase distance, wire colours, `--circuits NAME`) is unchanged.
//!
//! Changes made here ([`Line::add_circuit`], [`Line::rename_circuit`],
//! [`Line::delete_circuit`], [`Line::assign_to_circuit`]) work on a copy of the line; the program
//! then puts the line's settings and the sections changed in one command
//! (one undo), refused when [`Line::check_circuit_change`] says so.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::Line;

/// A circuit: a named group of conductor paths (e.g. the three phases of
/// *North*). Its members are the conductor paths whose sections give its
/// name ([`crate::Section::circuit_name`]).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Circuit {
    /// Unique id (`--circuits` takes it or the name).
    pub id: String,
    /// Unique name: what its sections give as `circuit_name`, and what
    /// checks, safety volumes and reports show.
    pub name: String,
    /// The electrical system its conductor paths are of, when it is one
    /// (none: not said — its paths may be of several systems or earthed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// What it is, in words (free text).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

impl Circuit {
    /// A circuit `id` called `name`, no system, no description.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            ..Self::default()
        }
    }
}

/// One member of a circuit: a conductor path, by its sections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// Its sections (in line order).
    pub sections: Vec<String>,
    /// Its key: its first section by id (as the Conductor paths table).
    pub key: String,
    /// The phase labels its sections give (one, unless mixed).
    pub phases: Vec<String>,
    /// The electrical systems its sections give (none: earthed).
    pub systems: Vec<Option<String>>,
}

/// A conductor path's key: its smallest section id.
pub fn path_key(path: &[String]) -> String {
    path.iter().min().cloned().unwrap_or_default()
}

impl Line {
    /// The circuit with id or name `which` (the id first).
    pub fn circuit(&self, which: &str) -> Option<&Circuit> {
        self.circuits
            .iter()
            .find(|c| c.id == which)
            .or_else(|| self.circuits.iter().find(|c| c.name == which))
    }

    /// The names of the circuits `given` by id or name (`--circuits`; `""`
    /// stays: the sections in no circuit). An unknown one is refused, with
    /// the circuits there are (a typo would check nothing).
    pub fn circuit_names_for(&self, given: &[String]) -> Result<Vec<String>, String> {
        let mut lines = std::borrow::Cow::Borrowed(self);
        if !self.unlisted_circuit_names().is_empty() {
            lines.to_mut().adopt_circuits();
        }
        given
            .iter()
            .map(|g| {
                if g.is_empty() {
                    return Ok(String::new());
                }
                lines.circuit(g).map(|c| c.name.clone()).ok_or_else(|| {
                    format!(
                        "no circuit \"{g}\" (by id or name); the line's circuits: {}",
                        if lines.circuits.is_empty() {
                            "none".to_owned()
                        } else {
                            lines
                                .circuits
                                .iter()
                                .map(|c| {
                                    if c.id == c.name {
                                        c.id.clone()
                                    } else {
                                        format!("{} ({})", c.id, c.name)
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join(", ")
                        }
                    )
                })
            })
            .collect()
    }

    /// The circuit names on sections that no circuit has, in the order of
    /// their first section.
    fn unlisted_circuit_names(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for s in &self.sections {
            let n = &s.circuit_name;
            if !n.is_empty() && !out.contains(n) && !self.circuits.iter().any(|c| &c.name == n) {
                out.push(n.clone());
            }
        }
        out
    }

    /// Gives every circuit name found on sections and on no circuit a
    /// circuit of its own (files written before circuits were objects, a
    /// name typed on a section): its id the name (`-2`, `-3`… added when
    /// another circuit has that id), no system, the members those of the
    /// name. Returns the circuits added. Nothing a calculation reads
    /// changes.
    pub fn adopt_circuits(&mut self) -> Vec<String> {
        let names = self.unlisted_circuit_names();
        for n in &names {
            let mut id = n.clone();
            let mut k = 1;
            while self.circuits.iter().any(|c| c.id == id) {
                k += 1;
                id = format!("{n}-{k}");
            }
            self.circuits.push(Circuit::new(id, n.clone()));
        }
        names
    }

    /// The members of circuit `name` (by name): the conductor paths any of
    /// whose sections give it, in line order.
    pub fn circuit_members(&self, name: &str) -> Vec<Member> {
        if name.is_empty() {
            return Vec::new();
        }
        let by_id: BTreeMap<&str, &crate::Section> =
            self.sections.iter().map(|s| (s.id.as_str(), s)).collect();
        crate::electrical::conductor_paths(self)
            .into_iter()
            .filter(|p| p.iter().any(|id| by_id[id.as_str()].circuit_name == name))
            .map(|p| {
                let mut phases: Vec<String> = Vec::new();
                let mut systems: Vec<Option<String>> = Vec::new();
                for id in &p {
                    let s = by_id[id.as_str()];
                    if !phases.contains(&s.phase_label) {
                        phases.push(s.phase_label.clone());
                    }
                    if !systems.contains(&s.system) && s.system.is_some() {
                        systems.push(s.system.clone());
                    }
                }
                if systems.is_empty() {
                    systems.push(None);
                }
                Member {
                    key: path_key(&p),
                    sections: p,
                    phases,
                    systems,
                }
            })
            .collect()
    }

    /// The circuits' errors: an empty or repeated id or name, a name with
    /// spaces around it, an unknown system.
    pub fn validate_circuits(&self) -> Result<(), String> {
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for c in &self.circuits {
            if c.id.trim().is_empty() {
                return Err(format!("circuit \"{}\": give it an id", c.name));
            }
            if !ids.insert(c.id.as_str()) {
                return Err(format!("circuit id {} given twice", c.id));
            }
            if c.name.trim().is_empty() {
                return Err(format!("circuit {}: give it a name", c.id));
            }
            if c.name.trim() != c.name {
                return Err(format!(
                    "circuit {}: its name \"{}\" begins or ends with a space",
                    c.id, c.name
                ));
            }
            if !names.insert(c.name.as_str()) {
                return Err(format!("circuit name \"{}\" given twice", c.name));
            }
            if let Some(sys) = &c.system
                && !self.systems.iter().any(|s| &s.id == sys)
            {
                return Err(format!("circuit {}: unknown system {sys}", c.id));
            }
        }
        Ok(())
    }

    /// What its circuits contradict, in words: a conductor path whose
    /// sections give several circuits (or some none), two conductor paths
    /// of one phase in a circuit (not connected), a member of a circuit
    /// with a system that is of another system or earthed. A change
    /// bringing a new one is refused ([`Line::check_circuit_change`]);
    /// files written with one are read, and it is noted.
    pub fn circuit_conflicts(&self) -> Vec<String> {
        let mut out = Vec::new();
        let by_id: BTreeMap<&str, &crate::Section> =
            self.sections.iter().map(|s| (s.id.as_str(), s)).collect();
        for p in crate::electrical::conductor_paths(self) {
            let mut names: Vec<&str> = p
                .iter()
                .map(|id| by_id[id.as_str()].circuit_name.as_str())
                .collect();
            names.sort_unstable();
            names.dedup();
            if names.len() > 1 {
                out.push(format!(
                    "conductor path {} (sections {}) is in several circuits: {} — its \
                     sections are connected, so one circuit",
                    path_key(&p),
                    p.join(" "),
                    names
                        .iter()
                        .map(|n| if n.is_empty() { "(none)" } else { n })
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
        let phases = self.section_phases().unwrap_or_default();
        for c in &self.circuits {
            out.extend(self.unconnected_phases(c, &phases));
        }
        for c in &self.circuits {
            let Some(sys) = &c.system else { continue };
            for m in self.circuit_members(&c.name) {
                let other = m.sections.iter().find_map(|id| {
                    let given = phases
                        .get(id)
                        .copied()
                        .flatten()
                        .map(|(k, _)| self.systems[k].id.clone());
                    (given.as_ref() != Some(sys)).then_some(given)
                });
                if let Some(given) = other {
                    out.push(format!(
                        "circuit {}: its conductor path {} is {}, not of its system {sys}",
                        c.id,
                        m.key,
                        given.map_or("earthed (no system)".to_owned(), |g| format!(
                            "of system {g}"
                        ))
                    ));
                }
            }
        }
        out
    }

    /// Circuit `c`'s conductor paths of one phase (system and phase label)
    /// that are not connected — two conductors in one circuit — in words.
    /// `phases`: [`Line::section_phases`].
    fn unconnected_phases(
        &self,
        c: &Circuit,
        phases: &BTreeMap<String, Option<(usize, usize)>>,
    ) -> Vec<String> {
        let mut seen: BTreeMap<(String, String), String> = BTreeMap::new();
        let mut out = Vec::new();
        for m in self.circuit_members(&c.name) {
            // Its system: as given on any of its sections, else none.
            let system = m
                .sections
                .iter()
                .find_map(|id| phases.get(id).copied().flatten())
                .map_or(String::new(), |(k, _)| self.systems[k].id.clone());
            for ph in &m.phases {
                let key = (system.clone(), ph.clone());
                if let Some(first) = seen.get(&key) {
                    out.push(format!(
                        "circuit {}: its conductor paths {first} and {} are both phase {}{} \
                         and not connected — connect them (a jumper or a wire strung \
                         through) to make them one circuit; else they are two circuits",
                        c.id,
                        m.key,
                        if ph.is_empty() { "(no label)" } else { ph },
                        if system.is_empty() {
                            String::new()
                        } else {
                            format!(" of system {system}")
                        }
                    ));
                } else {
                    seen.insert(key, m.key.clone());
                }
            }
        }
        out
    }

    /// Everything to say about its circuits, in words: those with no
    /// member (allowed), then [`Line::circuit_conflicts`].
    pub fn circuit_notes(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .circuits
            .iter()
            .filter(|c| !self.sections.iter().any(|s| s.circuit_name == c.name))
            .map(|c| format!("circuit {} ({}) has no conductor path", c.id, c.name))
            .collect();
        out.extend(self.circuit_conflicts());
        out
    }

    /// Whether the line `self`, made from `before` by a change of
    /// circuits, may be: its circuits valid, and no contradiction that
    /// `before` did not have ([`Line::circuit_conflicts`]).
    pub fn check_circuit_change(&self, before: &Line) -> Result<(), String> {
        self.validate_circuits()?;
        let old = before.circuit_conflicts();
        let new: Vec<String> = self
            .circuit_conflicts()
            .into_iter()
            .filter(|c| !old.contains(c))
            .collect();
        if new.is_empty() {
            Ok(())
        } else {
            Err(new.join("; "))
        }
    }

    /// Adds circuit `c` (its id and name trimmed).
    pub fn add_circuit(&mut self, mut c: Circuit) -> Result<(), String> {
        c.id = c.id.trim().to_owned();
        c.name = c.name.trim().to_owned();
        if self.circuits.iter().any(|x| x.id == c.id) {
            return Err(format!("circuit id {} is taken", c.id));
        }
        if self.circuits.iter().any(|x| x.name == c.name) {
            return Err(format!("a circuit is called \"{}\" already", c.name));
        }
        self.circuits.push(c);
        self.validate_circuits()
    }

    fn circuit_index(&self, id: &str) -> Result<usize, String> {
        self.circuits
            .iter()
            .position(|c| c.id == id)
            .ok_or_else(|| format!("no circuit {id}"))
    }

    /// Renames circuit `id` to `name`; its members follow (their sections'
    /// `circuit_name`). Another circuit's name is refused: circuits are
    /// not merged, they become one only when connected.
    pub fn rename_circuit(&mut self, id: &str, name: &str) -> Result<(), String> {
        let name = name.trim();
        let k = self.circuit_index(id)?;
        let old = self.circuits[k].name.clone();
        if old == name {
            return Ok(());
        }
        if name.is_empty() {
            return Err(format!("circuit {id}: give it a name"));
        }
        if self.circuits.iter().any(|c| c.name == name) {
            return Err(format!(
                "circuit {id}: another circuit is called \"{name}\" — circuits are not merged \
                 by name; they become one when connected (a jumper or a wire strung through)"
            ));
        }
        self.circuits[k].name = name.to_owned();
        for s in self.sections.iter_mut().filter(|s| s.circuit_name == old) {
            s.circuit_name = name.to_owned();
        }
        Ok(())
    }

    /// Deletes circuit `id`; its members are in no circuit after.
    pub fn delete_circuit(&mut self, id: &str) -> Result<(), String> {
        let k = self.circuit_index(id)?;
        let c = self.circuits.remove(k);
        for s in self
            .sections
            .iter_mut()
            .filter(|s| s.circuit_name == c.name)
        {
            s.circuit_name.clear();
        }
        Ok(())
    }

    /// Puts the conductor paths of `sections` (each section takes its
    /// whole path: connected sections stay in one circuit) into circuit
    /// `circuit` (id or name; none: into no circuit), out of the circuit
    /// they were in. Refused when the circuit would then have two
    /// conductor paths of one phase that are not connected.
    pub fn assign_to_circuit(
        &mut self,
        sections: &[String],
        circuit: Option<&str>,
    ) -> Result<(), String> {
        let name = match circuit {
            None => String::new(),
            Some(c) => self
                .circuit(c)
                .ok_or_else(|| format!("no circuit {c}"))?
                .name
                .clone(),
        };
        let paths = crate::electrical::conductor_paths(self);
        let mut take = BTreeSet::new();
        for id in sections {
            let p = paths
                .iter()
                .find(|p| p.contains(id))
                .ok_or_else(|| format!("no section {id}"))?;
            take.extend(p.iter().cloned());
        }
        let phases = self.section_phases().unwrap_or_default();
        let target = self.circuits.iter().find(|c| c.name == name).cloned();
        let old = target
            .as_ref()
            .map(|c| self.unconnected_phases(c, &phases))
            .unwrap_or_default();
        for s in self.sections.iter_mut().filter(|s| take.contains(&s.id)) {
            s.circuit_name = name.clone();
        }
        if let Some(c) = target {
            let new: Vec<String> = self
                .unconnected_phases(&c, &phases)
                .into_iter()
                .filter(|x| !old.contains(x))
                .collect();
            if !new.is_empty() {
                return Err(new.join("; "));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line() -> Line {
        Line::from_json(include_str!(
            "../../../examples/synthetic-line/synthetic-line.ohl.json"
        ))
        .unwrap()
    }

    /// The example line (version 6, names on its sections only) is read
    /// into one circuit per name, the sections' names unchanged. Its
    /// branch S3 is tapped on S1 by a jumper: one conductor path with two
    /// names, as the file has it (noted, not changed).
    #[test]
    fn names_on_sections_are_read_into_circuits() {
        let l = line();
        let ids: Vec<(&str, &str)> = l
            .circuits
            .iter()
            .map(|c| (c.id.as_str(), c.name.as_str()))
            .collect();
        assert_eq!(ids, [("Example 1", "Example 1"), ("Branch", "Branch")]);
        let names: Vec<&str> = l.sections.iter().map(|s| s.circuit_name.as_str()).collect();
        assert_eq!(names, ["Example 1", "Example 1", "Branch"]);
        let members = l.circuit_members("Example 1");
        assert_eq!(members.len(), 2);
        assert_eq!(members[0].sections, ["S1", "S3"]);
        assert_eq!(members[1].sections, ["S2"]);
        let notes = l.circuit_notes();
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("several circuits"), "{notes:?}");
        // Written and read again: the same.
        let again = Line::from_json(&l.to_json()).unwrap();
        assert_eq!(again, l);
        assert!(l.to_json().contains("\"circuits\""));
    }

    /// Every example line: its sections' circuit names as in the file (what
    /// everything grouped by circuit reads, so results are the same), one
    /// circuit per distinct name, its id the name, no system.
    #[test]
    fn example_lines_migrate_unchanged() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line");
        let mut n = 0;
        for e in std::fs::read_dir(&dir).unwrap() {
            let path = e.unwrap().path();
            if !path.to_string_lossy().ends_with(".ohl.json") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            let raw: serde_json::Value = serde_json::from_str(&text).unwrap();
            let names: Vec<String> = raw["sections"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s["circuit_name"].as_str().unwrap_or("").to_owned())
                .collect();
            let l = Line::from_json(&text).unwrap();
            let read: Vec<String> = l.sections.iter().map(|s| s.circuit_name.clone()).collect();
            assert_eq!(read, names, "{}", path.display());
            let mut distinct: Vec<String> = Vec::new();
            for x in names.into_iter().filter(|x| !x.is_empty()) {
                if !distinct.contains(&x) {
                    distinct.push(x);
                }
            }
            let circuits: Vec<String> = l.circuits.iter().map(|c| c.name.clone()).collect();
            assert_eq!(circuits, distinct, "{}", path.display());
            assert!(
                l.circuits
                    .iter()
                    .all(|c| c.id == c.name && c.system.is_none())
            );
            n += 1;
        }
        assert!(n >= 3);
    }

    #[test]
    fn an_id_taken_gets_a_suffix() {
        let mut l = line();
        l.circuits = vec![Circuit::new("Branch", "Other")];
        l.adopt_circuits();
        let ids: Vec<&str> = l.circuits.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["Branch", "Example 1", "Branch-2"]);
    }

    /// The example with its branch not tapped (and of the system):
    /// three conductor paths, S1 L1, S2 L2, S3 L1.
    fn untapped() -> Line {
        let mut l = line();
        l.jumpers.clear();
        l.sections[2].system = Some("ex220".into());
        l
    }

    #[test]
    fn rename_delete_and_assign() {
        let before = untapped();
        assert!(
            before.circuit_notes().is_empty(),
            "{:?}",
            before.circuit_notes()
        );
        let mut l = before.clone();
        l.rename_circuit("Example 1", "North").unwrap();
        assert_eq!(l.sections[0].circuit_name, "North");
        assert_eq!(l.circuit("Example 1").unwrap().name, "North");
        l.check_circuit_change(&before).unwrap();

        let mut d = before.clone();
        d.delete_circuit("Branch").unwrap();
        assert!(d.sections[2].circuit_name.is_empty());

        let mut a = before.clone();
        a.add_circuit(Circuit::new("c2", "South")).unwrap();
        assert!(a.add_circuit(Circuit::new("c3", "South")).is_err());
        assert!(a.add_circuit(Circuit::new("c2", "West")).is_err());
        // Empty: allowed, noted.
        assert_eq!(a.circuit_notes().len(), 1);
        a.assign_to_circuit(&["S3".into()], Some("South")).unwrap();
        assert_eq!(a.circuit_members("South")[0].sections, ["S3"]);
        // Branch is empty now.
        assert!(a.circuit_notes()[0].contains("Branch"));
        a.assign_to_circuit(&["S3".into()], None).unwrap();
        assert!(a.circuit_members("South").is_empty());
        assert!(
            a.assign_to_circuit(&["S3".into()], Some("nowhere"))
                .is_err()
        );
        // Another phase into Example 1 (L1 and L2 there): allowed.
        let mut p = before.clone();
        p.sections[2].phase_label = "L3".into();
        p.assign_to_circuit(&["S3".into()], Some("Example 1"))
            .unwrap();
        assert_eq!(p.circuit_members("Example 1").len(), 3);
    }

    /// Grouping two conductor paths of one phase that are not connected is
    /// refused, by assigning or by renaming; connecting them with a jumper
    /// makes them one path, in one circuit.
    #[test]
    fn unconnected_paths_are_not_grouped_until_connected() {
        let before = untapped();
        let mut l = before.clone();
        // S3 is L1 of ex220, as S1 in Example 1.
        let err = l
            .assign_to_circuit(&["S3".into()], Some("Example 1"))
            .unwrap_err();
        assert!(err.contains("not connected"), "{err}");
        let mut r = before.clone();
        let err = r.rename_circuit("Branch", "Example 1").unwrap_err();
        assert!(err.contains("not merged"), "{err}");
        // Connected by the jumper of the example: one conductor path.
        let mut j = before.clone();
        j.jumpers = line().jumpers;
        let paths = crate::electrical::conductor_paths(&j);
        assert!(paths.contains(&vec!["S1".to_owned(), "S3".to_owned()]));
        // Named as one (what the jumper's window does): no contradiction;
        // Branch is left with none (noted).
        j.assign_to_circuit(&["S3".into()], Some("Example 1"))
            .unwrap();
        assert_eq!(j.circuit_conflicts(), Vec::<String>::new());
        assert!(j.circuit_notes()[0].contains("Branch"));
        j.check_circuit_change(&before).unwrap();
    }

    /// A conductor path in two circuits, a member of another system, a
    /// repeated or padded name, an unknown system: refused.
    #[test]
    fn contradictions_are_refused() {
        let before = untapped();
        // S2 and S3 joined (a shared wire point): one path, two circuits.
        let mut l = before.clone();
        let p = l.sections[1].points.last().unwrap().clone();
        l.sections[2].points[0] = p;
        l.sections[2].phase_label = "L2".into();
        let err = l.check_circuit_change(&before).unwrap_err();
        assert!(err.contains("several circuits"), "{err}");
        // A line that had it already: noted, not refused again.
        l.check_circuit_change(&l.clone()).unwrap();

        let mut s = untapped();
        s.sections[2].system = None;
        let before_s = s.clone();
        s.circuits[1].system = Some("ex220".into());
        let err = s.check_circuit_change(&before_s).unwrap_err();
        assert!(err.contains("earthed"), "{err}");

        let mut n = before.clone();
        n.circuits[1].name = "Example 1".into();
        assert!(n.check_circuit_change(&before).is_err());
        n.circuits[1].name = " Branch".into();
        assert!(n.check_circuit_change(&before).is_err());
        let mut u = before.clone();
        u.circuits[1].system = Some("nowhere".into());
        assert!(u.validate().is_err());
    }
}
