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
//! - **the joint-fit limits** ([`crate::joint_fit`]): the leg takes its
//!   bolt (the hole clear of the root fillet at the layout's gauge, the
//!   code's edge distance kept to the toe), the member is long enough for
//!   the bolts its ends need (the code's end distance and spacing), a
//!   plate the shop makes takes its leg (the detailing practice in force,
//!   as Lay out joints takes it) — a choice never lands on a member that
//!   cannot be connected where a connectable one passes (none: the
//!   lightest passing, its problems said).
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
//! **The bolts** (stage 1b, 2026-11-27, [`crate::sizing_bolts`]): a
//! group failing only by the bolt shear its plugin names is not made
//! heavier for it — the bolts govern: more of its own bolts, else a larger
//! bolt of the project's library, proposed within the joint-fit limits
//! and written by Apply with its sections.
//!
//! Groups whose members carry capacities written by hand, and structures
//! analysed from a finite-element model file of their own, are not sized
//! (said). Later stages are in ROADMAP O49.

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
use crate::joint_fit::joint_fit_cached;
pub use crate::joint_fit::{FitCache, FitProblem, FitRule, FitRules, joint_fit};
use crate::sizing_bolts::{self, BoltChange, BoltPick, BoltSet, Caps};

/// The note of a sizing by the core's simple check: what it leaves out,
/// why that can be unsafe, what makes it safe.
pub const SIMPLE_NOTE: &str = "NOT ON THE SAFE SIDE: sized by the core's simple check (Euler \
     buckling, gross yield) — it leaves out a design code's column curve, imperfections, net \
     sections and connections, so a chosen section can be too small for a design code's check; \
     load the design code's plugin ([analysis] capacity, e.g. asce10-97 or sia263-2013) and \
     choose again to make it safe";

/// The sizer's settings (stored in no file).
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// The largest usage a chosen section may have (1: its capacity).
    pub target: f64,
    /// Keep to the joint-fit limits ([`crate::joint_fit`]).
    pub joint_fit: bool,
    /// The most rounds of forces and choice (at least [`MIN_ROUNDS`]).
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

/// The fewest rounds: one round cannot show that a choice holds with its
/// own forces (not settled, the heavier of the last two — the sections as
/// they are — would be taken).
pub const MIN_ROUNDS: usize = 2;

/// The most times one round takes back the groups a failed solve names
/// (each time at least one group); then every group made lighter goes
/// back at once (the choice that solved). The program's bound on the
/// work, not a standard's.
pub const MAX_REPAIRS: usize = 20;

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
    /// The joint-fit problems before and after, in words.
    pub problems: [Vec<String>; 2],
    /// The lightest section passing without the joint-fit limits, where it
    /// differs from the proposal (the limits' effect).
    pub without_fit: Option<String>,
    /// Why, in words.
    pub why: String,
    /// How the choice was made.
    pub basis: Basis,
    /// The design data removed with the change, in words.
    pub removed: Vec<String>,
    /// A member of the group above the target after.
    pub over_target: bool,
    /// Left as it is (not sized: its section not in the library, asked).
    pub not_sized: bool,
    /// Its bolts proposed (stage 1b): where they govern, per bolt size of
    /// its members (stage 1c), and its members' bolts their joints take to
    /// another size (one bolt size per joint, stage 1c); empty: none (where
    /// no proposal could be made, `bolts_govern` says).
    pub bolts: Vec<BoltChange>,
    /// Where the bolts govern, in words (none: they do not).
    pub bolts_govern: Option<String>,
}

impl GroupChange {
    /// Changed: its section, or its bolts.
    pub fn changed(&self) -> bool {
        self.from != self.to || !self.bolts.is_empty()
    }

