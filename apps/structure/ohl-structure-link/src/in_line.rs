// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Pieces in line proposed "continuous"** (O44 J9; the owner's decision
//! D15 of 2026-11-19): two pieces of one member line — the same section,
//! in line through a joint within the core's angle for one part
//! ([`ohl_structures::connections::CONTINUOUS_ANGLE_DEG`]), the same
//! reference and orientation, their rolls within [`ROLL_TOLERANCE_DEG`] —
//! with nothing given at the joint of how they are joined are *proposed*
//! as one part
//! (`kind = "continuous"`) for the user to confirm, one joint at a time or
//! all at once, in one undo step ([`confirm`]). **Never a silent
//! default**: until confirmed such pieces each end at the joint in the
//! joint checks and in the layout (their steel from their cut; on the safe
//! side).
//!
//! Pieces of a **leg** with bolts (their own or their member's: the
//! import's splice bolts, at both ends of every leg member) are proposed
//! one part too, as **leg splice places** (O44 J7, the owner's rule of
//! 2026-11-19: "a change of section is a splice; the same section is one
//! part unless marked"), flagged [`InLine::splice_bolts`] and listed with
//! the leg splices ([`crate::leg_splices::leg_splices`]), not with these:
//! confirmed, their bolts are not checked at this joint — **not on the
//! safe side** where a splice of one section is really there (to mark by
//! hand, a splice given here).
//!
//! Not proposed, and said why:
//!
//! - a **splice given** at the joint naming either piece, a connection's
//!   `kind` given at either end, either end given by hand to bolt onto
//!   something else, or bolts given at either end: the joint is detailed
//!   (by hand) as something else;
//! - not of one section, other references or orientations, or rolls more
//!   than [`ROLL_TOLERANCE_DEG`] apart: not one part.
//!
//! Rolls apart by less (Orient angles turns each piece to its face on its
//! own): confirming writes both pieces the roll half-way between, so that
//! the core reads them as one part (its rule: the same roll); each piece
//! turns by at most half the tolerance, said in the proposal.
//!
//! The layout's own values at the ends (marked *automatic*) do not count
//! as given: a later run replaces them anyway.

use std::collections::{BTreeMap, BTreeSet};

use ohl_structure_edit::connection_edits;
use ohl_structures::StructureType;
use ohl_structures::connections::{CONTINUOUS_ANGLE_DEG, ConnectionKind};
use toml_edit::DocumentMut;

use crate::layout_run::LayoutRecord;

/// How far apart the rolls of two pieces in line may be and still be
/// proposed one part, degrees: the flatness the joint details allow a leg
/// bolted ([`crate::FLAT_OFF`], 2° — the owner's value of 2026-11-19, not a
/// standard's); confirming writes both the roll half-way between.
pub const ROLL_TOLERANCE_DEG: f64 = crate::joints::FLAT_OFF;

/// Two pieces in line through a joint: proposed one part, or not and why.
#[derive(Debug, Clone, PartialEq)]
pub struct InLine {
    /// The joint.
    pub joint: String,
    /// The piece of the smaller id.
    pub member: String,
    /// The other piece.
    pub onto: String,
    /// None: proposed as one part; else why not.
    pub not: Option<String>,
    /// Their rolls apart (within [`ROLL_TOLERANCE_DEG`]): the one roll
    /// confirming writes to both, radians; none: alike already.
    pub roll: Option<f64>,
    /// Pieces of a leg with bolts (O44 J7): a leg splice place, proposed
    /// one part by the owner's rule (the same section is one part unless
    /// marked) and listed with the leg splices, not with the pieces in
    /// line.
    pub splice_bolts: bool,
}

impl InLine {
    /// Proposed (to confirm).
    pub fn proposed(&self) -> bool {
        self.not.is_none()
    }

    /// In words: "H1 and H2 at M".
    pub fn words(&self) -> String {
        format!("{} and {} at {}", self.member, self.onto, self.joint)
    }

    /// What confirming does beyond `kind = "continuous"`, in words (none:
    /// nothing).
    pub fn also(&self) -> Option<String> {
        self.roll.map(|r| {
            format!(
                "their rolls a little apart: both turned to {:.2}°",
                r.to_degrees()
            )
        })
    }
}

