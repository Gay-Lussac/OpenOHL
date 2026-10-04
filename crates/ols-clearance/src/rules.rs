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
//! - `cases = ["max temperature", "glaze*"]`: the weather cases the rule
//!   applies to (default: all).
//! - `case_pairs = [["glaze ice", "everyday"]]` (wire rules): the first
//!   wire in the first case, the second wire in the second — e.g. an iced
//!   upper wire sagging onto a lower wire without ice.
//! - Exclusion zones, where wires come close by design (at supports, down
//!   to cable terminations): `exclude_near_ends = "10 m"` leaves out the
//!   wire within that horizontal distance of its ends;
//!   `exclude_spans_at = ["con_*"]` leaves out spans ending at those
//!   structures.
//!
//! ## Clearances by voltage
//!
//! `vertical` and `radial` are a distance, or depend on the nominal
//! voltage of the wire's system (kV; [`ols_line::electrical`]; earthed
//! wires count as 0 kV):
//!
//! ```toml
//! [[rule]]
//! name = "Buildings"
//! target = "points"
//! roles = ["building"]
//! # Rows [up to kV, distance]: the first row whose voltage is reached.
//! vertical = { by_voltage = [[0, "5 m"], [150, "6 m"], [245, "6.5 m"], [420, "7.5 m"]] }
//! # base + per_kv · (U − above_kv) above `above_kv`, else base.
//! radial = { base = "5 m", per_kv = "0.01 m", above_kv = 50 }
//! ```
//!
//! A voltage above the last row of a table is an error, reported before
//! any check runs ([`Rule::check_voltages`]). For wire rules (`same_line`,
//! `other_lines`) the voltage of a pair is the higher of the two systems'
//! nominal voltages (`pair_voltage = "higher"`, default), their sum
//! (`"sum"`), or the actual voltage between the two conductors from their
//! phase angles or DC polarities (`"difference"`: the full phase-to-phase
//! voltage between two phases of a system, 0 between the same phase of
//! parallel systems; see [`Potential::between`]).

use std::path::Path;

use ols_core::units::Length;
use ols_line::electrical::Potential;
use serde::Deserialize;

/// File extension of rule sets.
pub const RULE_FILE_EXTENSION: &str = "olsr";
/// Format identifier.
pub const RULE_FILE_FORMAT: &str = "ols-clearance-rules";
/// Newest format version read.
pub const RULE_FILE_VERSION: u32 = 1;

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

/// A required clearance: a distance, or one depending on the voltage.
#[derive(Debug, Clone, PartialEq)]
pub enum Requirement {
    /// The same for every voltage, m.
    Fixed(f64),
    /// Rows (up to kV, m), voltages increasing: the first row whose
    /// voltage is not exceeded.
    ByVoltage(Vec<(f64, f64)>),
    /// `base + per_kv · (U − above_kv)` for U above `above_kv`, else
    /// `base` (m, m/kV, kV).
    Formula {
        /// Distance up to `above_kv`, m.
        base: f64,
        /// Increase per kV above `above_kv`, m/kV.
        per_kv: f64,
        /// Voltage from which the distance increases, kV.
        above_kv: f64,
    },
}

impl Requirement {
    /// The distance at `kv`; `Err` above the last row of a table.
    pub fn at(&self, kv: f64) -> Result<f64, String> {
        match self {
            Self::Fixed(d) => Ok(*d),
            Self::ByVoltage(rows) => rows
                .iter()
                .find(|(up_to, _)| kv <= *up_to)
                .map(|(_, d)| *d)
                .ok_or_else(|| {
                    format!(
                        "no requirement for {kv} kV (the table ends at {} kV)",
                        rows.last().map_or(0.0, |r| r.0)
                    )
                }),
            Self::Formula {
                base,
                per_kv,
                above_kv,
            } => Ok(base + per_kv * (kv - above_kv).max(0.0)),
        }
    }

    /// Whether the distance depends on the voltage.
    pub fn by_voltage(&self) -> bool {
        !matches!(self, Self::Fixed(_))
    }
}

impl std::fmt::Display for Requirement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fixed(d) => write!(f, "{d:.2} m"),
            Self::ByVoltage(rows) => {
                let parts: Vec<String> = rows
                    .iter()
                    .map(|(u, d)| format!("≤ {u} kV: {d:.2} m"))
                    .collect();
                write!(f, "{}", parts.join(", "))
            }
            Self::Formula {
                base,
                per_kv,
                above_kv,
            } => write!(f, "{base:.2} m + {per_kv} m/kV above {above_kv} kV"),
        }
    }
}