    /// Its bolt proposals in words: "2 × 16 mm to 3 × 16 mm; …" (empty:
    /// none).
    pub fn bolts_words(&self) -> String {
        self.bolts
            .iter()
            .map(|b| format!("{} to {}", b.from, b.to))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// One bolt size per joint (O49 stage 1c, the owner's words of
/// 2026-11-27: "for the bolts please try to use the same ones for a joint.
/// this would ease the construction once outside"): the joints with
/// bolts of one size and those with several, before and after.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JointBoltSizes {
    /// Joints with bolts of one size, before and after.
    pub single: [usize; 2],
    /// Joints with bolts of several sizes, before and after.
    pub mixed: [usize; 2],
    /// Members whose bolts their joints took to a larger size.
    pub taken_along: usize,
    /// The joints with several sizes after, each with its sizes and, where
    /// the proposal made or kept it so, why.
    pub mixed_after: Vec<(String, String)>,
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
    /// The effect of a target below 100 % in words (groups kept heavier by
    /// it, the mass, with the last round's forces).
    pub target_effect: String,
    /// The structure file with every proposal taken.
    pub text: String,
    /// How long the whole run took.
    pub took: Duration,
    /// One bolt size per joint (stage 1c): the joints before and after.
    pub joint_sizes: JointBoltSizes,
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
    (
        s.shape.as_str().to_owned(),
        lib.material_of(s).map(|m| m.id.clone()),
    )
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
/// at their own section untouched) and the members' bolts as `bolts` (per
/// member id; the others as they are): the text, and per group the fields
/// removed.
fn with_choice(
    doc: &DocumentMut,
    groups: &[Group],
    choice: &[String],
    bolts: &[(String, BoltPick)],
) -> (String, Vec<Vec<&'static str>>) {
    let mut doc = doc.clone();
    let mut removed = vec![Vec::new(); groups.len()];
    for (id, b) in bolts {
        sizing_bolts::set_bolts(&mut doc, id, b);
    }
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
    /// Per member its largest tension over the strength factor, N.
    tension: Vec<f64>,
    /// Per member its largest compression over the strength factor, N.
    compression: Vec<f64>,
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

/// Why the forces of a choice could not be worked out: the words, the
/// members a buckling mode names (in compression at the joint moving
/// most, straining most in the mode) and the members meeting at the joints
/// a refusal names as moving most, with those joints' places (empty for
/// any other refusal).
#[derive(Debug, Clone)]
struct NotSolved {
    words: String,
    members: Vec<String>,
    around: Vec<String>,
    places: Vec<[f64; 3]>,
}

impl From<String> for NotSolved {
    fn from(words: String) -> Self {
        Self {
            words,
            members: Vec::new(),
            around: Vec::new(),
            places: Vec::new(),
        }
    }
}

/// The places of the joints `joints` of `model` (by id).
fn places_of(model: &Model, joints: &[String]) -> Vec<[f64; 3]> {
    model
        .nodes
        .iter()
        .filter(|n| joints.contains(&n.id))
        .map(|n| n.point)
        .collect()
}

/// The members of `model` meeting at the joints `joints` (by id).
fn members_at(model: &Model, joints: &[String]) -> Vec<String> {
    let at: std::collections::HashSet<usize> = model
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| joints.contains(&n.id))
        .map(|(k, _)| k)
        .collect();
    model
        .members
        .iter()
        .filter(|m| at.contains(&m.i) || at.contains(&m.j))
        .map(|m| m.id.clone())
        .collect()
}

/// The structure of `text` (its model, the counted self-weight as the run
/// takes it) with every case solved. Err: the model does not build, or a
/// case does not solve (named, with the members its buckling mode names).
fn forces(input: &SizingInput, text: &str, budget: &Budget) -> Result<Forces, NotSolved> {
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
        let loads =
            crate::fe_loads(&model, input.table, &c.name, self_weight).map_err(NotSolved::from)?;
        ohl_structures_fe::analyse(&model, &loads, c.strength, &case_budget(budget))
            .map(|(s, _)| s)
            .map_err(|e| {
                use ohl_structures_fe::FeError;
                NotSolved {
                    words: format!(
                        "case {}: did not solve: {}",
                        c.name,
                        ohl_structures_fe::refusal_text(&model, &e)
                    ),
                    members: match &e {
                        FeError::Unstable {
                            compressed,
                            straining,
                            ..
                        } => compressed
                            .iter()
                            .map(|x| x.0.clone())
                            .chain(straining.iter().cloned())
                            .collect(),
                        _ => Vec::new(),
                    },
                    around: match &e {
                        FeError::Unstable { joints, .. }
                        | FeError::Implausible { joints, .. }
                        | FeError::Mechanism { joints, .. } => members_at(&model, joints),
                        _ => Vec::new(),
                    },
                    places: match &e {
                        FeError::Unstable { joints, .. }
                        | FeError::Implausible { joints, .. }
                        | FeError::Mechanism { joints, .. } => places_of(&model, joints),
                        _ => Vec::new(),
                    },
                }
            })
    });
    let solutions = solved.into_iter().collect::<Result<Vec<_>, NotSolved>>()?;
    let n = model.members.len();
    let mut axial = vec![0.0f64; n];
    let mut compressed = vec![false; n];
    let mut tension = vec![0.0f64; n];
    let mut compression = vec![0.0f64; n];
    for (c, s) in cases.iter().zip(&solutions) {
        let strength = if c.strength > 0.0 { c.strength } else { 1.0 };
        for (k, f) in s.members.iter().enumerate().take(n) {
            axial[k] = axial[k].max(f.axial.abs() / strength);
            if f.axial < 0.0 {
                compressed[k] = true;
                compression[k] = compression[k].max(-f.axial / strength);
            } else {
                tension[k] = tension[k].max(f.axial / strength);
            }
        }
    }
    Ok(Forces {
        model,
        cases,
        solutions,
        axial,
        compressed,
        tension,
        compression,
    })
}

/// Per member of `model` its largest usage over the solved cases of
/// `forces` (none: not checked or no capacity) — the plugin's check of
/// the model's code (one call), else the core's own — and its capacities
/// as the plugin gave them (none: no plugin, or none for it).
type TrialUsage = (Vec<Option<f64>>, Vec<Option<Caps>>);

