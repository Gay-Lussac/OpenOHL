// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A wind at an angle to the line on the wires (owner 2026-11-15): where it
//! blows at each place, and the calculation options of its law
//! ([`WireWindOptions`]).
//!
//! A weather case without a direction, or with one across the line
//! (`right`, `left`, 90°, 270°), keeps the across rule — every span the
//! full pressure square to itself — and none of this is used: those cases
//! give the same results as before, bit for bit. A case with another
//! direction ([`ohl_weather::WeatherCase::oblique`]) is one wind, blowing
//! from that direction relative to the line's direction at each place
//! ([`crate::Line::direction_at`]: the route's segment, the bisector at an
//! angle point); each piece of wire takes the part normal to it by the
//! law of [`WireWindOptions::law`], plus a force along it when
//! [`WireWindOptions::friction`] is given.

use nalgebra::Vector3;
use ohl_weather::{ObliqueLaw, WeatherCase};
use serde::{Deserialize, Serialize};

/// The calculation options of a wind at an angle on the wires
/// (Criteria › Calculation options; line file version 20 when not the
/// defaults). The defaults change nothing in a case without a direction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireWindOptions {
    /// The force normal to a wire at an angle to the wind: the projected
    /// area (\\( qD\sin\theta \\), the default, on the safe side) or the
    /// cross-flow principle (\\( qD\sin^2\theta \\)).
    #[serde(default, skip_serializing_if = "ObliqueLaw::is_default")]
    pub law: ObliqueLaw,
    /// The skin friction along the wire (owner 2026-11-15: "an option
    /// for the friction on the wire"): its coefficient \\( c_t \\), the
    /// force along the wire per metre \\( c_t\,qD\cos^2\theta \\) with the
    /// wind's part along it ([`ohl_weather::oblique_wire_force`]). None
    /// (the default): no force along the wire. No value is given by the
    /// program: the user gives it from a source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub friction: Option<Friction>,
}

impl WireWindOptions {
    /// The defaults.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// In words, for notes.
    pub fn words(&self) -> String {
        match self.friction {
            Some(f) => format!(
                "{}; friction along the wire c_t = {} (q·D·c_t·cos² θ)",
                self.law.words(),
                f.0
            ),
            None => format!("{}; no friction along the wire", self.law.words()),
        }
    }
}

/// The skin friction coefficient along a wire, \\( c_t \\) (on the
/// diameter, of the dynamic pressure of the wind's speed along the wire):
/// 0 or more, finite.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Friction(pub f64);

// Finite by construction (refused otherwise when read).
impl Eq for Friction {}

impl Friction {
    /// The coefficient, checked: finite and 0 or more.
    pub fn new(c: f64) -> Result<Self, String> {
        if c.is_finite() && c >= 0.0 {
            Ok(Self(c))
        } else {
            Err(format!(
                "wire friction coefficient {c}: a number, 0 or more"
            ))
        }
    }
}

impl<'de> Deserialize<'de> for Friction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::new(f64::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

/// One wind at a place: where it blows and the law it acts by.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaceWind {
    /// Where it blows, a horizontal unit vector (project axes).
    pub towards: Vector3<f64>,
    /// The law on wires at an angle to it.
    pub law: ObliqueLaw,
    /// The friction coefficient along wires (none: no force along them).
    pub friction: Option<f64>,
}

impl PlaceWind {
    /// The wind of `case` at a place where the line runs along `ahead`
    /// (a unit vector in plan): none for the across rule
    /// ([`WeatherCase::oblique`]).
    pub fn of(case: &WeatherCase, ahead: [f64; 2], options: &WireWindOptions) -> Option<Self> {
        let [x, y] = case.towards_at(ahead)?;
        Some(Self {
            towards: Vector3::new(x, y, 0.0),
            law: options.law,
            friction: options.friction.map(|f| f.0),
        })
    }

    /// Its force per metre on a straight piece of wire along `d` whose
    /// pressure times diameter is `qd` (N/m): the part normal to the wire
    /// by the law, and the friction along it
    /// ([`ohl_weather::oblique_wire_force`]).
    pub fn on_wire(&self, qd: f64, d: Vector3<f64>) -> Vector3<f64> {
        let f = ohl_weather::oblique_wire_force(
            qd,
            [self.towards.x, self.towards.y, self.towards.z],
            [d.x, d.y, d.z],
            self.law,
            self.friction,
        );
        Vector3::new(f[0], f[1], f[2])
    }
}

/// The line's direction at each span and point of a section, for the
/// cases with a wind at an angle: per span at the middle between its
/// structures, per point at its structure (the bisector at an angle
/// point). Without routes, the section's first-to-last direction.
#[derive(Debug, Clone)]
pub(crate) struct WireWind {
    spans: Vec<[f64; 2]>,
    points: Vec<[f64; 2]>,
    options: WireWindOptions,
}

impl WireWind {
    /// For `section` of `line`, its clamps at `clamps` (for the fallback
    /// direction and points without a structure).
    pub(crate) fn of(
        line: &crate::Line,
        section: &crate::Section,
        clamps: &[Vector3<f64>],
    ) -> Self {
        let views = line.alignment_views();
        let first_to_last = match (clamps.first(), clamps.last()) {
            (Some(a), Some(b)) if (b - a).xy().norm() > 0.0 => {
                let h = (b - a).xy() / (b - a).xy().norm();
                [h.x, h.y]
            }
            _ => [0.0, 1.0],
        };
        let place = |k: usize| -> [f64; 2] {
            section
                .points
                .get(k)
                .and_then(|p| line.structure(&p.structure))
                .map(|s| [s.position[0], s.position[1]])
                .or_else(|| clamps.get(k).map(|c| [c.x, c.y]))
                .unwrap_or([0.0, 0.0])
        };
        let along = |at: [f64; 2]| {
            line.bisected_direction_at_in(&views, at)
                .unwrap_or(first_to_last)
        };
        let n = section.points.len();
        Self {
            points: (0..n).map(|k| along(place(k))).collect(),
            spans: (0..n.saturating_sub(1))
                .map(|s| {
                    let (a, b) = (place(s), place(s + 1));
                    along([0.5 * (a[0] + b[0]), 0.5 * (a[1] + b[1])])
                })
                .collect(),
            options: line.solver.wind,
        }
    }

    /// The wind of `case` on span `s` (none: the across rule).
    pub(crate) fn span(&self, case: &WeatherCase, s: usize) -> Option<PlaceWind> {
        PlaceWind::of(case, *self.spans.get(s)?, &self.options)
    }

    /// The wind of `case` at point `k` (none: the across rule).
    pub(crate) fn point(&self, case: &WeatherCase, k: usize) -> Option<PlaceWind> {
        PlaceWind::of(case, *self.points.get(k)?, &self.options)
    }
}
