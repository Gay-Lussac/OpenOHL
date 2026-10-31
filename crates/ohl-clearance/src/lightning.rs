// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The **lightning shielding check**'s rules (owner, 2026-10-31; built, in
//! active development, not approved yet): whether the earth wires keep
//! lightning off the phase conductors, by a **fixed shielding angle** or
//! by the **rolling sphere** of the electrogeometric model — Checks ›
//! Lightning shielding… and `ohl lightning`. The geometry is computed by
//! `ohl_electrical::lightning`; this module holds the rules as a rule
//! file keeps them (`[[lightning_rule]]` tables of a `.ohlr`).
//!
//! A rule names the phases it protects (wire types, systems, circuits;
//! none: every phase), the wires shielding them (earthed wires: those
//! without a system; or wire types named), its weather cases, the method
//! and the step of the cross-sections along each span. The striking
//! distance of the sphere is typed (m) or follows a stroke current,
//! r = A·I^b, and the ground's is β times it. No value is built in beyond
//! the defaults of that form (A = 10, b = 0.65, β = 1): the rule's values
//! are the user's or a design-code plugin's preset.

use crate::tension::TensionTargets;

/// The default step of the cross-sections along a span, m.
pub const DEFAULT_STEP: f64 = 5.0;

/// The striking distance's coefficient A of r = A·I^b (m, I in kA), the
/// common form's default (Love 1973).
pub const DEFAULT_A: f64 = 10.0;

/// Its exponent b.
pub const DEFAULT_B: f64 = 0.65;

/// How the striking distance to the wires is given.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StrikingDistance {
    /// Typed, m.
    Typed(f64),
    /// From a stroke current I (kA): r = A·I^b.
    Current {
        /// The stroke current, kA.
        ka: f64,
        /// A, m.
        a: f64,
        /// b.
        b: f64,
    },
}

impl StrikingDistance {
    /// To the wires, m.
    pub fn metres(self) -> f64 {
        match self {
            Self::Typed(r) => r,
            Self::Current { ka, a, b } => a * ka.powf(b),
        }
    }

    /// The stroke current of striking distance `r` m by the same form (none
    /// for a typed distance).
    pub fn current_of(self, r: f64) -> Option<f64> {
        match self {
            Self::Typed(_) => None,
            Self::Current { a, b, .. } => Some((r / a).powf(1.0 / b)),
        }
    }

    /// In words: "40.0 m", "40.0 m (10 kA, r = 10·I^0.65)".
    pub fn describe(self) -> String {
        match self {
            Self::Typed(r) => format!("{r:.1} m"),
            Self::Current { ka, a, b } => {
                format!("{:.1} m ({ka} kA, r = {a}·I^{b})", self.metres())
            }
        }
    }
}

/// The method of a lightning rule.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Method {
    /// Fixed shielding angles from the vertical through each shielding
    /// wire, degrees: towards another shielding wire (`inner`) and
    /// elsewhere (`outer`).
    Angle {
        /// Away from the other shielding wires, degrees.
        outer: f64,
        /// Towards another shielding wire, degrees.
        inner: f64,
    },
    /// The rolling sphere: its radius to the wires, and to the ground
    /// `ground_factor` times it.
    Sphere {
        /// The striking distance to the wires.
        distance: StrikingDistance,
        /// β: the striking distance to the ground over that to the wires.
        ground_factor: f64,
    },
}

impl Method {
    /// `"angle"` or `"sphere"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Angle { .. } => "angle",
            Self::Sphere { .. } => "sphere",
        }
    }

    /// In words, for reports.
    pub fn describe(self) -> String {
        match self {
            Self::Angle { outer, inner } => {
                format!("shielding angle {outer}° outer, {inner}° inner")
            }
            Self::Sphere {
                distance,
                ground_factor,
            } => {
                if (ground_factor - 1.0).abs() < 1e-12 {
                    format!("rolling sphere {}", distance.describe())
                } else {
                    format!(
                        "rolling sphere {}, to the ground × {ground_factor}",
                        distance.describe()
                    )
                }
            }
        }
    }
}

/// One rule of the lightning shielding check (`[[lightning_rule]]`).
#[derive(Debug, Clone, PartialEq)]
pub struct LightningRule {
    /// Name, for reports.
    pub name: String,
    /// Weather cases (names, `*` `?` wildcards); empty: every case.
    pub cases: Vec<String>,
    /// The method and its values.
    pub method: Method,
    /// The phase conductors it protects (empty lists: every phase; only
    /// sections with a system are phases).
    pub targets: TensionTargets,
    /// The wire types that shield (`*` `?` wildcards); empty: every
    /// earthed wire (a section without a system).
    pub shields: Vec<String>,
    /// The cross-sections' step along each span, m.
    pub step: f64,
}

impl LightningRule {
    /// A shielding-angle rule for every phase, its angles to set (0°).
    pub fn blank(name: &str) -> Self {
        Self {
            name: name.into(),
            cases: Vec::new(),
            method: Method::Angle {
                outer: 0.0,
                inner: 0.0,
            },
            targets: TensionTargets::default(),
            shields: Vec::new(),
            step: DEFAULT_STEP,
        }
    }

