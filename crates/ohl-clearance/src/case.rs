// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The checks of one weather case, as `ohl line` and the application run
//! them: every rule that applies to the case, its parts of the wires
//! (exclusion zones) against the ground, points of its classes, the
//! line's other wires or other lines' wires. Reporting is left to the
//! caller.

use ohl_line::Line;
use ohl_line::electrical::Potential;
use ohl_terrain::Terrain;

use crate::checks::{
    Hit, PairMode, PairResult, PointCheck, Wire, WireResult, WireSet, checked_parts, ground_check,
    wire_pairs,
};
use crate::rules::{Rule, Target};

/// The distinct potentials of a line's sections (earthed: `None`).
pub fn voltages_of(line: &Line) -> Result<Vec<Option<Potential>>, String> {
    let mut v: Vec<Option<Potential>> = Vec::new();
    for p in line.potentials().map_err(|e| e.to_string())?.into_values() {
        if !v.contains(&p) {
            v.push(p);
        }
    }
    Ok(v)
}

/// Whether `rule` applies to weather case `case` (its `cases` list,
/// `*` `?` wildcards, case-insensitive; empty: every case).
pub fn applies(rule: &Rule, case: &str) -> bool {
    rule.cases.is_empty()
        || rule
            .cases
            .iter()
            .any(|c| ohl_core::pattern::matches(c, case))
}

/// The lowest vertical and radial clearances over `results`, in words:
/// "lowest vertical 6.12 m, radial 5.80 m", or with `places` where on the
/// wire: "lowest vertical 6.12 m at (x, y); lowest radial …".
pub fn lowest<'a>(results: impl Iterator<Item = &'a WireResult> + Clone, places: bool) -> String {
    let min = |f: fn(&WireResult) -> Option<Hit>| {
        results
            .clone()
            .filter_map(f)
            .min_by(|x, y| x.distance.total_cmp(&y.distance))
    };
    let found: Vec<(&str, Hit)> = [
        ("vertical", min(|r| r.vertical)),
        ("radial", min(|r| r.radial)),
    ]
    .into_iter()
    .filter_map(|(k, h)| h.map(|h| (k, h)))
    .collect();
    if found.is_empty() {
        return "nothing within reach".into();
    }
    if places {
        found
            .iter()
            .map(|(k, h)| {
                format!(
                    "lowest {k} {:.2} m at ({:.1}, {:.1})",
                    h.distance, h.wire[0], h.wire[1]
                )
            })
            .collect::<Vec<_>>()
            .join("; ")
    } else {
        let parts: Vec<String> = found
            .iter()
            .map(|(k, h)| format!("{k} {:.2} m", h.distance))
            .collect();
        format!("lowest {}", parts.join(", "))
    }
}

/// Why a rule was not checked in a weather case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skipped {
    /// A ground rule, and no ground surface.
    NoGround,
    /// A point rule whose classes and roles match no class of the scheme.
    NoClasses,
    /// A point rule, and no points.
    NoPoints,
    /// A rule on other lines, and no other line.
    NoOtherLines,
    /// A wire rule with case pairs: checked across weather cases.
    AcrossCases,
}

/// What one rule found in a weather case.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// One result per wire (ground and point rules).
    Wires(Vec<WireResult>),
    /// Wire pairs: `a` indexes the rule's wires, `b` the same wires (same
    /// line) or the other lines' wires.
    Pairs(Vec<PairResult>),
    /// Not checked.
    Skipped(Skipped),
}

/// One rule checked in a weather case.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleCheck {
    /// Index of the rule.
    pub rule: usize,
    /// The rule's parts of the wires (exclusion zones left out).
    pub wires: Vec<Wire>,
    /// What it found.
    pub outcome: Outcome,
}

/// What a weather case is checked against.
pub struct CaseTargets<'a> {
    /// Potentials of the line's sections ([`voltages_of`]).
    pub voltages: &'a [Option<Potential>],
    /// Potentials of the other lines' sections.
    pub other_voltages: &'a [Option<Potential>],
    /// The ground surface (of the ground rules naming none).
    pub ground: Option<&'a Terrain>,
    /// The terrain surfaces the ground rules name ([`Rule::surface`]),
    /// by name.
    pub surfaces: &'a [(String, &'a Terrain)],
    /// Classes of each point rule, by rule index (others: ignored).
    pub point_classes: &'a [Vec<u8>],
    /// Feeds the points of the classes asked for to the visitor (called
    /// once, when a point rule applies); `None`: no points.
    #[allow(clippy::type_complexity)]
    pub points:
        Option<&'a mut dyn FnMut(&[u8], &mut dyn FnMut(f64, f64, f64, u8)) -> Result<(), String>>,
    /// The other lines' wires in this weather case.
    pub other_wires: &'a [Wire],
}

