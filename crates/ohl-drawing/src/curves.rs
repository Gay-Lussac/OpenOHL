// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Lines below the lowest conductor (the "clearance curve" under the sag
//! curve): for each circuit, its lowest conductor along the profile,
//! lowered by the ground clearance the project's rules require for it —
//! the ground must stay below the line. The distances come from the
//! clearance rules for the ground (`target = "ground"`, `vertical`), by
//! the circuit's voltage (`by_voltage`, `base` / `per_kv` / `above_kv`),
//! as the clearance checks read them; a design code's preset of those
//! rules fills them in. Nothing is computed that the checks do not: the
//! line is the drawn wires' lowest point at each station, lowered.

use ohl_line::Line;

use crate::{RouteDrawing, Trace};

/// A line below the lowest conductor of one circuit.
#[derive(Debug, Clone, PartialEq)]
pub struct ConductorClearance {
    /// The circuit (its sections' circuit name).
    pub circuit: String,
    /// Its nominal voltage, kV (none: earthed).
    pub voltage_kv: Option<f64>,
    /// The required vertical clearance to the ground, m.
    pub distance: f64,
    /// The rule giving it.
    pub rule: String,
    /// The weather cases its lowest conductor is taken in (names, `*` and
    /// `?` wildcards: the rule's); none: the drawn case with the largest
    /// sag.
    pub cases: Vec<String>,
}

/// The lines below the lowest conductor for every circuit of `line` that
/// a ground rule of `rules` checks (the rule with the largest vertical
/// distance at the circuit's voltage when several do), and notes on what
/// was left out.
///
/// **Conservative:** where several ground rules check a circuit, the
/// largest distance is drawn (the strictest line); the checks themselves
/// report each rule.
pub fn conductor_clearances(
    line: &Line,
    rules: &[ohl_clearance::Rule],
) -> (Vec<ConductorClearance>, Vec<String>) {
    let potentials = line.potentials().unwrap_or_default();
    // Circuits in the order of their first section, with their voltage.
    let mut circuits: Vec<(String, Option<f64>)> = Vec::new();
    for s in &line.sections {
        let kv = potentials
            .get(&s.id)
            .cloned()
            .flatten()
            .map(|p| p.nominal_kv);
        match circuits.iter_mut().find(|c| c.0 == s.circuit_name) {
            Some(c) => {
                c.1 = match (c.1, kv) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (a, b) => a.or(b),
                }
            }
            None => circuits.push((s.circuit_name.clone(), kv)),
        }
    }
    let mut out = Vec::new();
    let mut notes = Vec::new();
    for (circuit, kv) in circuits {
        let mut best: Option<(f64, &ohl_clearance::Rule)> = None;
        for r in rules
            .iter()
            .filter(|r| r.target == ohl_clearance::Target::Ground)
            .filter(|r| r.checks_voltage(kv))
        {
            let Some(req) = &r.vertical else {
                continue;
            };
            match req.at(kv.unwrap_or(0.0)) {
                Ok(d) if d > 0.0 => {
                    if best.is_none_or(|(b, _)| d > b) {
                        best = Some((d, r));
                    }
                }
                Ok(_) => {}
                Err(e) => notes.push(format!(
                    "circuit {circuit}: rule \"{}\": {e} (no line below its lowest conductor)",
                    r.name
                )),
            }
        }
        if let Some((distance, r)) = best {
            out.push(ConductorClearance {
                circuit,
                voltage_kv: kv,
                distance,
                rule: r.name.clone(),
                cases: r.cases.clone(),
            });
        }
    }
    (out, notes)
}

/// The height of trace `t` at station `s`, where it reaches it
/// (interpolated between its points).
fn height_at(t: &Trace, s: f64) -> Option<f64> {
    t.points.windows(2).find_map(|w| {
        let (a, b) = (w[0], w[1]);
        let (lo, hi) = (a[0].min(b[0]), a[0].max(b[0]));
        if s < lo - 1e-9 || s > hi + 1e-9 {
            return None;
        }
        if (b[0] - a[0]).abs() < 1e-12 {
            return Some(a[2].min(b[2]));
        }
        let k = (s - a[0]) / (b[0] - a[0]);
        Some(a[2] + k * (b[2] - a[2]))
    })
}

