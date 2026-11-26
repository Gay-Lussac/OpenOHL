// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **The member sizer** (2026-11-26, roadmap O49 stage 1, the owner's
//! words of that day: "Would it maybe help the joints to also automate the
//! level 3? (choice of members) where there would be a basic choice and
//! then plugins can adapt the values based on norms", then "Okay go ahead
//! with the builder"; built, in active development, not approved yet).
//!
//! For each **member group** of a level 3 structure the sizer proposes the
//! lightest section of the project's section library, in the group's own
//! family (the same shape, the same material: an angle stays an angle),
//! that passes the check of the structure's code and every limit:
//!
//! - **the code's check**: the usage of every member of the group at most
//!   the target (1 by default) in every load case — the loaded code
//!   plugin's (`asce10`, `sia263`, another), or the core's `simple` check
//!   (then NOT ON THE SAFE SIDE, said);
//! - **the code's limits** on the members' sizes (slenderness, flat width
//!   to thickness, least thickness) from the plugin's `member-limits`
//!   export ([`ohl_member_check::limits`]); none known: said;
//! - **the joint-fit limits** ([`joint_fit`]): the leg takes its bolt (the
//!   hole clear of the root fillet at the usual gauge, the code's edge
//!   distance kept to the toe) and the member is long enough for the bolts
//!   its end force needs (the code's end distance and spacing) — a choice
//!   never lands on a member that cannot be connected.
//!
//! **The loop**: the forces of the structure as it is; per group the
//! lightest passing section with those forces (each trial one model and
//! one call of the plugin for every group at once); the forces again with
//! the new sections (forces change with stiffness and weight); until the
//! choice stops changing. A choice that comes back (a cycle): per group
//! the heavier of the cycle's, said; not settled within the rounds: the
//! heavier of the last two, said.
//!
//! **The result** is always a full run of the structure with the chosen
//! sections ([`crate::analyse_structure`]), never estimated forces: each
//! group's usage before and after, its governing case, its weight, its
//! joint-fit problems before and after; every member above the target
//! said. **Never silent**: the sizer proposes, the designer accepts
//! ([`apply`]: only the groups ticked, one edit). A member's section and,
//! where they described the old section, its design data's `thickness`
//! and `width_thickness` (removed, said) are the only things it changes:
//! never a work point, a roll or a group.
//!
//! **Groups** ([`sizing_groups`]): the members of one `group` with one
//! section stay one group; members without a group stay with their mirror
//! images (`[[symmetry]]`) of the same section; the designer may split a
//! group (each member with its mirror images alone).
//!
//! The detailing practice's own plate and bolt rules (O44 L29) plug in at
//! one place ([`practice_fit`]); later stages are in ROADMAP O49.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::time::{Duration, Instant};

use ohl_core::budget::Budget;
use ohl_member_check::limits::{Limit, Limits};
use ohl_sections::SectionLibrary;
use ohl_structure_loads::{LoadCaseRecipe, LoadTable};
use ohl_structures::StructureType;
use ohl_structures_fe::{Model, Solution};
use toml_edit::DocumentMut;

use crate::CaseToRun;

/// The sizer's settings (stored in no file).
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// The largest usage a chosen section may have (1: its capacity).
    pub target: f64,
    /// Keep to the joint-fit limits ([`joint_fit`]).
    pub joint_fit: bool,
    /// The most rounds of forces and choice.
    pub rounds: usize,
    /// Groups split: each member (with its mirror images) sized alone.
    pub split: Vec<String>,
    /// Groups left as they are.
    pub keep: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            target: 1.0,
            joint_fit: true,
            rounds: DEFAULT_ROUNDS,
            split: Vec::new(),
            keep: Vec::new(),
        }
    }
}

/// The most rounds by default.
pub const DEFAULT_ROUNDS: usize = 10;

/// What the sizer works on.
pub struct SizingInput<'a> {
    /// The structure.
    pub structure: &'a StructureType,
    /// Its file's text.
    pub text: &'a str,
    /// Its folder (its plugins and bolt libraries beside it).
    pub dir: Option<&'a Path>,
    /// The project's section library: the only sections proposed.
    pub sections: &'a SectionLibrary,
    /// The load table.
    pub table: &'a LoadTable,
    /// The recipes of its cases.
    pub recipes: &'a [LoadCaseRecipe],
    /// The cases.
    pub cases: &'a [CaseToRun],
    /// The settings.
    pub settings: &'a Settings,
}

/// A member group as the sizer takes it.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// Its name ("a", "a · L50x5" where the group has several sections,
    /// "D1 + 1 mirrored" for members without a group).
    pub name: String,
    /// Its members' ids.
    pub members: Vec<String>,
    /// Their section.
    pub section: String,
    /// The `group` they belong to (none: no group).
    pub group: Option<String>,
}

/// One group's proposal.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupChange {
    /// The group.
    pub group: Group,
    /// Its section now and proposed (the same: kept).
    pub from: String,
    /// Proposed.
    pub to: String,
    /// Its members' length, m.
    pub length: f64,
    /// Their mass before and after, kg (profile mass per metre × length
    /// between joints).
    pub mass: [f64; 2],
    /// Their largest usage before and after (none: not checked), with the
    /// governing case and what governs.
    pub usage: [Option<f64>; 2],
    /// The governing cases.
    pub case: [String; 2],
    /// What governs after.
    pub governs: String,
    /// Joint-fit problems before and after.
    pub fit: [usize; 2],
    /// The lightest section passing without the joint-fit limits, where it
    /// differs from the proposal (the limits' effect).
    pub without_fit: Option<String>,
    /// Why, in words.
    pub why: String,
    /// The design data removed with the change, in words.
    pub removed: Vec<String>,
    /// A member of the group above the target after.
    pub over_target: bool,
    /// Left as it is (not sized: its section not in the library, asked).
    pub not_sized: bool,
}

impl GroupChange {
    /// Changed.
    pub fn changed(&self) -> bool {
        self.from != self.to
    }
}

/// How the loop ended.
#[derive(Debug, Clone, PartialEq)]
pub enum Settled {
    /// The choice stopped changing.
    Settled,
    /// A choice came back: the heavier of the cycle's taken per group,
    /// said.
    Cycle(String),
    /// Not settled within the rounds: the heavier of the last two, said.
    NotSettled(String),
}

/// One round.
#[derive(Debug, Clone, PartialEq)]
pub struct Round {
    /// Groups whose choice changed.
    pub changed: usize,
    /// Trials (one model and one check of every group still open each).
    pub trials: usize,
    /// How long it took.
    pub took: Duration,
}

/// A member above the target in the final run.
#[derive(Debug, Clone, PartialEq)]
pub struct OverTarget {
    /// The member.
    pub member: String,
    /// Its usage.
    pub usage: f64,
    /// The case.
    pub case: String,
    /// What governs.
    pub governs: String,
}

