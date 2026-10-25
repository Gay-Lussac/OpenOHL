// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The **string check** (owner, 2026-10-24): every string as solved in the
//! weather cases of the rules — Checks › String checks… and `ohl strings`.
//!
//! Each rule ([`StringRule`], a `[[string_rule]]` table of a rule file
//! `.ohlr`, which a design-code plugin's preset may bring: `checks =
//! ["strings"]`) names its weather cases and its strings (suspension,
//! strain or all; the voltages of their sections), and what is checked:
//!
//! - **load**: the string's axial force (tension) against **its own
//!   maximum load** (`max_load`, the string's, else its insulator's; a
//!   plain comparison). A string without one is not checked (counted in
//!   the notes).
//! - **swing**: its swing as solved (from straight down; across and
//!   along the line in its structure's axes, either side) against **its
//!   own swing limits** where it has them, and against the rule's own
//!   (`max_swing_deg`, `max_swing_across_deg`, `max_swing_along_deg`)
//!   where the rule gives them: each limit given is checked, a row each.
//! - **uplift**: a suspension string (not a dead end) that **goes slack**
//!   — pushed while it takes tension only — or, one that may push (a
//!   compression limit given), pushed beyond that limit (owner: some
//!   strings hang upside down by design, so not the geometry).
//!
//! Every string at every point of every section is checked in each case
//! of the rule ([`check`]): the value, the limit, the usage (value /
//! limit) and pass or fail, the largest usage first ([`StringRow`]). No
//! limit is built in: they are the strings' own and the rules'.

use std::collections::BTreeSet;

use ohl_line::Line;
use ohl_line::solve::SectionShape;

/// Which strings a rule applies to by kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StringKind {
    /// Every string (the default).
    #[default]
    All,
    /// Suspension strings (not dead ends).
    Suspension,
    /// Dead ends (tension strings).
    Strain,
}

impl StringKind {
    /// `"all"`, `"suspension"` or `"strain"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Suspension => "suspension",
            Self::Strain => "strain",
        }
    }

    /// From its name.
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "all" => Ok(Self::All),
            "suspension" => Ok(Self::Suspension),
            "strain" => Ok(Self::Strain),
            other => Err(format!(
                "strings \"{other}\": use all, suspension or strain"
            )),
        }
    }

    /// In words, for tables.
    pub fn describe(self) -> &'static str {
        match self {
            Self::All => "every string",
            Self::Suspension => "suspension strings",
            Self::Strain => "strain strings (dead ends)",
        }
    }

    /// Whether a string (a dead end or not) is of this kind.
    pub fn takes(self, strain: bool) -> bool {
        match self {
            Self::All => true,
            Self::Suspension => !strain,
            Self::Strain => strain,
        }
    }
}

/// One rule of the string check (`[[string_rule]]`).
#[derive(Debug, Clone, PartialEq)]
pub struct StringRule {
    /// Name, for reports.
    pub name: String,
    /// Weather cases (names, `*` `?` wildcards); empty: every case.
    pub cases: Vec<String>,
    /// The strings by kind.
    pub strings: StringKind,
    /// Only the strings of sections of these nominal voltages, kV (0: an
    /// earth wire's); empty: every string.
    pub voltages: Vec<f64>,
    /// Check the strings' own maximum load.
    pub load: bool,
    /// Check the strings' own swing limits.
    pub swing: bool,
    /// The rule's swing limit from straight down, degrees (none: not).
    pub max_swing_deg: Option<f64>,
    /// The rule's swing limit across the line, either side, degrees.
    pub max_swing_across_deg: Option<f64>,
    /// The rule's swing limit along the line, either way, degrees.
    pub max_swing_along_deg: Option<f64>,
    /// Check suspension strings for uplift (going slack, or pushed beyond
    /// their compression limit).
    pub uplift: bool,
}

