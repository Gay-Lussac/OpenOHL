// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Why a joint is not laid out whole** (O44 L30, the owner, 2026-11-26:
//! "setup ways to study the latest joint and lay them out. The goal is not
//! to reach 100% but not far off so that a human can relatively quickly
//! check the missing joints"). Built, in active development, not approved
//! yet.
//!
//! The layout says, for every member end it leaves for a hand detail, why
//! (the plan's words, [`crate::JointPlan::hand`], or a clash after
//! writing), and keeps it in the file's layout record (`hand`, one row per
//! joint). This module sorts those words into a few **reason classes**
//! ([`LeftReason`]), each with what a person checking the joint looks at
//! ([`LeftReason::check`]), and lists every joint left with its classes —
//! from a run ([`joints_left_of_run`]) or from a laid-out file's record
//! alone ([`joints_left`]). The one place the classes are defined: the
//! command line's `ohl-structure-layout left`, the run's notes and the
//! review of the joints left use these.
//!
//! The classes are read from the words the layout writes (the words are
//! the layout's own and stay so; each class's words are tested here
//! against the texts the layout builds). A reason no class knows is
//! [`LeftReason::Other`], its words kept.

use std::collections::{BTreeMap, BTreeSet};

use ohl_structures::{LibraryItem, StructureType};

use crate::layout_run::{LayoutRecord, LayoutRun};

/// Why a member end is left for a hand detail, sorted (O44 L30).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LeftReason {
    /// Laid out, it would run into another part of the joint (steel, a
    /// plate, a bolt's head, shank or nut): left as it was.
    Clash,
    /// Laid out, what it bolts onto (left as it is) would then not fit onto
    /// it, or a plate's own bolts would leave its steel or edge distances.
    NotFitAfter,
    /// Its bolted leg lies off the flange it bolts onto and no turn lays it
    /// flat at both its ends; a bent or welded plate does not fit either.
    LegOffFlange,
    /// Its axis runs out of the face of the flange it bolts onto by more
    /// than a bent plate takes.
    AxisOutOfFace,
    /// Its outstanding leg points towards the flange (a plate in its own
    /// plane would stand on the flat plate).
    LegIntoFlange,
    /// On the inner face of the flange it bolts onto it runs towards that
    /// angle's other leg.
    InnerFace,
    /// Its bolts do not all land on the flange it bolts onto (a built-up
    /// member, a splice's cover).
    BoltsOffFlange,
    /// On a plate, the plate would run into another part.
    PlateRunsInto,
    /// It would need pulling back more than the longest pull-back (or no
    /// room for two ends on one plate).
    PullBack,
    /// Too short for its bolts.
    TooShort,
    /// A member left as it is that bolts onto it would lose a bolt off it.
    PartnerLeft,
    /// Its member's design data draws bolts at the joint, but it bolts onto
    /// nothing there.
    BoltsJoinNothing,
    /// A value given by hand (its cut, its face) keeps it from being laid
    /// out.
    GivenByHand,
    /// A form the layout does not make here: a plate between a built-up
    /// member's angles too thin, a level plate not held, a splice's place
    /// unknown, a welded plate the detailing practice does not make.
    Form,
    /// None of the above (the words kept).
    Other,
}

impl LeftReason {
    /// Every class, in order.
    pub const ALL: [LeftReason; 15] = [
        LeftReason::Clash,
        LeftReason::NotFitAfter,
        LeftReason::LegOffFlange,
        LeftReason::AxisOutOfFace,
        LeftReason::LegIntoFlange,
        LeftReason::InnerFace,
        LeftReason::BoltsOffFlange,
        LeftReason::PlateRunsInto,
        LeftReason::PullBack,
        LeftReason::TooShort,
        LeftReason::PartnerLeft,
        LeftReason::BoltsJoinNothing,
        LeftReason::GivenByHand,
        LeftReason::Form,
        LeftReason::Other,
    ];

