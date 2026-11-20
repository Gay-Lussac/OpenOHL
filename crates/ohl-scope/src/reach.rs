// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Each check's **reach**: how far from what it checks other objects can
//! change its result (ADR-0013, phase 10). Built-in checks declare theirs
//! here; a plugin's check may declare one in its manifest
//! (`ohl_plugins::Manifest::check_reach`), and without one the most
//! conservative default applies ([`Reach::Unknown`]).

use serde::{Deserialize, Serialize};

/// The field distance (owner, 2026-11-16): a field check pulls in the
/// lines within this distance of the checked wires, m. A setting of the
/// check; said in every result.
pub const FIELD_DISTANCE: f64 = 500.0;

/// A plugin check that declares no reach: the working set's whole line
/// and every line within this distance of its wires, m (stated in every
/// such result).
pub const UNKNOWN_REACH: f64 = 2_000.0;

/// The largest reach a plugin may declare, m (more: refused when its
/// manifest is read — use no reach, the whole line, instead).
pub const MAX_DECLARED_REACH: f64 = 10_000.0;

/// Twin towers: structures of two lines within this distance in plan and
/// of the same type are one tower carrying both lines' wires (the
/// network's import lists them so, `ohl_store_pg::import::TWIN_TOLERANCE`).
pub const TWIN_TOLERANCE: f64 = 0.5;

/// How far other objects can change a check's result.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reach {
    /// What the working set holds already: its sections, and what solving
    /// each alone reads (the other sections and jumpers on its
    /// structures) — the measurements between the conductors of one span.
    Own,
    /// Every wire, of this line or another, that comes within `metres` in
    /// plan of a checked wire (clearances between wires, crossings).
    Wires {
        /// The distance, m.
        metres: f64,
    },
    /// The other lines' wires on the checked structures: their twins
    /// within `tolerance` (structure loads).
    Towers {
        /// Twins within this distance in plan, m.
        tolerance: f64,
    },
    /// Every line with wires within `metres` in plan of the checked
    /// wires, its wires within that distance (fields).
    Lines {
        /// The distance, m.
        metres: f64,
    },
    /// Not known (a plugin's check declaring none): the most conservative
    /// default — the working set's whole line and every line within
    /// `metres`.
    Unknown {
        /// Every line within this distance, m.
        metres: f64,
    },
}

impl Reach {
    /// In words, for results ("wires within 12.4 m").
    pub fn describe(&self) -> String {
        match self {
            Reach::Own => "the working set and the wires on its structures".into(),
            Reach::Wires { metres } => format!("wires within {metres:.1} m"),
            Reach::Towers { tolerance } => {
                format!("other lines' wires on the same towers (twins within {tolerance} m)")
            }
            Reach::Lines { metres } => format!("lines within {metres:.0} m"),
            Reach::Unknown { metres } => {
                format!("no reach declared: the whole line and every line within {metres:.0} m")
            }
        }
    }
}

/// The checks the network computes on a working set.
#[derive(Debug, Clone, PartialEq)]
pub enum CheckKind {
    /// Clearances by rule: wires to the ground, to points and to other
    /// wires of this line and of other lines (crossings among them). Its
    /// reach is the largest of its wire rules' ([`crate::clearance`]).
    Clearances {
        /// The largest reach of its wire-to-wire rules, m.
        wire_rules: f64,
    },
    /// The distance between conductors in a span (and under unequal
    /// wind), measured for a design code's plugin.
    PhaseDistance,
    /// The magnetic field, with the lines within `distance`.
    Fields {
        /// The field distance, m ([`FIELD_DISTANCE`] by default).
        distance: f64,
    },
    /// The loads on the structures, with other lines' sections on shared
    /// towers.
    StructureLoads,
    /// A plugin's check: the reach its manifest declares, if any.
    Plugin {
        /// Declared, m (none: not declared).
        declared: Option<f64>,
    },
}

/// The reach of `check` (built in: declared here; a plugin's: as its
/// manifest says, else [`Reach::Unknown`]).
pub fn reach_of(check: &CheckKind) -> Reach {
    match check {
        CheckKind::Clearances { wire_rules } if *wire_rules > 0.0 => Reach::Wires {
            metres: *wire_rules,
        },
        CheckKind::Clearances { .. } => Reach::Own,
        CheckKind::PhaseDistance => Reach::Own,
        CheckKind::Fields { distance } => Reach::Lines { metres: *distance },
        CheckKind::StructureLoads => Reach::Towers {
            tolerance: TWIN_TOLERANCE,
        },
        CheckKind::Plugin {
            declared: Some(m), ..
        } if m.is_finite() && *m >= 0.0 => {
            if *m == 0.0 {
                Reach::Own
            } else {
                Reach::Wires { metres: *m }
            }
        }
        CheckKind::Plugin { .. } => Reach::Unknown {
            metres: UNKNOWN_REACH,
        },
    }
}

/// Whether a check of this reach run on a working set's replica alone
/// (nothing pulled in) misses what can change its result: only
/// [`Reach::Own`] is held whole by a replica.
pub fn needs_neighbours(reach: &Reach) -> bool {
    !matches!(reach, Reach::Own)
}

/// What a check run on a replica alone says, when its reach needs
/// neighbours (never shown as passing): `None` when the replica holds its
/// reach.
pub fn replica_note(check: &str, reach: &Reach) -> Option<String> {
    needs_neighbours(reach).then(|| {
        format!(
            "NOT ALL CHECKED: {check} on a working set's replica, its neighbours not pulled in \
             ({}): run it on the network to pull them in — it is not passing because nothing \
             is there",
            reach.describe()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_checks_declare_their_reach() {
        assert_eq!(
            reach_of(&CheckKind::Clearances { wire_rules: 12.5 }),
            Reach::Wires { metres: 12.5 }
        );
        assert_eq!(
            reach_of(&CheckKind::Clearances { wire_rules: 0.0 }),
            Reach::Own
        );
        assert_eq!(reach_of(&CheckKind::PhaseDistance), Reach::Own);
        assert_eq!(
            reach_of(&CheckKind::Fields {
                distance: FIELD_DISTANCE
            }),
            Reach::Lines { metres: 500.0 }
        );
        assert_eq!(
            reach_of(&CheckKind::StructureLoads),
            Reach::Towers { tolerance: 0.5 }
        );
    }

    #[test]
    fn a_plugin_without_a_reach_takes_the_most_conservative_default() {
        assert_eq!(
            reach_of(&CheckKind::Plugin { declared: None }),
            Reach::Unknown {
                metres: UNKNOWN_REACH
            }
        );
        assert_eq!(
            reach_of(&CheckKind::Plugin {
                declared: Some(f64::NAN)
            }),
            Reach::Unknown {
                metres: UNKNOWN_REACH
            }
        );
        assert_eq!(
            reach_of(&CheckKind::Plugin {
                declared: Some(30.0)
            }),
            Reach::Wires { metres: 30.0 }
        );
        assert_eq!(
            reach_of(&CheckKind::Plugin {
                declared: Some(0.0)
            }),
            Reach::Own
        );
    }

    #[test]
    fn a_replica_alone_says_what_it_did_not_pull_in() {
        assert!(replica_note("phase distances", &Reach::Own).is_none());
        let n = replica_note("clearances", &Reach::Wires { metres: 9.0 }).expect("said");
        assert!(n.starts_with("NOT ALL CHECKED"), "{n}");
        assert!(n.contains("wires within 9.0 m"), "{n}");
    }
}
