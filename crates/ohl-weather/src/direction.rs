// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The wind's direction (owner 2026-11-15) and the force of a wind at an
//! angle on a wire.
//!
//! **Convention** (one for the whole program): the angle θ names where the
//! wind comes **from**, relative to the line's direction at the place (the
//! bisector at an angle point), counted **clockwise seen from above**:
//! 0° from ahead, 90° from the right, 180° from behind, 270° from the
//! left. With \\( \hat a \\) the line's direction ahead (plan) and
//! \\( \hat r \\) the direction to its right, the wind comes from
//! \\( \hat f = \hat a\cos\theta + \hat r\sin\theta \\) and blows towards
//! \\( \hat w = -\hat f \\).
//!
//! Load case files up to version 6 and OHL-Structure count the other way
//! (from; 0° from the right, counted towards ahead):
//! \\( \theta = 90° - \theta_{old} \\). Such an angle is kept as read
//! ([`WindDirection::OldAngle`]) so that its forces are the old ones bit
//! for bit.

use serde::Deserialize;

/// Where the wind comes from, relative to the line at each place (see the
/// module's convention).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindDirection {
    /// From ahead (blowing back along the line), 0°.
    Ahead,
    /// From the right (blowing the wires to the left), 90°.
    Right,
    /// From behind (blowing ahead, "forward"), 180°.
    Back,
    /// From the left, 270°.
    Left,
    /// From this angle, degrees: 0 from ahead, 90 from the right,
    /// clockwise.
    Angle(f64),
    /// An angle in the convention of load case files up to version 6 and
    /// OHL-Structure (0° from the right, 90° from ahead), kept as read so
    /// that its forces are the old ones bit for bit; its angle in the
    /// program's convention is \\( 90° - \theta_{old} \\)
    /// ([`WindDirection::degrees`]).
    OldAngle(f64),
}

/// The named directions between the four main ones (OHL-Structure's names),
/// degrees in the program's convention.
const DIAGONALS: [(&str, f64); 4] = [
    ("ahead_right", 45.0),
    ("back_right", 135.0),
    ("back_left", 225.0),
    ("ahead_left", 315.0),
];

impl WindDirection {
    /// From a number of degrees (the program's convention).
    pub fn from_number(a: f64) -> Result<Self, String> {
        if a.is_finite() {
            Ok(Self::Angle(a))
        } else {
            Err(format!("wind direction {a}: an angle in degrees"))
        }
    }

    /// From its text: `"ahead"`, `"right"`, `"back"`, `"left"`, the
    /// diagonals (`"ahead_right"` 45°, `"back_right"` 135°, `"back_left"`
    /// 225°, `"ahead_left"` 315°) or an angle (`"30°"`, `"30"`).
    pub fn parse(text: &str) -> Result<Self, String> {
        let t = text.trim();
        match t {
            "ahead" => return Ok(Self::Ahead),
            "right" => return Ok(Self::Right),
            "back" => return Ok(Self::Back),
            "left" => return Ok(Self::Left),
            _ => {}
        }
        if let Some((_, a)) = DIAGONALS.iter().find(|(n, _)| *n == t) {
            return Ok(Self::Angle(*a));
        }
        t.parse::<ohl_core::units::Angle>()
            .map(|a| Self::Angle(a.degrees()))
            .ok()
            .or_else(|| {
                t.parse::<f64>()
                    .ok()
                    .filter(|a| a.is_finite())
                    .map(Self::Angle)
            })
            .ok_or_else(|| {
                format!(
                    "wind direction \"{t}\": \"ahead\", \"right\", \"back\", \"left\" or an angle \
                     in degrees the wind comes from (0 from ahead, 90 from the right, clockwise)"
                )
            })
    }

    /// The angle it comes from, degrees, in the program's convention (0
    /// from ahead, 90 from the right), in 0 to 360.
    pub fn degrees(self) -> f64 {
        match self {
            Self::Ahead => 0.0,
            Self::Right => 90.0,
            Self::Back => 180.0,
            Self::Left => 270.0,
            Self::Angle(a) => a.rem_euclid(360.0),
            Self::OldAngle(a) => (90.0 - a).rem_euclid(360.0),
        }
    }

