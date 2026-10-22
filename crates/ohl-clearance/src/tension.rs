// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The **wire tension check** (owner, 2026-10-22): the largest tension a
//! wire may carry, by rules — Checks › Wire tensions… and `ohl tensions`.
//!
//! Each rule ([`TensionRule`], a `[[tension_rule]]` table of a rule file
//! `.ohlr`) names the wires it applies to (wire types, electrical systems,
//! circuits; none: every wire), its weather cases and a **factor of the
//! wire type's rated tensile strength** (RTS): the allowed tension is
//! factor × RTS. The tension compared is by default the **maximum**, at
//! the span's most tensioned support (the larger end tension,
//! `SpanShape::max_tension`), or the **horizontal** tension, as the rule
//! says. Every span of every wire is checked in each case of the rule
//! ([`check`]): tension, allowed, usage (tension / allowed) and pass or
//! fail, sorted by usage, the largest first ([`TensionRow`]).
//!
//! No factor is built in: the factors are the user's or a design code
//! plugin's preset (a preset with `checks = ["tension"]`).

use ohl_line::Line;
use ohl_line::solve::SectionShape;

/// What a tension rule compares with its limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TensionMeasure {
    /// The tension at the span's most tensioned support (the default).
    #[default]
    Max,
    /// The horizontal tension.
    Horizontal,
}

impl TensionMeasure {
    /// `"max"` or `"horizontal"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Max => "max",
            Self::Horizontal => "horizontal",
        }
    }

    /// From its name.
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "max" => Ok(Self::Max),
            "horizontal" => Ok(Self::Horizontal),
            other => Err(format!("measure \"{other}\": use max or horizontal")),
        }
    }

    /// In words, for tables.
    pub fn describe(self) -> &'static str {
        match self {
            Self::Max => "maximum (most tensioned support)",
            Self::Horizontal => "horizontal",
        }
    }
}

/// The wires a tension rule applies to: a wire counts when it matches
/// every list given (empty lists: every wire).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TensionTargets {
    /// Wire type ids (`*` `?` wildcards).
    pub wire_types: Vec<String>,
    /// Electrical system ids (`none`: the sections without a system).
    pub systems: Vec<String>,
    /// Circuit names.
    pub circuits: Vec<String>,
}

impl TensionTargets {
    /// Every wire.
    pub fn is_all(&self) -> bool {
        self.wire_types.is_empty() && self.systems.is_empty() && self.circuits.is_empty()
    }

    /// Whether section `s` of a line is a target.
    pub fn takes(&self, s: &ohl_line::Section) -> bool {
        let types = self.wire_types.is_empty()
            || self
                .wire_types
                .iter()
                .any(|p| ohl_core::pattern::matches(p, &s.cable));
        let systems = self.systems.is_empty()
            || self.systems.iter().any(|x| match &s.system {
                Some(id) => id == x,
                None => x == "none",
            });
        let circuits = self.circuits.is_empty() || self.circuits.contains(&s.circuit_name);
        types && systems && circuits
    }

    /// In words: "every wire", "wire types a, b; circuits North".
    pub fn describe(&self) -> String {
        if self.is_all() {
            return "every wire".into();
        }
        let mut parts = Vec::new();
        for (what, list) in [
            ("wire types", &self.wire_types),
            ("systems", &self.systems),
            ("circuits", &self.circuits),
        ] {
            if !list.is_empty() {
                parts.push(format!("{what} {}", list.join(", ")));
            }
        }
        parts.join("; ")
    }
}

/// One rule of the wire tension check (`[[tension_rule]]`).
#[derive(Debug, Clone, PartialEq)]
pub struct TensionRule {
    /// Name, for reports.
    pub name: String,
    /// Weather cases (names, `*` `?` wildcards); empty: every case.
    pub cases: Vec<String>,
    /// The allowed tension as a factor of the wire type's RTS (above 0).
    pub factor: f64,
    /// The tension compared.
    pub measure: TensionMeasure,
    /// The wires it applies to.
    pub targets: TensionTargets,
}