/// The terrain surface of ground rule `rule`: the one it names, else the
/// checks' own.
fn ground_of<'a>(
    rule: &Rule,
    ground: Option<&'a Terrain>,
    surfaces: &[(String, &'a Terrain)],
) -> Option<&'a Terrain> {
    match &rule.surface {
        Some(name) => surfaces.iter().find(|(n, _)| n == name).map(|(_, t)| *t),
        None => ground,
    }
}

/// Checks the `wires` of weather case `case` by every rule that applies
/// to it, in rule order. Wire rules with case pairs are left for the
/// check across cases ([`Skipped::AcrossCases`]).
pub fn check_case(
    rules: &[Rule],
    case: &str,
    wires: &[Wire],
    targets: CaseTargets,
) -> Result<Vec<RuleCheck>, String> {
    let active: Vec<usize> = (0..rules.len())
        .filter(|&k| applies(&rules[k], case))
        .collect();
    // How far the wire-to-wire checks look: a screen's band, not its
    // 1000 m (2026-10-27).
    let reach = active
        .iter()
        .map(|&k| rules[k].reach_in_plan(targets.voltages, targets.other_voltages))
        .fold(0.0, f64::max);
    // The reach of every rule, as the point checks were capped before.
    let every = active
        .iter()
        .map(|&k| rules[k].reach(targets.voltages, targets.other_voltages))
        .fold(0.0, f64::max);
    let classes = |k: usize| targets.point_classes.get(k).map_or(&[][..], Vec::as_slice);
    let mut out: Vec<RuleCheck> = Vec::new();
    let mut sets: Vec<Option<WireSet>> = Vec::new();
    for &k in &active {
        let rule = &rules[k];
        let (parts, outcome) = if !rule.case_pairs.is_empty() {
            (Vec::new(), Some(Outcome::Skipped(Skipped::AcrossCases)))
        } else {
            let parts = checked_parts(rule, wires);
            let skipped = match &rule.target {
                Target::Ground if ground_of(rule, targets.ground, targets.surfaces).is_none() => {
                    Some(Skipped::NoGround)
                }
                Target::Points { .. } if classes(k).is_empty() => Some(Skipped::NoClasses),
                Target::Points { .. } if targets.points.is_none() => Some(Skipped::NoPoints),
                Target::OtherLines if targets.other_wires.is_empty() => Some(Skipped::NoOtherLines),
                _ => None,
            };
            (parts, skipped.map(Outcome::Skipped))
        };
        sets.push(match rule.target {
            _ if outcome.is_some() => None,
            // A point rule: its own reach (no point beyond it counts:
            // the results are those of the reach of every rule), its
            // segments listed as far in plan as a point counts
            // ([`crate::checks::plan_reach`]).
            Target::Points { .. } => {
                let own = parts
                    .iter()
                    .map(|w| rule.required(w.kv()).largest() + rule.search_margin)
                    .fold(
                        rule.reach(targets.voltages, targets.other_voltages),
                        f64::max,
                    )
                    .min(every);
                let plan: Vec<f64> = parts
                    .iter()
                    .map(|w| crate::checks::plan_reach(rule, w, own, [true, true]))
                    .collect();
                Some(WireSet::within(parts.clone(), own, None, &plan))
            }
            Target::SameLine { .. } => Some(WireSet::new(parts.clone(), reach)),
            _ => None,
        });
        out.push(RuleCheck {
            rule: k,
            wires: parts,
            // Filled in below.
            outcome: outcome.unwrap_or(Outcome::Wires(Vec::new())),
        });
    }
    let checked = |c: &RuleCheck| !matches!(c.outcome, Outcome::Skipped(_));

    // Point rules: one pass over the points for all of them.
    let mut point_checks: Vec<Option<PointCheck>> = out
        .iter()
        .zip(&sets)
        .map(|(c, set)| {
            let rule = &rules[c.rule];
            (checked(c) && matches!(rule.target, Target::Points { .. }))
                .then(|| PointCheck::new(rule, classes(c.rule), set.as_ref().expect("indexed")))
        })
        .collect();
    if let Some(feed) = targets.points
        && point_checks.iter().any(Option::is_some)
    {
        let mut wanted = [false; 256];
        for (c, _) in out.iter().zip(&point_checks).filter(|(_, p)| p.is_some()) {
            for &class in classes(c.rule) {
                wanted[class as usize] = true;
            }
        }
        let wanted: Vec<u8> = (0..=255u8).filter(|c| wanted[*c as usize]).collect();
        feed(&wanted, &mut |x, y, z, c| {
            for (check, set) in point_checks.iter_mut().zip(&sets) {
                if let (Some(check), Some(set)) = (check, set) {
                    check.visit(set, x, y, z, c);
                }
            }
        })?;
    }

    let other_set = (!targets.other_wires.is_empty())
        .then(|| WireSet::new(targets.other_wires.to_vec(), reach));
    for (n, c) in out.iter_mut().enumerate() {
        if !checked(c) {
            continue;
        }
        let rule = &rules[c.rule];
        c.outcome = match &rule.target {
            Target::Ground => Outcome::Wires(ground_check(
                rule,
                &c.wires,
                ground_of(rule, targets.ground, targets.surfaces).expect("checked above"),
            )),
            Target::Points { .. } => {
                Outcome::Wires(point_checks[n].take().expect("built above").results)
            }
            Target::SameLine { include_same_phase } => Outcome::Pairs(wire_pairs(
                rule,
                &c.wires,
                sets[n].as_ref().expect("indexed"),
                PairMode::SameLine {
                    include_same_phase: *include_same_phase,
                },
            )),
            Target::OtherLines => Outcome::Pairs(wire_pairs(
                rule,
                &c.wires,
                other_set.as_ref().expect("checked above"),
                PairMode::OtherLines,
            )),
        };
    }
    Ok(out)
}

