// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The **default bolt** of a member whose data gives no bolts (2026-11-26,
//! roadmap O19 stage 5, the owner's answer of 2026-11-25: "Ok for plugin
//! fixes"; built, in active development, not approved yet). A code plugin
//! takes a hole in each leg of such an angle in the net section until the
//! bolts are known (2026-11-24); before, of its own 20 mm bolt — harsh on
//! small angles (an L40 leg loses about half its area with two holes).
//! Now OpenOHL sends each such member a default bolt, with why
//! (`member-check-default-bolts`), in this order:
//!
//! 1. **The largest bolt of the structure's bolt library its leg's usual
//!    gauge takes** ([`Source::Leg`]): one gauge line at mid-leg (the leg
//!    divided evenly, as OpenOHL draws a leg without gauges), the bolt
//!    there taken when its hole is clear of the root fillet (the gauge at
//!    least t + r + d/2 from the heel: geometry, the bolt tools' least
//!    gauge) and the distance on to the toe at least **the code's own edge
//!    distance** for that bolt in that leg, from the plugin's
//!    `bolt-distances` as the bolt tools take it without an analysis (the
//!    toe rolled unless a connection of the member says `rolled_edge =
//!    false`). The leg: an angle's longer leg (a built-up member's angles'),
//!    the larger bolt — on the safe side for the holes in each leg.
//! 2. **Else the structure's most used bolt** ([`Source::Structure`]): the
//!    bolt of the most bolts at member ends in the structure's data — only
//!    where step 1 cannot be worked out (no bolt library, the plugin gives
//!    no edge distance).
//! 3. **None** ([`Source::None`]) where the leg's usual gauge takes no bolt
//!    of the library, or nothing gives one: the plugin keeps its own
//!    default (today's 20 mm), said — never a smaller hole than today's
//!    without a bolt that fits.
//!
//! The member's own bolts at its other end still come first in the plugin
//! (they are data). The default is an assumption, not on the safe side
//! where the real bolts are larger: the plugin says the connection is not
//! checked and that larger or more holes lower the net section.

use ohl_plugins::bolt_distances as dist;
use ohl_plugins::bolt_distances_full as dist_full;
use ohl_plugins::member_check_default_bolts as wit;
use ohl_sections::Shape;
use ohl_structures::bolts::BoltType;
use ohl_structures_fe::Model;
use ohl_structures_fe::design::MemberClass;
use ohl_structures_fe::model::PluginMethod;

/// Where a default bolt comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The largest bolt of the library the leg's usual gauge takes.
    Leg,
    /// The structure's most used bolt.
    Structure,
    /// None: the plugin keeps its own default.
    None,
}

impl Source {
    /// Its word in the interface.
    pub fn word(self) -> &'static str {
        match self {
            Source::Leg => "leg",
            Source::Structure => "structure",
            Source::None => "none",
        }
    }
}

/// The default bolt of one member.
#[derive(Debug, Clone, PartialEq)]
pub struct DefaultBolt {
    /// The member (its index in the model).
    pub member: usize,
    /// The library bolt's id (none: not a library bolt).
    pub bolt: Option<String>,
    /// Its diameter, m (none: no default, [`Source::None`]).
    pub diameter: Option<f64>,
    /// Its hole, m (none: the code's standard hole).
    pub hole: Option<f64>,
    /// Where it comes from.
    pub source: Source,
    /// Why, in words.
    pub why: String,
}

/// The leg a default bolt is fitted to: its width (heel to toe),
/// thickness and root radius, m — an angle's longer leg, a built-up
/// member's angles' (none: not an angle).
pub fn leg(shape: &Shape) -> Option<(f64, f64, f64)> {
    match *shape {
        Shape::EqualAngle { h, t, r } | Shape::CrossAngle { h, t, r, .. } => Some((h, t, r)),
        Shape::UnequalAngle { h, b, t, r }
        | Shape::DoubleAngle { h, b, t, r, .. }
        | Shape::StarAngle { h, b, t, r, .. } => Some((h.max(b), t, r)),
        _ => None,
    }
}

