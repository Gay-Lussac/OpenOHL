// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Structure groups**: which load cases each structure takes (owner,
//! 2026-11-15: "most of the features here are doubled with simply
//! defining load cases and then have them attributed in groups of
//! pylons"; roadmap O38).
//!
//! A group ([`StructureGroup`]) has a name, a description, the structures
//! **picked** by hand and **rules** ([`GroupRules`]: types, line angle
//! from–to, station from–to on an alignment, dead end or not — all of
//! them must hold), and the load cases of the line's load case library
//! its structures take. Membership is live: the rules are read from the
//! line as it is (move a structure past 10° and its groups change).
//!
//! **Which cases a structure takes** ([`Line::case_sets`], [`CaseSets`]):
//! the union of the cases of every group it is in (two groups: both
//! groups' cases — more cases, the safe side); in no group: every case of
//! the library; in groups that tick no case only: none (no loads, no
//! check, said wherever loads and checks are listed per structure).
//!
//! Before 2026-11-15 a structure gave its own set (`load_cases`,
//! `no_load_cases`) and a structure type its default set
//! (`structure_loads.type_load_cases`), taken in that order. Such a line
//! is read into groups ([`Line::adopt_structure_groups`]): a group per
//! type set (rule: that type), a group per distinct own set (picking its
//! structures, named after them) and a group *no load case* for the
//! structures that took none; each group's description says where it
//! came from. With the union, a structure that had its own set and whose
//! type had one now takes both (on the safe side).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Line, Structure};

/// Tolerance of the bounds of the rules (inclusive): degrees and metres.
const TOLERANCE: f64 = 1e-6;

/// A group of structures and the load cases they take
/// ([`Line::structure_groups`], line file version 18).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructureGroup {
    /// Its name, unique in the line.
    pub name: String,
    /// What it is for, in words.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// The structures picked by hand, by id (in the group whatever the
    /// rules say; an id not in the line is left out).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub structures: Vec<String>,
    /// The rules: a structure for which every rule given holds is in the
    /// group (none given: only the picked structures).
    #[serde(default, skip_serializing_if = "GroupRules::is_empty")]
    pub rules: GroupRules,
    /// The load cases its structures take: names of cases of the line's
    /// load case library. Empty: its structures take none from it (no
    /// loads, no check — unless another group of theirs gives some).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub load_cases: Vec<String>,
}

/// The rules of a [`StructureGroup`]: each one given must hold (bounds
/// inclusive); none given: no structure by rule.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupRules {
    /// Its structure type is one of these (by id); empty: any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<String>,
    /// The line angle there, its size (left or right), at least this many
    /// degrees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle_from_deg: Option<f64>,
    /// The same, at most this many degrees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle_to_deg: Option<f64>,
    /// On this alignment (by id); with a station bound, the station is
    /// on it. None: any alignment the structure stands on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<String>,
    /// Its station at least this, m.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub station_from: Option<f64>,
    /// Its station at most this, m.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub station_to: Option<f64>,
    /// A dead end (true: a section of the line ends at it) or not (false);
    /// none: either. Read from the line, not from the type's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dead_end: Option<bool>,
}

impl GroupRules {
    /// No rule given.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Whether a rule needs the stations and line angles (the alignments).
    fn needs_stations(&self) -> bool {
        self.angle_from_deg.is_some()
            || self.angle_to_deg.is_some()
            || self.alignment.is_some()
            || self.station_from.is_some()
            || self.station_to.is_some()
    }

    /// Whether every rule given holds for structure `s` with its `facts`
    /// (none given: false).
    pub fn holds(&self, s: &Structure, facts: &Facts) -> bool {
        if self.is_empty() {
            return false;
        }
        if !self.types.is_empty()
            && !s
                .structure_type
                .as_deref()
                .is_some_and(|t| self.types.iter().any(|x| x == t))
        {
            return false;
        }
        if self.angle_from_deg.is_some() || self.angle_to_deg.is_some() {
            let Some(a) = facts.angle_deg else {
                return false;
            };
            if !within(a.abs(), self.angle_from_deg, self.angle_to_deg) {
                return false;
            }
        }
        if self.alignment.is_some() || self.station_from.is_some() || self.station_to.is_some() {
            let on = facts.stations.iter().any(|(alignment, station)| {
                self.alignment.as_deref().is_none_or(|a| a == alignment)
                    && within(*station, self.station_from, self.station_to)
            });
            if !on {
                return false;
            }
        }
        if let Some(d) = self.dead_end
            && d != facts.dead_end
        {
            return false;
        }
        true
    }

