// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Distance between conductors in the span, still air, as a code gives it
//! by a formula of the local sag and string length:
//! \\( z = k \sqrt{f + l_k} + C \\) (Swiss LeV, Annex 6, 2.2.1 — see
//! `docs/development/lev-phase-spacing.md`).
//!
//! The code's values are data (`.olspd`): C₁/C₂ by nominal voltage (a low
//! and a high row where the insulation level gives two), the k lines of
//! each diagram (k linear in the system angle β, one line per swing class
//! w = d/q), and the altitude rule. The check pairs the conductors hanging
//! between the same two structures; along each, every `spacing` of its
//! length, it takes the closest point on the other and there: the
//! distance, each conductor's local sag f (vertically below its chord)
//! and l_k (linear between its ends by the kind of string holding them),
//! β (of the line joining the points, from the vertical), k, and the
//! required z; the margin d − z.

use std::collections::BTreeSet;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use ols_core::budget::Budget;
use ols_core::units::Length;
use ols_line::solve::SectionShape;
use ols_line::{Line, Structure};
use serde::Deserialize;

use crate::checks::{Wire, wires_of};

/// File format id of a phase distance code.
pub const PHASE_DISTANCE_FILE_FORMAT: &str = "ols-phase-distance";
/// Newest version read.
pub const PHASE_DISTANCE_FILE_VERSION: u32 = 1;
/// File extension.
pub const PHASE_DISTANCE_FILE_EXTENSION: &str = "olspd";

type P3 = [f64; 3];

/// C₁ (same circuit, and towards an earth wire) and C₂ (different
/// circuits) up to a nominal voltage, m: the low and the high row (equal
/// where the code gives one).
#[derive(Debug, Clone, PartialEq)]
pub struct VoltageRow {
    /// Nominal voltage, kV: this row for systems up to it.
    pub kv: f64,
    /// C₁, low and high row, m.
    pub c1: [f64; 2],
    /// C₂, low and high row, m.
    pub c2: [f64; 2],
}

/// One line of a k diagram: for conductors of swing class w = d/q up to
/// `w_max` (d in mm, q in kg/m; none: no limit), k from `k0` at β = 0°
/// to `k90` at β = 90°, linearly.
#[derive(Debug, Clone, PartialEq)]
pub struct KLine {
    /// Upper limit of the class (none: the last).
    pub w_max: Option<f64>,
    /// k at β = 0° (conductors one above the other).
    pub k0: f64,
    /// k at β = 90° (side by side).
    pub k90: f64,
}

/// A k diagram: for a wind pressure, its lines by swing class.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    /// Its name (e.g. "2.4.2").
    pub name: String,
    /// Wind pressure it is for, Pa.
    pub wind_pressure: f64,
    /// Lines in increasing w.
    pub lines: Vec<KLine>,
}

impl Diagram {
    /// k for a conductor of swing class `w` at system angle `beta_deg`.
    pub fn k(&self, w: f64, beta_deg: f64) -> f64 {
        let line = self
            .lines
            .iter()
            .find(|l| l.w_max.is_none_or(|m| w <= m))
            .or(self.lines.last())
            .expect("a diagram has lines");
        let t = (beta_deg / 90.0).clamp(0.0, 1.0);
        line.k0 + (line.k90 - line.k0) * t
    }
}

/// A code's values for the distance between conductors in the span.
#[derive(Debug, Clone, PartialEq)]
pub struct PhaseDistanceCode {
    /// Name.
    pub name: String,
    /// C₁/C₂ rows in increasing voltage.
    pub voltages: Vec<VoltageRow>,
    /// The k diagrams.
    pub diagrams: Vec<Diagram>,
    /// Altitude up to which C holds, m.
    pub altitude_from: f64,
    /// C grows by this share per 1000 m above `altitude_from`.
    pub altitude_per_1000: f64,
}

/// The insulation level of a circuit: the low or the high row of the
/// table where a voltage has two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Level {
    /// The low row.
    Low,
    /// The high row (the stricter; the default).
    #[default]
    High,
}

impl PhaseDistanceCode {
    /// C for a pair: same circuit (or towards an earth wire) C₁, else C₂,
    /// at nominal voltage `kv` (the first row up to it; above the last:
    /// the last), at `level`, before the altitude.
    pub fn c(&self, same_circuit: bool, kv: f64, level: Level) -> f64 {
        let row = self
            .voltages
            .iter()
            .find(|r| kv <= r.kv + 1e-9)
            .or(self.voltages.last())
            .expect("a code has voltage rows");
        let i = usize::from(level == Level::High);
        if same_circuit { row.c1[i] } else { row.c2[i] }
    }

    /// The altitude factor at mean height `h` above sea level, m.
    pub fn altitude_factor(&self, h: f64) -> f64 {
        1.0 + self.altitude_per_1000 * ((h - self.altitude_from).max(0.0) / 1000.0)
    }

