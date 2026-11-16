// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Measured movements of a placed structure's feet (owner 2026-11-16:
//! "the tower should not be initially built from a scan but from plans
//! then adjusted based on the movement of the feet and the lean of the
//! weather case"; in active development).
//!
//! A structure is modelled from its design plans (its type, placed where
//! the plans place it). What it does after it is built is measured at its
//! feet: a foundation settled, a leg pushed out, a pole's base tilted.
//! [`FootMovements`] keeps those measurements with the placed structure
//! ([`crate::Structure::foot_movements`]; line file version 23, written
//! only when a structure has some), with their source and date.
//!
//! - **A foot** ([`FootMovement::joint`]) is a support joint of the
//!   structure's model (its id: the foot of a leg), `"base"` for a pole,
//!   or [`ALL_FEET`] — every foot alike: the whole structure moved as one
//!   body about its origin (the only form a structure without a model
//!   takes).
//! - **Its movement** from the design: a translation (structure axes: x
//!   right of the line, y ahead, z up; m) and a tilt (degrees, the axis
//!   leaning to the right of the line and ahead) — a pole's base, or a
//!   foot held against turning.
//!
//! The structure analysis takes them as prescribed displacements of its
//! supports (`ohl_structures_fe::Prescribed`): the rigid-body motion and,
//! in a structure standing on more feet than it needs (a lattice tower
//! on four), the members' forces a foot moved apart from the others puts
//! on it (a differential settlement). Taken in the structures' reference
//! state (`ohl_loads::reference`; without a reference case, the foot
//! movements alone, solved without loads), in the calculation-level-3
//! setup and in the structure checks (theory: Structure flexibility ›
//! Foot movements).

use serde::{Deserialize, Serialize};

use crate::Point;

/// The foot name for every foot of a structure alike
/// ([`FootMovement::joint`]): the whole structure moved as one body (its
/// translation, and its tilt about its origin at the ground).
pub const ALL_FEET: &str = "all";

/// The foot name of a pole's base ([`FootMovement::joint`]).
pub const POLE_BASE: &str = "base";

/// A placed structure's measured foot movements
/// ([`crate::Structure::foot_movements`]).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FootMovements {
    /// Where they come from: who measured them and how (free text: "GNSS
    /// survey of the foundations").
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    /// When they were measured (free text; by convention 2026-05-12).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub date: String,
    /// The feet measured (a foot not listed: as designed).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub feet: Vec<FootMovement>,
}

/// One foot's measured movement from the design.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FootMovement {
    /// The foot: a support joint of the structure's model (its id),
    /// [`POLE_BASE`] for a pole, or [`ALL_FEET`].
    pub joint: String,
    /// Its translation, structure axes (x right of the line, y ahead, z
    /// up), m.
    #[serde(default, skip_serializing_if = "is_zero_point")]
    pub translation: Point,
    /// Its tilt, degrees: the axis leaning to the right of the line (x)
    /// and ahead (y) — a rotation about y by the first, about −x by the
    /// second. Taken where the foot is held against turning (a pole's
    /// base, a fixed foot); a pinned foot turns freely (said).
    #[serde(default, skip_serializing_if = "is_zero_tilt")]
    pub tilt_deg: [f64; 2],
}

fn is_zero_point(p: &Point) -> bool {
    *p == [0.0; 3]
}

fn is_zero_tilt(t: &[f64; 2]) -> bool {
    *t == [0.0; 2]
}

impl FootMovement {
    /// Its six displacements (translations m, rotations rad; structure
    /// axes): the tilt as a rotation about y (to the right) and about −x
    /// (ahead).
    pub fn displacement(&self) -> [f64; 6] {
        let [tx, ty] = self.tilt_deg.map(f64::to_radians);
        let t = self.translation;
        [t[0], t[1], t[2], -ty, tx, 0.0]
    }

    /// Whether it moves nothing.
    pub fn is_zero(&self) -> bool {
        is_zero_point(&self.translation) && is_zero_tilt(&self.tilt_deg)
    }

    /// The movement of a point at `r` (structure axes, from the
    /// structure's origin at the ground; m) when the whole structure moves
    /// with this foot as one body ([`ALL_FEET`]): t + θ × r.
    pub fn rigid_at(&self, r: Point) -> Point {
        let d = self.displacement();
        let w = [d[3], d[4], d[5]];
        [
            d[0] + w[1] * r[2] - w[2] * r[1],
            d[1] + w[2] * r[0] - w[0] * r[2],
            d[2] + w[0] * r[1] - w[1] * r[0],
        ]
    }
}