/// The lowest of `traces` along the stations, as runs of (station, z):
/// at every point of every trace the lowest height of all those reaching
/// that station; a run ends where none does.
pub fn lower_envelope(traces: &[&Trace]) -> Vec<Vec<[f64; 2]>> {
    let mut stations: Vec<f64> = traces
        .iter()
        .flat_map(|t| t.points.iter().map(|p| p[0]))
        .filter(|s| s.is_finite())
        .collect();
    stations.sort_by(f64::total_cmp);
    stations.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    let mut runs: Vec<Vec<[f64; 2]>> = Vec::new();
    let mut run: Vec<[f64; 2]> = Vec::new();
    for s in stations {
        let z = traces
            .iter()
            .filter_map(|t| height_at(t, s))
            .fold(f64::INFINITY, f64::min);
        if z.is_finite() {
            run.push([s, z]);
        } else if !run.is_empty() {
            runs.push(std::mem::take(&mut run));
        }
    }
    if !run.is_empty() {
        runs.push(run);
    }
    runs.retain(|r| r.len() > 1);
    runs
}

/// A line below a lowest conductor, ready to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
    /// Runs of (station, z).
    pub runs: Vec<Vec<[f64; 2]>>,
    /// Its label on the sheet.
    pub label: String,
    /// What the legend says (in English; a template's words:
    /// [`Curve::legend_with`]).
    pub legend: String,
    /// What the legend's text is made of: circuit, voltage ("220 kV",
    /// "earthed"), distance (m, written), rule, the weather cases, whether
    /// the rule named none (the largest sag taken).
    pub parts: CurveParts,
}

/// What a [`Curve`]'s legend entry is made of.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CurveParts {
    /// The circuit.
    pub circuit: String,
    /// Its voltage in words.
    pub voltage: String,
    /// The distance, m, written.
    pub distance: String,
    /// The rule.
    pub rule: String,
    /// The weather cases its lowest conductor is taken in.
    pub case: String,
    /// The rule names no case: the drawn case with the largest sag.
    pub largest_sag: bool,
}

impl Curve {
    /// The legend's text in `legend`'s words (the template's
    /// `[legend.labels]` `conductor_clearance` and
    /// `conductor_clearance_note`).
    pub fn legend_with(&self, legend: &crate::template::LegendSpec) -> String {
        let p = &self.parts;
        // The one line for the whole line ([`envelope`]): its own words.
        if p.circuit.is_empty() {
            return self.legend.clone();
        }
        let note = if p.largest_sag {
            legend.label("conductor_clearance_note", &[("case", &p.case)])
        } else {
            String::new()
        };
        legend.label(
            "conductor_clearance",
            &[
                ("circuit", &p.circuit),
                ("voltage", &p.voltage),
                ("distance", &p.distance),
                ("rule", &p.rule),
                ("case", &p.case),
                ("note", &note),
            ],
        )
    }
}