    /// The angle it comes from in the old convention (0° from the right,
    /// 90° from ahead): load case files up to version 6, OHL-Structure.
    /// An [`WindDirection::OldAngle`] gives its angle as read.
    pub fn old_degrees(self) -> f64 {
        match self {
            Self::Right => 0.0,
            Self::Ahead => 90.0,
            Self::Left => 180.0,
            Self::Back => 270.0,
            Self::Angle(a) => (90.0 - a).rem_euclid(360.0),
            Self::OldAngle(a) => a,
        }
    }

    /// The side of a wind straight across the line (from the right: 90°,
    /// from the left: 270°), which takes the across rule; none otherwise.
    pub fn across_side(self) -> Option<crate::WindSide> {
        match self {
            Self::Right => Some(crate::WindSide::Right),
            Self::Left => Some(crate::WindSide::Left),
            Self::Angle(a) if a.rem_euclid(360.0) == 90.0 => Some(crate::WindSide::Right),
            Self::Angle(a) if a.rem_euclid(360.0) == 270.0 => Some(crate::WindSide::Left),
            Self::OldAngle(a) if a.rem_euclid(360.0) == 0.0 => Some(crate::WindSide::Right),
            Self::OldAngle(a) if a.rem_euclid(360.0) == 180.0 => Some(crate::WindSide::Left),
            _ => None,
        }
    }

    /// The direction of a side (the across rule's).
    pub fn of_side(side: crate::WindSide) -> Self {
        match side {
            crate::WindSide::Right => Self::Right,
            crate::WindSide::Left => Self::Left,
        }
    }

    /// Where it blows to, a unit vector in the line's axes at the place
    /// (x to the right of the line, y ahead): \\( (-\sin\theta,
    /// -\cos\theta) \\); exact for the four names, and for an
    /// [`WindDirection::OldAngle`] the old \\( (-\cos\theta_{old},
    /// -\sin\theta_{old}) \\) bit for bit.
    pub fn towards(self) -> [f64; 2] {
        match self {
            Self::Right => [-1.0, 0.0],
            Self::Ahead => [0.0, -1.0],
            Self::Left => [1.0, 0.0],
            Self::Back => [0.0, 1.0],
            Self::Angle(a) => {
                let r = a.to_radians();
                [-r.sin(), -r.cos()]
            }
            Self::OldAngle(a) => {
                let r = a.to_radians();
                [-r.cos(), -r.sin()]
            }
        }
    }

    /// Where it blows to in the project's axes, a horizontal unit vector,
    /// at a place where the line runs along `ahead` (a unit vector in
    /// plan).
    pub fn towards_in(self, ahead: [f64; 2]) -> [f64; 2] {
        let right = [ahead[1], -ahead[0]];
        let [x, y] = self.towards();
        [right[0] * x + ahead[0] * y, right[1] * x + ahead[1] * y]
    }

    /// Mirrored about the line (left and right swapped): θ → 360° − θ.
    pub fn mirrored(self) -> Self {
        match self {
            Self::Right => Self::Left,
            Self::Left => Self::Right,
            Self::Angle(a) => Self::Angle((360.0 - a).rem_euclid(360.0)),
            Self::OldAngle(a) => Self::OldAngle((180.0 - a).rem_euclid(360.0)),
            other => other,
        }
    }

    /// As written in files of the program's convention: the name, or the
    /// angle (an old angle converted, \\( 90° - \theta_{old} \\)).
    pub fn text(self) -> String {
        match self {
            Self::Right => "\"right\"".into(),
            Self::Ahead => "\"ahead\"".into(),
            Self::Left => "\"left\"".into(),
            Self::Back => "\"back\"".into(),
            Self::Angle(a) => format!("{a:?}"),
            Self::OldAngle(_) => format!("{:?}", self.degrees()),
        }
    }

