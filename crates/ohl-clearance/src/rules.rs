// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Clearance rules (`*.ohlr`): what to check against, and the required
//! vertical and/or radial distances (DESIGN_GOALS §13).
//!
//! ```toml
//! format = "ohl-clearance-rules"
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
//!   `circuits = "different"` checks only wires of different circuits
//!   (circuit-to-circuit, e.g. the two circuits of a double-circuit
//!   tower, or a phase and an earth wire), `"same"` only wires of the same
//!   circuit (phase-to-phase); default `"any"`.
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
//! - `voltages = [220]`: only the wires of these nominal voltages (kV; 0
//!   for earthed wires) are checked (default: all).
//! - `wires = "sections"` or `"jumpers"`: only the sections' spans, or only
//!   the jumpers, are checked (default `"both"`).
//! - Vertical zone: `vertical_band` is its half width across the line (to
//!   the ground: default 0, straight below the wire);
//!   `vertical_depth = "0.2 m"` cuts it along the line in slices of that
//!   depth, the wire taken at its lowest over each slice.
//! - `[wind]` (`steps`, `step`, `sides`, `arc`): how the desktop clearance
//!   windows apply the wind ([`WindSteps`]), and `ohl zones`; `ohl line`
//!   ignores it. `arc = true` (2026-10-20) adds each span's swing arc;
//!   `band_margin = "0.15 m"` (2026-10-21, optional, written only when not
//!   the default [`crate::positions::BAND_MARGIN`]) is the spans' swing
//!   band margin; `jumper_band_margin = "0.3 m"` (2026-10-21, optional,
//!   written only when not the default
//!   [`crate::positions::JUMPER_BAND_MARGIN`], 0) the jumpers'.
//! - `[check]` (`circuits`, `method`, `search_distance`): what the
//!   desktop clearance windows solve and read ([`CheckScope`]): only the
//!   circuits named (by circuit name; default every circuit), every
//!   section by this calculation method (`"fe"` finite elements, `"rs"`
//!   ruling span; default each section's own), LiDAR points only within
//!   `search_distance` of the wires (default: as far as the rules reach);
//!   `ohl zones` too; `ohl line` ignores it.
//!
//! ## Clearances by voltage
//!
//! `vertical` and `radial` are a distance, or depend on the nominal
//! voltage of the wire's system (kV; [`ohl_line::electrical`]; earthed
//! wires count as 0 kV):
//!
//! ```toml
//! [[rule]]
//! name = "Buildings"
//! target = "points"
//! roles = ["building"]
//! # Rows [up to kV, distance]: the first row whose voltage is not exceeded.
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

use ohl_core::units::{Length, Stress};
use ohl_line::condition::Method;
use ohl_line::electrical::Potential;
use serde::Deserialize;

/// File extension of rule sets.
pub const RULE_FILE_EXTENSION: &str = "ohlr";
/// Format identifier.
pub const RULE_FILE_FORMAT: &str = "ohl-clearance-rules";
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
                        "no required clearance for {kv} kV (the table ends at {} kV)",
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

    /// The distance at `kv`, above a table's last row its last distance
    /// (use [`Requirement::at`] or [`Rule::check_voltages`] first to say
    /// so).
    pub fn at_or_last(&self, kv: f64) -> f64 {
        match (self.at(kv), self) {
            (Ok(d), _) => d,
            (Err(_), Self::ByVoltage(rows)) => rows.last().map_or(0.0, |x| x.1),
            (Err(_), _) => 0.0,
        }
    }

    /// The largest distance over the voltages `kvs` (kV; none: the
    /// distance at 0 kV), above a table's last row its last distance.
    pub fn largest(&self, kvs: &[f64]) -> f64 {
        if kvs.is_empty() {
            return self.at_or_last(0.0);
        }
        kvs.iter()
            .map(|&kv| self.at_or_last(kv))
            .fold(0.0, f64::max)
    }

    /// What is wrong with it as an editor fills it in, if anything: a
    /// table needs rows, voltages ≥ 0 and increasing, distances above 0; a
    /// formula a base above 0, the amount per kV and the voltage ≥ 0; a
    /// distance above 0 (0 m is no requirement: no value is invented).
    pub fn problem(&self) -> Option<String> {
        let positive = |d: f64| d.is_finite() && d > 0.0;
        match self {
            Self::Fixed(d) if !positive(*d) => Some("set the distance".into()),
            Self::Fixed(_) => None,
            Self::ByVoltage(rows) if rows.is_empty() => {
                Some("add a row: up to kV, distance".into())
            }
            Self::ByVoltage(rows) => {
                if rows.iter().any(|r| !(r.0.is_finite() && r.0 >= 0.0)) {
                    Some("voltages must be ≥ 0 kV".into())
                } else if rows.windows(2).any(|w| w[1].0 <= w[0].0) {
                    Some("the voltages must increase from row to row (Sort)".into())
                } else if rows.iter().any(|r| !positive(r.1)) {
                    Some("set every row's distance (above 0 m)".into())
                } else {
                    None
                }
            }
            Self::Formula {
                base,
                per_kv,
                above_kv,
            } => {
                if !positive(*base) {
                    Some("set the base distance".into())
                } else if !(per_kv.is_finite() && *per_kv >= 0.0) {
                    Some("the amount per kV must be ≥ 0".into())
                } else if !(above_kv.is_finite() && *above_kv >= 0.0) {
                    Some("the voltage it starts from must be ≥ 0 kV".into())
                } else {
                    None
                }
            }
        }
    }

    /// Reads a requirement as a rule file writes it ([`Requirement::to_toml`]):
    /// `5` or `"5 m"`, `{ by_voltage = [[220, "7 m"], [380, "9 m"]] }`,
    /// `{ base = "3 m", per_kv = "0.01 m", above_kv = 110 }`.
    pub fn parse(text: &str) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct Value {
            v: RawRequirement,
        }
        let v: Value = toml::from_str(&format!("v = {}", text.trim()))
            .map_err(|e| format!("{}: {}", text.trim(), e.message()))?;
        v.v.resolve()
    }

    /// As written in a rule file (`"7 m"`, `{ by_voltage = … }`,
    /// `{ base = …, per_kv = …, above_kv = … }`): read back by
    /// [`RuleSet::parse`] (and [`Requirement::parse`]) as the same
    /// requirement.
    pub fn to_toml(&self) -> String {
        match self {
            Self::Fixed(d) => length(*d),
            Self::ByVoltage(rows) => format!(
                "{{ by_voltage = {} }}",
                list(rows.iter().map(|(u, d)| format!("[{u:?}, {}]", length(*d))))
            ),
            Self::Formula {
                base,
                per_kv,
                above_kv,
            } => format!(
                "{{ base = {}, per_kv = {}, above_kv = {above_kv:?} }}",
                length(*base),
                length(*per_kv)
            ),
        }
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

/// Which wire pairs of the same line a `same_line` rule checks, by their
/// circuits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CircuitFilter {
    /// Every pair.
    #[default]
    Any,
    /// Pairs of the same circuit (phase to phase).
    Same,
    /// Pairs of different circuits (circuit to circuit).
    Different,
}