/// The sizer's answer.
#[derive(Debug, Clone, PartialEq)]
pub struct Sizing {
    /// Each group's proposal (changed first, by mass saved).
    pub groups: Vec<GroupChange>,
    /// The rounds.
    pub rounds: Vec<Round>,
    /// How the loop ended.
    pub settled: Settled,
    /// What applies to the whole run (the check, the limits, warnings).
    pub notes: Vec<String>,
    /// Members above the target in the final run.
    pub over_target: Vec<OverTarget>,
    /// The sized members' mass before and after, kg.
    pub mass: [f64; 2],
    /// The largest member usage before and after.
    pub usage: [Option<f64>; 2],
    /// Joint-fit problems before and after (the sized members').
    pub fit: [usize; 2],
    /// The effect of the joint-fit limits in words (groups kept heavier by
    /// them, the mass).
    pub fit_effect: String,
    /// The structure file with every proposal taken.
    pub text: String,
    /// How long the whole run took.
    pub took: Duration,
}

/// A section of the library a group may take.
#[derive(Debug, Clone, PartialEq)]
struct Candidate {
    id: String,
    /// kg/m.
    mass: f64,
    /// m².
    area: f64,
    /// Pa.
    fy: Option<f64>,
}

/// The mass per metre of library section `s`, kg/m (none: no material
/// density known).
fn mass_per_metre(lib: &SectionLibrary, s: &ohl_sections::Section) -> Option<f64> {
    let m = lib.material_of(s)?;
    let a = s.properties().area;
    (m.density > 0.0 && a > 0.0).then_some(a * m.density)
}

/// The family of library section `s`: its shape's kind and its material.
fn family(lib: &SectionLibrary, s: &ohl_sections::Section) -> (String, Option<String>) {
    (s.shape.as_str().to_owned(), lib.material_of(s).map(|m| m.id.clone()))
}

/// The sections of the library in the family of section `id`, the
/// lightest first (then the smaller area, then the id). None: `id` is not
/// in the library, or its mass is not known.
fn candidates(lib: &SectionLibrary, id: &str) -> Option<Vec<Candidate>> {
    let s = lib.section(id)?;
    mass_per_metre(lib, s)?;
    let f = family(lib, s);
    let mut out: Vec<Candidate> = lib
        .sections
        .iter()
        .filter(|x| family(lib, x) == f)
        .filter_map(|x| {
            Some(Candidate {
                id: x.id.clone(),
                mass: mass_per_metre(lib, x)?,
                area: x.properties().area,
                fy: lib.material_of(x).and_then(|m| m.yield_strength),
            })
        })
        .collect();
    out.sort_by(|a, b| {
        a.mass
            .total_cmp(&b.mass)
            .then(a.area.total_cmp(&b.area))
            .then(a.id.cmp(&b.id))
    });
    out.dedup_by(|a, b| a.id == b.id);
    Some(out)
}

/// The member groups of `t` the sizer takes (members with a section,
/// design-checked in `model`, not cables): the members of one `group` with
/// one section; members without a group with their mirror images of the
/// same section. A group named in `split`: each member with its mirror
/// images in that group alone.
pub fn sizing_groups(t: &StructureType, model: &Model, split: &[String]) -> Vec<Group> {
    let checked: HashMap<&str, bool> = model
        .members
        .iter()
        .map(|m| {
            (
                m.id.as_str(),
                m.check && m.kind != ohl_structures_fe::MemberKind::Cable,
            )
        })
        .collect();
    let members: Vec<&ohl_structures::structures::TypeMember> = t
        .members
        .iter()
        .filter(|m| m.section.is_some() && checked.get(m.id.as_str()) == Some(&true))
        .collect();
    let index: HashMap<&str, usize> = members
        .iter()
        .enumerate()
        .map(|(k, m)| (m.id.as_str(), k))
        .collect();
    // Union-find over the members: same group and section (unless split),
    // or mirror images of the same section and group.
    let mut parent: Vec<usize> = (0..members.len()).collect();
    fn root(p: &mut [usize], mut k: usize) -> usize {
        while p[k] != k {
            p[k] = p[p[k]];
            k = p[k];
        }
        k
    }
    let join = |p: &mut Vec<usize>, a: usize, b: usize| {
        let (ra, rb) = (root(p, a), root(p, b));
        if ra != rb {
            p[ra.max(rb)] = ra.min(rb);
        }
    };
    let mut by_group: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for (k, m) in members.iter().enumerate() {
        let (Some(g), Some(s)) = (m.group.as_deref(), m.section.as_deref()) else {
            continue;
        };
        if split.iter().any(|x| x == g) {
            continue;
        }
        match by_group.get(&(g, s)) {
            Some(&first) => join(&mut parent, first, k),
            None => {
                by_group.insert((g, s), k);
            }
        }
    }
    for (k, m) in members.iter().enumerate() {
        for p in ohl_structure_edit::symmetry::member_partners(t, &m.id) {
            if let Some(&j) = index.get(p.as_str())
                && members[j].section == m.section
                && members[j].group == m.group
            {
                join(&mut parent, k, j);
            }
        }
    }
    let mut sets: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for k in 0..members.len() {
        let r = root(&mut parent, k);
        sets.entry(r).or_default().push(k);
    }
    // A group's sections, to name the groups with several.
    let mut sections_of: HashMap<&str, Vec<&str>> = HashMap::new();
    for m in &members {
        if let (Some(g), Some(s)) = (m.group.as_deref(), m.section.as_deref()) {
            let v = sections_of.entry(g).or_default();
            if !v.contains(&s) {
                v.push(s);
            }
        }
    }
    let mut out: Vec<Group> = sets
        .into_values()
        .map(|ks| {
            let first = members[ks[0]];
            let section = first.section.clone().unwrap_or_default();
            let ids: Vec<String> = ks.iter().map(|&k| members[k].id.clone()).collect();
            let name = match first.group.as_deref() {
                Some(g) if split.iter().any(|x| x == g) => format!("{g} · {}", ids[0]),
                Some(g) if sections_of.get(g).is_some_and(|v| v.len() > 1) => {
                    format!("{g} · {section}")
                }
                Some(g) => g.to_owned(),
                None if ids.len() > 1 => format!("{} + {} mirrored", ids[0], ids.len() - 1),
                None => ids[0].clone(),
            };
            Group {
                name,
                members: ids,
                section,
                group: first.group.clone(),
            }
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The design data fields that describe a member's section (removed when
/// the sizer changes it: they would describe the old one).
pub const SECTION_FIELDS: [&str; 2] = ["thickness", "width_thickness"];

/// The `[[member]]` table of `id` in `doc`.
fn member_table<'a>(doc: &'a mut DocumentMut, id: &str) -> Option<&'a mut toml_edit::Table> {
    doc.get_mut("member")?
        .as_array_of_tables_mut()?
        .iter_mut()
        .find(|t| t.get("id").and_then(|v| v.as_str()) == Some(id))
}

/// Member `id`'s section set to `section` in `doc`, the design data fields
/// describing the old section removed; the fields removed.
fn set_section(doc: &mut DocumentMut, id: &str, section: &str) -> Vec<&'static str> {
    let Some(t) = member_table(doc, id) else {
        return Vec::new();
    };
    t.insert("section", toml_edit::value(section));
    let mut removed = Vec::new();
    match t.get_mut("design") {
        Some(toml_edit::Item::Value(toml_edit::Value::InlineTable(d))) => {
            for f in SECTION_FIELDS {
                if d.remove(f).is_some() {
                    removed.push(f);
                }
            }
        }
        Some(toml_edit::Item::Table(d)) => {
            for f in SECTION_FIELDS {
                if d.remove(f).is_some() {
                    removed.push(f);
                }
            }
        }
        _ => {}
    }
    removed
}