    /// Parses and checks a code file.
    pub fn parse(text: &str) -> Result<Self, String> {
        let raw: RawCode = toml::from_str(text).map_err(|e| e.to_string())?;
        ols_core::files::check_header(
            raw.format.as_deref(),
            raw.version,
            PHASE_DISTANCE_FILE_FORMAT,
            PHASE_DISTANCE_FILE_VERSION,
            "phase distance",
        )?;
        if raw.voltages.is_empty() {
            return Err("give at least one [[voltage]]".into());
        }
        if raw.diagrams.is_empty() {
            return Err("give at least one [[diagram]]".into());
        }
        let pair = |v: Vec<Length>, what: &str, kv: f64| -> Result<[f64; 2], String> {
            match v.as_slice() {
                [a] => Ok([a.si(), a.si()]),
                [a, b] => Ok([a.si(), b.si()]),
                _ => Err(format!(
                    "{kv} kV: {what} needs one value or two (low, high)"
                )),
            }
        };
        let mut voltages = Vec::new();
        for r in raw.voltages {
            let row = VoltageRow {
                kv: r.kv,
                c1: pair(r.c1, "c1", r.kv)?,
                c2: pair(r.c2, "c2", r.kv)?,
            };
            if row
                .c1
                .iter()
                .chain(&row.c2)
                .any(|c| !(c.is_finite() && *c >= 0.0))
            {
                return Err(format!("{} kV: C must be ≥ 0", r.kv));
            }
            voltages.push(row);
        }
        if voltages.windows(2).any(|w| w[1].kv <= w[0].kv) {
            return Err("[[voltage]] rows must be in increasing kv".into());
        }
        let mut diagrams = Vec::new();
        for d in raw.diagrams {
            if d.lines.is_empty() {
                return Err(format!("diagram {}: give its [[diagram.line]]s", d.name));
            }
            let lines: Vec<KLine> = d
                .lines
                .into_iter()
                .map(|l| KLine {
                    w_max: l.w_max,
                    k0: l.k0,
                    k90: l.k90,
                })
                .collect();
            if lines
                .iter()
                .any(|l| !(l.k0 > 0.0 && l.k90 > 0.0 && l.k0.is_finite() && l.k90.is_finite()))
            {
                return Err(format!("diagram {}: k must be > 0", d.name));
            }
            diagrams.push(Diagram {
                name: d.name,
                wind_pressure: d.wind_pressure.map_or(0.0, |p| p.si()),
                lines,
            });
        }
        let altitude = raw.altitude.unwrap_or_default();
        Ok(Self {
            name: raw.name,
            voltages,
            diagrams,
            altitude_from: altitude.from.map_or(f64::INFINITY, |l| l.si()),
            altitude_per_1000: altitude.per_1000_m,
        })
    }