/// The pieces in line through the joints `joints` of `t` (none: every
/// joint) that are not one part yet: those of one section in line within
/// the core's angle, each with whether it is proposed and, if not, why; in
/// the file's order of the joints, then by the pieces' ids.
pub fn in_line_pieces(t: &StructureType, joints: Option<&BTreeSet<String>>) -> Vec<InLine> {
    let record = LayoutRecord::of(t);
    let mut out = Vec::new();
    let point = |id: &str| {
        t.nodes
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.point.map(|v| v.si()))
    };
    for n in &t.nodes {
        if joints.is_some_and(|j| !j.contains(&n.id)) {
            continue;
        }
        let Some(at) = point(&n.id) else { continue };
        // The members ending here, each with its direction away.
        let mut here: Vec<(&ohl_structures::structures::TypeMember, [f64; 3])> = t
            .members
            .iter()
            .filter(|m| m.nodes.contains(&n.id) && m.nodes[0] != m.nodes[1])
            .filter_map(|m| {
                let far = if m.nodes[0] == n.id {
                    &m.nodes[1]
                } else {
                    &m.nodes[0]
                };
                let q = point(far)?;
                let d = [q[0] - at[0], q[1] - at[1], q[2] - at[2]];
                let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                (l > 1e-9).then(|| (m, d.map(|v| v / l)))
            })
            .collect();
        here.sort_by(|a, b| a.0.id.cmp(&b.0.id));
        let mut used: BTreeSet<&str> = BTreeSet::new();
        for x in 0..here.len() {
            for y in x + 1..here.len() {
                let ((a, u), (b, v)) = (here[x], here[y]);
                if used.contains(a.id.as_str()) || used.contains(b.id.as_str()) {
                    continue;
                }
                let cos = -(u[0] * v[0] + u[1] * v[1] + u[2] * v[2]);
                let angle = cos.clamp(-1.0, 1.0).acos().to_degrees();
                if angle > CONTINUOUS_ANGLE_DEG || a.section.is_none() || a.section != b.section {
                    continue;
                }
                used.insert(&a.id);
                used.insert(&b.id);
                // One part already.
                if t.continuous_with(&a.id, &n.id) == Some(b.id.as_str()) {
                    continue;
                }
                let (not, roll) = match turned_alike(a, b) {
                    Err(why) => (Some(why), None),
                    Ok(roll) => (why_not(t, &record, &n.id, &a.id, &b.id), roll),
                };
                out.push(InLine {
                    joint: n.id.clone(),
                    member: a.id.clone(),
                    onto: b.id.clone(),
                    roll: roll.filter(|_| not.is_none()),
                    not,
                    splice_bolts: leg_with_bolts(t, &n.id, &a.id, &b.id),
                });
            }
        }
    }
    out
}

/// A member's roll, radians (none: 0).
fn roll_of(m: &ohl_structures::structures::TypeMember) -> f64 {
    match &m.roll {
        None => 0.0,
        Some(v) => v
            .clone()
            .try_into::<ohl_core::units::Angle>()
            .map_or(0.0, |a| a.radians()),
    }
}

/// Pieces `a` and `b` turned alike enough to be one part: the roll to
/// write to both when theirs differ (half-way between), none when alike;
/// else why not.
fn turned_alike(
    a: &ohl_structures::structures::TypeMember,
    b: &ohl_structures::structures::TypeMember,
) -> Result<Option<f64>, String> {
    if a.reference != b.reference || a.orientation != b.orientation {
        return Err(format!(
            "{} and {} are not turned alike (their references or orientations differ)",
            a.id, b.id
        ));
    }
    if a.roll == b.roll {
        return Ok(None);
    }
    let (ra, rb) = (roll_of(a), roll_of(b));
    let tau = std::f64::consts::TAU;
    let d = (rb - ra + std::f64::consts::PI).rem_euclid(tau) - std::f64::consts::PI;
    if d.abs().to_degrees() > ROLL_TOLERANCE_DEG + 1e-9 {
        return Err(format!(
            "{} and {} are not turned alike (their rolls {:.1}° apart, more than {ROLL_TOLERANCE_DEG}°)",
            a.id,
            b.id,
            d.abs().to_degrees()
        ));
    }
    Ok(Some(ra + d / 2.0))
}