impl CircuitFilter {
    /// Whether a pair of wires of circuits `a` and `b` is checked.
    pub fn admits(self, a: &str, b: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Same => a == b,
            Self::Different => a != b,
        }
    }
}

impl PairVoltage {
    /// Reads `higher`, `sum` or `difference`.
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "higher" => Ok(Self::Higher),
            "sum" => Ok(Self::Sum),
            "difference" => Ok(Self::Difference),
            other => Err(format!(
                "pair_voltage \"{other}\": use higher, sum or difference"
            )),
        }
    }

    /// The voltage of a pair of wires, kV; `None` for two earthed wires.
    pub fn of(self, a: Option<&Potential>, b: Option<&Potential>) -> Option<f64> {
        let kv = |p: Option<&Potential>| p.map_or(0.0, |p| p.nominal_kv);
        match (a, b, self) {
            (None, None, _) => None,
            (a, b, Self::Higher) => Some(kv(a).max(kv(b))),
            (a, b, Self::Sum) => Some(kv(a) + kv(b)),
            (a, b, Self::Difference) => Some(Potential::between(a, b)),
        }
    }
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
    /// Wire pairs of the same line checked, by circuit (`same_line`).
    pub circuits: CircuitFilter,
    /// Only the wires of these nominal voltages are checked, kV (0: an
    /// earthed wire); empty: every wire. Wire rules: the first wire of a
    /// pair.
    pub voltages: Vec<f64>,
    /// Which wires are checked: the sections' spans, the jumpers or both.
    pub wires: Wires,
    /// Vertical clearances to points and the ground: the wire is taken at
    /// its lowest (its highest for a target above it) within this distance
    /// along the line, centred on the target — the zone under the wire is
    /// cut in slices of this depth, m (0: the wire straight above the
    /// target).
    pub vertical_depth: f64,
    /// Ground rules: the terrain surface (TIN layer) checked, by name
    /// (none: the surface the checks use, the project's check surface or
    /// `--terrain`).
    pub surface: Option<String>,
}

/// Which wires a rule checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Wires {
    /// The sections' spans and the jumpers.
    #[default]
    Both,
    /// The sections' spans only.
    Sections,
    /// The jumpers only.
    Jumpers,
}

impl Wires {
    /// `both`, `sections` or `jumpers`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::Sections => "sections",
            Self::Jumpers => "jumpers",
        }
    }

    /// Reads `both`, `sections` or `jumpers`.
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "both" => Ok(Self::Both),
            "sections" => Ok(Self::Sections),
            "jumpers" => Ok(Self::Jumpers),
            _ => Err(format!(
                "wires = \"{text}\": give \"sections\", \"jumpers\" or \"both\""
            )),
        }
    }

    /// Whether a wire of section id `section` (`jumper:<id>` for a jumper)
    /// is checked.
    pub fn takes(self, section: &str) -> bool {
        let jumper = section.starts_with("jumper:");
        match self {
            Self::Both => true,
            Self::Sections => !jumper,
            Self::Jumpers => jumper,
        }
    }
}

impl Rule {
    /// Whether the rule checks wires of nominal voltage `kv` (none:
    /// earthed, 0 kV): its `voltages`, or every voltage.
    pub fn checks_voltage(&self, kv: Option<f64>) -> bool {
        let kv = kv.unwrap_or(0.0);
        self.voltages.is_empty() || self.voltages.iter().any(|v| (v - kv).abs() < 1e-6)
    }

    /// The requirements for a wire of voltage `kv` (none: 0 kV). Use
    /// [`Rule::check_voltages`] first: above a table's last row the last
    /// row's distance is used.
    pub fn required(&self, kv: Option<f64>) -> Required {
        let kv = kv.unwrap_or(0.0);
        let at = |r: &Option<Requirement>| r.as_ref().map(|r| r.at_or_last(kv));
        Required {
            vertical: at(&self.vertical),
            radial: at(&self.radial),
        }
    }

    /// The voltage of a pair of wires (wire rules), kV; `None` for two
    /// earthed wires.
    pub fn pair_voltage(&self, a: Option<&Potential>, b: Option<&Potential>) -> Option<f64> {
        self.pair_voltage.of(a, b)
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
        // The wires of the rule's voltages.
        let mine: Vec<Option<Potential>> = potentials
            .iter()
            .filter(|p| self.checks_voltage(p.map(|p| p.nominal_kv)))
            .copied()
            .collect();
        let mut v: Vec<Option<f64>> = match self.target {
            Target::Ground | Target::Points { .. } => {
                mine.iter().map(|p| p.map(|p| p.nominal_kv)).collect()
            }
            Target::SameLine { .. } => pairs(&mine, potentials),
            Target::OtherLines => pairs(&mine, others),
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
    #[serde(default)]
    circuits: Option<String>,
    #[serde(default)]
    voltages: Vec<f64>,
    #[serde(default)]
    wires: Option<String>,
    #[serde(default)]
    vertical_depth: Option<Length>,
    #[serde(default)]
    surface: Option<String>,
}

/// A requirement as written in a file: a distance, a table by voltage or a
/// formula.
#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum RawRequirement {
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
    pub(crate) fn resolve(self) -> Result<Requirement, String> {
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
    #[serde(default)]
    wind: Option<RawWind>,
    #[serde(default)]
    check: Option<RawCheck>,
    #[serde(default)]
    vegetation: Option<RawVegetation>,
    #[serde(default)]
    checks: Vec<String>,
    #[serde(rename = "rule", default)]
    rules: Vec<RawRule>,
    #[serde(rename = "tension_rule", default)]
    tension_rules: Vec<crate::tension::RawTensionRule>,
    #[serde(rename = "string_rule", default)]
    string_rules: Vec<crate::strings::RawStringRule>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCheck {
    #[serde(default)]
    circuits: Vec<String>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    search_distance: Option<Length>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWind {
    #[serde(default)]
    steps: bool,
    #[serde(default)]
    step: Option<Stress>,
    #[serde(default)]
    sides: Option<String>,
    #[serde(default)]
    arc: bool,
    #[serde(default)]
    band_margin: Option<Length>,
    #[serde(default)]
    jumper_band_margin: Option<Length>,
}

/// The sides the wind blows from, looking ahead along the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindSides {
    /// From the left and from the right, each in turn.
    #[default]
    Both,
    /// From the left only.
    Left,
    /// From the right only.
    Right,
}

impl WindSides {
    /// Every choice.
    pub const ALL: [WindSides; 3] = [WindSides::Both, WindSides::Left, WindSides::Right];

    /// `both`, `left` or `right`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::Left => "left",
            Self::Right => "right",
        }
    }

    /// Reads `both`, `left` or `right`.
    pub fn parse(text: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|s| s.as_str() == text)
            .ok_or_else(|| format!("wind sides \"{text}\": use both, left or right"))
    }