/// The lines below the lowest conductor of route drawing `d` for the
/// circuits of `wanted` (the template's `circuits`: none, every one), each
/// from its circuit's wires (spans, not jumpers) in its rule's weather
/// cases — none named: the drawn case whose wires of the circuit sag
/// lowest, said in the legend — lowered by its distance; `label` with
/// `{distance}`, `{rule}`, `{circuit}`, `{case}`, `{voltage}` filled. What
/// to warn about (a rule's cases not drawn).
pub fn curves(
    d: &RouteDrawing,
    specs: &[ConductorClearance],
    wanted: &[String],
    label: &str,
) -> (Vec<Curve>, Vec<String>) {
    let mut out = Vec::new();
    let mut warnings = Vec::new();
    for c in specs {
        if !wanted.is_empty() && !wanted.iter().any(|w| w == &c.circuit) {
            continue;
        }
        let of_circuit: Vec<&Trace> = d
            .wires
            .iter()
            .filter(|t| t.circuit == c.circuit && !t.label.starts_with("jumper"))
            .collect();
        if of_circuit.is_empty() {
            continue;
        }
        let (chosen, case_words, largest_sag): (Vec<&Trace>, String, bool) = if c.cases.is_empty() {
            // The case drawn whose wires sag lowest.
            let mut cases: Vec<&str> = of_circuit.iter().map(|t| t.case.as_str()).collect();
            cases.sort_unstable();
            cases.dedup();
            let lowest = |case: &str| {
                of_circuit
                    .iter()
                    .filter(|t| t.case == case)
                    .flat_map(|t| t.points.iter().map(|p| p[2]))
                    .fold(f64::INFINITY, f64::min)
            };
            let Some(case) = cases
                .into_iter()
                .min_by(|a, b| lowest(a).total_cmp(&lowest(b)))
            else {
                continue;
            };
            (
                of_circuit
                    .iter()
                    .copied()
                    .filter(|t| t.case == case)
                    .collect(),
                case.to_owned(),
                true,
            )
        } else {
            let chosen: Vec<&Trace> = of_circuit
                .iter()
                .copied()
                .filter(|t| {
                    c.cases
                        .iter()
                        .any(|p| ohl_core::pattern::matches(p, &t.case))
                })
                .collect();
            if chosen.is_empty() {
                warnings.push(format!(
                    "circuit {}: rule \"{}\" names the weather cases {}, none of them drawn: no \
                     line below its lowest conductor (draw those cases)",
                    c.circuit,
                    c.rule,
                    c.cases.join(", ")
                ));
                continue;
            }
            let mut names: Vec<&str> = chosen.iter().map(|t| t.case.as_str()).collect();
            names.sort_unstable();
            names.dedup();
            (chosen, names.join(", "), false)
        };
        let runs: Vec<Vec<[f64; 2]>> = lower_envelope(&chosen)
            .into_iter()
            .map(|r| r.into_iter().map(|p| [p[0], p[1] - c.distance]).collect())
            .collect();
        let distance = crate::template::format_number(c.distance, "");
        let voltage = c.voltage_kv.map_or("earthed".to_owned(), |kv| {
            format!("{} kV", crate::template::format_number(kv, ""))
        });
        let text = crate::template::fill(label, &|n, _| match n {
            "distance" => Some(distance.clone()),
            "rule" => Some(c.rule.clone()),
            "circuit" => Some(c.circuit.clone()),
            "case" => Some(case_words.clone()),
            "voltage" => Some(voltage.clone()),
            _ => None,
        });
        let mut curve = Curve {
            runs,
            legend: String::new(),
            label: text,
            parts: CurveParts {
                circuit: c.circuit.clone(),
                voltage,
                distance,
                rule: c.rule.clone(),
                case: case_words,
                largest_sag,
            },
        };
        curve.legend = curve.legend_with(&crate::template::LegendSpec::default());
        out.push(curve);
    }
    (out, warnings)
}

