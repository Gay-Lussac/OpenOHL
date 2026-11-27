// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **The member sizer when the bolts govern** (O49 stage 1b, 2026-11-27;
//! the owner's words of that day: "Are the bolt capacities checked too?",
//! then "Yes please"; built, in active development, not approved yet).
//!
//! A member's bolt shear capacity — its bolts' count, shear planes and
//! per-plane capacity — does not depend on its section: a group failing
//! by bolt shear is not made heavier for it. The code's plugin names
//! what governs each capacity (`bolt shear`, `bearing`, `block shear`,
//! `net section`, the column's KL/r): where every member of a group above
//! the target fails by the capacity its plugin says bolt shear governs,
//! **the bolts govern**: the sizer proposes more of the members' own bolts
//! (`[member.design] bolts`), and where they do not fit the member (the
//! joint-fit limits: its length for the code's end distances and spacing,
//! its leg for the bolt), a larger bolt of the project's bolt library;
//! the plugin's check of the next trial is the judge. Bearing, block shear
//! and the net section depend on the section's thickness and area: there
//! a heavier section can help, and the sizer goes on as before.
//!
//! Proposals only, as the sections: Choose the members… and
//! `ohl-structure-size` show them; Apply writes them in the same undo step.
//! Members whose bolts are given at their ends (`[[connection]]`, level 4)
//! or not given have no bolt proposal (said).
//!
//! **Per bolt size within a group** (O49 stage 1c, the owner's answer of
//! 2026-11-27): a group's members are taken by their bolts' size — the
//! diameter and the grade ([`SizeKey`]) — each size its own proposal
//! ([`BoltSet`], [`sets`]): its count, else the next larger bolt.
//!
//! **One bolt size per joint wherever it can** (the owner, 2026-11-27:
//! "for the bolts please try to use the same ones for a joint. this would
//! ease the construction once outside"): more bolts of the joint's size
//! first (the count above); where a member's bolts go to a larger size,
//! every other bolt of the old size at its joints goes with it, and on
//! through the joints those reach that had one size ([`same_size`], the
//! same count of the larger bolt: never less bolt shear or bearing);
//! mixed sizes only where one size cannot be had — a joint with sizes
//! mixed already, bolts laid out at the joint (`[[connection]]`, plates),
//! a member the sizer does not change, a member whose leg or length does
//! not take the larger bolt (the joint-fit limits) or whose check fails
//! with it — each said with the reason ([`JointSizes`] counts the joints
//! with one size and with several, before and after).

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use ohl_structures::StructureType;
use ohl_structures::bolts::BoltLibrary;
use ohl_structures_fe::Model;
use toml_edit::DocumentMut;

/// The most bolt proposals tried for one group in one round (the
/// program's bound on the work, not a standard's).
pub const MAX_BOLT_TRIES: usize = 12;

/// The word of a plugin's capacity governed by its bolts' shear (the
/// bundled `asce10` and `sia263` both say it).
pub const BOLT_SHEAR: &str = "bolt shear";

/// A group's bolts: every member's own bolts (`[member.design] bolts`)
/// as these, at least `count` per end.
#[derive(Debug, Clone, PartialEq)]
pub struct BoltPick {
    /// Bolts per end, at least.
    pub count: u32,
    /// The bolt's diameter, m.
    pub diameter: f64,
    /// Its hole, m (none: not given — the code's standard hole).
    pub hole: Option<f64>,
    /// One bolt's shear capacity per plane, N (none: the code works it out
    /// from the bolt's strength).
    pub shear_capacity: Option<f64>,
    /// The library bolt (none: the members' own bolt, more of them).
    pub bolt: Option<String>,
    /// Its grade as the files name it (none: not given).
    pub grade: Option<String>,
}

/// A bolt's size (O49 stage 1c): its diameter in tenths of a millimetre
/// and its grade's name ("" not given). A grade not given is the same size
/// as any grade of that diameter ([`same_size_key`]): only a grade given on
/// both sides tells two bolts of one diameter apart (a shear capacity does
/// not: it also depends on the threads in the shear plane).
pub type SizeKey = (i64, String);

/// The size of a bolt of diameter `d` (m) and grade `grade` (`capacity`,
/// its shear capacity per plane, is not a size: kept for the callers'
/// symmetry).
pub fn size_key(d: f64, grade: Option<&str>, capacity: Option<f64>) -> SizeKey {
    let _ = capacity;
    let g = grade
        .map(str::trim)
        .filter(|g| !g.is_empty())
        .unwrap_or_default()
        .to_owned();
    ((d * 1e4).round() as i64, g)
}

