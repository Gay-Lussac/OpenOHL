// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The **conductor jump when ice drops** (ice shedding; roadmap O13,
//! owner 2026-11-02: "Let's go with the simplified for now") — Checks ›
//! Ice shedding… and `ohl ice-shedding`. Built, in active development,
//! not approved yet.
//!
//! The **simplified method** ([`Method::Simplified`], the only one so
//! far; a dynamic time-domain solve may come beside it as another
//! method): when the ice falls off a wire, the wire rises from its iced
//! position towards its bare one and overshoots it. The static rise
//! Δ(x) — the bare position less the iced one at each point x of the
//! span — times the rule's **rebound factor** k gives the jumped
//! position:
//!
//! ```text
//! p_jump(x) = p_iced(x) + k · Δ(x)
//! ```
//!
//! k = 1 is the bare position at rest; a published rebound factor above 1
//! the overshoot. **No factor is built in**: the rule's `factor` is
//! required and is to be taken from a published source the user designs
//! to (see the theory page, `docs/theory/ice-shedding.md`).
//!
//! The static rise, by the rule's `shedding`:
//!
//! - [`Shedding::One`] (default): **one span sheds alone**, the others
//!   keep their ice. The span's horizontal tension is taken as held at its
//!   iced value (the neighbouring iced spans hold it through the swinging
//!   strings — the limit of a long section with free strings), so the bare
//!   wire hangs on a catenary of parameter c_b = c_i · w_i / w_b, c_i the
//!   iced span's (fitted to its solved sag), w_i and w_b the wire's
//!   vertical load per metre iced and bare. A **conservative assumption**:
//!   any drop in tension (strings that do not swing freely, a short
//!   section, every span shedding) makes the bare sag larger and the rise
//!   smaller. The rise is vertical.
//! - [`Shedding::All`]: **every span of the wire sheds** at once: the line
//!   solved in the bare case (the section's tensions re-balanced, the
//!   strings swung, the wire's elasticity), Δ the bare point less the
//!   iced one at the same fraction of the span.
//!
//! The bare case is the rule's `bare` case, else the iced case without
//! its ice (the same temperature and wind). The **other wires keep their
//! ice**: they stay where the iced case puts them (the wire above that
//! shed too would rise as well: keeping it iced is the conservative
//! assumption). The clearance is the shortest distance between the jumped
//! wire's polyline and each other wire of the same span (segment against
//! segment, exact), less both wires' outer radii — the jumped wire's
//! bare, the other's iced — and the polylines' error bounds.

use std::collections::BTreeSet;

use ohl_core::budget::Budget;
use ohl_line::Line;
use ohl_line::solve::SectionShape;
use ohl_weather::WeatherCase;
use serde::{Deserialize, Serialize};

use crate::calculation::Calc;
use crate::checks::{Wire, wires_of};
use crate::geometry::{P3, closest_between_segments, distance};
use crate::rules::{PairVoltage, RawRequirement, Requirement};
use crate::tension::TensionTargets;

/// How the jump is found (owner, 2026-11-02: the simplified method now; a
/// dynamic time-domain solve may come later beside it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Method {
    /// The static rise between the iced and the bare wire times a
    /// rebound factor.
    #[default]
    Simplified,
}

impl Method {
    /// `"simplified"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Simplified => "simplified",
        }
    }

    /// From its name.
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "simplified" => Ok(Self::Simplified),
            other => Err(format!("method \"{other}\": use simplified")),
        }
    }

    /// In words, for tables.
    pub fn describe(self) -> &'static str {
        match self {
            Self::Simplified => "simplified (static rise × rebound factor)",
        }
    }
}

/// Which spans of a wire shed their ice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Shedding {
    /// Each span alone, the others keeping their ice: the tension held at
    /// its iced value (conservative).
    #[default]
    One,
    /// Every span of the wire at once: the line solved in the bare case.
    All,
}

impl Shedding {
    /// `"one"` or `"all"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::One => "one",
            Self::All => "all",
        }
    }

    /// From its name.
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "one" => Ok(Self::One),
            "all" => Ok(Self::All),
            other => Err(format!("shedding \"{other}\": use one or all")),
        }
    }

    /// In words, for tables.
    pub fn describe(self) -> &'static str {
        match self {
            Self::One => "one span alone (tension held)",
            Self::All => "every span of the wire (bare case solved)",
        }
    }
}

/// One rule of the ice shedding check (`[[ice_shedding_rule]]`).
#[derive(Debug, Clone, PartialEq)]
pub struct IceSheddingRule {
    /// Name, for reports.
    pub name: String,
    /// The iced weather cases (names, `*` `?` wildcards); empty: every
    /// case with ice.
    pub cases: Vec<String>,
    /// The bare case after the ice fell (a weather case's name); none:
    /// each iced case without its ice.
    pub bare: Option<String>,
    /// How the wires are solved (none: each section's own calculation).
    pub calculation: Calc,
    /// How the jump is found.
    pub method: Method,
    /// Which spans shed.
    pub shedding: Shedding,
    /// The rebound factor k (above 0; required: from a published source,
    /// none built in; 0: not set).
    pub factor: f64,
    /// The clearance to keep between the jumped wire and each other wire.
    pub distance: Requirement,
    /// The voltage of a pair, for a distance by voltage.
    pub pair_voltage: PairVoltage,
    /// The wires that shed (none given: every wire).
    pub targets: TensionTargets,
}

impl IceSheddingRule {
    /// A rule for every wire in every iced case, its factor and distance
    /// to set.
    pub fn blank(name: &str) -> Self {
        Self {
            name: name.into(),
            cases: Vec::new(),
            bare: None,
            calculation: None,
            method: Method::Simplified,
            shedding: Shedding::One,
            factor: 0.0,
            distance: Requirement::Fixed(0.0),
            pair_voltage: PairVoltage::Higher,
            targets: TensionTargets::default(),
        }
    }

