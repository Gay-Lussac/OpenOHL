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
//! **A load case's own list** (owner, 2026-11-15: "manual should always
//! be available where load cases rules are defined and on which pylon
//! they apply with a list of them. Not per type but per position"): a
//! group with [`StructureGroup::load_case`] set belongs to that case and
//! is edited where the case is defined (Load cases… › Applies to). Its
//! picked structures take the case **on top of** what their groups give;
//! its **excluded** structures do not take it, whatever their groups give
//! (an exclusion is said wherever the structure's cases are listed). It
//! has no rules and no other case, and it does not count as a group for
//! "in no group: every case": it only ever adds its case, or takes away
//! exactly that case, never more (the safe side; [`Line::give_case`]).
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
    /// Its name, unique in the line (a load case's own list has none: it
    /// is named after its case, [`StructureGroup::label`]).
    #[serde(default, skip_serializing_if = "String::is_empty")]
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
    /// Set: the group is this load case's **own list** (2026-11-15, the
    /// Load cases form's *Applies to*; at most one per case). Its picked
    /// structures take this case on top of their groups' cases; it has no
    /// rules and no `load_cases`, and does not count as a group for "in
    /// no group: every case".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_case: Option<String>,
    /// A load case's own list only: the structures that do **not** take
    /// its case, whatever their groups give (said wherever their cases are
    /// listed; an id not in the line is left out).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded: Vec<String>,
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

    /// Load case `case`'s own list, holding nothing.
    pub fn own_list(case: &str) -> Self {
        Self {
            load_case: Some(case.to_owned()),
            ..Self::default()
        }
    }

    /// Whether it is a load case's own list ([`StructureGroup::load_case`]).
    pub fn is_own_list(&self) -> bool {
        self.load_case.is_some()
    }

    /// Its name as shown: its name, or for a load case's own list
    /// `wind (own list)`.
    pub fn label(&self) -> String {
        match &self.load_case {
            Some(c) => format!("{c} (own list)"),
            None => self.name.clone(),
        }
    }

    /// The load cases it gives: those ticked, or a load case's own list's
    /// case.
    pub fn cases(&self) -> Vec<String> {
        match &self.load_case {
            Some(c) => vec![c.clone()],
            None => self.load_cases.clone(),
        }
    }

    /// Whether structure `s` (with its `facts`) is in the group: picked,
    /// or every rule holds (a load case's own list: picked only).
    pub fn holds(&self, s: &Structure, facts: &Facts) -> bool {
        self.structures.contains(&s.id) || (!self.is_own_list() && self.rules.holds(s, facts))
    }

    /// Whether this group's rules need the stations and line angles.
    fn needs_stations(&self) -> bool {
        self.rules.needs_stations()
    }
}

/// The load cases one structure takes ([`CaseSets::of`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum CaseSet {
    /// In no group: every case of the library.
    #[default]
    Every,
    /// In no group, excluded from these cases in their own lists: every
    /// case of the library but these.
    EveryBut(Vec<String>),
    /// These cases (the union of its groups' and the cases whose own list
    /// picks it, in their order of first appearance, less those it is
    /// excluded from).
    These(Vec<String>),
    /// In groups ticking no case only (or excluded from each of their
    /// cases): none (no loads, no check).
    NoCase,
}

impl CaseSet {
    /// Whether it takes load case `case`.
    pub fn takes(&self, case: &str) -> bool {
        match self {
            CaseSet::Every => true,
            CaseSet::EveryBut(c) => !c.iter().any(|x| x == case),
            CaseSet::These(c) => c.iter().any(|x| x == case),
            CaseSet::NoCase => false,
        }
    }

    /// The names it gives (empty: every case — all, or all but some —,
    /// or none).
    pub fn names(&self) -> &[String] {
        match self {
            CaseSet::These(c) => c,
            _ => &[],
        }
    }
}

/// How one group gives a structure a load case ([`CaseWhy::by`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupWhy {
    /// The group's name ([`StructureGroup::label`]).
    pub group: String,
    /// The structure is picked in it.
    pub picked: bool,
    /// The group's rules hold for it.
    pub rules: bool,
}

/// How a structure gets a load case, or why it does not
/// ([`CaseSets::why`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CaseWhy {
    /// It takes the case.
    pub takes: bool,
    /// It is in no group (so it takes every case, less its exclusions).
    pub no_group: bool,
    /// Its groups ticking the case.
    pub by: Vec<GroupWhy>,
    /// The case's own list picks it.
    pub picked_here: bool,
    /// The case's own list excludes it.
    pub excluded_here: bool,
    /// Every group it is in (its names), the case's own list left out.
    pub groups: Vec<String>,
}