/// Two sizes are the same bolt size: the same diameter, and the same grade
/// where both give one.
pub fn same_size_key(a: &SizeKey, b: &SizeKey) -> bool {
    a.0 == b.0 && (a.1 == b.1 || a.1.is_empty() || b.1.is_empty())
}

/// `keys` with a size whose grade is not given dropped where a size of the
/// same diameter with a grade is there (the same bolt size).
fn merged(keys: BTreeSet<SizeKey>) -> BTreeSet<SizeKey> {
    let graded: Vec<i64> = keys
        .iter()
        .filter(|k| !k.1.is_empty())
        .map(|k| k.0)
        .collect();
    keys.into_iter()
        .filter(|k| !k.1.is_empty() || !graded.contains(&k.0))
        .collect()
}

/// A size in words: "16 mm", "16 mm 8.8".
pub fn size_words(k: &SizeKey) -> String {
    let d = k.0 as f64 / 10.0;
    let mm = if (d - d.round()).abs() < 1e-9 {
        format!("{d:.0} mm")
    } else {
        format!("{d:.1} mm")
    };
    if k.1.is_empty() {
        mm
    } else {
        format!("{mm} {}", k.1)
    }
}

impl BoltPick {
    /// Its size.
    pub fn key(&self) -> SizeKey {
        size_key(self.diameter, self.grade.as_deref(), self.shear_capacity)
    }

    /// In words: "3 × 16 mm", "2 × 20 mm (M20)".
    pub fn words(&self) -> String {
        format!(
            "{} × {:.0} mm{}",
            self.count,
            self.diameter * 1e3,
            self.bolt
                .as_ref()
                .map_or(String::new(), |b| format!(" ({b})"))
        )
    }
}

/// A bolt proposal for some members of a group, for the window and the
/// command line.
#[derive(Debug, Clone, PartialEq)]
pub struct BoltChange {
    /// Its bolts now, in words.
    pub from: String,
    /// Proposed, in words.
    pub to: String,
    /// The bolts written by Apply.
    pub pick: BoltPick,
    /// The members it is for (the group's members of one bolt size, or
    /// those its joints take to another size).
    pub members: Vec<String>,
    /// Why, in words ("the bolts govern …", "one bolt size per joint: …").
    pub why: String,
}

/// The size of member `m`'s own bolts (`[member.design] bolts`; none: no
/// bolts, or none counted).
pub fn member_key(m: &ohl_structures_fe::model::Member) -> Option<SizeKey> {
    let b = m.design.bolts?;
    (b.count > 0).then(|| {
        size_key(
            b.diameter,
            b.grade.map(|g| g.name()).as_deref(),
            b.shear_capacity,
        )
    })
}

/// A member's own bolts as a pick (its count, its size).
fn pick_of(m: &ohl_structures_fe::model::Member) -> Option<BoltPick> {
    let b = m.design.bolts?;
    Some(BoltPick {
        count: b.count,
        diameter: b.diameter,
        hole: b.hole,
        shear_capacity: b.shear_capacity,
        bolt: None,
        grade: b.grade.map(|g| g.name()),
    })
}

/// Why member `m` has no bolt proposal (none: it may have one).
fn no_proposal(m: &ohl_structures_fe::model::Member) -> Option<String> {
    if m.design.ends.iter().flatten().any(|e| e.bolts.is_some()) {
        return Some(
            "bolts given at the ends ([[connection]]): lay out the joints again with more bolts"
                .into(),
        );
    }
    match m.design.bolts {
        None => Some("no bolts in the design data".into()),
        Some(b) if b.count == 0 => Some("no bolts counted in the design data (holes only)".into()),
        Some(_) => None,
    }
}

/// A group's own bolts as they are (the base of its proposals); Err: why
/// it has no bolt proposal (O49 stage 1b; since stage 1c the members of
/// one size, [`sets`]).
pub fn base(model: &Model, members: &[usize]) -> Result<BoltPick, String> {
    let mut out: Option<BoltPick> = None;
    for &k in members {
        let m = &model.members[k];
        if let Some(w) = no_proposal(m) {
            return Err(format!("{}: {w}", m.id));
        }
        let Some(b) = pick_of(m) else {
            return Err(format!("{} has no bolts in its design data", m.id));
        };
        match &mut out {
            None => out = Some(b),
            Some(o) => {
                if o.key() != b.key() {
                    return Err("its members' bolts differ in size".into());
                }
                o.count = o.count.max(b.count);
            }
        }
    }
    out.ok_or_else(|| "no member".into())
}

