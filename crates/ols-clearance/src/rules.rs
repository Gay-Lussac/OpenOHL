// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Clearance rules (`*.olsr`): what to check against, and the required
//! vertical and/or radial distances (DESIGN_GOALS §13).
//!
//! ```toml
//! format = "ols-clearance-rules"
//! version = 1
//! name = "Example rules"
//!
//! [[rule]]
//! name = "Ground"
//! target = "ground"
//! vertical = "7 m"
//!
//! [[rule]]
//! name = "Vegetation"
//! target = "points"
//! roles = ["vegetation"]      # and/or classes = [42, 43]
//! vertical = "5 m"
//! radial = "4 m"
//!
//! [[rule]]
//! name = "Phase to phase"
//! target = "same_line"
//! radial = "3 m"
//!
//! [[rule]]
//! name = "Crossed lines"
//! target = "other_lines"
//! vertical = "4 m"
//! radial = "4 m"
//! ```
//!
//! - **vertical**: height of the wire above the target directly below it —
//!   the ground surface; points within `vertical_band` (horizontal) of the
//!   wire, since points are never exactly below it; wires crossing in plan.
//! - **radial**: shortest 3D distance between the wire and the target.
//! - Results are reported for targets within the required distance plus
//!   `search_margin` (default 5 m): smallest clearances, and violations.
//! - `same_line`: other wires of the same line, in the same span and other
//!   spans; wires of the same circuit and phase (its back and ahead spans,
//!   jumpers) are skipped unless `include_same_phase = true`.

use std::path::Path;

use ols_core::units::Length;
use serde::Deserialize;

/// File extension of rule sets.
pub const RULE_FILE_EXTENSION: &str = "olsr";
/// Format identifier.
pub const RULE_FILE_FORMAT: &str = "ols-clearance-rules";

/// What a rule checks against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// The ground surface.
    Ground,
    /// Points of these classes, or of classes with these roles.
    Points {
        /// Class numbers.
        classes: Vec<u8>,
        /// Class roles (resolved with a class scheme).
        roles: Vec<String>,
    },
    /// Other wires of the same line.
    SameLine {
        /// Also check wires of the same circuit and phase.
        include_same_phase: bool,
    },
    /// Wires of other lines.
    OtherLines,
}

/// One rule.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    /// Name, for reports.
    pub name: String,
    /// Target.
    pub target: Target,
    /// Required vertical clearance, m.
    pub vertical: Option<f64>,
    /// Required radial clearance, m.
    pub radial: Option<f64>,
    /// Points count for vertical clearance within this horizontal distance
    /// of the wire, m.
    pub vertical_band: f64,
    /// Targets are reported within the required distance plus this, m.
    pub search_margin: f64,
}

impl Rule {
    /// Distance within which targets are looked for, m.
    pub fn reach(&self) -> f64 {
        self.vertical.unwrap_or(0.0).max(self.radial.unwrap_or(0.0)) + self.search_margin
    }

