// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **SIA 263's buckling lengths from what braces a member** (2026-11-19,
//! stage 4.3 of the joint details): the rules this plugin applies to the
//! bracing OpenOHL sends ([`crate::bracing`]). A member giving its own
//! lengths (`lk`, a `k` other than 1, `length_ratios`) keeps them
//! ([`given`]).
//!
//! - **L between the joints held in the direction of buckling** (5.4.1.3:
//!   "la longueur théorique L des barres entre les nœuds tenus dans la
//!   direction du flambage"), about the section's principal axes (4.5.1.2:
//!   an angle's u and v; y and z, and v where smaller, for the others): a
//!   joint holds a direction only where a member holds it along that
//!   direction (within 20°) or it is held every way — read strictly: a
//!   joint held across the direction (a redundant on one face of an angle
//!   leg, at 45° to its v axis's buckling) is not taken as held, the
//!   longer length.
//! - **The reductions of Table 13** (5.4.1.4, 5.4.1.5, 5.5.4.1: when the
//!   joints restrain the members enough, and not for all the members of a
//!   joint buckling together) are not taken: those conditions are not
//!   known; nor a crossing diagonal held out of its plane by the other
//!   (Table 13's 0.5 L "valable seulement dans le cas de barres comprimées
//!   et tendues identiques", under the same conditions): a crossing holds
//!   in its plane only. 5.4.1.6's 0.9 L for an angle bolted by one leg (or
//!   crossed angles) about its minor axis is kept (the check's own).
//! - **A bracing member strong enough** (4.2.3.7: "Chaque élément jouant le
//!   rôle d'appui latéral ponctuel d'un élément ou d'une membrure comprimée
//!   doit être capable de reprendre au moins 1% de l'effort maximal de
//!   compression sollicitant l'élément à stabiliser"): a member holding a
//!   point inside the run counts in a load case only while its resistance
//!   (the lower of its buckling and tension resistances, its lengths with
//!   every bracing point counted — one pass, an assumption) is at least 1 %
//!   of the largest compression along the run in that case.
//! - **A run's ends** at a joint other members frame into are the nodes
//!   the bars' theoretical length runs between (5.4.1.3), held every way
//!   as the member's own ends always were; the strict reading above is for
//!   the joints the member runs on through. A stretch to a free end (a
//!   run end nothing frames into, not supported): twice its length; a run
//!   nothing holds in a direction: twice its length, said not on the safe
//!   side ([`crate::bracing`]).

use std::collections::HashMap;

use crate::bracing::{Axis, Bracing, End, Held, Span, span};
use crate::model::Member;

/// A lateral support takes at least this share of the largest compression
/// of what it holds (SIA 263 4.2.3.7: 1 %).
pub const SUPPORT_SHARE: f64 = 0.01;

/// Whether member `m` gives its own buckling lengths: they win.
pub fn given(m: &Member) -> bool {
    m.lk.is_some() || m.k != 1.0 || m.design.length_ratios.is_some()
}

/// A member's buckling lengths worked out from its bracing, one load case.
#[derive(Debug, Clone, PartialEq)]
pub struct Worked {
    /// About y, z, v and u (the stretches).
    pub y: Span,
    /// See `y`.
    pub z: Span,
    /// See `y`.
    pub v: Span,
    /// See `y`.
    pub u: Span,
    /// What was taken and why, in words.
    pub notes: Vec<String>,
    /// A key of the lengths (the capacities' store).
    pub key: u64,
}

impl Worked {
    /// Every axis over the member's own length between its two joints:
    /// the check is the one before (the lengths the default took), its
    /// words unchanged (what the notes say changes nothing then).
    pub fn own(&self, length: f64) -> bool {
        [&self.y, &self.z, &self.v, &self.u].iter().all(|s| {
            matches!(&s.ends, [End::Own(a), End::Own(b)] if a != b) && !s.free && s.length == length
        })
    }

    /// The lengths in words, the angle's u and v or the y, z and v.
    pub fn words(&self, angle: bool) -> String {
        let list: Vec<(&str, &Span)> = if angle {
            vec![("u", &self.u), ("v", &self.v)]
        } else {
            vec![("y", &self.y), ("z", &self.z), ("v", &self.v)]
        };
        list.iter()
            .map(|(n, s)| format!("{n} {}", s.words()))
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// The stretch about axis `name` (y, z, v, u).
    pub fn of(&self, name: &str) -> &Span {
        match name {
            "y" => &self.y,
            "z" => &self.z,
            "u" => &self.u,
            _ => &self.v,
        }
    }
}

/// What the rules read beyond the member and its bracing.
pub struct Context<'a> {
    /// Each member's resistance as a lateral support, N (none: no load
    /// case — every member counts).
    pub resistance: Option<&'a HashMap<String, f64>>,
    /// Each member's axial force in this load case, N, tension positive.
    pub axial: Option<&'a HashMap<String, f64>>,
    /// The load case's strength factor.
    pub strength: f64,
}