    /// The rules in words (`types 1A, 1B; line angle 10–30°; dead end`);
    /// empty: none. `alignment_name` gives an alignment's name from its
    /// id.
    pub fn words(&self, alignment_name: impl Fn(&str) -> String) -> String {
        let mut out = Vec::new();
        if !self.types.is_empty() {
            out.push(format!("type {}", self.types.join(", ")));
        }
        let bounds = |from: Option<f64>, to: Option<f64>, unit: &str| match (from, to) {
            (Some(a), Some(b)) => format!("{}–{}{unit}", num(a), num(b)),
            (Some(a), None) => format!("≥ {}{unit}", num(a)),
            (None, Some(b)) => format!("≤ {}{unit}", num(b)),
            (None, None) => String::new(),
        };
        if self.angle_from_deg.is_some() || self.angle_to_deg.is_some() {
            out.push(format!(
                "line angle {}",
                bounds(self.angle_from_deg, self.angle_to_deg, "°")
            ));
        }
        if self.alignment.is_some() || self.station_from.is_some() || self.station_to.is_some() {
            let on = self
                .alignment
                .as_deref()
                .map_or("any alignment".to_owned(), &alignment_name);
            if self.station_from.is_some() || self.station_to.is_some() {
                out.push(format!(
                    "station {} on {on}",
                    bounds(self.station_from, self.station_to, " m")
                ));
            } else {
                out.push(format!("on {on}"));
            }
        }
        match self.dead_end {
            Some(true) => out.push("dead end".into()),
            Some(false) => out.push("not a dead end".into()),
            None => {}
        }
        out.join("; ")
    }

    /// What is wrong with the rules (a bound from above its to, a number
    /// not finite), in words.
    pub fn problem(&self) -> Option<String> {
        for (what, from, to) in [
            ("line angle", self.angle_from_deg, self.angle_to_deg),
            ("station", self.station_from, self.station_to),
        ] {
            if from.into_iter().chain(to).any(|v| !v.is_finite()) {
                return Some(format!("{what}: give numbers"));
            }
            if let (Some(a), Some(b)) = (from, to)
                && a > b
            {
                return Some(format!("{what}: from {} is above to {}", num(a), num(b)));
            }
        }
        if self
            .angle_from_deg
            .into_iter()
            .chain(self.angle_to_deg)
            .any(|a| !(0.0..=180.0).contains(&a))
        {
            return Some("line angle: between 0 and 180°".into());
        }
        None
    }
}

/// A number as the rules' words write it (no trailing zeros).
fn num(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_owned() }
}

/// `v` within `from`..=`to` (each bound optional), with the tolerance.
fn within(v: f64, from: Option<f64>, to: Option<f64>) -> bool {
    from.is_none_or(|f| v >= f - TOLERANCE) && to.is_none_or(|t| v <= t + TOLERANCE)
}

/// What the rules read of one structure, from the line as it is.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Facts {
    /// The size of the line angle there, degrees (the largest over the
    /// alignments whose run it is on); none: on no alignment's run.
    pub angle_deg: Option<f64>,
    /// Its station on each alignment it stands on: (alignment id, m).
    pub stations: Vec<(String, f64)>,
    /// A section of the line ends at it.
    pub dead_end: bool,
}

impl StructureGroup {
    /// A group of that name holding nothing, taking no case.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            ..Self::default()
        }
    }

    /// Whether structure `s` (with its `facts`) is in the group: picked,
    /// or every rule holds.
    pub fn holds(&self, s: &Structure, facts: &Facts) -> bool {
        self.structures.contains(&s.id) || self.rules.holds(s, facts)
    }

    /// Whether this group's rules need the stations and line angles.
    fn needs_stations(&self) -> bool {
        self.rules.needs_stations()
    }
}