impl CaseWhy {
    /// Whether it gets the case without the case's own list: in no
    /// group, or a group of its ticks the case.
    pub fn by_groups(&self) -> bool {
        self.no_group || !self.by.is_empty()
    }
}

/// One structure's groups and cases ([`CaseSets`]).
#[derive(Debug, Clone, Default, PartialEq)]
struct Entry {
    /// Its groups (indices in [`Line::structure_groups`], own lists left
    /// out): picked in it, its rules hold.
    groups: Vec<(usize, bool, bool)>,
    /// Its cases.
    set: CaseSet,
    /// The cases whose own list picks it.
    picked_for: Vec<String>,
    /// The cases whose own list excludes it (and does not pick it).
    excluded_from: Vec<String>,
}

/// Every structure's groups and load cases, resolved once from the line
/// ([`Line::case_sets`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CaseSets {
    /// Per structure (by id).
    by_structure: BTreeMap<String, Entry>,
    /// The structures, in the line's order.
    order: Vec<String>,
    /// The groups' names ([`StructureGroup::label`]), in their order.
    names: Vec<String>,
    /// Per group, the cases it gives ([`StructureGroup::cases`]).
    cases: Vec<Vec<String>>,
    /// Per group, the case it is the own list of.
    own: Vec<Option<String>>,
    /// Per group, its structures (an own list: those it picks), in the
    /// line's order.
    members: Vec<Vec<String>>,
    /// Per group, the structures it excludes (own lists), in the line's
    /// order.
    excluded: Vec<Vec<String>>,
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
        self.by_structure.get(id).map_or(&EVERY, |x| &x.set)
    }

    /// Whether structure `id` takes load case `case`.
    pub fn takes(&self, id: &str, case: &str) -> bool {
        self.of(id).takes(case)
    }

    /// Whether structure `id` takes no load case at all.
    pub fn takes_none(&self, id: &str) -> bool {
        matches!(self.of(id), CaseSet::NoCase)
    }

    /// The names of the groups structure `id` is in, in their order (the
    /// load cases' own lists left out: [`CaseSets::picked_for`],
    /// [`CaseSets::excluded_from`]).
    pub fn groups_of(&self, id: &str) -> Vec<&str> {
        self.by_structure.get(id).map_or_else(Vec::new, |x| {
            x.groups
                .iter()
                .map(|&(k, _, _)| self.names[k].as_str())
                .collect()
        })
    }

    /// The load cases whose own list picks structure `id`.
    pub fn picked_for(&self, id: &str) -> &[String] {
        self.by_structure
            .get(id)
            .map_or(&[], |x| x.picked_for.as_slice())
    }

    /// The load cases whose own list excludes structure `id` (it does not
    /// take them, whatever its groups give).
    pub fn excluded_from(&self, id: &str) -> &[String] {
        self.by_structure
            .get(id)
            .map_or(&[], |x| x.excluded_from.as_slice())
    }

    /// How structure `id` gets load case `case`, or why it does not.
    pub fn why(&self, id: &str, case: &str) -> CaseWhy {
        let Some(e) = self.by_structure.get(id) else {
            return CaseWhy {
                takes: true,
                no_group: true,
                ..CaseWhy::default()
            };
        };
        CaseWhy {
            takes: e.set.takes(case),
            no_group: e.groups.is_empty(),
            by: e
                .groups
                .iter()
                .filter(|(k, _, _)| self.cases[*k].iter().any(|c| c == case))
                .map(|&(k, picked, rules)| GroupWhy {
                    group: self.names[k].clone(),
                    picked,
                    rules,
                })
                .collect(),
            picked_here: e.picked_for.iter().any(|c| c == case),
            excluded_here: e.excluded_from.iter().any(|c| c == case),
            groups: e
                .groups
                .iter()
                .map(|&(k, _, _)| self.names[k].clone())
                .collect(),
        }
    }

    /// The structures taking load case `case`, in the line's order.
    pub fn takers(&self, case: &str) -> Vec<String> {
        self.order
            .iter()
            .filter(|id| self.takes(id, case))
            .cloned()
            .collect()
    }

    /// The structures of group `k` (its index; a load case's own list:
    /// those it picks), in the line's order.
    pub fn members(&self, k: usize) -> &[String] {
        self.members.get(k).map_or(&[], Vec::as_slice)
    }

    /// The structures group `k` (a load case's own list) excludes, in the
    /// line's order.
    pub fn excluded(&self, k: usize) -> &[String] {
        self.excluded.get(k).map_or(&[], Vec::as_slice)
    }

    /// The index of load case `case`'s own list, if it has one.
    pub fn own_list(&self, case: &str) -> Option<usize> {
        self.own.iter().position(|c| c.as_deref() == Some(case))
    }

    /// The structures in no group (they take every case, less those they
    /// are excluded from), in the line's order.
    pub fn in_no_group(&self) -> &[String] {
        &self.alone
    }

    /// The structures taking no load case, in the line's order.
    pub fn without_load_cases(&self) -> &[String] {
        &self.none
    }

    /// Whether the line has groups (own lists too; else every structure
    /// takes every case).
    pub fn has_groups(&self) -> bool {
        !self.names.is_empty()
    }
}