/// Why pieces `a` and `b` in line at joint `j` (turned alike enough) are
/// not proposed one part (none: proposed).
fn why_not(t: &StructureType, record: &LayoutRecord, j: &str, a: &str, b: &str) -> Option<String> {
    if let Some(s) = t
        .splices
        .iter()
        .find(|s| s.joint == j && s.members.iter().any(|m| m == a || m == b))
    {
        return Some(format!(
            "a splice is given here ({}): a joint detailed by hand",
            s.id
        ));
    }
    for (m, other) in [(a, b), (b, a)] {
        let Some(c) = t.connection(m, j) else {
            continue;
        };
        let auto = record
            .ends
            .get(&(m.to_owned(), j.to_owned()))
            .cloned()
            .unwrap_or_default();
        let hand = |f: &str| !auto.contains(f);
        if let Some(k) = c.kind.filter(|_| hand("kind")) {
            return Some(format!(
                "{m}'s end here is given as {} (by hand)",
                k.as_str()
            ));
        }
        if let Some(o) = c.onto.as_deref().filter(|o| *o != other && hand("onto")) {
            return Some(format!("{m} is given to bolt onto {o} here (by hand)"));
        }
        if c.bolts.is_some() && hand("bolts") {
            return Some(format!(
                "bolts are given at {m}'s end here (by hand): a bolted joint"
            ));
        }
    }
    None
}

/// Pieces `a` and `b` at joint `j` are of a leg (either) with bolts (either:
/// their members' design data, or given at the end): a leg splice place
/// (O44 J7).
fn leg_with_bolts(t: &StructureType, j: &str, a: &str, b: &str) -> bool {
    let member = |id: &str| t.members.iter().find(|m| m.id == id);
    let design = |id: &str, key: &str| -> Option<toml::Value> {
        member(id)?.design.as_ref()?.get(key).cloned()
    };
    let leg = |id: &str| {
        design(id, "class").and_then(|v| v.as_str().map(str::to_owned)) == Some("leg".into())
    };
    let bolted = |id: &str| {
        design(id, "bolts")
            .and_then(|b| b.get("count").and_then(toml::Value::as_integer))
            .is_some_and(|n| n > 0)
            || t.connection(id, j)
                .and_then(|c| c.bolts.as_ref())
                .is_some_and(|b| b.count > 0)
    };
    (leg(a) || leg(b)) && (bolted(a) || bolted(b))
}

