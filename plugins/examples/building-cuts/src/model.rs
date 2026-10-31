// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! What the plugin takes from the export model (`ohl-export-model`): the
//! structures, the spans, the alignments (where cuts are placed: at a
//! station, perpendicular to the alignment, at an angle point along the
//! bisector — the core's rule, [`Alignment::plane_at`]) and the solved
//! wires of the case run.

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

/// A station this close to an alignment point (m) is cut along the
/// bisector of the angle there (the core's `ohl_line::route::AT_VERTEX`).
pub const AT_VERTEX: f64 = 0.01;

/// A vertical plane across an alignment: its point on the axis and the
/// direction looked in (ahead, unit, plan).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    /// On the axis, plan.
    pub origin: P2,
    /// Ahead along the alignment (unit).
    pub ahead: P2,
    /// At an angle point (along the bisector).
    pub angle_point: bool,
}

impl Plane {
    /// To the right looking ahead (the engine's offsets).
    pub fn right(&self) -> P2 {
        [self.ahead[1], -self.ahead[0]]
    }

    /// How far `p` lies right of the axis in the plane's direction, m.
    pub fn offset(&self, p: P2) -> f64 {
        let r = self.right();
        (p[0] - self.origin[0]) * r[0] + (p[1] - self.origin[1]) * r[1]
    }

    /// How far `p` lies ahead of the plane, m.
    pub fn along(&self, p: P2) -> f64 {
        (p[0] - self.origin[0]) * self.ahead[0] + (p[1] - self.origin[1]) * self.ahead[1]
    }
}

/// An alignment of the line: its centreline through its points.
#[derive(Debug, Clone, PartialEq)]
pub struct Alignment {
    /// Its id (what a cut names).
    pub id: String,
    /// Its centreline in plan.
    pub points: Vec<P2>,
    /// Its plan length, m.
    pub length: f64,
}

impl Alignment {
    /// The segments: start station, end station, unit direction, start.
    fn segments(&self) -> Vec<(f64, f64, P2, P2)> {
        let mut out = Vec::new();
        let mut start = 0.0;
        for w in self.points.windows(2) {
            let (dx, dy) = (w[1][0] - w[0][0], w[1][1] - w[0][1]);
            let len = dx.hypot(dy);
            if len <= 0.0 {
                continue;
            }
            out.push((start, start + len, [dx / len, dy / len], w[0]));
            start += len;
        }
        out
    }

    /// `p` projected onto the centreline (its closest segment): station
    /// and offset (right positive), m.
    pub fn project(&self, p: P2) -> (f64, f64) {
        let mut best = (f64::INFINITY, 0.0, 0.0);
        for (s0, s1, u, a) in self.segments() {
            let (px, py) = (p[0] - a[0], p[1] - a[1]);
            let along = (px * u[0] + py * u[1]).clamp(0.0, s1 - s0);
            let (cx, cy) = (px - along * u[0], py - along * u[1]);
            let d2 = cx * cx + cy * cy;
            if d2 < best.0 {
                best = (d2, s0 + along, px * u[1] - py * u[0]);
            }
        }
        (best.1, best.2)
    }

    /// The plan distance from `p` to the centreline, m.
    pub fn distance(&self, p: P2) -> f64 {
        self.points
            .windows(2)
            .map(|w| nearest_on_segment(p, w[0], w[1]).1)
            .fold(f64::INFINITY, f64::min)
    }

    /// The plan point at `station`.
    pub fn point_at(&self, station: f64) -> P2 {
        let segs = self.segments();
        let n = segs.len();
        for (k, (s0, s1, u, a)) in segs.iter().enumerate() {
            if station <= *s1 || k + 1 == n {
                let t = station - s0;
                return [a[0] + u[0] * t, a[1] + u[1] * t];
            }
        }
        self.points.first().copied().unwrap_or([0.0, 0.0])
    }

    /// The plane across the alignment at `station` (kept within it):
    /// perpendicular to it, at an angle point (within [`AT_VERTEX`]) along
    /// the bisector — as the core places a cut and a cross-section panel.
    pub fn plane_at(&self, station: f64) -> Plane {
        let station = station.clamp(0.0, self.length);
        let segs = self.segments();
        let origin = self.point_at(station);
        let k = segs
            .iter()
            .position(|s| station <= s.1)
            .unwrap_or(segs.len().saturating_sub(1));
        let s = segs[k];
        let bisector = |a: P2, b: P2| {
            let v = [a[0] + b[0], a[1] + b[1]];
            let n = v[0].hypot(v[1]);
            if n < 1e-9 { b } else { [v[0] / n, v[1] / n] }
        };
        let turns = |a: P2, b: P2| (a[0] * b[1] - a[1] * b[0]).abs() > 1e-9;
        let (ahead, angle_point) = if station - s.0 < AT_VERTEX && k > 0 {
            let a = segs[k - 1].2;
            (bisector(a, s.2), turns(a, s.2))
        } else if s.1 - station < AT_VERTEX && k + 1 < segs.len() {
            let b = segs[k + 1].2;
            (bisector(s.2, b), turns(s.2, b))
        } else {
            (s.2, false)
        };
        Plane {
            origin,
            ahead,
            angle_point,
        }
    }
}

/// A solved wire (one span of a section, or a jumper).
#[derive(Debug, Clone, PartialEq)]
pub struct Wire {
    /// Its section (`jumper:…` for a jumper).
    pub section: String,
    /// Its label in a drawing: circuit and phase (`North L1`), else its
    /// section.
    pub label: String,
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
    /// The line's alignments (a cut is placed on one; none: no cut).
    pub alignments: Vec<Alignment>,
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
        // The alignments: their points' positions in order.
        let mut points: BTreeMap<String, P2> = BTreeMap::new();
        for p in line["alignment_points"].as_array().into_iter().flatten() {
            let (Some(id), Some(q)) = (p["id"].as_str(), p["position"].as_array()) else {
                continue;
            };
            if let (Some(x), Some(y)) = (q.first().and_then(num), q.get(1).and_then(num)) {
                points.insert(id.to_owned(), [x, y]);
            }
        }
        let mut alignments = Vec::new();
        for a in line["alignments"].as_array().into_iter().flatten() {
            let Some(id) = a["id"].as_str() else {
                continue;
            };
            let pts: Vec<P2> = a["points"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| p.as_str().and_then(|k| points.get(k)).copied())
                .collect();
            let length: f64 = pts
                .windows(2)
                .map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]))
                .sum();
            if pts.len() >= 2 && length > 0.0 {
                alignments.push(Alignment {
                    id: id.to_owned(),
                    points: pts,
                    length,
                });
            }
        }
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
            let section = w["section"].as_str().unwrap_or("").to_owned();
            let (circuit, phase) = (
                w["circuit"].as_str().unwrap_or("").trim(),
                w["phase"].as_str().unwrap_or("").trim(),
            );
            let label = match (circuit.is_empty(), phase.is_empty()) {
                (false, false) => format!("{circuit} {phase}"),
                (false, true) => format!("{circuit} {section}"),
                _ => section.clone(),
            };
            wires.push(Wire {
                label,
                section,
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
            alignments,
            no_systems,
        })
    }
}