/// `doc` with each group of `groups` at its section in `choice` (the ones
/// at their own section untouched): the text, and per group the fields
/// removed.
fn with_choice(
    doc: &DocumentMut,
    groups: &[Group],
    choice: &[String],
) -> (String, Vec<Vec<&'static str>>) {
    let mut doc = doc.clone();
    let mut removed = vec![Vec::new(); groups.len()];
    for (k, g) in groups.iter().enumerate() {
        if choice[k] == g.section {
            continue;
        }
        for id in &g.members {
            for f in set_section(&mut doc, id, &choice[k]) {
                if !removed[k].contains(&f) {
                    removed[k].push(f);
                }
            }
        }
    }
    (doc.to_string(), removed)
}

/// The forces of one structure: its model (sections as in the text) and
/// each case solved.
struct Forces {
    model: Model,
    /// The cases solved (the light count's where it applies).
    cases: Vec<CaseToRun>,
    /// Per case its solution.
    solutions: Vec<Solution>,
    /// Per member the largest |N| / strength over the cases, N.
    axial: Vec<f64>,
    /// Per member: compressed in a case.
    compressed: Vec<bool>,
}

/// A fresh budget for one case: 200 iterations, 30 s, `budget`'s cancel.
fn case_budget(budget: &Budget) -> Budget {
    let b = Budget::new()
        .max_iterations(200)
        .time_limit(Duration::from_secs(30));
    match budget.token() {
        Some(t) => b.cancel_token(t.clone()),
        None => b,
    }
}

/// Cancelled through `budget`.
fn cancelled(budget: &Budget) -> bool {
    budget.token().is_some_and(|t| t.is_cancelled())
}

/// The structure of `text` (its model, the counted self-weight as the run
/// takes it) with every case solved. Err: the model does not build, or a
/// case does not solve (named).
fn forces(input: &SizingInput, text: &str, budget: &Budget) -> Result<Forces, String> {
    use ohl_structures::LibraryItem as _;
    let t = StructureType::parse(Path::new("structure.ohls"), text).map_err(|e| e.message)?;
    let mut model = crate::model_of(&t, text, input.dir, input.sections)?;
    let bolts = input
        .dir
        .map(|d| ohl_structures::bolts::BoltLibrary::beside(d).0)
        .unwrap_or_default();
    let sw = crate::counted_self_weight(&t, text, input.sections, &bolts, None, &model);
    if let Some(f) = sw.factor {
        crate::apply_counted(&mut model, f);
    }
    let cases: Vec<CaseToRun> = if sw.light_ratio < 1.0 {
        crate::run::favourable_light(input.cases, sw.light_ratio)
    } else {
        input.cases.to_vec()
    };
    let solved = crate::in_case_order(cases.len(), crate::threads(cases.len()), |k| {
        let c = &cases[k];
        let self_weight = if model.analysis.self_weight {
            c.self_weight
        } else {
            0.0
        };
        let loads = crate::fe_loads(&model, input.table, &c.name, self_weight)?;
        ohl_structures_fe::analyse(&model, &loads, c.strength, &case_budget(budget))
            .map(|(s, _)| s)
            .map_err(|e| {
                format!(
                    "case {}: did not solve: {}",
                    c.name,
                    ohl_structures_fe::refusal_text(&model, &e)
                )
            })
    });
    let solutions = solved.into_iter().collect::<Result<Vec<_>, String>>()?;
    let n = model.members.len();
    let mut axial = vec![0.0f64; n];
    let mut compressed = vec![false; n];
    for (c, s) in cases.iter().zip(&solutions) {
        let strength = if c.strength > 0.0 { c.strength } else { 1.0 };
        for (k, f) in s.members.iter().enumerate().take(n) {
            axial[k] = axial[k].max(f.axial.abs() / strength);
            if f.axial < 0.0 {
                compressed[k] = true;
            }
        }
    }
    Ok(Forces {
        model,
        cases,
        solutions,
        axial,
        compressed,
    })
}

/// Per member of `model` its largest usage over the solved cases of
/// `forces` (none: not checked or no capacity) — the plugin's check of
/// the model's code (one call), else the core's own.
fn trial_usage(model: &Model, forces: &Forces) -> Result<Vec<Option<f64>>, String> {
    let n = model.members.len();
    let mut out: Vec<Option<f64>> = vec![Some(0.0); n];
    let mut seen = vec![false; n];
    let mut take = |k: usize, u: Option<f64>| {
        if k >= n {
            return;
        }
        seen[k] = true;
        out[k] = match (out[k], u) {
            (Some(a), Some(b)) => Some(if b.is_nan() { f64::INFINITY } else { a.max(b) }),
            _ => None,
        };
    };
    if model.capacity_plugin.is_some() {
        let solved: Vec<ohl_member_check::Solved> = forces
            .cases
            .iter()
            .zip(&forces.solutions)
            .map(|(c, s)| ohl_member_check::Solved {
                name: &c.name,
                strength: c.strength,
                self_weight: if model.analysis.self_weight {
                    c.self_weight
                } else {
                    0.0
                },
                solution: s,
                moved: None,
            })
            .collect();
        match ohl_member_check::check(model, &solved) {
            Some(Ok(c)) => {
                for case in &c.usage {
                    for u in case {
                        take(u.member, u.usage);
                    }
                }
            }
            Some(Err(e)) => return Err(e),
            None => return Err("the code's plugin was not found".into()),
        }
    } else {
        for (c, s) in forces.cases.iter().zip(&forces.solutions) {
            for u in ohl_structures_fe::check::member_usage(model, s, c.strength) {
                take(u.member, u.usage);
            }
        }
    }
    for (k, s) in seen.iter().enumerate() {
        if !s {
            out[k] = None;
        }
    }
    Ok(out)
}