    /// As a code file (read back by [`Self::parse`]); `comment` heads it
    /// (each line becomes a `#` line).
    pub fn to_text(&self, comment: &str) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        for l in comment.lines() {
            let _ = writeln!(out, "# {l}");
        }
        if !comment.is_empty() {
            out.push('\n');
        }
        let q = |s: &str| toml::Value::String(s.to_owned()).to_string();
        let _ = writeln!(out, "format = \"{PHASE_DISTANCE_FILE_FORMAT}\"");
        let _ = writeln!(out, "version = {PHASE_DISTANCE_FILE_VERSION}");
        let _ = writeln!(out, "name = {}", q(&self.name));
        if self.altitude_from.is_finite() || self.altitude_per_1000 != 0.0 {
            out.push_str("\n[altitude]\n");
            if self.altitude_from.is_finite() {
                let _ = writeln!(out, "from = \"{} m\"", self.altitude_from);
            }
            let _ = writeln!(out, "per_1000_m = {}", self.altitude_per_1000);
        }
        let pair = |c: [f64; 2]| {
            if c[0] == c[1] {
                format!("[\"{} m\"]", c[0])
            } else {
                format!("[\"{} m\", \"{} m\"]", c[0], c[1])
            }
        };
        for r in &self.voltages {
            let _ = write!(
                out,
                "\n[[voltage]]\nkv = {}\nc1 = {}\nc2 = {}\n",
                r.kv,
                pair(r.c1),
                pair(r.c2)
            );
        }
        for d in &self.diagrams {
            let _ = write!(
                out,
                "\n[[diagram]]\nname = {}\nwind_pressure = \"{} Pa\"\nline = [\n",
                q(&d.name),
                d.wind_pressure
            );
            for l in &d.lines {
                let w = l.w_max.map_or(String::new(), |w| format!("w_max = {w}, "));
                let _ = writeln!(out, "    {{ {w}k0 = {}, k90 = {} }},", l.k0, l.k90);
            }
            out.push_str("]\n");
        }
        out
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawVoltage {
    kv: f64,
    c1: Vec<Length>,
    c2: Vec<Length>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLine {
    #[serde(default)]
    w_max: Option<f64>,
    k0: f64,
    k90: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDiagram {
    name: String,
    #[serde(default)]
    wind_pressure: Option<ols_core::units::Stress>,
    #[serde(rename = "line")]
    lines: Vec<RawLine>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawAltitude {
    #[serde(default)]
    from: Option<Length>,
    #[serde(default)]
    per_1000_m: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCode {
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    version: Option<u32>,
    #[serde(default)]
    name: String,
    #[serde(rename = "voltage")]
    voltages: Vec<RawVoltage>,
    #[serde(rename = "diagram")]
    diagrams: Vec<RawDiagram>,
    #[serde(default)]
    altitude: Option<RawAltitude>,
}

/// How the check runs.
#[derive(Debug, Clone, PartialEq)]
pub struct PhaseDistanceSettings {
    /// The k diagram (index in the code's).
    pub diagram: usize,
    /// Stations every this length along a conductor, m.
    pub spacing: f64,
    /// The circuits checked (none: all); a pair when both its conductors'
    /// circuits are.
    pub circuits: Option<BTreeSet<String>>,
    /// Also pairs of conductors not between the same two structures
    /// (another line's, the next span's) where they come within
    /// `max_distance`.
    pub nearby: bool,
    /// Nearby pairs: measured where closer than this, m (keep it above
    /// the largest z, about 12 m at 380 kV).
    pub max_distance: f64,
}

impl Default for PhaseDistanceSettings {
    fn default() -> Self {
        Self {
            diagram: 0,
            spacing: 1.0,
            circuits: None,
            nearby: false,
            max_distance: 20.0,
        }
    }
}

/// A conductor as the check sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Conductor {
    /// The wire (section span).
    pub wire: Wire,
    /// Swing class w = d/q (d in mm, q in kg/m).
    pub w: f64,
    /// l_k at its first and last point, m.
    pub lk: [f64; 2],
    /// Insulation level of its circuit (its section's [`LEVEL_FIELD`]);
    /// not set: the default, high.
    pub level: Option<Level>,
}

/// Where a section keeps its circuit's insulation level: the LeV plugin's
/// column of the Circuits table (`low` or `high`).
pub const LEVEL_FIELD: &str = "lev-ch.insulation_level";

impl Level {
    /// From a value of [`LEVEL_FIELD`]: `low` or `high` (any case).
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "low" => Some(Self::Low),
            "high" => Some(Self::High),
            _ => None,
        }
    }
}

/// The worst station of a pair.
#[derive(Debug, Clone, PartialEq)]
pub struct Station {
    /// The point on the first conductor and on the second.
    pub at: [P3; 2],
    /// Where along each conductor, m of its length from its first point
    /// (near 0 or its length: at a clamp, where f is about 0).
    pub along: [f64; 2],
    /// Distance between them, m.
    pub distance: f64,
    /// Required z, m.
    pub required: f64,
    /// Local sag of each conductor, m.
    pub sag: [f64; 2],
    /// l_k of each conductor there, m.
    pub lk: [f64; 2],
    /// System angle β, degrees from the vertical.
    pub beta: f64,
    /// k of each conductor.
    pub k: [f64; 2],
    /// C (after the altitude), m.
    pub c: f64,
}

impl Station {
    /// The distance less the required one, m (negative: a violation).
    pub fn margin(&self) -> f64 {
        self.distance - self.required
    }
}

/// A pair of conductors checked.
#[derive(Debug, Clone, PartialEq)]
pub struct PhaseDistancePair {
    /// First conductor (index in those checked).
    pub a: usize,
    /// Second conductor.
    pub b: usize,
    /// Same circuit, or towards an earth wire: C₁.
    pub same_circuit: bool,
    /// Nominal voltage used, kV.
    pub kv: f64,
    /// Both hang between the same two structures (else a nearby pair).
    pub same_span: bool,
    /// Mean conductor height of the span, m above sea level.
    pub altitude: f64,
    /// The worst station.
    pub worst: Station,
    /// Length of conductor below the requirement, m: on the conductor
    /// and subconductor where it is longest (each stretch counted once,
    /// not per subconductor pair or measuring direction).
    pub length_below: f64,
}

/// l_k at the end of a conductor at `key` (`structure:set`): the length
/// of a pure suspension I-string; a V, a tension string or a wire clamped
/// to the structure: 0; a string below a V: its length; a TAK (tension
/// strings from a node on a suspension string): that suspension string's
/// length.
pub fn lk_at(line: &Line, key: &str) -> f64 {
    let Some((structure, set)) = key.rsplit_once(':') else {
        return 0.0;
    };
    let (Some(s), Ok(set)) = (line.structure(structure), set.parse::<u32>()) else {
        return 0.0;
    };
    let Some(w) = s.wire_at_set(set) else {
        return 0.0;
    };
    lk_of(s, &w.label)
}

fn lk_of(s: &Structure, wire: &str) -> f64 {
    let holding: Vec<_> = s.strings_to(wire).collect();
    let [x] = holding.as_slice() else {
        // None (clamped to the structure) or a V.
        return 0.0;
    };
    let length = s.string_length(x).unwrap_or(0.0);
    if !x.strain {
        // An I-string, or the string below a V.
        return length;
    }
    // A tension string: from a node (a TAK) hanging on one suspension
    // string, that string's length; else 0.
    if s.point(&x.top).is_none() && s.wire(&x.top).is_some() {
        let above: Vec<_> = s.strings_to(&x.top).collect();
        if let [y] = above.as_slice()
            && !y.strain
        {
            return s.string_length(y).unwrap_or(0.0);
        }
    }
    0.0
}

/// The conductors of a solved line: every section span (not jumpers),
/// with its swing class and its ends' l_k.
pub fn conductors(
    line: &Line,
    shapes: &[SectionShape],
    library: &ols_cables::Library,
) -> Vec<Conductor> {
    wires_of(line, shapes, &[])
        .into_iter()
        .map(|wire| {
            let section = line.sections.iter().find(|s| s.id == wire.section);
            let level = section
                .and_then(|s| s.fields.get(LEVEL_FIELD))
                .and_then(|v| Level::parse(v));
            let w = section
                .and_then(|s| library.get(&s.cable))
                .map_or(0.0, |c| {
                    let q = c.weight().si() / 9.806_65;
                    if q > 0.0 {
                        c.diameter.si() * 1000.0 / q
                    } else {
                        0.0
                    }
                });
            let lk = match wire.ends.as_slice() {
                [a, b] => [lk_at(line, a), lk_at(line, b)],
                _ => [0.0, 0.0],
            };
            Conductor { wire, w, lk, level }
        })
        .collect()
}

/// l_k at share `t` of a conductor's length, linear between its ends.
fn lk_linear(ends: [f64; 2], t: f64) -> f64 {
    ends[0] + (ends[1] - ends[0]) * t
}

/// The structures a wire hangs between, sorted.
fn span_key(w: &Wire) -> Option<[String; 2]> {
    if w.ends.len() != 2 || w.section.starts_with("jumper:") {
        return None;
    }
    let s = |e: &str| e.rsplit_once(':').map_or(e, |x| x.0).to_owned();
    let (a, b) = (s(&w.ends[0]), s(&w.ends[1]));
    Some(if a <= b { [a, b] } else { [b, a] })
}

/// A conductor's polylines: the centre line, or each subconductor of a
/// bundle (offsets across and up around it).
fn subconductors(w: &Wire) -> Vec<Vec<P3>> {
    let pts = &w.points;
    if w.bundle.len() <= 1 || pts.len() < 2 {
        return vec![pts.clone()];
    }
    let (f, l) = (pts[0], pts[pts.len() - 1]);
    let flat = (l[0] - f[0]).hypot(l[1] - f[1]);
    let right = if flat > 0.0 {
        [(l[1] - f[1]) / flat, -(l[0] - f[0]) / flat]
    } else {
        [1.0, 0.0]
    };
    w.bundle
        .iter()
        .map(|o| {
            pts.iter()
                .map(|p| [p[0] + right[0] * o[0], p[1] + right[1] * o[0], p[2] + o[1]])
                .collect()
        })
        .collect()
}

fn dist(a: P3, b: P3) -> f64 {
    (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f64>().sqrt()
}

/// A polyline with its piece lengths and the boxes of runs of pieces,
/// for fast closest-point searches.
struct Poly<'a> {
    points: &'a [P3],
    /// Length from the first point to each point, m.
    run: Vec<f64>,
    /// Bounding box (min, max) of each run of `CHUNK` pieces.
    boxes: Vec<(P3, P3)>,
}

/// Pieces per box.
const CHUNK: usize = 8;

impl<'a> Poly<'a> {
    fn new(points: &'a [P3]) -> Self {
        let mut run = vec![0.0];
        for w in points.windows(2) {
            run.push(run[run.len() - 1] + dist(w[0], w[1]));
        }
        let boxes = (0..points.len().saturating_sub(1))
            .step_by(CHUNK)
            .map(|k| {
                let ps = &points[k..(k + CHUNK + 1).min(points.len())];
                let mut lo = ps[0];
                let mut hi = ps[0];
                for p in ps {
                    for c in 0..3 {
                        lo[c] = lo[c].min(p[c]);
                        hi[c] = hi[c].max(p[c]);
                    }
                }
                (lo, hi)
            })
            .collect();
        Self { points, run, boxes }
    }

    fn total(&self) -> f64 {
        self.run[self.run.len() - 1]
    }

    /// Bounding box of the whole polyline.
    fn bounds(&self) -> (P3, P3) {
        self.boxes.iter().fold(
            ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]),
            |(lo, hi), (a, b)| {
                (
                    [0, 1, 2].map(|c| lo[c].min(a[c])),
                    [0, 1, 2].map(|c| hi[c].max(b[c])),
                )
            },
        )
    }

