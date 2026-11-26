// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **The limits a design code puts on its members' sizes** (2026-11-26,
//! roadmap O49; built, in active development, not approved yet): a
//! compression member's slenderness, a leg's flat width to thickness, a
//! member's least thickness — what a member's usage does not hold. The
//! plugin of the model's code works each out by its own rules and gives
//! the bound with its clause (`member-limits`, an optional export of
//! interface 0.3). The member sizer of OHL-Structure takes a section only
//! where every limit holds; the member check itself does not change.
//!
//! A plugin without the export, and the core's `simple` check, give no
//! limits ([`Limits::Unknown`]): nothing is checked, and the sizer says
//! so. An empty list from the plugin: its code bounds none of these
//! members.

use ohl_plugins::member_limits as wit;
use ohl_structures_fe::Model;

/// One limit on one member.
#[derive(Debug, Clone, PartialEq)]
pub struct Limit {
    /// The member (its index in the model).
    pub member: usize,
    /// What is bounded, in words ("KL/r, the effective slenderness").
    pub name: String,
    /// The member's value as the code works it out.
    pub value: f64,
    /// The largest value allowed.
    pub maximum: Option<f64>,
    /// The smallest value allowed.
    pub minimum: Option<f64>,
    /// It holds only for a member compressed in a load case.
    pub compression: bool,
    /// The clause.
    pub clause: String,
}

impl Limit {
    /// The share of the limit used (1: at the bound; above 1: broken) —
    /// value / maximum, minimum / value (the larger of the two when both
    /// are given; a value of 0 below a minimum: infinite).
    pub fn usage(&self) -> f64 {
        let over = self.maximum.map_or(0.0, |m| {
            if m > 0.0 {
                self.value / m
            } else if self.value > m {
                f64::INFINITY
            } else {
                0.0
            }
        });
        let under = self.minimum.map_or(0.0, |m| {
            if self.value > 0.0 {
                m / self.value
            } else if m > 0.0 {
                f64::INFINITY
            } else {
                0.0
            }
        });
        over.max(under)
    }

    /// Broken: the value beyond a bound.
    pub fn broken(&self) -> bool {
        self.maximum.is_some_and(|m| self.value > m * (1.0 + 1e-12))
            || self.minimum.is_some_and(|m| self.value < m * (1.0 - 1e-12))
    }

    /// In words: "KL/r, the effective slenderness 162 above 150 (ASCE
    /// 10-97 3.4)", "… 120 (at most 150; ASCE 10-97 3.4)".
    pub fn words(&self) -> String {
        let v = number(self.value);
        let bound = match (self.maximum, self.minimum) {
            (Some(m), _) if self.value > m => format!("{v} above {}", number(m)),
            (_, Some(m)) if self.value < m => format!("{v} below {}", number(m)),
            (Some(m), Some(n)) => format!("{v} (between {} and {})", number(n), number(m)),
            (Some(m), None) => format!("{v} (at most {})", number(m)),
            (None, Some(n)) => format!("{v} (at least {})", number(n)),
            (None, None) => v,
        };
        if self.clause.is_empty() {
            format!("{} {bound}", self.name)
        } else {
            format!("{} {bound} ({})", self.name, self.clause)
        }
    }
}

/// A number for the words: three significant figures, no trailing zeros.
fn number(x: f64) -> String {
    if x.abs() >= 100.0 {
        format!("{x:.0}")
    } else if x.abs() >= 10.0 {
        format!("{x:.1}")
    } else {
        format!("{x:.4}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    }
}

/// What the model's code gives as limits.
#[derive(Debug, Clone, PartialEq)]
pub enum Limits {
    /// The plugin's limits (empty: none on these members).
    Given {
        /// The limits.
        limits: Vec<Limit>,
        /// The code, in words (the plugin and its method).
        code: String,
    },
    /// No limits known, and why ("the simple check gives no limits", "the
    /// plugin sia263 has no member-limits export").
    Unknown(String),
}

/// The limits the code of `model`'s capacity method puts on its checked
/// members (its plugin found, [`crate::find`]). Err: the plugin failed —
/// no limits, never another code's in their place.
///
/// Expected cost: one plugin call with the members' records and their
/// bracing (as a member check without load cases: tens of milliseconds
/// for a tower of a few thousand members).
pub fn limits(model: &Model) -> Result<Limits, String> {
    let Some(p) = model.capacity_plugin.as_ref() else {
        return Ok(Limits::Unknown(
            "the simple check gives no limits on the members' sizes".into(),
        ));
    };
    let Some(dir) = p.dir.as_deref() else {
        return Ok(Limits::Unknown(format!(
            "the plugin {} was not looked for",
            p.plugin
        )));
    };
    let host = crate::host().map_err(|e| format!("member limits: {e}"))?;
    let params = p.parameters.as_ref().map_or("", |x| x.1.as_str());
    let input = crate::full::input(model, &[], params);
    let braces = crate::full::bracing(model);
    let given = host
        .member_limits(dir, &p.method, &input, &braces)
        .map_err(|e| {
            format!(
                "member limits: plugin {} (method {}): {e}",
                p.plugin, p.method
            )
        })?;
    let code = p.alias().map_or_else(
        || format!("{} of the plugin {}", p.method, p.plugin),
        str::to_owned,
    );
    let Some(given) = given else {
        return Ok(Limits::Unknown(format!(
            "{code}: the plugin {} gives no limits on the members' sizes (no member-limits \
             export)",
            p.plugin
        )));
    };
    let index: std::collections::HashMap<&str, usize> = model
        .members
        .iter()
        .enumerate()
        .map(|(k, m)| (m.id.as_str(), k))
        .collect();
    let limits = given
        .into_iter()
        .filter_map(|l: wit::MemberLimit| {
            Some(Limit {
                member: *index.get(l.member.as_str())?,
                name: l.name,
                value: l.value,
                maximum: l.maximum,
                minimum: l.minimum,
                compression: l.compression,
                clause: l.clause,
            })
        })
        .collect();
    Ok(Limits::Given { limits, code })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limit(value: f64, maximum: Option<f64>, minimum: Option<f64>) -> Limit {
        Limit {
            member: 0,
            name: "KL/r".into(),
            value,
            maximum,
            minimum,
            compression: true,
            clause: "3.4".into(),
        }
    }

    #[test]
    fn usage_and_words() {
        let l = limit(162.0, Some(150.0), None);
        assert!((l.usage() - 1.08).abs() < 1e-12);
        assert!(l.broken());
        assert_eq!(l.words(), "KL/r 162 above 150 (3.4)");
        let l = limit(120.0, Some(150.0), None);
        assert!(!l.broken());
        assert_eq!(l.words(), "KL/r 120 (at most 150) (3.4)");
        let t = limit(0.002, None, Some(0.003));
        assert!(t.broken());
        assert!((t.usage() - 1.5).abs() < 1e-12);
        assert_eq!(t.words(), "KL/r 0.002 below 0.003 (3.4)");
        // At the bound: not broken.
        assert!(!limit(150.0, Some(150.0), None).broken());
        assert_eq!(limit(0.0, None, Some(0.003)).usage(), f64::INFINITY);
    }
}