    /// The sides, in turn.
    pub fn sides(self) -> &'static [ohl_weather::WindSide] {
        use ohl_weather::WindSide::{Left, Right};
        match self {
            Self::Both => &[Left, Right],
            Self::Left => &[Left],
            Self::Right => &[Right],
        }
    }
}

/// How the wind of a weather case is applied by the clearance windows of
/// the desktop application (`[wind]` of a rule file; `ohl line` checks the
/// final state of each case only, with the case's own wind side).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindSteps {
    /// Every wind pressure from 0 up to the case's, in steps of `step`
    /// (else the case's pressure only).
    pub stepped: bool,
    /// Step of the wind pressure, Pa.
    pub step: f64,
    /// Sides the wind blows from.
    pub sides: WindSides,
    /// Each span also anywhere on its **swing arc** (2026-10-20): the span
    /// swung as one about its attachment axis between still air and the
    /// full blowouts of the sides chosen, at the largest of their radii
    /// ([`crate::positions::SwingArc`]): three solves per windy case
    /// (still air, left, right) and one closed-form distance per point.
    /// Alongside the steps or alone (`[wind] arc = true`).
    pub arc: bool,
    /// The swing band's margin, m (`[wind] band_margin`, 2026-10-21):
    /// every distance to a swing arc's band is taken this much smaller.
    /// Default [`crate::positions::BAND_MARGIN`] (the measured worst
    /// excess of the intermediate pressures plus an allowance;
    /// conservative); a smaller value is on the user's responsibility.
    pub band_margin: f64,
    /// The jumpers' swing band margin, m (`[wind] jumper_band_margin`,
    /// 2026-10-21, the owner's decision: a value of its own): every
    /// distance to a jumper's band is taken this much smaller. Default
    /// [`crate::positions::JUMPER_BAND_MARGIN`], 0 — a Simplification on
    /// the user's responsibility (the band does not hold a jumper between
    /// its blowouts; exact: the wind steps); the user enters a value for
    /// their jumpers.
    pub jumper_band_margin: f64,
}

/// Default step of the wind pressure, Pa.
pub const DEFAULT_WIND_STEP: f64 = ohl_line::Defaults::PROGRAM.clearance_wind_step;

impl Default for WindSteps {
    fn default() -> Self {
        Self {
            stepped: false,
            step: DEFAULT_WIND_STEP,
            sides: WindSides::Both,
            arc: false,
            band_margin: crate::positions::BAND_MARGIN,
            jumper_band_margin: crate::positions::JUMPER_BAND_MARGIN,
        }
    }
}

/// What is wrong with a swing band's margin (m), if anything: it must be
/// a number ≥ 0.
pub fn band_margin_problem(m: f64) -> Option<String> {
    (!(m.is_finite() && m >= 0.0)).then(|| format!("band margin {m} m: must be ≥ 0 m"))
}

impl WindSteps {
    /// The bands' margins: the spans' and the jumpers'.
    pub fn margins(&self) -> crate::positions::BandMargins {
        crate::positions::BandMargins {
            span: self.band_margin,
            jumper: self.jumper_band_margin,
        }
    }

    /// The wind pressures checked for a case of wind pressure `full` (Pa),
    /// increasing: 0 and every step up to `full` when stepped (the last
    /// one `full`), else `full` alone. At most `max` steps above 0 (the
    /// step is made larger).
    pub fn pressures(&self, full: f64, max: usize) -> Vec<f64> {
        if full.is_nan() || full <= 0.0 {
            return vec![0.0];
        }
        if !self.stepped {
            return vec![full];
        }
        let step = self.step.max(full / max.max(1) as f64).max(1e-9);
        let n = ((full / step) - 1e-9).ceil().max(1.0) as usize;
        let mut v: Vec<f64> = (0..n).map(|k| k as f64 * step).collect();
        v.push(full);
        v
    }
}

/// A set of rules.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleSet {
    /// Name.
    pub name: String,
    /// Rules.
    pub rules: Vec<Rule>,
    /// How the clearance windows of the desktop application apply the
    /// wind (`[wind]`).
    pub wind: Option<WindSteps>,
    /// What the clearance windows of the desktop application solve
    /// (`[check]`).
    pub check: Option<CheckScope>,
    /// Settings of the vegetation and danger-tree check (`[vegetation]`):
    /// what a design code presets for it (with `[wind]` for its wind
    /// steps).
    pub vegetation: Option<VegetationPreset>,
    /// The checks a preset is for, as its file declares them (top-level
    /// `checks = ["vertical", "radial", "vegetation", "tension", "strings"]`,
    /// optional); empty: not declared, told from its content
    /// ([`RuleSet::is_for`]).
    pub checks: Vec<PresetCheck>,
    /// The wire tension check's rules (`[[tension_rule]]`, optional, since
    /// 2026-10-22: [`crate::tension`]); a file without them reads as
    /// before.
    pub tension_rules: Vec<crate::tension::TensionRule>,
    /// The string check's rules (`[[string_rule]]`, optional, since
    /// 2026-10-24: [`crate::strings`]); a file without them reads as
    /// before.
    pub string_rules: Vec<crate::strings::StringRule>,
}

/// A check a rule file (a design code's preset) is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetCheck {
    /// The vertical clearance window (rules with a vertical distance).
    Vertical,
    /// The radial clearance window (rules with a radial distance).
    Radial,
    /// The vegetation and danger-tree check (`[vegetation]`).
    Vegetation,
    /// The wire tension check (`[[tension_rule]]`, 2026-10-22).
    Tension,
    /// The string check (`[[string_rule]]`, 2026-10-24).
    Strings,
}

impl PresetCheck {
    /// `"vertical"`, `"radial"`, `"vegetation"` or `"tension"`.
    pub fn as_str(self) -> &'static str {
        match self {
            PresetCheck::Vertical => "vertical",
            PresetCheck::Radial => "radial",
            PresetCheck::Vegetation => "vegetation",
            PresetCheck::Tension => "tension",
            PresetCheck::Strings => "strings",
        }
    }

    /// From its name.
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "vertical" => Ok(PresetCheck::Vertical),
            "radial" => Ok(PresetCheck::Radial),
            "vegetation" => Ok(PresetCheck::Vegetation),
            "tension" => Ok(PresetCheck::Tension),
            "strings" => Ok(PresetCheck::Strings),
            other => Err(format!(
                "checks: \"{other}\" is not vertical, radial, vegetation, tension or strings"
            )),
        }
    }
}