/// A wire rule with case pairs checked across two weather cases: the
/// rule's wires (its parts, exclusion zones left out) in the first case
/// against those in the second.
#[derive(Debug, Clone, PartialEq)]
pub struct CasePairCheck {
    /// Index of the rule.
    pub rule: usize,
    /// The first case (index in the cases given).
    pub first: usize,
    /// The second case (index in the cases given).
    pub second: usize,
    /// The rule's parts of the wires in the first case.
    pub wires: Vec<Wire>,
    /// The rule's parts of the wires in the second case.
    pub others: Vec<Wire>,
    /// The pairs found: `a` indexes [`CasePairCheck::wires`], `b`
    /// [`CasePairCheck::others`].
    pub pairs: Vec<PairResult>,
}

/// The rules between the wires of this line with case pairs
/// (`case_pairs`, [`Skipped::AcrossCases`] in [`check_case`]): for each
/// pair `[first, second]` (names, `*` `?` wildcards), every case of
/// `cases` (name, its wires) matching the first against every one
/// matching the second — the first wire in the first case, the second in
/// the second, every ordered pair but a wire with itself, connected wires
/// and (unless the rule asks) the same circuit and phase
/// ([`PairMode::SameLineOtherCase`]), as `ohl line --weather` does. Rules
/// to other lines are not checked here. A pair naming no case of `cases`
/// gives nothing (the caller says so).
pub fn check_case_pairs(
    rules: &[Rule],
    cases: &[(&str, &[Wire])],
    voltages: &[Option<Potential>],
) -> Vec<CasePairCheck> {
    let paired: Vec<usize> = (0..rules.len())
        .filter(|&k| {
            !rules[k].case_pairs.is_empty() && matches!(rules[k].target, Target::SameLine { .. })
        })
        .collect();
    // How far the wire-to-wire checks look, as in a case.
    let reach = paired
        .iter()
        .map(|&k| rules[k].reach_in_plan(voltages, &[]))
        .fold(0.0, f64::max);
    let matching = |pattern: &str| -> Vec<usize> {
        (0..cases.len())
            .filter(|&c| ohl_core::pattern::matches(pattern, cases[c].0))
            .collect()
    };
    let mut out = Vec::new();
    for k in paired {
        let rule = &rules[k];
        let Target::SameLine { include_same_phase } = rule.target else {
            continue;
        };
        for [first, second] in &rule.case_pairs {
            for a in matching(first) {
                for b in matching(second) {
                    let wires = checked_parts(rule, cases[a].1);
                    let set = WireSet::new(checked_parts(rule, cases[b].1), reach);
                    let pairs = wire_pairs(
                        rule,
                        &wires,
                        &set,
                        PairMode::SameLineOtherCase { include_same_phase },
                    );
                    out.push(CasePairCheck {
                        rule: k,
                        first: a,
                        second: b,
                        wires,
                        others: set.wires,
                        pairs,
                    });
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::Envelope;
    use crate::rules::RuleSet;

    /// A straight wire of section `section`, phase `phase`, 100 m along x
    /// at `y`, height `z`.
    fn wire(section: &str, phase: &str, y: f64, z: f64) -> Wire {
        Wire {
            section: section.into(),
            span: 0,
            circuit: "c".into(),
            phase: phase.into(),
            points: (0..=50).map(|k| [2.0 * k as f64, y, z]).collect(),
            ends: vec![],
            potential: None,
            bundle: vec![],
            surface_radius: 0.0,
            polyline_error: 0.0,
        }
    }

    /// Phase A over phase B, 2 m across and 6 m apart in height; in "ice"
    /// every wire 3 m lower (the drop of an iced wire, invented).
    fn wires(case: &str) -> Vec<Wire> {
        let drop = if case == "ice" { 3.0 } else { 0.0 };
        vec![
            wire("A", "L1", 0.0, 20.0 - drop),
            wire("B", "L2", 2.0, 14.0 - drop),
        ]
    }

    fn targets<'a>() -> CaseTargets<'a> {
        CaseTargets {
            voltages: &[],
            other_voltages: &[],
            ground: None,
            surfaces: &[],
            point_classes: &[],
            points: None,
            other_wires: &[],
        }
    }

    /// Several weather cases: each checked as one case (the same results
    /// as checking it alone), the envelope keeping per wire pair the
    /// lowest clearance with the case that governs it.
    #[test]
    fn every_case_of_the_rules_and_their_envelope() {
        let set = RuleSet::parse(
            "[[rule]]\nname = \"pp\"\ntarget = \"same_line\"\nradial = 7.0\n\
             [[rule]]\nname = \"hot only\"\ntarget = \"same_line\"\nradial = 9.0\ncases = [\"hot\"]",
        )
        .unwrap();
        let mut env = Envelope::default();
        let mut alone = Vec::new();
        for case in ["cold", "hot"] {
            let w = wires(case);
            let checks = check_case(&set.rules, case, &w, targets()).unwrap();
            // A rule applies only in its cases.
            assert_eq!(checks.len(), if case == "hot" { 2 } else { 1 });
            for c in &checks {
                let Outcome::Pairs(pairs) = &c.outcome else {
                    panic!("{c:?}")
                };
                for p in pairs {
                    env.add(
                        c.rule,
                        case,
                        &c.wires[p.a].section,
                        Some(&c.wires[p.b].section),
                        &p.result,
                    );
                }
            }
            alone.push(checks);
        }
        // The same as one case at a time.
        assert_eq!(
            check_case(&set.rules, "cold", &wires("cold"), targets()).unwrap(),
            alone[0]
        );
        let d = 2.0f64.hypot(6.0);
        let e = env.violations(0);
        assert_eq!(e.len(), 1);
        let (hit, case) = e[0].radial.as_ref().unwrap();
        assert!((hit.distance - d).abs() < 1e-9, "{}", hit.distance);
        // Both cases alike: the first one keeps it.
        assert_eq!(case, "cold");
        assert_eq!(e[0].violated_in.iter().collect::<Vec<_>>(), ["cold", "hot"]);
        assert_eq!(
            env.violations(1)[0].violated_in.iter().collect::<Vec<_>>(),
            ["hot"]
        );
    }

    /// A rule with a case pair: skipped by each case, checked across
    /// them — the iced upper wire over the cold lower one is 3 m nearer
    /// than in either case alone; a pair naming no case run gives nothing.
    #[test]
    fn case_pairs_check_one_case_against_another() {
        let set = RuleSet::parse(
            "[[rule]]\nname = \"ice over cold\"\ntarget = \"same_line\"\nradial = 5.0\n\
             case_pairs = [[\"ice\", \"cold\"], [\"wind\", \"cold\"]]",
        )
        .unwrap();
        let (ice, cold) = (wires("ice"), wires("cold"));
        let one = check_case(&set.rules, "ice", &ice, targets()).unwrap();
        assert_eq!(one[0].outcome, Outcome::Skipped(Skipped::AcrossCases));
        let cases: Vec<(&str, &[Wire])> = vec![("cold", &cold), ("ice", &ice)];
        let found = check_case_pairs(&set.rules, &cases, &[]);
        // Only [ice, cold]: no case "wind".
        assert_eq!(found.len(), 1);
        let c = &found[0];
        assert_eq!((c.rule, c.first, c.second), (0, 1, 0));
        // A (iced, z 17) to B (cold, z 14): 2 m across, 3 m down; B (iced,
        // z 11) to A (cold, z 20): 9 m down.
        let mut d: Vec<f64> = c
            .pairs
            .iter()
            .map(|p| p.result.radial.unwrap().distance)
            .collect();
        d.sort_by(f64::total_cmp);
        assert!((d[0] - 2.0f64.hypot(3.0)).abs() < 1e-9, "{d:?}");
        assert!(c.pairs.iter().any(|p| p.result.violates()));
        // Each case alone keeps 6.3 m: no violation without the pair.
        assert!(2.0f64.hypot(6.0) > 5.0);
    }
}