/// What [`Line::give_case`] did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Given {
    /// Picked in the case's own list (no group of theirs gives it).
    pub picked: Vec<String>,
    /// Taken out of the case's own list's picks.
    pub unpicked: Vec<String>,
    /// Excluded in the case's own list, with how they got the case
    /// before (in no group, or through which groups — by rules or picked).
    pub excluded: Vec<(String, CaseWhy)>,
    /// Their exclusion taken away.
    pub unexcluded: Vec<String>,
}

impl Given {
    /// Nothing done (they already took, or did not take, the case).
    pub fn is_empty(&self) -> bool {
        self.picked.is_empty()
            && self.unpicked.is_empty()
            && self.excluded.is_empty()
            && self.unexcluded.is_empty()
    }
}

impl Line {
    /// What the rules of the groups read of every structure (by id): its
    /// line angles and stations on the alignments (computed only when a
    /// group's rule needs them) and whether a section ends at it.
    pub fn structure_facts(&self) -> BTreeMap<String, Facts> {
        self.facts(
            self.structure_groups
                .iter()
                .any(StructureGroup::needs_stations),
        )
    }

    /// [`Line::structure_facts`] with the stations and line angles always
    /// computed (to show them: the Load cases form's *Applies to*).
    /// Expected cost: the stations of every alignment (milliseconds for a
    /// long line).
    pub fn structure_facts_with_stations(&self) -> BTreeMap<String, Facts> {
        self.facts(true)
    }

    /// The facts, with the stations when `stations`.
    fn facts(&self, stations: bool) -> BTreeMap<String, Facts> {
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
        if stations {
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
    /// ticking none only, none — then the load cases' own lists: a case
    /// whose own list picks it added (in no group it has every case
    /// already), a case whose own list excludes it taken away (in no
    /// group: [`CaseSet::EveryBut`]). A line still holding the sets of before
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
            names: groups.iter().map(StructureGroup::label).collect(),
            cases: groups.iter().map(StructureGroup::cases).collect(),
            own: groups.iter().map(|g| g.load_case.clone()).collect(),
            members: vec![Vec::new(); groups.len()],
            excluded: vec![Vec::new(); groups.len()],
            ..CaseSets::default()
        };
        let none = Facts::default();
        for s in &self.structures {
            if out.by_structure.contains_key(&s.id) {
                continue;
            }
            let f = facts.get(&s.id).unwrap_or(&none);
            let mut e = Entry::default();
            for (k, g) in groups.iter().enumerate() {
                if let Some(case) = &g.load_case {
                    if g.structures.contains(&s.id) {
                        out.members[k].push(s.id.clone());
                        if !e.picked_for.contains(case) {
                            e.picked_for.push(case.clone());
                        }
                    }
                    if g.excluded.contains(&s.id) {
                        out.excluded[k].push(s.id.clone());
                        if !e.excluded_from.contains(case) {
                            e.excluded_from.push(case.clone());
                        }
                    }
                    continue;
                }
                let picked = g.structures.contains(&s.id);
                let rules = g.rules.holds(s, f);
                if picked || rules {
                    out.members[k].push(s.id.clone());
                    e.groups.push((k, picked, rules));
                }
            }
            // Picked and excluded for one case (refused when read,
            // `Line::groups_problem`): picked — more cases, the safe side.
            let picked_for = e.picked_for.clone();
            e.excluded_from.retain(|c| !picked_for.contains(c));
            e.set = if e.groups.is_empty() {
                // An own list's pick does not count as a group: every
                // case, less the exclusions.
                out.alone.push(s.id.clone());
                if e.excluded_from.is_empty() {
                    CaseSet::Every
                } else {
                    CaseSet::EveryBut(e.excluded_from.clone())
                }
            } else {
                let mut cases: Vec<String> = Vec::new();
                for c in e
                    .groups
                    .iter()
                    .flat_map(|&(k, _, _)| &groups[k].load_cases)
                    .chain(&e.picked_for)
                {
                    if !cases.contains(c) && !e.excluded_from.contains(c) {
                        cases.push(c.clone());
                    }
                }
                if cases.is_empty() {
                    out.none.push(s.id.clone());
                    CaseSet::NoCase
                } else {
                    CaseSet::These(cases)
                }
            };
            out.order.push(s.id.clone());
            out.by_structure.insert(s.id.clone(), e);
        }
        out
    }