/// The settings of the vegetation and danger-tree check a rule file
/// carries (`[vegetation]`), each optional: those given replace the
/// check's, the others stay. A design-code plugin's rule preset carrying
/// it is a preset of the vegetation check (it may hold no `[[rule]]`).
///
/// ```toml
/// [vegetation]
/// classes = [42, 43]
/// clearance = { by_voltage = [[110, "5 m"], [220, "7 m"], [380, "9 m"]] }
/// root_ball = 0.05
/// fall_allowance = "2 m"
/// max_base_ground_distance = "5 m"
/// search_distance = "50 m"
/// swing_arc = false
/// ```
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VegetationPreset {
    /// The LiDAR classes checked as vegetation (empty: not set).
    pub classes: Vec<u8>,
    /// Required clearance from the wires (by voltage or not).
    pub clearance: Option<Requirement>,
    /// Growth per year, m.
    pub growth_per_year: Option<f64>,
    /// Years until the next cut.
    pub years: Option<f64>,
    /// Buffer b, m.
    pub buffer: Option<f64>,
    /// Look for trees falling in.
    pub fall_in: Option<bool>,
    /// Root-ball fraction k.
    pub root_ball: Option<f64>,
    /// Clearance allowance A of a falling tree (by voltage or not).
    pub fall_allowance: Option<Requirement>,
    /// Smallest tree height, m.
    pub min_tree_height: Option<f64>,
    /// Tallest tree height, m.
    pub max_tree_height: Option<f64>,
    /// Largest distance from a tree base to the nearest ground point, m.
    pub max_base_ground_distance: Option<f64>,
    /// Search distance: half width of the corridor read, m.
    pub search_distance: Option<f64>,
    /// Each span along its swing arc too.
    pub swing_arc: Option<bool>,
    /// The weather cases the check runs in (names of the preset's weather
    /// cases, `*` `?` wildcards; empty: not named — the preset's cases no
    /// rule names, or all of them without rules).
    pub cases: Vec<String>,
}

impl VegetationPreset {
    /// As written in a rule file (`[vegetation]` and its lines), read back
    /// by [`RuleSet::parse`].
    pub fn to_toml(&self) -> String {
        let mut t = String::from("[vegetation]\n");
        if !self.classes.is_empty() {
            let c: Vec<String> = self.classes.iter().map(u8::to_string).collect();
            t.push_str(&format!("classes = [{}]\n", c.join(", ")));
        }
        let mut req = |k: &str, r: &Option<Requirement>| {
            if let Some(r) = r {
                t.push_str(&format!("{k} = {}\n", r.to_toml()));
            }
        };
        req("clearance", &self.clearance);
        req("fall_allowance", &self.fall_allowance);
        for (k, v) in [
            (
                "growth_per_year",
                self.growth_per_year.map(|v| format!("\"{v} m\"")),
            ),
            ("years", self.years.map(|v| format!("{v:?}"))),
            ("buffer", self.buffer.map(length)),
            ("root_ball", self.root_ball.map(|v| format!("{v:?}"))),
            ("min_tree_height", self.min_tree_height.map(length)),
            ("max_tree_height", self.max_tree_height.map(length)),
            (
                "max_base_ground_distance",
                self.max_base_ground_distance.map(length),
            ),
            ("search_distance", self.search_distance.map(length)),
            ("fall_in", self.fall_in.map(|v| v.to_string())),
            ("swing_arc", self.swing_arc.map(|v| v.to_string())),
            (
                "cases",
                (!self.cases.is_empty()).then(|| list(self.cases.iter().map(|c| quoted(c)))),
            ),
        ] {
            if let Some(v) = v {
                t.push_str(&format!("{k} = {v}\n"));
            }
        }
        t
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawVegetation {
    #[serde(default)]
    classes: Vec<u8>,
    #[serde(default)]
    clearance: Option<RawRequirement>,
    #[serde(default)]
    growth_per_year: Option<Length>,
    #[serde(default)]
    years: Option<f64>,
    #[serde(default)]
    buffer: Option<Length>,
    #[serde(default)]
    fall_in: Option<bool>,
    #[serde(default)]
    root_ball: Option<f64>,
    #[serde(default)]
    fall_allowance: Option<RawRequirement>,
    #[serde(default)]
    min_tree_height: Option<Length>,
    #[serde(default)]
    max_tree_height: Option<Length>,
    #[serde(default)]
    max_base_ground_distance: Option<Length>,
    #[serde(default)]
    search_distance: Option<Length>,
    #[serde(default)]
    swing_arc: Option<bool>,
    #[serde(default)]
    cases: Vec<String>,
}

impl RawVegetation {
    fn resolve(self) -> Result<VegetationPreset, String> {
        let bad = |m: &str| Err(format!("[vegetation] {m}"));
        let len = |l: Option<Length>| l.map(|l| l.si());
        let non_negative = |v: Option<f64>| v.is_none_or(|v| v.is_finite() && v >= 0.0);
        let p = VegetationPreset {
            classes: self.classes,
            clearance: self
                .clearance
                .map(RawRequirement::resolve)
                .transpose()
                .map_err(|e| format!("[vegetation] clearance: {e}"))?,
            growth_per_year: len(self.growth_per_year),
            years: self.years,
            buffer: len(self.buffer),
            fall_in: self.fall_in,
            root_ball: self.root_ball,
            fall_allowance: self
                .fall_allowance
                .map(RawRequirement::resolve)
                .transpose()
                .map_err(|e| format!("[vegetation] fall_allowance: {e}"))?,
            min_tree_height: len(self.min_tree_height),
            max_tree_height: len(self.max_tree_height),
            max_base_ground_distance: len(self.max_base_ground_distance),
            search_distance: len(self.search_distance),
            swing_arc: self.swing_arc,
            cases: self.cases,
        };
        if ![
            p.growth_per_year,
            p.years,
            p.buffer,
            p.root_ball,
            p.min_tree_height,
            p.max_tree_height,
            p.max_base_ground_distance,
            p.search_distance,
        ]
        .into_iter()
        .all(non_negative)
        {
            return bad("values must be ≥ 0");
        }
        if p.search_distance == Some(0.0) {
            return bad("search_distance must be > 0 m");
        }
        Ok(p)
    }
}

/// What a desktop clearance window solves (`[check]` of a rule file).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CheckScope {
    /// The circuits checked, by circuit name (`""`: the sections without
    /// one); empty: every circuit.
    pub circuits: Vec<String>,
    /// Every section solved by this method (its own condition kept);
    /// `None`: each section's own calculation.
    pub method: Option<Method>,
    /// LiDAR points farther from the wires are ignored, m (`None`: as
    /// far as the rules reach).
    pub search_distance: Option<f64>,
}

impl CheckScope {
    /// Whether it changes nothing (every circuit, each section's method).
    pub fn is_empty(&self) -> bool {
        self.circuits.is_empty() && self.method.is_none() && self.search_distance.is_none()
    }

