// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The member wind's force law on one member (owner 2026-11-15; theory:
//! `docs/theory/structure-wind.md`): the **inclination factor** k(β) on
//! the force normal to a member inclined to the wind, and the **default
//! force coefficient** of a profile whose drag factor nobody gave.
//!
//! - **Inclination** (β the angle between the wind and the member's
//!   normal, 90° − θ, θ between the wind and the member's axis): the
//!   shape's curve from the section library's `[wind] inclination`, else
//!   built in from SIA 261:2014 Tableau 74 (p. 112) — the sharp-edged
//!   profiles' reduction factor c_red for every shape that is not round,
//!   sin θ (= cos β) for round bars and tubes, raised to Tableau 74's own
//!   ratio c_β / c_β(0) where that is larger. SIA 261 gives nothing beyond
//!   β = 60°: the factor at 60° is held there (on the safe side).
//!   `[wind] member_law = "sin2"` keeps the cross-flow law (k = sin²θ) for
//!   comparison only.
//! - **Force coefficient** of a profile when no drag factor is given at
//!   all (no face drag, no drag for its shape, no `[analysis] drag`):
//!   SIA 261:2014 Tableau 71 (p. 109) for profiles, Tableau 72 (p. 110) for
//!   round bars and tubes, the largest over what the program does not look
//!   up (the wind's angle to the profile, the member's slenderness, the
//!   Reynolds number), on the table's reference width h — so the force
//!   q·c·h·L whatever the wind's angle to the profile.

use std::f64::consts::PI;

use ohl_core::units::Angle;
use ohl_sections::{MemberLaw, Section, Shape, StructureWind};

/// The standard the built-in values are read from.
pub const SIA261: &str = "SIA 261:2014";

/// SIA 261:2014 Tableau 74 (p. 112), "Coefficients pour treillis
/// spatiaux": β (degrees between the wind and the bar's normal) of both
/// tables.
pub const TAB74_BETA: [f64; 5] = [0.0, 15.0, 30.0, 45.0, 60.0];
/// Tableau 74, sharp-edged profiles ("profilés à arêtes vives"): the
/// reduction factor c_red for the inclination β.
pub const TAB74_SHARP: [f64; 5] = [1.0, 0.98, 0.93, 0.88, 0.8];
/// Tableau 74, round bars, d·√q_p0·(z/10)^α_r ≤ 0.07: c_β.
pub const TAB74_ROUND_LOW: [f64; 5] = [1.2, 1.16, 1.04, 0.85, 0.6];
/// Tableau 74, round bars, d·√q_p0·(z/10)^α_r > 0.07: c_β.
pub const TAB74_ROUND_HIGH: [f64; 5] = [0.6, 0.58, 0.53, 0.42, 0.28];

/// SIA 261:2014 Tableau 72 (p. 110): the force coefficient of smooth or
/// slightly rough bars and tubes for d·√q_p0·(z/10)^α_r ≤ 0.07 — the
/// largest of the table for them (0.5 and 0.7 above 0.07).
pub const TAB72_ROUND: f64 = 1.2;
/// SIA 261:2014 Tableau 71 (p. 109): the largest |c_f1| or |c_f2| over the
/// tabulated wind angles φ (0°, 45°, 90°, 135°, 180°) of the two angles —
/// unequal legs 0.5 h × h: 2.0; equal legs: 2.1 (c_f1 at φ = 45°).
pub const TAB71_ANGLE: f64 = 2.1;
/// Tableau 71, the channel (flanges 0.43 h): 2.05 (c_f1 at φ = 0°).
pub const TAB71_CHANNEL: f64 = 2.05;
/// Tableau 71, the I sections (flanges 0.48 h: 2.05 at φ = 0°; flanges h:
/// 1.9 at φ = 90°) — the larger.
pub const TAB71_I: f64 = 2.05;
/// Tableau 71, two channels with their flanges facing, closed by plates
/// (1.6 h wide): c_f2 2.2 at φ = 90°; taken with the I sections' for two
/// channels back to back (not in the table): the larger.
pub const TAB71_DOUBLE_CHANNEL: f64 = 2.2;
/// Tableau 71, the boxes (0.5 h × h: 2.1 at φ = 0°; h × h: 2.0) — the
/// larger, also for a polygonal tube (not in the table: an assumption).
pub const TAB71_BOX: f64 = 2.1;
/// Tableau 71, two angles back to back as a T (outstanding legs 0.45 h of
/// the flange h, 0.1 h apart): 1.6 (c_f1 at φ = 0°).
pub const TAB71_DOUBLE_ANGLE: f64 = 1.6;
/// Tableau 71, the T of two angles with a plate 1.1 h between them: 2.4
/// (c_f2 at φ = 135°) — for a double angle whose back-to-back legs are
/// longer than half its width (the table's T is 0.45).
pub const TAB71_DOUBLE_ANGLE_LONG: f64 = 2.4;
/// Tableau 71, two angles in a star (a cross, 0.1 h apart): 1.75 (c_f1
/// at φ = 0°).
pub const TAB71_CROSS: f64 = 1.75;
/// Tableau 71, the flat plate (0.1 h × h): 2.0 (c_f1 at φ = 0°) — the
/// least drag of a gusset plate whose members take a built-in
/// coefficient.
pub const TAB71_PLATE: f64 = 2.0;

