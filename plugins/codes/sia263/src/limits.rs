// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The limits SIA 263:2013 puts on a member's size (`member-limits`,
//! 2026-11-26, roadmap O49): what OpenOHL's member sizer keeps to beside
//! the usage, the value the check's own.
//!
//! - **4.5.1.7** (taken up for lattice members by 5.4.1.7): the
//!   slenderness of compression members λ_K = L_K / i at most 250 for
//!   bracing and secondary members, 200 for main load-carrying members —
//!   applied by OpenOHL to a member compressed in a load case only. As the
//!   check's own note takes it: a redundant member is secondary (250),
//!   every other member load-carrying (200; a tower's legs and its loaded
//!   bracing carry computed loads — the smaller value, on the safe side).
//!   A crossing diagonal's length with its partner not bracing it (the
//!   longer one: on the safe side).
//!
//! Left out (said in the theory page): 4.5.1.7's 160 for members of
//! structures loaded in fatigue (the data do not say so); 5.7.3.2's least
//! thicknesses (for cold-formed sheets and profiles, not hot-rolled
//! angles). SIA 263 bounds no flat width to thickness of hot-rolled
//! angles: a slender leg lowers the resistance (class 4) instead.

use crate::model::{Member, MemberClass, MemberKind};
use crate::shared::Capacity;

/// One limit, as the interface gives it.
#[derive(Debug, Clone, PartialEq)]
pub struct Limit {
    /// What is bounded.
    pub name: String,
    /// The member's value.
    pub value: f64,
    /// Largest allowed.
    pub maximum: Option<f64>,
    /// Smallest allowed.
    pub minimum: Option<f64>,
    /// Only for a member carrying compression.
    pub compression: bool,
    /// The clause.
    pub clause: String,
}

/// 4.5.1.7: bracing and secondary members.
pub const SECONDARY_SLENDERNESS: f64 = 250.0;
/// 4.5.1.7: main load-carrying members.
pub const MAIN_SLENDERNESS: f64 = 200.0;

/// The limits of member `m` whose capacities are `c` (worked out with
/// its crossing partner not bracing it).
pub fn member_limits(m: &Member, c: &Capacity) -> Vec<Limit> {
    if matches!(m.kind, MemberKind::Cable | MemberKind::Tension) {
        return Vec::new();
    }
    let Some(lambda) = c.kl_r else {
        return Vec::new();
    };
    let (maximum, kind) = if m.design.class == Some(MemberClass::Redundant) {
        (SECONDARY_SLENDERNESS, "secondary members")
    } else {
        (MAIN_SLENDERNESS, "main load-carrying members")
    };
    vec![Limit {
        name: "λ_K, the slenderness of a compression member".into(),
        value: lambda,
        maximum: Some(maximum),
        minimum: None,
        compression: true,
        clause: format!("SIA 263:2013 4.5.1.7 ({kind})"),
    }]
}
