// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A reference state for the structures (plan of 2026-11-15, phase 5b;
//! owner 2026-11-16: "define a weather case at which the structure is
//! drawn … solve at level 4 and then keep it for later … level 2 solves
//! … to make the model match the lidar scans"; in active development, at
//! calculation level 3 until level 4 is built).
//!
//! - **The setting** ([`Line::reference`], [`StructureReference`]): a
//!   weather case, by name — typically the survey day's temperature and
//!   wind. None: as before, bit for bit.
//! - **The state** ([`ReferenceState`]): that case solved once with the
//!   structures flexible (level 3: [`crate::solve::flexible`], done by
//!   `ohl-loads`, which reads the structures' models), kept with the
//!   line: per attachment point of each flexible structure its
//!   displacement from the structure as strung and the wires' force on it
//!   (line axes), the structures taken rigid with why, and the
//!   [`ReferenceFingerprint`] of everything it was solved from.
//! - **Used** only while it is known current ([`ReferenceStatus::Current`],
//!   set by the program that compared the fingerprint with the inputs it
//!   has now — `ohl_loads::check_reference`; never stored, so a line read
//!   or edited is [`ReferenceStatus::Unchecked`] until checked again):
//!   every finite-element section solve then places each point of a
//!   structure that holds it at its drawn position plus the stored
//!   displacement ([`Line::reference_move`]; the stringing stays on the
//!   structures as drawn). That is the level-2 solve "with the structures
//!   as deflected in the reference case": as fast as before. A stale or
//!   unchecked state is never used: the solve takes the structures as
//!   drawn, and the results say so ([`Line::reference_note`]).
//! - **Not used** by the coupled solves (levels 3 and 4: each case finds
//!   its own deflection from the structures as strung,
//!   [`crate::solve::Coupling::strung`]), nor where a check sets it aside
//!   ([`Line::as_drawn`]: the structure loads keep the structures as
//!   drawn, rigid — on the safe side; internal clearances, whose
//!   structure body is not deflected).

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::{Line, Point};

/// The line's reference case and the state solved in it
/// ([`Line::reference`]; line file version 22, written only when set).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructureReference {
    /// The weather case (its name).
    pub case: String,
    /// The state solved in it (none: not solved yet — the structures as
    /// drawn, said).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<ReferenceState>,
    /// Whether the state is current against the inputs now: set by the
    /// program that checked it, never stored ([`ReferenceStatus`]).
    #[serde(skip)]
    pub status: ReferenceStatus,
}

impl PartialEq for StructureReference {
    /// The setting and the state (the status is the program's knowledge,
    /// not the line's content).
    fn eq(&self, other: &Self) -> bool {
        self.case == other.case && self.state == other.state
    }
}

impl StructureReference {
    /// The setting of case `case`, not solved yet.
    pub fn new(case: &str) -> Self {
        Self {
            case: case.to_owned(),
            state: None,
            status: ReferenceStatus::Unchecked,
        }
    }
}

/// Whether a reference state may be used ([`StructureReference::status`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub enum ReferenceStatus {
    /// Not compared with the inputs here (a line just read or edited):
    /// not used.
    #[default]
    Unchecked,
    /// Its fingerprint is that of the inputs now: used by the level-2
    /// solves.
    Current,
    /// Out of date, with what changed ("the structures changed since it
    /// was solved"): not used.
    Stale(String),
    /// Set aside on purpose, with why ("ignored by --structure-reference
    /// ignore", "the structure loads keep the structures as drawn"): not
    /// used.
    SetAside(String),
}

/// The state of the structures in the reference case
/// ([`StructureReference::state`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceState {
    /// The case it was solved in (another case set since: out of date).
    pub case: String,
    /// The calculation level it was solved at (3; 4 once built).
    pub level: u8,
    /// What it was solved from.
    pub fingerprint: ReferenceFingerprint,
    /// Each attachment point of each flexible structure.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<ReferencePoint>,
    /// The structures taken rigid (as drawn), with why.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rigid: Vec<ReferenceRigid>,
    /// What the solve assumed and left out, in words.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// The coupled solve's passes.
    #[serde(default)]
    pub passes: usize,
    /// Its time (the structures' setup included), s, on the computer
    /// that solved it.
    #[serde(default)]
    pub seconds: f64,
}