/// The circuits' lines `curves` as one: at each station the lowest of
/// those there (the limit the ground must stay under along the whole
/// line), the points where another circuit's line becomes the lowest
/// added; labelled `label` (`{distance}`, `{rule}`, `{case}`: those of the
/// circuits in it, joined). None without a line.
pub fn envelope(curves: &[Curve], label: &str) -> Option<Curve> {
    if curves.is_empty() {
        return None;
    }
    // z of a run at station s (None outside it).
    let at = |run: &[[f64; 2]], s: f64| -> Option<f64> {
        let w = run
            .windows(2)
            .find(|w| s >= w[0][0] - 1e-9 && s <= w[1][0] + 1e-9)?;
        let k = if w[1][0] > w[0][0] {
            ((s - w[0][0]) / (w[1][0] - w[0][0])).clamp(0.0, 1.0)
        } else {
            0.0
        };
        Some(w[0][1] + k * (w[1][1] - w[0][1]))
    };
    let lowest = |s: f64| -> Option<f64> {
        curves
            .iter()
            .flat_map(|c| &c.runs)
            .filter_map(|r| at(r, s))
            .reduce(f64::min)
    };
    let mut stations: Vec<f64> = curves
        .iter()
        .flat_map(|c| &c.runs)
        .flatten()
        .map(|p| p[0])
        .collect();
    stations.sort_by(f64::total_cmp);
    stations.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    // Where the lowest changes between two stations: the crossing added
    // (bisection on which line is lowest, to a millimetre).
    let which = |s: f64| -> Option<usize> {
        let mut best: Option<(usize, f64)> = None;
        for (k, c) in curves.iter().enumerate() {
            for r in &c.runs {
                if let Some(z) = at(r, s)
                    && best.is_none_or(|b| z < b.1)
                {
                    best = Some((k, z));
                }
            }
        }
        best.map(|b| b.0)
    };
    let mut all = Vec::with_capacity(stations.len() * 2);
    for w in stations.windows(2) {
        all.push(w[0]);
        let (a, b) = (which(w[0]), which(w[1]));
        if a.is_some() && b.is_some() && a != b {
            let (mut lo, mut hi) = (w[0], w[1]);
            while hi - lo > 1e-3 {
                let m = 0.5 * (lo + hi);
                if which(m) == a {
                    lo = m;
                } else {
                    hi = m;
                }
            }
            all.push(0.5 * (lo + hi));
        }
    }
    if let Some(l) = stations.last() {
        all.push(*l);
    }
    // Runs where some line is (a gap where none is).
    let mut runs: Vec<Vec<[f64; 2]>> = Vec::new();
    let mut run: Vec<[f64; 2]> = Vec::new();
    for s in all {
        match lowest(s) {
            Some(z) => run.push([s, z]),
            None if run.len() > 1 => runs.push(std::mem::take(&mut run)),
            None => run.clear(),
        }
    }
    if run.len() > 1 {
        runs.push(run);
    }
    // Each value once (distances by size, the others by name).
    let joined = |f: &dyn Fn(&CurveParts) -> &str| {
        let mut v: Vec<&str> = curves.iter().map(|c| f(&c.parts)).collect();
        v.sort_by(|a, b| match (a.parse::<f64>(), b.parse::<f64>()) {
            (Ok(x), Ok(y)) => x.total_cmp(&y),
            _ => a.cmp(b),
        });
        v.dedup();
        v.join(" / ")
    };
    let (distance, rule, case) = (
        joined(&|p| p.distance.as_str()),
        joined(&|p| p.rule.as_str()),
        joined(&|p| p.case.as_str()),
    );
    let text = crate::template::fill(label, &|n, _| match n {
        "distance" => Some(distance.clone()),
        "rule" => Some(rule.clone()),
        "case" => Some(case.clone()),
        _ => None,
    });
    Some(Curve {
        runs,
        legend: text.clone(),
        label: text,
        parts: CurveParts {
            circuit: String::new(),
            voltage: String::new(),
            distance,
            rule,
            case,
            largest_sag: false,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two circuits' lines crossing: one line, at each station the lower
    /// of them, the crossing point added; its label with both distances.
    #[test]
    fn the_envelope_is_the_lowest_line() {
        let line = |circuit: &str, runs: Vec<Vec<[f64; 2]>>, distance: &str| Curve {
            runs,
            label: String::new(),
            legend: String::new(),
            parts: CurveParts {
                circuit: circuit.into(),
                distance: distance.into(),
                rule: "ground".into(),
                case: "80 °C".into(),
                ..CurveParts::default()
            },
        };
        let a = line("A", vec![vec![[0.0, 10.0], [100.0, 20.0]]], "7");
        let b = line("B", vec![vec![[0.0, 20.0], [100.0, 10.0]]], "9");
        let e = envelope(&[a, b], "limit ({distance} m, {rule})").unwrap();
        assert_eq!(e.label, "limit (7 / 9 m, ground)");
        assert_eq!(
            e.legend_with(&crate::template::LegendSpec::default()),
            e.label
        );
        let run = &e.runs[0];
        assert_eq!(run.first(), Some(&[0.0, 10.0]));
        assert_eq!(run.last(), Some(&[100.0, 10.0]));
        // The crossing at 50 m, 15 m (within the bisection's millimetre).
        let mid = run.iter().find(|p| (p[0] - 50.0).abs() < 0.01).unwrap();
        assert!((mid[1] - 15.0).abs() < 0.01, "{run:?}");
        assert!(envelope(&[], "x").is_none());
    }

    /// A span from station 0 to 300 m between attachments at 30 m, its
    /// lowest point `sag` m lower at 150 m (a parabola), on `circuit`.
    fn span(circuit: &str, kv: f64, sag: f64, case: &str) -> Trace {
        Trace {
            label: format!("{circuit} span 0"),
            points: (0..=30)
                .map(|k| {
                    let s = 10.0 * f64::from(k);
                    let u = s / 300.0;
                    [s, 0.0, 530.0 - 4.0 * sag * u * (1.0 - u)]
                })
                .collect(),
            circuit: circuit.into(),
            voltage_kv: Some(kv),
            case: case.into(),
            ..Trace::default()
        }
    }

    /// Checked by hand: the line lies the rule's distance below the
    /// lowest conductor at several stations, for two voltages (110 kV:
    /// 6 m, 380 kV: 7 m from the table [[150, 6], [420, 7]]), each from
    /// its own circuit.
    #[test]
    fn the_line_is_the_distance_below_the_lowest_conductor() {
        let rules = ohl_clearance::RuleSet::parse(
            "format = \"ohl-clearance-rules\"\nversion = 1\n[[rule]]\nname = \"Ground\"\n\
             target = \"ground\"\nvertical = { by_voltage = [[150, 6.0], [420, 7.0]] }",
        )
        .unwrap()
        .rules;
        let specs = [
            ConductorClearance {
                circuit: "A".into(),
                voltage_kv: Some(110.0),
                distance: rules[0].vertical.as_ref().unwrap().at(110.0).unwrap(),
                rule: "Ground".into(),
                cases: Vec::new(),
            },
            ConductorClearance {
                circuit: "B".into(),
                voltage_kv: Some(380.0),
                distance: rules[0].vertical.as_ref().unwrap().at(380.0).unwrap(),
                rule: "Ground".into(),
                cases: Vec::new(),
            },
        ];
        assert_eq!((specs[0].distance, specs[1].distance), (6.0, 7.0));
        // Circuit A: two phases (sags 8 m and 10 m) in the case "hot", one
        // phase sagging 11 m in "cold" (not the lowest overall: "hot" has
        // the lowest wire at 520 m, "cold" 519 m — "cold" is chosen).
        let d = RouteDrawing {
            wires: vec![
                span("A", 110.0, 8.0, "hot"),
                span("A", 110.0, 10.0, "hot"),
                span("A", 110.0, 11.0, "cold"),
                span("B", 380.0, 6.0, "hot"),
            ],
            ..RouteDrawing::default()
        };
        let (c, w) = curves(&d, &specs, &[], "{distance} m ({rule}, {voltage})");
        assert!(w.is_empty());
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].label, "6 m (Ground, 110 kV)");
        assert!(c[0].legend.contains("largest sag, cold"), "{}", c[0].legend);
        // By hand, A in "cold": z = 530 − 44·u(1 − u); at 150 m (u = ½)
        // 519 m, the line 513 m; at 60 m (u = 0.2) 530 − 7.04 = 522.96,
        // the line 516.96 (and at 240 m, u = 0.8); at 0 m 530, the line 524.
        let at = |runs: &Vec<Vec<[f64; 2]>>, s: f64| {
            runs[0].iter().find(|p| (p[0] - s).abs() < 1e-9).unwrap()[1]
        };
        for (s, z) in [
            (150.0, 513.0),
            (60.0, 516.96),
            (0.0, 524.0),
            (240.0, 516.96),
        ] {
            assert!((at(&c[0].runs, s) - z).abs() < 1e-9, "A at {s}");
        }
        // B (380 kV, 7 m): z = 530 − 24·u(1 − u): at 150 m 524, the line
        // 517; at 30 m (u = 0.1) 530 − 2.16 = 527.84, the line 520.84.
        for (s, z) in [(150.0, 517.0), (30.0, 520.84)] {
            assert!((at(&c[1].runs, s) - z).abs() < 1e-9, "B at {s}");
        }
        // The rule names a case: that one ("hot": the lower of its two
        // phases, 520 m at 150 m, the line 514 m).
        let named = [ConductorClearance {
            cases: vec!["h*".into()],
            ..specs[0].clone()
        }];
        let (c, _) = curves(&d, &named, &[], "{case}");
        assert_eq!(c[0].label, "hot");
        assert!((at(&c[0].runs, 150.0) - 514.0).abs() < 1e-9);
        // A case not drawn: warned, no line.
        let missing = [ConductorClearance {
            cases: vec!["ice".into()],
            ..specs[0].clone()
        }];
        let (c, w) = curves(&d, &missing, &[], "");
        assert!(c.is_empty() && w[0].contains("none of them drawn"));
        // Only the circuits wanted.
        assert_eq!(curves(&d, &specs, &["B".into()], "").0.len(), 1);
    }

    #[test]
    fn the_rules_give_each_circuit_its_distance() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/synthetic-line");
        let line = Line::load(&root.join("synthetic-line.ohl.json")).unwrap();
        let rules = ohl_clearance::RuleSet::parse(
            "format = \"ohl-clearance-rules\"\nversion = 1\n[[rule]]\nname = \"low\"\n\
             target = \"ground\"\nvertical = \"5 m\"\n[[rule]]\nname = \"high\"\n\
             target = \"ground\"\nvertical = { base = \"6 m\", per_kv = \"0.01 m\", above_kv = 110 }\n\
             cases = [\"max temperature\"]",
        )
        .unwrap()
        .rules;
        let (c, notes) = conductor_clearances(&line, &rules);
        assert!(notes.is_empty(), "{notes:?}");
        assert!(!c.is_empty());
        for x in &c {
            // The larger of 5 m and 6 + 0.01 (U − 110).
            let kv = x.voltage_kv.unwrap_or(0.0);
            let want = (6.0 + 0.01 * (kv - 110.0).max(0.0)).max(5.0);
            assert!((x.distance - want).abs() < 1e-9, "{x:?}");
            assert_eq!(x.rule, "high");
            assert_eq!(x.cases, ["max temperature"]);
        }
    }
}
