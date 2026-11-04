// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A structure's **local conditions** (owner, 2026-11-03: "the wind for
//! the internal clearance might change per structure if we have a wind
//! study. the wind for structure calculation might be different but also
//! on a per structure basis and the snow too"): factors on the weather
//! cases' wind and ice and the snow on the ground there, taken by a check
//! with **Use local conditions** ticked; the spans take the worst (or the
//! mean) of their two structures' ([`SpanRule`]). They replace the weather
//! case groups as the way to give local conditions.

use serde::{Deserialize, Serialize};

/// A structure's local conditions (each optional: none, the weather case
/// as written).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalConditions {
    /// Factor on the wind for strength: the wires' and the structure's
    /// wind in the structure loads, string resistance and the tensions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind_static: Option<f64>,
    /// Factor on the wind for clearances: the swing of the strings and
    /// jumpers at the structure (internal clearances, clearances to the
    /// structure).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind_clearance: Option<f64>,
    /// Factor on the ice (and wet snow) on the wires of its spans.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wire_ice: Option<f64>,
    /// Ice on the structure's members, thickness, m.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structure_ice: Option<f64>,
    /// The snow on the ground, s_k, N/m² (none: from `snow_altitude`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snow_ground: Option<f64>,
    /// The reference altitude h_0 for SIA 261's snow (5.2.6), m: its
    /// altitude with the regional correction of SIA 261 Annex D.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snow_altitude: Option<f64>,
    /// The wind pressure a wind study gives at this structure for the
    /// conductors' distances to it, Pa (owner 2026-11-04: "make the wind
    /// study local"): with **Use local conditions** ticked, Internal
    /// clearances checks the structure in the wind study's cases at this
    /// pressure (raised to the check's least, `[check] wind_study_min`)
    /// in place of the case's. Before, a weather case group's
    /// `structure_wind` column.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind_study: Option<f64>,
}

impl LocalConditions {
    /// Nothing given.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Its values are sane: factors and the ice 0 or more, the snow 0 or
    /// more, the altitude a number, the wind study above 0.
    pub fn check(&self) -> Result<(), String> {
        for (name, v) in [
            ("wind_static", self.wind_static),
            ("wind_clearance", self.wind_clearance),
            ("wire_ice", self.wire_ice),
            ("structure_ice", self.structure_ice),
            ("snow_ground", self.snow_ground),
        ] {
            if let Some(v) = v
                && !(v.is_finite() && v >= 0.0)
            {
                return Err(format!("local {name}: 0 or more"));
            }
        }
        if self.snow_altitude.is_some_and(|h| !h.is_finite()) {
            return Err("local snow_altitude: a number".into());
        }
        if self.wind_study.is_some_and(|p| !(p.is_finite() && p > 0.0)) {
            return Err("local wind_study: a pressure above 0 Pa".into());
        }
        Ok(())
    }

    /// The snow on the ground s_k, N/m²: given, else by SIA 261 (10) from
    /// the reference altitude; none without either.
    pub fn snow(&self) -> Option<f64> {
        self.snow_ground.or(self.snow_altitude.map(snow_on_ground))
    }
}

/// The characteristic snow load on flat ground by SIA 261:2014 (10),
/// N/m²: s_k = [1 + (h_0/350)²] · 0.4 kN/m², at least 0.9 kN/m²; h_0 the
/// reference altitude, m (Annex D: the altitude with the regional
/// correction). Not for structures above 2000 m (5.1.4: site studies).
pub fn snow_on_ground(h0: f64) -> f64 {
    ((1.0 + (h0 / 350.0).powi(2)) * 400.0).max(900.0)
}

/// How a span takes the local factors of its two structures (Line ›
/// Criteria › Calculation options; owner 2026-11-03).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanRule {
    /// The larger of the two (on the safe side), the default.
    #[default]
    Worst,
    /// Their mean.
    Mean,
}

impl SpanRule {
    /// Every choice.
    pub const ALL: [Self; 2] = [Self::Worst, Self::Mean];

    /// The default.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// As written.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Worst => "worst",
            Self::Mean => "mean",
        }
    }

    /// From its written name.
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.as_str() == text)
    }

    /// A span's factor from its two structures' (none: not given, 1);
    /// none when neither gives one.
    pub fn span(self, a: Option<f64>, b: Option<f64>) -> Option<f64> {
        match (a, b) {
            (None, None) => None,
            (a, b) => {
                let (a, b) = (a.unwrap_or(1.0), b.unwrap_or(1.0));
                Some(match self {
                    Self::Worst => a.max(b),
                    Self::Mean => (a + b) / 2.0,
                })
            }
        }
    }
}

/// What a check takes the local conditions for: the wind on the wires
/// for strength (tensions, string resistance, structure loads) or for
/// clearances (the swing of the strings and jumpers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Purpose {
    /// The structures' `wind_static`.
    Strength,
    /// The structures' `wind_clearance`.
    Clearance,
}

