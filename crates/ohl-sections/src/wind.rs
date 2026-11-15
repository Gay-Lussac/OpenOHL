// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Wind factors for structures given as **data** (the `[wind]` table of a
//! section library): a drag factor per profile shape, a drag factor of a
//! lattice face by its solidity ratio, a shielding factor of a face
//! sheltered by another, by the sheltering face's solidity ratio (and the
//! spacing ratio), and (version 2, 2026-11-15) an inclination factor per
//! shape: the factor on the force of a member inclined to the wind. They
//! come from a design-code plugin (its parameter file) or from the user.
//! Without a drag factor, a member takes the built-in default of its shape
//! (`ohl_structures_fe::wind_law`), without an inclination factor the
//! built-in inclination law; nothing is shielded without shielding factors.
//!
//! ```toml
//! [wind]
//! source = "my parameter file"        # shown with the results
//! drag = { equal_angle = 2.0, circular_hollow = 1.2 }   # per shape
//! member_law = "inclination"          # default; "sin2": the cross-flow law
//!                                     # (comparison only, version 2)
//!
//! [wind.inclination.equal_angle]       # version 2: per shape
//! angle = [0, 30, 60]                  # β, degrees: wind to the member's normal
//! factor = [1.0, 0.9, 0.7]             # on the force normal to the member
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
pub const SHAPE_NAMES: [&str; 13] = [
    "equal_angle",
    "unequal_angle",
    "channel",
    "i_section",
    "rectangular_hollow",
    "circular_hollow",
    "round_bar",
    "polygonal_hollow",
    "general",
    "double_angle",
    "star_angle",
    "cross_angle",
    "double_channel",
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

/// The solidity ratios at which a function linear between the table's
/// `xs` takes its extremes on `lo..=hi`: the two ends and the table's
/// points between them.
fn candidates(xs: &[f64], lo: f64, hi: f64) -> Vec<f64> {
    let mut out = vec![lo, hi];
    out.extend(xs.iter().copied().filter(|x| *x > lo && *x < hi));
    out
}

impl Curve {
    /// The factor at solidity ratio `phi`; none outside the table.
    pub fn at(&self, phi: f64) -> Option<f64> {
        interpolate(&self.solidity, &self.factor, phi)
    }

    /// The largest factor for a solidity ratio anywhere in `lo..=hi`
    /// (a face whose ratio is known only within bounds: on the safe
    /// side); none when part of it is outside the table.
    pub fn largest_in(&self, lo: f64, hi: f64) -> Option<f64> {
        let (lo, hi) = (lo.min(hi), lo.max(hi));
        candidates(&self.solidity, lo, hi)
            .into_iter()
            .map(|x| self.at(x))
            .try_fold(f64::NEG_INFINITY, |m, c| c.map(|c| m.max(c)))
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

    /// The largest factor (the least shelter) for a solidity ratio of
    /// the face in front anywhere in `lo..=hi` at spacing ratio
    /// `spacing` (on the safe side when the ratio is known only within
    /// bounds); none when part of it is outside the table. At a fixed
    /// spacing the factor is linear between the table's solidity
    /// ratios, so its largest value is at an end or one of them.
    pub fn largest_in(&self, lo: f64, hi: f64, spacing: f64) -> Option<f64> {
        let (lo, hi) = (lo.min(hi), lo.max(hi));
        candidates(&self.solidity, lo, hi)
            .into_iter()
            .map(|x| self.at(x, spacing))
            .try_fold(f64::NEG_INFINITY, |m, c| c.map(|c| m.max(c)))
    }
}

/// The section library format version of `[wind] inclination` and
/// `member_law` (2026-11-15); a file is written at it only when it gives
/// one of them.
pub const INCLINATION_VERSION: u32 = 2;

/// The law of the force on a member inclined to the wind (`[wind]
/// member_law`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MemberLaw {
    /// `"inclination"` (the default, 2026-11-15): the force normal to the
    /// member is the force of the member normal to the wind times an
    /// inclination factor k(β) — the shape's curve given in
    /// [`StructureWind::inclination`], else the built-in one.
    #[default]
    Inclination,
    /// `"sin2"`: the cross-flow law, the force normal to the member ∝
    /// sin²θ (θ between the wind and the member) — the member wind before
    /// 2026-11-15, kept for comparison; below a code's inclined-member
    /// factors, not on the safe side.
    Sin2,
}

impl MemberLaw {
    /// Name in files.
    pub fn as_str(self) -> &'static str {
        match self {
            MemberLaw::Inclination => "inclination",
            MemberLaw::Sin2 => "sin2",
        }
    }

    /// The law of a name in files.
    pub fn parse(name: &str) -> Result<Self, String> {
        match name.trim() {
            "inclination" => Ok(MemberLaw::Inclination),
            "sin2" => Ok(MemberLaw::Sin2),
            other => Err(format!(
                "member_law \"{other}\": inclination (default: the force normal to an inclined \
                 member times an inclination factor) or sin2 (the cross-flow law sin²θ, for \
                 comparison only: not on the safe side)"
            )),
        }
    }
}

