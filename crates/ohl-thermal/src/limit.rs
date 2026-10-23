// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Maximum operating temperature from clearance: per section, the highest
//! conductor temperature at which its wires still meet every ground and
//! point (LiDAR) rule of the clearance rules.
//!
//! Each section is solved alone ([`ohl_line::solve::solve_one`]) in a base
//! weather case (still air, or a case of the user's: ice, wind, creep
//! state) with only its temperature changed, and checked with
//! [`ohl_clearance::check_case`]:
//!
//! 1. at the lowest temperature of the range (fails: the section is below
//!    the range);
//! 2. upwards in steps of `scan_step` °C until a check fails (none fails:
//!    above the range);
//! 3. bisection between the last temperature that passed and the first
//!    that failed, until they are `tolerance` °C apart. The limit is the
//!    temperature that passed (on the safe side).
//!
//! Clearance falls with temperature as the sag grows; the scan finds the
//! first failure on its grid even where it does not (e.g. a structure
//! swinging), so a temperature range that passes between two failures
//! narrower than the step can be missed.
//!
//! Cost: with the defaults (0–150 °C, 10 °C steps, 0.5 °C) at most
//! 1 + 15 + 5 = 21 solutions and checks per section — milliseconds for
//! a short section, about a second for a long section with LiDAR points.
//! Exit clauses: the caller's budget counts every solution
//! ([`default_budget`]).
//!
//! Wire-to-wire rules (same line, other lines, case pairs) are not
//! checked: heating every section alike hardly changes their distances.

use std::time::Duration;

use ohl_cables::Library;
use ohl_clearance::{CaseTargets, Outcome, Rule, Target};
use ohl_core::budget::{Budget, Run};
use ohl_core::units::Temperature;
use ohl_line::Line;
use ohl_terrain::Terrain;
use ohl_weather::WeatherCase;

/// Temperature range and resolution of the search.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LimitOptions {
    /// Lowest temperature tried, °C.
    pub min_temperature: f64,
    /// Highest temperature tried, °C.
    pub max_temperature: f64,
    /// Step of the upward scan, °C.
    pub scan_step: f64,
    /// Bisection stops when the bracket is this narrow, °C.
    pub tolerance: f64,
}

impl Default for LimitOptions {
    fn default() -> Self {
        Self {
            min_temperature: 0.0,
            max_temperature: 150.0,
            scan_step: 10.0,
            tolerance: 0.5,
        }
    }
}

impl LimitOptions {
    /// Checks the values.
    pub fn validate(&self) -> Result<(), String> {
        if !(self.max_temperature > self.min_temperature
            && self.scan_step > 0.0
            && self.tolerance > 0.0
            && self.max_temperature.is_finite()
            && self.min_temperature.is_finite())
        {
            return Err(
                "temperature range: the maximum must exceed the minimum; step and tolerance > 0"
                    .into(),
            );
        }
        Ok(())
    }

    /// The most solutions one section can take.
    pub fn solves_per_section(&self) -> u64 {
        let scan = ((self.max_temperature - self.min_temperature) / self.scan_step).ceil();
        let bisect = (self.scan_step / self.tolerance).log2().ceil().max(0.0);
        2 + scan as u64 + bisect as u64
    }
}

/// Exit clauses for `sections` sections: every solution of the search
/// (plus slack), and 30 minutes.
pub fn default_budget(options: &LimitOptions, sections: usize) -> Budget {
    Budget::new()
        .max_iterations(options.solves_per_section() * sections as u64 + 10)
        .time_limit(Duration::from_secs(1800))
}

/// What a section's search found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Limit {
    /// The highest temperature found that meets every clearance, °C.
    At(f64),
    /// Even the lowest temperature of the range fails.
    BelowRange,
    /// The highest temperature of the range passes.
    AboveRange,
}

/// The clearance that fails just above the limit.
#[derive(Debug, Clone, PartialEq)]
pub struct Governing {
    /// Rule name.
    pub rule: String,
    /// Span of the section (0 = first).
    pub span: usize,
    /// `vertical` or `radial`.
    pub measure: &'static str,
    /// Clearance found, m.
    pub clearance: f64,
    /// Clearance required, m.
    pub required: f64,
    /// Temperature it was found at, °C.
    pub temperature: f64,
    /// Where on the wire.
    pub at: [f64; 3],
}

/// The limit of one section.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionLimit {
    /// Section id.
    pub section: String,
    /// Its cable.
    pub cable: String,
    /// The limit, or why there is none.
    pub result: Result<Limit, String>,
    /// The failing clearance (none above the range).
    pub governing: Option<Governing>,
    /// Solutions used.
    pub solves: u32,
}

