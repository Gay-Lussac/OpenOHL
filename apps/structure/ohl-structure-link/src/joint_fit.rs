// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **The joint-fit limits** of the member sizer (roadmap O49 stage 1,
//! 2026-11-27; built, in active development, not approved yet): what keeps
//! a chosen section connectable, worked out with the rules the joint
//! layout and the bolt tools already use, so that the sizer and Lay out
//! joints agree.
//!
//! - **The code's distances** of an end with bolts of its own:
//!   [`crate::member_view::rules_in`] — the very function Lay out joints'
//!   edge rule and the bolt tools take (the code plugin's `bolt-distances`,
//!   else the structure's default minimums, said). An end without bolts:
//!   OpenOHL's default bolt for the member (the plugin's
//!   `member-check-default-bolts`, [`ohl_member_check::default_bolts`])
//!   with the code's distances for it, never below the default minimums the
//!   layout draws a picked bolt with (the larger: on the safe side).
//! - **The detailing practice in force** (O44 L29: the structure's own
//!   `[layout_practice]` over its code plugin's, value by value, as Lay out
//!   joints takes it, [`practice_for`]): the bolts per end the layout gives
//!   an end without bolts (per member class) and the plate thicknesses the
//!   shop makes ([`crate::LayoutSettings::plate_thickness`], the layout's
//!   own function).
//!
//! The rules ([`joint_fit`]), per member:
//!
//! 1. **The leg takes its bolt** at the gauge the layout draws one line at
//!    (mid-leg): the hole clear of the root fillet (t + r + d/2 from the
//!    heel, [`ohl_structures_fe::bolt_rules::BoltRules::least_gauge`]) and
//!    the code's edge distance kept to the toe.
//! 2. **The member is long enough** between its joints for both ends'
//!    bolts in one line: per end n = the larger of its bolts (its own, else
//!    the practice's count for its class) and ⌈N / P⌉ (N its largest force
//!    over the cases, the strength factor taken out; P the force one bolt
//!    passes by its code), each end needing 2 e + (n − 1) s (e, s: the
//!    code's end distance and spacing).
//! 3. **A plate the shop makes** takes its leg: the practice's thicknesses
//!    list one at least as thick as the member's leg (a plate is never
//!    thinner than the thickest leg on it); none: an end onto a plate cannot
//!    be laid out, said.
//! 4. **A bolt**: a member without bolts of its own and without a default
//!    bolt (its leg takes none of the bolt library's) cannot be connected.
//!
//! What is assumed (the theory page marks each): one gauge line at
//! mid-leg; the length between the joints' work points (the steel is
//! shorter by its ends' cuts: a member passing may still be short where
//! the layout pulls its ends back — Lay out joints then says so).

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use ohl_structures_fe::Model;
use ohl_structures_fe::bolt_rules::{BoltRules, Force};

use crate::practice::{DetailingPractice, RecordedPractice};

/// The detailing practice's rules the joint fit keeps to.
#[derive(Debug, Clone, PartialEq)]
pub struct FitRules {
    /// The layout's settings with the practice applied, as Lay out joints
    /// runs with them ([`crate::practice::with_practice`]).
    layout: crate::LayoutSettings,
    /// The practice's bolts per end of the picked bolt.
    bolt_count: u32,
    /// Where the practice comes from, in words ("the default practice",
    /// "the structure's own over the plugin asce10's").
    pub source: String,
    /// Why a practice given is not used (refused, or the plugin failed):
    /// the default then, said.
    pub refused: Option<String>,
}

impl Default for FitRules {
    /// The default practice (the layout as before L24).
    fn default() -> Self {
        Self::of(None, None)
    }
}

impl FitRules {
    /// The rules of practice `p` (none: the default); `refused`: why one
    /// given is not used.
    pub fn of(p: Option<&RecordedPractice>, refused: Option<String>) -> Self {
        let layout = crate::practice::with_practice(crate::LayoutSettings::default(), p);
        let practice = p.map_or_else(DetailingPractice::default, |p| p.practice.clone());
        let source = match p {
            None => "the default detailing practice".to_owned(),
            Some(p) => format!("the detailing practice of {}", p.source.label()),
        };
        Self {
            layout,
            bolt_count: practice.bolt_count,
            source,
            refused,
        }
    }