/// The members of one group with bolts of one size (O49 stage 1c): one
/// proposal each.
#[derive(Debug, Clone, PartialEq)]
pub struct BoltSet {
    /// The group (its place in the sizer's groups).
    pub group: usize,
    /// Its members (places in the model).
    pub members: Vec<usize>,
    /// Their bolts as they are (the largest count), or why they have no
    /// proposal.
    pub base: Result<BoltPick, String>,
    /// The larger bolts of the library they may take ([`options`]).
    pub options: Vec<BoltPick>,
}

/// Every group's bolt sets (`index`: each group's members' places in the
/// model): its members by their bolts' size, each size a set; members
/// without a proposal possible a set each reason (its `base` the reason).
pub fn sets(model: &Model, index: &[Vec<usize>], lib: &BoltLibrary) -> Vec<BoltSet> {
    let mut out = Vec::new();
    for (g, ks) in index.iter().enumerate() {
        let mut by_key: BTreeMap<SizeKey, Vec<usize>> = BTreeMap::new();
        let mut by_why: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for &k in ks {
            let m = &model.members[k];
            match (no_proposal(m), member_key(m)) {
                (None, Some(key)) => by_key.entry(key).or_default().push(k),
                (Some(w), _) => by_why.entry(w).or_default().push(k),
                (None, None) => by_why
                    .entry("no bolts in the design data".into())
                    .or_default()
                    .push(k),
            }
        }
        for (_, members) in by_key {
            let base = base(model, &members);
            let options = base.as_ref().map_or(Vec::new(), |b| options(lib, b));
            out.push(BoltSet {
                group: g,
                members,
                base,
                options,
            });
        }
        for (why, members) in by_why {
            let ids: Vec<&str> = members
                .iter()
                .map(|&k| model.members[k].id.as_str())
                .collect();
            out.push(BoltSet {
                group: g,
                base: Err(format!("{}: {why}", ids.join(", "))),
                members,
                options: Vec::new(),
            });
        }
    }
    out
}

/// The larger bolts of the project's library a group may take instead of
/// its own `base` (its next proposals when more of its own do not fit):
/// one per diameter above the base's, the lightest-labelled with a shear
/// capacity with the threads in the shear plane where one is given (on
/// the safe side), smallest first.
pub fn options(lib: &BoltLibrary, base: &BoltPick) -> Vec<BoltPick> {
    let mut out: Vec<BoltPick> = Vec::new();
    let mut bolts: Vec<&ohl_structures::bolts::BoltType> = lib
        .bolts
        .iter()
        .filter(|b| b.diameter.si() > base.diameter + 1e-9)
        .collect();
    bolts.sort_by(|a, b| {
        a.diameter
            .si()
            .total_cmp(&b.diameter.si())
            .then(b.shear_capacity.is_some().cmp(&a.shear_capacity.is_some()))
            .then(a.id.cmp(&b.id))
    });
    for b in bolts {
        if out
            .last()
            .is_some_and(|o| (o.diameter - b.diameter.si()).abs() < 1e-9)
        {
            continue;
        }
        out.push(BoltPick {
            count: base.count,
            diameter: b.diameter.si(),
            hole: b.hole.map(|h| h.si()),
            shear_capacity: b.shear_capacity.map(|f| f.si()),
            bolt: Some(b.id.clone()),
            grade: b.grade.clone(),
        });
    }
    out
}

/// The count of bolt `to` passing as `count` bolts of `from` do: by their
/// per-plane capacities where both give one, else by the bolts' areas
/// (d²); at least one.
pub fn count_for(from: &BoltPick, count: u32, to: &BoltPick) -> u32 {
    let ratio = match (from.shear_capacity, to.shear_capacity) {
        (Some(a), Some(b)) if b > 0.0 => a / b,
        _ => (from.diameter / to.diameter).powi(2),
    };
    ((f64::from(count) * ratio) - 1e-9).ceil().max(1.0) as u32
}