    /// A short key (for tables and the command line).
    pub fn key(self) -> &'static str {
        match self {
            LeftReason::Clash => "clash",
            LeftReason::NotFitAfter => "not-fit-after",
            LeftReason::LegOffFlange => "leg-off-flange",
            LeftReason::AxisOutOfFace => "axis-out-of-face",
            LeftReason::LegIntoFlange => "leg-into-flange",
            LeftReason::InnerFace => "inner-face",
            LeftReason::BoltsOffFlange => "bolts-off-flange",
            LeftReason::PlateRunsInto => "plate-runs-into",
            LeftReason::PullBack => "pull-back",
            LeftReason::TooShort => "too-short",
            LeftReason::PartnerLeft => "partner-left",
            LeftReason::BoltsJoinNothing => "bolts-join-nothing",
            LeftReason::GivenByHand => "given-by-hand",
            LeftReason::Form => "form",
            LeftReason::Other => "other",
        }
    }

    /// The class of a [`LeftReason::key`].
    pub fn of_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.key() == key)
    }

    /// In words, short.
    pub fn as_str(self) -> &'static str {
        match self {
            LeftReason::Clash => "a clash after laying out",
            LeftReason::NotFitAfter => "what it bolts onto would not fit after",
            LeftReason::LegOffFlange => "its leg off the flange, no turn lays it flat",
            LeftReason::AxisOutOfFace => "its axis out of the flange's face",
            LeftReason::LegIntoFlange => "its outstanding leg into the flange",
            LeftReason::InnerFace => "on the flange's inner face",
            LeftReason::BoltsOffFlange => "its bolts off the flange (built up, splice cover)",
            LeftReason::PlateRunsInto => "its plate would run into another part",
            LeftReason::PullBack => "more pull-back than the longest",
            LeftReason::TooShort => "too short for its bolts",
            LeftReason::PartnerLeft => "a member left would lose a bolt",
            LeftReason::BoltsJoinNothing => "its bolts join nothing",
            LeftReason::GivenByHand => "a value given by hand",
            LeftReason::Form => "a form the layout does not make here",
            LeftReason::Other => "other",
        }
    }

    /// What a person checking such a joint looks at.
    pub fn check(self) -> &'static str {
        match self {
            LeftReason::Clash => {
                "the two parts named: pull the end back, put it on the other face or on a plate, \
                 or move the bolts (the close-up shows where they meet)"
            }
            LeftReason::NotFitAfter => {
                "the member it bolts onto: its cut or bolts as written no longer fit with the end \
                 laid out; detail both together"
            }
            LeftReason::LegOffFlange => {
                "the member's roll: a turn it can take at both its ends, a bent plate, or a \
                 detail by hand (a gusset, a filler)"
            }
            LeftReason::AxisOutOfFace => {
                "the plane the end lies in: a plate in its own plane, a bent plate past 20°, or \
                 another member to bolt onto"
            }
            LeftReason::LegIntoFlange => {
                "the member's roll (a half or a quarter turn was weighed and did not help): a \
                 stand-off or a detail by hand"
            }
            LeftReason::InnerFace => {
                "which face: the outer face, the member turned, or a plate past the heel"
            }
            LeftReason::BoltsOffFlange => {
                "the built-up member or splice cover: a plate across it, or the bolts placed by \
                 hand"
            }
            LeftReason::PlateRunsInto => {
                "the plate's outline and what it meets: a smaller plate, another face, or the \
                 other part moved"
            }
            LeftReason::PullBack => {
                "how far the end must be pulled back: a longer pull-back accepted, a shift, or a \
                 plate"
            }
            LeftReason::TooShort => "the member's length and bolts: fewer or smaller bolts",
            LeftReason::PartnerLeft => {
                "the member left as it is beside it: detail the two together (its own reason \
                 first)"
            }
            LeftReason::BoltsJoinNothing => {
                "the member's design data (bolts drawn at a joint where it bolts onto nothing): \
                 what it should bolt onto there, or no bolts"
            }
            LeftReason::GivenByHand => {
                "the value given by hand: keep it, or let the layout replace it"
            }
            LeftReason::Form => "the form named: detail it by hand",
            LeftReason::Other => "the words given",
        }
    }

    /// The class of the words the layout gives for an end left for a hand
    /// detail (one member's: "M: …", or the words alone).
    pub fn of(words: &str) -> Self {
        let w = words;
        let has = |p: &str| w.contains(p);
        // (After writing: "laid out, …" — the words of a clash may also
        // follow another end's.)
        if has("would then not fit onto it")
            || has("beyond the end of its steel")
            || has("nearer the plate's edge")
        {
            return LeftReason::NotFitAfter;
        }
        if has("laid out, ") || (has("would run") && has("into each other")) {
            return LeftReason::Clash;
        }
        if has("its bolted leg lies") && has("off the flange of") {
            return LeftReason::LegOffFlange;
        }
        if has(crate::flange_turns::INTO_FLANGE) {
            return LeftReason::LegIntoFlange;
        }
        if has("its axis runs") && has("out of the face of") {
            return LeftReason::AxisOutOfFace;
        }
        if has("on the inner face of") && has("runs towards the angle's other leg") {
            return LeftReason::InnerFace;
        }
        if has(crate::joint_layout::BOLTS_JOIN_NOTHING) {
            return LeftReason::BoltsJoinNothing;
        }
        if has(crate::joint_layout::PARTNER_LEFT) {
            return LeftReason::PartnerLeft;
        }
        if has("given by hand") {
            return LeftReason::GivenByHand;
        }
        if has("too short for its bolts") {
            return LeftReason::TooShort;
        }
        if has("do not all land on") {
            return LeftReason::BoltsOffFlange;
        }
        if has("pulled back") || has("no room on one plate") {
            return LeftReason::PullBack;
        }
        if has("would run into") {
            return LeftReason::PlateRunsInto;
        }
        if has("across the gap between")
            || has(crate::joint_layout::LEVEL_NOT_HELD)
            || has(crate::joint_layout::SPLICE_UNKNOWN)
            || has("no plate welded in its own plane")
        {
            return LeftReason::Form;
        }
        LeftReason::Other
    }
}

