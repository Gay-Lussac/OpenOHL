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
//! A group whose members' bolts are given at their ends (`[[connection]]`,
//! level 4), differ within the group, or are not given has no bolt
//! proposal (said).

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
}

impl BoltPick {
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

/// A group's bolt proposal, for the window and the command line.
#[derive(Debug, Clone, PartialEq)]
pub struct BoltChange {
    /// Its bolts now, in words.
    pub from: String,
    /// Proposed, in words.
    pub to: String,
    /// The bolts written by Apply.
    pub pick: BoltPick,
}

/// A group's own bolts as they are (the base of its proposals); Err: why
/// it has no bolt proposal.
pub fn base(model: &Model, members: &[usize]) -> Result<BoltPick, String> {
    let mut out: Option<BoltPick> = None;
    for &k in members {
        let m = &model.members[k];
        if m.design.ends.iter().flatten().any(|e| e.bolts.is_some()) {
            return Err(format!(
                "{}'s bolts are given at its ends ([[connection]]): lay out its joints again \
                 with more bolts",
                m.id
            ));
        }
        let Some(b) = m.design.bolts else {
            return Err(format!("{} has no bolts in its design data", m.id));
        };
        match &mut out {
            None => {
                out = Some(BoltPick {
                    count: b.count,
                    diameter: b.diameter,
                    hole: b.hole,
                    shear_capacity: b.shear_capacity,
                    bolt: None,
                })
            }
            Some(o) => {
                if (o.diameter - b.diameter).abs() > 1e-9 {
                    return Err("its members' bolts differ in diameter".into());
                }
                o.count = o.count.max(b.count);
            }
        }
    }
    out.ok_or_else(|| "no member".into())
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
    }
    true
}
