// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Structure checks: how much of its capacity each structure uses in each
//! load case (its type's `[capacity]`, see `ols_structures::structures`).
//!
//! - **Method 1, allowable spans**: the structure's wind span (half the sum
//!   of its adjacent spans, the largest over its attachments) and weight
//!   span (the wires' vertical load at the attachment, without the
//!   insulator, over their vertical load per metre — the distance between
//!   the low points of the adjacent spans for a catenary; the largest and
//!   the smallest over its attachments) against the type's allowable spans
//!   at the structure's line angle. Usage: the largest of wind span /
//!   allowable and weight span / allowable; a weight span below the
//!   minimum (uplift) is 100 % plus the shortfall over the allowable weight
//!   span; a line angle beyond the type's last one fails outright. The
//!   strength factor does not apply (allowable spans are the type's
//!   design spans).
//! - **Method 2, allowable loads** (load trees): at every attachment the
//!   type gives, the factored loads in the structure's axes (summed over a
//!   V-string's legs) against the allowable loads times the load case's
//!   strength factor; per direction, or added up with a linear
//!   interaction. Usage: the largest over the attachments.
//! - **Finite elements** (`fe`): the type's structure model (`model =
//!   "….olsfe"`) solved with the factored attachment loads (and its
//!   members' weight times the vertical factor); usage: the member using
//!   most of its capacity times the strength factor
//!   ([`ols_structures_fe`]).
//! - **Pole** (`pole`): the type's pole (`pole = "id"`): the bending
//!   moment of the factored loads at the type's attachment points and of
//!   the wind on the pole, P-Delta amplified, against the strength factor
//!   times the moment capacity along the pole.
//!
//! A structure's usage is the largest of its checks; a structure without
//! a type or a type with nothing to check against is listed, not checked.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use ols_line::Line;
use ols_structures::{Interaction, StructureLibrary};
use ols_weather::WeatherCase;

use crate::CaseLoads;

/// Which method a check is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// Allowable spans (Method 1).
    Spans,
    /// Allowable loads at the attachments (Method 2).
    Loads,
    /// The type's finite-element model.
    Fe,
    /// The type's pole.
    Pole,
}

impl Method {
    /// Name in reports.
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Spans => "spans",
            Method::Loads => "loads",
            Method::Fe => "fe",
            Method::Pole => "pole",
        }
    }
}

/// One check of a structure in one load case.
#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    /// Which method.
    pub method: Method,
    /// Share of the capacity used (1 = 100 %; infinite: not allowed at all,
    /// e.g. a line angle beyond the type's).
    pub usage: f64,
    /// What governs, in words (e.g. "weight span 640 m of 600 m").
    pub governs: String,
}

/// A structure in one load case.
#[derive(Debug, Clone, PartialEq)]
pub struct StructureUsage {
    /// Structure id.
    pub structure: String,
    /// Its type, if it has one.
    pub structure_type: Option<String>,
    /// Line angle, degrees (the largest of its routes, either way).
    pub line_angle_deg: f64,
    /// Wind span, m.
    pub wind_span: f64,
    /// Weight span, m: smallest and largest over the attachments (none:
    /// no wire load found).
    pub weight_span: Option<[f64; 2]>,
    /// Method 1, when the type gives allowable spans.
    pub spans: Option<Check>,
    /// Method 2, when the type gives allowable loads.
    pub loads: Option<Check>,
    /// The finite-element check, when the type names a model.
    pub fe: Option<Check>,
    /// The pole check, when the type names a pole.
    pub pole: Option<Check>,
    /// Why something is not checked.
    pub notes: Vec<String>,
}

impl StructureUsage {
    /// The governing check (the largest usage).
    pub fn governing(&self) -> Option<&Check> {
        [
            self.spans.as_ref(),
            self.loads.as_ref(),
            self.fe.as_ref(),
            self.pole.as_ref(),
        ]
        .into_iter()
        .flatten()
        .max_by(|a, b| a.usage.total_cmp(&b.usage))
    }
}

/// The structures in one load case.
#[derive(Debug, Clone, PartialEq)]
pub struct CaseUsage {
    /// Load case name.
    pub case: String,
    /// Every structure with loads in this case.
    pub structures: Vec<StructureUsage>,
}

/// An attachment point: (structure, set, phase).
type Key = (String, u32, u32);

