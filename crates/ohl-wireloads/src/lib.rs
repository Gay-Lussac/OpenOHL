// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Additional loads on wires (DESIGN_GOALS §3).
//!
//! - **Point loads** (N): aircraft markers, spacers, dampers, bird diverters,
//!   a line cart or worker, …
//! - **Distributed loads** (N/m of cable) over the whole span or part of it:
//!   partial ice or ice shedding, lashed cables, de-icing equipment, …
//!
//! Each load is **permanent** (empty `cases`) or applies only in the named
//! load cases. Positions are horizontal distances along the span, given from
//! either support or as a fraction of the span.
//!
//! Loads have vertical, transverse and longitudinal components. Sag-tension
//! in the vertical plane uses the vertical components; transverse and
//! longitudinal components need the 3D finite-element wire model (roadmap
//! Phase 4) and are rejected by [`resolve_vertical`] until then rather than
//! silently ignored.
//!
//! ```toml
//! [[point]]
//! label = "aircraft marker"
//! position = { from_left = "150 m" }
//! vertical = "70 N"
//!
//! [[distributed]]
//! label = "partial ice"
//! from = { fraction = 0.0 }
//! to = { fraction = 0.5 }
//! vertical = "12 N/m"
//! cases = ["ice shedding"]
//! ```

use std::fmt;

use ohl_core::units::{Force, ForcePerLength, Length};
use serde::Deserialize;

/// Horizontal position along a span.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Position {
    /// Distance from the left (back) support.
    FromLeft(Length),
    /// Distance from the right (ahead) support.
    FromRight(Length),
    /// Fraction of the span from the left support, 0 to 1.
    Fraction(f64),
}

impl Position {
    /// Horizontal distance from the left support for a span of length `span`.
    pub fn resolve(&self, span: Length) -> Result<Length, LoadError> {
        let x = match *self {
            Self::FromLeft(d) => d,
            Self::FromRight(d) => span - d,
            Self::Fraction(f) => span * f,
        };
        if x.si().is_finite() && x.si() >= 0.0 && x <= span {
            Ok(x)
        } else {
            Err(LoadError::OutsideSpan { position: x, span })
        }
    }
}

/// A concentrated load.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointLoad {
    /// Label for reports.
    pub label: String,
    /// Where it acts.
    pub position: Position,
    /// Downward component.
    #[serde(default)]
    pub vertical: Force,
    /// Component perpendicular to the line, horizontal.
    #[serde(default)]
    pub transverse: Force,
    /// Component along the line.
    #[serde(default)]
    pub longitudinal: Force,
    /// Load cases where it applies; empty = permanent.
    #[serde(default)]
    pub cases: Vec<String>,
}

/// A load distributed along the cable.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DistributedLoad {
    /// Label for reports.
    pub label: String,
    /// Start of the loaded interval.
    pub from: Position,
    /// End of the loaded interval.
    pub to: Position,
    /// Downward load per unit length of cable.
    #[serde(default)]
    pub vertical: ForcePerLength,
    /// Horizontal load per unit length, perpendicular to the line.
    #[serde(default)]
    pub transverse: ForcePerLength,
    /// Load cases where it applies; empty = permanent.
    #[serde(default)]
    pub cases: Vec<String>,
}

/// The additional loads of one span.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpanLoads {
    /// Point loads.
    #[serde(default)]
    pub point: Vec<PointLoad>,
    /// Distributed loads.
    #[serde(default)]
    pub distributed: Vec<DistributedLoad>,
}

fn applies(cases: &[String], case: &str) -> bool {
    cases.is_empty() || cases.iter().any(|c| c == case)
}

/// Vertical loads of a span in one load case, positions resolved to
/// horizontal distances from the left support.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VerticalLoads {
    /// `(x, P)`: downward point loads.
    pub point: Vec<(Length, Force)>,
    /// `(x_start, x_end, w)`: downward load per unit cable length over an interval.
    pub distributed: Vec<(Length, Length, ForcePerLength)>,
}