    /// Whether it applies to weather case `case` (by its name; whether it
    /// has ice is asked apart).
    pub fn applies(&self, case: &str) -> bool {
        self.cases.is_empty()
            || self
                .cases
                .iter()
                .any(|c| ohl_core::pattern::matches(c, case))
    }

    /// What is wrong with it (none: nothing).
    pub fn problem(&self) -> Option<String> {
        if !(self.factor.is_finite() && self.factor > 0.0) {
            return Some(format!(
                "\"{}\": set the rebound factor (from a published source)",
                self.name
            ));
        }
        if let Some(e) = self.distance.problem() {
            return Some(format!("\"{}\": {e}", self.name));
        }
        if self.bare.as_deref().is_some_and(|b| b.trim().is_empty()) {
            return Some(format!("\"{}\": the bare case has no name", self.name));
        }
        None
    }

    /// The bare case for iced case `iced`, from the weather cases `all`:
    /// the rule's, else `iced` without its ice.
    pub fn bare_case(
        &self,
        iced: &WeatherCase,
        all: &[WeatherCase],
    ) -> Result<WeatherCase, String> {
        match &self.bare {
            None => Ok(iced.without_ice()),
            Some(name) => all
                .iter()
                .find(|c| &c.name == name)
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "\"{}\": its bare case \"{name}\" is not a weather case",
                        self.name
                    )
                }),
        }
    }

    /// The rule as an `[[ice_shedding_rule]]` table of a rule file.
    pub fn to_toml(&self) -> String {
        use std::fmt::Write;
        let q = |t: &str| toml::Value::String(t.to_owned()).to_string();
        let list = |v: &[String]| {
            format!(
                "[{}]",
                v.iter().map(|x| q(x)).collect::<Vec<_>>().join(", ")
            )
        };
        let mut t = format!("\n[[ice_shedding_rule]]\nname = {}\n", q(&self.name));
        if !self.cases.is_empty() {
            let _ = writeln!(t, "cases = {}", list(&self.cases));
        }
        if let Some(b) = &self.bare {
            let _ = writeln!(t, "bare = {}", q(b));
        }
        t.push_str(&crate::calculation::toml_line(self.calculation));
        if self.method != Method::Simplified {
            let _ = writeln!(t, "method = \"{}\"", self.method.as_str());
        }
        if self.shedding != Shedding::One {
            let _ = writeln!(t, "shedding = \"{}\"", self.shedding.as_str());
        }
        let _ = writeln!(t, "factor = {:?}", self.factor);
        let _ = writeln!(t, "distance = {}", self.distance.to_toml());
        match self.pair_voltage {
            PairVoltage::Higher => {}
            PairVoltage::Sum => t.push_str("pair_voltage = \"sum\"\n"),
            PairVoltage::Difference => t.push_str("pair_voltage = \"difference\"\n"),
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

/// An `[[ice_shedding_rule]]` table as read.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawIceSheddingRule {
    name: String,
    #[serde(default)]
    cases: Vec<String>,
    #[serde(default)]
    bare: Option<String>,
    #[serde(default)]
    calculation: Option<ohl_line::condition::Calculation>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    shedding: Option<String>,
    factor: f64,
    distance: RawRequirement,
    #[serde(default)]
    pair_voltage: Option<String>,
    #[serde(default)]
    targets: Option<RawTargets>,
}

#[derive(serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawTargets {
    #[serde(default)]
    wire_types: Vec<String>,
    #[serde(default)]
    systems: Vec<String>,
    #[serde(default)]
    circuits: Vec<String>,
}

impl RawIceSheddingRule {
    pub(crate) fn resolve(self) -> Result<IceSheddingRule, String> {
        let bad = |e: String| format!("ice shedding rule \"{}\": {e}", self.name);
        let method = self
            .method
            .as_deref()
            .map_or(Ok(Method::Simplified), Method::parse)
            .map_err(bad)?;
        let shedding = self
            .shedding
            .as_deref()
            .map_or(Ok(Shedding::One), Shedding::parse)
            .map_err(bad)?;
        let pair_voltage = self
            .pair_voltage
            .as_deref()
            .map_or(Ok(PairVoltage::Higher), PairVoltage::parse)
            .map_err(bad)?;
        let distance = self
            .distance
            .resolve()
            .map_err(|e| bad(format!("distance: {e}")))?;
        let t = self.targets.unwrap_or_default();
        let rule = IceSheddingRule {
            name: self.name,
            cases: self.cases,
            bare: self.bare,
            calculation: self.calculation,
            method,
            shedding,
            factor: self.factor,
            distance,
            pair_voltage,
            targets: TensionTargets {
                wire_types: t.wire_types,
                systems: t.systems,
                circuits: t.circuits,
            },
        };
        match rule.problem() {
            Some(e) => Err(format!("ice shedding rule {e}")),
            None => Ok(rule),
        }
    }
}

/// The deflection below the chord of a catenary of parameter `c` (m)
/// hanging over a span of horizontal length `l` and height difference `h`
/// (m, the far end higher when positive), at `x` (m from the first end):
/// \\( \delta(x) = h x / l - z(x) \\), \\( z(x) = c\,[\cosh((x-m)/c) -
/// \cosh(m/c)] \\), the low point at
/// \\( m = l/2 - c\,\mathrm{asinh}(h / (2c \sinh(l/2c))) \\).
pub fn catenary_deflection(l: f64, h: f64, c: f64, x: f64) -> f64 {
    let m = l / 2.0 - c * (h / (2.0 * c * (l / (2.0 * c)).sinh())).asinh();
    let z = c * (((x - m) / c).cosh() - (m / c).cosh());
    h * x / l - z
}

