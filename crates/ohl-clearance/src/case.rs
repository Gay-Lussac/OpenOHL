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
    /// The ground surface.
    pub ground: Option<&'a Terrain>,
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
    let reach = active
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
                Target::Ground if targets.ground.is_none() => Some(Skipped::NoGround),
                Target::Points { .. } if classes(k).is_empty() => Some(Skipped::NoClasses),
                Target::Points { .. } if targets.points.is_none() => Some(Skipped::NoPoints),
                Target::OtherLines if targets.other_wires.is_empty() => Some(Skipped::NoOtherLines),
                _ => None,
            };
            (parts, skipped.map(Outcome::Skipped))
        };
        sets.push(
            (outcome.is_none()
                && matches!(rule.target, Target::Points { .. } | Target::SameLine { .. }))
            .then(|| WireSet::new(parts.clone(), reach)),
        );
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
                targets.ground.expect("checked above"),
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
