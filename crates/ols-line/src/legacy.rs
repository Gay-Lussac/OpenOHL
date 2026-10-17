// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Forms older files gave, read into the current model and not written:
//! point loads and spread loads of a section before points and objects
//! ([`crate::Section::add_point_load`], [`crate::Section::add_spread_load`]).

use serde::{Deserialize, Serialize};

use crate::objects::{LineObject, Shape};
use crate::{SpanPlace, is_zero};

/// A load spread along part of a span (e.g. a heated stretch, a cable
/// tray, a wire carrying another): downward, N/m of the conductor between
/// two places, on the whole bundle.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpreadLoad {
    /// Span index (0 = first span of the section).
    pub span: usize,
    /// Where it starts.
    pub start: SpanPlace,
    /// Where it ends.
    pub end: SpanPlace,
    /// Downward load, N per metre of conductor (the whole bundle's).
    pub load: f64,
    /// Label.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
}

/// A point load in a span: its weight, and as a body of some size (a
/// marker ball, an obstruction light) the wind on it and the ice it takes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpanLoad {
    /// Span index (0 = first span of the section).
    pub span: usize,
    /// Horizontal fraction of the span from its back point, 0–1.
    #[serde(default)]
    pub fraction: f64,
    /// Horizontal distance from the span's back point, m: given, it is
    /// the place (the fraction is not used).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance: Option<f64>,
    /// Station of the line, m: given, it is the place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub station: Option<f64>,
    /// Downward force, N: on the whole bundle (shared between its
    /// subconductors).
    pub vertical: f64,
    /// Label, e.g. `"marker ball"`.
    #[serde(default)]
    pub label: String,
    /// Diameter of the body, m, taken as a sphere: its wind area
    /// \\( \pi D^2/4 \\) and its surface for the ice. 0: a point, no wind
    /// nor ice (unless given as areas).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub diameter: f64,
    /// Drag coefficient of the body (none: 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drag: Option<f64>,
    /// Wind area, m², instead of the sphere's (bare, without ice).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind_area: Option<f64>,
    /// Surface taking the ice, m², instead of the sphere's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ice_area: Option<f64>,
}

impl SpanLoad {
    /// Its place along the span.
    pub fn place(&self) -> SpanPlace {
        SpanPlace::from_parts(self.fraction, self.distance, self.station)
    }
}

impl SpanLoad {
    /// The object it is: a body, a sphere of its diameter (a point
    /// without one), or of its given areas.
    pub fn object(&self) -> LineObject {
        use std::f64::consts::PI;
        let d = self.diameter.max(0.0);
        let shape = match (self.wind_area, self.ice_area) {
            (None, None) if d > 0.0 => Shape::Sphere { diameter: d },
            (None, None) => Shape::Point,
            (w, i) => Shape::Areas {
                wind_area: w.unwrap_or(PI / 4.0 * d * d),
                ice_area: i.unwrap_or(PI * d * d),
            },
        };
        LineObject {
            drag: self.drag,
            ..LineObject::body(self.vertical, shape)
        }
    }
}
