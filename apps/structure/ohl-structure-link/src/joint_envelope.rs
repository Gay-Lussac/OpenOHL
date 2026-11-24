// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The end releases and the bolt slip never lower a usage (stage 4.4
//! follow-up of the joint details, 2026-11-24; the owner's question 2 of
//! 2026-11-23, "build it before the slip is used in design"; built, in
//! active development, not approved yet). A run with either option solves
//! every case without it too (for its effect); [`envelope`] keeps, per
//! member and per joint check, the result of the solve where its usage is
//! larger — its forces, usage and what governs, the solve without the
//! options said in its `governs` — as the foot movements
//! ([`ohl_structures_fe::feet::larger_usage`]) and the estimated
//! eccentricities ([`crate::eccentricities::envelope`]) do. The tables,
//! the report and the 3D view's colours read the members and plates, so
//! they follow. The joints' displacements: each joint's larger of the two;
//! the support reactions: component by component the larger in magnitude
//! (an envelope of the two, said); the sums of the loads and reactions:
//! the solve's with the options (one equilibrium).

use ohl_structures_fe::joint_stiffness::governs_without;

use crate::CaseResult;

/// The words of a member's or a joint check's result taken from the solve
/// without the joint options (in its `governs`).
pub const WITHOUT: &str = "without the joint options";

/// What [`envelope`] took from each solve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Governed {
    /// Members whose usage governs with the options, and without them
    /// (members with no usage in either: in neither).
    pub members: (usize, usize),
    /// Joint checks' rows (plates, covers, welds, bolt groups) likewise.
    pub plates: (usize, usize),
    /// Joints whose displacement is larger without the options.
    pub joints_without: usize,
    /// Support reactions with a component larger in magnitude without the
    /// options.
    pub reactions_without: usize,
    /// Support reactions with a component of the opposite sign in the two
    /// solves, both more than 1 % of the largest reaction force (moment:
    /// of the largest reaction moment): the smaller,
    /// opposite one is not shown (not on the safe side, said).
    pub reactions_flipped: usize,
}

impl Governed {
    /// In words, for the case's notes.
    pub fn words(&self) -> String {
        let mut t = format!(
            "joint options: each member's usage the larger of with and without them — {} \
             member(s) governed by the solve with them, {} by the solve without them (said \
             \"{WITHOUT}\" in what governs)",
            self.members.0, self.members.1
        );
        if self.plates != (0, 0) {
            t.push_str(&format!(
                "; the joints' checks likewise ({} row(s) with them, {} without)",
                self.plates.0, self.plates.1
            ));
        }
        t.push_str(&format!(
            "; each joint's displacement the larger of the two ({} joint(s) from the solve \
             without them; the deformed shape drawn mixes the two solves there); the support \
             reactions component by component the larger in magnitude ({} support(s) with a \
             component from the solve without them) — an envelope, not one equilibrium: the sum \
             of the reactions shown is the solve's with them",
            self.joints_without, self.reactions_without
        ));
        t
    }

    /// The reactions whose component changed sign, in words (none: none).
    pub fn flipped_words(&self) -> Option<String> {
        (self.reactions_flipped > 0).then(|| {
            format!(
                "NOT ON THE SAFE SIDE: {} support reaction(s) with a component of opposite sign \
                 with and without the joint options — the larger in magnitude shown, the smaller \
                 of opposite sign not (compare the two runs for the foundations there)",
                self.reactions_flipped
            )
        })
    }
}

/// `what` with the words of the solve without the options added.
fn tagged(what: &str) -> String {
    if what.is_empty() {
        format!("({WITHOUT})")
    } else {
        format!("{what} ({WITHOUT})")
    }
}

fn norm(d: [f64; 3]) -> f64 {
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// One case's results `with` the joint options and `without` them (the
/// same model, the same cases): each member and each joint check's row
/// from the solve where its usage is larger ([`governs_without`]; equal:
/// with them), each joint's displacement the larger of the two, each
/// support's reaction component by component the larger in magnitude; the
/// load and reaction sums, the points and the climbing check with them. The
/// notes: those with the options, what was taken from each solve
/// ([`Governed::words`]) and what the solve without them says is not on
/// the safe side, once.
pub fn envelope(with: CaseResult, without: &CaseResult) -> (CaseResult, Governed) {
    let mut out = with;
    let mut g = Governed::default();
    for (k, m) in out.members.iter_mut().enumerate() {
        let alt = without.members.get(k).filter(|a| a.id == m.id);
        match alt {
            Some(a) if governs_without(m.usage, a.usage) => {
                *m = a.clone();
                m.governs = tagged(&a.governs);
                g.members.1 += 1;
            }
            _ if m.usage.is_some() => g.members.0 += 1,
            _ => {}
        }
    }
    for (k, p) in out.plates.iter_mut().enumerate() {
        let alt = without
            .plates
            .get(k)
            .filter(|a| a.joint == p.joint && a.plate == p.plate && a.part == p.part);
        match alt {
            Some(a) if governs_without(p.usage, a.usage) => {
                *p = a.clone();
                p.governs = tagged(&a.governs);
                g.plates.1 += 1;
            }
            _ if p.usage.is_some() => g.plates.0 += 1,
            _ => {}
        }
    }
    for (k, j) in out.joints.iter_mut().enumerate() {
        if let Some(a) = without.joints.get(k).filter(|a| a.id == j.id)
            && norm(a.displacement) > norm(j.displacement)
        {
            j.displacement = a.displacement;
            g.joints_without += 1;
        }
    }
    let largest = |f: fn(&crate::ReactionResult) -> [f64; 3]| {
        out.reactions
            .iter()
            .chain(&without.reactions)
            .map(|r| norm(f(r)))
            .fold(0.0, f64::max)
    };
    let (force, moment) = (largest(|r| r.force), largest(|r| r.moment));
    for r in &mut out.reactions {
        let Some(a) = without.reactions.iter().find(|a| a.joint == r.joint) else {
            continue;
        };
        let (mut took, mut flipped) = (false, false);
        let pairs = r
            .force
            .iter_mut()
            .zip(a.force)
            .map(|(x, y)| (x, y, 0.01 * force))
            .chain(
                r.moment
                    .iter_mut()
                    .zip(a.moment)
                    .map(|(x, y)| (x, y, 0.01 * moment)),
            );
        for (x, y, small) in pairs {
            if *x * y < 0.0 && x.abs() > small && y.abs() > small {
                flipped = true;
            }
            if y.abs() > x.abs() {
                *x = y;
                took = true;
            }
        }
        g.reactions_without += usize::from(took);
        g.reactions_flipped += usize::from(flipped);
    }
    out.notes.push(g.words());
    if let Some(w) = g.flipped_words() {
        out.notes.push(w);
    }
    for n in &without.notes {
        if n.starts_with("NOT ON THE SAFE SIDE") {
            let n = format!("{n} ({WITHOUT})");
            if !out.notes.contains(&n) {
                out.notes.push(n);
            }
        }
    }
    (out, g)
}