/// An attachment point of a flexible structure in the reference state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferencePoint {
    /// The placed structure's id.
    pub structure: String,
    /// The point's label.
    pub label: String,
    /// Its displacement from the structure as strung (drawn), line axes,
    /// m.
    pub displacement: Point,
    /// The wires' force on it in the reference case (the whole bundles',
    /// unfactored), line axes, N; zero where no wire solved by finite
    /// elements holds it.
    pub force: Point,
    /// The same at the stringing state (the state the structure's
    /// stiffness is taken at): with `force`, the deflected body is drawn
    /// again from the stored state alone (and the structures' models).
    pub strung: Point,
}

/// A structure taken rigid in the reference state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceRigid {
    /// The placed structure's id.
    pub structure: String,
    /// Why ("no structure model (detail level 1 or 2)").
    pub reason: String,
}

/// What a reference state was solved from: stable 128-bit fingerprints
/// ([`StableHash`], hexadecimal) of its inputs, each part apart so that
/// a stale state says what changed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceFingerprint {
    /// The line as solved ([`line_fingerprint`]).
    pub line: String,
    /// The wires' library entries the line uses.
    pub wires: String,
    /// The structure types the line places, their models, poles and
    /// cross-sections.
    pub structures: String,
    /// The reference case's weather (and so the structures' own wind and
    /// ice in it).
    pub case: String,
}

impl ReferenceFingerprint {
    /// What differs from `now`, in words (empty: nothing).
    pub fn changed(&self, now: &Self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.line != now.line {
            out.push("the line");
        }
        if self.wires != now.wires {
            out.push("the wires");
        }
        if self.structures != now.structures {
            out.push("the structures");
        }
        if self.case != now.case {
            out.push("the reference case's weather");
        }
        out
    }
}

/// A stable 128-bit hash (FNV-1a), the same in every build and every
/// run: fingerprints stored in files ([`ReferenceFingerprint`]) must not
/// change with the compiler (the standard library's hasher may).
#[derive(Debug, Clone, Copy)]
pub struct StableHash(u128);

impl Default for StableHash {
    fn default() -> Self {
        Self(0x6c62_272e_07bb_0142_62b8_2175_6295_c58d)
    }
}

impl StableHash {
    /// Feeds `bytes`.
    pub fn bytes(&mut self, bytes: &[u8]) {
        const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
        for b in bytes {
            self.0 ^= u128::from(*b);
            self.0 = self.0.wrapping_mul(PRIME);
        }
    }

    /// Feeds `value`'s `Debug` text and a separator.
    pub fn debug(&mut self, value: &dyn std::fmt::Debug) {
        // Writing into a hash never fails.
        let _ = std::fmt::Write::write_fmt(self, format_args!("{value:?}\u{1f}"));
    }

    /// Feeds `value` as JSON and a separator (a value that cannot be
    /// written: its `Debug` text).
    pub fn json<T: Serialize + std::fmt::Debug>(&mut self, value: &T) {
        if serde_json::to_writer(&mut *self, value).is_err() {
            self.debug(value);
        }
        self.bytes(&[0x1f]);
    }

    /// The hash, 32 hexadecimal digits.
    pub fn hex(&self) -> String {
        format!("{:032x}", self.0)
    }
}

impl std::fmt::Write for StableHash {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.bytes(s.as_bytes());
        Ok(())
    }
}