/// What the wires are checked against.
pub struct Targets<'a> {
    /// The ground surface (of the ground rules naming none).
    pub ground: Option<&'a Terrain>,
    /// The terrain surfaces the ground rules name ([`Rule::surface`]),
    /// by name (as [`CaseTargets::surfaces`]).
    pub surfaces: &'a [(String, &'a Terrain)],
    /// Classes of each point rule, by rule index (as
    /// [`CaseTargets::point_classes`]).
    pub point_classes: &'a [Vec<u8>],
    /// Feeds the points of the classes asked for (none: no points).
    #[allow(clippy::type_complexity)]
    pub points:
        Option<&'a mut dyn FnMut(&[u8], &mut dyn FnMut(f64, f64, f64, u8)) -> Result<(), String>>,
}

/// The rules checked (ground and points, applying to `case`), with their
/// point classes.
fn chosen_rules(rules: &[Rule], classes: &[Vec<u8>], case: &str) -> (Vec<Rule>, Vec<Vec<u8>>) {
    rules
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            ohl_clearance::applies(r, case)
                && r.case_pairs.is_empty()
                && matches!(r.target, Target::Ground | Target::Points { .. })
        })
        .map(|(k, r)| (r.clone(), classes.get(k).cloned().unwrap_or_default()))
        .unzip()
}

struct Search<'s, 'a> {
    line: &'s Line,
    library: &'s Library,
    base: &'s WeatherCase,
    rules: Vec<Rule>,
    classes: Vec<Vec<u8>>,
    voltages: Vec<Option<ohl_line::electrical::Potential>>,
    targets: &'s mut Targets<'a>,
    checked_any: bool,
}

impl Search<'_, '_> {
    /// Checks `section` at `t` °C: `None` when every clearance is met,
    /// else the worst shortfall.
    fn check(
        &mut self,
        section: &ohl_line::Section,
        t: f64,
        run: &mut Run,
        solves: &mut u32,
    ) -> Result<Option<Governing>, String> {
        run.step().map_err(|e| e.to_string())?;
        *solves += 1;
        let mut case = self.base.clone();
        case.temperature = Temperature::from_celsius(t);
        let shape = ohl_line::solve::solve_one(self.line, section, self.library, &case)
            .map_err(|e| e.to_string())?;
        let wires = ohl_clearance::wires_of(self.line, &[shape], &[]);
        let points = self.targets.points.as_mut().map(|f| &mut **f as _);
        let checks = ohl_clearance::check_case(
            &self.rules,
            &case.name,
            &wires,
            CaseTargets {
                voltages: &self.voltages,
                other_voltages: &[],
                ground: self.targets.ground,
                surfaces: self.targets.surfaces,
                point_classes: &self.classes,
                points,
                other_wires: &[],
            },
        )?;
        let mut worst: Option<Governing> = None;
        for c in &checks {
            let Outcome::Wires(results) = &c.outcome else {
                continue;
            };
            self.checked_any = true;
            for (w, r) in results.iter().enumerate() {
                for (measure, hit, required, _) in r.measures() {
                    let (Some(h), Some(req)) = (hit, required) else {
                        continue;
                    };
                    if h.distance >= req {
                        continue;
                    }
                    let short = req - h.distance;
                    if worst
                        .as_ref()
                        .is_none_or(|g| short > g.required - g.clearance)
                    {
                        worst = Some(Governing {
                            rule: self.rules[c.rule].name.clone(),
                            span: c.wires[w].span,
                            measure,
                            clearance: h.distance,
                            required: req,
                            temperature: t,
                            at: h.wire,
                        });
                    }
                }
            }
        }
        Ok(worst)
    }

    fn section(
        &mut self,
        section: &ohl_line::Section,
        o: &LimitOptions,
        run: &mut Run,
    ) -> SectionLimit {
        let mut solves = 0;
        let mut governing = None;
        let result = (|| -> Result<Limit, String> {
            if let Some(g) = self.check(section, o.min_temperature, run, &mut solves)? {
                governing = Some(g);
                return Ok(Limit::BelowRange);
            }
            let mut lo = o.min_temperature;
            let mut hi = None;
            while lo < o.max_temperature {
                let t = (lo + o.scan_step).min(o.max_temperature);
                match self.check(section, t, run, &mut solves)? {
                    Some(g) => {
                        governing = Some(g);
                        hi = Some(t);
                        break;
                    }
                    None => lo = t,
                }
            }
            let Some(mut hi) = hi else {
                return Ok(Limit::AboveRange);
            };
            while hi - lo > o.tolerance {
                let mid = 0.5 * (lo + hi);
                match self.check(section, mid, run, &mut solves)? {
                    Some(g) => {
                        governing = Some(g);
                        hi = mid;
                    }
                    None => lo = mid,
                }
            }
            Ok(Limit::At(lo))
        })();
        SectionLimit {
            section: section.id.clone(),
            cable: section.cable.clone(),
            result,
            governing,
            solves,
        }
    }
}