impl StringRule {
    /// A rule for every string in every case checking the strings' own
    /// limits and uplift.
    pub fn blank(name: &str) -> Self {
        Self {
            name: name.into(),
            cases: Vec::new(),
            strings: StringKind::All,
            voltages: Vec::new(),
            load: true,
            swing: true,
            max_swing_deg: None,
            max_swing_across_deg: None,
            max_swing_along_deg: None,
            uplift: true,
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

    /// Whether it applies to a section of nominal voltage `kv` (none: 0).
    pub fn takes_voltage(&self, kv: Option<f64>) -> bool {
        let kv = kv.unwrap_or(0.0);
        self.voltages.is_empty() || self.voltages.iter().any(|v| (v - kv).abs() < 1e-6)
    }

    /// The rule's own swing limits: (what, limit).
    fn own_swings(&self) -> impl Iterator<Item = (Measure, f64)> {
        [
            (Measure::Swing, self.max_swing_deg),
            (Measure::SwingAcross, self.max_swing_across_deg),
            (Measure::SwingAlong, self.max_swing_along_deg),
        ]
        .into_iter()
        .filter_map(|(m, v)| v.map(|v| (m, v)))
    }

    /// What is wrong with it (none: nothing).
    pub fn problem(&self) -> Option<String> {
        let bad = self
            .own_swings()
            .any(|(_, v)| !(v.is_finite() && v > 0.0 && v <= 180.0));
        if bad {
            return Some(format!(
                "\"{}\": a swing limit must be above 0° and at most 180°",
                self.name
            ));
        }
        let nothing =
            !self.load && !self.swing && !self.uplift && self.own_swings().next().is_none();
        nothing.then(|| format!("\"{}\": checks nothing", self.name))
    }

    /// What it checks, in words: "load, swing (own; ≤ 60° across),
    /// uplift".
    pub fn describe_checks(&self) -> String {
        let mut parts = Vec::new();
        if self.load {
            parts.push("load".to_owned());
        }
        let mut swing = Vec::new();
        if self.swing {
            swing.push("the strings' own".to_owned());
        }
        for (m, v) in self.own_swings() {
            swing.push(format!("≤ {v}° {}", m.short()));
        }
        if !swing.is_empty() {
            parts.push(format!("swing ({})", swing.join("; ")));
        }
        if self.uplift {
            parts.push("uplift".to_owned());
        }
        parts.join(", ")
    }

    /// The rule as a `[[string_rule]]` table of a rule file.
    pub fn to_toml(&self) -> String {
        use std::fmt::Write;
        let q = |t: &str| toml::Value::String(t.to_owned()).to_string();
        let mut t = format!("\n[[string_rule]]\nname = {}\n", q(&self.name));
        if !self.cases.is_empty() {
            let list: Vec<String> = self.cases.iter().map(|c| q(c)).collect();
            let _ = writeln!(t, "cases = [{}]", list.join(", "));
        }
        if self.strings != StringKind::All {
            let _ = writeln!(t, "strings = \"{}\"", self.strings.as_str());
        }
        if !self.voltages.is_empty() {
            let list: Vec<String> = self.voltages.iter().map(|v| format!("{v:?}")).collect();
            let _ = writeln!(t, "voltages = [{}]", list.join(", "));
        }
        for (k, v) in [
            ("load", self.load),
            ("swing", self.swing),
            ("uplift", self.uplift),
        ] {
            if !v {
                let _ = writeln!(t, "{k} = false");
            }
        }
        for (k, v) in [
            ("max_swing_deg", self.max_swing_deg),
            ("max_swing_across_deg", self.max_swing_across_deg),
            ("max_swing_along_deg", self.max_swing_along_deg),
        ] {
            if let Some(v) = v {
                let _ = writeln!(t, "{k} = {v:?}");
            }
        }
        t
    }
}

fn yes() -> bool {
    true
}

/// A `[[string_rule]]` table as read.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawStringRule {
    name: String,
    #[serde(default)]
    cases: Vec<String>,
    #[serde(default)]
    strings: Option<String>,
    #[serde(default)]
    voltages: Vec<f64>,
    #[serde(default = "yes")]
    load: bool,
    #[serde(default = "yes")]
    swing: bool,
    #[serde(default)]
    max_swing_deg: Option<f64>,
    #[serde(default)]
    max_swing_across_deg: Option<f64>,
    #[serde(default)]
    max_swing_along_deg: Option<f64>,
    #[serde(default = "yes")]
    uplift: bool,
}

impl RawStringRule {
    pub(crate) fn resolve(self) -> Result<StringRule, String> {
        let rule = StringRule {
            strings: self
                .strings
                .as_deref()
                .map_or(Ok(StringKind::All), StringKind::parse)
                .map_err(|e| format!("string rule \"{}\": {e}", self.name))?,
            name: self.name,
            cases: self.cases,
            voltages: self.voltages,
            load: self.load,
            swing: self.swing,
            max_swing_deg: self.max_swing_deg,
            max_swing_across_deg: self.max_swing_across_deg,
            max_swing_along_deg: self.max_swing_along_deg,
            uplift: self.uplift,
        };
        match rule.problem() {
            Some(e) => Err(format!("string rule {e}")),
            None => Ok(rule),
        }
    }
}

/// What a row compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Measure {
    /// The axial force against the maximum load, N.
    Load,
    /// The swing from straight down, degrees.
    Swing,
    /// The swing across the line, degrees (either side).
    SwingAcross,
    /// The swing along the line, degrees (either way).
    SwingAlong,
    /// A suspension string slack or pushed beyond its compression limit.
    Uplift,
}

