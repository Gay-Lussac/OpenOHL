// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A scoped check run and reported (phase 10), shared by the network's
//! server and an offline working set's checks (phase 12): the same check
//! on the same neighbours gives the same report, whoever runs it. What a
//! report says of a neighbour is decided by a [`Viewer`] (the server: the
//! user's rights; an offline file: everything it holds — what its user
//! could not see was never written into it).

use std::collections::{BTreeMap, BTreeSet};

use ohl_scope::clearance::{ClearanceResult, Outcome, WireId};
use ohl_scope::electric::{ElectricResult, ElectricSpec};
use ohl_scope::fields::{FieldResult, FieldSpec};
use ohl_scope::loads::LoadsResult;
use ohl_scope::{Neighbour, Neighbourhood, PulledIn, WorkingSet};

use crate::{Finding, PulledReport, ScopedCheck, ScopedReport};

/// What a check computed, before a viewer's filter.
pub(crate) enum Raw {
    Clearances(Box<ClearanceResult>),
    Fields(Box<FieldResult>),
    Electric(Box<ElectricResult>),
    Loads(Box<LoadsResult>),
}

impl Raw {
    pub(crate) fn pulled(&self) -> &PulledIn {
        match self {
            Raw::Clearances(r) => &r.pulled,
            Raw::Fields(r) => &r.pulled,
            Raw::Electric(r) => &r.pulled,
            Raw::Loads(r) => &r.pulled,
        }
    }

    pub(crate) fn reach(&self) -> ohl_scope::Reach {
        match self {
            Raw::Clearances(r) => r.reach,
            Raw::Fields(r) => r.reach,
            Raw::Electric(r) => r.reach,
            Raw::Loads(r) => r.reach(),
        }
    }
}

/// Who reads a report: what of the neighbours it may be told.
pub(crate) trait Viewer {
    /// Whether neighbour `n` may be named and placed.
    fn see(&mut self, n: &Neighbour) -> bool;
    /// The routes of neighbour `n` it may be told (none: its model is not
    /// known).
    fn routes(&mut self, n: &Neighbour) -> Option<BTreeSet<String>>;
    /// Whether `text` names something it may not see.
    fn names_hidden(&self, text: &str) -> bool;
}

/// Runs `check` on working set `ws` (its structures may be narrowed: the
/// loads on its towers with a twin, `has_twin`) with the neighbours `net`
/// gives.
pub(crate) fn run<N: Neighbourhood>(
    net: &mut N,
    ws: &mut WorkingSet,
    check: &ScopedCheck,
    mut has_twin: impl FnMut(&mut N, &str, &str) -> bool,
) -> Result<Raw, String> {
    let model = ws.model.clone();
    Ok(match check {
        ScopedCheck::Clearances { case, rules } => {
            let rules = ohl_clearance::RuleSet::parse(rules)?.rules;
            let case = find_case(net, &model, case)?;
            Raw::Clearances(Box::new(ohl_scope::clearance::clearances(
                net, ws, &rules, &case,
            )?))
        }
        ScopedCheck::Fields {
            case,
            distance,
            amps,
            limit_ut,
        } => {
            let case = find_case(net, &model, case)?;
            let spec = FieldSpec {
                distance: distance.unwrap_or(ohl_scope::reach::FIELD_DISTANCE),
                amps: *amps,
                limit_ut: *limit_ut,
                ..FieldSpec::default()
            };
            Raw::Fields(Box::new(ohl_scope::fields::fields(net, ws, &case, &spec)?))
        }
        ScopedCheck::ElectricField {
            case,
            distance,
            limit_kv_m,
        } => {
            let case = find_case(net, &model, case)?;
            let spec = ElectricSpec {
                distance: distance.unwrap_or(ohl_scope::reach::FIELD_DISTANCE),
                limit_kv_m: *limit_kv_m,
                ..ElectricSpec::default()
            };
            Raw::Electric(Box::new(ohl_scope::electric::electric_field(
                net, ws, &case, &spec,
            )?))
        }
        ScopedCheck::StructureLoads { structures } => {
            if structures.is_empty() {
                let all = std::mem::take(&mut ws.structures);
                ws.structures = all
                    .into_iter()
                    .filter(|s| has_twin(net, &model, s))
                    .collect();
            } else {
                for s in structures {
                    if !ws.structures.contains(s) {
                        return Err(format!("structure {s} is not in the working set"));
                    }
                }
                ws.structures = structures.clone();
            }
            Raw::Loads(Box::new(ohl_scope::loads::shared_tower_loads(
                net,
                ws,
                &ohl_loads::default_budget(),
            )?))
        }
    })
}

