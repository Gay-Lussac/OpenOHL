// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Galloping ellipses (DESIGN_GOALS §13): the space a galloping wire may
//! sweep, drawn as an ellipse around the wire's position in a weather case,
//! and the clearance between the ellipses of wires in the same span —
//! phase to phase, circuit to circuit, phase to earth wire.
//!
//! The published empirical methods (A. E. Davison, "Dancing conductors",
//! AIEE Trans. 1939; the single- and two-loop ellipses of RUS Bulletin
//! 1724E-200; the amplitude formulas collected in CIGRE TB 322, "State of
//! the art of conductor galloping", 2007) size the ellipse from the span's
//! sag, its length, the conductor diameter and the number of loops. OpenOHL
//! ships none of their coefficients: every coefficient comes from a
//! parameter file (`*.olsg`), and the shipped example's are invented.
//!
//! ```toml
//! format = "ols-galloping"
//! version = 1
//! name = "Example — invented values"
//! required = "0.3 m"            # or { by_voltage = [[...]] }, as in rules
//! pair_voltage = "difference"   # higher (default), sum, difference
//! stations = 20                 # ellipses compared at 19 inner stations
//! cases = ["galloping*"]        # weather cases it applies to (default all)
//!
//! [[method]]
//! name = "Single loop"
//! loops = 1
//! # Major axis A = sag·D + span·L + sqrt_sag·√D + diameter·d
//! #               + log.factor·n·d·ln(log.scale·D / (n²·d)) + constant
//! major = { sag = 1.25, constant = "0.3 m" }
//! minor = { major = 0.4 }       # minor axis B: the same terms, + major·A
//! tilt = 10.0                   # major axis from vertical, ° (or "10°"), top to the right
//! centre_above_rest = 0.3       # ellipse centre above the wire, × A
//! along = "sine"                # size along the span: sine (loops) or constant
//! ```
//!
//! D is the wire's sag in the weather case checked (the largest vertical
//! distance below the chord), L the horizontal span length, d the
//! conductor diameter, n the number of loops. Along the span the ellipse
//! is scaled by |sin(nπt)| at the fraction t of the span (`sine`: each
//! loop largest at its middle) or kept the same (`constant`); its centre
//! moves with it. Ellipses are drawn in the plane square to the span at
//! each station, `right` being to the right looking from the span's first
//! to its last structure of the first wire of the pair.
//!
//! Accuracy: ellipses are compared as inscribed 72-sided polygons — the
//! distance is high by at most A·(1 − cos 2.5°)/2 ≈ 0.05 % of A; wires of
//! one span are taken to be parallel (their cross-section planes the same).

use std::path::Path;

use ols_core::budget::Budget;
use ols_core::units::Length;
use ols_line::Line;
use ols_line::solve::SectionShape;
use serde::Deserialize;

use crate::checks::{Wire, wires_of};
use crate::geometry::P3;
use crate::rules::{PairVoltage, RawRequirement, Requirement};

/// File extension of galloping parameter files.
pub const GALLOPING_FILE_EXTENSION: &str = "olsg";
/// Format identifier.
pub const GALLOPING_FILE_FORMAT: &str = "ols-galloping";
/// Newest format version read.
pub const GALLOPING_FILE_VERSION: u32 = 1;

/// Vertices of the polygons standing in for the ellipses.
const POLYGON: usize = 72;

/// One axis of an ellipse as a sum of terms (m): sag D, span L, conductor
/// diameter d, loops n, and (minor axis) the major axis A.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AxisFormula {
    /// Factor on the sag.
    pub sag: f64,
    /// Factor on the span length.
    pub span: f64,
    /// Factor on √D, √m.
    pub sqrt_sag: f64,
    /// Factor on the conductor diameter.
    pub diameter: f64,
    /// Factor of the logarithmic term n·d·ln(scale·D / (n²·d)).
    pub log_factor: f64,
    /// Scale inside the logarithm.
    pub log_scale: f64,
    /// Constant, m.
    pub constant: f64,
    /// Factor on the major axis (minor axis only).
    pub major: f64,
}