/// The sag of that catenary: its largest deflection below the chord,
/// where its slope is the chord's (\\( \sinh((x-m)/c) = h/l \\)).
pub fn catenary_sag(l: f64, h: f64, c: f64) -> f64 {
    let m = l / 2.0 - c * (h / (2.0 * c * (l / (2.0 * c)).sinh())).asinh();
    let x = (m + c * (h / l).asinh()).clamp(0.0, l);
    catenary_deflection(l, h, c, x)
}

/// The catenary parameter c (m) giving sag `sag` over that span, by
/// bisection on ln c (the sag falls as c grows); none for no sag.
pub fn fit_parameter(l: f64, h: f64, sag: f64) -> Option<f64> {
    if !(l > 0.0 && sag > 0.0 && l.is_finite() && h.is_finite() && sag.is_finite()) {
        return None;
    }
    // From a sag of about l/2 down to a sag of a micrometre.
    let (mut lo, mut hi) = ((l * 1e-3).ln(), (l * l / 8e-6).ln());
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if catenary_sag(l, h, mid.exp()) > sag {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some((0.5 * (lo + hi)).exp())
}

/// The vertical rise (m, upwards) at each of `points` (an iced span's
/// polyline) when the span sheds alone with its tension held
/// ([`Shedding::One`]): the iced catenary fitted to the span's sag
/// `sag`, the bare one of parameter c · `ratio` (`ratio` = w_i / w_b);
/// rise = δ_iced(x) − δ_bare(x). Empty when the span has no sag.
pub fn rise_held(points: &[P3], sag: f64, ratio: f64) -> Vec<f64> {
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return Vec::new();
    };
    let l = (last[0] - first[0]).hypot(last[1] - first[1]);
    let h = last[2] - first[2];
    let Some(c) = fit_parameter(l, h, sag) else {
        return vec![0.0; points.len()];
    };
    let cb = c * ratio;
    points
        .iter()
        .map(|p| {
            // Along the span in plan, from its first end.
            let x = ((p[0] - first[0]) * (last[0] - first[0])
                + (p[1] - first[1]) * (last[1] - first[1]))
                / l;
            let x = x.clamp(0.0, l);
            catenary_deflection(l, h, c, x) - catenary_deflection(l, h, cb, x)
        })
        .collect()
}

/// The rise at each of `iced` towards `bare` (both one span's polylines,
/// in the same direction) at the same fraction of the span in plan
/// ([`Shedding::All`]).
pub fn rise_to(iced: &[P3], bare: &[P3]) -> Vec<P3> {
    let Some(first) = iced.first() else {
        return Vec::new();
    };
    let last = iced[iced.len() - 1];
    let len = (last[0] - first[0]).hypot(last[1] - first[1]);
    iced.iter()
        .map(|p| {
            let t = if len > 0.0 {
                ((p[0] - first[0]) * (last[0] - first[0])
                    + (p[1] - first[1]) * (last[1] - first[1]))
                    / (len * len)
            } else {
                0.0
            };
            let b = crate::galloping::at_fraction(bare, t);
            [b[0] - p[0], b[1] - p[1], b[2] - p[2]]
        })
        .collect()
}

/// The shortest distance between two polylines (segment against segment,
/// exact), m, and the closest points (on `a`, on `b`) with the fraction
/// of `a`'s horizontal length where it is.
pub fn polyline_distance(a: &[P3], b: &[P3]) -> Option<(f64, [P3; 2], f64)> {
    if a.len() < 2 || b.len() < 2 {
        return None;
    }
    let boxed = |s: &[P3]| -> ([f64; 3], [f64; 3]) {
        let mut lo = s[0];
        let mut hi = s[0];
        for p in s {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        (lo, hi)
    };
    let gap = |x: &([f64; 3], [f64; 3]), y: &([f64; 3], [f64; 3])| -> f64 {
        let mut d2 = 0.0;
        for k in 0..3 {
            let g = (x.0[k] - y.1[k]).max(y.0[k] - x.1[k]).max(0.0);
            d2 += g * g;
        }
        d2.sqrt()
    };
    let boxes_b: Vec<_> = b.windows(2).map(boxed).collect();
    let mut best: Option<(f64, [P3; 2], usize, P3)> = None;
    for (i, sa) in a.windows(2).enumerate() {
        let ba = boxed(sa);
        for (sb, bb) in b.windows(2).zip(&boxes_b) {
            if best.as_ref().is_some_and(|x| gap(&ba, bb) >= x.0) {
                continue;
            }
            let (p, q) = closest_between_segments(sa[0], sa[1], sb[0], sb[1]);
            let d = distance(p, q);
            if best.as_ref().is_none_or(|x| d < x.0) {
                best = Some((d, [p, q], i, p));
            }
        }
    }
    let (d, pts, _, p) = best?;
    // Where along `a` in plan, as a fraction.
    let (f, l) = (a[0], a[a.len() - 1]);
    let len = (l[0] - f[0]).hypot(l[1] - f[1]);
    let at = if len > 0.0 {
        (((p[0] - f[0]) * (l[0] - f[0]) + (p[1] - f[1]) * (l[1] - f[1])) / (len * len))
            .clamp(0.0, 1.0)
    } else {
        0.0
    };
    Some((d, pts, at))
}

/// A wire's span that sheds, with what its jump needs.
#[derive(Debug, Clone, PartialEq)]
pub struct JumpWire {
    /// The wire in the iced case.
    pub wire: Wire,
    /// Its rise at each point (iced to bare at rest), m.
    pub rise: Vec<P3>,
    /// Its subconductors' bare surface radius, m.
    pub bare_radius: f64,
}

impl JumpWire {
    /// The jumped polyline: each point raised by `factor` × its rise.
    pub fn jumped(&self, factor: f64) -> Vec<P3> {
        self.wire
            .points
            .iter()
            .zip(&self.rise)
            .map(|(p, r)| {
                [
                    p[0] + factor * r[0],
                    p[1] + factor * r[1],
                    p[2] + factor * r[2],
                ]
            })
            .collect()
    }

    /// The largest vertical rise (at rest), m.
    pub fn largest_rise(&self) -> f64 {
        self.rise.iter().map(|r| r[2]).fold(0.0, f64::max)
    }
}

/// One wire's span jumping against one other wire of its span, by one
/// rule in one iced case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IceSheddingRow {
    /// Index of the rule.
    pub rule: usize,
    /// The iced case.
    pub case: String,
    /// The bare case.
    pub bare: String,
    /// The calculation the wires were solved by (none: each section's
    /// own).
    pub calculation: Calc,
    /// The section that sheds.
    pub section: String,
    /// Its span, from 0.
    pub span: usize,
    /// The other wire's section.
    pub other: String,
    /// Its span, from 0.
    pub other_span: usize,
    /// The largest static rise (iced to bare at rest), m.
    pub rise: f64,
    /// The largest jump: factor × rise, m.
    pub jump: f64,
    /// The clearance with both wires iced, m.
    pub before: f64,
    /// The clearance with the wire jumped, m.
    pub after: f64,
    /// Required, m.
    pub required: f64,
    /// The pair's voltage, kV (none: both earthed).
    pub kv: Option<f64>,
    /// Where: fraction of the span (in plan) on the jumped wire.
    pub at: f64,
    /// The closest points: on the jumped wire, on the other.
    pub points: [P3; 2],
}