/// Whether a shape is round (Tableau 74's round bars): a round bar or a
/// circular tube.
pub fn is_round(shape: &str) -> bool {
    matches!(shape, "round_bar" | "circular_hollow")
}

/// Linear interpolation in Tableau 74's β (0–60°); `beta` within it.
fn tab74(values: &[f64; 5], beta: f64) -> f64 {
    let b = beta.clamp(0.0, 60.0);
    let k = ((b / 15.0).floor() as usize).min(3);
    let t = (b - TAB74_BETA[k]) / 15.0;
    values[k] + t * (values[k + 1] - values[k])
}

/// Where an inclination factor comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InclinationFrom {
    /// The shape's curve in the section library's `[wind] inclination`.
    Data,
    /// Built in: SIA 261 Tableau 74's sharp-edged profiles.
    Sharp,
    /// Built in: round bars, sin θ (raised to Tableau 74's ratio).
    Round,
    /// The comparison law sin²θ (`member_law = "sin2"`).
    Sin2,
}

/// An inclination factor and how it was found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Inclined {
    /// The factor on the force normal to the member (1: the member normal
    /// to the wind).
    pub k: f64,
    /// Where it comes from.
    pub from: InclinationFrom,
    /// β is beyond the curve's last angle: its last factor held.
    pub held: bool,
}

/// β in degrees from sin θ (θ between the wind and the member's axis).
pub fn beta_degrees(sin_theta: f64) -> f64 {
    sin_theta.clamp(0.0, 1.0).acos().to_degrees()
}

/// The built-in factor of round bars at β (degrees): sin θ = cos β, raised
/// to the larger of Tableau 74's two ratios c_β / c_β(0) where that is
/// larger (they differ from cos β by up to 2 %), held beyond 60°.
pub fn round_factor(beta: f64) -> f64 {
    let b = beta.clamp(0.0, 60.0);
    let ratio = tab74(&TAB74_ROUND_LOW, b) / TAB74_ROUND_LOW[0];
    let high = tab74(&TAB74_ROUND_HIGH, b) / TAB74_ROUND_HIGH[0];
    b.to_radians().cos().max(ratio).max(high)
}

/// The built-in factor of every shape that is not round at β (degrees):
/// Tableau 74's c_red, held beyond 60°, at least cos β (the projected
/// area: the straight line from 1.0 at 0° to 0.98 at 15° lies up to 0.3 %
/// below cos β between 0° and about 9°).
pub fn sharp_factor(beta: f64) -> f64 {
    let b = beta.clamp(0.0, 60.0);
    tab74(&TAB74_SHARP, b).max(b.to_radians().cos())
}

/// The inclination factor of a member of shape `shape` (its name in
/// files) at sin θ, with the section library's `[wind]` (`data`).
pub fn inclination(shape: &str, sin_theta: f64, data: Option<&StructureWind>) -> Inclined {
    let law = data.map_or(MemberLaw::default(), |d| d.member_law);
    if law == MemberLaw::Sin2 {
        return Inclined {
            k: sin_theta * sin_theta,
            from: InclinationFrom::Sin2,
            held: false,
        };
    }
    let beta = beta_degrees(sin_theta);
    if let Some(curve) = data.and_then(|d| d.inclination_of(shape)) {
        return Inclined {
            k: curve.at(beta),
            from: InclinationFrom::Data,
            held: curve.held(beta),
        };
    }
    let held = beta > TAB74_BETA[4] + 1e-9;
    if is_round(shape) {
        Inclined {
            k: round_factor(beta),
            from: InclinationFrom::Round,
            held,
        }
    } else {
        Inclined {
            k: sharp_factor(beta),
            from: InclinationFrom::Sharp,
            held,
        }
    }
}