/// The loads at one attachment point in one load case.
#[derive(Default)]
struct AtPoint {
    /// Factored, structure axes, summed over the supports, N.
    factored: [f64; 3],
    /// The unfactored vertical load (down), N.
    vertical: f64,
    /// The sections hanging there.
    sections: Vec<String>,
}

/// Half the horizontal spans next to each attachment point, summed over
/// the sections through it, m, by (structure, set, phase).
fn wind_spans(line: &Line) -> BTreeMap<Key, f64> {
    let mut out: BTreeMap<Key, f64> = BTreeMap::new();
    for s in &line.sections {
        let at: Vec<Option<[f64; 3]>> = s
            .points
            .iter()
            .map(|p| line.attachment(p).map(|a| a.wire_point))
            .collect();
        let half = |a: Option<[f64; 3]>, b: Option<[f64; 3]>| match (a, b) {
            (Some(a), Some(b)) => 0.5 * (b[0] - a[0]).hypot(b[1] - a[1]),
            _ => 0.0,
        };
        for (k, p) in s.points.iter().enumerate() {
            let back = if k > 0 { half(at[k - 1], at[k]) } else { 0.0 };
            let ahead = at.get(k + 1).map_or(0.0, |&b| half(at[k], b));
            *out.entry((p.structure.clone(), p.set, p.phase))
                .or_default() += back + ahead;
        }
    }
    out
}

/// The largest line angle of each structure over the routes through it,
/// degrees.
fn line_angles(line: &Line) -> BTreeMap<String, f64> {
    let mut out: BTreeMap<String, f64> = BTreeMap::new();
    for r in line.effective_routes() {
        for s in r.stations(line).unwrap_or_default() {
            let a = out.entry(s.structure).or_default();
            *a = a.max(s.line_angle_deg.abs());
        }
    }
    out
}

/// Vertical load per metre of a section's whole bundle in `case`, N/m.
fn bundle_weight(
    line: &Line,
    library: &ols_cables::Library,
    case: &WeatherCase,
    section: &str,
) -> Option<f64> {
    let s = line.sections.iter().find(|s| s.id == section)?;
    let cable = library.get(&s.cable)?;
    let w = case.wire_load(cable.diameter, cable.weight()).vertical.si();
    Some(w * f64::from(s.bundle.max(1)))
}