/// The load cases one structure takes ([`CaseSets::of`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaseSet {
    /// In no group: every case of the library.
    Every,
    /// These cases (the union of its groups', in their order of first
    /// appearance).
    These(Vec<String>),
    /// In groups ticking no case only: none (no loads, no check).
    NoCase,
}

impl CaseSet {
    /// Whether it takes load case `case`.
    pub fn takes(&self, case: &str) -> bool {
        match self {
            CaseSet::Every => true,
            CaseSet::These(c) => c.iter().any(|x| x == case),
            CaseSet::NoCase => false,
        }
    }

    /// The names it gives (empty: every case, or none).
    pub fn names(&self) -> &[String] {
        match self {
            CaseSet::These(c) => c,
            _ => &[],
        }
    }
}

/// Every structure's groups and load cases, resolved once from the line
/// ([`Line::case_sets`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CaseSets {
    /// Per structure (by id): the indices of its groups in
    /// [`Line::structure_groups`], and its cases.
    by_structure: BTreeMap<String, (Vec<usize>, CaseSet)>,
    /// The groups' names, in their order.
    names: Vec<String>,
    /// Per group, its structures, in the line's order.
    members: Vec<Vec<String>>,
    /// The structures in no group, in the line's order.
    alone: Vec<String>,
    /// The structures taking no load case, in the line's order.
    none: Vec<String>,
}

/// Taken by a structure not in the line.
static EVERY: CaseSet = CaseSet::Every;

impl CaseSets {
    /// The cases structure `id` takes (one not in the line: every case).
    pub fn of(&self, id: &str) -> &CaseSet {
        self.by_structure.get(id).map_or(&EVERY, |x| &x.1)
    }

    /// Whether structure `id` takes load case `case`.
    pub fn takes(&self, id: &str, case: &str) -> bool {
        self.of(id).takes(case)
    }

    /// Whether structure `id` takes no load case at all.
    pub fn takes_none(&self, id: &str) -> bool {
        matches!(self.of(id), CaseSet::NoCase)
    }

    /// The names of the groups structure `id` is in, in their order.
    pub fn groups_of(&self, id: &str) -> Vec<&str> {
        self.by_structure.get(id).map_or_else(Vec::new, |x| {
            x.0.iter().map(|&k| self.names[k].as_str()).collect()
        })
    }

    /// The structures of group `k` (its index), in the line's order.
    pub fn members(&self, k: usize) -> &[String] {
        self.members.get(k).map_or(&[], Vec::as_slice)
    }

    /// The structures in no group (they take every case), in the line's
    /// order.
    pub fn in_no_group(&self) -> &[String] {
        &self.alone
    }

    /// The structures taking no load case, in the line's order.
    pub fn without_load_cases(&self) -> &[String] {
        &self.none
    }

    /// Whether the line has groups (else every structure takes every
    /// case).
    pub fn has_groups(&self) -> bool {
        !self.names.is_empty()
    }
}

impl Line {
    /// What the rules of the groups read of every structure (by id): its
    /// line angles and stations on the alignments (computed only when a
    /// group's rule needs them) and whether a section ends at it.
    pub fn structure_facts(&self) -> BTreeMap<String, Facts> {
        let mut out: BTreeMap<String, Facts> = self
            .structures
            .iter()
            .map(|s| (s.id.clone(), Facts::default()))
            .collect();
        for sec in &self.sections {
            for p in [sec.points.first(), sec.points.last()]
                .into_iter()
                .flatten()
            {
                if let Some(f) = out.get_mut(&p.structure) {
                    f.dead_end = true;
                }
            }
        }
        if self
            .structure_groups
            .iter()
            .any(StructureGroup::needs_stations)
        {
            for view in self.alignment_views() {
                let Ok(stations) = view.stations(self) else {
                    continue;
                };
                for st in stations {
                    let Some(f) = out.get_mut(&st.structure) else {
                        continue;
                    };
                    f.stations.push((view.id.clone(), st.station));
                    if !view.away.contains(&st.structure) {
                        let a = st.line_angle_deg.abs();
                        f.angle_deg = Some(f.angle_deg.map_or(a, |b: f64| b.max(a)));
                    }
                }
            }
        }
        out
    }