    /// Whether it applies to weather case `case`.
    pub fn applies(&self, case: &str) -> bool {
        self.cases.is_empty()
            || self
                .cases
                .iter()
                .any(|c| ohl_core::pattern::matches(c, case))
    }

    /// Whether section `s` is a phase it protects.
    pub fn protects(&self, s: &ohl_line::Section) -> bool {
        s.system.is_some() && self.targets.takes(s)
    }

    /// Whether section `s` shields.
    pub fn shields(&self, s: &ohl_line::Section) -> bool {
        if self.shields.is_empty() {
            s.system.is_none()
        } else {
            self.shields
                .iter()
                .any(|p| ohl_core::pattern::matches(p, &s.cable))
        }
    }

    /// What is wrong with it (none: nothing).
    pub fn problem(&self) -> Option<String> {
        let n = &self.name;
        if !(self.step.is_finite() && self.step >= 0.1) {
            return Some(format!(
                "\"{n}\": the step along the span must be at least 0.1 m"
            ));
        }
        match self.method {
            Method::Angle { outer, inner } => {
                for (what, a) in [("outer", outer), ("inner", inner)] {
                    if !(a.is_finite() && a > 0.0 && a < 90.0) {
                        return Some(format!(
                            "\"{n}\": the {what} shielding angle must be above 0° and below 90°"
                        ));
                    }
                }
                None
            }
            Method::Sphere {
                distance,
                ground_factor,
            } => {
                if let StrikingDistance::Current { ka, a, b } = distance
                    && !(ka > 0.0 && a > 0.0 && b > 0.0 && ka.is_finite())
                {
                    return Some(format!(
                        "\"{n}\": the stroke current and A and b must be above 0"
                    ));
                }
                let r = distance.metres();
                if !(r.is_finite() && r > 0.0) {
                    return Some(format!("\"{n}\": the striking distance must be above 0 m"));
                }
                if !(ground_factor.is_finite() && ground_factor > 0.0) {
                    return Some(format!("\"{n}\": the ground factor must be above 0"));
                }
                None
            }
        }
    }

    /// The rule as a `[[lightning_rule]]` table of a rule file.
    pub fn to_toml(&self) -> String {
        use std::fmt::Write;
        let q = |t: &str| toml::Value::String(t.to_owned()).to_string();
        let list = |v: &[String]| {
            format!(
                "[{}]",
                v.iter().map(|x| q(x)).collect::<Vec<_>>().join(", ")
            )
        };
        let mut t = format!("\n[[lightning_rule]]\nname = {}\n", q(&self.name));
        if !self.cases.is_empty() {
            let _ = writeln!(t, "cases = {}", list(&self.cases));
        }
        let _ = writeln!(t, "method = \"{}\"", self.method.as_str());
        match self.method {
            Method::Angle { outer, inner } => {
                let _ = writeln!(t, "outer_angle = {outer:?}\ninner_angle = {inner:?}");
            }
            Method::Sphere {
                distance,
                ground_factor,
            } => {
                match distance {
                    StrikingDistance::Typed(r) => {
                        let _ = writeln!(t, "striking_distance = {r:?}");
                    }
                    StrikingDistance::Current { ka, a, b } => {
                        let _ = writeln!(t, "stroke_current = {ka:?}");
                        if a != DEFAULT_A || b != DEFAULT_B {
                            let _ = writeln!(t, "distance_a = {a:?}\ndistance_b = {b:?}");
                        }
                    }
                }
                if ground_factor != 1.0 {
                    let _ = writeln!(t, "ground_factor = {ground_factor:?}");
                }
            }
        }
        if self.step != DEFAULT_STEP {
            let _ = writeln!(t, "step = {:?}", self.step);
        }
        if !self.shields.is_empty() {
            let _ = writeln!(t, "shields = {}", list(&self.shields));
        }
        if !self.targets.is_all() {
            let mut parts = Vec::new();
            for (k, v) in [
                ("wire_types", &self.targets.wire_types),
                ("systems", &self.targets.systems),
                ("circuits", &self.targets.circuits),
            ] {
                if !v.is_empty() {
                    parts.push(format!("{k} = {}", list(v)));
                }
            }
            let _ = writeln!(t, "targets = {{ {} }}", parts.join(", "));
        }
        t
    }
}

/// A `[[lightning_rule]]` table as read.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawLightningRule {
    name: String,
    #[serde(default)]
    cases: Vec<String>,
    method: String,
    #[serde(default)]
    outer_angle: Option<f64>,
    #[serde(default)]
    inner_angle: Option<f64>,
    #[serde(default)]
    striking_distance: Option<f64>,
    #[serde(default)]
    stroke_current: Option<f64>,
    #[serde(default)]
    distance_a: Option<f64>,
    #[serde(default)]
    distance_b: Option<f64>,
    #[serde(default)]
    ground_factor: Option<f64>,
    #[serde(default)]
    step: Option<f64>,
    #[serde(default)]
    shields: Vec<String>,
    #[serde(default)]
    targets: Option<RawTargets>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTargets {
    #[serde(default)]
    wire_types: Vec<String>,
    #[serde(default)]
    systems: Vec<String>,
    #[serde(default)]
    circuits: Vec<String>,
}