impl FootMovements {
    /// Whether nothing is measured (no foot, or every one zero).
    pub fn is_empty(&self) -> bool {
        self.feet.iter().all(FootMovement::is_zero)
    }

    /// The movement of the whole structure as one body, when given
    /// ([`ALL_FEET`]).
    pub fn all(&self) -> Option<&FootMovement> {
        self.feet.iter().find(|f| f.joint == ALL_FEET)
    }

    /// Why they are not valid (none: valid): a foot without a name, one
    /// named twice, every foot alike together with single feet, a value
    /// that is not a number, a tilt of 45° or more (not a measured lean).
    pub fn problem(&self) -> Option<String> {
        let mut seen: Vec<&str> = Vec::new();
        for f in &self.feet {
            let joint = f.joint.trim();
            if joint.is_empty() {
                return Some("a foot movement without its foot (a support joint)".into());
            }
            if seen.contains(&joint) {
                return Some(format!("foot {joint} given twice"));
            }
            seen.push(joint);
            if !f
                .translation
                .iter()
                .chain(&f.tilt_deg)
                .all(|v| v.is_finite())
            {
                return Some(format!("foot {joint}: a movement that is not a number"));
            }
            if f.tilt_deg.iter().any(|t| t.abs() >= 45.0) {
                return Some(format!(
                    "foot {joint}: a tilt of {:?}° — a measured lean is a few degrees at most",
                    f.tilt_deg
                ));
            }
        }
        if seen.len() > 1 && seen.contains(&ALL_FEET) {
            return Some(format!(
                "foot \"{ALL_FEET}\" (every foot alike) together with single feet: give one or \
                 the other"
            ));
        }
        None
    }

    /// In words for a list: "3 feet (GNSS survey, 2026-05-12)".
    pub fn summary(&self) -> String {
        let n = self.feet.iter().filter(|f| !f.is_zero()).count();
        let what = match self.all() {
            Some(_) => "every foot alike".to_owned(),
            None if n == 1 => "1 foot".to_owned(),
            None => format!("{n} feet"),
        };
        let about: Vec<&str> = [self.source.as_str(), self.date.as_str()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect();
        if about.is_empty() {
            what
        } else {
            format!("{what} ({})", about.join(", "))
        }
    }
}

impl crate::Line {
    /// Whether a structure has measured foot movements.
    pub fn has_foot_movements(&self) -> bool {
        self.structures
            .iter()
            .any(|s| s.foot_movements.as_ref().is_some_and(|f| !f.is_empty()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tilt_is_a_rotation_and_moves_points_rigidly() {
        let f = FootMovement {
            joint: ALL_FEET.into(),
            translation: [0.0, 0.0, -0.01],
            tilt_deg: [0.5, 0.0],
        };
        let d = f.displacement();
        assert!((d[4] - 0.5f64.to_radians()).abs() < 1e-15);
        // A point 20 m up leans to the right by 20 m × tan(0.5°) (small).
        let p = f.rigid_at([0.0, 0.0, 20.0]);
        assert!((p[0] - 20.0 * 0.5f64.to_radians()).abs() < 1e-12);
        assert!((p[2] + 0.01).abs() < 1e-15);
        let ahead = FootMovement {
            tilt_deg: [0.0, 1.0],
            ..f.clone()
        };
        assert!(ahead.rigid_at([0.0, 0.0, 10.0])[1] > 0.0);
    }

    #[test]
    fn problems_are_named() {
        let mut m = FootMovements {
            feet: vec![
                FootMovement {
                    joint: "F1".into(),
                    translation: [0.0, 0.0, -0.01],
                    ..FootMovement::default()
                },
                FootMovement {
                    joint: "F1".into(),
                    ..FootMovement::default()
                },
            ],
            ..FootMovements::default()
        };
        assert!(m.problem().is_some_and(|p| p.contains("twice")));
        m.feet[1].joint = ALL_FEET.into();
        assert!(m.problem().is_some_and(|p| p.contains("every foot alike")));
        m.feet.pop();
        assert_eq!(m.problem(), None);
        m.feet[0].tilt_deg = [60.0, 0.0];
        assert!(m.problem().is_some());
    }
}