    /// Every structure's groups and load cases ([`CaseSets`]): the union
    /// of the cases of its groups; in no group, every case; in groups
    /// ticking none only, none. A line still holding the sets of before
    /// 2026-11-15 (a structure's own, a type's) is resolved as if read
    /// into groups ([`Line::adopt_structure_groups`]). Expected cost: one
    /// pass over the structures and groups, plus the stations of every
    /// alignment when a rule needs them (milliseconds for a long line).
    pub fn case_sets(&self) -> CaseSets {
        if self.has_old_case_sets() {
            let mut adopted = self.clone();
            adopted.adopt_structure_groups();
            return adopted.case_sets();
        }
        let facts = self.structure_facts();
        let groups = &self.structure_groups;
        let mut out = CaseSets {
            names: groups.iter().map(|g| g.name.clone()).collect(),
            members: vec![Vec::new(); groups.len()],
            ..CaseSets::default()
        };
        let none = Facts::default();
        for s in &self.structures {
            let f = facts.get(&s.id).unwrap_or(&none);
            let mine: Vec<usize> = groups
                .iter()
                .enumerate()
                .filter(|(_, g)| g.holds(s, f))
                .map(|(k, _)| k)
                .collect();
            for &k in &mine {
                out.members[k].push(s.id.clone());
            }
            let set = if mine.is_empty() {
                out.alone.push(s.id.clone());
                CaseSet::Every
            } else {
                let mut cases: Vec<String> = Vec::new();
                for &k in &mine {
                    for c in &groups[k].load_cases {
                        if !cases.contains(c) {
                            cases.push(c.clone());
                        }
                    }
                }
                if cases.is_empty() {
                    out.none.push(s.id.clone());
                    CaseSet::NoCase
                } else {
                    CaseSet::These(cases)
                }
            };
            out.by_structure.entry(s.id.clone()).or_insert((mine, set));
        }
        out
    }

    /// Whether the line still holds load case sets of before 2026-11-15:
    /// a structure's own (`load_cases`, `no_load_cases`) or a type's
    /// (`structure_loads.type_load_cases`).
    pub fn has_old_case_sets(&self) -> bool {
        !self.structure_loads.type_load_cases.is_empty()
            || self
                .structures
                .iter()
                .any(|s| !s.load_cases.is_empty() || s.no_load_cases)
    }