    /// Whether a section of circuit `name` is checked.
    pub fn takes_circuit(&self, name: &str) -> bool {
        self.circuits.is_empty() || self.circuits.iter().any(|c| c == name)
    }
}

/// A calculation method's name in rule files (`fe`, `rs`).
pub fn method_name(m: Method) -> &'static str {
    match m {
        Method::Fe => "fe",
        Method::Rs => "rs",
    }
}

/// A calculation method from its name in rule files.
pub fn parse_method(text: &str) -> Result<Method, String> {
    match text.trim().to_lowercase().as_str() {
        "fe" => Ok(Method::Fe),
        "rs" => Ok(Method::Rs),
        other => Err(format!(
            "method \"{other}\": use fe (finite elements) or rs (ruling span)"
        )),
    }
}

/// Default horizontal band for vertical clearance to points, m.
pub const DEFAULT_VERTICAL_BAND: f64 = ohl_line::Defaults::PROGRAM.clearance_vertical_band;
/// Default search margin, m.
pub const DEFAULT_SEARCH_MARGIN: f64 = ohl_line::Defaults::PROGRAM.clearance_search_margin;

/// What a rule file's rules take where they give none: the program's
/// ([`Default`]) or a project's default values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RuleDefaults {
    /// `[wind]` step, Pa.
    pub wind_step: f64,
    /// Vertical band to points, m.
    pub vertical_band: f64,
    /// Search margin, m.
    pub search_margin: f64,
}

impl Default for RuleDefaults {
    fn default() -> Self {
        Self {
            wind_step: DEFAULT_WIND_STEP,
            vertical_band: DEFAULT_VERTICAL_BAND,
            search_margin: DEFAULT_SEARCH_MARGIN,
        }
    }
}

impl From<&ohl_line::Defaults> for RuleDefaults {
    fn from(d: &ohl_line::Defaults) -> Self {
        Self {
            wind_step: d.clearance_wind_step,
            vertical_band: d.clearance_vertical_band,
            search_margin: d.clearance_search_margin,
        }
    }
}

impl RuleSet {
    /// Parses and checks a rule set (the program's defaults where a rule
    /// gives none).
    pub fn parse(text: &str) -> Result<Self, String> {
        Self::parse_with(text, &RuleDefaults::default())
    }

