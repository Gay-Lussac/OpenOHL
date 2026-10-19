// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Wind factors for structures given as **data** (the `[wind]` table of a
//! section library): a drag factor per profile shape, a drag factor of a
//! lattice face by its solidity ratio, and a shielding factor of a face
//! sheltered by another, by the sheltering face's solidity ratio (and the
//! spacing ratio). OpenOHL ships none of these values: they come from a
//! design-code plugin (its parameter file) or from the user. Without them
//! the member wind keeps its one generic drag factor and no shielding.
//!
//! ```toml
//! [wind]
//! source = "my parameter file"        # shown with the results
//! drag = { equal_angle = 2.0, circular_hollow = 1.2 }   # per shape
//!
//! [wind.face_drag]                     # a face's drag by its solidity
//! solidity = [0.1, 0.3, 0.5]
//! factor = [3.0, 2.6, 2.2]
//!
//! [wind.shielding]                     # factor on a sheltered face
//! solidity = [0.1, 0.3, 0.5]           # of the face in front
//! spacing = [0.5, 2.0]                 # optional: distance / breadth
//! factor = [[0.9, 0.7, 0.5], [1.0, 0.8, 0.6]]   # a row per spacing
//! ```
//! (all numbers above invented).

use std::collections::BTreeMap;

use serde::Deserialize;

/// The shape names a `drag` key may take (the section shapes).
pub const SHAPE_NAMES: [&str; 9] = [
    "equal_angle",
    "unequal_angle",
    "channel",
    "i_section",
    "rectangular_hollow",
    "circular_hollow",
    "round_bar",
    "polygonal_hollow",
    "general",
];

/// A factor by solidity ratio, linear between the points; none outside
/// them.
#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
    /// Solidity ratios, rising, 0–1.
    pub solidity: Vec<f64>,
    /// The factor at each.
    pub factor: Vec<f64>,
}

/// Linear interpolation on rising `xs`; none outside them.
fn interpolate(xs: &[f64], ys: &[f64], x: f64) -> Option<f64> {
    const EDGE: f64 = 1e-12;
    let (first, last) = (*xs.first()?, *xs.last()?);
    if !(x >= first - EDGE && x <= last + EDGE) {
        return None;
    }
    let x = x.clamp(first, last);
    let k = xs.windows(2).position(|w| x <= w[1]).unwrap_or(0);
    let (x0, x1) = (xs[k], xs[k + 1]);
    let t = if x1 > x0 { (x - x0) / (x1 - x0) } else { 0.0 };
    Some(ys[k] + t * (ys[k + 1] - ys[k]))
}

impl Curve {
    /// The factor at solidity ratio `phi`; none outside the table.
    pub fn at(&self, phi: f64) -> Option<f64> {
        interpolate(&self.solidity, &self.factor, phi)
    }
}

/// The shielding factor of a sheltered face by the solidity ratio of the
/// face in front of it and, when given, the spacing ratio (the distance
/// between the two faces over the front face's breadth).
#[derive(Debug, Clone, PartialEq)]
pub struct Shielding {
    /// Solidity ratios of the face in front, rising, 0–1.
    pub solidity: Vec<f64>,
    /// Spacing ratios, rising (none: the factor does not depend on it).
    pub spacing: Option<Vec<f64>>,
    /// One row per spacing ratio (one row without), each a factor per
    /// solidity ratio, 0–1.
    pub factor: Vec<Vec<f64>>,
}

impl Shielding {
    /// The factor for solidity ratio `phi` and spacing ratio `spacing`
    /// (bilinear); none outside the table.
    pub fn at(&self, phi: f64, spacing: f64) -> Option<f64> {
        let rows: Vec<f64> = self
            .factor
            .iter()
            .map(|row| interpolate(&self.solidity, row, phi))
            .collect::<Option<_>>()?;
        match &self.spacing {
            None => rows.first().copied(),
            Some(s) => interpolate(s, &rows, spacing),
        }
    }
}

/// Wind factors of a section library's `[wind]` table.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StructureWind {
    /// Where the values come from (free text, shown with the results).
    pub source: String,
    /// Drag factor per profile shape (`equal_angle`, …).
    pub drag: BTreeMap<String, f64>,
    /// Drag factor of a lattice face by its solidity ratio (for the
    /// members of a face, instead of their shape's).
    pub face_drag: Option<Curve>,
    /// Shielding factor of a sheltered face.
    pub shielding: Option<Shielding>,
}

