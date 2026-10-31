// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! What the plugin takes from the export model (`ohl-export-model`): the
//! structures, the spans (in the direction the engine measures a cut's
//! fraction from) and the solved wires of the case run.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::geom::{P2, nearest_on_segment};

/// A span: its structures and its chord in plan.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    /// The structure a cut's fraction is measured from.
    pub from: String,
    /// The other.
    pub to: String,
    /// `from` in plan.
    pub a: P2,
    /// `to` in plan.
    pub b: P2,
    /// Unit direction from `a` to `b`.
    pub u: P2,
    /// Unit right, looking from `a` to `b` (the engine's offsets).
    pub right: P2,
    /// Plan length, m.
    pub length: f64,
    /// The station of `a` along the line (the spans' lengths before it).
    pub start: f64,
}

impl Span {
    /// Along the chord from `a`, and to its right, m.
    pub fn local(&self, p: P2) -> (f64, f64) {
        let d = [p[0] - self.a[0], p[1] - self.a[1]];
        (
            d[0] * self.u[0] + d[1] * self.u[1],
            d[0] * self.right[0] + d[1] * self.right[1],
        )
    }

    /// The plan point `t` along the chord.
    pub fn at(&self, t: f64) -> P2 {
        [self.a[0] + t * self.u[0], self.a[1] + t * self.u[1]]
    }

    /// The plan distance from `p` to the chord (segment).
    pub fn distance(&self, p: P2) -> f64 {
        nearest_on_segment(p, self.a, self.b).1
    }
}

/// A solved wire (one span of a section, or a jumper).
#[derive(Debug, Clone, PartialEq)]
pub struct Wire {
    /// Its section (`jumper:…` for a jumper).
    pub section: String,
    /// A phase conductor (it has a system); else an earth wire.
    pub phase: bool,
    /// The span it hangs in (index into the spans), if any.
    pub span: Option<usize>,
    /// Its points, m.
    pub points: Vec<[f64; 3]>,
}

/// The model as the plugin needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    /// The spans, in the line's order.
    pub spans: Vec<Span>,
    /// The wires of the case run.
    pub wires: Vec<Wire>,
    /// The line has routes or alignments: the engine orders its spans by
    /// them, which this plugin does not follow.
    pub routed: bool,
    /// No wire has a system: every wire taken as a phase conductor.
    pub no_systems: bool,
}

fn num(v: &Value) -> Option<f64> {
    v.as_f64()
}

impl Model {
    /// The model of `json` (`ohl-export-model`).
    pub fn parse(json: &str) -> Result<Self, String> {
        let v: Value = serde_json::from_str(json).map_err(|e| format!("model: {e}"))?;
        if v["format"] != "ohl-export-model" {
            return Err("model: not an ohl-export-model".into());
        }
        let line = &v["line"];
        let mut at: BTreeMap<String, P2> = BTreeMap::new();
        for s in line["structures"].as_array().into_iter().flatten() {
            let (Some(id), Some(p)) = (s["id"].as_str(), s["position"].as_array()) else {
                continue;
            };
            if let (Some(x), Some(y)) = (p.first().and_then(num), p.get(1).and_then(num)) {
                at.insert(id.to_owned(), [x, y]);
            }
        }
        // The spans as the engine finds them off routes: each pair of
        // consecutive structures of a section once, in the direction most
        // sections are drawn (ties: the first drawn), in the order first
        // drawn.
        let mut pairs: Vec<(String, String)> = Vec::new();
        let mut drawn: BTreeMap<(String, String), i64> = BTreeMap::new();
        let mut first: BTreeMap<(String, String), bool> = BTreeMap::new();
        for s in line["sections"].as_array().into_iter().flatten() {
            let ids: Vec<&str> = s["points"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| p["structure"].as_str())
                .collect();
            for w in ids.windows(2) {
                let (a, b) = (w[0].to_owned(), w[1].to_owned());
                if a == b {
                    continue;
                }
                let (key, d) = if a < b {
                    ((a.clone(), b.clone()), 1)
                } else {
                    ((b.clone(), a.clone()), -1)
                };
                *drawn.entry(key.clone()).or_insert(0) += d;
                if !first.contains_key(&key) {
                    first.insert(key.clone(), a < b);
                    pairs.push(key);
                }
            }
        }
        let mut spans: Vec<Span> = Vec::new();
        let mut start = 0.0;
        for (lo, hi) in pairs {
            let key = (lo.clone(), hi.clone());
            let n = drawn[&key];
            let low_first = n > 0 || (n == 0 && first[&key]);
            let (from, to) = if low_first { (lo, hi) } else { (hi, lo) };
            let (Some(&a), Some(&b)) = (at.get(&from), at.get(&to)) else {
                continue;
            };
            let length = (b[0] - a[0]).hypot(b[1] - a[1]);
            if length < 1e-6 {
                continue;
            }
            let u = [(b[0] - a[0]) / length, (b[1] - a[1]) / length];
            spans.push(Span {
                from,
                to,
                a,
                b,
                u,
                right: [u[1], -u[0]],
                length,
                start,
            });
            start += length;
        }
        if spans.is_empty() {
            return Err("model: no span".into());
        }
        let routed = line["routes"].as_array().is_some_and(|r| !r.is_empty())
            || line["alignments"].as_array().is_some_and(|r| !r.is_empty());
        let case = &v["cases"][0];
        let mut wires = Vec::new();
        for w in case["wires"].as_array().into_iter().flatten() {
            let points: Vec<[f64; 3]> = w["points"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| {
                    let p = p.as_array()?;
                    Some([num(p.first()?)?, num(p.get(1)?)?, num(p.get(2)?)?])
                })
                .collect();
            if points.len() < 2 {
                continue;
            }
            let mid = points[points.len() / 2];
            let span = spans
                .iter()
                .enumerate()
                .map(|(k, s)| (k, s.distance([mid[0], mid[1]])))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(k, _)| k);
            wires.push(Wire {
                section: w["section"].as_str().unwrap_or("").to_owned(),
                phase: !w["system"].is_null(),
                span,
                points,
            });
        }
        if wires.is_empty() {
            return Err("model: no solved wire in the case".into());
        }
        let no_systems = !wires.iter().any(|w| w.phase);
        if no_systems {
            for w in &mut wires {
                w.phase = true;
            }
        }
        Ok(Self {
            spans,
            wires,
            routed,
            no_systems,
        })
    }
}