/// A member's capacities as its plugin gave them: (value, N; what governs
/// it) in tension and in compression.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Caps {
    /// Tension.
    pub tension: Option<(f64, String)>,
    /// Compression.
    pub compression: Option<(f64, String)>,
}

/// How far the bolts are short where they alone make a group fail: over
/// the members above the target (`usage`), each failing by the capacity
/// (tension or compression, the larger of force over capacity, the forces
/// `tension`, `compression` of the case's strength) its plugin says
/// [`BOLT_SHEAR`] governs, the largest force over capacity over the target;
/// a member within both capacities by its own forces but above the target
/// (its usage from a nominal force, e.g. ASCE 10-97's on a redundant
/// member) counted where the bolt shear governs every capacity it has; how
/// far short by the plugin's usage where larger; none where any of them
/// fails by another check (a heavier section may help there), or none
/// fails.
pub fn short_by(
    members: &[usize],
    usage: &[Option<f64>],
    caps: &[Option<Caps>],
    tension: &[f64],
    compression: &[f64],
    target: f64,
) -> Option<f64> {
    let mut worst: Option<f64> = None;
    for &k in members {
        let u = usage.get(k).copied().flatten()?;
        if u <= target * (1.0 + 1e-12) {
            continue;
        }
        let c = caps.get(k)?.as_ref()?;
        let ratio = |cap: &Option<(f64, String)>, f: f64| {
            cap.as_ref()
                .filter(|c| c.0 > 0.0)
                .map(|c| (f / c.0, c.1.contains(BOLT_SHEAR)))
        };
        let t = ratio(&c.tension, tension.get(k).copied().unwrap_or(0.0));
        let p = ratio(&c.compression, compression.get(k).copied().unwrap_or(0.0));
        let (r, bolts) = match (t, p) {
            (Some(a), Some(b)) => {
                if a.0 >= b.0 {
                    a
                } else {
                    b
                }
            }
            (Some(a), None) | (None, Some(a)) => a,
            (None, None) => return None,
        };
        // Its own forces within both capacities, its usage above (a
        // nominal force, e.g. ASCE 10-97's on a redundant member): the
        // bolts govern only where they govern every capacity it has.
        let bolts = if r <= target * (1.0 + 1e-12) {
            [t, p].iter().flatten().all(|x| x.1)
        } else {
            bolts
        };
        if !bolts {
            return None;
        }
        // The plugin's usage is the judge of how far they are short.
        let r = u.max(r);
        worst = Some(worst.map_or(r / target, |w: f64| w.max(r / target)));
    }
    worst
}

/// Member `id`'s own bolts in `doc` set to `pick`: the same bolt, at least
/// `pick.count` of them; another bolt, `pick.count` of it with its hole
/// and shear capacity (a capacity or hole given for the old bolt removed).
/// False: no such member or bolts.
pub fn set_bolts(doc: &mut DocumentMut, id: &str, pick: &BoltPick) -> bool {
    let Some(t) = doc
        .get_mut("member")
        .and_then(|m| m.as_array_of_tables_mut())
        .and_then(|a| {
            a.iter_mut()
                .find(|t| t.get("id").and_then(|v| v.as_str()) == Some(id))
        })
    else {
        return false;
    };
    let Some(design) = t.get_mut("design") else {
        return false;
    };
    let bolts = match design {
        toml_edit::Item::Value(toml_edit::Value::InlineTable(d)) => d.get_mut("bolts"),
        toml_edit::Item::Table(d) => d.get_mut("bolts").and_then(|i| i.as_value_mut()),
        _ => None,
    };
    let Some(toml_edit::Value::InlineTable(b)) = bolts else {
        return false;
    };
    let len = |v: f64| toml_edit::Value::from(crate::mm_text(v));
    let diameter = b
        .get("diameter")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<ohl_core::units::Length>().ok())
        .map(|l| l.si());
    let count = b
        .get("count")
        .and_then(|v| v.as_integer())
        .unwrap_or(0)
        .max(0) as u32;
    let same = diameter.is_some_and(|d| (d - pick.diameter).abs() < 1e-9);
    if same {
        b.insert("count", i64::from(count.max(pick.count)).into());
    } else {
        b.insert("diameter", len(pick.diameter));
        b.insert("count", i64::from(pick.count).into());
        match pick.hole {
            Some(h) => {
                b.insert("hole", len(h));
            }
            None => {
                b.remove("hole");
            }
        }
        match pick.shear_capacity {
            Some(v) => {
                b.insert(
                    "shear_capacity",
                    toml_edit::Value::from(format!("{:.3} kN", v / 1e3)),
                );
            }
            None => {
                b.remove("shear_capacity");
            }
        }
        // (O49 stage 1c) The new bolt's grade, never the old one's.
        match &pick.grade {
            Some(g) => {
                b.insert("grade", toml_edit::Value::from(g.as_str()));
            }
            None => {
                b.remove("grade");
            }
        }
    }
    true
}