impl AxisFormula {
    /// The axis length, m (never negative), for sag `d_sag`, span `span`,
    /// conductor diameter `dia`, `loops` and major axis `major` (m).
    pub fn eval(&self, d_sag: f64, span: f64, dia: f64, loops: u32, major: f64) -> f64 {
        let n = f64::from(loops);
        let mut v = self.sag * d_sag
            + self.span * span
            + self.sqrt_sag * d_sag.max(0.0).sqrt()
            + self.diameter * dia
            + self.constant
            + self.major * major;
        if self.log_factor != 0.0 && dia > 0.0 && d_sag > 0.0 {
            let arg = self.log_scale * d_sag / (n * n * dia);
            if arg > 0.0 {
                v += self.log_factor * n * dia * arg.ln();
            }
        }
        v.max(0.0)
    }
}

/// How the ellipse's size varies along the span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Along {
    /// Scaled by |sin(nπt)|: largest in the middle of each loop.
    #[default]
    Sine,
    /// The same all along.
    Constant,
}

/// One galloping method: an ellipse of a number of loops.
#[derive(Debug, Clone, PartialEq)]
pub struct Method {
    /// Name, for reports.
    pub name: String,
    /// Loops in the span.
    pub loops: u32,
    /// Major axis (full length).
    pub major: AxisFormula,
    /// Minor axis (full length).
    pub minor: AxisFormula,
    /// Major axis from vertical, degrees, top to the right.
    pub tilt_deg: f64,
    /// Ellipse centre above the wire at rest, as a fraction of the major
    /// axis (0.5: the wire at the ellipse's lower end).
    pub centre_above_rest: f64,
    /// Size along the span.
    pub along: Along,
}

/// An ellipse in the plane square to a span: (right, up), m.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ellipse {
    /// Centre.
    pub centre: [f64; 2],
    /// Half the major axis, m.
    pub semi_major: f64,
    /// Half the minor axis, m.
    pub semi_minor: f64,
    /// Major axis from vertical, radians, top to the right.
    pub tilt: f64,
}

impl Ellipse {
    /// The point at parameter `theta` (0: the upper end of the major axis).
    pub fn point(&self, theta: f64) -> [f64; 2] {
        let (s, c) = self.tilt.sin_cos();
        let (major, minor) = ([s, c], [c, -s]);
        let (a, b) = (self.semi_major * theta.cos(), self.semi_minor * theta.sin());
        [
            self.centre[0] + a * major[0] + b * minor[0],
            self.centre[1] + a * major[1] + b * minor[1],
        ]
    }

    /// The ellipse moved by `d`.
    pub fn moved(&self, d: [f64; 2]) -> Self {
        Self {
            centre: [self.centre[0] + d[0], self.centre[1] + d[1]],
            ..*self
        }
    }

    fn polygon(&self) -> Vec<[f64; 2]> {
        (0..POLYGON)
            .map(|k| self.point(std::f64::consts::TAU * k as f64 / POLYGON as f64))
            .collect()
    }
}

impl Method {
    /// The ellipse around the wire at rest (the origin) at fraction `t` of
    /// a span with sag `d_sag`, length `span`, conductor diameter `dia`.
    pub fn ellipse(&self, d_sag: f64, span: f64, dia: f64, t: f64) -> Ellipse {
        let a = self.major.eval(d_sag, span, dia, self.loops, 0.0);
        let b = self.minor.eval(d_sag, span, dia, self.loops, a);
        let k = match self.along {
            Along::Sine => (f64::from(self.loops) * std::f64::consts::PI * t)
                .sin()
                .abs(),
            Along::Constant => 1.0,
        };
        let tilt = self.tilt_deg.to_radians();
        let up = self.centre_above_rest * a * k;
        Ellipse {
            centre: [up * tilt.sin(), up * tilt.cos()],
            semi_major: a * k / 2.0,
            semi_minor: b * k / 2.0,
            tilt,
        }
    }
}

