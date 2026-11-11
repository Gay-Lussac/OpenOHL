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
//! In place of the factor a rule may give (roadmap O22 item 3, rule file
//! version 2) an absolute **tension** (`tension = "25 kN"`) or a largest
//! **catenary constant** (`catenary_constant = "1800 m"`): the horizontal
//! tension over the bare weight per metre of the wire as strung
//! ([`ohl_line::solve::strung_cable`]) — also in an iced case, where the
//! ratio to the iced load would be smaller: on the safe side.
//!
//! **Sag to limits** ([`sag_to_limits`]): the largest stringing tension of
//! a section in the blocks that keeps every rule, found by bisection —
//! a command whose result is written back as the section's stringing,
//! never a stringing re-solved on every change.
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
    /// How the wires are solved for it (2026-11-01; none: each section's
    /// own calculation; [`crate::calculation`]).
    pub calculation: crate::calculation::Calc,
    /// The allowed tension as a factor of the wire type's RTS (above 0;
    /// 0 with one of the two limits below).
    pub factor: f64,
    /// In place of the factor: the largest catenary constant, m — the
    /// horizontal tension over the bare weight per metre of the wire as
    /// strung (the measure is then the horizontal tension).
    pub catenary_constant: Option<f64>,
    /// In place of the factor: the allowed tension, N.
    pub tension: Option<f64>,
    /// The tension compared.
    pub measure: TensionMeasure,
    /// The wires it applies to.
    pub targets: TensionTargets,
}

/// What a tension rule limits the tension to ([`TensionRule::limit`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RuleLimit {
    /// A factor of the wire type's rated tensile strength.
    Factor(f64),
    /// A catenary constant H / w, m (w: the bare weight per metre of the
    /// wire as strung).
    CatenaryConstant(f64),
    /// A tension, N.
    Tension(f64),
}

impl RuleLimit {
    /// In words: "0.4 × RTS", "H/w ≤ 1800 m", "25.00 kN".
    pub fn describe(self) -> String {
        match self {
            Self::Factor(f) => format!("{f} × RTS"),
            Self::CatenaryConstant(c) => format!("H/w ≤ {c} m"),
            Self::Tension(t) => format!("{:.2} kN", t / 1000.0),
        }
    }
}

impl TensionRule {
    /// A rule for every wire in every case, its factor to set (0).
    pub fn blank(name: &str) -> Self {
        Self {
            name: name.into(),
            cases: Vec::new(),
            calculation: None,
            factor: 0.0,
            catenary_constant: None,
            tension: None,
            measure: TensionMeasure::Max,
            targets: TensionTargets::default(),
        }
    }

    /// Its limit: the catenary constant or the tension when given, else
    /// the factor of the RTS.
    pub fn limit(&self) -> RuleLimit {
        match (self.catenary_constant, self.tension) {
            (Some(c), _) => RuleLimit::CatenaryConstant(c),
            (None, Some(t)) => RuleLimit::Tension(t),
            (None, None) => RuleLimit::Factor(self.factor),
        }
    }

    /// The tension compared: the rule's measure; the horizontal tension
    /// for a catenary constant.
    pub fn measured(&self) -> TensionMeasure {
        match self.limit() {
            RuleLimit::CatenaryConstant(_) => TensionMeasure::Horizontal,
            _ => self.measure,
        }
    }

    /// The allowed tension, N, for a wire of rated tensile strength `rts`
    /// (N; 0: none given) and bare weight per metre as strung `weight`
    /// (N/m); none: a factor of an RTS the wire type does not give.
    pub fn allowed(&self, rts: f64, weight: f64) -> Option<f64> {
        match self.limit() {
            RuleLimit::Factor(f) => (rts.is_finite() && rts > 0.0).then_some(f * rts),
            RuleLimit::CatenaryConstant(c) => Some(c * weight),
            RuleLimit::Tension(t) => Some(t),
        }
    }

    /// Whether it applies to weather case `case`.
    pub fn applies(&self, case: &str) -> bool {
        ohl_core::pattern::matches_list(&self.cases, case)
    }

    /// What is wrong with it (none: nothing).
    pub fn problem(&self) -> Option<String> {
        let above_zero = |v: f64| v.is_finite() && v > 0.0;
        let given = usize::from(self.factor != 0.0)
            + usize::from(self.catenary_constant.is_some())
            + usize::from(self.tension.is_some());
        if given > 1 {
            return Some(format!(
                "\"{}\": give one limit — a factor of the RTS, a catenary constant or a tension",
                self.name
            ));
        }
        let (what, value) = match self.limit() {
            RuleLimit::Factor(f) => ("factor of the RTS", f),
            RuleLimit::CatenaryConstant(c) => ("catenary constant", c),
            RuleLimit::Tension(t) => ("tension", t),
        };
        (!above_zero(value)).then(|| format!("\"{}\": the {what} must be above 0", self.name))
    }