/// The joints of a structure and their bolts' sizes (O49 stage 1c, one
/// bolt size per joint).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JointSizes {
    /// Per joint the members ending there with bolts of their own
    /// (`[member.design] bolts`, places in the model).
    pub members: BTreeMap<String, Vec<usize>>,
    /// Per joint the sizes of bolts laid out there (a member end's
    /// `[[connection]]` bolts, a plate's own bolts), with whose.
    pub laid_out: BTreeMap<String, Vec<(SizeKey, String)>>,
}

impl JointSizes {
    /// The joints of `model` (and `t`'s plates' own bolts).
    pub fn of(model: &Model, t: &StructureType, lib: &BoltLibrary) -> Self {
        let mut out = Self::default();
        for (k, m) in model.members.iter().enumerate() {
            for (e, j) in joints_of(model, k).iter().enumerate() {
                match m
                    .design
                    .ends
                    .get(e)
                    .and_then(Option::as_ref)
                    .and_then(|x| x.bolts)
                {
                    Some(b) => {
                        if let Some(d) = b.diameter
                            && b.count > 0
                        {
                            let key =
                                size_key(d, b.grade.map(|g| g.name()).as_deref(), b.shear_capacity);
                            out.laid_out
                                .entry(j.clone())
                                .or_default()
                                .push((key, m.id.clone()));
                        }
                    }
                    None => {
                        if member_key(m).is_some() {
                            out.members.entry(j.clone()).or_default().push(k);
                        }
                    }
                }
            }
        }
        let conn = |b: &ohl_structures::connections::ConnectionBolts| -> Option<SizeKey> {
            let libb = b.bolt.as_deref().and_then(|id| lib.get(id));
            let d = b
                .diameter
                .map(|d| d.si())
                .or(libb.map(|x| x.diameter.si()))?;
            let grade = b.grade.clone().or(libb.and_then(|x| x.grade.clone()));
            let cap = libb.and_then(|x| x.shear_capacity).map(|f| f.si());
            Some(size_key(d, grade.as_deref(), cap))
        };
        for p in &t.plates {
            let mut add = |b: &ohl_structures::connections::ConnectionBolts, whose: String| {
                if b.count > 0
                    && let Some(k) = conn(b)
                {
                    out.laid_out
                        .entry(p.joint.clone())
                        .or_default()
                        .push((k, whose));
                }
            };
            for l in p.leg_bolts.iter().chain(p.also_bolts.iter()) {
                add(&l.bolts, format!("plate {}'s own bolts", p.id));
            }
            if let Some(q) = &p.paired
                && let Some(b) = &q.bolts
            {
                add(b, format!("plate {}'s own bolts", p.id));
            }
        }
        out
    }

    /// The sizes at joint `j` with `mb` taken (per model member its
    /// proposed bolts; none: its own).
    pub fn sizes_at(&self, model: &Model, j: &str, mb: &[Option<BoltPick>]) -> BTreeSet<SizeKey> {
        let mut out: BTreeSet<SizeKey> = BTreeSet::new();
        for &k in self.members.get(j).map_or(&[][..], Vec::as_slice) {
            if let Some(key) = key_now(model, k, mb) {
                out.insert(key);
            }
        }
        for (key, _) in self.laid_out.get(j).map_or(&[][..], Vec::as_slice) {
            out.insert(key.clone());
        }
        merged(out)
    }

    /// The joints with one size and those with several (each with its
    /// sizes in words), with `mb` taken.
    pub fn count(&self, model: &Model, mb: &[Option<BoltPick>]) -> (usize, Vec<(String, String)>) {
        let joints: BTreeSet<&String> = self.members.keys().chain(self.laid_out.keys()).collect();
        let mut single = 0;
        let mut mixed = Vec::new();
        for j in joints {
            let sizes = self.sizes_at(model, j, mb);
            match sizes.len() {
                0 => {}
                1 => single += 1,
                _ => mixed.push((
                    j.clone(),
                    sizes.iter().map(size_words).collect::<Vec<_>>().join(", "),
                )),
            }
        }
        (single, mixed)
    }
}