impl RawLightningRule {
    pub(crate) fn resolve(self) -> Result<LightningRule, String> {
        let n = self.name.clone();
        let err = |e: &str| format!("lightning rule \"{n}\": {e}");
        let method = match self.method.as_str() {
            "angle" => {
                if self.striking_distance.is_some() || self.stroke_current.is_some() {
                    return Err(err("a striking distance belongs to method = \"sphere\""));
                }
                let outer = self
                    .outer_angle
                    .ok_or_else(|| err("method = \"angle\" needs outer_angle (degrees)"))?;
                Method::Angle {
                    outer,
                    // One angle given: the same on both sides.
                    inner: self.inner_angle.unwrap_or(outer),
                }
            }
            "sphere" => {
                if self.outer_angle.is_some() || self.inner_angle.is_some() {
                    return Err(err("the shielding angles belong to method = \"angle\""));
                }
                let distance = match (self.striking_distance, self.stroke_current) {
                    (Some(r), None) => {
                        if self.distance_a.is_some() || self.distance_b.is_some() {
                            return Err(err(
                                "distance_a and distance_b go with stroke_current, not a typed striking_distance",
                            ));
                        }
                        StrikingDistance::Typed(r)
                    }
                    (None, Some(ka)) => StrikingDistance::Current {
                        ka,
                        a: self.distance_a.unwrap_or(DEFAULT_A),
                        b: self.distance_b.unwrap_or(DEFAULT_B),
                    },
                    (Some(_), Some(_)) => {
                        return Err(err("give striking_distance or stroke_current, not both"));
                    }
                    (None, None) => {
                        return Err(err(
                            "method = \"sphere\" needs striking_distance (m) or stroke_current (kA)",
                        ));
                    }
                };
                Method::Sphere {
                    distance,
                    ground_factor: self.ground_factor.unwrap_or(1.0),
                }
            }
            other => return Err(err(&format!("method \"{other}\": use angle or sphere"))),
        };
        let t = self.targets.unwrap_or_default();
        let rule = LightningRule {
            name: self.name,
            cases: self.cases,
            method,
            targets: TensionTargets {
                wire_types: t.wire_types,
                systems: t.systems,
                circuits: t.circuits,
            },
            shields: self.shields,
            step: self.step.unwrap_or(DEFAULT_STEP),
        };
        match rule.problem() {
            Some(e) => Err(format!("lightning rule {e}")),
            None => Ok(rule),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::RuleSet;

    /// Both methods read, written and read back the same; one angle given
    /// is both; the current's form by default 10·I^0.65.
    #[test]
    fn lightning_rules_in_a_rule_file() {
        let text = r#"
format = "ohl-clearance-rules"
version = 1
name = "invented"

[[lightning_rule]]
name = "angle 30"
cases = ["max sag"]
method = "angle"
outer_angle = 30.0

[[lightning_rule]]
name = "sphere 10 kA"
method = "sphere"
stroke_current = 10.0
ground_factor = 0.9
step = 2.0
targets = { circuits = ["North"] }
"#;
        let set = RuleSet::parse(text).unwrap();
        assert_eq!(set.lightning_rules.len(), 2);
        let (a, b) = (&set.lightning_rules[0], &set.lightning_rules[1]);
        assert_eq!(
            a.method,
            Method::Angle {
                outer: 30.0,
                inner: 30.0
            }
        );
        assert!(a.applies("max sag") && !a.applies("ice"));
        let Method::Sphere {
            distance,
            ground_factor,
        } = b.method
        else {
            panic!("a sphere");
        };
        assert!((distance.metres() - 10.0 * 10f64.powf(0.65)).abs() < 1e-9);
        assert!((distance.current_of(distance.metres()).unwrap() - 10.0).abs() < 1e-9);
        assert_eq!(ground_factor, 0.9);
        assert_eq!(b.step, 2.0);
        let back = RuleSet::parse(&set.to_toml()).unwrap();
        assert_eq!(back.lightning_rules, set.lightning_rules);
        assert!(set.is_for(crate::rules::PresetCheck::Lightning));
    }

    /// What cannot be checked is refused with the rule's name.
    #[test]
    fn a_rule_without_its_values_is_refused() {
        for (body, says) in [
            ("method = \"angle\"", "needs outer_angle"),
            ("method = \"sphere\"", "needs striking_distance"),
            (
                "method = \"sphere\"\nstriking_distance = 30.0\nstroke_current = 5.0",
                "not both",
            ),
            ("method = \"angle\"\nouter_angle = 95.0", "below 90°"),
            ("method = \"cone\"", "use angle or sphere"),
        ] {
            let text = format!(
                "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"x\"\n[[lightning_rule]]\nname = \"r\"\n{body}\n"
            );
            let e = RuleSet::parse(&text).unwrap_err();
            assert!(e.contains("\"r\"") && e.contains(says), "{e}");
        }
    }
}