/// The rule a joint-fit problem breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitRule {
    /// The leg does not take its bolt at the usual gauge (the hole not
    /// clear of the root fillet, or the code's edge distance to the toe).
    LegTakesBolt,
    /// The member is not long enough for the bolts its end force needs.
    Length,
    /// The leg takes no bolt of the structure's bolt library.
    NoBolt,
    /// A rule of the detailing practice ([`practice_fit`]).
    Practice,
}

/// One joint-fit problem.
#[derive(Debug, Clone, PartialEq)]
pub struct FitProblem {
    /// The member (its index in the model).
    pub member: usize,
    /// Its end (none: the member as a whole).
    pub end: Option<usize>,
    /// The rule.
    pub rule: FitRule,
    /// In words.
    pub words: String,
}

/// The code's distances for one bolt in one part: end, spacing, edge (m)
/// and the force one bolt passes (N; none: not known).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Distances {
    end: f64,
    spacing: f64,
    edge: f64,
    force: Option<f64>,
}

/// The distances asked, by what they depend on (few distinct keys in a
/// tower: a plugin call each).
type DistanceCache = HashMap<String, Option<Distances>>;

/// Millimetres for the words.
fn mm(x: f64) -> String {
    format!("{:.0}", x * 1000.0)
}

/// The code's distances for a bolt of diameter `d` in a part `t` thick of
/// ultimate strength `fu` (`bolts`: the end's bolts' data; `sheared`,
/// `redundant` as the bolt tools take them): the plugin's
/// `bolt-distances-full` without an analysis, else its `bolt-distances`,
/// else the structure's default minimums (no force known). Cached.
#[allow(clippy::too_many_arguments)]
fn distances(
    model: &Model,
    d: f64,
    t: f64,
    fu: Option<f64>,
    bolts: Option<&ohl_structures_fe::bolt_rules::EndBolts>,
    sheared: bool,
    redundant: bool,
    cache: &mut DistanceCache,
) -> Option<Distances> {
    let key = format!(
        "{d:e}|{t:e}|{fu:?}|{sheared}|{redundant}|{:?}",
        bolts.map(|b| (
            b.shear_planes,
            b.shear_capacity,
            b.shear_capacity_shank,
            b.ultimate,
            b.threads_in_shear,
            &b.grade
        ))
    );
    if let Some(x) = cache.get(&key) {
        return *x;
    }
    let assumed = || {
        let m = &model.bolt_defaults;
        Some(Distances {
            end: m.end * d,
            spacing: m.spacing * d,
            edge: m.edge * d,
            force: bolts.and_then(|b| b.shear_capacity.map(|c| c * f64::from(b.shear_planes))),
        })
    };
    let out = match model.capacity_plugin.as_ref() {
        Some(p) => {
            use ohl_plugins::bolt_distances as dist;
            use ohl_plugins::bolt_distances_full as full;
            let input = full::DistanceInput {
                diameter: d,
                thickness: t,
                ultimate: fu,
                analysis_force: None,
                bolts: Some(full::Bolts {
                    count: 1,
                    shear_planes: bolts.map_or(1, |b| b.shear_planes.max(1)),
                    shear_capacity: bolts.and_then(|b| b.shear_capacity),
                    shear_capacity_shank: bolts.and_then(|b| b.shear_capacity_shank),
                    ultimate: bolts.and_then(|b| b.ultimate),
                    threads_in_shear: bolts.and_then(|b| b.threads_in_shear),
                    grade: bolts.and_then(|b| b.grade.clone()).unwrap_or_default(),
                }),
                sheared_edge: sheared,
                redundant,
                parameters: p
                    .parameters
                    .as_ref()
                    .map(|x| x.1.clone())
                    .unwrap_or_default(),
            };
            match ohl_member_check::joints::minimum_distances_full(p, &input) {
                Ok(Some(x)) => Some(Distances {
                    end: x.end,
                    spacing: x.spacing,
                    edge: x.edge,
                    force: x.bolt_force,
                }),
                Ok(None) => ohl_member_check::joints::minimum_distances(
                    p,
                    &dist::DistanceInput {
                        diameter: d,
                        thickness: t,
                        ultimate: fu,
                        bolt_force: None,
                        sheared_edge: sheared,
                        redundant,
                    },
                )
                .ok()
                .map(|x| Distances {
                    end: x.end,
                    spacing: x.spacing,
                    edge: x.edge,
                    force: None,
                })
                .or_else(assumed),
                Err(_) => assumed(),
            }
        }
        None => assumed(),
    };
    cache.insert(key, out);
    out
}

/// What an end's bolt is fitted with: the bolt's diameter, the part's
/// thickness and ultimate strength, the bolts' data, the bolted leg (width,
/// thickness, root radius; none: not an angle), sheared, redundant.
struct EndBolt {
    d: f64,
    t: f64,
    fu: Option<f64>,
    bolts: Option<ohl_structures_fe::bolt_rules::EndBolts>,
    leg: Option<(f64, f64, f64)>,
    sheared: bool,
    redundant: bool,
}

/// The joint-fit problems of the members `members` of `model` under their
/// largest end forces `axial` (N per member, the strength factor taken
/// out): per end, the leg takes its bolt at the usual gauge (mid-leg: the
/// hole clear of the root fillet, t + r + d/2 from the heel; the code's
/// edge distance to the toe); per member, its length between its joints
/// holds both ends' bolts in one line — n = ⌈N / P⌉ bolts each (P: the
/// force one bolt passes by its code, in this leg), 2 e + (n − 1) s an end
/// (e, s: the code's end distance and spacing). The bolt: the end's own
/// (its data), else OpenOHL's default bolt for the member
/// ([`ohl_member_check::default_bolts`]). Members with neither: not
/// checked (counted in the second value).
pub fn joint_fit(model: &Model, members: &[usize], axial: &[f64]) -> (Vec<FitProblem>, usize) {
    let mut cache = DistanceCache::new();
    joint_fit_cached(model, members, axial, &mut cache)
}