/// The required clearances that apply to one wire (or pair), m.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Required {
    /// Vertical.
    pub vertical: Option<f64>,
    /// Radial.
    pub radial: Option<f64>,
}

impl Required {
    /// The larger of the two (0 without any).
    pub fn largest(&self) -> f64 {
        self.vertical.unwrap_or(0.0).max(self.radial.unwrap_or(0.0))
    }
}

/// The voltage of a wire pair, for wire rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PairVoltage {
    /// The higher of the two wires' voltages.
    #[default]
    Higher,
    /// The sum of both.
    Sum,
    /// The voltage between the two conductors (phase angles, polarities).
    Difference,
}

/// One rule.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    /// Name, for reports.
    pub name: String,
    /// Target.
    pub target: Target,
    /// Required vertical clearance.
    pub vertical: Option<Requirement>,
    /// Required radial clearance.
    pub radial: Option<Requirement>,
    /// Voltage of a wire pair (wire rules).
    pub pair_voltage: PairVoltage,
    /// Points count for vertical clearance within this horizontal distance
    /// of the wire, m.
    pub vertical_band: f64,
    /// Targets are reported within the required distance plus this, m.
    pub search_margin: f64,
    /// Weather cases the rule applies to (names, `*` `?` wildcards); empty:
    /// all cases.
    pub cases: Vec<String>,
    /// Wire rules: pairs of weather cases — the first wire in the first
    /// case, the second wire in the second (e.g. an iced upper wire over a
    /// cold lower one). Empty: both wires in the same case.
    pub case_pairs: Vec<[String; 2]>,
    /// Exclusion zone: the wire within this horizontal distance of each of
    /// its ends (the clamps) is not checked, m (0: none).
    pub exclude_near_ends: f64,
    /// Exclusion zone: spans ending at these structures (names, `*` `?`
    /// wildcards) are not checked — e.g. down-leads to cable terminations.
    pub exclude_spans_at: Vec<String>,
}

impl Rule {
    /// The requirements for a wire of voltage `kv` (none: 0 kV). Use
    /// [`Rule::check_voltages`] first: above a table's last row the last
    /// row's distance is used.
    pub fn required(&self, kv: Option<f64>) -> Required {
        let kv = kv.unwrap_or(0.0);
        let at = |r: &Option<Requirement>| {
            r.as_ref().map(|r| match (r.at(kv), r) {
                (Ok(d), _) => d,
                (Err(_), Requirement::ByVoltage(rows)) => rows.last().map_or(0.0, |x| x.1),
                (Err(_), _) => 0.0,
            })
        };
        Required {
            vertical: at(&self.vertical),
            radial: at(&self.radial),
        }
    }

    /// The voltage of a pair of wires (wire rules), kV; `None` for two
    /// earthed wires.
    pub fn pair_voltage(&self, a: Option<&Potential>, b: Option<&Potential>) -> Option<f64> {
        let kv = |p: Option<&Potential>| p.map_or(0.0, |p| p.nominal_kv);
        match (a, b, self.pair_voltage) {
            (None, None, _) => None,
            (a, b, PairVoltage::Higher) => Some(kv(a).max(kv(b))),
            (a, b, PairVoltage::Sum) => Some(kv(a) + kv(b)),
            (a, b, PairVoltage::Difference) => Some(Potential::between(a, b)),
        }
    }

    /// Whether a requirement depends on the voltage.
    pub fn by_voltage(&self) -> bool {
        [&self.vertical, &self.radial]
            .into_iter()
            .flatten()
            .any(Requirement::by_voltage)
    }

    /// The voltages the rule meets: those of the wires (`voltages`; for
    /// wire rules, of the pairs they can make, with `others` the other
    /// lines' wires).
    fn voltages_met(
        &self,
        potentials: &[Option<Potential>],
        others: &[Option<Potential>],
    ) -> Vec<Option<f64>> {
        let pairs = |a: &[Option<Potential>], b: &[Option<Potential>]| -> Vec<Option<f64>> {
            a.iter()
                .flat_map(|x| b.iter().map(move |y| (x, y)))
                .map(|(x, y)| self.pair_voltage(x.as_ref(), y.as_ref()))
                .collect()
        };
        let mut v: Vec<Option<f64>> = match self.target {
            Target::Ground | Target::Points { .. } => {
                potentials.iter().map(|p| p.map(|p| p.nominal_kv)).collect()
            }
            Target::SameLine { .. } => pairs(potentials, potentials),
            Target::OtherLines => pairs(potentials, others),
        };
        v.sort_by(|a, b| a.unwrap_or(0.0).total_cmp(&b.unwrap_or(0.0)));
        v.dedup();
        v
    }