/// The pieces `list` proposed (those with no reason against) written one
/// part into `doc` (`kind = "continuous"` at both ends, each onto the
/// other; the layout's own values at those ends taken out first, as they
/// were for pieces ending there). Pieces whose rolls are a little apart
/// get one roll — of each chain of pieces confirmed together, half-way
/// between its pieces' rolls — no longer the layout's. The pieces written
/// one part, and those that could not be (in words, with why).
pub fn confirm(
    doc: &mut DocumentMut,
    t: &StructureType,
    list: &[InLine],
) -> Result<(usize, Vec<String>), String> {
    let mut record = LayoutRecord::of(t);
    let mut conns = connection_edits::Connections::of(doc);
    let mut touched = false;
    let yes: Vec<&InLine> = list.iter().filter(|x| x.proposed()).collect();
    // The chains of pieces confirmed together (a member in three pieces:
    // one chain), each one roll where any of its pieces' rolls differ.
    let mut chain: BTreeMap<String, usize> = BTreeMap::new();
    let mut chains: Vec<Vec<String>> = Vec::new();
    for x in &yes {
        match (chain.get(&x.member).copied(), chain.get(&x.onto).copied()) {
            (None, None) => {
                chains.push(vec![x.member.clone(), x.onto.clone()]);
                chain.insert(x.member.clone(), chains.len() - 1);
                chain.insert(x.onto.clone(), chains.len() - 1);
            }
            (Some(k), None) => {
                chains[k].push(x.onto.clone());
                chain.insert(x.onto.clone(), k);
            }
            (None, Some(k)) => {
                chains[k].push(x.member.clone());
                chain.insert(x.member.clone(), k);
            }
            (Some(k), Some(l)) if k != l => {
                let moved = std::mem::take(&mut chains[l]);
                for m in &moved {
                    chain.insert(m.clone(), k);
                }
                chains[k].extend(moved);
            }
            _ => {}
        }
    }
    let member = |id: &str| t.members.iter().find(|m| m.id == id);
    for c in chains.iter().filter(|c| !c.is_empty()) {
        let pieces: Vec<&ohl_structures::structures::TypeMember> =
            c.iter().filter_map(|m| member(m)).collect();
        if pieces.windows(2).all(|w| w[0].roll == w[1].roll) {
            continue;
        }
        // Half-way: the mean of the rolls about the first's.
        let r0 = roll_of(pieces[0]);
        let mean = r0
            + pieces
                .iter()
                .map(|m| {
                    (roll_of(m) - r0 + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)
                        - std::f64::consts::PI
                })
                .sum::<f64>()
                / pieces.len() as f64;
        // O44 L19: the turns from the joints' planes not weighed yet of a
        // chain all listed: the roll before them half-way between theirs too
        // (taking them back leaves the pieces one roll, as confirming them
        // without the turns would); else each moved as the roll with it.
        let befores: Option<Vec<f64>> = c
            .iter()
            .map(|m| {
                record
                    .plane_turns
                    .get(m)
                    .map(|r| crate::weighed_turns::roll_radians(&r.before))
            })
            .collect();
        let mean_before = befores.filter(|b| !b.is_empty()).map(|b| {
            let b0 = b[0];
            b0 + b
                .iter()
                .map(|x| {
                    (x - b0 + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)
                        - std::f64::consts::PI
                })
                .sum::<f64>()
                / b.len() as f64
        });
        for m in c {
            ohl_structure_edit::edits::set_member_roll(doc, m, mean);
            touched |= record.rolled.remove(m).is_some();
            if let (Some(row), Some(x)) = (record.plane_turns.get_mut(m), member(m)) {
                let before = mean_before.unwrap_or_else(|| {
                    crate::weighed_turns::roll_radians(&row.before) + mean - roll_of(x)
                });
                row.before = crate::weighed_turns::roll_written(before);
                row.with = crate::weighed_turns::roll_written(mean);
                touched = true;
            }
        }
    }
    for x in &yes {
        for m in [&x.member, &x.onto] {
            let key = (m.clone(), x.joint.clone());
            if let Some(fields) = record.ends.remove(&key) {
                for f in fields {
                    if f == "gauge" {
                        continue;
                    }
                    if f == "preset" {
                        for k in connection_edits::CARRIED {
                            let _ = conns.set(doc, m, &x.joint, k, None);
                        }
                        continue;
                    }
                    let _ = conns.set(doc, m, &x.joint, &f, None);
                }
                touched = true;
            }
            touched |= record.provisional.remove(&key);
        }
    }
    conns.tidy(doc);
    if touched {
        record.write(doc);
    }
    // (The core's rule read on the rolls as now written.)
    let now;
    let t = if yes.iter().any(|x| x.roll.is_some()) {
        now = <StructureType as ohl_structures::LibraryItem>::parse(
            std::path::Path::new("structure.ohls"),
            &doc.to_string(),
        )
        .map_err(|e| e.message)?;
        &now
    } else {
        t
    };
    let mut n = 0;
    let mut not = Vec::new();
    for x in &yes {
        match connection_edits::set_continuous(doc, t, &x.member, &x.joint, &x.onto, true) {
            Ok(()) => n += 1,
            Err(e) => not.push(format!("{}: {e}", x.words())),
        }
    }
    Ok((n, not))
}

/// [`confirm`] on a structure file's text: the text after.
pub fn confirm_text(text: &str, list: &[InLine]) -> Result<String, String> {
    let t = <StructureType as ohl_structures::LibraryItem>::parse(
        std::path::Path::new("structure.ohls"),
        text,
    )
    .map_err(|e| e.message)?;
    let mut doc: DocumentMut = text.parse().map_err(|e| format!("not TOML: {e}"))?;
    confirm(&mut doc, &t, list)?;
    Ok(doc.to_string())
}

/// Whether `kind` of a connection given by hand is one part through the
/// joint (kept by a run replacing values given by hand: a statement of
/// what the steel is, not a layout value).
pub(crate) fn one_part(kind: Option<ConnectionKind>) -> bool {
    kind == Some(ConnectionKind::Continuous)
}