impl IceSheddingRow {
    /// Whether the clearance is kept.
    pub fn passes(&self) -> bool {
        self.after >= self.required
    }

    /// Clearance less required, m.
    pub fn margin(&self) -> f64 {
        self.after - self.required
    }
}

/// A jumped span drawn in the views.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JumpedSpan {
    /// Index of the rule.
    pub rule: usize,
    /// The iced case.
    pub case: String,
    /// The section.
    pub section: String,
    /// The span, from 0.
    pub span: usize,
    /// The jumped polyline.
    pub points: Vec<P3>,
    /// A clearance of it below the requirement.
    pub short: bool,
}

/// The result of a check.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IceSheddingReport {
    /// Every pair checked, the smallest margin first.
    pub rows: Vec<IceSheddingRow>,
    /// Every jumped span.
    pub jumped: Vec<JumpedSpan>,
    /// What was assumed, not checked, and why.
    pub notes: Vec<String>,
}

impl IceSheddingReport {
    /// The rows below their requirement.
    pub fn failures(&self) -> impl Iterator<Item = &IceSheddingRow> {
        self.rows.iter().filter(|r| !r.passes())
    }

    /// The results as CSV (one row per pair, rule and case; the smallest
    /// margin first).
    pub fn csv(&self, rules: &[IceSheddingRule]) -> String {
        use std::fmt::Write;
        let mut out = String::from(
            "rule,case,bare_case,section,span,other_section,other_span,factor,rise_m,jump_m,\
             before_m,after_m,required_m,margin_m,status,at_fraction,x,y,z,other_x,other_y,other_z\n",
        );
        for r in &self.rows {
            let rule = &rules[r.rule];
            let _ = writeln!(
                out,
                "{},{},{},{},{},{},{},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3}",
                ohl_core::text::csv_field(&rule.name),
                ohl_core::text::csv_field(&crate::calculation::label(&r.case, r.calculation)),
                ohl_core::text::csv_field(&r.bare),
                ohl_core::text::csv_field(&r.section),
                r.span + 1,
                ohl_core::text::csv_field(&r.other),
                r.other_span + 1,
                rule.factor,
                r.rise,
                r.jump,
                r.before,
                r.after,
                r.required,
                r.margin(),
                if r.passes() { "pass" } else { "fail" },
                r.at,
                r.points[0][0],
                r.points[0][1],
                r.points[0][2],
                r.points[1][0],
                r.points[1][1],
                r.points[1][2],
            );
        }
        out
    }
}

/// The structures a wire hangs between, sorted (none: a jumper).
fn span_key(w: &Wire) -> Option<[String; 2]> {
    if w.ends.len() < 2 || w.section.starts_with("jumper:") {
        return None;
    }
    let s = |e: &str| e.rsplitn(3, ':').last().unwrap_or(e).to_owned();
    let (a, b) = (s(&w.ends[0]), s(&w.ends[1]));
    Some(if a <= b { [a, b] } else { [b, a] })
}