/// The joints at member `k`'s ends (its first, its second).
pub fn joints_of(model: &Model, k: usize) -> [String; 2] {
    let m = &model.members[k];
    [model.nodes[m.i].id.clone(), model.nodes[m.j].id.clone()]
}

/// Member `k`'s bolt size with `mb` taken (its proposed bolts, else its
/// own).
pub fn key_now(model: &Model, k: usize, mb: &[Option<BoltPick>]) -> Option<SizeKey> {
    match mb.get(k).and_then(Option::as_ref) {
        Some(p) => Some(p.key()),
        None => member_key(&model.members[k]),
    }
}

/// A joint where one bolt size cannot be had, and why.
pub type JointStop = (String, String);

/// One bolt size per joint (O49 stage 1c, [module docs](self)): where
/// members `start` go from size `old` to the bolts `new`, every member of
/// size `old` at their joints goes with them — the same count of `new` —
/// and on through the joints those reach that had size `old` alone; never
/// a member in `frozen` (the sizer may not change it, or it does not take
/// `new`). Returns the members taken along (place, pick) and, per joint
/// where one size cannot be had, why.
pub fn same_size(
    model: &Model,
    joints: &JointSizes,
    start: &[usize],
    old: &SizeKey,
    new: &BoltPick,
    mb: &[Option<BoltPick>],
    frozen: &HashMap<usize, String>,
) -> (Vec<(usize, BoltPick)>, Vec<JointStop>) {
    let mut taken: BTreeMap<usize, BoltPick> = BTreeMap::new();
    let mut stops: Vec<JointStop> = Vec::new();
    let moving: BTreeSet<usize> = start.iter().copied().collect();
    let mut queue: VecDeque<String> = start.iter().flat_map(|&k| joints_of(model, k)).collect();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let new_key = new.key();
    while let Some(j) = queue.pop_front() {
        if !seen.insert(j.clone()) {
            continue;
        }
        if let Some(l) = joints.laid_out.get(&j)
            && l.iter().any(|(k, _)| !same_size_key(k, &new_key))
        {
            let whose: Vec<&str> = l.iter().map(|x| x.1.as_str()).collect();
            stops.push((
                j.clone(),
                format!(
                    "its bolts laid out ({}) keep their size: lay the joint out again with {}",
                    whose.join(", "),
                    size_words(&new_key)
                ),
            ));
            continue;
        }
        let here: Vec<usize> = joints.members.get(&j).map_or(Vec::new(), |v| {
            v.iter()
                .copied()
                .filter(|k| !moving.contains(k) && !taken.contains_key(k))
                .collect()
        });
        let keys: BTreeSet<SizeKey> = here.iter().filter_map(|&k| key_now(model, k, mb)).collect();
        if keys
            .iter()
            .any(|k| !same_size_key(k, old) && !same_size_key(k, &new_key))
        {
            stops.push((
                j.clone(),
                format!(
                    "its bolts were of several sizes already ({})",
                    keys.iter().map(size_words).collect::<Vec<_>>().join(", ")
                ),
            ));
            continue;
        }
        if let Some((k, why)) = here
            .iter()
            .filter(|&&k| key_now(model, k, mb).is_some_and(|x| same_size_key(&x, old)))
            .find_map(|&k| frozen.get(&k).map(|w| (k, w)))
        {
            stops.push((
                j.clone(),
                format!(
                    "{} keeps its {} bolts: {why}",
                    model.members[k].id,
                    size_words(old)
                ),
            ));
            continue;
        }
        for &k in &here {
            if !key_now(model, k, mb).is_some_and(|x| same_size_key(&x, old))
                || key_now(model, k, mb).is_some_and(|x| same_size_key(&x, &new_key))
            {
                continue;
            }
            let count = mb
                .get(k)
                .and_then(Option::as_ref)
                .map(|p| p.count)
                .or(model.members[k].design.bolts.map(|b| b.count))
                .unwrap_or(new.count);
            taken.insert(
                k,
                BoltPick {
                    count,
                    ..new.clone()
                },
            );
            for o in &joints_of(model, k) {
                if !seen.contains(o) {
                    queue.push_back(o.clone());
                }
            }
        }
    }
    (taken.into_iter().collect(), stops)
}