/// Galloping parameters: methods and the clearance between ellipses.
#[derive(Debug, Clone, PartialEq)]
pub struct GallopingParams {
    /// Name.
    pub name: String,
    /// Methods (each checked on its own).
    pub methods: Vec<Method>,
    /// Required clearance between two ellipses (none: distances only).
    pub required: Option<Requirement>,
    /// Voltage of a pair, for a requirement by voltage.
    pub pair_voltage: PairVoltage,
    /// Stations along the span: ellipses compared at `stations − 1` inner
    /// points.
    pub stations: usize,
    /// Weather cases the check applies to (`*` `?` wildcards; empty: all).
    pub cases: Vec<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawLog {
    factor: f64,
    scale: f64,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawAxis {
    #[serde(default)]
    sag: f64,
    #[serde(default)]
    span: f64,
    #[serde(default)]
    sqrt_sag: f64,
    #[serde(default)]
    diameter: f64,
    #[serde(default)]
    log: Option<RawLog>,
    #[serde(default)]
    constant: Option<Length>,
    #[serde(default)]
    major: f64,
}

impl RawAxis {
    fn resolve(self) -> AxisFormula {
        let log = self.log.unwrap_or_default();
        AxisFormula {
            sag: self.sag,
            span: self.span,
            sqrt_sag: self.sqrt_sag,
            diameter: self.diameter,
            log_factor: log.factor,
            log_scale: log.scale,
            constant: self.constant.map_or(0.0, |l| l.si()),
            major: self.major,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMethod {
    name: String,
    loops: u32,
    major: RawAxis,
    minor: RawAxis,
    // Degrees, or text with its unit ("10°").
    #[serde(default, deserialize_with = "ols_core::units::si::de_degrees")]
    tilt: f64,
    #[serde(default)]
    centre_above_rest: f64,
    #[serde(default)]
    along: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawParams {
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    version: Option<u32>,
    #[serde(default)]
    name: String,
    #[serde(default)]
    required: Option<RawRequirement>,
    #[serde(default)]
    pair_voltage: Option<String>,
    #[serde(default)]
    stations: Option<usize>,
    #[serde(default)]
    cases: Vec<String>,
    #[serde(rename = "method", default)]
    methods: Vec<RawMethod>,
}

impl GallopingParams {
    /// Parses and checks a parameter file.
    pub fn parse(text: &str) -> Result<Self, String> {
        let raw: RawParams = toml::from_str(text).map_err(|e| e.to_string())?;
        ols_core::files::check_header(
            raw.format.as_deref(),
            raw.version,
            GALLOPING_FILE_FORMAT,
            GALLOPING_FILE_VERSION,
            "galloping",
        )?;
        if raw.methods.is_empty() {
            return Err("give at least one [[method]]".into());
        }
        let methods = raw
            .methods
            .into_iter()
            .map(|m| {
                let bad = |e: &str| Err(format!("method \"{}\": {e}", m.name));
                if !(1..=10).contains(&m.loops) {
                    return bad("loops must be 1 to 10");
                }
                if !(m.tilt.is_finite() && m.tilt.abs() <= 90.0) {
                    return bad("tilt must be within ±90°");
                }
                if !m.centre_above_rest.is_finite() {
                    return bad("centre_above_rest must be a number");
                }
                let along = match m.along.as_deref() {
                    None | Some("sine") => Along::Sine,
                    Some("constant") => Along::Constant,
                    Some(other) => return bad(&format!("along \"{other}\": use sine or constant")),
                };
                if m.major.major != 0.0 {
                    return bad("the major axis cannot depend on itself (major = …)");
                }
                Ok(Method {
                    name: m.name,
                    loops: m.loops,
                    major: m.major.resolve(),
                    minor: m.minor.resolve(),
                    tilt_deg: m.tilt,
                    centre_above_rest: m.centre_above_rest,
                    along,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let stations = raw.stations.unwrap_or(20);
        if !(2..=1000).contains(&stations) {
            return Err("stations must be 2 to 1000".into());
        }
        Ok(Self {
            name: raw.name,
            methods,
            required: raw.required.map(RawRequirement::resolve).transpose()?,
            pair_voltage: raw
                .pair_voltage
                .as_deref()
                .map_or(Ok(PairVoltage::Higher), PairVoltage::parse)?,
            stations,
            cases: raw.cases,
        })
    }

    /// Reads a parameter file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Whether the check applies to weather case `case`.
    pub fn applies(&self, case: &str) -> bool {
        self.cases.is_empty()
            || self
                .cases
                .iter()
                .any(|c| ols_core::pattern::matches(c, case))
    }
}

/// A span's wire with what its ellipse needs.
#[derive(Debug, Clone, PartialEq)]
pub struct GallopingWire {
    /// The wire (a span of a section).
    pub wire: Wire,
    /// Its sag in the weather case, m.
    pub sag: f64,
    /// Conductor diameter, m.
    pub diameter: f64,
}

/// The spans of a solved line (jumpers left out), with their sags and
/// conductor diameters from `library`.
pub fn galloping_wires(
    line: &Line,
    shapes: &[SectionShape],
    library: &ols_cables::Library,
) -> Vec<GallopingWire> {
    let sags = shapes.iter().flat_map(|s| s.spans.iter().map(|x| x.sag));
    wires_of(line, shapes, &[])
        .into_iter()
        .zip(sags)
        .map(|(wire, sag)| {
            let diameter = line
                .sections
                .iter()
                .find(|s| s.id == wire.section)
                .and_then(|s| library.get(&s.cable))
                .map_or(0.0, |c| c.diameter.si());
            GallopingWire {
                wire,
                sag,
                diameter,
            }
        })
        .collect()
}

/// The clearance between the ellipses of two wires of a span, by one
/// method: the smallest over the stations.
#[derive(Debug, Clone, PartialEq)]
pub struct GallopingPair {
    /// First wire (index in the wires checked).
    pub a: usize,
    /// Second wire.
    pub b: usize,
    /// Method (index in the parameters).
    pub method: usize,
    /// Smallest distance between the ellipses, m (0: they overlap).
    pub distance: f64,
    /// The ellipses overlap somewhere.
    pub overlap: bool,
    /// Where: fraction of the span along the first wire.
    pub at: f64,
    /// The wires at rest there.
    pub rest: [P3; 2],
    /// The ellipses there, in the first wire's cross-section (right, up)
    /// from its position at rest.
    pub ellipses: [Ellipse; 2],
    /// Major and minor axes of each wire's ellipse at its largest, m.
    pub axes: [[f64; 2]; 2],
    /// Required clearance (by the pair's voltage).
    pub required: Option<f64>,
}

impl GallopingPair {
    /// Whether the required clearance is not kept.
    pub fn violates(&self) -> bool {
        self.required.is_some_and(|r| self.distance < r)
    }
}

/// The structures a wire hangs between, sorted.
fn span_key(w: &Wire) -> Option<[String; 2]> {
    if w.ends.len() != 2 || w.section.starts_with("jumper:") {
        return None;
    }
    let s = |e: &str| e.rsplitn(3, ':').last().unwrap_or(e).to_owned();
    let (a, b) = (s(&w.ends[0]), s(&w.ends[1]));
    Some(if a <= b { [a, b] } else { [b, a] })
}

/// The point of a polyline at fraction `t` of its horizontal length.
pub fn at_fraction(points: &[P3], t: f64) -> P3 {
    let mut along = vec![0.0];
    for s in points.windows(2) {
        let last = *along.last().expect("starts with 0");
        along.push(last + (s[1][0] - s[0][0]).hypot(s[1][1] - s[0][1]));
    }
    let total = *along.last().expect("one point at least");
    let target = t.clamp(0.0, 1.0) * total;
    let k = along
        .partition_point(|&a| a < target)
        .clamp(1, points.len().max(2) - 1);
    if points.len() < 2 {
        return points[0];
    }
    let (a0, a1) = (along[k - 1], along[k]);
    let f = if a1 > a0 {
        (target - a0) / (a1 - a0)
    } else {
        0.0
    };
    let (p, q) = (points[k - 1], points[k]);
    [
        p[0] + (q[0] - p[0]) * f,
        p[1] + (q[1] - p[1]) * f,
        p[2] + (q[2] - p[2]) * f,
    ]
}

fn seg_distance(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let len2 = d[0] * d[0] + d[1] * d[1];
    let t = if len2 > 0.0 {
        (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p[0] - a[0] - d[0] * t).hypot(p[1] - a[1] - d[1] * t)
}

fn cross(o: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// Whether `p` is inside the convex polygon `poly` (counter-clockwise).
fn inside(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
    let n = poly.len();
    let sign = cross(poly[0], poly[1], poly[2]).signum();
    (0..n).all(|k| cross(poly[k], poly[(k + 1) % n], p) * sign >= 0.0)
}

fn segments_cross(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let (d1, d2) = (cross(c, d, a), cross(c, d, b));
    let (d3, d4) = (cross(a, b, c), cross(a, b, d));
    d1 * d2 < 0.0 && d3 * d4 < 0.0
}

/// The distance between two ellipses (as inscribed polygons), m; 0 when
/// they overlap.
pub fn ellipse_distance(a: &Ellipse, b: &Ellipse) -> f64 {
    let (pa, pb) = (a.polygon(), b.polygon());
    let degenerate = |e: &Ellipse| e.semi_major <= 0.0 || e.semi_minor <= 0.0;
    if !degenerate(a) && pb.iter().any(|p| inside(*p, &pa))
        || !degenerate(b) && pa.iter().any(|p| inside(*p, &pb))
    {
        return 0.0;
    }
    let n = POLYGON;
    let mut best = f64::INFINITY;
    for i in 0..n {
        let (a0, a1) = (pa[i], pa[(i + 1) % n]);
        for j in 0..n {
            let (b0, b1) = (pb[j], pb[(j + 1) % n]);
            if segments_cross(a0, a1, b0, b1) {
                return 0.0;
            }
            best = best
                .min(seg_distance(a0, b0, b1))
                .min(seg_distance(b0, a0, a1));
        }
    }
    best
}

/// Checks the ellipses of every pair of wires hanging between the same two
/// structures, by every method: the smallest distance of each pair and
/// method. Exit clause: `budget`, one step per pair and method.
///
/// Cost: `stations` × 72² point-to-edge distances per pair and method —
/// about 0.1 ms; a 500-span line with 6 wires a span in well under a
/// second.
pub fn check(
    params: &GallopingParams,
    wires: &[GallopingWire],
    budget: &Budget,
) -> Result<Vec<GallopingPair>, String> {
    let mut run = budget.start();
    let keys: Vec<Option<[String; 2]>> = wires.iter().map(|w| span_key(&w.wire)).collect();
    let mut out = Vec::new();
    for i in 0..wires.len() {
        let Some(key) = &keys[i] else { continue };
        for j in i + 1..wires.len() {
            if keys[j].as_ref() != Some(key) || wires[j].wire.section == wires[i].wire.section {
                continue;
            }
            let (wa, wb) = (&wires[i], &wires[j]);
            let (pa, pb) = (&wa.wire.points, &wb.wire.points);
            if pa.len() < 2 || pb.len() < 2 {
                continue;
            }
            let (first, last) = (pa[0], pa[pa.len() - 1]);
            let len = (last[0] - first[0]).hypot(last[1] - first[1]);
            if len <= 0.0 {
                continue;
            }
            // Right of the first wire's direction, in plan.
            let right = [(last[1] - first[1]) / len, -(last[0] - first[0]) / len];
            let flat = |p: &P3, q: &P3| (p[0] - q[0]).hypot(p[1] - q[1]);
            let reversed = flat(&pb[0], &first) > flat(&pb[0], &last);
            let span_b = {
                let (f, l) = (pb[0], pb[pb.len() - 1]);
                (l[0] - f[0]).hypot(l[1] - f[1])
            };
            let kv = params
                .pair_voltage
                .of(wa.wire.potential.as_ref(), wb.wire.potential.as_ref());
            let required = params.required.as_ref().map(|r| {
                r.at(kv.unwrap_or(0.0)).unwrap_or_else(|_| match r {
                    Requirement::ByVoltage(rows) => rows.last().map_or(0.0, |x| x.1),
                    _ => 0.0,
                })
            });
            for (m, method) in params.methods.iter().enumerate() {
                run.step()
                    .map_err(|e| format!("galloping check stopped: {e}"))?;
                let largest = |w: &GallopingWire, span: f64| {
                    let e = Method {
                        along: Along::Constant,
                        ..method.clone()
                    }
                    .ellipse(w.sag, span, w.diameter, 0.5);
                    [2.0 * e.semi_major, 2.0 * e.semi_minor]
                };
                let mut best: Option<GallopingPair> = None;
                for k in 1..params.stations {
                    let t = k as f64 / params.stations as f64;
                    let tb = if reversed { 1.0 - t } else { t };
                    let (ra, rb) = (at_fraction(pa, t), at_fraction(pb, tb));
                    let ea = method.ellipse(wa.sag, len, wa.diameter, t);
                    let offset = [
                        (rb[0] - ra[0]) * right[0] + (rb[1] - ra[1]) * right[1],
                        rb[2] - ra[2],
                    ];
                    let eb = method.ellipse(wb.sag, span_b, wb.diameter, t).moved(offset);
                    // Quick bound: centres apart by more than both ellipses.
                    let apart = (eb.centre[0] - ea.centre[0]).hypot(eb.centre[1] - ea.centre[1])
                        - ea.semi_major
                        - eb.semi_major;
                    if best.as_ref().is_some_and(|b| apart >= b.distance) {
                        continue;
                    }
                    let d = ellipse_distance(&ea, &eb);
                    if best.as_ref().is_none_or(|b| d < b.distance) {
                        best = Some(GallopingPair {
                            a: i,
                            b: j,
                            method: m,
                            distance: d,
                            overlap: d <= 0.0,
                            at: t,
                            rest: [ra, rb],
                            ellipses: [ea, eb],
                            axes: [largest(wa, len), largest(wb, span_b)],
                            required,
                        });
                    }
                }
                out.extend(best);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(text: &str) -> GallopingParams {
        GallopingParams::parse(text).unwrap()
    }

    const CIRCLE: &str = "[[method]]\nname = \"c\"\nloops = 1\nmajor = { constant = \"2 m\" }\nminor = { major = 1.0 }\nalong = \"constant\"\n";

    fn level(section: &str, y: f64, z: f64, sag: f64) -> GallopingWire {
        GallopingWire {
            wire: Wire {
                section: section.into(),
                span: 0,
                circuit: "c".into(),
                phase: section.into(),
                potential: None,
                bundle: vec![],
                points: (0..=40)
                    .map(|k| {
                        [
                            k as f64 * 10.0,
                            y,
                            z - sag * (1.0 - (k as f64 / 20.0 - 1.0).powi(2)),
                        ]
                    })
                    .collect(),
                ends: vec![format!("T1:1:{section}"), format!("T2:1:{section}")],
            },
            sag,
            diameter: 0.03,
        }
    }

    #[test]
    fn axes_from_sag_by_hand() {
        // Invented coefficients: A = 1.25 D + 0.3 m, B = 0.4 A.
        let p = params(
            "[[method]]\nname = \"one\"\nloops = 1\nmajor = { sag = 1.25, constant = \"0.3 m\" }\nminor = { major = 0.4 }\ntilt = \"10°\"\ncentre_above_rest = 0.5\n",
        );
        let m = &p.methods[0];
        let e = m.ellipse(10.0, 400.0, 0.03, 0.5);
        assert!((2.0 * e.semi_major - 12.8).abs() < 1e-12);
        assert!((2.0 * e.semi_minor - 5.12).abs() < 1e-12);
        // The wire at rest is the lower end of the major axis.
        let low = e.point(std::f64::consts::PI);
        assert!(low[0].abs() < 1e-12 && low[1].abs() < 1e-12, "{low:?}");
        // Centre 6.4 m along the axis tilted 10° to the right.
        assert!((e.centre[0] - 6.4 * 10f64.to_radians().sin()).abs() < 1e-12);
        // A quarter of the span: sin(π/4) of the size.
        let q = m.ellipse(10.0, 400.0, 0.03, 0.25);
        assert!((2.0 * q.semi_major - 12.8 * 0.5f64.sqrt()).abs() < 1e-12);
        // Two loops: zero at mid-span, largest at the quarter points.
        let two = Method {
            loops: 2,
            ..m.clone()
        };
        assert!(two.ellipse(10.0, 400.0, 0.03, 0.5).semi_major < 1e-12);
        assert!((two.ellipse(10.0, 400.0, 0.03, 0.25).semi_major - 6.4).abs() < 1e-12);
        // Logarithmic term: 80·n·d·ln(8 D / (n² d)) for D = 10, d = 0.03.
        let log = AxisFormula {
            log_factor: 80.0,
            log_scale: 8.0,
            ..AxisFormula::default()
        };
        let v = log.eval(10.0, 400.0, 0.03, 1, 0.0);
        assert!((v - 80.0 * 0.03 * (8.0 * 10.0 / 0.03f64).ln()).abs() < 1e-9);
    }

    #[test]
    fn circles_apart_by_hand() {
        let p = params(CIRCLE);
        let (a, b) = (
            p.methods[0].ellipse(5.0, 400.0, 0.03, 0.5),
            p.methods[0]
                .ellipse(5.0, 400.0, 0.03, 0.5)
                .moved([5.0, 0.0]),
        );
        // Radius 1 m each, 5 m apart: 3 m.
        assert!((ellipse_distance(&a, &b) - 3.0).abs() < 1e-9);
        assert_eq!(ellipse_distance(&a, &a.moved([1.5, 0.0])), 0.0);
        // Two thin ellipses crossing: no vertex inside the other.
        let thin = |tilt: f64| Ellipse {
            centre: [0.0, 0.0],
            semi_major: 5.0,
            semi_minor: 0.01,
            tilt,
        };
        assert_eq!(ellipse_distance(&thin(0.0), &thin(1.2)), 0.0);
    }

    #[test]
    fn pairs_of_a_span() {
        // Two wires 5 m apart, the same sag; a third in another span.
        let mut other = level("C", 30.0, 30.0, 10.0);
        other.wire.ends = vec!["T2:1:1".into(), "T3:1:1".into()];
        let wires = vec![
            level("A", 0.0, 30.0, 10.0),
            level("B", 5.0, 30.0, 10.0),
            other,
        ];
        let pairs = check(
            &params(&format!("required = \"3.5 m\"\n{CIRCLE}")),
            &wires,
            &Budget::new(),
        )
        .unwrap();
        assert_eq!(pairs.len(), 1);
        let p = &pairs[0];
        assert_eq!((p.a, p.b), (0, 1));
        assert!((p.distance - 3.0).abs() < 1e-9, "{p:?}");
        assert!(p.violates());
        // Vertical arrangement, 8 m apart, single loop A = D: the upper
        // ellipse's lower end on its wire, the lower one 8 m below with
        // its top 10 m above its wire at mid-span — 2 m overlap.
        let vertical = params(
            "[[method]]\nname = \"v\"\nloops = 1\nmajor = { sag = 1.0 }\nminor = { major = 0.5 }\ncentre_above_rest = 0.5\n",
        );
        let wires = vec![level("U", 0.0, 38.0, 10.0), level("L", 0.0, 30.0, 10.0)];
        let p = &check(&vertical, &wires, &Budget::new()).unwrap()[0];
        assert!(p.overlap && p.distance == 0.0);
        // 12 m apart: at mid-span 2 m; nearer the towers the ellipses
        // shrink with sin(πt) and the gap grows.
        let wires = vec![level("U", 0.0, 42.0, 10.0), level("L", 0.0, 30.0, 10.0)];
        let p = &check(&vertical, &wires, &Budget::new()).unwrap()[0];
        assert!(
            (p.distance - 2.0).abs() < 1e-9 && (p.at - 0.5).abs() < 1e-12,
            "{p:?}"
        );
        // Exit clause.
        let stopped = check(&vertical, &wires, &Budget::new().max_iterations(0));
        assert!(stopped.is_err());
    }

    #[test]
    fn rejects_bad_files() {
        for text in [
            "name = \"none\"",
            "[[method]]\nname = \"a\"\nloops = 0\nmajor = {}\nminor = {}",
            "[[method]]\nname = \"a\"\nloops = 1\nmajor = { major = 1.0 }\nminor = {}",
            "[[method]]\nname = \"a\"\nloops = 1\nmajor = {}\nminor = {}\nalong = \"cosine\"",
            "[[method]]\nname = \"a\"\nloops = 1\nmajor = {}\nminor = {}\ntilt = 120.0",
            "stations = 1\n[[method]]\nname = \"a\"\nloops = 1\nmajor = {}\nminor = {}",
            "format = \"ols-clearance-rules\"\n[[method]]\nname = \"a\"\nloops = 1\nmajor = {}\nminor = {}",
        ] {
            assert!(GallopingParams::parse(text).is_err(), "{text}");
        }
    }
}