/// Weather case `name` of `model`.
pub(crate) fn find_case<N: Neighbourhood>(
    net: &mut N,
    model: &str,
    name: &str,
) -> Result<ohl_weather::WeatherCase, String> {
    let cases = net.cases(model)?;
    cases
        .weather
        .iter()
        .find(|c| c.name == name)
        .cloned()
        .ok_or_else(|| {
            format!(
                "no weather case \"{name}\" on line {model} (its cases: {})",
                cases
                    .weather
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

/// The report of `raw` as `v` may read it.
pub(crate) fn report(
    v: &mut dyn Viewer,
    ws: &WorkingSet,
    name: &str,
    raw: &Raw,
    seconds: f64,
) -> ScopedReport {
    let pulled = raw.pulled();
    let mut report = PulledReport {
        read: pulled.read.len(),
        ..PulledReport::default()
    };
    let mut lines = BTreeSet::new();
    let mut routes = BTreeSet::new();
    for n in &pulled.used {
        if v.see(n) {
            lines.insert(n.model.clone());
            if let Some(rs) = v.routes(n) {
                for r in rs {
                    routes.insert(format!("line {}: route {r}", n.model));
                }
            }
            report
                .objects
                .push(format!("line {}: {} {}", n.model, n.kind.word(), n.id));
        } else {
            report.hidden += 1;
        }
    }
    report.lines = lines.into_iter().collect();
    report.routes = routes.into_iter().collect();
    let mut findings = Vec::new();
    let (mut results, mut violations) = (0, 0);
    let mut not_all: Vec<String> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    match raw {
        Raw::Clearances(r) => {
            not_all.extend(r.not_all_checked.iter().cloned());
            notes.extend(r.notes.iter().cloned());
            for rule in &r.rules {
                let mut lowest: BTreeMap<&'static str, Finding> = BTreeMap::new();
                let mut push = |f: Finding, findings: &mut Vec<Finding>| {
                    if f.violates {
                        findings.push(f.clone());
                    }
                    let k = if f.measure == "radial" {
                        "radial"
                    } else {
                        "vertical"
                    };
                    if lowest.get(k).is_none_or(|l| f.value < l.value) {
                        lowest.insert(k, f);
                    }
                };
                match &rule.outcome {
                    Outcome::NotChecked(why) => {
                        not_all.push(format!("rule {}: {why}", rule.name));
                    }
                    Outcome::Wires(w) => {
                        for (a, x) in w {
                            results += 1;
                            violations += usize::from(x.violates());
                            for f in wire_findings(v, &rule.name, a, None, x) {
                                push(f, &mut findings);
                            }
                        }
                    }
                    Outcome::Pairs(p) => {
                        for (a, b, x) in p {
                            results += 1;
                            violations += usize::from(x.violates());
                            for f in wire_findings(v, &rule.name, a, Some(b), x) {
                                push(f, &mut findings);
                            }
                        }
                    }
                }
                for (_, f) in lowest {
                    if !f.violates {
                        findings.push(Finding {
                            what: format!("{} (lowest)", f.what),
                            ..f
                        });
                    }
                }
            }
        }
        Raw::Fields(r) => {
            not_all.extend(r.not_all_checked.iter().cloned());
            notes.extend(r.notes.iter().cloned());
            for p in &r.profiles {
                results += p.points.len();
                let top = p.points.iter().max_by(|a, b| a.b_ut.total_cmp(&b.b_ut));
                if let Some(t) = top {
                    findings.push(Finding {
                        what: "magnetic field".into(),
                        on: format!("span {}–{}", p.from, p.to),
                        against: None,
                        measure: "B (µT)".into(),
                        value: t.b_ut,
                        required: r.limit_ut,
                        violates: r.limit_ut.is_some_and(|l| t.b_ut >= l),
                        at: Some(t.at),
                        hidden: false,
                    });
                }
            }
            violations = r.above_limit;
            if report.hidden > 0 {
                notes.push(format!(
                    "the field of {} object(s) you may not see is in the values (not named)",
                    report.hidden
                ));
            }
        }
        Raw::Electric(r) => {
            not_all.extend(r.not_all_checked.iter().cloned());
            notes.extend(r.notes.iter().cloned());
            notes.push(format!("phase angles: {}", r.method));
            notes.extend(r.taken_in.iter().map(|t| format!("taken in: {t}")));
            for p in &r.profiles {
                results += p.points.len();
                let top = p.points.iter().max_by(|a, b| a.e_kv_m.total_cmp(&b.e_kv_m));
                if let Some(t) = top {
                    findings.push(Finding {
                        what: "electric field".into(),
                        on: format!("span {}–{}", p.from, p.to),
                        against: None,
                        measure: "E (kV/m)".into(),
                        value: t.e_kv_m,
                        required: r.limit_kv_m,
                        violates: r.limit_kv_m.is_some_and(|l| t.e_kv_m >= l),
                        at: Some(t.at),
                        hidden: false,
                    });
                }
            }
            violations = r.above_limit;
            if report.hidden > 0 {
                notes.push(format!(
                    "the field of {} object(s) you may not see is in the values (not named)",
                    report.hidden
                ));
            }
        }
        Raw::Loads(r) => {
            not_all.extend(r.not_all_checked.iter().cloned());
            for t in &r.towers {
                let seen = t.twins.iter().filter(|n| v.see(n)).count();
                notes.push(format!(
                    "structure {}: {} twin(s) of other lines on it{}",
                    t.structure,
                    t.twins.len(),
                    if seen < t.twins.len() {
                        format!(
                            ", {} you may not see (their sections in the loads, not named)",
                            t.twins.len() - seen
                        )
                    } else {
                        String::new()
                    }
                ));
                notes.extend(t.notes.iter().cloned());
                for c in &t.cases {
                    results += c.points.len();
                }
                let measures = [
                    "factored transverse (N)",
                    "factored longitudinal (N)",
                    "factored vertical (N)",
                ];
                for w in &t.worst {
                    for (k, measure) in measures.iter().enumerate() {
                        let (line, case) = &w.governing[k];
                        let own = *line == ws.model;
                        let line_seen = own
                            || t.twins
                                .iter()
                                .filter(|n| &n.model == line)
                                .any(|n| v.see(n));
                        findings.push(Finding {
                            what: "structure loads (worst)".into(),
                            on: format!(
                                "structure {} point {}",
                                t.structure,
                                if w.point.is_empty() {
                                    "(body)"
                                } else {
                                    &w.point
                                }
                            ),
                            against: Some(if own {
                                format!("case {case}")
                            } else if line_seen {
                                format!("line {line}'s case {case}")
                            } else {
                                "a case of a line you may not see".into()
                            }),
                            measure: (*measure).into(),
                            value: w.factored[k],
                            required: None,
                            violates: false,
                            at: None,
                            hidden: !line_seen,
                        });
                    }
                }
            }
        }
    }
    // Texts naming what the reader may not see: replaced.
    let not_all: Vec<String> = not_all
        .into_iter()
        .map(|t| {
            if v.names_hidden(&t) {
                "a neighbour you may not see could not be read or solved: not all checked"
                    .to_owned()
            } else {
                t
            }
        })
        .collect();
    let mut notes: Vec<String> = notes
        .into_iter()
        .map(|t| {
            if !v.names_hidden(&t) {
                t
            } else if t.starts_with("NOT ON THE SAFE SIDE") {
                "NOT ON THE SAFE SIDE: something left out about a neighbour you may not see \
                 (not named) can make this result too low; someone with rights to it sees why"
                    .into()
            } else {
                "a note about a neighbour you may not see (not named)".into()
            }
        })
        .collect();
    notes.dedup();
    let mut not_all = not_all;
    not_all.dedup();
    ScopedReport {
        check: name.to_owned(),
        model: ws.model.clone(),
        reach: raw.reach().describe(),
        status: if not_all.is_empty() {
            "every part checked".into()
        } else {
            "NOT ALL CHECKED".into()
        },
        findings,
        results,
        violations,
        pulled: report,
        not_all_checked: not_all,
        notes,
        seconds,
    }
}

/// One clearance result as findings (radial, vertical), its target
/// named and placed only when the reader may see it.
fn wire_findings(
    v: &mut dyn Viewer,
    rule: &str,
    a: &WireId,
    b: Option<&WireId>,
    x: &ohl_clearance::WireResult,
) -> Vec<Finding> {
    let seen = b.is_none_or(|b| v.see(&b.neighbour()));
    let against = b.map(|b| {
        if !seen {
            if b.model == a.model {
                "a hidden object of this line".to_owned()
            } else {
                "a hidden object of another line".to_owned()
            }
        } else if b.model == a.model {
            b.describe()
        } else {
            format!("line {}: {}", b.model, b.describe())
        }
    });
    let mut out = Vec::new();
    for (measure, hit, required, count) in x.measures() {
        let Some(h) = hit else { continue };
        let violates = required.is_some() && count > 0;
        out.push(Finding {
            what: rule.to_owned(),
            on: if !seen && violates {
                format!(
                    "{} — check result limited by an object you may not see",
                    a.describe()
                )
            } else {
                a.describe()
            },
            against: against.clone(),
            measure: measure.to_owned(),
            value: h.distance,
            required,
            violates,
            at: seen.then_some(h.wire),
            hidden: !seen,
        });
    }
    out
}