fn joint_fit_cached(
    model: &Model,
    members: &[usize],
    axial: &[f64],
    cache: &mut DistanceCache,
) -> (Vec<FitProblem>, usize) {
    let defaults: HashMap<usize, ohl_member_check::default_bolts::DefaultBolt> =
        ohl_member_check::default_bolts::default_bolts(model)
            .into_iter()
            .map(|d| (d.member, d))
            .collect();
    let mut problems = Vec::new();
    let mut unknown = 0;
    for &k in members {
        let Some(m) = model.members.get(k) else {
            continue;
        };
        let s = &model.sections[m.section];
        let mut ends: Vec<Option<EndBolt>> = Vec::with_capacity(2);
        for e in 0..2 {
            let b = match ohl_structures_fe::bolt_rules::basis(model, k, e, None, None) {
                Some(b) => Some(EndBolt {
                    d: b.diameter,
                    t: b.thickness,
                    fu: b.ultimate,
                    leg: b.leg.map(|(w, t)| (w, t, b.root)),
                    bolts: b.bolts,
                    sheared: b.sheared,
                    redundant: b.redundant,
                }),
                None => match defaults.get(&k) {
                    Some(dflt) => match dflt.diameter {
                        Some(d) => {
                            let leg = s
                                .profile
                                .as_ref()
                                .and_then(|p| ohl_member_check::default_bolts::leg(&p.shape));
                            Some(EndBolt {
                                d,
                                t: m.design.thickness.or(leg.map(|l| l.1)).unwrap_or(0.0),
                                fu: s.ultimate,
                                bolts: None,
                                leg,
                                sheared: ohl_structures_fe::bolt_rules::sheared_edge(
                                    s, None, false,
                                ),
                                redundant: m.design.class
                                    == Some(ohl_structures_fe::design::MemberClass::Redundant),
                            })
                        }
                        None => {
                            if e == 0 {
                                problems.push(FitProblem {
                                    member: k,
                                    end: None,
                                    rule: FitRule::NoBolt,
                                    words: format!("{}: {}", m.id, dflt.why),
                                });
                            }
                            None
                        }
                    },
                    None => {
                        unknown += 1;
                        None
                    }
                },
            };
            ends.push(b);
        }
        let mut length_needed = 0.0;
        let mut length_known = true;
        for (e, b) in ends.iter().enumerate() {
            let Some(b) = b else {
                length_known = false;
                continue;
            };
            let dist = distances(
                model,
                b.d,
                b.t,
                b.fu,
                b.bolts.as_ref(),
                b.sheared,
                b.redundant,
                cache,
            );
            let joint = &model.nodes[if e == 0 { m.i } else { m.j }].id;
            if let (Some((w, t, r)), Some(x)) = (b.leg, dist) {
                let g = w / 2.0;
                let least = t + r + b.d / 2.0;
                if g < least - 1e-9 {
                    problems.push(FitProblem {
                        member: k,
                        end: Some(e),
                        rule: FitRule::LegTakesBolt,
                        words: format!(
                            "{} at {joint}: the {} mm bolt's hole not clear of the root fillet at \
                             mid-leg ({} mm from the heel, at least {} mm)",
                            m.id,
                            mm(b.d),
                            mm(g),
                            mm(least)
                        ),
                    });
                } else if w - g < x.edge - 1e-9 {
                    problems.push(FitProblem {
                        member: k,
                        end: Some(e),
                        rule: FitRule::LegTakesBolt,
                        words: format!(
                            "{} at {joint}: {} mm to the toe at mid-leg, below the code's edge \
                             distance {} mm for the {} mm bolt",
                            m.id,
                            mm(w - g),
                            mm(x.edge),
                            mm(b.d)
                        ),
                    });
                }
            }
            match dist {
                Some(x) => {
                    let n = match x.force {
                        Some(p) if p > 0.0 => {
                            ((axial.get(k).copied().unwrap_or(0.0) / p).ceil()).max(1.0)
                        }
                        _ => 1.0,
                    };
                    length_needed += 2.0 * x.end + (n - 1.0) * x.spacing;
                }
                None => length_known = false,
            }
            problems.extend(practice_fit(model, k, e));
        }
        let length = model.length(m);
        if length_known && length_needed > length + 1e-9 {
            problems.push(FitProblem {
                member: k,
                end: None,
                rule: FitRule::Length,
                words: format!(
                    "{}: {} mm between its joints, its bolts in one line need {} mm (both ends: \
                     the code's end distances and spacing, the bolts its force needs)",
                    m.id,
                    mm(length),
                    mm(length_needed)
                ),
            });
        }
    }
    (problems, unknown)
}

/// **The seam for the detailing practice's rules** (O44 L29, to come; ROADMAP
/// O49 stage 4): the practice's own plate and bolt rules as joint-fit
/// limits on member `member`'s end `end` of `model`. None yet.
pub fn practice_fit(_model: &Model, _member: usize, _end: usize) -> Vec<FitProblem> {
    Vec::new()
}

/// The broken limits of the members of `model` (`compressed`: per member,
/// compressed in a case — a compression limit holds only then).
fn broken(limits: &Limits, compressed: &[bool]) -> HashMap<usize, Vec<Limit>> {
    let mut out: HashMap<usize, Vec<Limit>> = HashMap::new();
    if let Limits::Given { limits, .. } = limits {
        for l in limits {
            if l.compression && !compressed.get(l.member).copied().unwrap_or(false) {
                continue;
            }
            if l.broken() {
                out.entry(l.member).or_default().push(l.clone());
            }
        }
    }
    out
}

/// One round's choice with the forces of `f`.
struct Pick {
    choice: Vec<String>,
    without_fit: Vec<Option<String>>,
    why: Vec<String>,
    trials: usize,
    limits_note: Option<String>,
}