/// Selects the loads applying in `case` and resolves their positions.
///
/// Fails if a selected load has a transverse or longitudinal component
/// (needs the 3D wire model) or lies outside the span.
pub fn resolve_vertical(
    loads: &SpanLoads,
    span: Length,
    case: &str,
) -> Result<VerticalLoads, LoadError> {
    let mut out = VerticalLoads::default();
    for p in loads.point.iter().filter(|p| applies(&p.cases, case)) {
        if p.transverse != Force::ZERO || p.longitudinal != Force::ZERO {
            return Err(LoadError::NeedsThreeDimensions(p.label.clone()));
        }
        out.point.push((p.position.resolve(span)?, p.vertical));
    }
    for d in loads.distributed.iter().filter(|d| applies(&d.cases, case)) {
        if d.transverse != ForcePerLength::ZERO {
            return Err(LoadError::NeedsThreeDimensions(d.label.clone()));
        }
        let (a, b) = (d.from.resolve(span)?, d.to.resolve(span)?);
        if a >= b {
            return Err(LoadError::EmptyInterval(d.label.clone()));
        }
        out.distributed.push((a, b, d.vertical));
    }
    Ok(out)
}

/// Errors in load definitions.
#[derive(Debug, Clone, PartialEq)]
pub enum LoadError {
    /// A position lies outside the span.
    OutsideSpan {
        /// Resolved position.
        position: Length,
        /// Span length.
        span: Length,
    },
    /// A distributed load's interval is empty or reversed.
    EmptyInterval(String),
    /// A load has components outside the vertical plane.
    NeedsThreeDimensions(String),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutsideSpan { position, span } => write!(
                f,
                "load position {:.3} m is outside the span (0 to {:.3} m)",
                position.meters(),
                span.meters()
            ),
            Self::EmptyInterval(label) => {
                write!(f, "distributed load \"{label}\": start must be before end")
            }
            Self::NeedsThreeDimensions(label) => write!(
                f,
                "load \"{label}\" has a transverse or longitudinal component: this needs the 3D \
                 finite-element wire model (roadmap Phase 4); only vertical loads are supported now"
            ),
        }
    }
}

impl std::error::Error for LoadError {}

#[cfg(test)]
mod tests {
    use super::*;

    const LOADS: &str = r#"
        [[point]]
        label = "marker"
        position = { from_left = "150 m" }
        vertical = "70 N"

        [[point]]
        label = "line cart"
        position = { fraction = 0.25 }
        vertical = "1.5 kN"
        cases = ["maintenance"]

        [[distributed]]
        label = "partial ice"
        from = { fraction = 0.0 }
        to = { from_right = "100 m" }
        vertical = "12 N/m"
        cases = ["ice shedding"]
    "#;

    fn loads() -> SpanLoads {
        toml::from_str(LOADS).unwrap()
    }

    #[test]
    fn permanent_loads_apply_everywhere() {
        let v = resolve_vertical(&loads(), Length::from_meters(300.0), "everyday").unwrap();
        assert_eq!(
            v.point,
            [(Length::from_meters(150.0), Force::from_newtons(70.0))]
        );
        assert!(v.distributed.is_empty());
    }

    #[test]
    fn case_loads_apply_only_in_their_cases() {
        let span = Length::from_meters(300.0);
        let m = resolve_vertical(&loads(), span, "maintenance").unwrap();
        assert_eq!(m.point.len(), 2);
        assert_eq!(
            m.point[1],
            (Length::from_meters(75.0), Force::from_kilonewtons(1.5))
        );
        let ice = resolve_vertical(&loads(), span, "ice shedding").unwrap();
        assert_eq!(
            ice.distributed,
            [(
                Length::ZERO,
                Length::from_meters(200.0),
                ForcePerLength::from_newtons_per_meter(12.0)
            )]
        );
    }

    #[test]
    fn rejects_out_of_span_and_3d_loads() {
        assert!(matches!(
            resolve_vertical(&loads(), Length::from_meters(120.0), "everyday"),
            Err(LoadError::OutsideSpan { .. })
        ));
        let mut l = loads();
        l.point[0].transverse = Force::from_newtons(10.0);
        let error = resolve_vertical(&l, Length::from_meters(300.0), "everyday").unwrap_err();
        assert!(error.to_string().contains("Phase 4"), "{error}");
    }
}