/// `line` with its structures' local conditions on its spans, for
/// `purpose`: each span's wind factor and ice factor from its two
/// structures' by the line's [`SpanRule`] — a span giving its own
/// (Section › local winds, local ice) keeps it. The line as it is when no
/// structure gives local conditions (the caller then needs no other
/// solve; [`has_local`]).
pub fn localized(line: &crate::Line, purpose: Purpose) -> crate::Line {
    let mut out = line.clone();
    let rule = line.solver.local_spans;
    let local = |id: &str| line.structure(id).map(|s| s.local).unwrap_or_default();
    for sec in &mut out.sections {
        let n = sec.points.len().saturating_sub(1);
        for span in 0..n {
            let (a, b) = (
                local(&sec.points[span].structure),
                local(&sec.points[span + 1].structure),
            );
            let wind = |l: LocalConditions| match purpose {
                Purpose::Strength => l.wind_static,
                Purpose::Clearance => l.wind_clearance,
            };
            if !sec.wind_factors.iter().any(|w| w.span == span)
                && let Some(f) = rule.span(wind(a), wind(b))
            {
                sec.set_wind_factor(span, span, f);
            }
            if !sec.ice_factors.iter().any(|w| w.span == span)
                && let Some(f) = rule.span(a.wire_ice, b.wire_ice)
                && f != 1.0
            {
                sec.ice_factors.push(crate::SpanWind { span, factor: f });
                sec.ice_factors.sort_by_key(|w| w.span);
            }
        }
    }
    out
}

/// Whether any structure of `line` gives local conditions.
pub fn has_local(line: &crate::Line) -> bool {
    line.structures.iter().any(|s| !s.local.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SIA 261 (10) by hand: 350 m gives 0.8 kN/m², raised to 0.9; 1000 m
    /// gives [1 + (1000/350)²] 0.4 = 3.665 kN/m².
    #[test]
    fn snow_by_sia261() {
        assert_eq!(snow_on_ground(350.0), 900.0);
        let s = snow_on_ground(1000.0);
        assert!((s - (1.0 + (1000.0f64 / 350.0).powi(2)) * 400.0).abs() < 1e-9);
        assert!((s - 3665.3).abs() < 0.1, "{s}");
        let l = LocalConditions {
            snow_altitude: Some(1000.0),
            ..Default::default()
        };
        assert_eq!(l.snow(), Some(s));
        let given = LocalConditions {
            snow_ground: Some(2000.0),
            ..l
        };
        assert_eq!(given.snow(), Some(2000.0));
    }

    /// The local conditions on the spans: from the two structures by the
    /// rule, a span's own kept.
    #[test]
    fn localized_spans() {
        let (mut line, _) = crate::solve::local_tests_fixture();
        for (id, w, i) in [("T1", 1.4, 2.0), ("T2", 1.0, 1.0)] {
            let s = line.structures.iter_mut().find(|s| s.id == id).unwrap();
            s.local = LocalConditions {
                wind_static: Some(w),
                wind_clearance: Some(0.5),
                wire_ice: Some(i),
                ..Default::default()
            };
        }
        let strength = localized(&line, Purpose::Strength);
        assert_eq!(strength.sections[0].wind_factor(0), 1.4);
        assert_eq!(strength.sections[0].ice_factor(0), 2.0);
        // Span 2 between T2 (1) and T3 (none: 1).
        assert_eq!(strength.sections[0].wind_factor(1), 1.0);
        assert_eq!(
            localized(&line, Purpose::Clearance).sections[0].wind_factor(0),
            0.5
        );
        line.solver.local_spans = SpanRule::Mean;
        let mean = localized(&line, Purpose::Strength);
        assert!((mean.sections[0].wind_factor(0) - 1.2).abs() < 1e-12);
        // A span's own wind kept.
        line.sections[0].set_wind_factor(0, 0, 0.9);
        assert_eq!(
            localized(&line, Purpose::Strength).sections[0].wind_factor(0),
            0.9
        );
        assert!(has_local(&line));
    }

    /// A span: the worst or the mean, a structure without one counting 1.
    #[test]
    fn span_rules() {
        assert_eq!(SpanRule::Worst.span(Some(0.8), Some(1.2)), Some(1.2));
        assert_eq!(SpanRule::Mean.span(Some(0.8), Some(1.2)), Some(1.0));
        assert_eq!(SpanRule::Worst.span(Some(0.8), None), Some(1.0));
        assert_eq!(SpanRule::Worst.span(None, None), None);
        assert!(
            LocalConditions {
                wind_static: Some(-1.0),
                ..Default::default()
            }
            .check()
            .is_err()
        );
        for p in [0.0, -300.0, f64::NAN] {
            assert!(
                LocalConditions {
                    wind_study: Some(p),
                    ..Default::default()
                }
                .check()
                .is_err()
            );
        }
        assert!(
            LocalConditions {
                wind_study: Some(450.0),
                ..Default::default()
            }
            .check()
            .is_ok()
        );
    }
}