    /// Whether it needs rule file version 2 (a catenary constant or a
    /// tension in place of the factor).
    pub fn since_version_2(&self) -> bool {
        self.catenary_constant.is_some() || self.tension.is_some()
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
        t.push_str(&crate::calculation::toml_line(self.calculation));
        match self.limit() {
            RuleLimit::Factor(f) => {
                let _ = writeln!(t, "factor = {f:?}");
            }
            RuleLimit::CatenaryConstant(c) => {
                let _ = writeln!(t, "catenary_constant = \"{c:?} m\"");
            }
            RuleLimit::Tension(n) => {
                let _ = writeln!(t, "tension = \"{n:?} N\"");
            }
        }
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
    #[serde(default)]
    calculation: Option<ohl_line::condition::Calculation>,
    #[serde(default)]
    factor: Option<f64>,
    #[serde(default)]
    catenary_constant: Option<ohl_core::units::Length>,
    #[serde(default)]
    tension: Option<ohl_core::units::Force>,
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
        let given = usize::from(self.factor.is_some())
            + usize::from(self.catenary_constant.is_some())
            + usize::from(self.tension.is_some());
        if given != 1 {
            return Err(format!(
                "tension rule \"{}\": give one limit — factor (of the RTS), catenary_constant \
                 or tension",
                self.name
            ));
        }
        if self.catenary_constant.is_some() && self.measure.as_deref() == Some("max") {
            return Err(format!(
                "tension rule \"{}\": a catenary constant limits the horizontal tension \
                 (leave measure out)",
                self.name
            ));
        }
        let rule = TensionRule {
            measure: self
                .measure
                .as_deref()
                .map_or(Ok(TensionMeasure::Max), TensionMeasure::parse)
                .map_err(|e| format!("tension rule \"{}\": {e}", self.name))?,
            name: self.name,
            cases: self.cases,
            calculation: self.calculation,
            factor: self.factor.unwrap_or(0.0),
            catenary_constant: self.catenary_constant.map(|c| c.si()),
            tension: self.tension.map(|t| t.si()),
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
    /// The calculation the wires were solved by (none: each section's
    /// own).
    pub calculation: crate::calculation::Calc,
    /// The section.
    pub section: String,
    /// The span, from 0.
    pub span: usize,
    /// Its wire type.
    pub wire_type: String,
    /// The tension compared (the rule's measure, per wire), N.
    pub tension: f64,
    /// The wire type's RTS, N (0: none given).
    pub rts: f64,
    /// The allowed tension, N: factor × RTS, the rule's tension, or its
    /// catenary constant × the wire's bare weight per metre as strung.
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
            "rule,case,section,span,wire_type,measure,tension_n,rts_n,factor,allowed_n,usage_pct,status,x,y,z,limit\n",
        );
        for r in &self.rows {
            let rule = &rules[r.rule];
            let _ = writeln!(
                out,
                "{},{},{},{},{},{},{:.1},{:.1},{},{:.1},{:.2},{},{:.3},{:.3},{:.3},{}",
                ohl_core::text::csv_field(&rule.name),
                ohl_core::text::csv_field(&crate::calculation::label(&r.case, r.calculation)),
                ohl_core::text::csv_field(&r.section),
                r.span + 1,
                ohl_core::text::csv_field(&r.wire_type),
                rule.measured().as_str(),
                r.tension,
                r.rts,
                rule.factor,
                r.allowed,
                100.0 * r.usage,
                if r.passes() { "pass" } else { "fail" },
                r.at[0],
                r.at[1],
                r.at[2],
                ohl_core::text::csv_field(&rule.limit().describe()),
            );
        }
        out
    }
}