/// One member end left at a joint, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeftEnd {
    /// The member (empty where the words name no one member).
    pub member: String,
    /// Its class.
    pub reason: LeftReason,
    /// The layout's words.
    pub words: String,
}

/// A joint not laid out whole, with why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeftJoint {
    /// The joint.
    pub joint: String,
    /// Laid out (written) with some ends left, or not laid out at all.
    pub written: bool,
    /// The ends left, each with its class.
    pub ends: Vec<LeftEnd>,
}

impl LeftJoint {
    /// The joint's main class: the one most of its ends have; of as many,
    /// the first in [`LeftReason::ALL`]'s order.
    pub fn main(&self) -> LeftReason {
        let mut n: BTreeMap<LeftReason, usize> = BTreeMap::new();
        for e in &self.ends {
            *n.entry(e.reason).or_default() += 1;
        }
        n.iter()
            .max_by_key(|(r, k)| (**k, std::cmp::Reverse(**r)))
            .map_or(LeftReason::Other, |(r, _)| *r)
    }

    /// Its classes (each once, in order).
    pub fn classes(&self) -> BTreeSet<LeftReason> {
        self.ends.iter().map(|e| e.reason).collect()
    }
}

/// The joints a run left not whole (written with ends left for a hand
/// detail, or not written though the layout had ends there), each with
/// its ends' classes; in the run's order. A joint with nothing for the
/// layout (every member running on, or none bolting there) is not listed.
pub fn joints_left_of_run(run: &LayoutRun) -> Vec<LeftJoint> {
    run.joints
        .iter()
        .filter(|o| !(o.written && o.hand.is_empty()))
        .filter(|o| !(o.plan.ends.is_empty() && o.plan.hand_members.is_empty()))
        .map(|o| LeftJoint {
            joint: o.joint.clone(),
            written: o.written,
            ends: o.hand.iter().map(|h| end_of(h)).collect(),
        })
        .collect()
}

/// The joints the layout record of the laid-out file `text` keeps as left
/// for a hand detail (its `hand` rows), each with its ends' classes; in
/// the record's order. Read from the file alone (no layout run).
pub fn joints_left(text: &str) -> Result<Vec<LeftJoint>, String> {
    let t = StructureType::parse(std::path::Path::new("structure.ohls"), text)
        .map_err(|e| e.message)?;
    let record = LayoutRecord::of(&t);
    Ok(record
        .hand
        .iter()
        .map(|(j, row)| {
            let members: BTreeSet<&str> = t
                .members
                .iter()
                .filter(|m| m.nodes.iter().any(|n| n == j))
                .map(|m| m.id.as_str())
                .collect();
            LeftJoint {
                joint: j.clone(),
                written: record.joints.contains(j),
                ends: split_row(row, &members).iter().map(|h| end_of(h)).collect(),
            }
        })
        .collect())
}

/// The joints of `text` whole by its layout record (laid out, none left).
pub fn joints_whole(text: &str) -> Result<BTreeSet<String>, String> {
    let t = StructureType::parse(std::path::Path::new("structure.ohls"), text)
        .map_err(|e| e.message)?;
    let record = LayoutRecord::of(&t);
    Ok(record
        .joints
        .iter()
        .filter(|j| !record.hand.contains_key(*j))
        .cloned()
        .collect())
}

/// The ends' words of a record's `hand` row (joined by "; ", a member's
/// own words may hold "; " too): split where the next words start with a
/// member of the joint (`members`) and ": ".
pub fn split_row(row: &str, members: &BTreeSet<&str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in row.split("; ") {
        let starts = part
            .split_once(": ")
            .is_some_and(|(m, _)| members.contains(m));
        match out.last_mut() {
            Some(last) if !starts => {
                last.push_str("; ");
                last.push_str(part);
            }
            _ => out.push(part.to_owned()),
        }
    }
    out
}

/// One end's words read: its member (before ": ", where that is one word)
/// and class.
fn end_of(words: &str) -> LeftEnd {
    let member = words
        .split_once(": ")
        .filter(|(m, _)| !m.contains(' '))
        .map_or_else(String::new, |(m, _)| m.to_owned());
    LeftEnd {
        member,
        reason: LeftReason::of(words),
        words: words.to_owned(),
    }
}

/// The joints left counted by class: per class, the joints having it as
/// their main class, the joints having it at all, and the ends.
pub fn counts(left: &[LeftJoint]) -> BTreeMap<LeftReason, (usize, usize, usize)> {
    let mut out: BTreeMap<LeftReason, (usize, usize, usize)> = BTreeMap::new();
    for j in left {
        out.entry(j.main()).or_default().0 += 1;
        for c in j.classes() {
            out.entry(c).or_default().1 += 1;
        }
        for e in &j.ends {
            out.entry(e.reason).or_default().2 += 1;
        }
    }
    out
}