    /// As written in a file of the old convention (load case files up to
    /// version 6): the name, or the old angle (as read for an
    /// [`WindDirection::OldAngle`]).
    pub fn old_text(self) -> String {
        match self {
            Self::OldAngle(a) => format!("{a:?}"),
            Self::Angle(_) => format!("{:?}", self.old_degrees()),
            named => named.text(),
        }
    }

    /// Whether it needs the program's convention in a file (an angle given
    /// in it, not a name and not an old angle).
    pub fn is_new_angle(self) -> bool {
        matches!(self, Self::Angle(_))
    }

    /// In words, always with "from": `from 90° (from the right)`, `from
    /// 180° (from behind: blowing ahead)`, `from 30°`.
    pub fn words(self) -> String {
        let a = self.degrees();
        let name = match a {
            0.0 => Some("from ahead: blowing back"),
            90.0 => Some("from the right"),
            180.0 => Some("from behind: blowing ahead"),
            270.0 => Some("from the left"),
            _ => None,
        };
        let a = if a.fract() == 0.0 {
            format!("{a:.0}")
        } else {
            format!("{a}")
        };
        match name {
            Some(n) => format!("from {a}° ({n})"),
            None => format!("from {a}°"),
        }
    }
}

impl<'de> Deserialize<'de> for WindDirection {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Number(f64),
            Text(String),
        }
        match Raw::deserialize(d)? {
            Raw::Number(a) => Self::from_number(a).map_err(serde::de::Error::custom),
            Raw::Text(t) => Self::parse(&t).map_err(serde::de::Error::custom),
        }
    }
}

/// How a wire takes a wind at an angle to it (Criteria › Calculation
/// options; owner 2026-11-15: the projected area by default, the
/// cross-flow principle as the option). Both give the full \\( qD \\) for
/// a wind square to the wire.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObliqueLaw {
    /// The projected area (the default): the force normal to the wire
    /// \\( q D \sin\theta \\) — SIA 261:2014 Table 74 for round bars, LeV
    /// Anhang 15 Ziff. 1.4 in words. The larger of the two (on the safe
    /// side).
    #[default]
    Projected,
    /// The cross-flow (independence) principle: the force normal to the
    /// wire that of a cross flow of the wind's normal speed,
    /// \\( q D \sin^2\theta \\) — the measurements of Relf & Powell
    /// (1917), as reported by Divaret et al. (2014). Lighter.
    CrossFlow,
}

impl ObliqueLaw {
    /// Every choice.
    pub const ALL: [Self; 2] = [Self::Projected, Self::CrossFlow];

    /// The default.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// As written in files.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Projected => "projected",
            Self::CrossFlow => "cross_flow",
        }
    }

    /// From its name in files.
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|l| l.as_str() == text)
    }

    /// In words.
    pub fn words(self) -> &'static str {
        match self {
            Self::Projected => "projected area (q·D·sin θ)",
            Self::CrossFlow => "cross-flow principle (q·D·sin² θ)",
        }
    }
}