    /// Stations every `spacing` of its 3D length, its ends included:
    /// (point, length from its first point, length it stands for).
    fn stations(&self, spacing: f64) -> Vec<(P3, f64, f64)> {
        let total = self.total();
        if total <= 0.0 {
            return vec![(self.points[0], 0.0, 0.0)];
        }
        let n = (total / spacing.max(1e-3)).ceil().max(1.0) as usize;
        let step = total / n as f64;
        let mut k = 0;
        (0..=n)
            .map(|i| {
                let s = step * i as f64;
                while k + 2 < self.points.len() && self.run[k + 1] < s {
                    k += 1;
                }
                let l = self.run[k + 1] - self.run[k];
                let t = if l > 0.0 {
                    ((s - self.run[k]) / l).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let (a, b) = (self.points[k], self.points[k + 1]);
                let weight = if i == 0 || i == n { step / 2.0 } else { step };
                ([0, 1, 2].map(|c| a[c] + (b[c] - a[c]) * t), s, weight)
            })
            .collect()
    }

    /// The closest point to `p`: (point, length from the first point,
    /// distance); none farther than `cutoff`.
    fn closest(&self, p: P3, cutoff: f64) -> Option<(P3, f64, f64)> {
        let mut order: Vec<(f64, usize)> = self
            .boxes
            .iter()
            .enumerate()
            .map(|(i, (lo, hi))| {
                let d2: f64 = (0..3)
                    .map(|c| (lo[c] - p[c]).max(p[c] - hi[c]).max(0.0).powi(2))
                    .sum();
                (d2.sqrt(), i)
            })
            .filter(|(d, _)| *d <= cutoff)
            .collect();
        order.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut best: Option<(P3, f64, f64)> = None;
        for (bound, i) in order {
            if best.is_some_and(|b| bound >= b.2) {
                break;
            }
            let start = i * CHUNK;
            let end = (start + CHUNK).min(self.points.len() - 1);
            for k in start..end {
                let (a, b) = (self.points[k], self.points[k + 1]);
                let d = [0, 1, 2].map(|c| b[c] - a[c]);
                let dd: f64 = d.iter().map(|x| x * x).sum();
                let t = if dd > 0.0 {
                    ((0..3).map(|c| (p[c] - a[c]) * d[c]).sum::<f64>() / dd).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let q = [0, 1, 2].map(|c| a[c] + d[c] * t);
                let e = dist(p, q);
                if e <= cutoff && best.is_none_or(|b| e < b.2) {
                    let s = self.run[k] + (self.run[k + 1] - self.run[k]) * t;
                    best = Some((q, s, e));
                }
            }
        }
        best
    }
}

/// The sag of a polyline at `p` (a point of it): vertically below its
/// chord (the straight line between its ends), m; 0 at the ends.
fn local_sag(points: &[P3], p: P3) -> f64 {
    let (f, l) = (points[0], points[points.len() - 1]);
    let flat = (l[0] - f[0]).hypot(l[1] - f[1]);
    if flat <= 0.0 {
        return 0.0;
    }
    let t = (((p[0] - f[0]) * (l[0] - f[0]) + (p[1] - f[1]) * (l[1] - f[1])) / (flat * flat))
        .clamp(0.0, 1.0);
    (f[2] + (l[2] - f[2]) * t - p[2]).max(0.0)
}

/// Checks the pairs of `conductors` hanging between the same two
/// structures, and with [`PhaseDistanceSettings::nearby`] those coming
/// within its distance elsewhere. Not paired: jumpers, two earth wires,
/// two conductors of the same circuit and phase (one potential), two
/// sharing a clamp. The pairs are measured on all cores. Exit clause:
/// the budget's time limit and cancellation (checked per pair).
pub fn check(
    code: &PhaseDistanceCode,
    settings: &PhaseDistanceSettings,
    conductors: &[Conductor],
    budget: &Budget,
) -> Result<Vec<PhaseDistancePair>, String> {
    let diagram = code
        .diagrams
        .get(settings.diagram)
        .ok_or_else(|| format!("no diagram {} in {}", settings.diagram, code.name))?;
    let run = budget.start();
    let ticked = |c: &Conductor| {
        settings
            .circuits
            .as_ref()
            .is_none_or(|set| set.contains(&c.wire.circuit))
    };
    let keys: Vec<Option<[String; 2]>> = conductors.iter().map(|c| span_key(&c.wire)).collect();
    let subs: Vec<Vec<Vec<P3>>> = conductors.iter().map(|c| subconductors(&c.wire)).collect();
    let polys: Vec<Vec<Poly>> = subs
        .iter()
        .map(|s| {
            s.iter()
                .filter(|p| p.len() >= 2)
                .map(|p| Poly::new(p))
                .collect()
        })
        .collect();
    let bounds: Vec<Option<(P3, P3)>> = polys
        .iter()
        .map(|ps| {
            ps.iter().map(Poly::bounds).reduce(|(a, b), (c, d)| {
                (
                    [0, 1, 2].map(|k| a[k].min(c[k])),
                    [0, 1, 2].map(|k| b[k].max(d[k])),
                )
            })
        })
        .collect();
    let apart = |i: usize, j: usize| match (bounds[i], bounds[j]) {
        (Some((a, b)), Some((c, d))) => (0..3)
            .map(|k| (c[k] - b[k]).max(a[k] - d[k]).max(0.0).powi(2))
            .sum::<f64>()
            .sqrt(),
        _ => f64::INFINITY,
    };
    // The pairs to measure (cheap tests first).
    let mut candidates = Vec::new();
    for i in 0..conductors.len() {
        let Some(key) = &keys[i] else { continue };
        if !ticked(&conductors[i]) || polys[i].is_empty() {
            continue;
        }
        for j in i + 1..conductors.len() {
            let Some(other) = &keys[j] else { continue };
            if !ticked(&conductors[j]) || polys[j].is_empty() {
                continue;
            }
            let same_span = other == key;
            if !same_span && (!settings.nearby || apart(i, j) > settings.max_distance) {
                continue;
            }
            let (a, b) = (&conductors[i], &conductors[j]);
            if !a.wire.phase.is_empty()
                && a.wire.circuit == b.wire.circuit
                && a.wire.phase == b.wire.phase
            {
                // One potential.
                continue;
            }
            if a.wire.ends.iter().any(|e| b.wire.ends.contains(e)) {
                // Connected at a clamp (the next span of the same wire).
                continue;
            }
            if a.wire.kv().unwrap_or(0.0).max(b.wire.kv().unwrap_or(0.0)) <= 0.0 {
                // Two earth wires: nothing to keep apart.
                continue;
            }
            candidates.push((i, j, same_span));
        }
    }
    let measure = |i: usize, j: usize, same_span: bool| -> Option<PhaseDistancePair> {
        let (a, b) = (&conductors[i], &conductors[j]);
        // Earth wires (no potential) count as the same circuit (C₁).
        let earth = a.wire.potential.is_none() || b.wire.potential.is_none();
        let same_circuit = earth || a.wire.circuit == b.wire.circuit;
        let kv = a.wire.kv().unwrap_or(0.0).max(b.wire.kv().unwrap_or(0.0));
        // The insulation level of the circuit of the higher voltage
        // (equal: high when either is).
        let level_of = |c: &Conductor| c.level.unwrap_or_default();
        let level = match (a.wire.kv(), b.wire.kv()) {
            (Some(x), Some(y)) if x > y => level_of(a),
            (Some(x), Some(y)) if y > x => level_of(b),
            (Some(_), Some(_)) => {
                if level_of(a) == Level::High || level_of(b) == Level::High {
                    Level::High
                } else {
                    Level::Low
                }
            }
            (Some(_), None) => level_of(a),
            _ => level_of(b),
        };
        let all: Vec<f64> = a
            .wire
            .points
            .iter()
            .chain(&b.wire.points)
            .map(|p| p[2])
            .collect();
        let altitude = all.iter().sum::<f64>() / all.len().max(1) as f64;
        let c = code.c(same_circuit, kv, level) * code.altitude_factor(altitude);
        // Same span: every station; nearby: where within the distance.
        let cutoff = if same_span {
            f64::INFINITY
        } else {
            settings.max_distance
        };
        let mut worst: Option<Station> = None;
        let mut length_below = 0.0f64;
        // Stations on each conductor in turn, the closest point on the
        // other (its nearest subconductor).
        for swap in [false, true] {
            let (fi, si) = if swap { (j, i) } else { (i, j) };
            let (cf, cs) = (&conductors[fi], &conductors[si]);
            for first in &polys[fi] {
                let mut below = 0.0;
                for (p, sp, weight) in first.stations(settings.spacing) {
                    let mut station: Option<Station> = None;
                    for second in &polys[si] {
                        let Some((q, sq, d)) = second.closest(p, cutoff) else {
                            continue;
                        };
                        let dz = (p[2] - q[2]).abs();
                        let beta = if d > 0.0 {
                            (dz / d).clamp(0.0, 1.0).acos().to_degrees()
                        } else {
                            0.0
                        };
                        let fp = local_sag(first.points, p);
                        let fq = local_sag(second.points, q);
                        let lp = lk_linear(cf.lk, sp / first.total().max(1e-12));
                        let lq = lk_linear(cs.lk, sq / second.total().max(1e-12));
                        let (kp, kq) = (diagram.k(cf.w, beta), diagram.k(cs.w, beta));
                        let z = (kp * (fp + lp).max(0.0).sqrt())
                            .max(kq * (fq + lq).max(0.0).sqrt())
                            + c;
                        let st = if swap {
                            Station {
                                at: [q, p],
                                along: [sq, sp],
                                distance: d,
                                required: z,
                                sag: [fq, fp],
                                lk: [lq, lp],
                                beta,
                                k: [kq, kp],
                                c,
                            }
                        } else {
                            Station {
                                at: [p, q],
                                along: [sp, sq],
                                distance: d,
                                required: z,
                                sag: [fp, fq],
                                lk: [lp, lq],
                                beta,
                                k: [kp, kq],
                                c,
                            }
                        };
                        if station.as_ref().is_none_or(|w| st.margin() < w.margin()) {
                            station = Some(st);
                        }
                    }
                    let Some(st) = station else { continue };
                    if st.margin() < 0.0 {
                        below += weight;
                    }
                    if worst.as_ref().is_none_or(|w| st.margin() < w.margin()) {
                        worst = Some(st);
                    }
                }
                length_below = length_below.max(below);
            }
        }
        worst.map(|worst| PhaseDistancePair {
            a: i,
            b: j,
            same_circuit,
            kv,
            same_span,
            altitude,
            worst,
            length_below,
        })
    };
    // The pairs shared out over the cores, in their order.
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(candidates.len())
        .max(1);
    let next = AtomicUsize::new(0);
    let stopped: Mutex<Option<String>> = Mutex::new(None);
    let mut found: Vec<(usize, PhaseDistancePair)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        if let Err(stop) = run.check() {
                            *stopped.lock().expect("not poisoned") =
                                Some(format!("phase distances: {stop}"));
                            return mine;
                        }
                        let k = next.fetch_add(1, Ordering::Relaxed);
                        let Some(&(i, j, same_span)) = candidates.get(k) else {
                            return mine;
                        };
                        if let Some(pair) = measure(i, j, same_span) {
                            mine.push((k, pair));
                        }
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|w| w.join().expect("a phase distance worker panicked"))
            .collect()
    });
    if let Some(stop) = stopped.into_inner().expect("not poisoned") {
        return Err(stop);
    }
    found.sort_by_key(|(k, _)| *k);
    Ok(found.into_iter().map(|(_, pair)| pair).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ols_line::electrical::{Potential, SystemKind};
    use ols_line::{InsulatorString, StructurePoint, WirePoint};

    fn lev() -> PhaseDistanceCode {
        PhaseDistanceCode::parse(include_str!(
            "../../../libraries/phase-distance/lev-ch.olspd"
        ))
        .unwrap()
    }

    fn kv(u: f64) -> Option<Potential> {
        Some(Potential {
            nominal_kv: u,
            kind: SystemKind::Ac,
            frequency_hz: Some(50.0),
            to_earth: [u / 3f64.sqrt(), 0.0],
        })
    }

    /// A conductor between T1 and T2 along x (300 m), its clamps at
    /// (0, y, z) and (300, y, z + rise), sagging `sag` at mid-span
    /// (a parabola below the chord), 301 points.
    fn conductor(
        circuit: &str,
        set: u32,
        y: f64,
        z: f64,
        rise: f64,
        sag: f64,
        lk: [f64; 2],
    ) -> Conductor {
        let points = (0..=300)
            .map(|i| {
                let x = f64::from(i);
                let t = x / 300.0;
                [x, y, z + rise * t - 4.0 * sag * t * (1.0 - t)]
            })
            .collect();
        Conductor {
            wire: Wire {
                section: format!("S{set}"),
                span: 0,
                circuit: circuit.into(),
                phase: String::new(),
                potential: kv(380.0),
                bundle: Vec::new(),
                points,
                ends: vec![format!("T1:{set}"), format!("T2:{set}")],
            },
            w: 10.0,
            lk,
            level: Some(Level::Low),
        }
    }

    /// The default settings (the conductors are at the low level).
    fn low() -> PhaseDistanceSettings {
        PhaseDistanceSettings::default()
    }

    fn run(c: &[Conductor], s: &PhaseDistanceSettings) -> Vec<PhaseDistancePair> {
        check(&lev(), s, c, &Budget::new()).unwrap()
    }

    /// Written back and read again: the same values.
    #[test]
    fn written_and_read_back() {
        let c = lev();
        let text = c.to_text("edited\nby a test");
        assert!(text.starts_with("# edited\n# by a test\n"));
        assert_eq!(PhaseDistanceCode::parse(&text).unwrap(), c);
    }

    /// The shipped LeV values: C₁/C₂ by voltage and level, the next row
    /// up between rows, the altitude rule, k by β and swing class.
    #[test]
    fn the_lev_values() {
        let c = lev();
        assert_eq!(c.c(true, 380.0, Level::Low), 2.64);
        assert_eq!(c.c(true, 380.0, Level::High), 2.90);
        assert_eq!(c.c(false, 380.0, Level::High), 3.64);
        assert_eq!(c.c(true, 110.0, Level::Low), 0.92);
        assert_eq!(c.c(false, 110.0, Level::Low), 1.38);
        assert_eq!(c.c(false, 132.0, Level::High), 1.38);
        // 65 kV: the 110/132 kV row.
        assert_eq!(c.c(true, 65.0, Level::Low), 0.92);
        assert!((c.altitude_factor(500.0) - 1.0).abs() < 1e-12);
        assert!((c.altitude_factor(2000.0) - 1.14).abs() < 1e-12);
        let d = &c.diagrams[0];
        assert!((d.k(10.0, 0.0) - 0.70).abs() < 1e-12);
        assert!((d.k(10.0, 90.0) - 0.60).abs() < 1e-12);
        assert!((d.k(20.0, 45.0) - 0.70).abs() < 1e-12);
        assert!((d.k(50.0, 0.0) - 0.95).abs() < 1e-12);
        // At 650 N/m² the classes change: w = 20 is the second line there
        // too, w = 25 the third.
        assert!((c.diagrams[1].k(25.0, 0.0) - 0.85).abs() < 1e-12);
    }

    /// One conductor 6 m above the other (β = 0°), 380 kV, the same
    /// circuit, 12 m sag, 4.5 m suspension strings: at mid-span
    /// z = 0.70·√16.5 + 2.64 = 5.483 m against 6 m — the worst station;
    /// at the clamps z = 0.70·√4.5 + 2.64.
    #[test]
    fn one_above_the_other() {
        let a = conductor("C1", 1, 0.0, 30.0, 0.0, 12.0, [4.5, 4.5]);
        let b = conductor("C1", 2, 0.0, 24.0, 0.0, 12.0, [4.5, 4.5]);
        let pairs = run(&[a, b], &low());
        assert_eq!(pairs.len(), 1);
        let w = &pairs[0].worst;
        assert!((w.distance - 6.0).abs() < 1e-9);
        assert!(w.beta.abs() < 1e-6);
        assert!((w.sag[0] - 12.0).abs() < 1e-9, "{:?}", w.sag);
        let z = 0.70 * 16.5f64.sqrt() + 2.64;
        assert!((w.required - z).abs() < 1e-9, "{} {z}", w.required);
        assert_eq!(pairs[0].length_below, 0.0);
        // 5 m apart: below around mid-span only.
        let a = conductor("C1", 1, 0.0, 30.0, 0.0, 12.0, [4.5, 4.5]);
        let b = conductor("C1", 2, 0.0, 25.0, 0.0, 12.0, [4.5, 4.5]);
        let p = &run(&[a, b], &low())[0];
        assert!(p.worst.margin() < 0.0 && p.length_below > 0.0);
        assert!(
            p.length_below < 300.0,
            "not at the clamps: {}",
            p.length_below
        );
    }

    /// The length below the requirement: 5 m apart, f = 12 m, l_k = 4.5 m
    /// at both ends: z > 5 m where 48·t(1 − t) + 4.5 > (2.36 / 0.70)²,
    /// t from 0.177 to 0.823, about 194 m of the 300 m — counted once,
    /// not per measuring direction.
    #[test]
    fn length_below() {
        let a = conductor("C1", 1, 0.0, 30.0, 0.0, 12.0, [4.5, 4.5]);
        let b = conductor("C1", 2, 0.0, 25.0, 0.0, 12.0, [4.5, 4.5]);
        let p = &run(&[a, b], &low())[0];
        let t = 0.5 - (0.25 - (((2.36f64 / 0.70).powi(2) - 4.5) / 48.0)).sqrt();
        let expected = 300.0 * (1.0 - 2.0 * t);
        assert!(
            (p.length_below - expected).abs() < 3.0,
            "{} {expected}",
            p.length_below
        );
        // Where along: the worst at mid-span.
        assert!(
            (p.worst.along[0] - 150.0).abs() < 2.0,
            "{:?}",
            p.worst.along
        );
    }

    /// Nearby pairs: a parallel line 6 m to the side on its own
    /// structures is checked only when asked, within the distance; two
    /// conductors of one circuit and phase, or sharing a clamp, never.
    #[test]
    fn nearby_pairs() {
        let a = conductor("C1", 1, 0.0, 30.0, 0.0, 12.0, [4.5, 4.5]);
        let mut b = conductor("C2", 1, 6.0, 30.0, 0.0, 12.0, [4.5, 4.5]);
        b.wire.ends = vec!["T3:1".into(), "T4:1".into()];
        assert!(run(&[a.clone(), b.clone()], &low()).is_empty());
        let near = PhaseDistanceSettings {
            nearby: true,
            ..low()
        };
        let pairs = run(&[a.clone(), b.clone()], &near);
        assert_eq!(pairs.len(), 1);
        assert!(!pairs[0].same_span && !pairs[0].same_circuit);
        assert!((pairs[0].worst.distance - 6.0).abs() < 1e-6);
        let far = PhaseDistanceSettings {
            max_distance: 5.0,
            ..near.clone()
        };
        assert!(run(&[a.clone(), b.clone()], &far).is_empty());
        // One circuit and phase: one potential.
        let (mut a1, mut b1) = (a.clone(), b.clone());
        a1.wire.phase = "L1".into();
        b1.wire.phase = "L1".into();
        b1.wire.circuit = "C1".into();
        assert!(run(&[a1, b1], &near).is_empty());
        // Sharing a clamp: the next span.
        let mut c = b.clone();
        c.wire.ends = vec!["T2:1".into(), "T4:1".into()];
        assert!(run(&[a, c], &near).is_empty());
    }

    /// The level from the circuit's column: not set, the high row
    /// (C₁ 2.90 m at 380 kV instead of 2.64 m).
    #[test]
    fn level_not_set_is_high() {
        let mut a = conductor("C1", 1, 0.0, 30.0, 0.0, 12.0, [4.5, 4.5]);
        let mut b = conductor("C1", 2, 0.0, 24.0, 0.0, 12.0, [4.5, 4.5]);
        assert!((run(&[a.clone(), b.clone()], &low())[0].worst.c - 2.64).abs() < 1e-9);
        a.level = None;
        b.level = None;
        assert!((run(&[a, b], &low())[0].worst.c - 2.90).abs() < 1e-9);
        assert_eq!(Level::parse(" Low "), Some(Level::Low));
        assert_eq!(Level::parse("HIGH"), Some(Level::High));
        assert_eq!(Level::parse("medium"), None);
    }

    /// Side by side (β = 90°): k = 0.60; two circuits: C₂.
    #[test]
    fn side_by_side_and_two_circuits() {
        let a = conductor("C1", 1, 0.0, 30.0, 0.0, 12.0, [4.5, 4.5]);
        let b = conductor("C2", 2, 8.0, 30.0, 0.0, 12.0, [4.5, 4.5]);
        let p = &run(&[a, b], &low())[0];
        assert!(!p.same_circuit);
        assert!((p.worst.beta - 90.0).abs() < 1e-6);
        let z = 0.60 * 16.5f64.sqrt() + 3.37;
        assert!(
            (p.worst.required - z).abs() < 1e-9,
            "{} {z}",
            p.worst.required
        );
    }

    /// l_k linear between the ends: a tension string at one (0), a 3 m
    /// suspension string at the other: 1.5 m at mid-span.
    #[test]
    fn lk_is_linear_between_the_ends() {
        let a = conductor("C1", 1, 0.0, 30.0, 0.0, 12.0, [0.0, 3.0]);
        let b = conductor("C1", 2, 0.0, 24.0, 0.0, 12.0, [0.0, 3.0]);
        let p = &run(&[a, b], &low())[0];
        // l_k at the worst point: 3 m × its share of the length from the
        // tension end (the worst is a little past mid-span: f + l_k peaks
        // at t = 51/96, where l_k ≈ 1.59 m).
        let at = p.worst.at[0][0] / 300.0;
        assert!(
            (p.worst.lk[0] - 3.0 * at).abs() < 0.01,
            "{:?} at {at}",
            p.worst.lk
        );
        assert!(p.worst.lk[0] > 1.5 && p.worst.lk[0] < 1.65);
        // The larger of the two conductors' terms.
        let term = |i: usize| p.worst.k[i] * (p.worst.sag[i] + p.worst.lk[i]).sqrt();
        assert!(p.worst.beta < 1.0 && (p.worst.k[0] - 0.70).abs() < 0.002);
        let z = term(0).max(term(1)) + 2.64;
        assert!((p.worst.required - z).abs() < 1e-9);
        // Mid-span itself: l_k 1.5 exactly.
        assert!((lk_linear([0.0, 3.0], 0.5) - 1.5).abs() < 1e-12);
    }

    /// Above 1000 m: C grows by 14 % per 1000 m of the mean conductor
    /// height; circuits not ticked are not checked.
    #[test]
    fn altitude_and_circuits() {
        let a = conductor("C1", 1, 0.0, 2030.0, 0.0, 12.0, [4.5, 4.5]);
        let b = conductor("C1", 2, 0.0, 2024.0, 0.0, 12.0, [4.5, 4.5]);
        let p = &run(&[a.clone(), b.clone()], &low())[0];
        // Mean height 2027 − 2·12·(2/3)… the parabola's mean: 2019 m.
        let f = 1.0 + 0.14 * (p.altitude - 1000.0) / 1000.0;
        assert!((p.worst.c - 2.64 * f).abs() < 1e-9);
        assert!(p.altitude > 2000.0 && p.altitude < 2030.0, "{}", p.altitude);
        let s = PhaseDistanceSettings {
            circuits: Some(BTreeSet::from(["C2".to_owned()])),
            ..low()
        };
        assert!(run(&[a, b], &s).is_empty());
    }

    /// The string kinds: an I-string its length; a V, a tension string,
    /// a wire clamped to the structure 0; the string below a V its
    /// length; a TAK its suspension string's length.
    #[test]
    fn lk_by_string_kind() {
        let p = |label: &str, z: f64| StructurePoint {
            description: String::new(),
            label: label.into(),
            position: [0.0, 0.0, z],
        };
        let w = |label: &str, z: f64| WirePoint {
            label: label.into(),
            position: [0.0, 0.0, z],
            ..Default::default()
        };
        let s = |label: &str, top: &str, wire: &str, strain: bool| InsulatorString {
            label: label.into(),
            top: top.into(),
            wire: wire.into(),
            strain,
            ..Default::default()
        };
        let st = Structure {
            id: "T".into(),
            position: [0.0; 3],
            orientation_deg: 0.0,
            structure_type: None,
            extension: None,
            description: String::new(),
            height_adjust: 0.0,
            points: vec![p("A", 30.0), p("B", 30.0)],
            strings: vec![
                s("I", "A", "WI", false),
                s("Va", "A", "WV", false),
                s("Vb", "B", "WV", false),
                s("T", "A", "WT", true),
                s("UVa", "A", "N", false),
                s("UVb", "B", "N", false),
                s("Below", "N", "WB", false),
                s("Sus", "A", "K", false),
                s("Ta", "K", "WK", true),
            ],
            wires: vec![
                w("WI", 27.0),
                w("WV", 26.0),
                w("WT", 30.0),
                w("N", 27.0),
                w("WB", 25.5),
                w("K", 28.0),
                w("WK", 28.0),
                w("WC", 20.0),
            ],
            omit: Vec::new(),
            omit_sets: Vec::new(),
            members: Vec::new(),
            routes: Vec::new(),
            fields: Default::default(),
        };
        let mut st = st;
        // The tension string runs along: its wire point 3 m away.
        st.wires[2].position = [3.0, 0.0, 30.0];
        st.wires[6].position = [3.0, 0.0, 28.0];
        assert!((lk_of(&st, "WI") - 3.0).abs() < 1e-9);
        assert_eq!(lk_of(&st, "WV"), 0.0);
        assert_eq!(lk_of(&st, "WT"), 0.0);
        assert!((lk_of(&st, "WB") - 1.5).abs() < 1e-9);
        assert!((lk_of(&st, "WK") - 2.0).abs() < 1e-9);
        assert_eq!(lk_of(&st, "WC"), 0.0);
    }
}