impl std::io::Write for StableHash {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.bytes(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The fingerprint of `line` as a reference state depends on it: its
/// geometry, structures (as resolved), sections, jumpers, spacers, the
/// creep and bimetallic settings and the calculation options — not its
/// name, display, electrical systems and circuits, structure groups, nor
/// the calculation level chosen for the checks (the state is always
/// solved at its own level), nor the reference itself.
///
/// Expected cost: one pass over the line as JSON (a few ms for a long
/// line).
pub fn line_fingerprint(line: &Line) -> String {
    let mut h = StableHash::default();
    h.json(&line.crs);
    h.json(&line.height_datum);
    h.json(&line.alignment);
    h.json(&line.routes);
    h.json(&line.alignments);
    h.json(&line.alignment_points);
    h.json(&line.structures);
    h.json(&line.sections);
    h.json(&line.jumpers);
    h.json(&line.spacers);
    h.json(&line.creep_stretch);
    h.json(&line.bimetallic);
    let mut solver = line.solver;
    solver.structures = crate::StructuresInSolve::default();
    h.json(&solver);
    h.json(&line.defaults);
    h.hex()
}

impl Line {
    /// The reference state the solves take now: set, solved and checked
    /// current ([`ReferenceStatus::Current`]); none otherwise.
    pub fn reference_in_use(&self) -> Option<&ReferenceState> {
        let r = self.reference.as_ref()?;
        (r.status == ReferenceStatus::Current)
            .then_some(r.state.as_ref())
            .flatten()
    }

    /// The displacement of point `label` of structure `structure` in the
    /// reference state in use (line axes, m); none: no state in use, or
    /// the point not moved (rigid, or no wire solved by finite elements
    /// on it).
    pub fn reference_move(&self, structure: &str, label: &str) -> Option<Point> {
        self.reference_in_use()?
            .points
            .iter()
            .find(|p| p.structure == structure && p.label == label)
            .map(|p| p.displacement)
            .filter(|d| *d != [0.0; 3])
    }

    /// This line with its reference state set aside for a check that
    /// keeps the structures as drawn, with why (said in its results): a
    /// copy only when one is in use.
    pub fn as_drawn(&self, why: &str) -> Cow<'_, Line> {
        if self.reference_in_use().is_none() {
            return Cow::Borrowed(self);
        }
        let mut line = self.clone();
        if let Some(r) = line.reference.as_mut() {
            r.status = ReferenceStatus::SetAside(why.to_owned());
        }
        Cow::Owned(line)
    }

    /// Marks the reference state out of date (`why`) until checked again
    /// — an edit made in memory only (a structure dragged).
    pub fn reference_stale(&mut self, why: &str) {
        if let Some(r) = self.reference.as_mut()
            && r.state.is_some()
        {
            r.status = ReferenceStatus::Stale(why.to_owned());
        }
    }

    /// What a result says of the reference state (none: no reference
    /// case set): in use — the structures as deflected in case X, other
    /// cases keeping that lean, the structures as drawn; else why the
    /// structures are as drawn.
    pub fn reference_note(&self) -> Option<String> {
        let r = self.reference.as_ref()?;
        let Some(state) = &r.state else {
            return Some(format!(
                "reference case \"{}\" not solved yet: structures as drawn (Line › Criteria › \
                 Calculation options: Solve)",
                r.case
            ));
        };
        Some(match &r.status {
            ReferenceStatus::Current => {
                let rigid = if state.rigid.is_empty() {
                    String::new()
                } else {
                    let mut ids: Vec<&str> =
                        state.rigid.iter().map(|x| x.structure.as_str()).collect();
                    let more = ids.len().saturating_sub(10);
                    ids.truncate(10);
                    format!(
                        "; as drawn (no computable model): {}{}",
                        ids.join(", "),
                        if more > 0 {
                            format!(" and {more} more")
                        } else {
                            String::new()
                        }
                    )
                };
                format!(
                    "structures as deflected in case \"{}\" (calculation level {}): each \
                     flexible structure's attachment points moved as solved there; every other \
                     case keeps that lean, not its own — NOT ON THE SAFE SIDE where a case \
                     leans the structures further (heavier ice, stronger wind, colder: theory, \
                     Structure flexibility › Reference state){rigid}",
                    state.case, state.level
                )
            }
            ReferenceStatus::Stale(why) => format!(
                "reference case \"{}\" out of date ({why}): structures as drawn — solve it again \
                 (Line › Criteria › Calculation options)",
                r.case
            ),
            ReferenceStatus::SetAside(why) => {
                format!(
                    "structures as drawn ({why}); the reference case \"{}\" not used here",
                    r.case
                )
            }
            ReferenceStatus::Unchecked => format!(
                "reference case \"{}\" not checked against the structure and wire libraries \
                 here: structures as drawn",
                r.case
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stable_hash_is_fnv_1a() {
        // FNV-1a 128 of "" is its offset basis; of "a" the published value.
        assert_eq!(
            StableHash::default().hex(),
            "6c62272e07bb014262b821756295c58d"
        );
        let mut h = StableHash::default();
        h.bytes(b"a");
        assert_eq!(h.hex(), "d228cb696f1a8caf78912b704e4a8964");
    }

    #[test]
    fn what_changed_in_words() {
        let a = ReferenceFingerprint {
            line: "1".into(),
            wires: "2".into(),
            structures: "3".into(),
            case: "4".into(),
        };
        assert!(a.changed(&a).is_empty());
        let b = ReferenceFingerprint {
            structures: "x".into(),
            case: "y".into(),
            ..a.clone()
        };
        assert_eq!(
            a.changed(&b),
            ["the structures", "the reference case's weather"]
        );
    }
}