    /// Checks that every voltage met has a requirement: `potentials` of
    /// this line's sections, `others` of other lines' sections.
    pub fn check_voltages(
        &self,
        potentials: &[Option<Potential>],
        others: &[Option<Potential>],
    ) -> Result<(), String> {
        for kv in self.voltages_met(potentials, others) {
            for (what, r) in [("vertical", &self.vertical), ("radial", &self.radial)] {
                if let Some(r) = r {
                    r.at(kv.unwrap_or(0.0))
                        .map_err(|e| format!("rule \"{}\": {what}: {e}", self.name))?;
                }
            }
        }
        Ok(())
    }

    /// Distance within which targets are looked for, m, for the voltages
    /// met (see [`Rule::check_voltages`]).
    pub fn reach(&self, potentials: &[Option<Potential>], others: &[Option<Potential>]) -> f64 {
        let met = self.voltages_met(potentials, others);
        let largest = if met.is_empty() {
            self.required(None).largest()
        } else {
            met.iter()
                .map(|&kv| self.required(kv).largest())
                .fold(0.0, f64::max)
        };
        largest + self.search_margin
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
    vertical: Option<RawRequirement>,
    #[serde(default)]
    radial: Option<RawRequirement>,
    #[serde(default)]
    pair_voltage: Option<String>,
    #[serde(default)]
    vertical_band: Option<Length>,
    #[serde(default)]
    search_margin: Option<Length>,
    #[serde(default)]
    cases: Vec<String>,
    #[serde(default)]
    case_pairs: Vec<[String; 2]>,
    #[serde(default)]
    exclude_near_ends: Option<Length>,
    #[serde(default)]
    exclude_spans_at: Vec<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawRequirement {
    Fixed(Length),
    Table {
        by_voltage: Vec<(f64, Length)>,
    },
    Formula {
        base: Length,
        per_kv: Length,
        #[serde(default)]
        above_kv: f64,
    },
}

impl RawRequirement {
    fn resolve(self) -> Result<Requirement, String> {
        let r = match self {
            Self::Fixed(l) => Requirement::Fixed(l.si()),
            Self::Table { by_voltage } => {
                let rows: Vec<(f64, f64)> =
                    by_voltage.into_iter().map(|(u, d)| (u, d.si())).collect();
                if rows.is_empty() {
                    return Err("by_voltage needs at least one row [up to kV, distance]".into());
                }
                if rows.windows(2).any(|w| w[1].0 <= w[0].0)
                    || rows.iter().any(|r| !(r.0.is_finite() && r.0 >= 0.0))
                {
                    return Err("by_voltage: voltages must be ≥ 0 and increasing".into());
                }
                Requirement::ByVoltage(rows)
            }
            Self::Formula {
                base,
                per_kv,
                above_kv,
            } => {
                if !(per_kv.si() >= 0.0 && above_kv.is_finite() && above_kv >= 0.0) {
                    return Err("formula: per_kv and above_kv must be ≥ 0".into());
                }
                Requirement::Formula {
                    base: base.si(),
                    per_kv: per_kv.si(),
                    above_kv,
                }
            }
        };
        let distances: Vec<f64> = match &r {
            Requirement::Fixed(d) => vec![*d],
            Requirement::ByVoltage(rows) => rows.iter().map(|x| x.1).collect(),
            Requirement::Formula { base, .. } => vec![*base],
        };
        if distances.iter().any(|d| d.is_nan() || *d < 0.0) {
            return Err("clearances must be ≥ 0".into());
        }
        Ok(r)
    }
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
        ols_core::files::check_header(
            raw.format.as_deref(),
            raw.version,
            RULE_FILE_FORMAT,
            RULE_FILE_VERSION,
            "rule",
        )?;
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
                let resolve =
                    |x: Option<RawRequirement>| x.map(RawRequirement::resolve).transpose();
                let (vertical, radial) = match (resolve(r.vertical), resolve(r.radial)) {
                    (Ok(v), Ok(rad)) => (v, rad),
                    (Err(e), _) | (_, Err(e)) => return bad(&e),
                };
                if vertical.is_none() && radial.is_none() {
                    return bad("give a vertical and/or a radial clearance");
                }
                let pair_voltage = match r.pair_voltage.as_deref() {
                    None | Some("higher") => PairVoltage::Higher,
                    Some("sum") => PairVoltage::Sum,
                    Some("difference") => PairVoltage::Difference,
                    Some(other) => {
                        return bad(&format!(
                            "pair_voltage \"{other}\": use higher, sum or difference"
                        ));
                    }
                };
                if r.pair_voltage.is_some()
                    && !matches!(target, Target::SameLine { .. } | Target::OtherLines)
                {
                    return bad("pair_voltage is for wire rules (same_line, other_lines)");
                }
                if !r.case_pairs.is_empty()
                    && !matches!(target, Target::SameLine { .. } | Target::OtherLines)
                {
                    return bad("case_pairs are for wire rules (same_line, other_lines)");
                }
                let not_negative = |l: Option<Length>| l.is_none_or(|l| l.si() >= 0.0);
                if !not_negative(r.vertical_band) || !not_negative(r.search_margin) {
                    return bad("vertical_band and search_margin must be ≥ 0");
                }
                Ok(Rule {
                    name: r.name,
                    target,
                    vertical,
                    radial,
                    pair_voltage,
                    vertical_band: r.vertical_band.map_or(DEFAULT_VERTICAL_BAND, |l| l.si()),
                    search_margin: r.search_margin.map_or(DEFAULT_SEARCH_MARGIN, |l| l.si()),
                    cases: r.cases,
                    case_pairs: r.case_pairs,
                    exclude_near_ends: r.exclude_near_ends.map_or(0.0, |l| l.si().max(0.0)),
                    exclude_spans_at: r.exclude_spans_at,
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
                vertical: Some(Requirement::Fixed(vertical)),
                radial: None,
                pair_voltage: PairVoltage::Higher,
                vertical_band: DEFAULT_VERTICAL_BAND,
                search_margin: DEFAULT_SEARCH_MARGIN,
                cases: Vec::new(),
                case_pairs: Vec::new(),
                exclude_near_ends: 0.0,
                exclude_spans_at: Vec::new(),
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
        assert_eq!(set.rules[0].radial, Some(Requirement::Fixed(6.5)));
        assert_eq!(set.rules[0].reach(&[], &[]), 12.0);
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
    fn case_options() {
        let set = RuleSet::parse(
            "[[rule]]\nname = \"pp\"\ntarget = \"same_line\"\nradial = 3.0\ncases = [\"hot*\"]\ncase_pairs = [[\"ice\", \"cold\"]]",
        )
        .unwrap();
        assert_eq!(set.rules[0].cases, vec!["hot*"]);
        assert_eq!(
            set.rules[0].case_pairs,
            vec![["ice".to_owned(), "cold".to_owned()]]
        );
        assert!(RuleSet::parse("[[rule]]\nname = \"g\"\ntarget = \"ground\"\nvertical = 1.0\ncase_pairs = [[\"a\", \"b\"]]").is_err());
    }

    /// Phase L1 of a three-phase system of `kv`.
    fn ac(kv: f64) -> Potential {
        Potential {
            nominal_kv: kv,
            kind: ols_line::electrical::SystemKind::Ac,
            frequency_hz: Some(50.0),
            to_earth: [kv / 3f64.sqrt(), 0.0],
        }
    }

    #[test]
    fn pair_voltage_from_phase_angles() {
        let set = RuleSet::parse(
            "[[rule]]\nname = \"pp\"\ntarget = \"same_line\"\nradial = { by_voltage = [[0, 0.5], [150, 2.0], [420, 3.5]] }\npair_voltage = \"difference\"",
        )
        .unwrap();
        let r = &set.rules[0];
        let l1 = ac(380.0);
        let mut l2 = l1;
        l2.to_earth = [
            380.0 / 3f64.sqrt() * (-120f64).to_radians().cos(),
            380.0 / 3f64.sqrt() * (-120f64).to_radians().sin(),
        ];
        // Two phases of a system: 380 kV; the same phase of a parallel
        // system: 0 kV; a phase and an earth wire: 219 kV.
        assert!((r.pair_voltage(Some(&l1), Some(&l2)).unwrap() - 380.0).abs() < 1e-9);
        assert_eq!(
            r.required(r.pair_voltage(Some(&l1), Some(&l1))).radial,
            Some(0.5)
        );
        assert_eq!(
            r.required(r.pair_voltage(Some(&l1), None)).radial,
            Some(3.5)
        );
        assert_eq!(r.pair_voltage(None, None), None);
    }

    #[test]
    fn requirements_by_voltage() {
        let set = RuleSet::parse(
            r#"
            [[rule]]
            name = "buildings"
            target = "points"
            roles = ["building"]
            vertical = { by_voltage = [[0, "5 m"], [150, "6 m"], [420, 7.5]] }
            radial = { base = "5 m", per_kv = "0.01 m", above_kv = 50 }
            [[rule]]
            name = "crossings"
            target = "other_lines"
            radial = { by_voltage = [[150, 3.0], [420, 4.0], [800, 5.5]] }
            pair_voltage = "sum"
        "#,
        )
        .unwrap();
        let b = &set.rules[0];
        assert!(b.by_voltage());
        // Earth wire (no voltage): the 0 kV row; the formula's base.
        assert_eq!(
            b.required(None),
            Required {
                vertical: Some(5.0),
                radial: Some(5.0)
            }
        );
        assert_eq!(b.required(Some(132.0)).vertical, Some(6.0));
        assert_eq!(b.required(Some(150.0)).vertical, Some(6.0));
        let r380 = b.required(Some(380.0));
        assert_eq!(r380.vertical, Some(7.5));
        assert!((r380.radial.unwrap() - 8.3).abs() < 1e-12);
        // 500 kV is beyond the table: an error before checking.
        assert!(b.check_voltages(&[Some(ac(380.0)), None], &[]).is_ok());
        let e = b.check_voltages(&[Some(ac(500.0))], &[]).unwrap_err();
        assert!(e.contains("500 kV") && e.contains("420 kV"), "{e}");
        // Reach: the largest requirement met, plus the search margin.
        assert!((b.reach(&[None, Some(ac(380.0))], &[]) - (8.3 + 5.0)).abs() < 1e-12);
        // Wire pairs: sum of the voltages (220 + 380 = 600 kV → 5.5 m).
        let x = &set.rules[1];
        assert_eq!(
            x.pair_voltage(Some(&ac(220.0)), Some(&ac(380.0))),
            Some(600.0)
        );
        assert_eq!(
            x.required(x.pair_voltage(Some(&ac(220.0)), Some(&ac(380.0))))
                .radial,
            Some(5.5)
        );
        assert!(
            x.check_voltages(&[Some(ac(380.0))], &[Some(ac(220.0))])
                .is_ok()
        );
        assert!(
            x.check_voltages(&[Some(ac(380.0))], &[Some(ac(500.0))])
                .is_err()
        );
        assert_eq!(x.reach(&[Some(ac(380.0))], &[Some(ac(220.0))]), 5.5 + 5.0);
    }

    #[test]
    fn rejects_bad_rules() {
        for text in [
            "[[rule]]\nname = \"a\"\ntarget = \"ground\"",
            "[[rule]]\nname = \"a\"\ntarget = \"points\"\nvertical = 1.0",
            "[[rule]]\nname = \"a\"\ntarget = \"ground\"\nclasses = [2]\nvertical = 1.0",
            "[[rule]]\nname = \"a\"\ntarget = \"sky\"\nvertical = 1.0",
            "[[rule]]\nname = \"a\"\ntarget = \"ground\"\nvertical = -1.0",
            "[[rule]]\nname = \"a\"\ntarget = \"ground\"\nvertical = { by_voltage = [[220, 7.0], [110, 6.0]] }",
            "[[rule]]\nname = \"a\"\ntarget = \"ground\"\nvertical = { by_voltage = [] }",
            "[[rule]]\nname = \"a\"\ntarget = \"ground\"\nvertical = { by_voltage = [[110, -6.0]] }",
            "[[rule]]\nname = \"a\"\ntarget = \"ground\"\nvertical = { base = 6.0 }",
            "[[rule]]\nname = \"a\"\ntarget = \"ground\"\nvertical = 6.0\npair_voltage = \"sum\"",
            "[[rule]]\nname = \"a\"\ntarget = \"same_line\"\nradial = 3.0\npair_voltage = \"max\"",
        ] {
            assert!(RuleSet::parse(text).is_err(), "{text}");
        }
    }
}