/// Member `m`'s lengths from bracing `b` by the rules above.
pub fn worked(m: &Member, b: &Bracing, cx: &Context) -> Worked {
    let mut notes: Vec<String> = Vec::new();
    // The largest compression along the run in this case.
    let compression = cx.axial.map_or(0.0, |a| {
        b.run
            .iter()
            .filter_map(|p| a.get(&p.member))
            .map(|n| (-n).max(0.0))
            .fold(0.0, f64::max)
    });
    let needed = SUPPORT_SHARE * compression;
    let mut dropped: Vec<String> = Vec::new();
    let held: Vec<Held> = b
        .points
        .iter()
        .map(|p| {
            let inside = !b.is_end(p.at);
            let mut dirs = Vec::new();
            for r in &p.restraints {
                if inside
                    && needed > 0.0
                    && !r.member.is_empty()
                    && let Some(res) = cx.resistance
                    && res.get(&r.member).is_none_or(|x| cx.strength * x < needed)
                {
                    let w = format!("{} at {}", r.member, name(p));
                    if !dropped.contains(&w) {
                        dropped.push(w);
                    }
                    continue;
                }
                dirs.push(r.direction);
            }
            // A run's end at a joint other members frame into: a node
            // of the truss the bar's theoretical length ends at (5.4.1.3:
            // "la longueur théorique L des barres entre les nœuds"), held
            // every way as the member's own ends always were.
            let end_node = !inside && !p.restraints.is_empty();
            Held::new(dirs, p.support || end_node)
        })
        .collect();
    if !dropped.is_empty() {
        notes.push(format!(
            "not counted as lateral supports (less than 1 % of the compression, 4.2.3.7): {}",
            dropped.join(", ")
        ));
    }
    let principal = m.section.properties.as_ref().map(|q| q.principal_angle);
    let axes = [
        Axis {
            name: "y",
            displacement: Some([0.0, 1.0]),
            any: false,
        },
        Axis {
            name: "z",
            displacement: Some([1.0, 0.0]),
            any: false,
        },
        // About v: the displacement along u; about u: along v.
        Axis {
            name: "v",
            displacement: principal.map(|a| [a.cos(), a.sin()]),
            any: false,
        },
        Axis {
            name: "u",
            displacement: principal.map(|a| [-a.sin(), a.cos()]),
            any: false,
        },
    ];
    let fallback = || Span {
        length: m.length,
        ends: [End::Own(0), End::Own(1)],
        between: [m.joints[0].clone(), m.joints[1].clone()],
        points: [None, None],
        free: false,
        unheld: false,
    };
    let spans: Vec<Span> = axes
        .iter()
        .map(|a| {
            span(b, &m.id, &held, a)
                .unwrap_or_else(fallback)
                .snapped(m.length)
        })
        .collect();
    for (a, s) in axes.iter().zip(&spans) {
        if s.unheld {
            notes.push(format!(
                "nothing holds its run about {}: twice the run's length taken — a mechanism \
                 unless the joints' bending holds it, not checked",
                a.name
            ));
        } else if s.free {
            notes.push(format!(
                "about {} a free end ({}–{}): a cantilever, twice its length",
                a.name, s.between[0], s.between[1]
            ));
        }
    }
    let mut key = 0xcbf2_9ce4_8422_2325u64;
    for s in &spans {
        key ^= s.length.to_bits();
        key = key.wrapping_mul(0x0100_0000_01b3);
    }
    let [y, z, v, u]: [Span; 4] = spans.try_into().expect("four axes");
    Worked {
        y,
        z,
        v,
        u,
        notes,
        key,
    }
}

/// A point in words.
fn name(p: &crate::bracing::Point) -> String {
    if p.joint.is_empty() {
        "the crossing".to_owned()
    } else {
        p.joint.clone()
    }
}

/// The lengths of the members of one call: each member's resistance as a
/// lateral support (its lengths with every bracing point counted), then
/// each member's lengths in a load case.
pub struct Lengths<'a> {
    bracing: &'a HashMap<String, Bracing>,
    resistance: HashMap<String, f64>,
}

impl<'a> Lengths<'a> {
    /// For `members` with what braces them (`bracing`, by member id).
    pub fn new(members: &'a [Member], bracing: &'a HashMap<String, Bracing>) -> Self {
        let mut out = Self {
            bracing,
            resistance: HashMap::new(),
        };
        let mut resistance = HashMap::new();
        for m in members {
            let w = out.worked(m, None, 1.0);
            let (c, _) = crate::sia263::capacity_worked(m, None, w.as_ref());
            let r = match (&c.compression, &c.tension) {
                (Some(a), Some(b)) => Some(a.0.min(b.0)),
                (Some(a), None) => Some(a.0),
                (None, Some(b)) => Some(b.0),
                (None, None) => None,
            };
            if let Some(r) = r {
                resistance.insert(m.id.clone(), r);
            }
        }
        out.resistance = resistance;
        out
    }

    /// Member `m`'s lengths from its bracing in a load case whose axial
    /// forces are `axial` and strength factor `strength` (none: no load
    /// case); none: it gives its own ([`given`]), has no bracing, or the
    /// bracing gives its own length about every axis ([`Worked::own`]: the
    /// check as before).
    pub fn worked(
        &self,
        m: &Member,
        axial: Option<&HashMap<String, f64>>,
        strength: f64,
    ) -> Option<Worked> {
        self.all(m, axial, strength).filter(|w| !w.own(m.length))
    }

    /// The same, also where the bracing gives the member's own length
    /// about every axis.
    pub fn all(
        &self,
        m: &Member,
        axial: Option<&HashMap<String, f64>>,
        strength: f64,
    ) -> Option<Worked> {
        if given(m) {
            return None;
        }
        let b = self.bracing.get(&m.id)?;
        Some(worked(
            m,
            b,
            &Context {
                resistance: axial.map(|_| &self.resistance),
                axial,
                strength,
            },
        ))
    }
}

#[cfg(test)]
#[path = "lengths_tests.rs"]
mod tests;