    /// The rules of the practice in force for structure file `text` in
    /// folder `dir` ([`practice_for`]).
    pub fn for_structure(text: &str, dir: Option<&Path>) -> Self {
        let (p, refused) = practice_for(text, dir);
        Self::of(p.as_ref(), refused)
    }

    /// The bolts per end the layout gives an end of class `class` without
    /// bolts of its own or a connection preset: the practice's for the
    /// class, else its picked bolt's count.
    pub fn picked_count(&self, class: Option<&str>) -> u32 {
        class
            .and_then(|c| self.layout.class_bolts.get(c))
            .copied()
            .unwrap_or(self.bolt_count)
    }

    /// The thickness of a plate the layout makes on legs `legs` thick
    /// ([`crate::LayoutSettings::plate_thickness`]); Err: none listed that
    /// thick, why.
    pub fn plate(&self, legs: f64) -> Result<f64, String> {
        self.layout.plate_thickness(legs)
    }

    /// The practice gives rules of its own beyond the default (a bolt
    /// count, a class's count, plate thicknesses).
    pub fn has_rules(&self) -> bool {
        self.bolt_count != DetailingPractice::default().bolt_count
            || !self.layout.class_bolts.is_empty()
            || !self.layout.plate_thicknesses.is_empty()
    }
}

/// **The detailing practice in force** for structure file `text` in folder
/// `dir`, as Lay out joints takes it: the file's own `[layout_practice]`
/// over its code plugin's answer, value by value, over the default (none);
/// with why a practice given is not used (refused, or the plugin failed —
/// the default, or the file's own alone, then applies).
///
/// Expected cost: the file read and one plugin call per plugin, method and
/// parameter file (kept for the session).
pub fn practice_for(text: &str, dir: Option<&Path>) -> (Option<RecordedPractice>, Option<String>) {
    use ohl_structures::LibraryItem as _;
    let own = ohl_structures::StructureType::parse(Path::new("structure.ohls"), text)
        .ok()
        .and_then(|t| t.layout_practice);
    let (plugin, failed) = match crate::practice::plugin_practice(text, dir) {
        Ok(p) => (p, None),
        Err(e) => (None, Some(e)),
    };
    match crate::practice::in_force(own.as_ref(), plugin.as_ref()) {
        Ok(p) => (
            p,
            failed.map(|e| format!("{e}. The code plugin's detailing practice is not used.")),
        ),
        Err(e) => (None, Some(crate::practice::refused_note(&e))),
    }
}

/// The rule a joint-fit problem breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitRule {
    /// The leg does not take its bolt at the layout's gauge (the hole not
    /// clear of the root fillet, or the code's edge distance to the toe).
    LegTakesBolt,
    /// The member is not long enough for the bolts its ends need.
    Length,
    /// The leg takes no bolt of the structure's bolt library.
    NoBolt,
    /// A rule of the detailing practice: no plate the shop makes takes the
    /// leg.
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

/// What one end's bolts need, by its code: end distance, spacing, edge
/// distance (m), the force one bolt passes (N; none: not known), the
/// bolt's diameter, the bolted leg (width, thickness; none: not an angle),
/// the leg's root radius and the bolts the end has (none: no bolts of its
/// own).
#[derive(Debug, Clone, PartialEq)]
struct EndNeed {
    end: f64,
    spacing: f64,
    edge: f64,
    force: Option<f64>,
    diameter: f64,
    leg: Option<(f64, f64)>,
    root: f64,
    count: Option<u32>,
}

impl EndNeed {
    fn of(r: &BoltRules, count: Option<u32>) -> Self {
        Self {
            end: r.end,
            spacing: r.spacing,
            edge: r.edge,
            force: match r.force {
                Force::Capacity(p) | Force::Analysis(p) if p > 0.0 => Some(p),
                _ => None,
            },
            diameter: r.diameter,
            leg: r.leg,
            root: r.root,
            count,
        }
    }

    /// The least gauge from the heel: the hole clear of the root fillet.
    fn least_gauge(&self) -> f64 {
        self.leg.map_or(0.0, |l| l.1) + self.root + self.diameter / 2.0
    }
}

/// The answers asked, by what they depend on (few distinct keys in a
/// tower: one plugin call each).
#[derive(Debug, Default)]
pub struct FitCache {
    rules: HashMap<String, Option<EndNeed>>,
}

/// Millimetres for the words.
fn mm(x: f64) -> String {
    format!("{:.0}", x * 1000.0)
}