impl StructureWind {
    /// The drag factor of shape `shape` (its name in files), if given.
    pub fn drag_of(&self, shape: &str) -> Option<f64> {
        self.drag.get(shape).copied()
    }

    /// Nothing given.
    pub fn is_empty(&self) -> bool {
        self.drag.is_empty() && self.face_drag.is_none() && self.shielding.is_none()
    }

    /// Where the values come from, for notes: the source, else "the
    /// section library".
    pub fn source_name(&self) -> &str {
        if self.source.trim().is_empty() {
            "the section library's [wind]"
        } else {
            &self.source
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WindFile {
    #[serde(default)]
    source: String,
    #[serde(default)]
    drag: BTreeMap<String, f64>,
    #[serde(default)]
    face_drag: Option<CurveFile>,
    #[serde(default)]
    shielding: Option<ShieldingFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CurveFile {
    solidity: Vec<f64>,
    factor: Vec<f64>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Rows {
    Flat(Vec<f64>),
    Rows(Vec<Vec<f64>>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ShieldingFile {
    solidity: Vec<f64>,
    #[serde(default)]
    spacing: Option<Vec<f64>>,
    factor: Rows,
}

/// Rising values, at least two, finite; `unit`: each within 0–1.
fn axis(name: &str, xs: &[f64], unit: bool) -> Result<(), String> {
    if xs.len() < 2 {
        return Err(format!("{name} needs at least two values"));
    }
    if xs
        .iter()
        .any(|x| !x.is_finite() || *x < 0.0 || (unit && *x > 1.0))
    {
        return Err(format!(
            "{name} values must be {}",
            if unit {
                "between 0 and 1"
            } else {
                "0 or above"
            }
        ));
    }
    if xs.windows(2).any(|w| w[1] <= w[0]) {
        return Err(format!("{name} values must rise"));
    }
    Ok(())
}

impl WindFile {
    pub(crate) fn resolve(self) -> Result<StructureWind, String> {
        for (shape, d) in &self.drag {
            if !SHAPE_NAMES.contains(&shape.as_str()) {
                return Err(format!(
                    "drag: \"{shape}\" is not a shape ({})",
                    SHAPE_NAMES.join(", ")
                ));
            }
            if !(d.is_finite() && *d > 0.0) {
                return Err(format!("drag of {shape} must be above 0"));
            }
        }
        let face_drag = match self.face_drag {
            None => None,
            Some(c) => {
                axis("face_drag: solidity", &c.solidity, true)?;
                if c.factor.len() != c.solidity.len() {
                    return Err("face_drag: one factor per solidity value".into());
                }
                if c.factor.iter().any(|f| !(f.is_finite() && *f > 0.0)) {
                    return Err("face_drag: factors must be above 0".into());
                }
                Some(Curve {
                    solidity: c.solidity,
                    factor: c.factor,
                })
            }
        };
        let shielding = match self.shielding {
            None => None,
            Some(s) => {
                axis("shielding: solidity", &s.solidity, true)?;
                let factor = match (&s.spacing, s.factor) {
                    (None, Rows::Flat(f)) => vec![f],
                    (None, Rows::Rows(_)) => {
                        return Err(
                            "shielding: rows of factors need spacing (one row per spacing value)"
                                .into(),
                        );
                    }
                    (Some(sp), Rows::Rows(r)) => {
                        axis("shielding: spacing", sp, false)?;
                        if r.len() != sp.len() {
                            return Err("shielding: one row of factors per spacing value".into());
                        }
                        r
                    }
                    (Some(_), Rows::Flat(_)) => {
                        return Err(
                            "shielding: with spacing, factor is a row per spacing value".into()
                        );
                    }
                };
                if factor.iter().any(|row| row.len() != s.solidity.len()) {
                    return Err("shielding: one factor per solidity value in each row".into());
                }
                if factor
                    .iter()
                    .flatten()
                    .any(|f| !(f.is_finite() && (0.0..=1.0).contains(f)))
                {
                    return Err("shielding: factors must be between 0 and 1".into());
                }
                Some(Shielding {
                    solidity: s.solidity,
                    spacing: s.spacing,
                    factor,
                })
            }
        };
        Ok(StructureWind {
            source: self.source,
            drag: self.drag,
            face_drag,
            shielding,
        })
    }
}