impl Measure {
    /// A short name: "load", "swing", "across", "along", "uplift".
    pub fn short(self) -> &'static str {
        match self {
            Self::Load => "load",
            Self::Swing => "swing",
            Self::SwingAcross => "across",
            Self::SwingAlong => "along",
            Self::Uplift => "uplift",
        }
    }

    /// In words, for tables.
    pub fn describe(self) -> &'static str {
        match self {
            Self::Load => "load",
            Self::Swing => "swing from vertical",
            Self::SwingAcross => "swing across the line",
            Self::SwingAlong => "swing along the line",
            Self::Uplift => "uplift",
        }
    }
}

/// Where a row's limit comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The string's own (or its insulator's).
    String,
    /// The rule's.
    Rule,
}

/// One string in one weather case, one limit, under one rule.
#[derive(Debug, Clone, PartialEq)]
pub struct StringRow {
    /// Index of the rule.
    pub rule: usize,
    /// The weather case.
    pub case: String,
    /// The section, and the index of its point the string holds.
    pub section: String,
    /// Index of the point in the section.
    pub point: usize,
    /// The structure.
    pub structure: String,
    /// The string's label on it.
    pub string: String,
    /// A dead end.
    pub strain: bool,
    /// What is compared.
    pub measure: Measure,
    /// Where its limit comes from.
    pub source: Source,
    /// The value: the force, N (tension positive), or the swing, degrees
    /// (its magnitude); uplift: the force.
    pub value: f64,
    /// The limit: N or degrees; uplift: 0 (failing: slack, or pushed
    /// beyond the compression it may take).
    pub limit: f64,
    /// Value / limit; uplift: 1 when slack or beyond (else 0).
    pub usage: f64,
    /// Whether it is within the limit.
    pub pass: bool,
    /// Where it is marked: the string's wire point, as solved.
    pub at: [f64; 3],
}

/// The result of a check.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StringReport {
    /// Every limit checked, failures first, then the largest usage.
    pub rows: Vec<StringRow>,
    /// What was not checked, and why.
    pub notes: Vec<String>,
}

impl StringReport {
    /// The rows beyond their limit.
    pub fn failures(&self) -> impl Iterator<Item = &StringRow> {
        self.rows.iter().filter(|r| !r.pass)
    }

    /// The points marked in the views: every string beyond a limit (each
    /// once), else the worst row's.
    pub fn marks(&self) -> Vec<[f64; 3]> {
        let mut failed: Vec<[f64; 3]> = Vec::new();
        for r in self.failures() {
            if !failed.contains(&r.at) {
                failed.push(r.at);
            }
        }
        if failed.is_empty() {
            self.rows.first().map(|r| vec![r.at]).unwrap_or_default()
        } else {
            failed
        }
    }