/// The clearance-limited temperature of every section of `line`: solved in
/// weather case `base` at other temperatures, checked by the ground and
/// point `rules` that apply to `base`. `progress` hears (sections done,
/// sections). Fails when no such rule applies, a ground rule names a
/// surface `targets` does not hold, or none could be checked (no ground,
/// no points); a section that cannot be solved, or the budget
/// running out, gives that section (and those after it) an error.
#[allow(clippy::too_many_arguments)]
pub fn clearance_limits(
    line: &Line,
    library: &Library,
    base: &WeatherCase,
    rules: &[Rule],
    targets: &mut Targets,
    options: &LimitOptions,
    budget: &Budget,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<Vec<SectionLimit>, String> {
    options.validate()?;
    let (rules, classes) = chosen_rules(rules, targets.point_classes, &base.name);
    if rules.is_empty() {
        return Err(format!(
            "no ground or point clearance rule applies to weather case \"{}\"",
            base.name
        ));
    }
    // A ground rule naming a surface not given would be skipped (no
    // ground) and the limit found without it: refused instead.
    for r in &rules {
        if let (Target::Ground, Some(name)) = (&r.target, &r.surface)
            && !targets.surfaces.iter().any(|(n, _)| n == name)
        {
            return Err(format!(
                "ground rule \"{}\": no terrain surface \"{name}\" (the project holds none of that name, or it could not be read)",
                r.name
            ));
        }
    }
    let voltages = ohl_clearance::voltages_of(line)?;
    for r in &rules {
        r.check_voltages(&voltages, &[])?;
    }
    let mut search = Search {
        line,
        library,
        base,
        rules,
        classes,
        voltages,
        targets,
        checked_any: false,
    };
    let mut run = budget.start();
    let n = line.sections.len();
    let mut out = Vec::with_capacity(n);
    for (k, section) in line.sections.iter().enumerate() {
        progress(k, n);
        let limit = search.section(section, options, &mut run);
        if !search.checked_any && limit.result.is_ok() {
            return Err(
                "no clearance could be checked: the rules need a terrain surface or points".into(),
            );
        }
        out.push(limit);
    }
    progress(n, n);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ohl_clearance::RuleSet;
    use std::path::Path;

    fn root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn example() -> (Line, Library, Terrain) {
        let ex = root().join("examples/synthetic-line");
        (
            Line::load(&ex.join("synthetic-line.ohl.json")).unwrap(),
            Library::load(&root().join("libraries/cables")).unwrap(),
            Terrain::load(&ex.join("synthetic-survey.csv")).unwrap(),
        )
    }

    fn limits(required: f64, options: &LimitOptions) -> Vec<SectionLimit> {
        let (line, library, terrain) = example();
        let base = WeatherCase::still("hot", Temperature::from_celsius(15.0));
        let rules = RuleSet::ground(required).rules;
        let mut targets = Targets {
            ground: Some(&terrain),
            surfaces: &[],
            point_classes: &[],
            points: None,
        };
        let mut calls = 0;
        let out = clearance_limits(
            &line,
            &library,
            &base,
            &rules,
            &mut targets,
            options,
            &default_budget(options, line.sections.len()),
            &mut |_, _| calls += 1,
        )
        .unwrap();
        assert_eq!(calls, line.sections.len() + 1);
        out
    }

    #[test]
    fn the_limit_is_where_the_clearance_is_just_met() {
        let options = LimitOptions::default();
        let (line, library, terrain) = example();
        let out = limits(5.0, &options);
        assert_eq!(out.len(), line.sections.len());
        let s2 = out.iter().find(|s| s.section == "S2").unwrap();
        let Ok(Limit::At(t)) = s2.result else {
            panic!("{s2:?}");
        };
        // The example's lowest clearance is 6–7 m at 15 °C, 4–5.5 m at 80 °C.
        assert!((15.0..80.0).contains(&t), "{t}");
        let g = s2.governing.as_ref().unwrap();
        assert_eq!((g.span, g.measure), (1, "vertical"));
        assert!(g.clearance < 5.0 && g.temperature - t <= options.tolerance + 1e-9);
        assert!(s2.solves as u64 <= options.solves_per_section());
        // Just below the limit the clearance is met, just above it is not.
        let rule = &RuleSet::ground(5.0).rules[0];
        let section = line.sections.iter().find(|s| s.id == "S2").unwrap();
        let lowest = |t: f64| {
            let case = WeatherCase::still("t", Temperature::from_celsius(t));
            let shape = ohl_line::solve::solve_one(&line, section, &library, &case).unwrap();
            let wires = ohl_clearance::wires_of(&line, &[shape], &[]);
            ohl_clearance::checks::ground_check(rule, &wires, &terrain)
                .iter()
                .map(|r| r.vertical.unwrap().distance)
                .fold(f64::INFINITY, f64::min)
        };
        assert!(lowest(t) >= 5.0);
        assert!(lowest(t + options.tolerance) < 5.0);
    }

    #[test]
    fn ranges_and_errors() {
        // Nothing required: every section passes the whole range.
        let options = LimitOptions {
            max_temperature: 60.0,
            ..LimitOptions::default()
        };
        assert!(
            limits(0.0, &options)
                .iter()
                .all(|s| s.result == Ok(Limit::AboveRange) && s.governing.is_none())
        );
        // 30 m: every section fails at once.
        assert!(
            limits(30.0, &options)
                .iter()
                .all(|s| s.result == Ok(Limit::BelowRange) && s.governing.is_some())
        );
        // No ground: nothing can be checked.
        let (line, library, _) = example();
        let base = WeatherCase::still("hot", Temperature::from_celsius(15.0));
        let mut targets = Targets {
            ground: None,
            surfaces: &[],
            point_classes: &[],
            points: None,
        };
        let budget = default_budget(&options, 4);
        let rules = RuleSet::ground(5.0).rules;
        let r = clearance_limits(
            &line,
            &library,
            &base,
            &rules,
            &mut targets,
            &options,
            &budget,
            &mut |_, _| {},
        );
        assert!(r.unwrap_err().contains("terrain surface"));
        // A budget of three solutions: the first section stops.
        let (_, _, terrain) = example();
        targets.ground = Some(&terrain);
        let tiny = Budget::new().max_iterations(3);
        let out = clearance_limits(
            &line,
            &library,
            &base,
            &rules,
            &mut targets,
            &options,
            &tiny,
            &mut |_, _| {},
        )
        .unwrap();
        assert!(out.iter().any(|s| s.result.is_err()));
    }

    #[test]
    fn a_ground_rule_checks_the_surface_it_names() {
        let (line, library, terrain) = example();
        // The example's survey surface raised by 1 m.
        let raised = Terrain::from_mesh(
            terrain
                .points()
                .iter()
                .map(|p| [p[0], p[1], p[2] + 1.0])
                .collect(),
            terrain.triangles().to_vec(),
        )
        .unwrap();
        let options = LimitOptions::default();
        let base = WeatherCase::still("hot", Temperature::from_celsius(15.0));
        let budget = default_budget(&options, line.sections.len());
        let named = [("raised TIN".to_owned(), &raised)];
        let s2 = |surface: Option<&str>| {
            let mut rules = RuleSet::ground(5.0).rules;
            rules[0].surface = surface.map(str::to_owned);
            let mut targets = Targets {
                ground: Some(&terrain),
                surfaces: &named,
                point_classes: &[],
                points: None,
            };
            clearance_limits(
                &line,
                &library,
                &base,
                &rules,
                &mut targets,
                &options,
                &budget,
                &mut |_, _| {},
            )
            .map(|out| out.into_iter().find(|s| s.section == "S2").unwrap())
        };
        let Ok(Limit::At(default)) = s2(None).unwrap().result else {
            panic!("default surface");
        };
        let own = s2(Some("raised TIN")).unwrap();
        let Ok(Limit::At(t)) = own.result else {
            panic!("{own:?}");
        };
        // 1 m less clearance: a lower limit.
        assert!(t < default - options.tolerance, "{t} {default}");
        assert!(own.governing.unwrap().clearance < 5.0);
        // A name the surfaces do not hold is reported, not passed.
        let e = s2(Some("no such TIN")).unwrap_err();
        assert!(e.contains("no such TIN"), "{e}");
    }
}