/// The code's needs of member `k`'s end `e` with bolts of its own:
/// [`crate::member_view::rules_in`], cached by its basis. None: the end
/// has no bolts of known diameter.
fn own_need(model: &Model, k: usize, e: usize, cache: &mut FitCache) -> Option<EndNeed> {
    let basis = ohl_structures_fe::bolt_rules::basis(model, k, e, None, None)?;
    let key = format!("own|{basis:?}");
    if let Some(x) = cache.rules.get(&key) {
        return x.clone();
    }
    let count = basis.bolts.as_ref().map(|b| b.count).or_else(|| {
        let d = &model.members[k].design;
        d.ends
            .get(e)
            .and_then(Option::as_ref)
            .and_then(|x| x.bolts.map(|b| b.count))
            .or(d.bolts.map(|b| b.count))
    });
    let out = crate::member_view::rules_in(model, k, e, None, None).map(|r| EndNeed::of(&r, count));
    cache.rules.insert(key, out.clone());
    out
}

/// The code's needs of member `k`'s end with OpenOHL's default bolt of
/// diameter `d` (the end has no bolts of its own): the plugin's distances
/// for a bolt of that diameter in the member's leg, never below the
/// structure's default minimums (those the layout draws a picked bolt
/// with: the larger, on the safe side); without a plugin, the default
/// minimums.
fn default_need(model: &Model, k: usize, d: f64, cache: &mut FitCache) -> EndNeed {
    let m = &model.members[k];
    let s = &model.sections[m.section];
    let leg3 = s
        .profile
        .as_ref()
        .and_then(|p| ohl_member_check::default_bolts::leg(&p.shape));
    let t = m.design.thickness.or(leg3.map(|l| l.1)).unwrap_or(0.0);
    let sheared = ohl_structures_fe::bolt_rules::sheared_edge(s, None, false);
    let redundant = m.design.class == Some(ohl_structures_fe::design::MemberClass::Redundant);
    let key = format!(
        "default|{d:e}|{t:e}|{:?}|{sheared}|{redundant}|{leg3:?}",
        s.ultimate
    );
    if let Some(Some(x)) = cache.rules.get(&key) {
        return x.clone();
    }
    let floor = &model.bolt_defaults;
    let (mut end, mut spacing, mut edge, mut force) =
        (floor.end * d, floor.spacing * d, floor.edge * d, None);
    if let Some(p) = model.capacity_plugin.as_ref() {
        use ohl_plugins::bolt_distances_full as full;
        let input = full::DistanceInput {
            diameter: d,
            thickness: t,
            ultimate: s.ultimate,
            analysis_force: None,
            bolts: Some(full::Bolts {
                count: 1,
                shear_planes: 1,
                shear_capacity: None,
                shear_capacity_shank: None,
                ultimate: None,
                threads_in_shear: None,
                grade: String::new(),
            }),
            sheared_edge: sheared,
            redundant,
            parameters: p
                .parameters
                .as_ref()
                .map(|x| x.1.clone())
                .unwrap_or_default(),
        };
        let given = match ohl_member_check::joints::minimum_distances_full(p, &input) {
            Ok(Some(x)) => Some((x.end, x.spacing, x.edge, x.bolt_force)),
            Ok(None) | Err(_) => ohl_member_check::joints::minimum_distances(
                p,
                &ohl_plugins::bolt_distances::DistanceInput {
                    diameter: d,
                    thickness: t,
                    ultimate: s.ultimate,
                    bolt_force: None,
                    sheared_edge: sheared,
                    redundant,
                },
            )
            .ok()
            .map(|x| (x.end, x.spacing, x.edge, None)),
        };
        if let Some((e, sp, ed, f)) = given {
            end = end.max(e);
            spacing = spacing.max(sp);
            edge = edge.max(ed);
            force = f.filter(|f| *f > 0.0);
        }
    }
    let out = EndNeed {
        end,
        spacing,
        edge,
        force,
        diameter: d,
        leg: leg3.map(|l| (l.0, l.1)),
        root: leg3.map_or(0.0, |l| l.2),
        count: None,
    };
    cache.rules.insert(key, Some(out.clone()));
    out
}