    /// Class numbers of a point rule: its classes and those of its roles
    /// (`classes_with_role`, e.g. from a LiDAR class scheme), sorted.
    pub fn point_classes(&self, classes_with_role: impl Fn(&str) -> Vec<u8>) -> Vec<u8> {
        let Target::Points { classes, roles } = &self.target else {
            return Vec::new();
        };
        let mut all = classes.clone();
        for r in roles {
            all.extend(classes_with_role(r));
        }
        all.sort_unstable();
        all.dedup();
        all
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRule {
    name: String,
    target: String,
    #[serde(default)]
    classes: Vec<u8>,
    #[serde(default)]
    roles: Vec<String>,
    #[serde(default)]
    include_same_phase: bool,
    #[serde(default)]
    vertical: Option<Length>,
    #[serde(default)]
    radial: Option<Length>,
    #[serde(default)]
    vertical_band: Option<Length>,
    #[serde(default)]
    search_margin: Option<Length>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRules {
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    version: Option<u32>,
    #[serde(default)]
    name: String,
    #[serde(rename = "rule", default)]
    rules: Vec<RawRule>,
}

/// A set of rules.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleSet {
    /// Name.
    pub name: String,
    /// Rules.
    pub rules: Vec<Rule>,
}

/// Default horizontal band for vertical clearance to points, m.
pub const DEFAULT_VERTICAL_BAND: f64 = 0.5;
/// Default search margin, m.
pub const DEFAULT_SEARCH_MARGIN: f64 = 5.0;

impl RuleSet {
    /// Parses and checks a rule set.
    pub fn parse(text: &str) -> Result<Self, String> {
        let raw: RawRules = toml::from_str(text).map_err(|e| e.to_string())?;
        if let Some(f) = raw.format.as_deref().filter(|f| *f != RULE_FILE_FORMAT) {
            return Err(format!("format \"{f}\" is not \"{RULE_FILE_FORMAT}\""));
        }
        if raw.version.is_some_and(|v| v > 1) {
            return Err("rule format version newer than supported (1)".into());
        }
        let rules = raw
            .rules
            .into_iter()
            .map(|r| {
                let bad = |m: &str| Err(format!("rule \"{}\": {m}", r.name));
                let target = match r.target.as_str() {
                    "ground" => Target::Ground,
                    "points" => {
                        if r.classes.is_empty() && r.roles.is_empty() {
                            return bad("a points rule needs classes and/or roles");
                        }
                        Target::Points {
                            classes: r.classes.clone(),
                            roles: r.roles.clone(),
                        }
                    }
                    "same_line" => Target::SameLine {
                        include_same_phase: r.include_same_phase,
                    },
                    "other_lines" => Target::OtherLines,
                    other => {
                        return bad(&format!(
                            "target \"{other}\" is not ground, points, same_line or other_lines"
                        ));
                    }
                };
                if !matches!(target, Target::Points { .. })
                    && (!r.classes.is_empty() || !r.roles.is_empty())
                {
                    return bad("classes and roles are for points rules");
                }
                let (vertical, radial) = (r.vertical.map(|l| l.si()), r.radial.map(|l| l.si()));
                if vertical.is_none() && radial.is_none() {
                    return bad("give a vertical and/or a radial clearance");
                }
                if vertical
                    .into_iter()
                    .chain(radial)
                    .any(|d| d.is_nan() || d < 0.0)
                {
                    return bad("clearances must be ≥ 0");
                }
                Ok(Rule {
                    name: r.name,
                    target,
                    vertical,
                    radial,
                    vertical_band: r.vertical_band.map_or(DEFAULT_VERTICAL_BAND, |l| l.si()),
                    search_margin: r.search_margin.map_or(DEFAULT_SEARCH_MARGIN, |l| l.si()),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self {
            name: raw.name,
            rules,
        })
    }

    /// Reads a rule file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// A single vertical ground clearance rule.
    pub fn ground(vertical: f64) -> Self {
        Self {
            name: "ground".into(),
            rules: vec![Rule {
                name: "ground".into(),
                target: Target::Ground,
                vertical: Some(vertical),
                radial: None,
                vertical_band: DEFAULT_VERTICAL_BAND,
                search_margin: DEFAULT_SEARCH_MARGIN,
            }],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_targets() {
        let text = r#"
            format = "ols-clearance-rules"
            name = "test"
            [[rule]]
            name = "g"
            target = "ground"
            vertical = "7 m"
            radial = 6.5
            [[rule]]
            name = "veg"
            target = "points"
            roles = ["vegetation"]
            classes = [99]
            vertical = "5 m"
            vertical_band = "1 m"
            [[rule]]
            name = "pp"
            target = "same_line"
            radial = "3 m"
            [[rule]]
            name = "x"
            target = "other_lines"
            vertical = "4 m"
        "#;
        let set = RuleSet::parse(text).unwrap();
        assert_eq!(set.rules.len(), 4);
        assert_eq!(set.rules[0].radial, Some(6.5));
        assert_eq!(set.rules[0].reach(), 12.0);
        assert_eq!(set.rules[1].vertical_band, 1.0);
        let classes = set.rules[1].point_classes(|r| {
            if r == "vegetation" {
                vec![43, 42]
            } else {
                vec![]
            }
        });
        assert_eq!(classes, vec![42, 43, 99]);
        assert_eq!(
            set.rules[2].target,
            Target::SameLine {
                include_same_phase: false
            }
        );
    }

    #[test]
    fn rejects_bad_rules() {
        for text in [
            "[[rule]]\nname = \"a\"\ntarget = \"ground\"",
            "[[rule]]\nname = \"a\"\ntarget = \"points\"\nvertical = 1.0",
            "[[rule]]\nname = \"a\"\ntarget = \"ground\"\nclasses = [2]\nvertical = 1.0",
            "[[rule]]\nname = \"a\"\ntarget = \"sky\"\nvertical = 1.0",
            "[[rule]]\nname = \"a\"\ntarget = \"ground\"\nvertical = -1.0",
        ] {
            assert!(RuleSet::parse(text).is_err(), "{text}");
        }
    }
}