/// Checks the jumps of `jumping` (index `rule` of a rule `rule_def`)
/// against `others` (every wire of the line in the iced case): each
/// jumping span against each other wire hanging between the same two
/// structures. Exit clause: `budget`, one step per pair.
pub fn check_spans(
    rule: usize,
    rule_def: &IceSheddingRule,
    jumping: &[JumpWire],
    others: &[Wire],
    run: &mut ohl_core::budget::Run<'_>,
) -> Result<(Vec<IceSheddingRow>, Vec<JumpedSpan>), String> {
    let keys: Vec<Option<[String; 2]>> = others.iter().map(span_key).collect();
    let mut rows = Vec::new();
    let mut drawn = Vec::new();
    for j in jumping {
        let Some(key) = span_key(&j.wire) else {
            continue;
        };
        let jumped = j.jumped(rule_def.factor);
        let jumped_wire = Wire {
            points: jumped.clone(),
            surface_radius: j.bare_radius,
            ..j.wire.clone()
        };
        let mut short = false;
        for (o, k) in others.iter().zip(&keys) {
            if k.as_ref() != Some(&key) || o.section == j.wire.section {
                continue;
            }
            run.step()
                .map_err(|e| format!("ice shedding check stopped: {e}"))?;
            let (Some(after), Some(before)) = (
                polyline_distance(&jumped, &o.points),
                polyline_distance(&j.wire.points, &o.points),
            ) else {
                continue;
            };
            let kv = rule_def
                .pair_voltage
                .of(j.wire.potential.as_ref(), o.potential.as_ref());
            let required = rule_def.distance.at_or_last(kv.unwrap_or(0.0));
            let row = IceSheddingRow {
                rule,
                case: String::new(),
                bare: String::new(),
                calculation: rule_def.calculation,
                section: j.wire.section.clone(),
                span: j.wire.span,
                other: o.section.clone(),
                other_span: o.span,
                rise: j.largest_rise(),
                jump: rule_def.factor * j.largest_rise(),
                before: (before.0 - j.wire.outer_radius() - o.outer_radius()).max(0.0),
                after: (after.0 - jumped_wire.outer_radius() - o.outer_radius()).max(0.0),
                required,
                kv,
                at: after.2,
                points: after.1,
            };
            short |= !row.passes();
            rows.push(row);
        }
        drawn.push(JumpedSpan {
            rule,
            case: String::new(),
            section: j.wire.section.clone(),
            span: j.wire.span,
            points: jumped,
            short,
        });
    }
    Ok((rows, drawn))
}

/// One weather case solved by one calculation: (case, calculation, the
/// sections solved).
pub type Solved<'a> = (&'a WeatherCase, Calc, &'a [SectionShape]);

/// What the rules need solved, from the weather cases `cases` (the first
/// `checked` of them checked as iced cases, every one a bare case a rule
/// may name): each iced case a rule applies to (with ice) and, for a rule
/// shedding every span, its bare case — each with the rule's calculation,
/// each pair once. With notes: the rules that apply to no iced case, bare
/// cases not found.
pub fn solves(
    rules: &[IceSheddingRule],
    cases: &[WeatherCase],
    checked: usize,
) -> (Vec<(WeatherCase, Calc)>, Vec<String>) {
    let mut out: Vec<(WeatherCase, Calc)> = Vec::new();
    let mut notes = Vec::new();
    let mut push = |c: WeatherCase, calc: Calc| {
        if !out.iter().any(|(x, k)| x.name == c.name && *k == calc) {
            out.push((c, calc));
        }
    };
    for r in rules {
        if r.problem().is_some() {
            continue;
        }
        let mut any = false;
        for c in cases[..checked.min(cases.len())]
            .iter()
            .filter(|c| r.applies(&c.name) && c.has_ice())
        {
            any = true;
            push(c.clone(), r.calculation);
            if r.shedding == Shedding::All {
                match r.bare_case(c, cases) {
                    Ok(b) => push(b, r.calculation),
                    Err(e) => notes.push(e),
                }
            }
        }
        if !any {
            notes.push(format!("\"{}\": none of its weather cases has ice", r.name));
        }
    }
    notes.dedup();
    (out, notes)
}