/// The joint-fit problems of the members `members` of `model` under their
/// largest forces `axial` (N per member, the strength factor taken out)
/// and the practice's `rules` (see the module): the problems, and the
/// member ends whose bolt is not known (no bolts of their own, no default
/// bolt: not checked).
pub fn joint_fit(
    model: &Model,
    members: &[usize],
    axial: &[f64],
    rules: &FitRules,
) -> (Vec<FitProblem>, usize) {
    joint_fit_cached(model, members, axial, rules, &mut FitCache::default())
}

/// [`joint_fit`] with the code's answers kept in `cache` (one plugin call
/// per distinct bolt, part and edge over a whole sizing).
pub fn joint_fit_cached(
    model: &Model,
    members: &[usize],
    axial: &[f64],
    rules: &FitRules,
    cache: &mut FitCache,
) -> (Vec<FitProblem>, usize) {
    let defaults: BTreeMap<usize, ohl_member_check::default_bolts::DefaultBolt> =
        ohl_member_check::default_bolts::default_bolts(model)
            .into_iter()
            .filter(|d| members.contains(&d.member))
            .map(|d| (d.member, d))
            .collect();
    let mut problems = Vec::new();
    let mut unknown = 0;
    for &k in members {
        let Some(m) = model.members.get(k) else {
            continue;
        };
        let class = m.design.class.map(|c| c.as_str());
        let mut needs: Vec<Option<EndNeed>> = Vec::with_capacity(2);
        for e in 0..2 {
            let need = match own_need(model, k, e, cache) {
                Some(n) => Some(n),
                None => match defaults.get(&k) {
                    Some(dflt) => match dflt.diameter {
                        Some(d) => {
                            let mut n = default_need(model, k, d, cache);
                            n.count = Some(rules.picked_count(class));
                            Some(n)
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
            needs.push(need);
        }
        let force = axial.get(k).copied().unwrap_or(0.0);
        let mut length_needed = 0.0;
        let mut length_known = true;
        for (e, need) in needs.iter().enumerate() {
            let Some(n) = need else {
                length_known = false;
                continue;
            };
            let joint = &model.nodes[if e == 0 { m.i } else { m.j }].id;
            if let Some((w, _)) = n.leg {
                let g = w / 2.0;
                let least = n.least_gauge();
                if g < least - 1e-9 {
                    problems.push(FitProblem {
                        member: k,
                        end: Some(e),
                        rule: FitRule::LegTakesBolt,
                        words: format!(
                            "{} at {joint}: the {} mm bolt's hole not clear of the root fillet at \
                             mid-leg ({} mm from the heel, at least {} mm)",
                            m.id,
                            mm(n.diameter),
                            mm(g),
                            mm(least)
                        ),
                    });
                } else if w - g < n.edge - 1e-9 {
                    problems.push(FitProblem {
                        member: k,
                        end: Some(e),
                        rule: FitRule::LegTakesBolt,
                        words: format!(
                            "{} at {joint}: {} mm to the toe at mid-leg, below the code's edge \
                             distance {} mm for the {} mm bolt",
                            m.id,
                            mm(w - g),
                            mm(n.edge),
                            mm(n.diameter)
                        ),
                    });
                }
            }
            let needed = n
                .force
                .map_or(1.0, |p| (force / p).ceil().max(1.0));
            let count = f64::from(n.count.unwrap_or(1).max(1)).max(needed);
            length_needed += 2.0 * n.end + (count - 1.0) * n.spacing;
        }
        let length = model.length(m);
        if length_known && length_needed > length + 1e-9 {
            problems.push(FitProblem {
                member: k,
                end: None,
                rule: FitRule::Length,
                words: format!(
                    "{}: {} mm between its joints, its bolts in one line need {} mm (both ends: \
                     the code's end distances and spacing, the bolts each end has or its force \
                     needs)",
                    m.id,
                    mm(length),
                    mm(length_needed)
                ),
            });
        }
        // The practice's plate thicknesses: a plate the shop makes takes
        // the member's leg.
        let leg_t = needs
            .iter()
            .flatten()
            .filter_map(|n| n.leg.map(|l| l.1))
            .fold(0.0f64, f64::max);
        if leg_t > 0.0
            && let Err(why) = rules.plate(leg_t)
        {
            problems.push(FitProblem {
                member: k,
                end: None,
                rule: FitRule::Practice,
                words: format!("{}: {why} (an end onto a plate)", m.id),
            });
        }
    }
    (problems, unknown)
}