/// How much of its capacity every structure uses in every load case of
/// `results` ([`crate::structure_loads`] of the same `line`, `library`
/// and `weather`). `types`: the structure types (their `[capacity]`,
/// models and poles; files they name are read once per call).
///
/// Expected cost: Methods 1 and 2 are instant; a model or pole check is a
/// finite-element solve per structure and load case (milliseconds).
pub fn usage(
    line: &Line,
    types: &StructureLibrary,
    library: &ols_cables::Library,
    weather: &[WeatherCase],
    results: &[CaseLoads],
) -> Vec<CaseUsage> {
    let wind = wind_spans(line);
    let angles = line_angles(line);
    let analyses = crate::analysis::Analyses::load(types);
    results
        .iter()
        .map(|r| {
            let case = weather.iter().find(|w| w.name == r.case.weather);
            // Per attachment: the factored loads (summed over the
            // supports) and the wires' unfactored vertical load.
            let mut at: BTreeMap<Key, AtPoint> = BTreeMap::new();
            // Set 0: wind on the structure body, not an attachment.
            for l in r.loads.iter().filter(|l| l.set != 0) {
                let e = at.entry((l.structure.clone(), l.set, l.phase)).or_default();
                for (sum, f) in e.factored.iter_mut().zip(l.factored) {
                    *sum += f;
                }
                e.vertical += l.local[2];
                for s in &l.sections {
                    if !e.sections.contains(s) {
                        e.sections.push(s.clone());
                    }
                }
            }
            let mut by_structure: BTreeMap<&str, Vec<(&Key, &AtPoint)>> = BTreeMap::new();
            for (k, v) in &at {
                by_structure.entry(k.0.as_str()).or_default().push((k, v));
            }
            let structures = by_structure
                .into_iter()
                .map(|(id, points)| {
                    let structure = line.structure(id);
                    let type_id = structure.and_then(|s| s.structure_type.clone());
                    let type_def = type_id.as_deref().and_then(|t| types.get(t));
                    let capacity = type_def.and_then(|t| t.capacity.as_ref());
                    let analysed = type_def.is_some_and(|t| t.model.is_some() || t.pole.is_some());
                    let line_angle_deg = angles.get(id).copied().unwrap_or(0.0);
                    let wind_span = points
                        .iter()
                        .map(|(k, _)| wind.get(*k).copied().unwrap_or(0.0))
                        .fold(0.0, f64::max);
                    // Weight span of each attachment.
                    let spans: Vec<f64> = points
                        .iter()
                        .filter_map(|((_, set, phase), p)| {
                            let insulator = structure
                                .and_then(|s| {
                                    s.attachments
                                        .iter()
                                        .find(|a| a.set == *set && a.phase == *phase)
                                })
                                .map_or(0.0, |a| a.insulator_weight);
                            let ws: Vec<f64> = p
                                .sections
                                .iter()
                                .filter_map(|s| bundle_weight(line, library, case?, s))
                                .collect();
                            let w = ws.iter().sum::<f64>() / ws.len().max(1) as f64;
                            (w > 0.0).then(|| (p.vertical - insulator) / w)
                        })
                        .collect();
                    let weight_span = (!spans.is_empty()).then(|| {
                        [
                            spans.iter().copied().fold(f64::INFINITY, f64::min),
                            spans.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                        ]
                    });
                    let mut notes = Vec::new();
                    match (&type_id, capacity) {
                        (None, _) => notes.push("no structure type: not checked".into()),
                        (Some(t), None) if !analysed => {
                            notes.push(format!("type {t} gives no capacity"));
                        }
                        _ => {}
                    }
                    let spans_check = capacity.filter(|c| !c.spans.is_empty()).map(|c| {
                        match c.spans_at(line_angle_deg) {
                            None => Check {
                                method: Method::Spans,
                                usage: f64::INFINITY,
                                governs: format!(
                                    "line angle {line_angle_deg:.1}° beyond the type's {:.1}°",
                                    c.max_angle().unwrap_or(0.0)
                                ),
                            },
                            Some(allowed) => {
                                let mut worst = (
                                    wind_span / allowed.wind_span,
                                    format!(
                                        "wind span {wind_span:.0} m of {:.0} m",
                                        allowed.wind_span
                                    ),
                                );
                                if let Some([lo, hi]) = weight_span {
                                    let r = hi / allowed.weight_span;
                                    if r > worst.0 {
                                        worst = (
                                            r,
                                            format!(
                                                "weight span {hi:.0} m of {:.0} m",
                                                allowed.weight_span
                                            ),
                                        );
                                    }
                                    if let Some(min) = allowed.min_weight_span
                                        && lo < min
                                    {
                                        let r = 1.0 + (min - lo) / allowed.weight_span;
                                        if r > worst.0 {
                                            worst = (
                                                r,
                                                format!(
                                                    "weight span {lo:.0} m below the minimum \
                                                     {min:.0} m (uplift)"
                                                ),
                                            );
                                        }
                                    }
                                }
                                Check {
                                    method: Method::Spans,
                                    usage: worst.0,
                                    governs: worst.1,
                                }
                            }
                        }
                    });
                    let loads_check = capacity.filter(|c| !c.attachments.is_empty()).map(|c| {
                        let phi = r.case.strength;
                        let mut worst = (0.0, String::new());
                        for ((_, set, phase), p) in &points {
                            let Some(cap) = c.attachment(*set, *phase) else {
                                notes.push(format!(
                                    "attachment {set}:{phase} not in the type's load tree"
                                ));
                                continue;
                            };
                            let [t, l, v] = p.factored;
                            // (ratio, direction, load, allowable), kN in words.
                            let mut ratios: Vec<(f64, &str, f64, f64)> = Vec::new();
                            if let Some(ct) = cap.transverse {
                                ratios.push((t.abs() / (phi * ct.si()), "transverse", t, ct.si()));
                            }
                            if let Some(cl) = cap.longitudinal {
                                ratios.push((
                                    l.abs() / (phi * cl.si()),
                                    "longitudinal",
                                    l,
                                    cl.si(),
                                ));
                            }
                            if v >= 0.0 {
                                if let Some(cv) = cap.vertical {
                                    ratios.push((v / (phi * cv.si()), "vertical", v, cv.si()));
                                }
                            } else if let Some(cu) = cap.uplift {
                                ratios.push((-v / (phi * cu.si()), "uplift", v, cu.si()));
                            } else if cap.vertical.is_some() {
                                ratios.push((f64::INFINITY, "uplift (none allowed)", v, 0.0));
                            }
                            let (usage, why) = match cap.interaction {
                                Interaction::Linear => (
                                    ratios.iter().map(|r| r.0).sum::<f64>(),
                                    format!(
                                        "{set}:{phase} combined {}",
                                        ratios
                                            .iter()
                                            .map(|r| format!("{} {:.0} %", r.1, r.0 * 100.0))
                                            .collect::<Vec<_>>()
                                            .join(" + ")
                                    ),
                                ),
                                Interaction::None => ratios
                                    .iter()
                                    .max_by(|a, b| a.0.total_cmp(&b.0))
                                    .map_or((0.0, String::new()), |r| {
                                        (
                                            r.0,
                                            format!(
                                                "{set}:{phase} {} {:.1} kN of {:.1} kN × {phi}",
                                                r.1,
                                                r.2.abs() / 1000.0,
                                                r.3 / 1000.0
                                            ),
                                        )
                                    }),
                            };
                            if usage > worst.0 || worst.1.is_empty() {
                                worst = (usage, why);
                            }
                        }
                        Check {
                            method: Method::Loads,
                            usage: worst.0,
                            governs: worst.1,
                        }
                    });
                    let factored: Vec<crate::analysis::Factored> = points
                        .iter()
                        .map(|((_, set, phase), p)| (*set, *phase, p.factored))
                        .collect();
                    let fe = type_def.and_then(|t| analyses.fe(t, &factored, &r.case, &mut notes));
                    let pole = type_def
                        .and_then(|t| analyses.pole(t, &factored, &r.case, case, &mut notes));
                    StructureUsage {
                        structure: id.to_owned(),
                        structure_type: type_id,
                        line_angle_deg,
                        wind_span,
                        weight_span,
                        spans: spans_check,
                        loads: loads_check,
                        fe,
                        pole,
                        notes,
                    }
                })
                .collect();
            CaseUsage {
                case: r.case.name.clone(),
                structures,
            }
        })
        .collect()
}