impl TensionRule {
    /// A rule for every wire in every case, its factor to set (0).
    pub fn blank(name: &str) -> Self {
        Self {
            name: name.into(),
            cases: Vec::new(),
            factor: 0.0,
            measure: TensionMeasure::Max,
            targets: TensionTargets::default(),
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

    /// What is wrong with it (none: nothing).
    pub fn problem(&self) -> Option<String> {
        (!(self.factor.is_finite() && self.factor > 0.0))
            .then(|| format!("\"{}\": the factor of the RTS must be above 0", self.name))
    }

    /// The rule as a `[[tension_rule]]` table of a rule file.
    pub fn to_toml(&self) -> String {
        use std::fmt::Write;
        let q = |t: &str| toml::Value::String(t.to_owned()).to_string();
        let list = |v: &[String]| {
            format!(
                "[{}]",
                v.iter().map(|x| q(x)).collect::<Vec<_>>().join(", ")
            )
        };
        let mut t = format!("\n[[tension_rule]]\nname = {}\n", q(&self.name));
        if !self.cases.is_empty() {
            let _ = writeln!(t, "cases = {}", list(&self.cases));
        }
        let _ = writeln!(t, "factor = {:?}", self.factor);
        if self.measure != TensionMeasure::Max {
            let _ = writeln!(t, "measure = \"{}\"", self.measure.as_str());
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

/// A `[[tension_rule]]` table as read.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawTensionRule {
    name: String,
    #[serde(default)]
    cases: Vec<String>,
    factor: f64,
    #[serde(default)]
    measure: Option<String>,
    #[serde(default)]
    targets: Option<RawTargets>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTargets {
    #[serde(default)]
    wire_types: Vec<String>,
    #[serde(default)]
    systems: Vec<String>,
    #[serde(default)]
    circuits: Vec<String>,
}

impl RawTensionRule {
    pub(crate) fn resolve(self) -> Result<TensionRule, String> {
        let t = self.targets.unwrap_or(RawTargets {
            wire_types: Vec::new(),
            systems: Vec::new(),
            circuits: Vec::new(),
        });
        let rule = TensionRule {
            measure: self
                .measure
                .as_deref()
                .map_or(Ok(TensionMeasure::Max), TensionMeasure::parse)
                .map_err(|e| format!("tension rule \"{}\": {e}", self.name))?,
            name: self.name,
            cases: self.cases,
            factor: self.factor,
            targets: TensionTargets {
                wire_types: t.wire_types,
                systems: t.systems,
                circuits: t.circuits,
            },
        };
        match rule.problem() {
            Some(e) => Err(format!("tension rule {e}")),
            None => Ok(rule),
        }
    }
}

/// One span of one wire in one weather case under one rule.
#[derive(Debug, Clone, PartialEq)]
pub struct TensionRow {
    /// Index of the rule.
    pub rule: usize,
    /// The weather case.
    pub case: String,
    /// The section.
    pub section: String,
    /// The span, from 0.
    pub span: usize,
    /// Its wire type.
    pub wire_type: String,
    /// The tension compared (the rule's measure, per wire), N.
    pub tension: f64,
    /// The wire type's RTS, N.
    pub rts: f64,
    /// The allowed tension, factor × RTS, N.
    pub allowed: f64,
    /// Tension / allowed.
    pub usage: f64,
    /// Where it is marked: the most tensioned support (the higher end of
    /// the span; for the horizontal tension too).
    pub at: [f64; 3],
}

impl TensionRow {
    /// Whether the tension is within the allowed.
    pub fn passes(&self) -> bool {
        self.tension <= self.allowed
    }
}

/// The result of a check.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TensionReport {
    /// Every span checked, the largest usage first.
    pub rows: Vec<TensionRow>,
    /// What was not checked, and why.
    pub notes: Vec<String>,
}

impl TensionReport {
    /// The rows above their allowed tension.
    pub fn failures(&self) -> impl Iterator<Item = &TensionRow> {
        self.rows.iter().filter(|r| !r.passes())
    }

    /// The points marked in the views: every span above its allowed
    /// tension, else the worst span (the largest usage).
    pub fn marks(&self) -> Vec<[f64; 3]> {
        let failed: Vec<[f64; 3]> = self.failures().map(|r| r.at).collect();
        if failed.is_empty() {
            self.rows.first().map(|r| vec![r.at]).unwrap_or_default()
        } else {
            failed
        }
    }

    /// The results as CSV (one row per span, case and rule; the largest
    /// usage first).
    pub fn csv(&self, rules: &[TensionRule]) -> String {
        use std::fmt::Write;
        let mut out = String::from(
            "rule,case,section,span,wire_type,measure,tension_n,rts_n,factor,allowed_n,usage_pct,status,x,y,z\n",
        );
        for r in &self.rows {
            let rule = &rules[r.rule];
            let _ = writeln!(
                out,
                "{},{},{},{},{},{},{:.1},{:.1},{},{:.1},{:.2},{},{:.3},{:.3},{:.3}",
                ohl_core::text::csv_field(&rule.name),
                ohl_core::text::csv_field(&r.case),
                ohl_core::text::csv_field(&r.section),
                r.span + 1,
                ohl_core::text::csv_field(&r.wire_type),
                rule.measure.as_str(),
                r.tension,
                r.rts,
                rule.factor,
                r.allowed,
                100.0 * r.usage,
                if r.passes() { "pass" } else { "fail" },
                r.at[0],
                r.at[1],
                r.at[2],
            );
        }
        out
    }
}

/// Checks `solved` — per weather case (its name), the line's sections
/// solved in it (`SectionShape::section` naming the section) — against
/// `rules`, the RTS of each section's wire type from `library`. A wire
/// type without an RTS (or not in the library) is not checked (a note).
pub fn check(
    line: &Line,
    library: &ohl_cables::Library,
    solved: &[(&str, &[SectionShape])],
    rules: &[TensionRule],
) -> TensionReport {
    let mut report = TensionReport::default();
    let mut noted = std::collections::BTreeSet::new();
    for (k, rule) in rules.iter().enumerate() {
        if let Some(e) = rule.problem() {
            report.notes.push(format!("{e}: not checked"));
            continue;
        }
        let mut any = false;
        for (case, shapes) in solved {
            if !rule.applies(case) {
                continue;
            }
            any = true;
            for shape in shapes.iter() {
                let Some(section) = line.sections.iter().find(|s| s.id == shape.section) else {
                    continue;
                };
                if !rule.targets.takes(section) {
                    continue;
                }
                let rts = library
                    .get(&section.cable)
                    .map_or(0.0, |c| c.rated_tensile_strength.si());
                if !(rts.is_finite() && rts > 0.0) {
                    if noted.insert(section.cable.clone()) {
                        report.notes.push(format!(
                            "wire type {}: no rated tensile strength, not checked",
                            section.cable
                        ));
                    }
                    continue;
                }
                let allowed = rule.factor * rts;
                for (j, span) in shape.spans.iter().enumerate() {
                    let tension = match rule.measure {
                        TensionMeasure::Max => span.max_tension,
                        TensionMeasure::Horizontal => span.horizontal_tension,
                    };
                    let (first, last) = (
                        span.points.first().copied().unwrap_or(span.mid_point),
                        span.points.last().copied().unwrap_or(span.mid_point),
                    );
                    report.rows.push(TensionRow {
                        rule: k,
                        case: (*case).to_owned(),
                        section: section.id.clone(),
                        span: j,
                        wire_type: section.cable.clone(),
                        tension,
                        rts,
                        allowed,
                        usage: tension / allowed,
                        at: if last[2] > first[2] { last } else { first },
                    });
                }
            }
        }
        if !any {
            report.notes.push(format!(
                "\"{}\": none of its weather cases is solved",
                rule.name
            ));
        }
    }
    report.rows.sort_by(|a, b| b.usage.total_cmp(&a.usage));
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::RuleSet;

    /// On the example line, still air at 15 °C, an INVENTED factor 0.2 of
    /// the RTS (not from any code): every span checked once per rule; the
    /// maximum tension of a span by hand — at its higher support the
    /// catenary's T = H + w (z_support − z_low), w the wire's weight per
    /// metre (the conserved weight differs by its strain, 0.1 % here) —
    /// gives its usage T / (0.2 RTS); the horizontal measure H / (0.2 RTS)
    /// exactly; sorted by usage; the marks at the higher support.
    #[test]
    fn usage_by_hand_on_the_example_line() {
        use ohl_core::units::Temperature;
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let line =
            Line::load(&root.join("examples/synthetic-line/synthetic-line.ohl.json")).unwrap();
        let library = ohl_cables::Library::load(&root.join("libraries/cables")).unwrap();
        let case = ohl_weather::WeatherCase::still("everyday", Temperature::from_celsius(15.0));
        let shapes: Vec<SectionShape> = ohl_line::solve::solve_line(&line, &library, &case)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        let rules = vec![
            TensionRule {
                factor: 0.2,
                ..TensionRule::blank("max, invented 0.2")
            },
            TensionRule {
                factor: 0.2,
                measure: TensionMeasure::Horizontal,
                cases: vec!["every*".into()],
                ..TensionRule::blank("horizontal, invented 0.2")
            },
            TensionRule {
                factor: 0.2,
                cases: vec!["ice".into()],
                ..TensionRule::blank("not in this case")
            },
        ];
        let report = check(&line, &library, &[("everyday", &shapes)], &rules);
        let spans: usize = shapes.iter().map(|s| s.spans.len()).sum();
        assert_eq!(report.rows.len(), 2 * spans);
        assert!(report.notes.iter().any(|n| n.contains("not in this case")));
        assert!(report.rows.windows(2).all(|w| w[0].usage >= w[1].usage));
        let section = &line.sections[0];
        let cable = library.get(&section.cable).unwrap();
        let rts = cable.rated_tensile_strength.si();
        let w = cable.weight().si();
        let span = &shapes[0].spans[0];
        let (a, b) = (span.points[0], *span.points.last().unwrap());
        let high = if b[2] > a[2] { b } else { a };
        let by_hand = span.horizontal_tension + w * (high[2] - span.low_point[2]);
        let row = |rule: usize| {
            report
                .rows
                .iter()
                .find(|r| r.rule == rule && r.section == section.id && r.span == 0)
                .unwrap()
        };
        let max = row(0);
        assert!(
            (max.usage - by_hand / (0.2 * rts)).abs() < 0.005 * max.usage,
            "{} against {}",
            max.usage,
            by_hand / (0.2 * rts)
        );
        assert_eq!(max.at, high);
        assert_eq!(max.allowed, 0.2 * rts);
        assert_eq!(max.passes(), max.usage <= 1.0);
        let h = row(1);
        assert_eq!(h.usage, span.horizontal_tension / (0.2 * rts));
        assert!(h.usage < max.usage);
        let csv = report.csv(&rules);
        assert!(csv.starts_with("rule,case,section,span,wire_type,measure,tension_n"));
        assert_eq!(csv.lines().count(), 1 + 2 * spans);
        // Every span within 0.2 RTS at 15 °C: the worst marked.
        assert_eq!(report.failures().count(), 0);
        assert_eq!(report.marks(), vec![report.rows[0].at]);
        // A factor so small that every span fails: every span marked.
        let tight = vec![TensionRule {
            factor: 0.01,
            ..TensionRule::blank("tight")
        }];
        let r = check(&line, &library, &[("everyday", &shapes)], &tight);
        assert_eq!(r.failures().count(), spans);
        assert_eq!(r.marks().len(), spans);
    }

    /// A rule file with tension rules reads and writes them back; one
    /// without reads as before (none).
    #[test]
    fn tension_rules_in_a_rule_file() {
        let text = "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"t\"\n\
            [[tension_rule]]\nname = \"ice\"\ncases = [\"ice*\"]\nfactor = 0.5\n\
            [[tension_rule]]\nname = \"everyday H\"\nfactor = 0.2\nmeasure = \"horizontal\"\n\
            targets = { wire_types = [\"acsr-*\"], circuits = [\"North\"] }\n";
        let set = RuleSet::parse(text).unwrap();
        assert!(set.rules.is_empty());
        assert_eq!(set.tension_rules.len(), 2);
        let (a, b) = (&set.tension_rules[0], &set.tension_rules[1]);
        assert_eq!((a.factor, a.measure), (0.5, TensionMeasure::Max));
        assert!(a.applies("ice heavy") && !a.applies("everyday"));
        assert!(a.targets.is_all());
        assert_eq!(b.measure, TensionMeasure::Horizontal);
        assert_eq!(b.targets.circuits, ["North"]);
        assert!(set.is_for(crate::rules::PresetCheck::Tension));
        assert!(!set.is_for(crate::rules::PresetCheck::Vertical));
        let back = RuleSet::parse(&set.to_toml()).unwrap();
        assert_eq!(back.tension_rules, set.tension_rules);
        // Old files: no tension rule.
        let old = RuleSet::parse(
            "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"o\"\n[[rule]]\nname = \"g\"\n\
             target = \"ground\"\nvertical = \"7 m\"\n",
        )
        .unwrap();
        assert!(old.tension_rules.is_empty());
        assert!(!old.is_for(crate::rules::PresetCheck::Tension));
        // A factor of 0 or below is refused; a wrong measure too.
        for bad in ["factor = 0.0", "factor = 0.5\nmeasure = \"mean\""] {
            let e = RuleSet::parse(&format!(
                "format = \"ohl-clearance-rules\"\nversion = 1\n[[tension_rule]]\nname = \"x\"\n{bad}\n"
            ))
            .unwrap_err();
            assert!(e.contains("tension rule"), "{e}");
        }
    }
}