    /// Structures `ids` made to take load case `case` (`take`), or not to,
    /// through the case's **own list** ([`StructureGroup::load_case`],
    /// made when needed; taken away when left empty). Per structure, from
    /// how it gets the case now ([`CaseSets::why`]):
    ///
    /// - to take it: its exclusion is taken away; picked in the own list
    ///   when no group of its gives the case;
    /// - not to take it: taken out of the own list's picks; **excluded**
    ///   when it still gets the case otherwise (in no group, or a group
    ///   giving it — by rules or picked there): exactly this case leaves,
    ///   none other (the safe side), said wherever its cases are listed.
    ///
    /// Its groups and their rules are never changed. What was done
    /// ([`Given`]; empty when they already did, or did not, take it). A
    /// structure not in the line is left out.
    pub fn give_case(&mut self, case: &str, ids: &[String], take: bool) -> Given {
        let sets = self.case_sets();
        let k = match self
            .structure_groups
            .iter()
            .position(|g| g.load_case.as_deref() == Some(case))
        {
            Some(k) => k,
            None => {
                self.structure_groups.push(StructureGroup::own_list(case));
                self.structure_groups.len() - 1
            }
        };
        let mut given = Given::default();
        for id in ids {
            if self.structure(id).is_none() {
                continue;
            }
            let why = sets.why(id, case);
            let g = &mut self.structure_groups[k];
            if take {
                if why.takes {
                    continue;
                }
                if g.excluded.contains(id) {
                    g.excluded.retain(|x| x != id);
                    given.unexcluded.push(id.clone());
                }
                if !why.by_groups() && !g.structures.contains(id) {
                    g.structures.push(id.clone());
                    given.picked.push(id.clone());
                }
            } else {
                if !why.takes {
                    continue;
                }
                if g.structures.contains(id) {
                    g.structures.retain(|x| x != id);
                    given.unpicked.push(id.clone());
                }
                if why.by_groups() && !g.excluded.contains(id) {
                    g.excluded.push(id.clone());
                    given.excluded.push((id.clone(), why));
                }
            }
        }
        let g = &self.structure_groups[k];
        if g.structures.is_empty() && g.excluded.is_empty() && g.description.is_empty() {
            self.structure_groups.remove(k);
        }
        given
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
                !x.is_own_list()
                    && x.structures == g.structures
                    && x.rules == g.rules
                    && x.load_cases == g.load_cases
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
        let mut own: Vec<&str> = Vec::new();
        for g in &self.structure_groups {
            if let Some(case) = &g.load_case {
                let case = case.as_str();
                if case.trim().is_empty() {
                    return Some("a load case's own list without its case's name".into());
                }
                if own.contains(&case) {
                    return Some(format!("load case \"{case}\": two own lists"));
                }
                own.push(case);
                if !g.rules.is_empty() || !g.load_cases.is_empty() {
                    return Some(format!(
                        "load case \"{case}\"'s own list: it holds picked and excluded structures \
                         only (no rules, no other case: make a group for them)"
                    ));
                }
                if let Some(id) = g.structures.iter().find(|s| g.excluded.contains(s)) {
                    return Some(format!(
                        "load case \"{case}\"'s own list: {id} both picked and excluded"
                    ));
                }
                continue;
            }
            if !g.excluded.is_empty() {
                return Some(format!(
                    "structure group \"{}\": only a load case's own list excludes structures",
                    g.name
                ));
            }
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