/// Checks `solved` — per weather case (its name), the line's sections
/// solved in it (`SectionShape::section` naming the section) — against
/// `rules`, the RTS of each section's wire type from `library`. A wire
/// type without an RTS (or not in the library) is not checked (a note).
/// The notes start with the bimetallic model of the line's wire types
/// ([`ohl_line::bimetallic_notes`]).
pub fn check(
    line: &Line,
    library: &ohl_cables::Library,
    solved: &[(&str, crate::calculation::Calc, &[SectionShape])],
    rules: &[TensionRule],
    filter: &ohl_weather::CaseFilter,
) -> TensionReport {
    // The bimetallic model of the wire types (roadmap O22 item 1): above
    // the knee point the default's tension is the lowest of the models.
    let mut report = TensionReport {
        notes: ohl_line::bimetallic_notes(line, library),
        ..TensionReport::default()
    };
    let mut noted = std::collections::BTreeSet::new();
    for (k, rule) in rules.iter().enumerate() {
        if let Some(e) = rule.problem() {
            report.notes.push(format!("{e}: not checked"));
            continue;
        }
        let mut any = false;
        for (case, calc, shapes) in solved {
            if !rule.applies(case) || rule.calculation != *calc {
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
                let wire = WireLimits::of(line, library, section);
                if rule.allowed(wire.rts, wire.weight).is_none() {
                    if noted.insert(section.cable.clone()) {
                        report.notes.push(format!(
                            "wire type {}: no rated tensile strength, not checked",
                            section.cable
                        ));
                    }
                    continue;
                }
                span_rows(
                    (k, rule),
                    (case, *calc),
                    section,
                    wire,
                    shape,
                    filter,
                    &mut report.rows,
                );
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

/// What a tension rule's limit needs of a section's wire.
#[derive(Debug, Clone, Copy)]
struct WireLimits {
    /// Rated tensile strength, N (0: none, or the wire type unknown).
    rts: f64,
    /// Bare weight per metre of the wire as strung in the section, N/m.
    weight: f64,
}

impl WireLimits {
    fn of(line: &Line, library: &ohl_cables::Library, section: &ohl_line::Section) -> Self {
        let Some(cable) = library.get(&section.cable) else {
            return Self {
                rts: 0.0,
                weight: 0.0,
            };
        };
        let own = line.bimetallic.apply(cable);
        let weight = ohl_line::solve::strung_cable(section, &own)
            .map_or(cable.weight().si(), |c| c.weight().si());
        Self {
            rts: cable.rated_tensile_strength.si(),
            weight,
        }
    }
}

/// The rows of one section solved (`shape`) in one weather case and
/// calculation under one rule (its index and the rule), pushed to `rows`:
/// every span the `filter` takes — the one implementation of the check
/// ([`check`]) and of sagging to limits ([`sag_to_limits`]). Nothing for a
/// rule whose limit the wire cannot give (a factor without an RTS).
fn span_rows(
    (index, rule): (usize, &TensionRule),
    (case, calculation): (&str, crate::calculation::Calc),
    section: &ohl_line::Section,
    wire: WireLimits,
    shape: &SectionShape,
    filter: &ohl_weather::CaseFilter,
    rows: &mut Vec<TensionRow>,
) {
    let Some(allowed) = rule.allowed(wire.rts, wire.weight) else {
        return;
    };
    for (j, span) in shape.spans.iter().enumerate() {
        // The overrides: the section's and its ends' groups.
        let end = |k: usize| section.points.get(k).map_or("", |p| p.structure.as_str());
        if !filter.takes_span(&section.id, [end(j), end(j + 1)], case) {
            continue;
        }
        let tension = match rule.measured() {
            TensionMeasure::Max => span.max_tension,
            TensionMeasure::Horizontal => span.horizontal_tension,
        };
        let (first, last) = (
            span.points.first().copied().unwrap_or(span.mid_point),
            span.points.last().copied().unwrap_or(span.mid_point),
        );
        rows.push(TensionRow {
            rule: index,
            case: case.to_owned(),
            calculation,
            section: section.id.clone(),
            span: j,
            wire_type: section.cable.clone(),
            tension,
            rts: wire.rts,
            allowed,
            usage: tension / allowed,
            at: if last[2] > first[2] { last } else { first },
        });
    }
}

/// Seconds a section may take when sagged to limits (the default of
/// `ohl tensions --sag-to-limits`): about 40 trial tensions, each one
/// solve of the section per weather case and calculation of the rules.
pub const SAG_TO_LIMITS_SECONDS: u64 = 60;

/// What governs a section sagged to limits: the row with the largest
/// usage at the tension found.
#[derive(Debug, Clone, PartialEq)]
pub struct Governing {
    /// Index of the rule.
    pub rule: usize,
    /// Its weather case.
    pub case: String,
    /// The calculation it was solved by (none: the section's own).
    pub calculation: crate::calculation::Calc,
    /// The span, from 0.
    pub span: usize,
    /// Tension / allowed there.
    pub usage: f64,
}

/// A section sagged to the limits of the tension rules
/// ([`sag_to_limits`]).
#[derive(Debug, Clone, PartialEq)]
pub struct SagToLimits {
    /// The section.
    pub section: String,
    /// The stringing found (a horizontal tension at the section's
    /// stringing temperature, in its stringing's condition); none: not
    /// sagged, `notes` say why.
    pub stringing: Option<ohl_line::Stringing>,
    /// What governs at the tension found (or, stopped early, so far).
    pub governing: Option<Governing>,
    /// Every rule taking the section: its index and largest usage at the
    /// tension found.
    pub usages: Vec<(usize, f64)>,
    /// Trial tensions solved.
    pub trials: usize,
    /// Whether the search stopped on its time limit (the tension given is
    /// then valid, not the tightest).
    pub stopped: bool,
    /// What the reader must know: why it was not sagged, where it stopped.
    pub notes: Vec<String>,
}

/// The outcome of [`sag_to_limits`].
#[derive(Debug, Clone, PartialEq)]
pub struct SaggedToLimits {
    /// Per section asked, in the line's order.
    pub sections: Vec<SagToLimits>,
    /// The line with the stringings found (the other sections as they
    /// were).
    pub line: Line,
    /// What every reader of the result must know.
    pub notes: Vec<String>,
}

impl SaggedToLimits {
    /// The sections given a stringing: (section id, stringing).
    pub fn stringings(&self) -> impl Iterator<Item = (&str, &ohl_line::Stringing)> {
        self.sections
            .iter()
            .filter_map(|s| Some((s.section.as_str(), s.stringing.as_ref()?)))
    }
}

/// Sags sections of `line` to the limits of `rules`: for each section in
/// the blocks among `sections` (ids; empty: every section), the largest
/// horizontal tension at its stringing temperature and condition
/// ([`ohl_line::StrungAt`]) at which no span exceeds a rule taking it, in
/// the weather cases `weather` each rule applies to and by its calculation
/// (none: the section's own).
///
/// The usage of a trial tension is the largest tension / allowed over
/// those rules, cases and spans ([`check`]'s own rows). It rises with the
/// stringing tension, so the tension is bracketed (from 0.1 × RTS, halved
/// or doubled, never beyond the RTS) and bisected to 10⁻⁶ relative; the
/// result is the bracket's low end rounded down to a newton and solved
/// once more — every rule at or below its limit there, whatever the sets
/// of a wire with curves do (a rule above its limit steps it down, at
/// most 10 times).
///
/// Each section is sagged alone (no shared assembly, no spacer pulls it:
/// the others as they are); check the whole line afterwards
/// ([`check`] on [`SaggedToLimits::line`]). The result is the **tightest**
/// line the rules allow: the least sag — and the highest loads on the
/// structures; clearances are to be checked after it, which the notes say.
///
/// Exit clauses: `budget` per section (its time limit, cancellation): the
/// bracket's low end is then given, valid but not the tightest, and said;
/// 60 trials at most.
///
/// Expected cost: about 40 trials per section, each one section solve per
/// weather case and calculation of the rules (10–100 ms for 10 spans; a
/// stringing after creep or load works its initial tension back in each).
pub fn sag_to_limits(
    line: &Line,
    library: &ohl_cables::Library,
    rules: &[TensionRule],
    weather: &[ohl_weather::WeatherCase],
    sections: &[String],
    filter: &ohl_weather::CaseFilter,
    budget: &ohl_core::budget::Budget,
) -> SaggedToLimits {
    let mut out = SaggedToLimits {
        sections: Vec::new(),
        line: line.clone(),
        notes: vec![
            "sagged to the limits: the tightest line the tension rules allow — the least sag \
             and the highest loads on the structures; check the clearances and the structures \
             after it"
                .to_owned(),
            "each section sagged alone (sections sharing an assembly or a spacer: the others \
             as they are): check the wire tensions of the whole line afterwards"
                .to_owned(),
        ],
    };
    for (k, rule) in rules.iter().enumerate() {
        if let Some(e) = rule.problem() {
            out.notes.push(format!("{e}: rule {} left out", k + 1));
        }
    }
    for section in &line.sections {
        if !sections.is_empty() && !sections.contains(&section.id) {
            continue;
        }
        let sagged = sag_section(line, library, rules, weather, section, filter, budget);
        if let (Some(s), Some(target)) = (
            &sagged.stringing,
            out.line.sections.iter_mut().find(|s| s.id == section.id),
        ) {
            target.stringing = s.clone();
        }
        out.sections.push(sagged);
    }
    for id in sections {
        if !line.sections.iter().any(|s| &s.id == id) {
            out.notes.push(format!("no section {id}"));
        }
    }
    out
}

/// One section of [`sag_to_limits`].
fn sag_section(
    line: &Line,
    library: &ohl_cables::Library,
    rules: &[TensionRule],
    weather: &[ohl_weather::WeatherCase],
    section: &ohl_line::Section,
    filter: &ohl_weather::CaseFilter,
    budget: &ohl_core::budget::Budget,
) -> SagToLimits {
    let mut result = SagToLimits {
        section: section.id.clone(),
        stringing: None,
        governing: None,
        usages: Vec::new(),
        trials: 0,
        stopped: false,
        notes: Vec::new(),
    };
    if section.stringing.clipped() {
        result
            .notes
            .push("clipped in: only a section in the blocks is sagged".into());
        return result;
    }
    let wire = WireLimits::of(line, library, section);
    // The rules taking it whose limit its wire can give, and what to
    // solve for them.
    let taking: Vec<(usize, &TensionRule)> = rules
        .iter()
        .enumerate()
        .filter(|(_, r)| r.problem().is_none() && r.targets.takes(section))
        .filter(|(_, r)| r.allowed(wire.rts, wire.weight).is_some())
        .collect();
    let names: Vec<&str> = weather.iter().map(|c| c.name.as_str()).collect();
    let solves = crate::calculation::solves(
        &names,
        &taking,
        |(_, r), c| r.applies(c),
        |(_, r)| r.calculation,
    );
    if solves.is_empty() {
        result.notes.push(
            "no tension rule limits it in the weather cases given (its wire type, its cases, \
             a factor without a rated tensile strength): not sagged"
                .into(),
        );
        return result;
    }
    let (temperature, condition) = (
        section.stringing.temperature(),
        section.stringing.condition(),
    );
    let stringing = |tension: f64| ohl_line::Stringing::Tension {
        horizontal_tension: tension,
        temperature,
        condition,
    };
    // The rows of the section strung at a trial tension.
    let mut trial = section.clone();
    let trials = std::cell::Cell::new(0usize);
    let mut rows_at = |tension: f64| -> Result<Vec<TensionRow>, String> {
        trials.set(trials.get() + 1);
        trial.stringing = stringing(tension);
        let mut rows = Vec::new();
        for &(k, calc) in &solves {
            trial.calculation = calc.unwrap_or(section.calculation);
            let shape = ohl_line::solve::solve_one(line, &trial, library, &weather[k])
                .map_err(|e| e.to_string())?;
            for &(index, rule) in &taking {
                if rule.calculation == calc && rule.applies(names[k]) {
                    span_rows(
                        (index, rule),
                        (names[k], calc),
                        &trial,
                        wire,
                        &shape,
                        filter,
                        &mut rows,
                    );
                }
            }
        }
        Ok(rows)
    };
    let worst = |rows: &[TensionRow]| rows.iter().map(|r| r.usage).fold(0.0, f64::max);
    let run = budget.start();
    // The bracket: within the rules at `lo`, beyond them at `hi`.
    let ceiling = if wire.rts.is_finite() && wire.rts > 0.0 {
        wire.rts
    } else {
        f64::INFINITY
    };
    let start = if ceiling.is_finite() {
        0.1 * ceiling
    } else {
        10_000.0
    };
    let mut lo = start;
    let mut kept: Option<Vec<TensionRow>> = None;
    let mut last_error = None;
    for _ in 0..12 {
        match rows_at(lo) {
            Ok(rows) if rows.is_empty() => {
                result
                    .notes
                    .push("none of its spans is checked in these cases: not sagged".into());
                result.trials = trials.get();
                return result;
            }
            Ok(rows) if worst(&rows) <= 1.0 => {
                kept = Some(rows);
                break;
            }
            Ok(_) => {}
            Err(e) => last_error = Some(e),
        }
        lo /= 2.0;
    }
    let Some(mut kept) = kept else {
        result.notes.push(match last_error {
            Some(e) => format!("not sagged: not solved at {:.2} kN: {e}", 2.0 * lo / 1000.0),
            None => format!(
                "not sagged: the rules are not met even at {:.2} kN",
                2.0 * lo / 1000.0
            ),
        });
        result.trials = trials.get();
        return result;
    };
    let mut hi = lo;
    let mut bounded = false;
    while !bounded && trials.get() < 30 {
        if hi >= ceiling {
            break;
        }
        hi = (hi * 2.0).min(ceiling);
        match rows_at(hi) {
            Ok(rows) if worst(&rows) <= 1.0 => {
                (lo, kept) = (hi, rows);
            }
            // Beyond the rules, or not solvable there: the high end.
            Ok(_) | Err(_) => bounded = true,
        }
    }
    if !bounded {
        result.notes.push(format!(
            "not sagged: the rules do not limit the stringing tension below {:.2} kN{}",
            hi / 1000.0,
            if ceiling.is_finite() {
                " (the wire's rated tensile strength)"
            } else {
                ""
            }
        ));
        result.trials = trials.get();
        return result;
    }
    while hi - lo > 1e-6 * hi && trials.get() < 60 {
        if let Err(e) = run.check() {
            result.stopped = true;
            result.notes.push(format!(
                "{e}: stopped at [{lo:.1}, {hi:.1}] N — the low end is a valid stringing, not \
                 the tightest"
            ));
            break;
        }
        let mid = 0.5 * (lo + hi);
        match rows_at(mid) {
            Ok(rows) if worst(&rows) <= 1.0 => (lo, kept) = (mid, rows),
            Ok(_) | Err(_) => hi = mid,
        }
    }
    // Written as whole newtons, solved there once more; a rule above its
    // limit (a set growing faster than the tension) steps it down.
    let mut tension = lo.floor().max(1.0);
    for _ in 0..10 {
        match rows_at(tension) {
            Ok(rows) if worst(&rows) <= 1.0 + 1e-8 => {
                kept = rows;
                break;
            }
            _ => tension = (tension * (1.0 - 1e-6)).floor(),
        }
    }
    if worst(&kept) > 1.0 + 1e-8 {
        result
            .notes
            .push("not sagged: no tension just below the limit keeps every rule".into());
        result.trials = trials.get();
        return result;
    }
    result.trials = trials.get();
    result.stringing = Some(stringing(tension));
    result.governing = kept
        .iter()
        .max_by(|a, b| a.usage.total_cmp(&b.usage))
        .map(|r| Governing {
            rule: r.rule,
            case: r.case.clone(),
            calculation: r.calculation,
            span: r.span,
            usage: r.usage,
        });
    result.usages = taking
        .iter()
        .map(|&(index, _)| {
            let usage = kept
                .iter()
                .filter(|r| r.rule == index)
                .map(|r| r.usage)
                .fold(0.0, f64::max);
            (index, usage)
        })
        .collect();
    result
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
        let report = check(
            &line,
            &library,
            &[("everyday", None, &shapes)],
            &rules,
            &Default::default(),
        );
        let spans: usize = shapes.iter().map(|s| s.spans.len()).sum();
        assert_eq!(report.rows.len(), 2 * spans);
        // The section override (owner, 2026-10-25): the first section's
        // groups lack "everyday": its spans left out; the structure
        // override with a structure's groups lacking it: the spans at it.
        let groups = vec![ohl_weather::CaseGroup {
            name: "snow".into(),
            description: String::new(),
            cases: vec!["heavy snow".into()],
            ..Default::default()
        }];
        let snow = ["snow".to_owned()];
        let first = &line.sections[0];
        let filter = ohl_weather::CaseFilter {
            sections: Some(ohl_weather::OwnCases::new(
                [(first.id.as_str(), &snow[..])],
                &groups,
            )),
            structures: None,
        };
        let r = check(
            &line,
            &library,
            &[("everyday", None, &shapes)],
            &rules,
            &filter,
        );
        let first_spans = first.points.len() - 1;
        assert_eq!(r.rows.len(), 2 * (spans - first_spans));
        assert!(r.rows.iter().all(|x| x.section != first.id));
        let at = first.points[0].structure.clone();
        let filter = ohl_weather::CaseFilter {
            structures: Some(ohl_weather::OwnCases::new(
                [(at.as_str(), &snow[..])],
                &groups,
            )),
            sections: None,
        };
        let r = check(
            &line,
            &library,
            &[("everyday", None, &shapes)],
            &rules,
            &filter,
        );
        assert!(r.rows.len() < 2 * spans);
        assert!(r.rows.iter().all(|x| {
            let s = line.sections.iter().find(|s| s.id == x.section).unwrap();
            s.points[x.span].structure != at && s.points[x.span + 1].structure != at
        }));
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
        let r = check(
            &line,
            &library,
            &[("everyday", None, &shapes)],
            &tight,
            &Default::default(),
        );
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

#[cfg(test)]
mod sag_tests {
    use super::*;
    use crate::rules::RuleSet;
    use ohl_core::units::Temperature;
    use ohl_line::{Stringing, StrungAt};

    fn example() -> (Line, ohl_cables::Library) {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        (
            Line::load(&root.join("examples/synthetic-line/synthetic-line.ohl.json")).unwrap(),
            ohl_cables::Library::load(&root.join("libraries/cables")).unwrap(),
        )
    }

    fn budget() -> ohl_core::budget::Budget {
        ohl_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(120))
    }

    /// By hand: a limit on the horizontal tension in the stringing state
    /// (still air at the stringing temperature, the wire alone) is the
    /// stringing tension itself — INVENTED limits, not from any code: a
    /// factor 0.2 of the RTS gives 0.2 × RTS, a tension of 21 kN gives
    /// 21 kN, a catenary constant gives C × w (w the bare weight per metre
    /// as strung); each rounded down to a newton. A rule in another case
    /// that does not bind changes nothing; the section's kind and value
    /// before do not matter.
    #[test]
    fn the_stringing_state_gives_back_its_limit() {
        let (mut line, library) = example();
        // The wire alone, on one tension through the section: the
        // stringing state is then exactly the value given.
        line.jumpers.clear();
        line.solver.blocks = ohl_line::InTheBlocks::EqualH;
        let id = line.sections[0].id.clone();
        line.sections[0].clear_point_loads();
        line.sections[0].stringing = Stringing::Sag {
            sag: 12.0,
            temperature: 15.0,
            span: None,
            condition: StrungAt::Initial,
        };
        let cable = library.get(&line.sections[0].cable).unwrap();
        let rts = cable.rated_tensile_strength.si();
        let w = ohl_line::solve::strung_cable(&line.sections[0], cable)
            .unwrap()
            .weight()
            .si();
        let weather = [
            ohl_weather::WeatherCase::still("everyday", Temperature::from_celsius(15.0)),
            ohl_weather::WeatherCase::still("cold", Temperature::from_celsius(-20.0)),
        ];
        let horizontal = |name: &str| TensionRule {
            cases: vec!["everyday".into()],
            measure: TensionMeasure::Horizontal,
            ..TensionRule::blank(name)
        };
        // Loose in the cold case: never governs.
        let loose = TensionRule {
            factor: 0.95,
            cases: vec!["cold".into()],
            ..TensionRule::blank("cold, loose")
        };
        for (rule, by_hand) in [
            (
                TensionRule {
                    factor: 0.2,
                    ..horizontal("factor")
                },
                0.2 * rts,
            ),
            (
                TensionRule {
                    tension: Some(21_000.0),
                    ..horizontal("tension")
                },
                21_000.0,
            ),
            (
                TensionRule {
                    catenary_constant: Some(1500.0),
                    ..TensionRule::blank("catenary constant")
                },
                1500.0 * w,
            ),
        ] {
            let mut rule = rule;
            if rule.catenary_constant.is_some() {
                rule.cases = vec!["everyday".into()];
            }
            let rules = [rule, loose.clone()];
            let out = sag_to_limits(
                &line,
                &library,
                &rules,
                &weather,
                std::slice::from_ref(&id),
                &Default::default(),
                &budget(),
            );
            assert_eq!(out.sections.len(), 1);
            let s = &out.sections[0];
            let Some(Stringing::Tension {
                horizontal_tension,
                temperature,
                condition,
            }) = &s.stringing
            else {
                panic!("{}: {:?}", rules[0].name, s.notes);
            };
            // Within 0.1 % of the hand value, never above it: the spans'
            // tensions differ a little where the strings settle (6 N in
            // 25 kN, `tension_stringing_reproduces_its_tension`).
            assert!(
                *horizontal_tension <= by_hand && *horizontal_tension > by_hand * 0.999,
                "{}: {horizontal_tension} against {by_hand} by hand",
                rules[0].name
            );
            assert_eq!((*temperature, *condition), (15.0, StrungAt::Initial));
            assert_eq!(horizontal_tension.fract(), 0.0);
            let g = s.governing.as_ref().unwrap();
            assert_eq!((g.rule, g.case.as_str()), (0, "everyday"));
            assert!(g.usage <= 1.0 && g.usage > 0.999, "{}", g.usage);
            assert_eq!(s.usages.len(), 2);
            assert!(s.usages[1].1 < 0.9, "{:?}", s.usages);
            assert!(s.trials < 60 && !s.stopped);
            // The line returned carries it; the others as they were.
            assert_eq!(
                out.line.sections[0].stringing,
                *s.stringing.as_ref().unwrap()
            );
            assert_eq!(out.line.sections[1], line.sections[1]);
            assert_eq!(out.stringings().count(), 1);
            assert!(out.notes.iter().any(|n| n.contains("check the clearances")));
        }
    }

    /// The whole line sagged, then checked: no span above its limit, and
    /// every section sagged close to it (INVENTED factors). A value that
    /// holds after creep stays one: the tension found is the tension after
    /// creep, its initial tension worked back from it.
    #[test]
    fn the_line_sagged_keeps_every_rule() {
        let (mut line, library) = example();
        let weather = [
            ohl_weather::WeatherCase::still("everyday", Temperature::from_celsius(15.0)),
            ohl_weather::WeatherCase::still("cold", Temperature::from_celsius(-20.0)),
        ];
        line.creep_stretch.creep_case = Some("everyday".into());
        line.set_weather_cases(&weather);
        let first = line.sections[0].id.clone();
        line.sections[0].stringing = Stringing::Tension {
            horizontal_tension: 20_000.0,
            temperature: 15.0,
            condition: StrungAt::Creep,
        };
        let rules = [
            TensionRule {
                factor: 0.18,
                cases: vec!["everyday".into()],
                measure: TensionMeasure::Horizontal,
                ..TensionRule::blank("everyday, invented 0.18")
            },
            TensionRule {
                factor: 0.22,
                cases: vec!["cold".into()],
                ..TensionRule::blank("cold, invented 0.22")
            },
        ];
        let out = sag_to_limits(
            &line,
            &library,
            &rules,
            &weather,
            &[],
            &Default::default(),
            &budget(),
        );
        assert_eq!(out.sections.len(), line.sections.len());
        for (s, before) in out.sections.iter().zip(&line.sections) {
            // The example's clipped section stays as it is, and says so.
            if before.stringing.clipped() {
                assert!(s.stringing.is_none() && s.notes[0].contains("clipped in"));
                continue;
            }
            assert!(s.stringing.is_some(), "{}: {:?}", s.section, s.notes);
            let g = s.governing.as_ref().unwrap();
            assert!(
                g.usage <= 1.0 && g.usage > 0.999,
                "{}: {}",
                s.section,
                g.usage
            );
        }
        assert_eq!(
            out.line.sections[0].stringing.condition(),
            StrungAt::Creep,
            "the condition of the value is kept"
        );
        let cable = library.get(&line.sections[0].cable).unwrap();
        let back = ohl_line::solve::worked_back(
            &out.line,
            &out.line.sections[0],
            cable,
            &ohl_wirefe::default_fe_budget(),
        )
        .unwrap()
        .unwrap();
        // (This library wire gives no creep: the same as initial, and said.)
        assert!(back.initial >= back.target, "{back:?}");
        // The whole line checked: sections sharing an assembly were sagged
        // alone, so allow them the coupling's part (well under 1 %).
        let solved: Vec<(String, Vec<SectionShape>)> = weather
            .iter()
            .map(|c| {
                let shapes = ohl_line::solve::solve_line(&out.line, &library, c)
                    .into_iter()
                    .map(Result::unwrap)
                    .collect();
                (c.name.clone(), shapes)
            })
            .collect();
        let views: Vec<(&str, crate::calculation::Calc, &[SectionShape])> = solved
            .iter()
            .map(|(n, s)| (n.as_str(), None, s.as_slice()))
            .collect();
        let report = check(&out.line, &library, &views, &rules, &Default::default());
        assert!(!report.rows.is_empty());
        assert!(report.rows[0].usage < 1.01, "{:?}", report.rows[0]);
        let of_first = report
            .rows
            .iter()
            .filter(|r| r.section == first)
            .map(|r| r.usage)
            .fold(0.0, f64::max);
        assert!(of_first > 0.99, "{of_first}");
    }

    /// What is not sagged says why: a clipped section, a section no rule
    /// takes, rules that never bind below the RTS, a section not there; a
    /// time limit of nothing gives the valid low end and says so.
    #[test]
    fn what_is_not_sagged_says_why() {
        let (mut line, library) = example();
        let weather = [ohl_weather::WeatherCase::still(
            "everyday",
            Temperature::from_celsius(15.0),
        )];
        let id = line.sections[0].id.clone();
        let rule = |factor: f64| TensionRule {
            factor,
            measure: TensionMeasure::Horizontal,
            ..TensionRule::blank("everyday")
        };
        let run =
            |line: &Line, rules: &[TensionRule], ids: &[String], b: &ohl_core::budget::Budget| {
                sag_to_limits(line, &library, rules, &weather, ids, &Default::default(), b)
            };
        let ids = [id.clone(), "nowhere".to_owned()];
        // Rules for another wire type only.
        let mut other = rule(0.2);
        other.targets.wire_types = vec!["no-such-wire".into()];
        let out = run(&line, &[other], &ids, &budget());
        assert!(out.sections[0].stringing.is_none());
        assert!(out.sections[0].notes[0].contains("no tension rule limits it"));
        assert!(out.notes.iter().any(|n| n == "no section nowhere"));
        assert_eq!(out.line, line);
        // A limit above the RTS never binds.
        let out = run(&line, &[rule(1.5)], &ids[..1], &budget());
        assert!(
            out.sections[0].notes[0].contains("do not limit the stringing tension"),
            "{:?}",
            out.sections[0].notes
        );
        // No time at all: the bracket's low end, valid, and said.
        let none = ohl_core::budget::Budget::new().time_limit(std::time::Duration::ZERO);
        let out = run(&line, &[rule(0.2)], &ids[..1], &none);
        let s = &out.sections[0];
        assert!(s.stopped && s.stringing.is_some(), "{s:?}");
        assert!(s.notes[0].contains("not the tightest"), "{:?}", s.notes);
        assert!(s.governing.as_ref().unwrap().usage <= 1.0);
        // Clipped in.
        let cable = library.get(&line.sections[0].cable).unwrap();
        let (lengths, temperature) =
            ohl_line::solve::clip_lengths(&line, &line.sections[0], cable).unwrap();
        line.sections[0].stringing = Stringing::UnstressedLengths {
            lengths,
            temperature,
            sagged: None,
        };
        let out = run(&line, &[rule(0.2)], &ids[..1], &budget());
        assert!(out.sections[0].notes[0].contains("clipped in"));
    }

    /// A rule file: a tension rule's limit as a catenary constant or a
    /// tension reads, writes back (the file then of version 2, else 1 as
    /// before), and one limit only is taken.
    #[test]
    fn limits_other_than_a_factor_in_a_rule_file() {
        let head = "format = \"ohl-clearance-rules\"\nversion = 2\nname = \"t\"\n";
        let set = RuleSet::parse(&format!(
            "{head}[[tension_rule]]\nname = \"vibration\"\ncatenary_constant = \"1800 m\"\n\
             [[tension_rule]]\nname = \"absolute\"\ntension = \"25 kN\"\nmeasure = \"horizontal\"\n\
             [[tension_rule]]\nname = \"factor\"\nfactor = 0.4\n"
        ))
        .unwrap();
        let [a, b, c] = &set.tension_rules[..] else {
            panic!()
        };
        assert_eq!(a.limit(), RuleLimit::CatenaryConstant(1800.0));
        assert_eq!(a.measured(), TensionMeasure::Horizontal);
        assert_eq!(a.allowed(0.0, 16.0), Some(28_800.0));
        assert_eq!(b.limit(), RuleLimit::Tension(25_000.0));
        assert_eq!(b.allowed(0.0, 16.0), Some(25_000.0));
        assert_eq!(c.limit(), RuleLimit::Factor(0.4));
        assert_eq!(c.allowed(0.0, 16.0), None);
        assert_eq!(c.allowed(100e3, 16.0), Some(40e3));
        assert_eq!(a.limit().describe(), "H/w ≤ 1800 m");
        let text = set.to_toml();
        assert!(text.contains("version = 2"), "{text}");
        assert_eq!(
            RuleSet::parse(&text).unwrap().tension_rules,
            set.tension_rules
        );
        // Factors only: written as version 1, as before.
        let mut plain = set.clone();
        plain.tension_rules.clear();
        plain.tension_rules.push(c.clone());
        assert!(plain.to_toml().contains("version = 1"));
        for (bad, says) in [
            ("factor = 0.4\ntension = \"25 kN\"", "give one limit"),
            ("name2 = 1", "unknown field"),
            ("", "give one limit"),
            ("catenary_constant = \"0 m\"", "must be above 0"),
            (
                "catenary_constant = \"1800 m\"\nmeasure = \"max\"",
                "limits the horizontal tension",
            ),
        ] {
            let e = RuleSet::parse(&format!("{head}[[tension_rule]]\nname = \"x\"\n{bad}\n"))
                .unwrap_err();
            assert!(e.contains(says), "{bad}: {e}");
        }
    }
}