/// An inclination factor by β, the angle between the wind and the
/// member's normal (90° − θ, θ between the wind and the member), linear
/// between the points; beyond the last angle β_n tapered with the
/// projected area, f_n · cos β / cos β_n (owner 2026-11-15: an
/// assumption, continuous there, 0 along the wind).
#[derive(Debug, Clone, PartialEq)]
pub struct Inclination {
    /// β, degrees, rising from 0, at most 90.
    pub angle: Vec<f64>,
    /// The factor at each, 0 or above (1 at β = 0 for a code's curve).
    pub factor: Vec<f64>,
}

impl Inclination {
    /// The factor at `beta` degrees (0–90): linear between the table's
    /// angles; beyond the last one β_n, its factor f_n × cos β / cos β_n.
    pub fn at(&self, beta: f64) -> f64 {
        let last = self.angle.len() - 1;
        let (b_n, f_n) = (self.angle[last], self.factor[last]);
        if beta >= b_n {
            let c_n = b_n.to_radians().cos();
            return if c_n > 0.0 {
                f_n * beta.min(90.0).to_radians().cos().max(0.0) / c_n
            } else {
                f_n
            };
        }
        interpolate(&self.angle, &self.factor, beta.max(0.0)).unwrap_or(self.factor[0])
    }

    /// Whether `beta` is beyond the table's last angle (its factor
    /// tapered).
    pub fn beyond(&self, beta: f64) -> bool {
        self.angle.last().is_some_and(|a| beta > *a + 1e-9)
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
    /// Inclination factor per profile shape (version 2).
    pub inclination: BTreeMap<String, Inclination>,
    /// The law of the force on an inclined member (version 2; default
    /// [`MemberLaw::Inclination`]).
    pub member_law: MemberLaw,
}

impl StructureWind {
    /// The drag factor of shape `shape` (its name in files), if given.
    pub fn drag_of(&self, shape: &str) -> Option<f64> {
        self.drag.get(shape).copied()
    }

    /// The inclination curve of shape `shape`, if given.
    pub fn inclination_of(&self, shape: &str) -> Option<&Inclination> {
        self.inclination.get(shape)
    }

    /// Nothing given.
    pub fn is_empty(&self) -> bool {
        self.drag.is_empty()
            && self.face_drag.is_none()
            && self.shielding.is_none()
            && self.inclination.is_empty()
            && self.member_law == MemberLaw::Inclination
    }

    /// Its inclination curves and member law in words, each after ", "
    /// (none given: empty), for the summaries of a table.
    pub fn inclination_words(&self) -> String {
        let mut s = String::new();
        if !self.inclination.is_empty() {
            s.push_str(&format!(
                ", inclination for {} shape(s)",
                self.inclination.len()
            ));
        }
        if self.member_law == MemberLaw::Sin2 {
            s.push_str(", the sin²θ law (comparison only, NOT ON THE SAFE SIDE)");
        }
        s
    }

    /// Whether it uses a key of version 2 ([`INCLINATION_VERSION`]):
    /// inclination curves or a member law.
    pub fn needs_version_2(&self) -> bool {
        !self.inclination.is_empty() || self.member_law != MemberLaw::Inclination
    }

    /// Where the values come from, for notes: the source, else "the
    /// section library".
    pub fn source_name(&self) -> &str {
        if self.source.trim().is_empty() {
            "the cross-section library's [wind]"
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
    #[serde(default)]
    inclination: BTreeMap<String, InclinationFile>,
    #[serde(default)]
    member_law: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InclinationFile {
    angle: Vec<f64>,
    factor: Vec<f64>,
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
        let mut inclination = BTreeMap::new();
        for (shape, c) in self.inclination {
            if !SHAPE_NAMES.contains(&shape.as_str()) {
                return Err(format!(
                    "inclination: \"{shape}\" is not a shape ({})",
                    SHAPE_NAMES.join(", ")
                ));
            }
            let at = |e: String| format!("inclination of {shape}: {e}");
            axis("angle", &c.angle, false).map_err(at)?;
            if c.angle[0] != 0.0 || c.angle.iter().any(|a| *a > 90.0) {
                return Err(at("angle starts at 0 and stays within 90 degrees".into()));
            }
            if c.factor.len() != c.angle.len() {
                return Err(at("one factor per angle".into()));
            }
            if c.factor.iter().any(|f| !(f.is_finite() && *f >= 0.0)) {
                return Err(at("factors must be 0 or above".into()));
            }
            inclination.insert(
                shape,
                Inclination {
                    angle: c.angle,
                    factor: c.factor,
                },
            );
        }
        let member_law = match &self.member_law {
            None => MemberLaw::default(),
            Some(m) => MemberLaw::parse(m)?,
        };
        Ok(StructureWind {
            source: self.source,
            drag: self.drag,
            face_drag,
            shielding,
            inclination,
            member_law,
        })
    }
}