/// The force per metre of a wind at an angle on a straight piece of wire,
/// N/m: `qd` the pressure times the (iced) diameter \\( qD \\), N/m;
/// `towards` where the wind blows (a unit vector); `tangent` the piece's
/// direction (any length, its sign does not matter; 0: no force).
///
/// With \\( \mathbf w_n = \hat w - (\hat w\cdot\hat t)\,\hat t \\) the
/// wind's part normal to the wire (\\( |\mathbf w_n| = \sin\theta \\), θ
/// between the wind and the wire):
///
/// - [`ObliqueLaw::Projected`]: \\( qD\,\mathbf w_n \\);
/// - [`ObliqueLaw::CrossFlow`]: \\( qD\,|\mathbf w_n|\,\mathbf w_n \\);
/// - `friction` \\( c_t \\) (none: no force along the wire): plus
///   \\( c_t\,qD\,(\hat w\cdot\hat t)\,|\hat w\cdot\hat t|\,\hat t \\),
///   along the wire with the wind's part along it — the dynamic pressure
///   of the wind's speed along the wire, \\( q\cos^2\theta \\), on the
///   diameter, times the user's coefficient.
///
/// A horizontal wind on an inclined wire has a vertical part normal to it
/// (down for a wind blowing up the slope, up for one blowing down).
pub fn oblique_wire_force(
    qd: f64,
    towards: [f64; 3],
    tangent: [f64; 3],
    law: ObliqueLaw,
    friction: Option<f64>,
) -> [f64; 3] {
    let n = tangent.iter().map(|x| x * x).sum::<f64>().sqrt();
    if n == 0.0 || qd == 0.0 {
        return [0.0; 3];
    }
    let t = tangent.map(|x| x / n);
    let along: f64 = (0..3).map(|k| towards[k] * t[k]).sum();
    let normal = [0, 1, 2].map(|k| towards[k] - along * t[k]);
    let scale = match law {
        ObliqueLaw::Projected => qd,
        ObliqueLaw::CrossFlow => qd * normal.iter().map(|x| x * x).sum::<f64>().sqrt(),
    };
    let skin = friction.map_or(0.0, |c| c * qd * along * along.abs());
    [0, 1, 2].map(|k| normal[k] * scale + t[k] * skin)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() <= tol, "{a} vs {b}");
    }

    /// The convention: 0° from ahead blows back, 90° from the right blows
    /// to the left, clockwise; the names exact.
    #[test]
    fn the_convention() {
        assert_eq!(WindDirection::Right.towards(), [-1.0, 0.0]);
        assert_eq!(WindDirection::Back.towards(), [0.0, 1.0]);
        let t = WindDirection::Angle(90.0).towards();
        close(t[0], -1.0, 1e-15);
        close(t[1], 0.0, 1e-15);
        let t = WindDirection::Angle(180.0).towards();
        close(t[1], 1.0, 1e-15);
        // 45° from ahead right blows back and to the left.
        let t = WindDirection::Angle(45.0).towards();
        assert!(t[0] < 0.0 && t[1] < 0.0);
        // In the project's axes on a line running north: from the right
        // (east) it blows west.
        let w = WindDirection::Right.towards_in([0.0, 1.0]);
        assert_eq!(w, [-1.0, 0.0]);
        // A line running east: from ahead (east) it blows west.
        let w = WindDirection::Ahead.towards_in([1.0, 0.0]);
        assert_eq!(w, [-1.0, 0.0]);
        assert_eq!(
            WindDirection::parse("back_left"),
            Ok(WindDirection::Angle(225.0))
        );
        assert!(WindDirection::parse("sideways").is_err());
    }

    /// θ = 90° − θ_old, and an old angle keeps the old vector bit for bit.
    #[test]
    fn old_angles_convert() {
        for old in [0.0_f64, 30.0, 37.3, 0.1, 90.0, 200.0, 270.0] {
            let d = WindDirection::OldAngle(old);
            close(d.degrees(), (90.0 - old).rem_euclid(360.0), 1e-12);
            let r = old.to_radians();
            assert_eq!(d.towards(), [-r.cos(), -r.sin()]);
            let new = WindDirection::Angle(d.degrees()).towards();
            close(new[0], d.towards()[0], 1e-12);
            close(new[1], d.towards()[1], 1e-12);
            assert_eq!(d.old_degrees(), old);
        }
        assert_eq!(WindDirection::Angle(60.0).old_degrees(), 30.0);
        assert_eq!(
            WindDirection::OldAngle(0.0).across_side(),
            Some(crate::WindSide::Right)
        );
    }

    /// The plan's hand case: 500 Pa on 30 mm, 300 m: 4500 N square to the
    /// wire; at 45° 3182 N (projected) and 2250 N (cross-flow); along it
    /// none, unless friction.
    #[test]
    fn the_two_laws_and_friction() {
        let qd = 500.0 * 0.030;
        let wire = [1.0, 0.0, 0.0];
        let s = std::f64::consts::FRAC_1_SQRT_2;
        let total = |w: [f64; 3], law, f| {
            let v = oblique_wire_force(qd, w, wire, law, f);
            v.iter().map(|x| x * x).sum::<f64>().sqrt() * 300.0
        };
        close(
            total([0.0, 1.0, 0.0], ObliqueLaw::Projected, None),
            4500.0,
            1e-9,
        );
        close(
            total([0.0, 1.0, 0.0], ObliqueLaw::CrossFlow, None),
            4500.0,
            1e-9,
        );
        close(
            total([s, s, 0.0], ObliqueLaw::Projected, None),
            3181.98,
            0.01,
        );
        close(
            total([s, s, 0.0], ObliqueLaw::CrossFlow, None),
            2250.0,
            1e-9,
        );
        close(
            total([1.0, 0.0, 0.0], ObliqueLaw::Projected, None),
            0.0,
            1e-12,
        );
        // Friction: along the wire with the wind, c·qD·cos²θ.
        let f = oblique_wire_force(qd, [1.0, 0.0, 0.0], wire, ObliqueLaw::Projected, Some(0.1));
        close(f[0], 0.1 * qd, 1e-15);
        let f = oblique_wire_force(
            qd,
            [-s, s, 0.0],
            [-2.0, 0.0, 0.0],
            ObliqueLaw::CrossFlow,
            Some(0.1),
        );
        close(f[0], -0.1 * qd * 0.5, 1e-15);
        close(f[1], qd * s * s, 1e-15);
    }

    /// Weather file version 2: a case's direction; across the line read as
    /// its side (the same case as before); compared, summarised.
    #[test]
    fn weather_cases_with_a_direction() {
        let set = crate::WeatherCases::parse(
            "format = \"ohl-weather-cases\"\nversion = 2\n\
             [[case]]\nname = \"forward\"\ntemperature = \"10 °C\"\nwind_pressure = \"300 Pa\"\n\
             wind_direction = 180\n\
             [[case]]\nname = \"left\"\ntemperature = \"10 °C\"\nwind_pressure = \"300 Pa\"\n\
             wind_direction = \"left\"\n\
             [[case]]\nname = \"old left\"\ntemperature = \"10 °C\"\nwind_pressure = \"300 Pa\"\n\
             wind_from = \"left\"\n\
             [[case]]\nname = \"calm ahead\"\ntemperature = \"10 °C\"\nwind_direction = 0\n",
        )
        .unwrap();
        let c = &set.cases;
        assert_eq!(c[0].oblique(), Some(WindDirection::Angle(180.0)));
        assert_eq!(
            c[0].summary(),
            "10.0 °C, wind 300 Pa from 180° (from behind: blowing ahead)"
        );
        assert_eq!(c[1].wind_direction, None);
        assert_eq!(c[1].wind_from, crate::WindSide::Left);
        assert!(c[1].same_physics(&c[2]));
        assert!(!c[0].same_physics(&c[2]));
        // No wind: no direction to take.
        assert_eq!(c[3].oblique(), None);
        // Without wind the direction does not matter.
        assert!(c[3].same_physics(&crate::WeatherCase::still("x", c[3].temperature)));
        // A newer version refused.
        assert!(crate::WeatherCases::parse("version = 3\n").is_err());
    }

    /// A span rising 30°, wind along it: cross-flow 3.75 N/m normal to it,
    /// 3.25 N/m of it vertical — down for a wind blowing uphill.
    #[test]
    fn vertical_part_on_an_inclined_span() {
        let qd = 15.0;
        let r = 30f64.to_radians();
        let wire = [r.cos(), 0.0, r.sin()];
        let up = oblique_wire_force(qd, [1.0, 0.0, 0.0], wire, ObliqueLaw::CrossFlow, None);
        let n = up.iter().map(|x| x * x).sum::<f64>().sqrt();
        close(n, 3.75, 1e-12);
        close(up[2], -3.25, 0.01);
        let down = oblique_wire_force(qd, [-1.0, 0.0, 0.0], wire, ObliqueLaw::CrossFlow, None);
        close(down[2], 3.25, 0.01);
    }
}