/// The governing load case of every structure: the one using most of its
/// capacity (structures not checked left out), worst first.
pub fn governing(cases: &[CaseUsage]) -> Vec<(&str, &StructureUsage)> {
    let mut best: BTreeMap<&str, (&str, &StructureUsage, f64)> = BTreeMap::new();
    for c in cases {
        for s in &c.structures {
            let Some(g) = s.governing() else { continue };
            let e = best
                .entry(s.structure.as_str())
                .or_insert((c.case.as_str(), s, g.usage));
            if g.usage > e.2 {
                *e = (c.case.as_str(), s, g.usage);
            }
        }
    }
    let mut out: Vec<(&str, &StructureUsage, f64)> = best.into_values().collect();
    out.sort_by(|a, b| b.2.total_cmp(&a.2));
    out.into_iter().map(|(c, s, _)| (c, s)).collect()
}

/// The usage as CSV, one row per load case and structure:
/// `case,structure,type,line_angle_deg,wind_span,weight_span_min,
/// weight_span_max,spans_usage,loads_usage,fe_usage,pole_usage,usage,
/// governs,notes`
/// (usages in %, empty when not checked).
pub fn usage_csv(cases: &[CaseUsage]) -> String {
    use ols_core::text::csv_field;
    let mut out = String::from(
        "case,structure,type,line_angle_deg,wind_span,weight_span_min,weight_span_max,\
         spans_usage,loads_usage,fe_usage,pole_usage,usage,governs,notes\n",
    );
    let pct = |c: &Option<Check>| {
        c.as_ref()
            .map_or(String::new(), |c| format!("{:.1}", c.usage * 100.0))
    };
    for c in cases {
        for s in &c.structures {
            let g = s.governing();
            let (lo, hi) = s
                .weight_span
                .map_or((String::new(), String::new()), |[a, b]| {
                    (format!("{a:.1}"), format!("{b:.1}"))
                });
            let _ = writeln!(
                out,
                "{},{},{},{:.2},{:.1},{lo},{hi},{},{},{},{},{},{},{}",
                csv_field(&c.case),
                csv_field(&s.structure),
                csv_field(s.structure_type.as_deref().unwrap_or("")),
                s.line_angle_deg,
                s.wind_span,
                pct(&s.spans),
                pct(&s.loads),
                pct(&s.fe),
                pct(&s.pole),
                g.map_or(String::new(), |g| format!("{:.1}", g.usage * 100.0)),
                csv_field(g.map_or("", |g| g.governs.as_str())),
                csv_field(&s.notes.join("; ")),
            );
        }
    }
    out
}
