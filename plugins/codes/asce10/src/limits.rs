// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The limits ASCE 10-97 puts on a member's size (`member-limits`,
//! 2026-11-26, roadmap O49): what OpenOHL's member sizer keeps to beside
//! the usage. Each value is the check's own (the same slenderness, w/t and
//! thickness the capacity is worked out with):
//!
//! - **3.4 Slenderness ratios**, members carrying calculated compressive
//!   stress: leg members L/r < 150; other members KL/r < 200; redundant
//!   members KL/r ≤ 250 — applied by OpenOHL to a member compressed in a
//!   load case only. A member without a class is taken as one of the
//!   "other members" (200, the smaller of the two that could apply: on the
//!   safe side). A crossing diagonal's length with its partner not
//!   bracing it (the longer one: on the safe side).
//! - **3.7.1 Maximum w/t ratio** of an angle's leg: 25 (compression
//!   members: angles).
//! - **3.3 Minimum sizes**: a thickness of 1/8 in. (3 mm) for members,
//!   "suggested" by the standard — given as a limit, its clause saying so.
//!
//! Left out (said in the theory page): 3.4's tension-only members
//! "detailed with draw" (L/r between 300 and 500: the data do not say
//! which are detailed with draw); 3.8.1's lipped angles (w/t 60: no lipped
//! angle profile in OpenOHL's sections).

use crate::asce10::Capacity;
use crate::model::{Member, MemberClass, MemberKind};

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

/// 3.4: leg members' L/r.
pub const LEG_SLENDERNESS: f64 = 150.0;
/// 3.4: other members' KL/r.
pub const OTHER_SLENDERNESS: f64 = 200.0;
/// 3.4: redundant members' KL/r.
pub const REDUNDANT_SLENDERNESS: f64 = 250.0;
/// 3.7.1: an angle's legs' w/t.
pub const ANGLE_W_T: f64 = 25.0;
/// 3.3: members' thickness (suggested), m.
pub const MEMBER_THICKNESS: f64 = 0.003;

/// The limits of member `m` whose capacities are `c` (worked out with
/// its crossing partner not bracing it, as the solve's cap takes them).
pub fn member_limits(m: &Member, c: &Capacity) -> Vec<Limit> {
    let mut out = Vec::new();
    let carries_compression = !matches!(m.kind, MemberKind::Cable | MemberKind::Tension);
    if carries_compression {
        match (m.design.class, c.l_r, c.kl_r) {
            (Some(MemberClass::Leg), Some(l_r), _) => out.push(Limit {
                name: "L/r, the slenderness of a leg".into(),
                value: l_r,
                maximum: Some(LEG_SLENDERNESS),
                minimum: None,
                compression: true,
                clause: "ASCE 10-97 3.4 (leg members)".into(),
            }),
            (Some(MemberClass::Redundant), _, Some(kl_r)) => out.push(Limit {
                name: "KL/r, the effective slenderness of a redundant".into(),
                value: kl_r,
                maximum: Some(REDUNDANT_SLENDERNESS),
                minimum: None,
                compression: true,
                clause: "ASCE 10-97 3.4 (redundant members)".into(),
            }),
            (class, _, Some(kl_r)) => out.push(Limit {
                name: "KL/r, the effective slenderness".into(),
                value: kl_r,
                maximum: Some(OTHER_SLENDERNESS),
                minimum: None,
                compression: true,
                clause: if class.is_none() {
                    "ASCE 10-97 3.4 (other members; a member without a class taken so)".into()
                } else {
                    "ASCE 10-97 3.4 (other members)".into()
                },
            }),
            _ => {}
        }
        if let Some(w_t) = c.w_t {
            out.push(Limit {
                name: "w/t, the flat width to thickness of the angle's legs".into(),
                value: w_t,
                maximum: Some(ANGLE_W_T),
                minimum: None,
                compression: true,
                clause: "ASCE 10-97 3.7.1".into(),
            });
        }
    }
    if let Some(t) = thickness(m) {
        out.push(Limit {
            name: "t, the thickness of the member's legs, m".into(),
            value: t,
            maximum: None,
            minimum: Some(MEMBER_THICKNESS),
            compression: false,
            clause: "ASCE 10-97 3.3 (suggested minimum, 1/8 in.)".into(),
        });
    }
    out
}

/// The thickness of an angle member's legs, m (the design data's where
/// given, else the profile's; none: not an angle).
fn thickness(m: &Member) -> Option<f64> {
    use ohl_sections::Shape;
    let t = match m.section.profile.as_ref()?.shape {
        Shape::EqualAngle { t, .. }
        | Shape::UnequalAngle { t, .. }
        | Shape::DoubleAngle { t, .. }
        | Shape::StarAngle { t, .. }
        | Shape::CrossAngle { t, .. } => t,
        _ => return None,
    };
    Some(m.design.thickness.unwrap_or(t))
}