/// A built-in force coefficient of a profile: the force on a member normal
/// to the wind is q × `coefficient` × `reference` × L.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CodeDrag {
    /// The force coefficient.
    pub coefficient: f64,
    /// The reference width it is on (the table's h), m.
    pub reference: f64,
    /// Where it comes from, in words (table, page, which value).
    pub source: &'static str,
}

impl CodeDrag {
    /// The drag factor on a projected width `width` (m) giving the same
    /// force: coefficient × reference / width.
    pub fn on_width(&self, width: f64) -> f64 {
        if width > 0.0 {
            self.coefficient * self.reference / width
        } else {
            self.coefficient
        }
    }
}

/// The built-in force coefficient of a profile (SIA 261:2014 Tableaux 71
/// and 72), with its reference width: the larger of the profile's extents
/// along the section's y and z (the table's h: an angle's longer leg, an
/// I section's height or width); a polygonal tube's width across its
/// corners. None for a `general` section (no shape: the generic drag).
pub fn code_drag(section: &Section) -> Option<CodeDrag> {
    let along_z = section.wind_width(Angle::from_degrees(0.0));
    let along_y = section.wind_width(Angle::from_degrees(90.0));
    let extent = along_z.max(along_y);
    let (coefficient, reference, source) = match &section.shape {
        Shape::EqualAngle { .. } | Shape::UnequalAngle { .. } => (
            TAB71_ANGLE,
            extent,
            "Tab. 71 (p. 109), angles: the largest c_f over the wind's angles, on the longer leg",
        ),
        Shape::Channel { .. } => (
            TAB71_CHANNEL,
            extent,
            "Tab. 71 (p. 109), channel: the largest c_f, on the larger of height and width",
        ),
        Shape::ISection { .. } => (
            TAB71_I,
            extent,
            "Tab. 71 (p. 109), I sections: the largest c_f, on the larger of height and width",
        ),
        Shape::DoubleChannel { .. } => (
            TAB71_DOUBLE_CHANNEL,
            extent,
            "Tab. 71 (p. 109), not tabulated: the largest c_f of the I sections and the two \
             channels, on the larger extent",
        ),
        Shape::RectangularHollow { .. } => (
            TAB71_BOX,
            extent,
            "Tab. 71 (p. 109), boxes: the largest c_f, on the longer side",
        ),
        Shape::PolygonalHollow { sides, d, .. } => (
            TAB71_BOX,
            d / (PI / f64::from((*sides).max(3))).cos(),
            "Tab. 71 (p. 109), not tabulated: the boxes' largest c_f (an assumption), on the \
             width across the corners",
        ),
        Shape::DoubleAngle { .. } => {
            // The back-to-back legs (along z) over the width (along y):
            // the table's T is 0.45 h; a longer one takes the T with the
            // long plate (the larger).
            if along_z <= 0.5 * along_y {
                (
                    TAB71_DOUBLE_ANGLE,
                    extent,
                    "Tab. 71 (p. 109), two angles back to back (T): the largest c_f, on the \
                     larger extent",
                )
            } else {
                (
                    TAB71_DOUBLE_ANGLE_LONG,
                    extent,
                    "Tab. 71 (p. 109), two angles back to back with long back legs: the largest \
                     c_f of the T shapes, on the larger extent",
                )
            }
        }
        Shape::StarAngle { .. } | Shape::CrossAngle { .. } => (
            TAB71_CROSS,
            extent,
            "Tab. 71 (p. 109), angles in a star (cross): the largest c_f, on the larger extent",
        ),
        Shape::CircularHollow { d, .. } | Shape::RoundBar { d } => (
            TAB72_ROUND,
            *d,
            "Tab. 72 (p. 110), bars and tubes: the largest c_f (d·√q_p0·(z/10)^α_r ≤ 0.07), on \
             the diameter",
        ),
        Shape::General { .. } => return None,
    };
    Some(CodeDrag {
        coefficient,
        reference,
        source,
    })
}