fn trial_usage(model: &Model, forces: &Forces) -> Result<TrialUsage, String> {
    let n = model.members.len();
    let mut caps: Vec<Option<Caps>> = vec![None; n];
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
                let place: HashMap<&str, usize> = model
                    .members
                    .iter()
                    .enumerate()
                    .map(|(k, m)| (m.id.as_str(), k))
                    .collect();
                for g in &c.results {
                    if let Some(&k) = place.get(g.id.as_str()) {
                        let by = |name: &str| {
                            g.capacities
                                .iter()
                                .find(|x| x.0 == name)
                                .map(|x| (x.1, x.2.clone()))
                        };
                        caps[k] = Some(Caps {
                            tension: by("tension"),
                            compression: by("compression"),
                        });
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
    Ok((out, caps))
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

/// How a group's choice was made in a round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Basis {
    /// The lightest of its family passing the check and every limit.
    Passes,
    /// No section of its family passes the check and the code's limits
    /// and also fits its joints: the lightest passing the check and the
    /// code's limits, its joint-fit problems said.
    FitNotMet,
    /// No section of its family passes the check at the target: the one
    /// using least of those meeting the code's limits.
    LeastUsage,
    /// (Stage 1c, the owner's answer of 2026-11-27) No section passes, the
    /// bolts govern (their shear) and no bolt proposal fits: its current
    /// section kept — a heavier one does not raise the bolt shear.
    BoltsKept,
    /// Nothing meets the code's limits with a usage, or not sized: left as
    /// it is.
    Left,
}

/// One round's choice with the forces of `f`.
struct Pick {
    choice: Vec<String>,
    basis: Vec<Basis>,
    without_fit: Vec<Option<String>>,
    /// Per group the lightest passing at a usage of 1 where it differs
    /// (the target below 1: its effect, measured with this round's
    /// forces).
    at_one: Vec<Option<String>>,
    why: Vec<String>,
    trials: usize,
    limits_note: Option<String>,
    /// Per bolt set its bolts proposed (stage 1b, per size since stage 1c;
    /// none: as they are).
    bolts: Vec<Option<BoltPick>>,
    /// Per group, where the bolts govern, in words.
    bolts_govern: Vec<Option<String>>,
}

/// Per group the lightest candidate passing with the forces `f` (`index`:
/// the model's member places of each group's members; `rules`: the
/// detailing practice's).
#[allow(clippy::too_many_arguments)]
fn pick(
    input: &SizingInput,
    doc: &DocumentMut,
    groups: &[Group],
    lists: &[Option<Vec<Candidate>>],
    current: &[String],
    f: &Forces,
    index: &[Vec<usize>],
    part_of: &[Option<usize>],
    floor: &[Option<(String, String)>],
    rules: &FitRules,
    cache: &mut FitCache,
    budget: &Budget,
    progress: &(dyn Fn(&str) + Sync),
    bolt_plan: &BoltPlan,
    input_ids: &[String],
) -> Result<Pick, String> {
    let s = input.settings;
    let n = groups.len();
    // Stage 1b: per bolt set (stage 1c: per size within a group) its bolts
    // (carried from the round before), the proposals tried, the library
    // bolt taken (its place in the options).
    let n_sets = bolt_plan.sets.len();
    let mut bolts: Vec<Option<BoltPick>> = bolt_plan.now.clone();
    let mut bolt_tries = vec![0usize; n_sets];
    let mut bolt_option: Vec<Option<usize>> = vec![None; n_sets];
    let mut bolts_govern: Vec<Option<String>> = vec![None; n];
    // (Stage 1c) Per group: the bolts govern and no bolt proposal fits —
    // why (its current section is then kept, never the lightest of equally
    // failing sections).
    let mut bolt_blocked: Vec<Option<String>> = vec![None; n];
    let mut pointer = vec![0usize; n];
    let mut done: Vec<Option<usize>> = vec![None; n];
    let mut no_fit: Vec<Option<usize>> = vec![None; n];
    let mut at_one: Vec<Option<usize>> = vec![None; n];
    // The lowest usage among candidates meeting the code's limits, for a
    // group nothing passes: (usage, candidate).
    let mut best: Vec<Option<(f64, usize)>> = vec![None; n];
    let mut open = vec![false; n];
    for g in 0..n {
        let Some(list) = &lists[g] else {
            continue;
        };
        open[g] = true;
        // A candidate whose gross yield A·f_y is below the force (N over
        // the strength factor, the largest over the cases) passes no
        // code's check: skipped (only ever a lighter one skipped).
        let force = index[g]
            .iter()
            .map(|&k| f.axial.get(k).copied().unwrap_or(0.0))
            .fold(0.0, f64::max);
        while pointer[g] < list.len() {
            let c = &list[pointer[g]];
            match c.fy {
                Some(fy) if c.area * fy * s.target < force => pointer[g] += 1,
                _ => break,
            }
        }
        // Never lighter than a section a lighter one made the structure
        // not solve with.
        if let Some((id, _)) = &floor[g]
            && let Some(at) = list.iter().position(|c| c.id == *id)
        {
            pointer[g] = pointer[g].max(at);
        }
    }
    let mut trials = 0;
    let mut limits_note = None;
    loop {
        if cancelled(budget) {
            return Err("cancelled".into());
        }
        let trying: Vec<usize> = (0..n)
            .filter(|&g| {
                open[g] && done[g].is_none() && pointer[g] < lists[g].as_ref().map_or(0, Vec::len)
            })
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
        let (text, _) = with_choice(doc, groups, &choice, &bolt_plan.assign(&bolts, input_ids));
        let t = {
            use ohl_structures::LibraryItem as _;
            StructureType::parse(Path::new("structure.ohls"), &text).map_err(|e| e.message)?
        };
        let model = crate::model_of(&t, &text, input.dir, input.sections)?;
        let (usage, caps) = trial_usage(&model, f)?;
        let limits = ohl_member_check::limits::limits(&model)?;
        if let Limits::Unknown(why) = &limits {
            limits_note = Some(why.clone());
        }
        let broken = broken(&limits, &f.compressed);
        let members: Vec<usize> = trying.iter().flat_map(|&g| index[g].clone()).collect();
        // Worked out with the limits off too: their effect is said.
        let (fit, _) = joint_fit_cached(
            &model,
            &members,
            &f.axial,
            rules,
            &|k| part_of.get(k).copied().flatten(),
            cache,
        );
        let mut misfit: HashMap<usize, usize> = HashMap::new();
        let mut misfit_rules: HashMap<usize, Vec<FitRule>> = HashMap::new();
        for p in &fit {
            *misfit.entry(p.member).or_default() += 1;
            misfit_rules.entry(p.member).or_default().push(p.rule);
        }
        for &g in &trying {
            let worst = index[g].iter().try_fold(0.0f64, |w, &k| {
                usage.get(k).copied().flatten().map(|u| w.max(u))
            });
            let ok_usage = worst.is_some_and(|u| u <= s.target * (1.0 + 1e-12));
            let ok_limits = index[g].iter().all(|k| !broken.contains_key(k));
            let ok_fit = !s.joint_fit || index[g].iter().all(|k| !misfit.contains_key(k));
            if ok_usage && ok_limits && no_fit[g].is_none() {
                no_fit[g] = Some(pointer[g]);
            }
            if worst.is_some_and(|u| u <= 1.0 + 1e-12) && ok_limits && ok_fit && at_one[g].is_none()
            {
                at_one[g] = Some(pointer[g]);
            }
            if ok_limits
                && let Some(u) = worst
                && best[g].is_none_or(|b| u < b.0)
            {
                best[g] = Some((u, pointer[g]));
            }
            // Stage 1b: failing only by the bolt shear its plugin names —
            // a heavier section does not raise it: more of its own bolts
            // (the same section tried again); not fitting the member (its
            // length, its leg), the next larger bolt of the library. Stage
            // 1c: per bolt size within the group (each set its own).
            let mut proposed = false;
            let mut by_bolts: Vec<usize> = Vec::new();
            for &si in &bolt_plan.of_group[g] {
                let set = &bolt_plan.sets[si];
                if ok_usage {
                    break;
                }
                let Some(r) = sizing_bolts::short_by(
                    &set.members,
                    &usage,
                    &caps,
                    &f.tension,
                    &f.compression,
                    s.target,
                ) else {
                    continue;
                };
                let governs = format!(
                    "the bolts govern (bolt shear, {:.0} % of their capacity): a heavier section \
                     does not raise it",
                    r * s.target * 100.0
                );
                let base = match &set.base {
                    Ok(b) => b,
                    Err(why) => {
                        bolt_blocked[g] = Some(format!("no bolt proposal ({why})"));
                        bolts_govern[g] = Some(format!("{governs}: no bolt proposal ({why})"));
                        continue;
                    }
                };
                if bolt_tries[si] >= sizing_bolts::MAX_BOLT_TRIES {
                    bolt_blocked[g] = Some(format!(
                        "the most bolt proposals tried ({})",
                        sizing_bolts::MAX_BOLT_TRIES
                    ));
                    continue;
                }
                let now = bolts[si].clone().unwrap_or_else(|| base.clone());
                let count = ((f64::from(now.count) * r) - 1e-9)
                    .ceil()
                    .max(f64::from(now.count + 1));
                bolts[si] = Some(BoltPick {
                    count: count as u32,
                    ..now
                });
                bolt_tries[si] += 1;
                bolts_govern[g] = Some(governs);
                by_bolts.extend(set.members.iter().copied());
                proposed = true;
            }
            if proposed {
                // Members above the target by another check: a heavier
                // section for them too (the bolts kept).
                let others = index[g].iter().any(|&k| {
                    !by_bolts.contains(&k)
                        && usage
                            .get(k)
                            .copied()
                            .flatten()
                            .is_some_and(|u| u > s.target * (1.0 + 1e-12))
                });
                if !others {
                    continue;
                }
            }
            let mut moved = false;
            if !proposed && ok_usage && ok_limits && !ok_fit {
                for &si in &bolt_plan.of_group[g] {
                    let set = &bolt_plan.sets[si];
                    let (Ok(_), Some(now)) = (&set.base, bolts[si].clone()) else {
                        continue;
                    };
                    let misfits = set.members.iter().any(|k| misfit.contains_key(k));
                    let bolt_misfit = misfits
                        && set.members.iter().all(|k| {
                            misfit_rules.get(k).is_none_or(|rs| {
                                rs.iter()
                                    .all(|r| matches!(r, FitRule::Length | FitRule::LegTakesBolt))
                            })
                        });
                    if !bolt_misfit || bolt_tries[si] >= sizing_bolts::MAX_BOLT_TRIES {
                        continue;
                    }
                    let next = bolt_option[si].map_or(0, |i| i + 1);
                    if let Some(o) = set.options.get(next) {
                        let count = sizing_bolts::count_for(&now, now.count, o);
                        bolts[si] = Some(BoltPick { count, ..o.clone() });
                        bolt_option[si] = Some(next);
                        bolt_tries[si] += 1;
                        moved = true;
                    }
                }
            }
            if moved {
                continue;
            }
            if ok_usage && ok_limits && ok_fit {
                done[g] = Some(pointer[g]);
            } else {
                pointer[g] += 1;
            }
        }
        trials += 1;
    }
    let mut out = current.to_vec();
    let mut basis = vec![Basis::Left; n];
    let mut why = vec![String::new(); n];
    let mut without_fit = vec![None; n];
    let mut one = vec![None; n];
    for g in 0..n {
        let Some(list) = &lists[g] else {
            continue;
        };
        match (done[g], no_fit[g], best[g]) {
            (Some(i), _, _) => {
                out[g] = list[i].id.clone();
                basis[g] = Basis::Passes;
                why[g] = "the lightest of its family passing the check and every limit".into();
            }
            (None, Some(i), _) => {
                out[g] = list[i].id.clone();
                basis[g] = Basis::FitNotMet;
                why[g] = "no section of its family passes the check and the code's limits and \
                          also fits its joints: the lightest passing the check and the code's \
                          limits, its joint-fit problems said (its joints to detail by hand)"
                    .into();
            }
            (None, None, Some(_)) if bolt_blocked[g].is_some() => {
                // (Stage 1c, the owner's answer of 2026-11-27) The bolts
                // govern and no bolt proposal fits: the current section
                // kept, not the lightest of equally failing ones.
                basis[g] = Basis::BoltsKept;
                why[g] = format!(
                    "no section of its family passes at the target, the bolts govern (bolt \
                     shear) and {}: its current section kept (a heavier one does not raise the \
                     bolt shear; a lighter one is not taken for it)",
                    bolt_blocked[g]
                        .as_deref()
                        .unwrap_or("no bolt proposal fits")
                );
            }
            (None, None, Some((u, i))) => {
                out[g] = list[i].id.clone();
                basis[g] = Basis::LeastUsage;
                why[g] = format!(
                    "no section of its family passes at the target: the one using least ({:.0} %) \
                     of those meeting the code's limits",
                    u * 100.0
                );
            }
            (None, None, None) => {
                why[g] =
                    "no section of its family meets the code's limits with a usage: left as it is"
                        .into();
            }
        }
        if let Some(i) = no_fit[g]
            && list[i].id != out[g]
        {
            without_fit[g] = Some(list[i].id.clone());
        }
        if let Some(i) = at_one[g]
            && list[i].id != out[g]
        {
            one[g] = Some(list[i].id.clone());
        }
    }
    for (g, said) in bolts_govern.iter_mut().enumerate() {
        let proposed: Vec<String> = bolt_plan.of_group[g]
            .iter()
            .filter_map(|&si| bolts[si].as_ref().map(BoltPick::words))
            .collect();
        if said.is_some() && !proposed.is_empty() {
            let w = said.take().unwrap_or_default();
            *said = Some(format!("{w}: {} proposed", proposed.join(", ")));
        }
    }
    Ok(Pick {
        choice: out,
        basis,
        without_fit,
        at_one: one,
        why,
        trials,
        limits_note,
        bolts,
        bolts_govern,
    })
}

/// The bolt proposals of a sizing (stage 1b; stage 1c: per bolt size
/// within a group): the bolt sets, each set's bolts now (carried from round
/// to round), each group's sets.
struct BoltPlan {
    sets: Vec<BoltSet>,
    now: Vec<Option<BoltPick>>,
    of_group: Vec<Vec<usize>>,
}

impl BoltPlan {
    /// The members' bolts as `now` (per set) gives them: per member id its
    /// pick (`ids`: the model's member ids by place).
    fn assign(&self, now: &[Option<BoltPick>], ids: &[String]) -> Vec<(String, BoltPick)> {
        let mut out = Vec::new();
        for (si, set) in self.sets.iter().enumerate() {
            if let Some(Some(b)) = now.get(si) {
                for &k in &set.members {
                    out.push((ids[k].clone(), b.clone()));
                }
            }
        }
        out
    }

    /// Per model member (`n` of them) its proposed bolts from `now` (none:
    /// as they are).
    fn per_member(&self, now: &[Option<BoltPick>], n: usize) -> Vec<Option<BoltPick>> {
        let mut out = vec![None; n];
        for (si, set) in self.sets.iter().enumerate() {
            if let Some(Some(b)) = now.get(si) {
                for &k in &set.members {
                    out[k] = Some(b.clone());
                }
            }
        }
        out
    }
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

/// Per member id its largest usage (none: not checked), the case and
/// what governs.
type Usages = HashMap<String, (Option<f64>, String, String)>;

/// Per member id its largest usage in `run` over the cases, with the case
/// and what governs; a case that did not solve: refused.
fn usages(run: &crate::StructureRun) -> Result<Usages, String> {
    let mut out: Usages = HashMap::new();
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
                    *e = (
                        Some(if b.is_nan() { f64::INFINITY } else { b }),
                        c.name.clone(),
                        m.governs.clone(),
                    );
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
fn group_usage(g: &Group, u: &Usages) -> (Option<f64>, String, String) {
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
    if !(s.target > 0.0 && s.target <= 1.0) {
        return Err(format!(
            "the target usage must be above 0 and at most 100 % (not {:.0} %): above 100 % a \
             member would be chosen beyond its capacity",
            s.target * 100.0
        ));
    }
    if let Some(file) = &input.structure.model {
        return Err(format!(
            "this structure's members are analysed from its own finite-element model file \
             ({file}): their sections are in that file, which the sizer does not change — size \
             a structure whose members are in its own file"
        ));
    }
    let doc: DocumentMut = input
        .text
        .parse()
        .map_err(|e| format!("the structure file does not read: {e}"))?;
    progress("the detailing practice in force");
    let rules = FitRules::for_structure(input.structure, input.text, input.dir);
    let mut cache = FitCache::default();
    progress("the structure as it is: forces");
    let f0 = forces(input, input.text, budget).map_err(|e| e.words)?;
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
    // Per member of the model its sizing group (none: not sized).
    let mut part_of: Vec<Option<usize>> = vec![None; f0.model.members.len()];
    for (g, ks) in index.iter().enumerate() {
        for &k in ks {
            part_of[k] = Some(g);
        }
    }
    let mut not_sized: Vec<Option<String>> = vec![None; groups.len()];
    let lists: Vec<Option<Vec<Candidate>>> = groups
        .iter()
        .enumerate()
        .map(|(k, g)| {
            if s.keep.contains(&g.name) {
                not_sized[k] = Some("left as it is (asked)".into());
                return None;
            }
            // Capacities written by hand describe the section they were
            // written for: never kept on another one, never removed
            // silently — the group is left, said.
            if let Some(m) = index[k]
                .iter()
                .map(|&i| &f0.model.members[i])
                .find(|m| m.capacity_tension.is_some() || m.capacity_compression.is_some())
            {
                not_sized[k] = Some(format!(
                    "left as it is: member {} has capacities written by hand (capacity_tension, \
                     capacity_compression), which describe its section {} — remove them to size \
                     the group",
                    m.id, g.section
                ));
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
    let bolt_lib = input
        .dir
        .map(|d| ohl_structures::bolts::BoltLibrary::beside(d).0)
        .unwrap_or_default();
    let mut bolt_plan = {
        let sets = sizing_bolts::sets(&f0.model, &index, &bolt_lib);
        let mut of_group = vec![Vec::new(); groups.len()];
        for (si, set) in sets.iter().enumerate() {
            of_group[set.group].push(si);
        }
        BoltPlan {
            now: vec![None; sets.len()],
            sets,
            of_group,
        }
    };
    let ids: Vec<String> = f0.model.members.iter().map(|m| m.id.clone()).collect();
    let joint_sizes = sizing_bolts::JointSizes::of(&f0.model, input.structure, &bolt_lib);
    let mut bolts_govern: Vec<Option<String>> = vec![None; groups.len()];
    let mut choice = current0.clone();
    let mut history: Vec<Vec<String>> = vec![choice.clone()];
    let mut rounds: Vec<Round> = Vec::new();
    let mut settled = None;
    let mut last = None;
    let mut f = f0;
    // The choice whose forces `f` are.
    let mut solved = current0.clone();
    // Per group the lightest section it may take again, and why: a lighter
    // one made the structure not solve (a buckling mode through it).
    let mut floor: Vec<Option<(String, String)>> = vec![None; groups.len()];
    let group_of: HashMap<&str, usize> = groups
        .iter()
        .enumerate()
        .flat_map(|(g, x)| x.members.iter().map(move |m| (m.as_str(), g)))
        .collect();
    let mass_of_id = |id: &str| {
        input
            .sections
            .section(id)
            .and_then(|x| mass_per_metre(input.sections, x))
            .unwrap_or(0.0)
    };
    // The forces of the structure as it is, kept for its joint fit.
    let mut f_first: Option<Forces> = None;
    for r in 0..s.rounds.max(MIN_ROUNDS) {
        let t_round = Instant::now();
        if r > 0 {
            progress(&format!("round {}: forces with the new sections", r + 1));
            let mut repairs = 0;
            let next = loop {
                let (text, _) = with_choice(
                    &doc,
                    &groups,
                    &choice,
                    &bolt_plan.assign(&bolt_plan.now, &ids),
                );
                match forces(input, &text, budget) {
                    Ok(x) => break x,
                    Err(e) => {
                        // A buckling mode with the lighter sections: the
                        // groups made lighter go back to the sections that
                        // solved (never lighter again), said — those of
                        // the members the mode names, else those meeting
                        // at the joints it moves most, else the nearer
                        // half of the groups made lighter (by their
                        // members' distance to those joints), after
                        // MAX_REPAIRS tries every group made lighter.
                        let is_lighter = |g: usize| mass_of_id(&choice[g]) < mass_of_id(&solved[g]);
                        let lighter = |ids: &[String]| -> Vec<usize> {
                            let mut out: Vec<usize> = ids
                                .iter()
                                .filter_map(|m| group_of.get(m.as_str()).copied())
                                .filter(|&g| is_lighter(g))
                                .collect();
                            out.sort_unstable();
                            out.dedup();
                            out
                        };
                        let all: Vec<usize> =
                            (0..groups.len()).filter(|&g| is_lighter(g)).collect();
                        repairs += 1;
                        let mut back = Vec::new();
                        if repairs < MAX_REPAIRS {
                            back = lighter(&e.members);
                            if back.is_empty() {
                                back = lighter(&e.around);
                            }
                            if back.is_empty() && !e.places.is_empty() && all.len() > 1 {
                                let model = &f.model;
                                let near = |g: usize| -> f64 {
                                    index[g]
                                        .iter()
                                        .map(|&k| {
                                            let m = &model.members[k];
                                            let (a, b) =
                                                (model.nodes[m.i].point, model.nodes[m.j].point);
                                            let c = [
                                                (a[0] + b[0]) / 2.0,
                                                (a[1] + b[1]) / 2.0,
                                                (a[2] + b[2]) / 2.0,
                                            ];
                                            e.places
                                                .iter()
                                                .map(|p| {
                                                    (p[0] - c[0]).powi(2)
                                                        + (p[1] - c[1]).powi(2)
                                                        + (p[2] - c[2]).powi(2)
                                                })
                                                .fold(f64::INFINITY, f64::min)
                                        })
                                        .fold(f64::INFINITY, f64::min)
                                };
                                let mut by: Vec<(f64, usize)> =
                                    all.iter().map(|&g| (near(g), g)).collect();
                                by.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                                back = by.iter().take(all.len().div_ceil(2)).map(|x| x.1).collect();
                            }
                        }
                        if back.is_empty() {
                            back = all;
                        }
                        if back.is_empty() {
                            return Err(format!(
                                "{} — with the sections chosen in round {r} (the structure as it \
                                 is solves): no lighter group left to take back",
                                e.words
                            ));
                        }
                        progress(&format!(
                            "round {}: {} group(s) taken back (the structure did not solve with \
                             them lighter)",
                            r + 1,
                            back.len()
                        ));
                        for g in back {
                            floor[g] = Some((
                                solved[g].clone(),
                                format!(
                                    "with {} the structure did not solve ({})",
                                    choice[g], e.words
                                ),
                            ));
                            choice[g] = solved[g].clone();
                        }
                    }
                }
            };
            solved = choice.clone();
            let old = std::mem::replace(&mut f, next);
            if f_first.is_none() {
                f_first = Some(old);
            }
        }
        let p = pick(
            input, &doc, &groups, &lists, &current0, &f, &index, &part_of, &floor, &rules,
            &mut cache, budget, progress, &bolt_plan, &ids,
        )?;
        bolt_plan.now = p.bolts.clone();
        for (g, w) in p.bolts_govern.iter().enumerate() {
            if w.is_some() {
                bolts_govern[g] = w.clone();
            }
        }
        let changed = p.choice.iter().zip(&choice).filter(|(a, b)| a != b).count();
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
                s.rounds.max(MIN_ROUNDS)
            ))
        }
    };
    let last = last.expect("one round at least");
    let f_first = f_first.unwrap_or(f);
    // (Stage 1c) One bolt size per joint: the members their joints take
    // along to a larger size, those that cannot go (and why).
    progress("one bolt size per joint");
    let n_members = ids.len();
    let own = bolt_plan.per_member(&bolt_plan.now, n_members);
    let mut frozen: HashMap<usize, String> = HashMap::new();
    for (k, part) in part_of.iter().enumerate().take(n_members) {
        let sized = part.is_some_and(|g| lists[g].is_some());
        let proposable = bolt_plan
            .sets
            .iter()
            .any(|set| set.base.is_ok() && set.members.contains(&k));
        if !sized {
            frozen.insert(k, "its group is not sized in this run".into());
        } else if !proposable {
            frozen.insert(k, "its bolts are not the sizer's to change".into());
        }
    }
    let along =
        |frozen: &HashMap<usize, String>| -> (BTreeMap<usize, BoltPick>, Vec<(String, String)>) {
            let mut taken: BTreeMap<usize, BoltPick> = BTreeMap::new();
            let mut stops: Vec<(String, String)> = Vec::new();
            for (si, set) in bolt_plan.sets.iter().enumerate() {
                let (Ok(base), Some(p)) = (&set.base, &bolt_plan.now[si]) else {
                    continue;
                };
                if sizing_bolts::same_size_key(&p.key(), &base.key()) {
                    continue;
                }
                let (t, st) = sizing_bolts::same_size(
                    &f_first.model,
                    &joint_sizes,
                    &set.members,
                    &base.key(),
                    p,
                    &own,
                    frozen,
                );
                for (k, b) in t {
                    if own[k]
                        .as_ref()
                        .is_none_or(|o| !sizing_bolts::same_size_key(&o.key(), &b.key()))
                    {
                        taken.entry(k).or_insert(b);
                    }
                }
                for x in st {
                    if !stops.contains(&x) {
                        stops.push(x);
                    }
                }
            }
            (taken, stops)
        };
    let with_taken = |taken: &BTreeMap<usize, BoltPick>| -> Vec<(String, BoltPick)> {
        let mut v = bolt_plan.assign(&bolt_plan.now, &ids);
        v.extend(taken.iter().map(|(k, b)| (ids[*k].clone(), b.clone())));
        v
    };
    let (mut taken, mut stops) = along(&frozen);
    // Taken along only where their leg and length take the larger bolt
    // (the joint-fit limits); else they keep theirs, said.
    for _ in 0..4 {
        if taken.is_empty() || !s.joint_fit {
            break;
        }
        let (text, _) = with_choice(&doc, &groups, &choice, &with_taken(&taken));
        let t = {
            use ohl_structures::LibraryItem as _;
            StructureType::parse(Path::new("structure.ohls"), &text).map_err(|e| e.message)?
        };
        let model = crate::model_of(&t, &text, input.dir, input.sections)?;
        let members: Vec<usize> = taken.keys().copied().collect();
        let (fit, _) = joint_fit_cached(
            &model,
            &members,
            &f_first.axial,
            &rules,
            &|k| part_of.get(k).copied().flatten(),
            &mut cache,
        );
        let mut more = false;
        for p in &fit {
            if matches!(p.rule, FitRule::Length | FitRule::LegTakesBolt)
                && let Some(b) = taken.get(&p.member)
                && !frozen.contains_key(&p.member)
            {
                frozen.insert(
                    p.member,
                    format!(
                        "its leg or length does not take {} bolts (the joint-fit limits)",
                        sizing_bolts::size_words(&b.key())
                    ),
                );
                more = true;
            }
        }
        if !more {
            break;
        }
        (taken, stops) = along(&frozen);
    }
    // The final run: a full analysis with the chosen sections.
    progress("the final run with the chosen sections");
    let (before_run, _) = full_run(input, input.text, budget)?;
    let (mut text, mut removed) = with_choice(&doc, &groups, &choice, &with_taken(&taken));
    let (mut after_run, _) = full_run(input, &text, budget)?;
    let before = usages(&before_run)?;
    let mut after = usages(&after_run)?;
    // A member taken along whose check fails with the larger bolt (its net
    // section with the larger hole): it keeps its bolts, said; once.
    if !taken.is_empty() {
        let mut more = false;
        for (k, b) in &taken {
            if let Some((Some(u), _, _)) = after.get(&ids[*k])
                && *u > s.target * (1.0 + 1e-12)
            {
                frozen.insert(
                    *k,
                    format!(
                        "with {} bolts its check fails ({:.0} %)",
                        sizing_bolts::size_words(&b.key()),
                        u * 100.0
                    ),
                );
                more = true;
            }
        }
        if more {
            (taken, stops) = along(&frozen);
            (text, removed) = with_choice(&doc, &groups, &choice, &with_taken(&taken));
            after_run = full_run(input, &text, budget)?.0;
            after = usages(&after_run)?;
        }
    }
    let joint_sizes_words = {
        let none = vec![None; n_members];
        let mut final_mb = own.clone();
        for (k, b) in &taken {
            final_mb[*k] = Some(b.clone());
        }
        let (single0, mixed0) = joint_sizes.count(&f_first.model, &none);
        let (single1, mixed1) = joint_sizes.count(&f_first.model, &final_mb);
        let before_mixed: BTreeMap<&String, &String> = mixed0.iter().map(|(j, w)| (j, w)).collect();
        JointBoltSizes {
            single: [single0, single1],
            mixed: [mixed0.len(), mixed1.len()],
            taken_along: taken.len(),
            mixed_after: mixed1
                .iter()
                .map(|(j, w)| {
                    let why: Vec<&str> = stops
                        .iter()
                        .filter(|x| &x.0 == j)
                        .map(|x| x.1.as_str())
                        .collect();
                    let words = if !why.is_empty() {
                        format!("{w} — {}", why.join("; "))
                    } else if before_mixed.get(j) == Some(&w) {
                        format!("{w} — as before")
                    } else {
                        format!("{w} — a member's bolts go to another size at its other joint")
                    };
                    (j.clone(), words)
                })
                .collect(),
        }
    };
    let f_after = forces(input, &text, budget).map_err(|e| e.words)?;
    let all: Vec<usize> = index.iter().flatten().copied().collect();
    let part = |k: usize| part_of.get(k).copied().flatten();
    let (fit0, unknown0) = joint_fit_cached(
        &f_first.model,
        &all,
        &f_first.axial,
        &rules,
        &part,
        &mut cache,
    );
    let (fit1, _) = joint_fit_cached(
        &f_after.model,
        &all,
        &f_after.axial,
        &rules,
        &part,
        &mut cache,
    );
    let per_member = |fit: &[FitProblem]| -> HashMap<usize, usize> {
        let mut out = HashMap::new();
        for p in fit {
            *out.entry(p.member).or_default() += 1;
        }
        out
    };
    let (by0, by1) = (per_member(&fit0), per_member(&fit1));
    let count = |by: &HashMap<usize, usize>, ks: &[usize]| -> usize {
        ks.iter().map(|k| by.get(k).copied().unwrap_or(0)).sum()
    };
    let words = |fit: &[FitProblem], ks: &[usize]| -> Vec<String> {
        let ks: std::collections::HashSet<usize> = ks.iter().copied().collect();
        fit.iter()
            .filter(|p| ks.contains(&p.member))
            .map(|p| p.words.clone())
            .collect()
    };
    let length_of = |ks: &[usize]| -> f64 {
        ks.iter()
            .map(|&k| f_first.model.length(&f_first.model.members[k]))
            .sum()
    };
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
    let mut target_heavier = 0;
    let mut target_mass = 0.0;
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
        if let Some(w) = last.at_one[g].as_ref().filter(|w| **w != to) {
            target_heavier += 1;
            target_mass += mass_of(&to, length) - mass_of(w, length);
        }
        let (why, basis) = match &not_sized[g] {
            Some(w) => (w.clone(), Basis::Left),
            None if last.choice[g] != to => (
                format!(
                    "{} (the heavier of the last choices: {})",
                    last.why[g],
                    match &settled {
                        Settled::Cycle(_) => "a cycle",
                        _ => "not settled",
                    }
                ),
                last.basis[g],
            ),
            None => (last.why[g].clone(), last.basis[g]),
        };
        // Kept at a section a lighter one made the structure not solve
        // with: said.
        let why = match &floor[g] {
            Some((at, words)) if *at == to => {
                format!("{why}; not lighter than {at}: {words}")
            }
            _ => why,
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
            fit: [count(&by0, &index[g]), count(&by1, &index[g])],
            problems: [words(&fit0, &index[g]), words(&fit1, &index[g])],
            without_fit,
            why,
            basis,
            removed: removed[g]
                .iter()
                .map(|f| format!("design {f} (it described {})", grp.section))
                .collect(),
            over_target: u1.is_none_or(|u| u > s.target * (1.0 + 1e-12)),
            not_sized: not_sized[g].is_some(),
            bolts: {
                let mut v: Vec<BoltChange> = Vec::new();
                for &si in &bolt_plan.of_group[g] {
                    let set = &bolt_plan.sets[si];
                    if let (Ok(b), Some(p)) = (&set.base, &bolt_plan.now[si])
                        && p != b
                    {
                        v.push(BoltChange {
                            from: b.words(),
                            to: p.words(),
                            pick: p.clone(),
                            members: set.members.iter().map(|&k| ids[k].clone()).collect(),
                            why: bolts_govern[g]
                                .clone()
                                .unwrap_or_else(|| "the bolts govern".into()),
                        });
                    }
                }
                // Taken along by their joints (one bolt size per joint).
                let mut along_here: BTreeMap<(String, String), (BoltPick, Vec<String>)> =
                    BTreeMap::new();
                for &k in &index[g] {
                    if let Some(b) = taken.get(&k) {
                        let from = f_first.model.members[k]
                            .design
                            .bolts
                            .map(|x| format!("{} × {:.0} mm", x.count, x.diameter * 1e3))
                            .unwrap_or_default();
                        along_here
                            .entry((from, b.words()))
                            .or_insert_with(|| (b.clone(), Vec::new()))
                            .1
                            .push(ids[k].clone());
                    }
                }
                for ((from, to), (pick, members)) in along_here {
                    v.push(BoltChange {
                        from,
                        to: to.clone(),
                        pick,
                        why: format!(
                            "one bolt size per joint: its joints' other bolts go to {to}'s size \
                             (the same count of the larger bolt)"
                        ),
                        members,
                    });
                }
                v
            },
            bolts_govern: bolts_govern[g].clone(),
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
    let worst = |u: &Usages| {
        u.values()
            .filter_map(|x| x.0)
            .fold(None, |w: Option<f64>, x| Some(w.map_or(x, |w| w.max(x))))
    };
    let mut notes = Vec::new();
    if f_first.model.capacity_plugin.is_none() {
        notes.push(SIMPLE_NOTE.to_owned());
    }
    if let Some(why) = &last.limits_note {
        notes.push(format!(
            "NOT ON THE SAFE SIDE: the code's limits on the members' sizes (slenderness, flat \
             width to thickness, least thickness) are not checked — {why}; a chosen section can \
             pass its usage and still break such a limit of the code (a slender member the code \
             does not allow); a code plugin with the member-limits export (the bundled asce10 \
             and sia263) checks them: load it and choose again"
        ));
    }
    if !s.joint_fit {
        notes.push(
            "joint-fit limits off: a chosen member may not take its bolts or be long enough \
             for them (its joints may not be buildable)"
                .to_owned(),
        );
    } else {
        notes.push(format!("joint fit by {}", rules.source));
        if let Some(r) = &rules.refused {
            notes.push(r.clone());
        }
        let practice = fit1.iter().filter(|p| p.rule == FitRule::Practice).count();
        if practice > 0 {
            notes.push(format!(
                "{practice} member(s) with legs thicker than every plate the detailing practice \
                 lists: their ends onto a plate cannot be laid out (list a thicker plate in \
                 Structures › Detailing practice, or detail them by hand)"
            ));
        }
    }
    let bolted = changes
        .iter()
        .filter(|c| {
            c.bolts
                .iter()
                .any(|b| b.why.starts_with("the bolts govern"))
        })
        .count();
    if bolted > 0 {
        notes.push(format!(
            "{bolted} group(s) where the bolts govern (bolt shear, which a heavier section does \
             not raise): more bolts or a larger bolt proposed, within the joint-fit limits; Apply \
             writes them with the sections"
        ));
    }
    let not_met = changes
        .iter()
        .filter(|c| c.basis == Basis::FitNotMet)
        .count();
    if not_met > 0 {
        notes.push(format!(
            "{not_met} group(s) with no section of their family both passing and fitting their \
             joints: the lightest passing taken, their joint-fit problems said (their joints to \
             detail by hand)"
        ));
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
    if input.structure.detail() == ohl_structures::Detail::Joints {
        notes.push(
            "level 4: the joints' details were laid out for the sections as they are — after \
             Apply, a changed member's plates, bolts and cuts no longer match it; lay the joints \
             out again (Design › Lay out joints) and check them"
                .to_owned(),
        );
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
    let target_effect = if s.target < 1.0 {
        if target_heavier == 0 {
            format!(
                "the target {:.0} % made no group heavier than 100 % would",
                s.target * 100.0
            )
        } else {
            format!(
                "the target {:.0} % kept {target_heavier} group(s) heavier than 100 % would \
                 (+{target_mass:.0} kg, with the last round's forces)",
                s.target * 100.0
            )
        }
    } else {
        "the target 100 %: each section up to its capacity".to_owned()
    };
    {
        let j = &joint_sizes_words;
        notes.push(format!(
            "one bolt size per joint (the owner, 2026-11-27): joints with one bolt size {} to {}, \
             with several {} to {}{}",
            j.single[0],
            j.single[1],
            j.mixed[0],
            j.mixed[1],
            if j.taken_along > 0 {
                format!(
                    "; {} member(s) whose joints take their bolts along to a larger size",
                    j.taken_along
                )
            } else {
                String::new()
            }
        ));
    }
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
        target_effect,
        text,
        took: started.elapsed(),
        joint_sizes: joint_sizes_words,
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
        if !c.changed() || !accepted.contains(&c.group.name) {
            continue;
        }
        for id in &c.group.members {
            if member_table(&mut doc, id).is_none() {
                return Err(format!("member {id} is no longer in the file: size again"));
            }
            if c.from != c.to {
                set_section(&mut doc, id, &c.to);
            }
            for b in c.bolts.iter().filter(|b| b.members.contains(id)) {
                sizing_bolts::set_bolts(&mut doc, id, &b.pick);
            }
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
    out.push(sizing.target_effect.clone());
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
            if c.case[1].is_empty() {
                "-"
            } else {
                &c.case[1]
            },
            c.fit[0],
            c.fit[1],
            c.why,
            c.without_fit.as_ref().map_or(String::new(), |w| format!(
                "; without the joint-fit limits {w}"
            )),
            if c.removed.is_empty() {
                String::new()
            } else {
                format!("; removed: {}", c.removed.join(", "))
            }
        ));
        for b in &c.bolts {
            out.push(format!(
                "    bolts {} → {} ({} member(s)) — {}",
                b.from,
                b.to,
                b.members.len(),
                b.why
            ));
        }
        if c.bolts.is_empty()
            && let Some(w) = &c.bolts_govern
        {
            out.push(format!("    {w}"));
        }
    }
    for (j, w) in &sizing.joint_sizes.mixed_after {
        out.push(format!("BOLT SIZES MIXED: joint {j}: {w}"));
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