/// Per group the lightest candidate passing with the forces `f` (`index`:
/// the model's member places of each group's members).
#[allow(clippy::too_many_arguments)]
fn pick(
    input: &SizingInput,
    doc: &DocumentMut,
    groups: &[Group],
    lists: &[Option<Vec<Candidate>>],
    current: &[String],
    f: &Forces,
    index: &[Vec<usize>],
    budget: &Budget,
    progress: &(dyn Fn(&str) + Sync),
) -> Result<Pick, String> {
    let s = input.settings;
    let n = groups.len();
    let mut pointer = vec![0usize; n];
    let mut done: Vec<Option<usize>> = vec![None; n];
    let mut no_fit: Vec<Option<usize>> = vec![None; n];
    // The lowest usage among candidates passing the limits, for a group
    // nothing passes: (usage, candidate).
    let mut best: Vec<Option<(f64, usize)>> = vec![None; n];
    let mut open = vec![false; n];
    // The largest strength factor (a candidate whose A·f_y times it is
    // below the force cannot pass any code: skipped).
    let strength = f.cases.iter().map(|c| c.strength).fold(1.0f64, f64::max);
    for g in 0..n {
        let Some(list) = &lists[g] else {
            continue;
        };
        open[g] = true;
        let force = index[g]
            .iter()
            .map(|&k| f.axial.get(k).copied().unwrap_or(0.0))
            .fold(0.0, f64::max);
        while pointer[g] < list.len() {
            let c = &list[pointer[g]];
            match c.fy {
                Some(fy) if c.area * fy * strength * s.target < force => pointer[g] += 1,
                _ => break,
            }
        }
    }
    let mut trials = 0;
    let mut limits_note = None;
    let mut cache = DistanceCache::new();
    loop {
        if cancelled(budget) {
            return Err("cancelled".into());
        }
        let trying: Vec<usize> = (0..n)
            .filter(|&g| open[g] && done[g].is_none() && pointer[g] < lists[g].as_ref().map_or(0, Vec::len))
            .collect();
        if trying.is_empty() {
            break;
        }
        progress(&format!(
            "trial {}: {} group(s) still open",
            trials + 1,
            trying.len()
        ));
        let choice: Vec<String> = (0..n)
            .map(|g| match (&lists[g], done[g]) {
                (Some(l), Some(i)) => l[i].id.clone(),
                (Some(l), None) if pointer[g] < l.len() => l[pointer[g]].id.clone(),
                _ => current[g].clone(),
            })
            .collect();
        let (text, _) = with_choice(doc, groups, &choice);
        let t = {
            use ohl_structures::LibraryItem as _;
            StructureType::parse(Path::new("structure.ohls"), &text).map_err(|e| e.message)?
        };
        let model = crate::model_of(&t, &text, input.dir, input.sections)?;
        let usage = trial_usage(&model, f)?;
        let limits = ohl_member_check::limits::limits(&model)?;
        if let Limits::Unknown(why) = &limits {
            limits_note = Some(why.clone());
        }
        let broken = broken(&limits, &f.compressed);
        let members: Vec<usize> = trying.iter().flat_map(|&g| index[g].clone()).collect();
        // Worked out with the limits off too: their effect is said.
        let (fit, _) = joint_fit_cached(&model, &members, &f.axial, &mut cache);
        let mut misfit: HashMap<usize, usize> = HashMap::new();
        for p in &fit {
            *misfit.entry(p.member).or_default() += 1;
        }
        for &g in &trying {
            let worst = index[g].iter().try_fold(0.0f64, |w, &k| {
                usage.get(k).copied().flatten().map(|u| w.max(u))
            });
            let ok_usage = worst.is_some_and(|u| u <= s.target * (1.0 + 1e-12));
            let ok_limits = index[g].iter().all(|k| !broken.contains_key(k));
            let ok_fit = index[g].iter().all(|k| !misfit.contains_key(k));
            if ok_usage && ok_limits && no_fit[g].is_none() {
                no_fit[g] = Some(pointer[g]);
            }
            if ok_limits
                && (ok_fit || !s.joint_fit)
                && let Some(u) = worst
                && best[g].is_none_or(|b| u < b.0)
            {
                best[g] = Some((u, pointer[g]));
            }
            if ok_usage && ok_limits && (ok_fit || !s.joint_fit) {
                done[g] = Some(pointer[g]);
            } else {
                pointer[g] += 1;
            }
        }
        trials += 1;
    }
    let mut out = current.to_vec();
    let mut why = vec![String::new(); n];
    let mut without_fit = vec![None; n];
    for g in 0..n {
        let Some(list) = &lists[g] else {
            continue;
        };
        match (done[g], best[g]) {
            (Some(i), _) => {
                out[g] = list[i].id.clone();
                why[g] = "the lightest of its family passing the check and every limit".into();
            }
            (None, Some((u, i))) => {
                out[g] = list[i].id.clone();
                why[g] = format!(
                    "no section of its family passes at the target: the one using least ({:.0} %) \
                     of those meeting the limits",
                    u * 100.0
                );
            }
            (None, None) => {
                why[g] = "no section of its family meets the limits and has a usage: left as it is"
                    .into();
            }
        }
        if let Some(i) = no_fit[g]
            && list[i].id != out[g]
        {
            without_fit[g] = Some(list[i].id.clone());
        }
    }
    Ok(Pick {
        choice: out,
        without_fit,
        why,
        trials,
        limits_note,
    })
}

/// A full run of the structure of `text` (as OHL-Structure's run, no
/// extension).
fn full_run(
    input: &SizingInput,
    text: &str,
    budget: &Budget,
) -> Result<(crate::StructureRun, StructureType), String> {
    use ohl_structures::LibraryItem as _;
    let t = StructureType::parse(Path::new("structure.ohls"), text).map_err(|e| e.message)?;
    let run = crate::analyse_structure(
        &crate::RunInput {
            structure: &t,
            text,
            dir: input.dir,
            sections: input.sections,
            table: input.table,
            recipes: input.recipes,
            cases: input.cases,
            extension: None,
            bolts: None,
            feet: None,
        },
        &case_budget(budget),
    )?;
    Ok((run, t))
}

/// Per member id its largest usage in `run` over the cases, with the case
/// and what governs; a case that did not solve: refused.
fn usages(run: &crate::StructureRun) -> Result<HashMap<String, (Option<f64>, String, String)>, String> {
    let mut out: HashMap<String, (Option<f64>, String, String)> = HashMap::new();
    for c in &run.cases {
        let r = match &c.fe {
            Some(Ok(r)) => r,
            Some(Err(e)) => return Err(format!("case {}: {e}", c.name)),
            None => continue,
        };
        for m in &r.members {
            let e = out
                .entry(m.id.clone())
                .or_insert((Some(0.0), String::new(), String::new()));
            match (e.0, m.usage) {
                (Some(a), Some(b)) if b > a || b.is_nan() || e.1.is_empty() => {
                    *e = (Some(if b.is_nan() { f64::INFINITY } else { b }), c.name.clone(), m.governs.clone());
                }
                (_, None) => *e = (None, c.name.clone(), m.governs.clone()),
                _ => {}
            }
        }
    }
    Ok(out)
}

/// The group's largest usage, its case and what governs (none: a member
/// without a usage).
fn group_usage(
    g: &Group,
    u: &HashMap<String, (Option<f64>, String, String)>,
) -> (Option<f64>, String, String) {
    let mut best: (Option<f64>, String, String) = (Some(0.0), String::new(), String::new());
    for id in &g.members {
        match u.get(id) {
            Some((Some(x), case, governs)) => {
                if best.0.is_some_and(|b| *x > b || best.1.is_empty()) {
                    best = (Some(*x), case.clone(), format!("{id}: {governs}"));
                }
            }
            Some((None, case, governs)) => {
                return (None, case.clone(), format!("{id}: {governs}"));
            }
            None => return (None, String::new(), format!("{id}: no result")),
        }
    }
    best
}

/// Sizes the members of the structure of `input` ([module docs](self)).
/// Err: the structure has no model, a case does not solve, the plugin
/// fails, or the run was cancelled.
///
/// Expected cost: per round one solve of every case and one trial per
/// section tried in the longest family (each trial: the model built and
/// one plugin call for every group still open), then two full runs (before
/// and after): seconds for a small truss, minutes for a tower of a few
/// thousand members with a family of forty sections.
pub fn size(input: &SizingInput, budget: &Budget) -> Result<Sizing, String> {
    size_with(input, budget, &|_| {})
}