    /// Reads a rule file with a project's default values
    /// ([`RuleSet::parse_with`]).
    pub fn load_with(path: &Path, defaults: &RuleDefaults) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse_with(&text, defaults).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Parses and checks a rule set, `defaults` where a rule gives none.
    pub fn parse_with(text: &str, defaults: &RuleDefaults) -> Result<Self, String> {
        let raw: RawRules = toml::from_str(text).map_err(|e| e.to_string())?;
        ohl_core::files::check_header(
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
                let surface = r.surface.as_deref().map(str::trim).map(str::to_owned);
                if surface.as_deref() == Some("") {
                    return bad("surface: give the name of a terrain surface");
                }
                if surface.is_some() && target != Target::Ground {
                    return bad("surface is for ground rules (a terrain surface by name)");
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
                let pair_voltage = match r.pair_voltage.as_deref().map(PairVoltage::parse) {
                    None => PairVoltage::Higher,
                    Some(Ok(p)) => p,
                    Some(Err(e)) => return bad(&e),
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
                let circuits = match r.circuits.as_deref() {
                    None | Some("any") => CircuitFilter::Any,
                    Some("same") => CircuitFilter::Same,
                    Some("different") => CircuitFilter::Different,
                    Some(other) => {
                        return bad(&format!("circuits \"{other}\": use any, same or different"));
                    }
                };
                if r.circuits.is_some() && !matches!(target, Target::SameLine { .. }) {
                    return bad("circuits is for same_line rules");
                }
                let not_negative = |l: Option<Length>| l.is_none_or(|l| l.si() >= 0.0);
                if !not_negative(r.vertical_band)
                    || !not_negative(r.search_margin)
                    || !not_negative(r.vertical_depth)
                {
                    return bad("vertical_band, vertical_depth and search_margin must be ≥ 0");
                }
                if r.voltages.iter().any(|v| !(v.is_finite() && *v >= 0.0)) {
                    return bad("voltages must be ≥ 0 kV");
                }
                let band = if matches!(target, Target::Ground) {
                    0.0
                } else {
                    defaults.vertical_band
                };
                Ok(Rule {
                    name: r.name,
                    target,
                    vertical,
                    radial,
                    pair_voltage,
                    vertical_band: r.vertical_band.map_or(band, |l| l.si()),
                    search_margin: r.search_margin.map_or(defaults.search_margin, |l| l.si()),
                    cases: r.cases,
                    case_pairs: r.case_pairs,
                    exclude_near_ends: r.exclude_near_ends.map_or(0.0, |l| l.si().max(0.0)),
                    exclude_spans_at: r.exclude_spans_at,
                    circuits,
                    voltages: r.voltages,
                    wires: r.wires.as_deref().map_or(Ok(Wires::Both), Wires::parse)?,
                    vertical_depth: r.vertical_depth.map_or(0.0, |l| l.si()),
                    surface,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let wind = raw
            .wind
            .map(|w| -> Result<WindSteps, String> {
                let step = w.step.map_or(defaults.wind_step, |s| s.si());
                if !(step.is_finite() && step > 0.0) {
                    return Err("[wind] step must be > 0 Pa".into());
                }
                let band_margin = w
                    .band_margin
                    .map_or(crate::positions::BAND_MARGIN, |l| l.si());
                if let Some(e) = band_margin_problem(band_margin) {
                    return Err(format!("[wind] {e}"));
                }
                let jumper_band_margin = w
                    .jumper_band_margin
                    .map_or(crate::positions::JUMPER_BAND_MARGIN, |l| l.si());
                if let Some(e) = band_margin_problem(jumper_band_margin) {
                    return Err(format!("[wind] jumper {e}"));
                }
                Ok(WindSteps {
                    band_margin,
                    jumper_band_margin,
                    stepped: w.steps,
                    step,
                    sides: w
                        .sides
                        .as_deref()
                        .map_or(Ok(WindSides::Both), WindSides::parse)?,
                    arc: w.arc,
                })
            })
            .transpose()?;
        let check = raw
            .check
            .map(|c| -> Result<CheckScope, String> {
                let search_distance = c.search_distance.map(|l| l.si());
                if search_distance.is_some_and(|d| !(d.is_finite() && d > 0.0)) {
                    return Err("[check] search_distance must be > 0 m".into());
                }
                Ok(CheckScope {
                    search_distance,
                    circuits: c.circuits,
                    method: c
                        .method
                        .as_deref()
                        .map(parse_method)
                        .transpose()
                        .map_err(|e| format!("[check] {e}"))?,
                })
            })
            .transpose()?;
        let vegetation = raw.vegetation.map(RawVegetation::resolve).transpose()?;
        let checks = raw
            .checks
            .iter()
            .map(|c| PresetCheck::parse(c))
            .collect::<Result<Vec<_>, _>>()?;
        let tension_rules = raw
            .tension_rules
            .into_iter()
            .map(crate::tension::RawTensionRule::resolve)
            .collect::<Result<Vec<_>, _>>()?;
        let string_rules = raw
            .string_rules
            .into_iter()
            .map(crate::strings::RawStringRule::resolve)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            name: raw.name,
            rules,
            wind,
            check,
            vegetation,
            checks,
            tension_rules,
            string_rules,
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
                vertical_band: 0.0,
                search_margin: DEFAULT_SEARCH_MARGIN,
                cases: Vec::new(),
                case_pairs: Vec::new(),
                exclude_near_ends: 0.0,
                exclude_spans_at: Vec::new(),
                circuits: CircuitFilter::Any,
                voltages: Vec::new(),
                wires: Wires::Both,
                vertical_depth: 0.0,
                surface: None,
            }],
            wind: None,
            check: None,
            vegetation: None,
            checks: Vec::new(),
            tension_rules: Vec::new(),
            string_rules: Vec::new(),
        }
    }

    /// Whether it is (a preset) for `check`: as its `checks` declare, else
    /// by its content — the vertical window: a rule with a vertical
    /// distance; the radial window: one with a radial distance; the
    /// vegetation check: a `[vegetation]` table.
    pub fn is_for(&self, check: PresetCheck) -> bool {
        if !self.checks.is_empty() {
            return self.checks.contains(&check);
        }
        match check {
            PresetCheck::Vertical => self.rules.iter().any(|r| r.vertical.is_some()),
            PresetCheck::Radial => self.rules.iter().any(|r| r.radial.is_some()),
            PresetCheck::Vegetation => self.vegetation.is_some(),
            PresetCheck::Tension => !self.tension_rules.is_empty(),
            PresetCheck::Strings => !self.string_rules.is_empty(),
        }
    }

    /// The rule file's text (TOML), read back by [`RuleSet::parse`] as the
    /// same set.
    pub fn to_toml(&self) -> String {
        use std::fmt::Write;
        let mut t = format!(
            "format = \"{RULE_FILE_FORMAT}\"\nversion = {RULE_FILE_VERSION}\nname = {}\n",
            quoted(&self.name)
        );
        if !self.checks.is_empty() {
            let _ = writeln!(
                t,
                "checks = {}",
                list(self.checks.iter().map(|c| quoted(c.as_str())))
            );
        }
        if let Some(w) = &self.wind {
            let _ = write!(
                t,
                "\n# How the desktop clearance windows apply the wind (ohl line: the final state).\n[wind]\nsteps = {}\nstep = \"{} Pa\"\nsides = \"{}\"\n",
                w.stepped,
                w.step,
                w.sides.as_str()
            );
            // Written only when on: a file without it reads as before.
            if w.arc {
                t.push_str("arc = true\n");
            }
            // Written only when not the default (2026-10-21): a file
            // without it is written as before.
            if w.band_margin != crate::positions::BAND_MARGIN {
                let _ = writeln!(t, "band_margin = {}", length(w.band_margin));
            }
            if w.jumper_band_margin != crate::positions::JUMPER_BAND_MARGIN {
                let _ = writeln!(t, "jumper_band_margin = {}", length(w.jumper_band_margin));
            }
        }
        if let Some(c) = self.check.as_ref().filter(|c| !c.is_empty()) {
            t.push_str("\n# What the desktop clearance windows solve (ohl line: every circuit, each section's method).\n[check]\n");
            if !c.circuits.is_empty() {
                let _ = writeln!(
                    t,
                    "circuits = {}",
                    list(c.circuits.iter().map(|n| quoted(n)))
                );
            }
            if let Some(m) = c.method {
                let _ = writeln!(t, "method = \"{}\"", method_name(m));
            }
            if let Some(d) = c.search_distance {
                let _ = writeln!(t, "search_distance = {}", length(d));
            }
        }
        if let Some(v) = &self.vegetation {
            t.push_str("\n# The vegetation and danger-tree check.\n");
            t.push_str(&v.to_toml());
        }
        for r in &self.rules {
            t.push_str(&r.to_toml());
        }
        for r in &self.tension_rules {
            t.push_str(&r.to_toml());
        }
        for r in &self.string_rules {
            t.push_str(&r.to_toml());
        }
        t
    }

    /// Writes the rule file.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        std::fs::write(path, self.to_toml()).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// Horizontal band of vertical clearances when a rule gives none: the
/// ground straight below the wire, points within
/// [`DEFAULT_VERTICAL_BAND`].
pub(crate) fn default_band(target: &Target) -> f64 {
    if matches!(target, Target::Ground) {
        0.0
    } else {
        DEFAULT_VERTICAL_BAND
    }
}

/// `text` as a TOML string.
fn quoted(text: &str) -> String {
    let mut s = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => s.push_str("\\\""),
            '\\' => s.push_str("\\\\"),
            c if c.is_control() => s.push_str(&format!("\\u{:04X}", c as u32)),
            c => s.push(c),
        }
    }
    s.push('"');
    s
}

/// A length in a rule file.
fn length(m: f64) -> String {
    format!("\"{m} m\"")
}

fn list(items: impl Iterator<Item = String>) -> String {
    format!("[{}]", items.collect::<Vec<_>>().join(", "))
}

impl Rule {
    /// The rule as a `[[rule]]` table of a rule file.
    pub fn to_toml(&self) -> String {
        use std::fmt::Write;
        let mut t = format!("\n[[rule]]\nname = {}\n", quoted(&self.name));
        let target = match &self.target {
            Target::Ground => "ground",
            Target::Points { .. } => "points",
            Target::SameLine { .. } => "same_line",
            Target::OtherLines => "other_lines",
        };
        let _ = writeln!(t, "target = \"{target}\"");
        match &self.target {
            Target::Points { classes, roles } => {
                if !classes.is_empty() {
                    let _ = writeln!(t, "classes = {}", list(classes.iter().map(u8::to_string)));
                }
                if !roles.is_empty() {
                    let _ = writeln!(t, "roles = {}", list(roles.iter().map(|r| quoted(r))));
                }
            }
            Target::SameLine {
                include_same_phase: true,
            } => t.push_str("include_same_phase = true\n"),
            _ => {}
        }
        if let Some(s) = &self.surface {
            let _ = writeln!(t, "surface = {}", quoted(s));
        }
        for (key, r) in [("vertical", &self.vertical), ("radial", &self.radial)] {
            if let Some(r) = r {
                let _ = writeln!(t, "{key} = {}", r.to_toml());
            }
        }
        match self.pair_voltage {
            PairVoltage::Higher => {}
            PairVoltage::Sum => t.push_str("pair_voltage = \"sum\"\n"),
            PairVoltage::Difference => t.push_str("pair_voltage = \"difference\"\n"),
        }
        if self.vertical_band != default_band(&self.target) {
            let _ = writeln!(t, "vertical_band = {}", length(self.vertical_band));
        }
        if self.vertical_depth != 0.0 {
            let _ = writeln!(t, "vertical_depth = {}", length(self.vertical_depth));
        }
        if self.search_margin != DEFAULT_SEARCH_MARGIN {
            let _ = writeln!(t, "search_margin = {}", length(self.search_margin));
        }
        if !self.voltages.is_empty() {
            let _ = writeln!(
                t,
                "voltages = {}",
                list(self.voltages.iter().map(|v| format!("{v:?}")))
            );
        }
        if self.wires != Wires::Both {
            let _ = writeln!(t, "wires = \"{}\"", self.wires.as_str());
        }
        if !self.cases.is_empty() {
            let _ = writeln!(t, "cases = {}", list(self.cases.iter().map(|c| quoted(c))));
        }
        if !self.case_pairs.is_empty() {
            let _ = writeln!(
                t,
                "case_pairs = {}",
                list(self.case_pairs.iter().map(|[a, b]| format!(
                    "[{}, {}]",
                    quoted(a),
                    quoted(b)
                )))
            );
        }
        if self.exclude_near_ends != 0.0 {
            let _ = writeln!(t, "exclude_near_ends = {}", length(self.exclude_near_ends));
        }
        if !self.exclude_spans_at.is_empty() {
            let _ = writeln!(
                t,
                "exclude_spans_at = {}",
                list(self.exclude_spans_at.iter().map(|c| quoted(c)))
            );
        }
        match self.circuits {
            CircuitFilter::Any => {}
            CircuitFilter::Same => t.push_str("circuits = \"same\"\n"),
            CircuitFilter::Different => t.push_str("circuits = \"different\"\n"),
        }
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `[wind] arc = true` is read and written back; a file without it
    /// reads as before (no arc), and is written without it.
    #[test]
    fn the_swing_arc_in_the_wind_table() {
        let head = "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"x\"\n";
        let old =
            RuleSet::parse(&format!("{head}[wind]\nsteps = true\nstep = \"50 Pa\"\n")).unwrap();
        let w = old.wind.unwrap();
        assert!(w.stepped && !w.arc);
        assert!(!old.to_toml().contains("arc ="));
        let set = RuleSet::parse(&format!("{head}[wind]\narc = true\nsides = \"left\"\n")).unwrap();
        let w = set.wind.unwrap();
        assert!(w.arc && !w.stepped);
        assert_eq!(w.sides, WindSides::Left);
        let again = RuleSet::parse(&set.to_toml()).unwrap();
        assert_eq!(again.wind, set.wind);
        assert!(set.to_toml().contains("arc = true"));
    }

    /// `[wind] band_margin` (2026-10-21): additive — a file without it
    /// reads the default 0.15 m and is written without it; another value
    /// is read and written back; a negative one is refused.
    #[test]
    fn the_band_margin_in_the_wind_table() {
        let head = "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"x\"\n";
        let old = RuleSet::parse(&format!("{head}[wind]\narc = true\n")).unwrap();
        assert_eq!(old.wind.unwrap().band_margin, crate::positions::BAND_MARGIN);
        assert!(!old.to_toml().contains("band_margin"));
        let set = RuleSet::parse(&format!(
            "{head}[wind]\narc = true\nband_margin = \"0.4 m\"\n"
        ))
        .unwrap();
        assert_eq!(set.wind.unwrap().band_margin, 0.4);
        assert!(set.to_toml().contains("band_margin = \"0.4 m\""));
        assert_eq!(RuleSet::parse(&set.to_toml()).unwrap().wind, set.wind);
        let e = RuleSet::parse(&format!("{head}[wind]\nband_margin = \"-0.1 m\"\n")).unwrap_err();
        assert!(e.contains("band margin"), "{e}");
    }

    /// `[wind] jumper_band_margin` (2026-10-21, the owner's decision): a
    /// value of its own, default 0 — a file without it reads 0 and is
    /// written without it; another value is read and written back,
    /// independent of the spans' margin; a negative one is refused.
    #[test]
    fn the_jumper_band_margin_in_the_wind_table() {
        let head = "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"x\"\n";
        let old = RuleSet::parse(&format!("{head}[wind]\narc = true\n")).unwrap();
        let w = old.wind.unwrap();
        assert_eq!(w.jumper_band_margin, 0.0);
        assert_eq!(w.margins(), crate::positions::BandMargins::default());
        assert!(!old.to_toml().contains("jumper_band_margin"));
        let set = RuleSet::parse(&format!(
            "{head}[wind]\narc = true\njumper_band_margin = \"0.35 m\"\n"
        ))
        .unwrap();
        let w = set.wind.unwrap();
        assert_eq!((w.band_margin, w.jumper_band_margin), (0.15, 0.35));
        assert!(set.to_toml().contains("jumper_band_margin = \"0.35 m\""));
        assert!(!set.to_toml().contains("\nband_margin"));
        assert_eq!(RuleSet::parse(&set.to_toml()).unwrap().wind, set.wind);
        let e = RuleSet::parse(&format!("{head}[wind]\njumper_band_margin = \"-0.1 m\"\n"))
            .unwrap_err();
        assert!(e.contains("jumper band margin"), "{e}");
    }

    /// A rule file read with a project's default values: its search
    /// margin, vertical band and wind step where the file gives none.
    #[test]
    fn project_defaults_fill_what_a_rule_file_leaves_out() {
        let text = "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"x\"\n\
                    [wind]\nsteps = true\n\
                    [[rule]]\nname = \"trees\"\ntarget = \"points\"\nclasses = [5]\nvertical = \"5 m\"\n";
        let program = RuleSet::parse(text).unwrap();
        assert_eq!(program.rules[0].search_margin, DEFAULT_SEARCH_MARGIN);
        let d = RuleDefaults {
            wind_step: 25.0,
            vertical_band: 1.5,
            search_margin: 8.0,
        };
        let mine = RuleSet::parse_with(text, &d).unwrap();
        assert_eq!(mine.rules[0].search_margin, 8.0);
        assert_eq!(mine.rules[0].vertical_band, 1.5);
        assert_eq!(mine.wind.unwrap().step, 25.0);
        let from_line = RuleDefaults::from(&ohl_line::Defaults::default());
        assert_eq!(from_line, RuleDefaults::default());
    }

    /// A ground rule may name a terrain surface (a TIN layer): kept when
    /// written and read again; refused on another target, or empty.
    #[test]
    fn a_ground_rule_names_its_surface() {
        let head = "format = \"ohl-clearance-rules\"\nname = \"t\"\n[[rule]]\nname = \"g\"\n";
        let set = RuleSet::parse(&format!(
            "{head}target = \"ground\"\nsurface = \"TIN 2024\"\nvertical = 7\n"
        ))
        .unwrap();
        assert_eq!(set.rules[0].surface.as_deref(), Some("TIN 2024"));
        let again = RuleSet::parse(&set.to_toml()).unwrap();
        assert_eq!(again, set);
        let none = RuleSet::parse(&format!("{head}target = \"ground\"\nvertical = 7\n")).unwrap();
        assert_eq!(none.rules[0].surface, None);
        assert!(!none.to_toml().contains("surface"));
        let e = RuleSet::parse(&format!(
            "{head}target = \"points\"\nclasses = [5]\nsurface = \"TIN\"\nvertical = 7\n"
        ))
        .unwrap_err();
        assert!(e.contains("surface is for ground rules"), "{e}");
        let e = RuleSet::parse(&format!(
            "{head}target = \"ground\"\nsurface = \" \"\nvertical = 7\n"
        ))
        .unwrap_err();
        assert!(e.contains("name of a terrain surface"), "{e}");
    }

    #[test]
    fn parses_all_targets() {
        let text = r#"
            format = "ohl-clearance-rules"
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
        let cc = RuleSet::parse(
            "[[rule]]\nname = \"cc\"\ntarget = \"same_line\"\nradial = 3.0\ncircuits = \"different\"",
        )
        .unwrap();
        assert_eq!(cc.rules[0].circuits, CircuitFilter::Different);
        assert!(cc.rules[0].circuits.admits("A", "B") && !cc.rules[0].circuits.admits("A", "A"));
        assert!(
            RuleSet::parse(
                "[[rule]]\nname = \"g\"\ntarget = \"ground\"\nvertical = 1.0\ncircuits = \"same\""
            )
            .is_err()
        );
        assert!(
            RuleSet::parse(
                "[[rule]]\nname = \"g\"\ntarget = \"same_line\"\nradial = 1.0\ncircuits = \"other\""
            )
            .is_err()
        );
    }

    /// Phase L1 of a three-phase system of `kv`.
    fn ac(kv: f64) -> Potential {
        Potential {
            nominal_kv: kv,
            kind: ohl_line::electrical::SystemKind::Ac,
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

    /// The three forms of a requirement written and read back exactly
    /// (values that are not round in binary too), and what an editor is
    /// told about one half filled in.
    #[test]
    fn requirement_forms_round_trip() {
        let forms = [
            Requirement::Fixed(7.1),
            Requirement::ByVoltage(vec![(0.0, 5.3), (110.0, 6.05), (420.0, 7.775)]),
            Requirement::Formula {
                base: 5.1,
                per_kv: 0.0123,
                above_kv: 52.5,
            },
        ];
        for f in forms {
            let text = format!(
                "[[rule]]\nname = \"g\"\ntarget = \"ground\"\nvertical = {}\n",
                f.to_toml()
            );
            let set = RuleSet::parse(&text).unwrap();
            assert_eq!(set.rules[0].vertical.as_ref(), Some(&f), "{text}");
            assert_eq!(RuleSet::parse(&set.to_toml()).unwrap(), set);
            assert_eq!(f.problem(), None);
        }
        // Largest over voltages: 0 and 220 kV meet 5.3 and 7.775 m.
        let t = Requirement::ByVoltage(vec![(0.0, 5.3), (110.0, 6.05), (420.0, 7.775)]);
        assert_eq!(t.largest(&[0.0, 220.0]), 7.775);
        assert_eq!(t.largest(&[]), 5.3);
        assert_eq!(t.at_or_last(500.0), 7.775);
        for (bad, why) in [
            (Requirement::Fixed(0.0), "set the distance"),
            (Requirement::ByVoltage(vec![]), "add a row"),
            (
                Requirement::ByVoltage(vec![(110.0, 5.0), (110.0, 6.0)]),
                "increase",
            ),
            (Requirement::ByVoltage(vec![(-1.0, 5.0)]), "≥ 0 kV"),
            (
                Requirement::ByVoltage(vec![(0.0, 5.0), (220.0, 0.0)]),
                "distance",
            ),
            (
                Requirement::Formula {
                    base: 0.0,
                    per_kv: 0.01,
                    above_kv: 0.0,
                },
                "base",
            ),
            (
                Requirement::Formula {
                    base: 5.0,
                    per_kv: -0.01,
                    above_kv: 0.0,
                },
                "per kV",
            ),
        ] {
            let p = bad.problem().unwrap();
            assert!(p.contains(why), "{p}");
        }
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

    /// A preset's checks: told from its content, or as its `checks`
    /// declare (written back, an unknown one refused); `[vegetation]
    /// cases` read and written back.
    #[test]
    fn a_preset_is_for_its_checks() {
        let mixed = "[[rule]]\nname = \"g\"\ntarget = \"ground\"\nvertical = 7.0\n\
                     [vegetation]\nclasses = [5]\ncases = [\"hot\", \"warm*\"]\n";
        let set = RuleSet::parse(mixed).unwrap();
        assert!(set.is_for(PresetCheck::Vertical));
        assert!(!set.is_for(PresetCheck::Radial));
        assert!(set.is_for(PresetCheck::Vegetation));
        assert_eq!(set.vegetation.as_ref().unwrap().cases, ["hot", "warm*"]);
        let back = RuleSet::parse(&set.to_toml()).unwrap();
        assert_eq!(back, set);
        // Declared: only those.
        let declared = RuleSet::parse(&format!("checks = [\"radial\"]\n{mixed}")).unwrap();
        assert_eq!(declared.checks, [PresetCheck::Radial]);
        assert!(declared.is_for(PresetCheck::Radial));
        assert!(!declared.is_for(PresetCheck::Vertical));
        assert!(!declared.is_for(PresetCheck::Vegetation));
        assert_eq!(RuleSet::parse(&declared.to_toml()).unwrap(), declared);
        assert!(RuleSet::parse(&format!("checks = [\"sky\"]\n{mixed}")).is_err());
    }
}
