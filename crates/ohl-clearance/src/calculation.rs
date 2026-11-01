// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A rule's wire calculation (owner, 2026-11-01: "on which test the
//! condition of the cable could also be defined in the rules (e.g. after
//! creep, after load, initial FE or max sag)"): a rule may solve every
//! wire it looks at by one calculation — a condition and a method,
//! [`Calculation`] — instead of each section's own. The line is solved
//! once per weather case and calculation asked ([`solves`]), on a copy
//! whose sections all take it ([`line_for`]); a finding names the
//! calculation beside its case ([`label`]).

use std::borrow::Cow;

use ohl_line::Line;
use ohl_line::condition::Calculation;

/// A rule's calculation: one for every wire, or (`None`) each section's
/// own.
pub type Calc = Option<Calculation>;

/// `line` with every section solved by `calc` (none: as it is).
pub fn line_for(line: &Line, calc: Calc) -> Cow<'_, Line> {
    match calc {
        None => Cow::Borrowed(line),
        Some(c) => {
            let mut l = line.clone();
            for s in &mut l.sections {
                s.calculation = c;
            }
            Cow::Owned(l)
        }
    }
}

/// What to solve for `rules` over the weather cases `names`: each case a
/// rule applies to (`applies`), with each calculation (`calc`) the rules
/// applying to it ask, in the cases' order, each pair once.
pub fn solves<R>(
    names: &[&str],
    rules: &[R],
    applies: impl Fn(&R, &str) -> bool,
    calc: impl Fn(&R) -> Calc,
) -> Vec<(usize, Calc)> {
    let mut out: Vec<(usize, Calc)> = Vec::new();
    for (k, name) in names.iter().enumerate() {
        for r in rules {
            let c = calc(r);
            if applies(r, name) && !out.contains(&(k, c)) {
                out.push((k, c));
            }
        }
    }
    out
}

/// The calculations the clearance rules `rules` ask, each once, in their
/// order (none first when a rule takes the sections' own; no rule: none).
pub fn of_rules(rules: &[crate::Rule]) -> Vec<Calc> {
    let mut out: Vec<Calc> = Vec::new();
    for r in rules {
        if !out.contains(&r.calculation) {
            out.push(r.calculation);
        }
    }
    out.sort_by_key(|c| c.is_some());
    if out.is_empty() {
        out.push(None);
    }
    out
}

/// Rules `rules` checked by calculation `calc` only: the others kept in
/// their places (their indices stay) but applying to no case and no case
/// pair.
pub fn only(rules: &[crate::Rule], calc: Calc) -> Vec<crate::Rule> {
    rules
        .iter()
        .map(|r| {
            if r.calculation == calc {
                r.clone()
            } else {
                crate::Rule {
                    cases: vec![NO_CASE.into()],
                    case_pairs: Vec::new(),
                    ..r.clone()
                }
            }
        })
        .collect()
}

/// A weather case name no case has (a rule set aside, [`only`]).
pub const NO_CASE: &str = "\u{0}";

/// The calculations the zone rules `rules` ask, each once (none first;
/// no rule: none).
pub fn of_zone_rules(rules: &[crate::ZoneRule]) -> Vec<Calc> {
    let mut out: Vec<Calc> = Vec::new();
    for r in rules {
        if !out.contains(&r.calculation) {
            out.push(r.calculation);
        }
    }
    out.sort_by_key(|c| c.is_some());
    if out.is_empty() {
        out.push(None);
    }
    out
}

/// Zone rules `rules` checked by calculation `calc` only, the others in
/// their places applying to no case ([`only`]).
pub fn only_zone(rules: &[crate::ZoneRule], calc: Calc) -> Vec<crate::ZoneRule> {
    rules
        .iter()
        .map(|r| {
            if r.calculation == calc {
                r.clone()
            } else {
                crate::ZoneRule {
                    cases: vec![NO_CASE.into()],
                    ..r.clone()
                }
            }
        })
        .collect()
}

/// A weather case named with the calculation it was solved by (none: the
/// sections' own, the case's name alone).
pub fn label(case: &str, calc: Calc) -> String {
    match calc {
        None => case.to_owned(),
        Some(c) => format!("{case}, {}", c.label()),
    }
}

/// `calculation = "…"` in a rule's TOML table (none: nothing).
pub fn toml_line(calc: Calc) -> String {
    calc.map_or(String::new(), |c| {
        format!("calculation = \"{}\"\n", c.name())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_case_is_solved_once_per_calculation_asked() {
        let rules = [
            ("hot", Some(Calculation::MAX_SAG_FE)),
            ("", None),
            ("hot", Some(Calculation::MAX_SAG_FE)),
        ];
        let s = solves(
            &["cold", "hot 80"],
            &rules,
            |r, c| c.starts_with(r.0),
            |r| r.1,
        );
        assert_eq!(
            s,
            [(0, None), (1, Some(Calculation::MAX_SAG_FE)), (1, None)]
        );
        assert_eq!(
            label("hot 80", Some(Calculation::MAX_SAG_FE)),
            "hot 80, Max Sag FE"
        );
        assert_eq!(
            toml_line(Some(Calculation::INITIAL_RS)),
            "calculation = \"initial-rs\"\n"
        );
    }
}