    /// The results as CSV (one row per string, case, limit and rule).
    pub fn csv(&self, rules: &[StringRule]) -> String {
        use std::fmt::Write;
        let q = ohl_core::text::csv_field;
        let mut out = String::from(
            "rule,case,structure,string,kind,section,point,measure,limit_from,value,limit,unit,usage_pct,status,x,y,z\n",
        );
        for r in &self.rows {
            let unit = match r.measure {
                Measure::Load | Measure::Uplift => "N",
                _ => "deg",
            };
            let _ = writeln!(
                out,
                "{},{},{},{},{},{},{},{},{},{:.2},{:.2},{unit},{:.2},{},{:.3},{:.3},{:.3}",
                q(&rules[r.rule].name),
                q(&r.case),
                q(&r.structure),
                q(&r.string),
                if r.strain { "strain" } else { "suspension" },
                q(&r.section),
                r.point + 1,
                r.measure.short(),
                match r.source {
                    Source::String => "string",
                    Source::Rule => "rule",
                },
                r.value,
                r.limit,
                100.0 * r.usage,
                if r.pass { "pass" } else { "fail" },
                r.at[0],
                r.at[1],
                r.at[2],
            );
        }
        out
    }
}

/// Checks `solved` — per weather case (its name), the line's sections
/// solved in it — against `rules`. A string shared by two sections (a
/// TAK's) is checked once per case, rule and limit (the larger usage).
pub fn check(
    line: &Line,
    solved: &[(&str, &[SectionShape])],
    rules: &[StringRule],
    filter: &ohl_weather::CaseFilter,
) -> StringReport {
    let mut report = StringReport::default();
    let kv: Vec<Option<f64>> = crate::case::voltages_of(line)
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.map(|p| p.nominal_kv))
        .collect();
    for (k, rule) in rules.iter().enumerate() {
        if let Some(e) = rule.problem() {
            report.notes.push(format!("{e}: not checked"));
            continue;
        }
        let mut any = false;
        let mut no_load: BTreeSet<(String, String)> = BTreeSet::new();
        let mut no_swing: BTreeSet<(String, String)> = BTreeSet::new();
        for (case, shapes) in solved {
            if !rule.applies(case) {
                continue;
            }
            any = true;
            let mut rows: Vec<StringRow> = Vec::new();
            for shape in shapes.iter() {
                let Some(si) = line.sections.iter().position(|s| s.id == shape.section) else {
                    continue;
                };
                if !rule.takes_voltage(kv.get(si).copied().flatten()) {
                    continue;
                }
                let section = &line.sections[si];
                for (p, strings) in shape.strings.iter().enumerate() {
                    let structure = section
                        .points
                        .get(p)
                        .map_or(String::new(), |x| x.structure.clone());
                    // The overrides: the structure's and the section's own
                    // groups' cases only.
                    if !filter.takes(Some(&structure), Some(&section.id), case) {
                        continue;
                    }
                    for st in strings {
                        if !rule.strings.takes(st.strain) {
                            continue;
                        }
                        let key = (structure.clone(), st.label.clone());
                        let row =
                            |measure, source, value: f64, limit: f64, usage: f64, pass| StringRow {
                                rule: k,
                                case: (*case).to_owned(),
                                section: section.id.clone(),
                                point: p,
                                structure: structure.clone(),
                                string: st.label.clone(),
                                strain: st.strain,
                                measure,
                                source,
                                value,
                                limit,
                                usage,
                                pass,
                                at: st.bottom,
                            };
                        if rule.load {
                            match st.limits.max_load {
                                Some(max) if max > 0.0 => rows.push(row(
                                    Measure::Load,
                                    Source::String,
                                    st.force,
                                    max,
                                    st.force / max,
                                    st.force <= max,
                                )),
                                _ => {
                                    no_load.insert(key.clone());
                                }
                            }
                        }
                        let swing = |m: Measure| match m {
                            Measure::Swing => st.swing.total_deg.abs(),
                            Measure::SwingAcross => st.swing.across_deg.abs(),
                            _ => st.swing.along_deg.abs(),
                        };
                        if rule.swing {
                            let own = [
                                (Measure::Swing, st.limits.max_swing_deg),
                                (Measure::SwingAcross, st.limits.max_swing_across_deg),
                                (Measure::SwingAlong, st.limits.max_swing_along_deg),
                            ];
                            if own.iter().all(|(_, v)| v.is_none()) {
                                no_swing.insert(key.clone());
                            }
                            for (m, v) in own {
                                if let Some(max) = v.filter(|v| *v > 0.0) {
                                    let a = swing(m);
                                    rows.push(row(m, Source::String, a, max, a / max, a <= max));
                                }
                            }
                        }
                        for (m, max) in rule.own_swings() {
                            let a = swing(m);
                            rows.push(row(m, Source::Rule, a, max, a / max, a <= max));
                        }
                        if rule.uplift && !st.strain {
                            let fails = st.slack || st.over;
                            rows.push(row(
                                Measure::Uplift,
                                Source::String,
                                st.force,
                                0.0,
                                if fails { 1.0 } else { 0.0 },
                                !fails,
                            ));
                        }
                    }
                }
            }
            // A string held by two sections (a TAK's): once, the larger
            // usage kept.
            rows.sort_by(|a, b| {
                (&a.structure, &a.string, a.measure, a.source as u8)
                    .cmp(&(&b.structure, &b.string, b.measure, b.source as u8))
                    .then(b.usage.total_cmp(&a.usage))
            });
            rows.dedup_by(|b, a| {
                (&a.structure, &a.string, a.measure, a.source)
                    == (&b.structure, &b.string, b.measure, b.source)
            });
            report.rows.extend(rows);
        }
        if !any {
            report.notes.push(format!(
                "\"{}\": none of its weather cases is solved",
                rule.name
            ));
            continue;
        }
        if !no_load.is_empty() {
            report.notes.push(format!(
                "\"{}\": {} string(s) without a maximum load: their load not checked",
                rule.name,
                no_load.len()
            ));
        }
        if !no_swing.is_empty() && rule.swing {
            report.notes.push(format!(
                "\"{}\": {} string(s) without swing limits of their own{}",
                rule.name,
                no_swing.len(),
                if rule.own_swings().next().is_some() {
                    " (the rule's checked)"
                } else {
                    ": their swing not checked"
                }
            ));
        }
    }
    report
        .rows
        .sort_by(|a, b| a.pass.cmp(&b.pass).then(b.usage.total_cmp(&a.usage)));
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{PresetCheck, RuleSet};
    use ohl_core::units::{Stress, Temperature};
    use ohl_weather::WeatherCase;

    fn solved(line: &Line, case: &WeatherCase) -> Vec<SectionShape> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let library = ohl_cables::Library::load(&root.join("libraries/cables")).unwrap();
        ohl_line::solve::solve_line(line, &library, case)
            .into_iter()
            .map(Result::unwrap)
            .collect()
    }

    fn example() -> Line {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        Line::load(&root.join("examples/synthetic-line/synthetic-line.ohl.json")).unwrap()
    }

    /// The example line with INVENTED limits (not from any code): every
    /// string given its own maximum load far above its force, one of them
    /// 1 N (it fails); a rule's limit across the line between the largest
    /// suspension string swing in still air and in the wind: passed in
    /// still air, failed in the wind (dead ends not taken by a suspension
    /// rule: the strings at the sections' ends); a slack suspension string is uplift;
    /// the strings without swing limits of their own said.
    #[test]
    fn load_swing_and_uplift_on_the_example_line() {
        let mut line = example();
        let mut first = None;
        for s in &mut line.structures {
            for st in &mut s.strings {
                let one = first.is_none();
                st.max_load = Some(if one { 1.0 } else { 1e9 });
                if one {
                    first = Some((s.id.clone(), st.label.clone()));
                }
            }
        }
        let (fs, fl) = first.expect("the example has explicit strings");
        let still = WeatherCase::still("still", Temperature::from_celsius(10.0));
        let mut windy = WeatherCase::still("wind", Temperature::from_celsius(10.0));
        windy.wind_pressure = Stress::from_pascals(500.0);
        let (a, b) = (solved(&line, &still), solved(&line, &windy));
        // The largest swing across of a suspension string in each case:
        // the rule's limit between them.
        let most = |shapes: &[SectionShape]| {
            shapes
                .iter()
                .flat_map(|s| s.strings.iter().flatten())
                .filter(|s| !s.strain)
                .map(|s| s.swing.across_deg.abs())
                .fold(0.0f64, f64::max)
        };
        let (calm, blown) = (most(&a), most(&b));
        assert!(blown > calm + 1.0, "{calm} {blown}");
        let limit = (calm + blown) / 2.0;
        let rules = vec![
            StringRule::blank("own limits"),
            StringRule {
                strings: StringKind::Suspension,
                load: false,
                swing: false,
                uplift: false,
                max_swing_across_deg: Some(limit),
                ..StringRule::blank("invented limit across")
            },
        ];
        let r = check(
            &line,
            &[("still", &a), ("wind", &b)],
            &rules,
            &Default::default(),
        );
        // The load: the one at 1 N fails in both cases, the others pass.
        let loads: Vec<&StringRow> = r
            .rows
            .iter()
            .filter(|x| x.measure == Measure::Load)
            .collect();
        assert!(!loads.is_empty());
        for x in &loads {
            let one = (x.structure.as_str(), x.string.as_str()) == (fs.as_str(), fl.as_str());
            assert_eq!(x.pass, !one, "{x:?}");
            assert!((x.usage - x.value / x.limit).abs() < 1e-12);
        }
        // The rule's swing: suspension strings only, passed in still air,
        // some failed in the wind.
        let swings: Vec<&StringRow> = r
            .rows
            .iter()
            .filter(|x| x.measure == Measure::SwingAcross)
            .collect();
        assert!(swings.iter().all(|x| !x.strain && x.source == Source::Rule));
        assert!(swings.iter().filter(|x| x.case == "still").all(|x| x.pass));
        assert!(swings.iter().any(|x| x.case == "wind" && !x.pass));
        // Uplift: none slack on the example line.
        assert!(
            r.rows
                .iter()
                .filter(|x| x.measure == Measure::Uplift)
                .all(|x| x.pass)
        );
        assert!(
            r.notes.iter().any(|n| n.contains("without swing limits")),
            "{:?}",
            r.notes
        );
        // Failures first, then the largest usage.
        let first_pass = r.rows.iter().position(|x| x.pass).unwrap();
        assert!(r.rows[..first_pass].iter().all(|x| !x.pass));
        assert!(
            r.rows[first_pass..]
                .windows(2)
                .all(|w| w[0].usage >= w[1].usage)
        );
        assert!(r.csv(&rules).starts_with("rule,case,structure,string,kind"));
        // A suspension string gone slack: uplift.
        let mut c = a.clone();
        let st = c
            .iter_mut()
            .flat_map(|s| s.strings.iter_mut().flatten())
            .find(|s| !s.strain)
            .unwrap();
        st.slack = true;
        let label = st.label.clone();
        let r = check(
            &line,
            &[("still", &c)],
            &[StringRule::blank("u")],
            &Default::default(),
        );
        let up: Vec<&StringRow> = r
            .rows
            .iter()
            .filter(|x| x.measure == Measure::Uplift && !x.pass)
            .collect();
        assert_eq!(up.len(), 1, "{up:?}");
        assert_eq!(up[0].string, label);
        assert!(r.marks().contains(&up[0].at));
    }

    /// String rules in a rule file read and write back; defaults (all
    /// strings, the strings' own limits and uplift checked); bad values
    /// refused.
    #[test]
    fn string_rules_in_a_rule_file() {
        let text = "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"s\"\n\
            [[string_rule]]\nname = \"own\"\n\
            [[string_rule]]\nname = \"wind\"\ncases = [\"wind*\"]\nstrings = \"suspension\"\n\
            voltages = [220.0]\nload = false\nmax_swing_across_deg = 60.0\n";
        let set = RuleSet::parse(text).unwrap();
        assert_eq!(set.string_rules.len(), 2);
        let (a, b) = (&set.string_rules[0], &set.string_rules[1]);
        assert_eq!(a, &StringRule::blank("own"));
        assert_eq!(b.strings, StringKind::Suspension);
        assert!(!b.load && b.swing && b.uplift);
        assert_eq!(b.max_swing_across_deg, Some(60.0));
        assert!(b.takes_voltage(Some(220.0)) && !b.takes_voltage(Some(380.0)));
        assert!(b.applies("wind 500") && !b.applies("still"));
        assert!(set.is_for(PresetCheck::Strings));
        let back = RuleSet::parse(&set.to_toml()).unwrap();
        assert_eq!(back.string_rules, set.string_rules);
        for bad in [
            "strings = \"pin\"",
            "max_swing_deg = 0.0",
            "load = false\nswing = false\nuplift = false",
        ] {
            let e = RuleSet::parse(&format!(
                "format = \"ohl-clearance-rules\"\nversion = 1\n[[string_rule]]\nname = \"x\"\n{bad}\n"
            ))
            .unwrap_err();
            assert!(e.contains("string rule"), "{e}");
        }
    }
}