/// [`size`] with `progress` told each step in words.
pub fn size_with(
    input: &SizingInput,
    budget: &Budget,
    progress: &(dyn Fn(&str) + Sync),
) -> Result<Sizing, String> {
    let started = Instant::now();
    let s = input.settings;
    if !(s.target > 0.0 && s.target.is_finite()) {
        return Err(format!("the target usage must be above 0 (not {})", s.target));
    }
    let doc: DocumentMut = input
        .text
        .parse()
        .map_err(|e| format!("the structure file does not read: {e}"))?;
    progress("the structure as it is: forces");
    let f0 = forces(input, input.text, budget)?;
    let groups = sizing_groups(input.structure, &f0.model, &s.split);
    if groups.is_empty() {
        return Err("no member with a section to size (a level 3 structure is needed)".into());
    }
    let place: HashMap<&str, usize> = f0
        .model
        .members
        .iter()
        .enumerate()
        .map(|(k, m)| (m.id.as_str(), k))
        .collect();
    let index: Vec<Vec<usize>> = groups
        .iter()
        .map(|g| {
            g.members
                .iter()
                .filter_map(|id| place.get(id.as_str()).copied())
                .collect()
        })
        .collect();
    let mut not_sized: Vec<Option<String>> = vec![None; groups.len()];
    let lists: Vec<Option<Vec<Candidate>>> = groups
        .iter()
        .enumerate()
        .map(|(k, g)| {
            if s.keep.iter().any(|x| *x == g.name) {
                not_sized[k] = Some("left as it is (asked)".into());
                return None;
            }
            match candidates(input.sections, &g.section) {
                Some(l) => Some(l),
                None => {
                    not_sized[k] = Some(format!(
                        "left as it is: its section {} is not in the project's section library \
                         (or has no material density)",
                        g.section
                    ));
                    None
                }
            }
        })
        .collect();
    let current0: Vec<String> = groups.iter().map(|g| g.section.clone()).collect();
    let mut choice = current0.clone();
    let mut history: Vec<Vec<String>> = vec![choice.clone()];
    let mut rounds: Vec<Round> = Vec::new();
    let mut settled = None;
    let mut last = None;
    let mut f = f0;
    // The forces of the structure as it is, kept for its joint fit.
    let mut f_first: Option<Forces> = None;
    for r in 0..s.rounds.max(1) {
        let t_round = Instant::now();
        if r > 0 {
            progress(&format!("round {}: forces with the new sections", r + 1));
            let (text, _) = with_choice(&doc, &groups, &choice);
            let next = forces(input, &text, budget)?;
            let old = std::mem::replace(&mut f, next);
            if f_first.is_none() {
                f_first = Some(old);
            }
        }
        let p = pick(
            input, &doc, &groups, &lists, &current0, &f, &index, budget, progress,
        )?;
        let changed = p
            .choice
            .iter()
            .zip(&choice)
            .filter(|(a, b)| a != b)
            .count();
        rounds.push(Round {
            changed,
            trials: p.trials,
            took: t_round.elapsed(),
        });
        let new = p.choice.clone();
        last = Some(p);
        if new == choice {
            settled = Some(Settled::Settled);
            break;
        }
        if let Some(at) = history.iter().position(|h| *h == new) {
            let cycle = &history[at..];
            choice = heavier(input.sections, cycle);
            settled = Some(Settled::Cycle(format!(
                "the choice came back after {} round(s) (a cycle of {} choices): per group the \
                 heavier of the cycle's sections taken (on the safe side)",
                rounds.len(),
                cycle.len()
            )));
            break;
        }
        history.push(new.clone());
        choice = new;
    }
    let settled = match settled {
        Some(x) => x,
        None => {
            let n = history.len();
            if n >= 2 {
                choice = heavier(input.sections, &history[n - 2..]);
            }
            Settled::NotSettled(format!(
                "not settled within {} round(s): per group the heavier of the last two choices \
                 taken (on the safe side)",
                s.rounds.max(1)
            ))
        }
    };
    let last = last.expect("one round at least");
    let f_first = f_first.unwrap_or(f);
    // The final run: a full analysis with the chosen sections.
    progress("the final run with the chosen sections");
    let (text, removed) = with_choice(&doc, &groups, &choice);
    let (before_run, _) = full_run(input, input.text, budget)?;
    let (after_run, _) = full_run(input, &text, budget)?;
    let before = usages(&before_run)?;
    let after = usages(&after_run)?;
    let f_after = forces(input, &text, budget)?;
    let all: Vec<usize> = index.iter().flatten().copied().collect();
    let (fit0, unknown0) = joint_fit(&f_first.model, &all, &f_first.axial);
    let (fit1, _) = joint_fit(&f_after.model, &all, &f_after.axial);
    let count = |fit: &[FitProblem], ks: &[usize]| fit.iter().filter(|p| ks.contains(&p.member)).count();
    let length_of = |ks: &[usize]| -> f64 { ks.iter().map(|&k| f_first.model.length(&f_first.model.members[k])).sum() };
    let mass_of = |id: &str, length: f64| -> f64 {
        input
            .sections
            .section(id)
            .and_then(|x| mass_per_metre(input.sections, x))
            .unwrap_or(0.0)
            * length
    };
    let mut changes: Vec<GroupChange> = Vec::new();
    let mut fit_heavier = 0;
    let mut fit_mass = 0.0;
    for (g, grp) in groups.iter().enumerate() {
        let length = length_of(&index[g]);
        let (u0, c0, _) = group_usage(grp, &before);
        let (u1, c1, gov1) = group_usage(grp, &after);
        let to = choice[g].clone();
        let without_fit = last.without_fit[g].clone().filter(|w| *w != to);
        if let Some(w) = &without_fit {
            fit_heavier += 1;
            fit_mass += mass_of(&to, length) - mass_of(w, length);
        }
        let why = match &not_sized[g] {
            Some(w) => w.clone(),
            None => last.why[g].clone(),
        };
        changes.push(GroupChange {
            group: grp.clone(),
            from: grp.section.clone(),
            to: to.clone(),
            length,
            mass: [mass_of(&grp.section, length), mass_of(&to, length)],
            usage: [u0, u1],
            case: [c0, c1],
            governs: gov1,
            fit: [count(&fit0, &index[g]), count(&fit1, &index[g])],
            without_fit,
            why,
            removed: removed[g]
                .iter()
                .map(|f| format!("design {f} (it described {})", grp.section))
                .collect(),
            over_target: u1.is_none_or(|u| u > s.target * (1.0 + 1e-12)),
            not_sized: not_sized[g].is_some(),
        });
    }
    changes.sort_by(|a, b| {
        b.changed()
            .cmp(&a.changed())
            .then((a.mass[1] - a.mass[0]).total_cmp(&(b.mass[1] - b.mass[0])))
            .then(a.group.name.cmp(&b.group.name))
    });
    let mut over: Vec<OverTarget> = after
        .iter()
        .filter_map(|(id, (u, case, governs))| {
            let u = (*u)?;
            (u > s.target * (1.0 + 1e-12)).then(|| OverTarget {
                member: id.clone(),
                usage: u,
                case: case.clone(),
                governs: governs.clone(),
            })
        })
        .collect();
    over.sort_by(|a, b| b.usage.total_cmp(&a.usage).then(a.member.cmp(&b.member)));
    let worst = |u: &HashMap<String, (Option<f64>, String, String)>| {
        u.values().filter_map(|x| x.0).fold(None, |w: Option<f64>, x| Some(w.map_or(x, |w| w.max(x))))
    };
    let mut notes = Vec::new();
    if f_first.model.capacity_plugin.is_none() {
        notes.push(
            "NOT ON THE SAFE SIDE: sized by the core's simple check (Euler buckling, gross yield): \
             it leaves out a design code's column curve, imperfections, net sections and \
             connections, so a chosen section can be too small for a design code; load the \
             design code's plugin ([analysis] capacity, e.g. asce10-97 or sia263-2013) and size \
             again to make it safe"
                .to_owned(),
        );
    }
    if let Some(why) = &last.limits_note {
        notes.push(format!(
            "NOT ON THE SAFE SIDE: the code's limits on the members' sizes (slenderness, flat \
             width to thickness, least thickness) not checked — {why}; a chosen section can pass \
             its usage and still break such a limit; a code plugin with the member-limits export \
             (the bundled asce10 and sia263) checks them"
        ));
    }
    if !s.joint_fit {
        notes.push(
            "joint-fit limits off: a chosen member may not take its bolts or be long enough \
             for them (its joints may not be buildable)"
                .to_owned(),
        );
    }
    if unknown0 > 0 {
        notes.push(format!(
            "{unknown0} member end(s) without a bolt known (no bolts in their data, no default \
             bolt): their joint fit not checked"
        ));
    }
    notes.push(
        "the choice is made with the forces of the structure as written (no extension, no foot \
         movements, the session's end releases and bolt slip off); the proposal's usage is \
         OHL-Structure's full run with the chosen sections, as written (the file's own options \
         in it)"
            .to_owned(),
    );
    if !input.structure.extensions.is_empty() {
        notes.push(format!(
            "NOT ON THE SAFE SIDE: sized on the structure as written, not on its {} height \
             extension(s) — an extension changes the forces in the body (a taller one larger at \
             the legs), so a member can be overloaded on one; run Results on each extension \
             after Apply (and size the members again where one is above the target) to make it \
             safe",
            input.structure.extensions.len()
        ));
    }
    let fit_effect = if s.joint_fit {
        if fit_heavier == 0 {
            "the joint-fit limits made no group heavier".to_owned()
        } else {
            format!(
                "the joint-fit limits kept {fit_heavier} group(s) heavier than their check alone \
                 needs (+{fit_mass:.0} kg)"
            )
        }
    } else {
        "the joint-fit limits off".to_owned()
    };
    let sized = |c: &GroupChange| !c.not_sized;
    Ok(Sizing {
        mass: [
            changes.iter().filter(|c| sized(c)).map(|c| c.mass[0]).sum(),
            changes.iter().filter(|c| sized(c)).map(|c| c.mass[1]).sum(),
        ],
        usage: [worst(&before), worst(&after)],
        fit: [fit0.len(), fit1.len()],
        groups: changes,
        rounds,
        settled,
        notes,
        over_target: over,
        fit_effect,
        text,
        took: started.elapsed(),
    })
}

