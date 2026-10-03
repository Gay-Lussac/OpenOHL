// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Clearance envelopes over several weather cases.
//!
//! Every case's results are added ([`Envelope::add`]); for each rule and
//! each wire (or pair of wires) the envelope keeps the lowest vertical and
//! the lowest radial clearance with the **case that governs** it, and the
//! cases in which a requirement is not met.

use std::collections::{BTreeMap, BTreeSet};

use crate::checks::{Hit, WireResult};
use crate::rules::Rule;

/// The envelope of one wire (or wire pair) under one rule.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EnvelopeEntry {
    /// The wire.
    pub a: String,
    /// The other wire, for wire rules.
    pub b: Option<String>,
    /// Lowest vertical clearance over the cases, with its case.
    pub vertical: Option<(Hit, String)>,
    /// Lowest radial clearance over the cases, with its case.
    pub radial: Option<(Hit, String)>,
    /// Cases in which a requirement of the rule is not met.
    pub violated_in: BTreeSet<String>,
}

impl EnvelopeEntry {
    /// The smallest of the vertical and radial clearances.
    pub fn lowest(&self) -> f64 {
        [self.vertical.as_ref(), self.radial.as_ref()]
            .into_iter()
            .flatten()
            .map(|(h, _)| h.distance)
            .fold(f64::INFINITY, f64::min)
    }
}

/// Envelopes of every rule.
#[derive(Debug, Clone, Default)]
pub struct Envelope {
    rules: BTreeMap<usize, BTreeMap<(String, Option<String>), EnvelopeEntry>>,
    cases: BTreeSet<String>,
}

impl Envelope {
    /// Adds the result of rule `rule_index` for wire `a` (and `b`) in
    /// `case`.
    pub fn add(
        &mut self,
        rule_index: usize,
        rule: &Rule,
        case: &str,
        a: &str,
        b: Option<&str>,
        r: &WireResult,
    ) {
        self.cases.insert(case.to_owned());
        let entry = self
            .rules
            .entry(rule_index)
            .or_default()
            .entry((a.to_owned(), b.map(str::to_owned)))
            .or_insert_with(|| EnvelopeEntry {
                a: a.to_owned(),
                b: b.map(str::to_owned),
                ..EnvelopeEntry::default()
            });
        let lower = |slot: &mut Option<(Hit, String)>, hit: Option<Hit>| {
            if let Some(h) = hit
                && slot.as_ref().is_none_or(|(s, _)| h.distance < s.distance)
            {
                *slot = Some((h, case.to_owned()));
            }
        };
        lower(&mut entry.vertical, r.vertical);
        lower(&mut entry.radial, r.radial);
        if r.violates(rule) {
            entry.violated_in.insert(case.to_owned());
        }
    }

    /// The cases added.
    pub fn cases(&self) -> &BTreeSet<String> {
        &self.cases
    }

    /// The entries of a rule (in no particular order).
    pub fn entries(&self, rule_index: usize) -> Vec<&EnvelopeEntry> {
        self.rules
            .get(&rule_index)
            .map_or(Vec::new(), |m| m.values().collect())
    }

    /// The entries of a rule violated in some case, lowest first.
    pub fn violations(&self, rule_index: usize) -> Vec<&EnvelopeEntry> {
        let mut v: Vec<&EnvelopeEntry> = self
            .entries(rule_index)
            .into_iter()
            .filter(|e| !e.violated_in.is_empty())
            .collect();
        v.sort_by(|x, y| x.lowest().total_cmp(&y.lowest()));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::RuleSet;

    fn hit(d: f64) -> Hit {
        Hit {
            distance: d,
            wire: [0.0; 3],
            target: [0.0; 3],
            class: None,
        }
    }

    #[test]
    fn keeps_the_lowest_with_its_case() {
        let rule = RuleSet::parse(
            "[[rule]]\nname = \"g\"\ntarget = \"ground\"\nvertical = 7.0\nradial = 6.0",
        )
        .unwrap()
        .rules
        .remove(0);
        let mut env = Envelope::default();
        let result = |v: f64, r: f64| WireResult {
            vertical: Some(hit(v)),
            radial: Some(hit(r)),
            ..WireResult::default()
        };
        env.add(0, &rule, "cold", "S1 span 0", None, &result(12.0, 11.0));
        env.add(0, &rule, "hot", "S1 span 0", None, &result(6.5, 6.2));
        env.add(0, &rule, "ice", "S1 span 0", None, &result(8.0, 5.5));
        env.add(0, &rule, "hot", "S1 span 1", None, &result(20.0, 19.0));
        let e = &env.violations(0)[0];
        assert_eq!(e.a, "S1 span 0");
        assert_eq!(
            (
                e.vertical.as_ref().unwrap().0.distance,
                e.vertical.as_ref().unwrap().1.as_str()
            ),
            (6.5, "hot")
        );
        assert_eq!(
            (
                e.radial.as_ref().unwrap().0.distance,
                e.radial.as_ref().unwrap().1.as_str()
            ),
            (5.5, "ice")
        );
        assert_eq!(e.violated_in.iter().collect::<Vec<_>>(), ["hot", "ice"]);
        assert_eq!(env.violations(0).len(), 1);
        assert_eq!(env.entries(0).len(), 2);
        assert_eq!(env.cases().len(), 3);
    }
}