    /// The load case sets of before 2026-11-15 read into groups (owner,
    /// 2026-11-15): a group `type X` per type set (rule: type X), a group
    /// per distinct own set (its structures picked, named after them), a
    /// group `no load case` for the structures that took none — each
    /// group's description saying where it came from; the old fields
    /// emptied. A group the same as one already there (an undone
    /// command putting an older value back) is not added twice. Whether
    /// anything changed.
    pub fn adopt_structure_groups(&mut self) -> bool {
        if !self.has_old_case_sets() {
            return false;
        }
        let mut made: Vec<StructureGroup> = Vec::new();
        for (t, cases) in std::mem::take(&mut self.structure_loads.type_load_cases) {
            made.push(StructureGroup {
                name: format!("type {t}"),
                description: format!(
                    "Made from the load cases of structure type {t} (Load cases per structure \
                     type, before 2026-11-15): every structure of the type"
                ),
                rules: GroupRules {
                    types: vec![t],
                    ..GroupRules::default()
                },
                load_cases: cases,
                ..StructureGroup::default()
            });
        }
        // The own sets: one group per distinct set, in the line's order.
        let mut sets: Vec<(Vec<String>, Vec<String>)> = Vec::new();
        let mut none: Vec<String> = Vec::new();
        for s in &mut self.structures {
            if s.no_load_cases {
                none.push(s.id.clone());
            } else if !s.load_cases.is_empty() {
                let set = std::mem::take(&mut s.load_cases);
                match sets.iter_mut().find(|(c, _)| *c == set) {
                    Some((_, ids)) => ids.push(s.id.clone()),
                    None => sets.push((set, vec![s.id.clone()])),
                }
            }
            s.load_cases.clear();
            s.no_load_cases = false;
        }
        for (cases, ids) in sets {
            let name = match ids.as_slice() {
                [one] => one.clone(),
                [a, b] => format!("{a}, {b}"),
                [a, b, rest @ ..] => format!("{a}, {b} and {} more", rest.len()),
                [] => continue,
            };
            made.push(StructureGroup {
                name,
                description: format!(
                    "Made from the own load cases of {} (Properties › Load cases, before \
                     2026-11-15)",
                    ids.join(", ")
                ),
                structures: ids,
                load_cases: cases,
                ..StructureGroup::default()
            });
        }
        if !none.is_empty() {
            made.push(StructureGroup {
                name: "no load case".into(),
                description: format!(
                    "Made from the structures that took no load case (Properties › Load cases › \
                     none, before 2026-11-15): {}. It ticks no case: they take none, unless \
                     another group of theirs gives some",
                    none.join(", ")
                ),
                structures: none,
                ..StructureGroup::default()
            });
        }
        for mut g in made {
            if self.structure_groups.iter().any(|x| {
                x.structures == g.structures && x.rules == g.rules && x.load_cases == g.load_cases
            }) {
                continue;
            }
            g.name = self.free_group_name(&g.name);
            self.structure_groups.push(g);
        }
        true
    }

    /// `name`, or `name (2)`, `name (3)`… when a group has it.
    pub fn free_group_name(&self, name: &str) -> String {
        let taken = |n: &str| self.structure_groups.iter().any(|g| g.name == n);
        if !taken(name) {
            return name.to_owned();
        }
        (2..)
            .map(|k| format!("{name} ({k})"))
            .find(|n| !taken(n))
            .expect("a free name")
    }

    /// What is wrong with the groups (a name empty or given twice, a rule
    /// that cannot hold), in words.
    pub fn groups_problem(&self) -> Option<String> {
        let mut seen: Vec<&str> = Vec::new();
        for g in &self.structure_groups {
            let name = g.name.trim();
            if name.is_empty() {
                return Some("a structure group without a name".into());
            }
            if seen.contains(&name) {
                return Some(format!("structure group \"{name}\" given twice"));
            }
            seen.push(name);
            if let Some(p) = g.rules.problem() {
                return Some(format!("structure group \"{name}\": {p}"));
            }
        }
        None
    }

    /// The Line › Criteria › Loads for structures settings of before
    /// 2026-11-15 a line still holds, read and no longer used (the
    /// weather cases sent unfactored, the wires' condition for them, the
    /// broken-wire cases sent, the older list of load cases sent): one
    /// note saying so, none when there are none.
    pub fn unused_structure_loads_note(&self) -> Option<String> {
        let sl = &self.structure_loads;
        let mut what = Vec::new();
        if !sl.weather_cases.is_empty() {
            what.push(format!(
                "weather cases sent ({})",
                sl.weather_cases.join(", ")
            ));
        }
        if let Some(c) = &sl.condition {
            what.push(format!("the wires' condition for them ({c})"));
        }
        if !sl.broken_cases.is_empty() {
            what.push(format!(
                "broken-wire cases sent ({})",
                sl.broken_cases.join(", ")
            ));
        }
        if sl.load_cases.is_some() {
            what.push("a list of load cases sent".into());
        }
        (!what.is_empty()).then(|| {
            format!(
                "Loads for structures (Line › Criteria, before 2026-11-15) is gone: this line's \
                 {} are kept as read and no longer used. A placed structure gets its library \
                 cases only (Structures › Load cases per group); an extra load case in \
                 OHL-Structure takes the unfactored forces of any of them.",
                what.join(", ")
            )
        })
    }
}