/// Millimetres for the words.
fn mm(x: f64) -> String {
    let v = x * 1e3;
    if (v - v.round()).abs() < 0.05 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

/// A library bolt in words: its id, and its diameter where the id does not
/// say it.
fn named(b: &BoltType) -> String {
    format!("{} ({} mm)", b.id, mm(b.diameter.si()))
}

/// The code's edge distance (to the toe) of library bolt `b` in a leg of
/// thickness `t` and ultimate strength `fu`, m, with its clause: the
/// plugin's `bolt-distances-full` without an analysis (its own force per
/// bolt), else its `bolt-distances` with the force terms left out. Err:
/// why there is none.
fn edge_minimum(
    p: &PluginMethod,
    b: &BoltType,
    t: f64,
    fu: Option<f64>,
    sheared: bool,
    redundant: bool,
) -> Result<(f64, String), String> {
    let d = b.diameter.si();
    let input = dist_full::DistanceInput {
        diameter: d,
        thickness: t,
        ultimate: fu,
        analysis_force: None,
        bolts: Some(dist_full::Bolts {
            count: 1,
            shear_planes: 1,
            shear_capacity: b.shear_capacity.map(|f| f.si()),
            shear_capacity_shank: b.shear_capacity_shank.map(|f| f.si()),
            ultimate: b.ultimate.map(|s| s.si()),
            threads_in_shear: None,
            grade: b.grade.clone().unwrap_or_default(),
        }),
        sheared_edge: sheared,
        redundant,
        parameters: p
            .parameters
            .as_ref()
            .map(|x| x.1.clone())
            .unwrap_or_default(),
    };
    let clause = |c: &[String]| c.get(2).cloned().unwrap_or_default();
    match crate::joints::minimum_distances_full(p, &input)? {
        Some(x) => Ok((x.edge, clause(&x.clauses))),
        None => {
            let x = crate::joints::minimum_distances(
                p,
                &dist::DistanceInput {
                    diameter: d,
                    thickness: t,
                    ultimate: fu,
                    bolt_force: None,
                    sheared_edge: sheared,
                    redundant,
                },
            )?;
            Ok((x.edge, clause(&x.clauses)))
        }
    }
}

/// How a library bolt fits a leg's usual gauge.
enum Fit {
    /// It fits: the code's edge distance and its clause, m.
    Takes(f64, String),
    /// It does not: why.
    Not(String),
}

/// Whether the usual gauge (mid-leg) of a leg `w` wide, `t` thick with root
/// radius `r` takes library bolt `b` (`edge`: the code's edge distance of
/// the bolt, [`edge_minimum`]). Err: the code's edge distance is not known.
fn fits(
    b: &BoltType,
    (w, t, r): (f64, f64, f64),
    edge: impl Fn(&BoltType) -> Result<(f64, String), String>,
) -> Result<Fit, String> {
    let d = b.diameter.si();
    let g = w / 2.0;
    let least = t + r + d / 2.0;
    if g < least - 1e-9 {
        return Ok(Fit::Not(format!(
            "its hole is not clear of the root fillet at mid-leg ({} mm from the heel, at least \
             {} mm)",
            mm(g),
            mm(least)
        )));
    }
    let (e, clause) = edge(b)?;
    if w - g < e - 1e-9 {
        return Ok(Fit::Not(format!(
            "{} mm to the toe at mid-leg, below the code's edge distance {} mm ({clause})",
            mm(w - g),
            mm(e)
        )));
    }
    Ok(Fit::Takes(e, clause))
}

/// The structure's most used bolt: (the library bolt's id if the ends
/// name one, diameter, hole, how many bolts), the most bolts at member
/// ends (an end giving holes only counts once), on a tie the larger.
fn most_used(model: &Model) -> Option<(Option<String>, f64, Option<f64>, u32)> {
    let mut seen: Vec<(Option<String>, f64, Option<f64>, u32)> = Vec::new();
    for m in &model.members {
        for end in 0..2 {
            let e = m.design.ends.get(end).and_then(Option::as_ref);
            let Some((b, _, _)) =
                ohl_structures_fe::member_ends::end_bolts(&m.design, e, &mut Vec::new())
            else {
                continue;
            };
            let id = e.and_then(|e| e.bolt.clone());
            let n = b.count.max(1);
            let same = |x: &&mut (Option<String>, f64, Option<f64>, u32)| {
                x.0 == id
                    && (x.1 - b.diameter).abs() < 1e-9
                    && x.2.map(f64::to_bits) == b.hole.map(f64::to_bits)
            };
            match seen.iter_mut().find(same) {
                Some(x) => x.3 += n,
                None => seen.push((id, b.diameter, b.hole, n)),
            }
        }
    }
    seen.into_iter().max_by(|a, b| {
        a.3.cmp(&b.3)
            .then(a.1.total_cmp(&b.1))
            .then(a.2.unwrap_or(0.0).total_cmp(&b.2.unwrap_or(0.0)))
    })
}

/// The members of `model` that need a default bolt: checked, an angle
/// profile, and no bolts at either end in their data (an end of the
/// member giving bolts is the plugin's typical bolt).
fn needing(model: &Model) -> Vec<usize> {
    (0..model.members.len())
        .filter(|&k| {
            let m = &model.members[k];
            let s = &model.sections[m.section];
            m.check
                && s.profile.as_ref().and_then(|p| leg(&p.shape)).is_some()
                && (0..2).all(|end| {
                    let e = m.design.ends.get(end).and_then(Option::as_ref);
                    ohl_structures_fe::member_ends::end_bolts(&m.design, e, &mut Vec::new())
                        .is_none()
                })
        })
        .collect()
}

/// The default bolt of each member of `model` whose data gives no bolts
/// (the module's order). Empty: the model names no plugin, or no member
/// needs one.
pub fn default_bolts(model: &Model) -> Vec<DefaultBolt> {
    let Some(p) = model.capacity_plugin.as_ref() else {
        return Vec::new();
    };
    let members = needing(model);
    if members.is_empty() {
        return Vec::new();
    }
    // The library's bolts, the largest first.
    let mut library: Vec<&BoltType> = model.bolt_library.bolts.iter().collect();
    library.sort_by(|a, b| {
        b.diameter.si().total_cmp(&a.diameter.si()).then(
            b.hole
                .map_or(0.0, |h| h.si())
                .total_cmp(&a.hole.map_or(0.0, |h| h.si())),
        )
    });
    let used = most_used(model);
    members
        .into_iter()
        .map(|k| one(model, p, k, &library, used.as_ref()))
        .collect()
}

/// The default bolt of member `k` (one of [`needing`]).
fn one(
    model: &Model,
    p: &PluginMethod,
    k: usize,
    library: &[&BoltType],
    used: Option<&(Option<String>, f64, Option<f64>, u32)>,
) -> DefaultBolt {
    let m = &model.members[k];
    let s = &model.sections[m.section];
    let (w, t0, r) = s
        .profile
        .as_ref()
        .and_then(|p| leg(&p.shape))
        .expect("needing: an angle");
    let t = m.design.thickness.unwrap_or(t0);
    // The toe rolled unless a connection of the member says otherwise.
    let sheared = m
        .design
        .ends
        .iter()
        .flatten()
        .any(|e| e.rolled_edge == Some(false));
    let toe = if sheared {
        "sheared (rolled_edge = false)"
    } else {
        "rolled"
    };
    let redundant = m.design.class == Some(MemberClass::Redundant);
    let edge = |b: &BoltType| edge_minimum(p, b, t, s.ultimate, sheared, redundant);
    // 1. The largest bolt of the library the leg's usual gauge takes.
    let not_worked = if library.is_empty() {
        "the structure has no bolt library".to_owned()
    } else {
        let mut refused: Option<(&BoltType, String)> = None;
        // The bolts the code gives no edge distance for, with why (the
        // first's): skipped, said.
        let mut unknown: Vec<&BoltType> = Vec::new();
        let mut unknown_why = String::new();
        let mut found = None;
        for b in library {
            match fits(b, (w, t, r), edge) {
                Ok(Fit::Takes(e, clause)) => {
                    found = Some((*b, e, clause));
                    break;
                }
                // The last refused: the next larger than the one found, or
                // the smallest of all.
                Ok(Fit::Not(why)) => refused = Some((*b, why)),
                Err(e) => {
                    if unknown.is_empty() {
                        unknown_why = e;
                    }
                    unknown.push(b);
                }
            }
        }
        let skipped = |below: f64| {
            let ids: Vec<&str> = unknown
                .iter()
                .filter(|b| b.diameter.si() > below + 1e-9)
                .map(|b| b.id.as_str())
                .collect();
            if ids.is_empty() {
                String::new()
            } else {
                format!(
                    "; not taken, the code's edge distance not known: {} ({unknown_why})",
                    ids.join(", ")
                )
            }
        };
        match (found, refused) {
            (Some((b, e, clause)), refused) => {
                let d = b.diameter.si();
                let next = refused
                    .filter(|(x, _)| x.diameter.si() > d + 1e-9)
                    .map(|(x, why)| format!("; the next larger, {}, does not fit: {why}", x.id))
                    .unwrap_or_default();
                return DefaultBolt {
                    member: k,
                    bolt: Some(b.id.clone()),
                    diameter: Some(d),
                    hole: b.hole.map(|h| h.si()),
                    source: Source::Leg,
                    why: format!(
                        "the largest bolt of the structure's library its {} mm leg's usual gauge \
                         takes, {}: at mid-leg {} mm from the heel, its hole clear of the root \
                         fillet (at least {} mm) and {} mm to the {toe} toe, at least the code's \
                         edge distance {} mm ({clause}){next}{}",
                        mm(w),
                        named(b),
                        mm(w / 2.0),
                        mm(t + r + d / 2.0),
                        mm(w / 2.0),
                        mm(e),
                        skipped(d)
                    ),
                };
            }
            // None fits (the code's edge distance known for one at least,
            // or the root fillet refusing): the plugin's own default.
            (None, Some((b, why))) => {
                return DefaultBolt {
                    member: k,
                    bolt: None,
                    diameter: None,
                    hole: None,
                    source: Source::None,
                    why: format!(
                        "its {} mm leg's usual gauge takes no bolt of the structure's library \
                         (the smallest, {}: {why}{})",
                        mm(w),
                        named(b),
                        skipped(0.0)
                    ),
                };
            }
            (None, None) => format!("the code's edge distance is not known ({unknown_why})"),
        }
    };
    // 2. The structure's most used bolt.
    match used {
        Some((id, d, hole, n)) => DefaultBolt {
            member: k,
            bolt: id.clone(),
            diameter: Some(*d),
            hole: *hole,
            source: Source::Structure,
            why: format!(
                "the structure's most used bolt, {}{} mm ({n} bolts at member ends); the largest \
                 its leg's usual gauge takes not worked out: {not_worked}",
                id.as_ref().map(|i| format!("{i}, ")).unwrap_or_default(),
                mm(*d)
            ),
        },
        None => DefaultBolt {
            member: k,
            bolt: None,
            diameter: None,
            hole: None,
            source: Source::None,
            why: format!("{not_worked}, and the structure's members give no bolts"),
        },
    }
}

/// The interface's records of `defaults` (of `model`'s members).
pub(crate) fn records(model: &Model, defaults: &[DefaultBolt]) -> Vec<wit::DefaultBolt> {
    defaults
        .iter()
        .map(|d| wit::DefaultBolt {
            member: model.members[d.member].id.clone(),
            bolt: d.bolt.clone().unwrap_or_default(),
            diameter: d.diameter,
            hole: d.hole,
            source: d.source.word().to_owned(),
            why: d.why.clone(),
        })
        .collect()
}