/// Per group the heaviest of `choices`' sections.
pub(crate) fn heavier(lib: &SectionLibrary, choices: &[Vec<String>]) -> Vec<String> {
    let mass = |id: &str| {
        lib.section(id)
            .and_then(|s| mass_per_metre(lib, s))
            .unwrap_or(0.0)
    };
    (0..choices[0].len())
        .map(|g| {
            choices
                .iter()
                .map(|c| c[g].clone())
                .max_by(|a, b| mass(a).total_cmp(&mass(b)).then(b.cmp(a)))
                .expect("one choice at least")
        })
        .collect()
}

/// The structure file `text` with the proposals of `sizing` for the groups
/// named in `accepted` taken (each member's section; the design data fields
/// describing the old section removed): one edit.
pub fn apply(text: &str, sizing: &Sizing, accepted: &[String]) -> Result<String, String> {
    let mut doc: DocumentMut = text
        .parse()
        .map_err(|e| format!("the structure file does not read: {e}"))?;
    for c in &sizing.groups {
        if !c.changed() || !accepted.iter().any(|a| *a == c.group.name) {
            continue;
        }
        for id in &c.group.members {
            if member_table(&mut doc, id).is_none() {
                return Err(format!("member {id} is no longer in the file: size again"));
            }
            set_section(&mut doc, id, &c.to);
        }
    }
    Ok(doc.to_string())
}

/// The proposal in lines, for the command line and the reports.
pub fn summary(sizing: &Sizing) -> Vec<String> {
    let pct = |u: Option<f64>| u.map_or("–".to_owned(), |u| format!("{:.0} %", u * 100.0));
    let changed = sizing.groups.iter().filter(|c| c.changed()).count();
    let mut out = vec![format!(
        "{changed} of {} group(s) changed; mass {:.0} → {:.0} kg; largest usage {} → {}; \
         joint-fit problems {} → {}; {} round(s), {:.1} s",
        sizing.groups.len(),
        sizing.mass[0],
        sizing.mass[1],
        pct(sizing.usage[0]),
        pct(sizing.usage[1]),
        sizing.fit[0],
        sizing.fit[1],
        sizing.rounds.len(),
        sizing.took.as_secs_f64()
    )];
    match &sizing.settled {
        Settled::Settled => {}
        Settled::Cycle(w) | Settled::NotSettled(w) => out.push(w.clone()),
    }
    out.push(sizing.fit_effect.clone());
    out.extend(sizing.notes.iter().cloned());
    for c in &sizing.groups {
        out.push(format!(
            "{}{} ({} member(s)): {} → {}, {:.1} → {:.1} kg, usage {} → {} ({}), joint fit {} → \
             {} — {}{}{}",
            if c.changed() { "* " } else { "  " },
            c.group.name,
            c.group.members.len(),
            c.from,
            c.to,
            c.mass[0],
            c.mass[1],
            pct(c.usage[0]),
            pct(c.usage[1]),
            if c.case[1].is_empty() { "-" } else { &c.case[1] },
            c.fit[0],
            c.fit[1],
            c.why,
            c.without_fit
                .as_ref()
                .map_or(String::new(), |w| format!("; without the joint-fit limits {w}")),
            if c.removed.is_empty() {
                String::new()
            } else {
                format!("; removed: {}", c.removed.join(", "))
            }
        ));
    }
    for o in &sizing.over_target {
        out.push(format!(
            "ABOVE THE TARGET: member {} {:.0} % in {} ({})",
            o.member,
            o.usage * 100.0,
            o.case,
            o.governs
        ));
    }
    out
}