/// Checks `rules` on `line`: `solved` holds each case of [`solves`]
/// solved (a case missing is noted), `cases` the weather cases (the first
/// `checked` checked as iced cases, every one a bare case a rule may
/// name), `filter` the structure and section overrides, `keep` the
/// sections that may shed (none: all). Exit clause: `budget`.
#[allow(clippy::too_many_arguments)]
pub fn check(
    line: &Line,
    library: &ohl_cables::Library,
    rules: &[IceSheddingRule],
    cases: &[WeatherCase],
    checked: usize,
    solved: &[Solved<'_>],
    filter: &ohl_weather::CaseFilter,
    keep: Option<&BTreeSet<String>>,
    budget: &Budget,
) -> Result<IceSheddingReport, String> {
    let mut report = IceSheddingReport::default();
    let mut run = budget.start();
    let find = |name: &str, calc: Calc| {
        solved
            .iter()
            .find(|(c, k, _)| c.name == name && *k == calc)
            .map(|(c, _, s)| (*c, *s))
    };
    let mut noted = BTreeSet::new();
    for (k, rule) in rules.iter().enumerate() {
        if let Some(e) = rule.problem() {
            report.notes.push(format!("{e}: not checked"));
            continue;
        }
        for case in cases[..checked.min(cases.len())]
            .iter()
            .filter(|c| rule.applies(&c.name) && c.has_ice())
        {
            let label = crate::calculation::label(&case.name, rule.calculation);
            let Some((_, iced)) = find(&case.name, rule.calculation) else {
                report.notes.push(format!("\"{label}\": not solved"));
                continue;
            };
            let bare_case = match rule.bare_case(case, cases) {
                Ok(b) => b,
                Err(e) => {
                    report.notes.push(e);
                    continue;
                }
            };
            if case.wind_pressure.si() > 0.0 && rule.shedding == Shedding::One {
                report.notes.push(format!(
                    "\"{label}\" has wind: the rise of a span shedding alone is taken in the \
                     vertical from the vertical loads; the wire's sideways swing kept as iced"
                ));
            }
            let bare = match rule.shedding {
                Shedding::One => None,
                Shedding::All => match find(&bare_case.name, rule.calculation) {
                    Some((_, s)) => Some(s),
                    None => {
                        report.notes.push(format!(
                            "\"{}\": not solved",
                            crate::calculation::label(&bare_case.name, rule.calculation)
                        ));
                        continue;
                    }
                },
            };
            let wires = wires_of(line, iced, &[]);
            let sags = iced.iter().flat_map(|s| s.spans.iter().map(|x| x.sag));
            let mut jumping = Vec::new();
            for (w, sag) in wires.iter().zip(sags) {
                let Some(section) = line.sections.iter().find(|s| s.id == w.section) else {
                    continue;
                };
                if !rule.targets.takes(section) || keep.is_some_and(|k| !k.contains(&section.id)) {
                    continue;
                }
                let end = |j: usize| section.points.get(j).map_or("", |p| p.structure.as_str());
                if !filter.takes_span(&section.id, [end(w.span), end(w.span + 1)], &case.name) {
                    continue;
                }
                let Some(cable) = library.get(&section.cable) else {
                    if noted.insert(section.cable.clone()) {
                        report.notes.push(format!(
                            "wire type {}: not in the library, not checked",
                            section.cable
                        ));
                    }
                    continue;
                };
                let rise = match (rule.shedding, bare) {
                    (Shedding::One, _) => {
                        let wi = case.wire_load(cable.diameter, cable.weight()).vertical.si();
                        let wb = bare_case
                            .wire_load(cable.diameter, cable.weight())
                            .vertical
                            .si();
                        if !(wb > 0.0 && wi > 0.0) {
                            continue;
                        }
                        rise_held(&w.points, sag, wi / wb)
                            .into_iter()
                            .map(|z| [0.0, 0.0, z])
                            .collect()
                    }
                    (Shedding::All, Some(bare)) => {
                        let Some(b) = bare
                            .iter()
                            .find(|s| s.section == section.id)
                            .and_then(|s| s.spans.get(w.span))
                        else {
                            report.notes.push(format!(
                                "section {} span {}: not solved in \"{}\"",
                                section.id,
                                w.span + 1,
                                bare_case.name
                            ));
                            continue;
                        };
                        rise_to(&w.points, &b.points)
                    }
                    (Shedding::All, None) => continue,
                };
                jumping.push(JumpWire {
                    wire: w.clone(),
                    rise,
                    bare_radius: cable.diameter.si() / 2.0,
                });
            }
            let (mut rows, mut drawn) = check_spans(k, rule, &jumping, &wires, &mut run)?;
            for r in &mut rows {
                r.case = case.name.clone();
                r.bare = bare_case.name.clone();
            }
            for d in &mut drawn {
                d.case = label.clone();
            }
            report.rows.extend(rows);
            report.jumped.extend(drawn);
        }
    }
    report
        .rows
        .sort_by(|a, b| a.margin().total_cmp(&b.margin()));
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::RuleSet;

    fn wire(section: &str, y: f64, z: f64, sag: f64) -> Wire {
        Wire {
            section: section.into(),
            span: 0,
            circuit: "c".into(),
            phase: section.into(),
            potential: None,
            bundle: vec![],
            // A parabola over 400 m, `sag` at mid-span, 10 m pieces.
            points: (0..=40)
                .map(|k| {
                    let t = k as f64 / 40.0;
                    [400.0 * t, y, z - 4.0 * sag * t * (1.0 - t)]
                })
                .collect(),
            ends: vec![format!("T1:1:{section}"), format!("T2:1:{section}")],
            surface_radius: 0.0,
            polyline_error: 0.0,
        }
    }

    /// The catenary by hand (invented span): level, 400 m, c = 1000 m —
    /// sag c (cosh(L/2c) − 1) = 20.067 m; sloped (h = 40 m) the sag at the
    /// point of the chord's slope; the fit gives c back.
    #[test]
    fn catenary_by_hand() {
        let sag = 1000.0 * (0.2f64.cosh() - 1.0);
        assert!((catenary_sag(400.0, 0.0, 1000.0) - sag).abs() < 1e-9);
        assert!((catenary_deflection(400.0, 0.0, 1000.0, 200.0) - sag).abs() < 1e-9);
        assert!(catenary_deflection(400.0, 0.0, 1000.0, 0.0).abs() < 1e-9);
        assert!(catenary_deflection(400.0, 40.0, 1000.0, 400.0).abs() < 1e-9);
        // Sloped: the largest deflection where sinh((x − m)/c) = h/L,
        // checked against a fine search.
        let s = catenary_sag(400.0, 40.0, 1000.0);
        let search = (0..=40000)
            .map(|i| catenary_deflection(400.0, 40.0, 1000.0, i as f64 * 0.01))
            .fold(0.0, f64::max);
        assert!((s - search).abs() < 1e-6, "{s} {search}");
        for (h, c) in [(0.0, 1000.0), (40.0, 1500.0), (-60.0, 800.0)] {
            let back = fit_parameter(400.0, h, catenary_sag(400.0, h, c)).unwrap();
            assert!((back - c).abs() < 1e-6 * c, "{back} {c}");
        }
        assert!(fit_parameter(400.0, 0.0, 0.0).is_none());
    }

    /// A span shedding alone, its tension held (invented numbers): the
    /// iced wire on a catenary c_i = 1000 m over a level 400 m span, the
    /// iced load twice the bare one: the bare wire on c_b = 2000 m, the
    /// rise at mid-span 1000 (cosh 0.2 − 1) − 2000 (cosh 0.1 − 1) =
    /// 10.059 m (the parabola's D_i (1 − w_b/w_i) = 10.033 m), none at
    /// the ends.
    #[test]
    fn rise_of_one_span_by_hand() {
        let c = 1000.0f64;
        let pts: Vec<P3> = (0..=40)
            .map(|k| {
                let x = 10.0 * k as f64;
                [x, 0.0, 30.0 - catenary_deflection(400.0, 0.0, c, x)]
            })
            .collect();
        let sag = c * ((200.0 / c).cosh() - 1.0);
        let rise = rise_held(&pts, sag, 2.0);
        let by_hand = 1000.0 * (0.2f64.cosh() - 1.0) - 2000.0 * (0.1f64.cosh() - 1.0);
        assert!((rise[20] - by_hand).abs() < 1e-6, "{} {by_hand}", rise[20]);
        assert!((by_hand - 10.0585).abs() < 1e-3);
        assert!(rise[0].abs() < 1e-9 && rise[40].abs() < 1e-9);
        // A ratio of 1 (nothing shed): no rise.
        assert!(rise_held(&pts, sag, 1.0).iter().all(|r| r.abs() < 1e-9));
    }

    /// The jump against the wire above (invented line): two wires of one
    /// span, the lower 6 m below the upper, both with a 10 m sag (in
    /// parabola); a rise of 4 m at mid-span (a parabola too) and a factor
    /// 1.25: the lower wire 5 m higher at mid-span, the clearance there
    /// 6 − 5 = 1 m; a required 1.5 m not kept, 0.8 m kept; the clearance
    /// before about 6 m; to the surfaces less both radii (the jumped wire's bare
    /// one).
    #[test]
    fn jump_against_the_wire_above_by_hand() {
        let upper = wire("U", 0.0, 36.0, 10.0);
        let lower = wire("L", 0.0, 30.0, 10.0);
        let rise: Vec<P3> = lower
            .points
            .iter()
            .map(|p| {
                let t = p[0] / 400.0;
                [0.0, 0.0, 16.0 * t * (1.0 - t)]
            })
            .collect();
        let j = JumpWire {
            wire: lower.clone(),
            rise,
            bare_radius: 0.0,
        };
        let rule = IceSheddingRule {
            factor: 1.25,
            distance: Requirement::Fixed(1.5),
            ..IceSheddingRule::blank("invented")
        };
        let budget = Budget::new();
        let mut run = budget.start();
        let (rows, drawn) = check_spans(
            0,
            &rule,
            std::slice::from_ref(&j),
            &[upper.clone(), lower.clone()],
            &mut run,
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert_eq!((r.section.as_str(), r.other.as_str()), ("L", "U"));
        assert!((r.rise - 4.0).abs() < 1e-12 && (r.jump - 5.0).abs() < 1e-12);
        // Before: two parabolas 6 m apart vertically are closest where
        // they are steepest, square to the first 10 m piece (its slope
        // 40 · 0.025 · 0.975 / 10 = 0.0975): 6 / √(1 + 0.0975²).
        let before = 6.0 / (1.0 + 0.0975f64.powi(2)).sqrt();
        assert!((r.before - before).abs() < 1e-9, "{r:?}");
        // After: 1 m at mid-span (the pieces either side of it level to
        // within a micrometre).
        assert!((r.after - 1.0).abs() < 1e-5, "{r:?}");
        assert!((r.at - 0.5).abs() < 1e-4);
        assert!(!r.passes() && (r.margin() + 0.5).abs() < 1e-5);
        assert!(drawn[0].short);
        let loose = IceSheddingRule {
            distance: Requirement::Fixed(0.8),
            ..rule.clone()
        };
        let (rows, _) = check_spans(
            0,
            &loose,
            std::slice::from_ref(&j),
            std::slice::from_ref(&upper),
            &mut run,
        )
        .unwrap();
        assert!(rows[0].passes());
        // Radii: the upper wire iced 15 mm, the lower bare 10 mm, 2 mm of
        // polyline error each: 1 − 0.015 − 0.010 − 0.004 m.
        let mut iced_upper = upper.clone();
        iced_upper.surface_radius = 0.015;
        iced_upper.polyline_error = 0.002;
        let mut lw = j.clone();
        lw.wire.surface_radius = 0.02;
        lw.wire.polyline_error = 0.002;
        lw.bare_radius = 0.01;
        let (rows, _) = check_spans(0, &rule, &[lw], &[iced_upper], &mut run).unwrap();
        assert!(
            (rows[0].after - (1.0 - 0.029)).abs() < 1e-5,
            "{:?}",
            rows[0]
        );
        assert!((rows[0].before - (before - 0.039)).abs() < 1e-9);
        // A wire of another span: not paired.
        let mut other = upper.clone();
        other.ends = vec!["T2:1:U".into(), "T3:1:U".into()];
        let (rows, _) =
            check_spans(0, &rule, std::slice::from_ref(&j), &[other], &mut run).unwrap();
        assert!(rows.is_empty());
        // Exit clause.
        let none = Budget::new().max_iterations(0);
        let mut stopped = none.start();
        assert!(check_spans(0, &rule, &[j], &[upper], &mut stopped).is_err());
    }

    /// Every span shedding: the rise towards the bare polyline at the same
    /// fraction of the span.
    #[test]
    fn rise_to_the_bare_wire() {
        let iced = wire("A", 0.0, 30.0, 10.0).points;
        let bare = wire("A", 0.5, 30.0, 7.0).points;
        let r = rise_to(&iced, &bare);
        assert!((r[20][2] - 3.0).abs() < 1e-9 && (r[20][1] - 0.5).abs() < 1e-12);
        assert!(r[0][2].abs() < 1e-12);
    }

    /// A rule file with ice shedding rules reads and writes them back;
    /// the factor and the distance are required.
    #[test]
    fn ice_shedding_rules_in_a_rule_file() {
        let text = "format = \"ohl-clearance-rules\"\nversion = 1\nname = \"i\"\n\
            [[ice_shedding_rule]]\nname = \"drop\"\ncases = [\"glaze*\"]\nfactor = 1.2\n\
            distance = \"1.5 m\"\n\
            [[ice_shedding_rule]]\nname = \"all\"\nbare = \"cold\"\nshedding = \"all\"\n\
            factor = 1.0\ndistance = { by_voltage = [[110, \"1 m\"], [400, \"2.5 m\"]] }\n\
            pair_voltage = \"sum\"\ntargets = { circuits = [\"North\"] }\n";
        let set = RuleSet::parse(text).unwrap();
        assert_eq!(set.ice_shedding_rules.len(), 2);
        let (a, b) = (&set.ice_shedding_rules[0], &set.ice_shedding_rules[1]);
        assert_eq!(
            (a.factor, a.shedding, a.bare.as_deref()),
            (1.2, Shedding::One, None)
        );
        assert!(a.applies("glaze ice") && !a.applies("everyday"));
        assert_eq!(b.shedding, Shedding::All);
        assert_eq!(b.pair_voltage, PairVoltage::Sum);
        assert_eq!(b.distance.at_or_last(220.0), 2.5);
        assert!(set.is_for(crate::rules::PresetCheck::IceShedding));
        let back = RuleSet::parse(&set.to_toml()).unwrap();
        assert_eq!(back.ice_shedding_rules, set.ice_shedding_rules);
        for bad in [
            "factor = 0.0\ndistance = \"1 m\"",
            "distance = \"1 m\"",
            "factor = 1.0",
            "factor = 1.0\ndistance = \"0 m\"",
            "factor = 1.0\ndistance = \"1 m\"\nshedding = \"some\"",
            "factor = 1.0\ndistance = \"1 m\"\nmethod = \"dynamic\"",
        ] {
            let r = RuleSet::parse(&format!(
                "format = \"ohl-clearance-rules\"\nversion = 1\n[[ice_shedding_rule]]\nname = \"x\"\n{bad}\n"
            ));
            assert!(r.is_err(), "{bad}");
        }
    }

    /// On the example line (S1 and S2 side by side in spans T1–T5), glaze
    /// ice: a span shedding alone rises by about D_i (1 − w_b/w_i) at
    /// mid-span (the parabola's, within 1 % on the catenary); every span
    /// of S1 paired with S2's span; the bare case solved for `all` rises
    /// less (the tension drops).
    #[test]
    fn on_the_example_line() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let line =
            Line::load(&root.join("examples/synthetic-line/synthetic-line.ohl.json")).unwrap();
        let library = ohl_cables::Library::load(&root.join("libraries/cables")).unwrap();
        let cases = ohl_weather::WeatherCases::load(
            &root.join("examples/synthetic-line/weather-cases.ohlwc"),
        )
        .unwrap()
        .cases;
        let rules = vec![
            IceSheddingRule {
                cases: vec!["glaze ice".into()],
                factor: 1.0,
                distance: Requirement::Fixed(0.5),
                targets: TensionTargets {
                    wire_types: vec![],
                    systems: vec![],
                    circuits: vec!["Example 1".into()],
                },
                ..IceSheddingRule::blank("one, invented")
            },
            IceSheddingRule {
                cases: vec!["glaze ice".into()],
                shedding: Shedding::All,
                factor: 1.0,
                distance: Requirement::Fixed(0.5),
                targets: TensionTargets {
                    wire_types: vec![],
                    systems: vec![],
                    circuits: vec!["Example 1".into()],
                },
                ..IceSheddingRule::blank("all, invented")
            },
        ];
        let (plan, notes) = solves(&rules, &cases, cases.len());
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(plan.len(), 2, "glaze ice and glaze ice without ice");
        let shapes: Vec<Vec<SectionShape>> = plan
            .iter()
            .map(|(c, _)| {
                ohl_line::solve::solve_line(&line, &library, c)
                    .into_iter()
                    .map(Result::unwrap)
                    .collect()
            })
            .collect();
        let solved: Vec<Solved<'_>> = plan
            .iter()
            .zip(&shapes)
            .map(|((c, k), s)| (c, *k, s.as_slice()))
            .collect();
        let report = check(
            &line,
            &library,
            &rules,
            &cases,
            cases.len(),
            &solved,
            &Default::default(),
            None,
            &Budget::new(),
        )
        .unwrap();
        // S1 and S2 (circuit Example 1) four spans each, each paired with
        // the other: 8 rows per rule.
        let one: Vec<&IceSheddingRow> = report.rows.iter().filter(|r| r.rule == 0).collect();
        let all: Vec<&IceSheddingRow> = report.rows.iter().filter(|r| r.rule == 1).collect();
        assert_eq!((one.len(), all.len()), (8, 8), "{:?}", report.notes);
        assert!(
            report
                .rows
                .windows(2)
                .all(|w| w[0].margin() <= w[1].margin())
        );
        let glaze = cases.iter().find(|c| c.name == "glaze ice").unwrap();
        let cable = library.get(&line.sections[0].cable).unwrap();
        let wi = glaze
            .wire_load(cable.diameter, cable.weight())
            .vertical
            .si();
        let wb = cable.weight().si();
        let iced = &shapes[0][0].spans[0];
        let r = one
            .iter()
            .find(|r| r.section == "S1" && r.span == 0)
            .unwrap();
        let parabola = iced.sag * (1.0 - wb / wi);
        assert!(
            (r.rise - parabola).abs() < 0.01 * parabola,
            "{} {parabola}",
            r.rise
        );
        let a = all
            .iter()
            .find(|r| r.section == "S1" && r.span == 0)
            .unwrap();
        assert!(a.rise < r.rise, "{} {}", a.rise, r.rise);
        assert!(a.rise > 0.0);
        assert_eq!(a.bare, "glaze ice without ice");
        assert!(report.jumped.len() == 16);
        let csv = report.csv(&rules);
        assert_eq!(csv.lines().count(), 1 + 16);
    }
}
