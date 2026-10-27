// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The magnetic flux density of the line (roadmap O18 step 4; built, in
//! active development, not approved yet).
//!
//! **3D** ([`Model::ThreeD`]): every conductor and earth wire as solved in
//! a weather case — the solver's polylines, a bundle subconductor by
//! subconductor ([`ohl_line::Bundle::polylines`]) — each straight piece
//! carrying a complex current (rms phasor); the field of a finite straight
//! segment in closed form ([`segment_field`], Biot–Savart), summed as
//! phasors per component. **2D** ([`Model::TwoD`]): the wires of a
//! cross-section as infinite straight parallel conductors
//! ([`line_field`]).
//!
//! Reported: the rms of the vector
//! \\( B = \sqrt{|B_x|^2 + |B_y|^2 + |B_z|^2} \\) and beside it the largest
//! value over the cycle, the semi-major axis of the field's ellipse (as an
//! rms value: \\( B_{max} \le B \\)). Systems of different frequencies are
//! computed apart and added as \\( \sqrt{\sum_f B_f^2} \\).
//!
//! Currents per circuit: given, or the wire's thermal rating
//! ([`thermal_currents`]); a direction per circuit (the load flow along
//! the line or against it); balanced phases at the systems' phase angles,
//! no harmonics. Earth wire currents optionally, induced by the phases on
//! an infinitely long earth wire earthed everywhere
//! (\\( I_e = -Z_{ee}^{-1} Z_{ep} I_p \\), per span from its mean
//! cross-section). The ground's return currents are left out (an
//! assumption: see the theory page, *Magnetic field*).
//!
//! A limit is optional input (µT), never a built-in value: with it, the
//! margin per point and the limit line's extent per profile.

use std::collections::BTreeMap;
use std::f64::consts::PI;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use nalgebra::{Complex, DMatrix, DVector};
use ohl_line::electrical::SystemKind;
use serde::Serialize;

use crate::cross_section::{
    self, Axis, CrossSection, LineSpan, PhaseData, Sources, StationAt, line_spans,
};
use crate::earth::MU0;
use crate::matrices::{self, Wire};
use crate::parameters::{CircuitKey, DEFAULT_FREQUENCY_HZ};

/// A complex number.
pub type C = Complex<f64>;

/// The line angle beyond which the 2D model is not to be used near it,
/// degrees (BAFU 2007, *Hochspannungsleitungen. Vollzugshilfe zur NISV*,
/// 8.1.4.2: a deviation from the straight line of more than 5°).
pub const LINE_ANGLE_2D_DEG: f64 = 5.0;

/// How far the wires' places relative to each other may differ between a
/// span's two structures before the span counts as one between
/// differently arranged structures (2D not to be used), m. A choice of
/// the program (to confirm by the owner).
pub const ARRANGEMENT_TOLERANCE_M: f64 = 0.5;

/// How many times the limit line's reach the wires should be modelled
/// before and after a station (BAFU 2007, 8.1.4.1: five times).
pub const MODELLED_LENGTH_FACTOR: f64 = 5.0;

// ---------------------------------------------------------------------------
// The closed forms.

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// The flux density at `p` of a current of 1 A flowing in the straight
/// segment from `a` to `b`, T (Biot–Savart integrated along the segment):
///
/// \\[ \vec B = \frac{\mu_0}{4\pi\rho^2}
///     \left( \frac{\vec r_a\cdot\hat u}{|\vec r_a|} - \frac{\vec r_b\cdot\hat u}{|\vec r_b|} \right)
///     \hat u \times \vec d \\]
///
/// with \\( \hat u \\) the segment's direction, \\( \vec r_a = p - a \\),
/// \\( \vec r_b = p - b \\), \\( \vec d \\) the perpendicular from the
/// segment's line to `p` and \\( \rho = |\vec d| \\) — the textbook form
/// \\( \frac{\mu_0 I}{4\pi\rho}(\cos\alpha_1 - \cos\alpha_2) \\) as a
/// vector. Zero on the segment's line (\\( \rho = 0 \\)) and for a segment
/// of no length.
pub fn segment_field(a: [f64; 3], b: [f64; 3], p: [f64; 3]) -> [f64; 3] {
    let ab = sub(b, a);
    let len = dot(ab, ab).sqrt();
    if len < 1e-12 {
        return [0.0; 3];
    }
    let u = ab.map(|x| x / len);
    let ra = sub(p, a);
    let rb = sub(p, b);
    let t = dot(ra, u);
    let d = [ra[0] - t * u[0], ra[1] - t * u[1], ra[2] - t * u[2]];
    let rho2 = dot(d, d);
    if rho2 < 1e-18 {
        return [0.0; 3];
    }
    let (la, lb) = (dot(ra, ra).sqrt(), dot(rb, rb).sqrt());
    let f = t / la - dot(rb, u) / lb;
    let k = MU0 / (4.0 * PI) * f / rho2;
    cross(u, d).map(|x| x * k)
}

/// The flux density at `(y, z)` of a current of 1 A in an infinite
/// straight conductor along \\( +x \\) through `(yk, zk)`, T, in the
/// right-handed frame (x along, y, z):
/// \\( \vec B = \frac{\mu_0}{2\pi r^2}\,\hat x \times \vec r \\),
/// \\( |\vec B| = \mu_0/(2\pi r) \\). Returns (B_y, B_z); B_x is zero.
pub fn line_field(yk: f64, zk: f64, y: f64, z: f64) -> [f64; 2] {
    let (dy, dz) = (y - yk, z - zk);
    let r2 = dy * dy + dz * dz;
    if r2 < 1e-18 {
        return [0.0; 2];
    }
    let k = MU0 / (2.0 * PI) / r2;
    [-dz * k, dy * k]
}

/// The rms of a phasor vector, \\( \sqrt{\sum_k |B_k|^2} \\), and its
/// largest value over the cycle as an rms value, the semi-major axis of
/// the ellipse \\( \Re(\vec B e^{j\omega t}) \\):
/// \\( \sqrt{(R^2 + I^2)/2 + \sqrt{((R^2 - I^2)/2)^2 + (\vec R\cdot\vec I)^2}} \\)
/// for \\( \vec B = \vec R + j\vec I \\).
pub fn rms_and_major(b: &[C]) -> (f64, f64) {
    let rr: f64 = b.iter().map(|c| c.re * c.re).sum();
    let ii: f64 = b.iter().map(|c| c.im * c.im).sum();
    let ri: f64 = b.iter().map(|c| c.re * c.im).sum();
    let rms = (rr + ii).sqrt();
    let half = 0.5 * (rr - ii);
    let major2 = 0.5 * (rr + ii) + (half * half + ri * ri).sqrt();
    (rms, major2.max(0.0).sqrt().min(rms))
}

/// A straight piece of a wire carrying a current (rms phasor, A, flowing
/// from `a` to `b`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    /// Start, m.
    pub a: [f64; 3],
    /// End, m.
    pub b: [f64; 3],
    /// Current, A (rms phasor), from `a` to `b`.
    pub current: C,
}

/// The segments of one frequency.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Group {
    /// Frequency, Hz.
    pub frequency_hz: f64,
    /// The segments.
    pub segments: Vec<Segment>,
}

/// The field of segment groups at `p`: B (rms) and its ellipse's
/// semi-major axis, T, each frequency's added as the root of the sum of
/// squares.
pub fn field_3d(groups: &[Group], p: [f64; 3]) -> (f64, f64) {
    let mut sum = (0.0, 0.0);
    for g in groups {
        let mut b = [C::new(0.0, 0.0); 3];
        for s in &g.segments {
            let f = segment_field(s.a, s.b, p);
            for k in 0..3 {
                b[k] += s.current * f[k];
            }
        }
        let (rms, major) = rms_and_major(&b);
        sum.0 += rms * rms;
        sum.1 += major * major;
    }
    (sum.0.sqrt(), sum.1.sqrt())
}

/// An infinite straight conductor of a 2D model: across (y, to the left
/// looking ahead), height (z), current along the line (rms phasor, A).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Conductor2d {
    /// Across, m (positive to the left looking ahead: a right-handed frame
    /// with x ahead and z up).
    pub y: f64,
    /// Height, m.
    pub z: f64,
    /// Current ahead, A.
    pub current: C,
}

/// The conductors of one frequency in a 2D model.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Group2d {
    /// Frequency, Hz.
    pub frequency_hz: f64,
    /// The conductors.
    pub conductors: Vec<Conductor2d>,
}

/// The field of 2D groups at `(y, z)`: B (rms) and the semi-major axis, T.
pub fn field_2d(groups: &[Group2d], y: f64, z: f64) -> (f64, f64) {
    let mut sum = (0.0, 0.0);
    for g in groups {
        let mut b = [C::new(0.0, 0.0); 2];
        for c in &g.conductors {
            let f = line_field(c.y, c.z, y, z);
            b[0] += c.current * f[0];
            b[1] += c.current * f[1];
        }
        let (rms, major) = rms_and_major(&b);
        sum.0 += rms * rms;
        sum.1 += major * major;
    }
    (sum.0.sqrt(), sum.1.sqrt())
}

// ---------------------------------------------------------------------------
// Inputs.

/// 3D (the solved wires) or 2D (infinite straight conductors).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Model {
    /// The wires as solved, finite straight pieces.
    #[default]
    ThreeD,
    /// A cross-section's wires as infinite straight parallel conductors.
    TwoD,
}

impl Model {
    /// In words.
    pub fn describe(self) -> &'static str {
        match self {
            Model::ThreeD => "3D (the wires as solved: sag, line angles, finite lengths)",
            Model::TwoD => "2D (infinite straight parallel conductors)",
        }
    }
}

/// Where the 2D model takes the wires' heights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Heights2d {
    /// At the station (the lowest point by default): the wires where they
    /// cross the profile's plane.
    #[default]
    Station,
    /// The span's mean (the line parameters' rule): higher than at the
    /// lowest point, so a lower field there.
    Mean,
}

/// The current of a circuit.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CircuitCurrent {
    /// The circuit: its name, or its system's id (`*`: every circuit).
    pub circuit: String,
    /// Current, A (rms; each phase).
    pub amps: f64,
    /// The load flows against the line's direction (from its last
    /// structure to its first): every phase's current turned by 180°.
    pub reverse: bool,
}

/// Where a profile is taken.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StationSpec {
    /// A structure of the span.
    pub from: String,
    /// The other.
    pub to: String,
    /// The fraction of the span from the first structure along the line
    /// (0–1); `None`: at the lowest point of its lowest phase conductor.
    pub fraction: Option<f64>,
}

/// A point the field is asked at.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GivenPoint {
    /// Its name (e.g. the place of sensitive use).
    pub name: String,
    /// Easting, m.
    pub x: f64,
    /// Northing, m.
    pub y: f64,
    /// Elevation, m; `None`: `height` above the ground.
    pub z: Option<f64>,
    /// Height above the ground, m (when `z` is not given).
    pub height: Option<f64>,
}

/// What to compute.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Settings {
    /// 3D or 2D.
    pub model: Model,
    /// The 2D model's wire heights.
    pub heights_2d: Heights2d,
    /// The currents per circuit.
    pub currents: Vec<CircuitCurrent>,
    /// Earth wire currents induced by the phases (else none).
    pub earth_wires: bool,
    /// The profiles' height above the ground, m.
    pub height: f64,
    /// The profiles reach this far to either side of the axis, m.
    pub width: f64,
    /// Between profile points, m.
    pub step: f64,
    /// The profiles; empty: every span at its lowest point.
    pub stations: Vec<StationSpec>,
    /// Grids on a horizontal plane at `height` over these spans (3D),
    /// `step` apart both ways, `width` to either side.
    pub grids: Vec<(String, String)>,
    /// Given points.
    pub points: Vec<GivenPoint>,
    /// A limit, µT (optional; none built in).
    pub limit_ut: Option<f64>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model: Model::ThreeD,
            heights_2d: Heights2d::Station,
            currents: Vec::new(),
            earth_wires: false,
            height: 1.0,
            width: 50.0,
            step: 1.0,
            stations: Vec::new(),
            grids: Vec::new(),
            points: Vec::new(),
            limit_ut: None,
        }
    }
}

/// Progress of a run and a way to stop it (shared with another thread).
#[derive(Debug, Default)]
pub struct Progress {
    /// Set to stop: the run returns an error at its next point.
    pub cancel: AtomicBool,
    /// Points computed.
    pub done: AtomicUsize,
    /// Points to compute.
    pub total: AtomicUsize,
}

// ---------------------------------------------------------------------------
// Results.

/// The field at one point.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldPoint {
    /// Its name (given points), else empty.
    pub name: String,
    /// Easting, northing, elevation, m.
    pub x: f64,
    /// Northing, m.
    pub y: f64,
    /// Elevation, m.
    pub z: f64,
    /// Across the span's axis, m, to the right looking ahead.
    pub offset: f64,
    /// Height above the ground, m.
    pub height: f64,
    /// B, µT (rms of the vector).
    pub b_ut: f64,
    /// The largest value over the cycle (the ellipse's semi-major axis,
    /// rms), µT.
    pub major_ut: f64,
    /// The limit less B, µT (negative: above the limit).
    pub margin_ut: Option<f64>,
}

/// Where the field is at or above the limit along a profile.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct LimitExtent {
    /// The leftmost offset at or above the limit, m (negative: left).
    pub left: f64,
    /// The rightmost, m.
    pub right: f64,
    /// The field is at or above the limit at an end of the profile: the
    /// limit line reaches further than the profile.
    pub beyond: bool,
}

/// A profile across the line.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Profile {
    /// The span's first structure along the line.
    pub from: String,
    /// Its second.
    pub to: String,
    /// The station, m from `from` along the span's axis.
    pub at: f64,
    /// The span's length, m.
    pub span_length: f64,
    /// The station along the line, m from its first structure (the spans'
    /// lengths summed).
    pub along_line: f64,
    /// The points, left to right.
    pub points: Vec<FieldPoint>,
    /// The highest B on it, µT, and its offset, m.
    pub max_ut: f64,
    /// Offset of the highest B, m.
    pub max_offset: f64,
    /// With a limit: where B is at or above it.
    pub limit: Option<LimitExtent>,
    /// With a limit reached: the farthest a point at or above it lies from
    /// the nearest conductor at the station, m (the reach for the
    /// modelled length rule; on the profile's height only).
    pub reach: Option<f64>,
    /// What to look at: the modelled length, 2D validity.
    pub warnings: Vec<String>,
}

/// A grid on a horizontal plane over a span.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldGrid {
    /// The span's first structure.
    pub from: String,
    /// Its second.
    pub to: String,
    /// Points, row by row along the span (each row left to right).
    pub points: Vec<FieldPoint>,
    /// Points along the span.
    pub rows: usize,
    /// Points across.
    pub columns: usize,
}

/// A circuit's current as used.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CurrentUsed {
    /// The circuit (name, else `system <id>`).
    pub circuit: String,
    /// Its system.
    pub system: String,
    /// A, rms per phase.
    pub amps: f64,
    /// Against the line's direction.
    pub reverse: bool,
    /// Hz.
    pub frequency_hz: f64,
    /// Where it comes from (given, thermal rating …).
    pub source: String,
}

/// An earth wire's induced current in one span.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EarthWireCurrent {
    /// Its section.
    pub section: String,
    /// The span's first structure along the line.
    pub from: String,
    /// Its second.
    pub to: String,
    /// Hz.
    pub frequency_hz: f64,
    /// A, rms, along the line.
    pub amps: f64,
    /// Its phase angle, degrees.
    pub angle_deg: f64,
}

/// The magnetic field of a line.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MagneticField {
    /// 3D or 2D.
    pub model: Model,
    /// The profiles' height above the ground, m.
    pub height: f64,
    /// The limit, µT, when given.
    pub limit_ut: Option<f64>,
    /// The currents.
    pub currents: Vec<CurrentUsed>,
    /// The earth wires' currents (with earth wires on).
    pub earth_wires: Vec<EarthWireCurrent>,
    /// The profiles.
    pub profiles: Vec<Profile>,
    /// The grids.
    pub grids: Vec<FieldGrid>,
    /// The given points.
    pub points: Vec<FieldPoint>,
    /// What was assumed or left out.
    pub notes: Vec<String>,
}

// ---------------------------------------------------------------------------
// Currents.

/// The circuits of the line (phases of one system in one named circuit,
/// or in none), in the order of their first section.
pub fn circuits(line: &ohl_line::Line) -> Vec<CircuitKey> {
    let Ok(phases) = line.section_phases() else {
        return Vec::new();
    };
    let mut out: Vec<CircuitKey> = Vec::new();
    for s in &line.sections {
        if let Some(Some((ks, _))) = phases.get(&s.id) {
            let key = CircuitKey {
                circuit: s.circuit_name.clone(),
                system: line.systems[*ks].id.clone(),
            };
            if !out.contains(&key) {
                out.push(key);
            }
        }
    }
    out
}

fn matches(c: &CircuitKey, name: &str) -> bool {
    name == "*" || c.circuit == name || c.system == name || c.label() == name
}

/// Each circuit's thermal rating, A: the least over its sections of the
/// bundle's subconductors times one subconductor's ampacity
/// ([`ohl_thermal::ampacity`]) at `max_temperature_c` in `ambient` — the
/// weakest section governs. The ambient case and the temperature are
/// inputs (an ordinance's conditions come from its parameter file, none is
/// built in). Returns the currents (forward) and notes.
pub fn thermal_currents(
    line: &ohl_line::Line,
    library: &ohl_cables::Library,
    method: ohl_thermal::Method,
    ambient: &ohl_thermal::Ambient,
    max_temperature_c: f64,
    defaults: &ohl_line::Defaults,
) -> Result<Vec<CircuitCurrent>, String> {
    let phases = line.section_phases().map_err(|e| e.to_string())?;
    let mut least: BTreeMap<String, (f64, String)> = BTreeMap::new();
    let mut order = Vec::new();
    for s in &line.sections {
        let Some(Some((ks, _))) = phases.get(&s.id) else {
            continue;
        };
        let key = CircuitKey {
            circuit: s.circuit_name.clone(),
            system: line.systems[*ks].id.clone(),
        };
        let label = if key.circuit.is_empty() {
            key.system.clone()
        } else {
            key.circuit.clone()
        };
        let cable = library
            .get(&s.cable)
            .ok_or_else(|| format!("section {}: wire type {} not in the library", s.id, s.cable))?;
        let conductor = ohl_thermal::Conductor::from_cable(cable)
            .map_err(|e| format!("section {}: {e}", s.id))?
            .with_defaults(defaults);
        let b = ohl_thermal::ampacity(method, &conductor, ambient, max_temperature_c)
            .map_err(|e| format!("section {}: {e}", s.id))?;
        let amps = b.current * f64::from(s.bundle.max(1));
        if !order.contains(&label) {
            order.push(label.clone());
        }
        let e = least.entry(label).or_insert((f64::INFINITY, String::new()));
        if amps < e.0 {
            *e = (amps, s.id.clone());
        }
    }
    Ok(order
        .into_iter()
        .map(|label| CircuitCurrent {
            amps: least[&label].0,
            circuit: label,
            reverse: false,
        })
        .collect())
}

/// The current of each circuit (by its key) as a phasor factor: amps and
/// the direction's sign.
struct Currents {
    by_key: BTreeMap<CircuitKey, (f64, f64)>,
}

impl Currents {
    fn new(
        line: &ohl_line::Line,
        given: &[CircuitCurrent],
        notes: &mut Vec<String>,
    ) -> Result<Self, String> {
        let all = circuits(line);
        let mut by_key = BTreeMap::new();
        for g in given {
            if !(g.amps.is_finite() && g.amps >= 0.0) {
                return Err(format!("circuit {}: a current of {} A", g.circuit, g.amps));
            }
            let hit: Vec<&CircuitKey> = all.iter().filter(|c| matches(c, &g.circuit)).collect();
            if hit.is_empty() {
                return Err(format!(
                    "no circuit {} (circuits: {})",
                    g.circuit,
                    all.iter()
                        .map(CircuitKey::label)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            for c in hit {
                by_key.insert(c.clone(), (g.amps, if g.reverse { -1.0 } else { 1.0 }));
            }
        }
        for c in &all {
            if !by_key.contains_key(c) {
                notes.push(format!(
                    "circuit {}: no current given, 0 A taken",
                    c.label()
                ));
            }
        }
        Ok(Self { by_key })
    }

    /// A phase's current along the line, A (rms phasor), and its
    /// frequency; `None`: DC or no current.
    fn of(&self, p: &PhaseData) -> Option<(f64, C)> {
        if p.kind == SystemKind::Dc {
            return None;
        }
        let key = CircuitKey {
            circuit: p.circuit.clone(),
            system: p.system.clone(),
        };
        let (amps, sign) = self.by_key.get(&key).copied()?;
        if amps == 0.0 {
            return None;
        }
        let f = p.frequency_hz.unwrap_or(DEFAULT_FREQUENCY_HZ);
        Some((f, C::from_polar(sign * amps, p.angle_deg.to_radians())))
    }
}

/// Earth wire currents per (section id, span index in its section): per
/// frequency, the current along the line (rms phasor, A).
type EarthWireMap = BTreeMap<(String, usize), Vec<(f64, C)>>;

/// The earth wires' induced currents in a span from its (mean)
/// cross-section, per frequency: \\( I_e = -Z_{ee}^{-1} Z_{ep} I_p \\)
/// with the primitive matrices of [`matrices::impedance`] (an infinitely
/// long earth wire earthed everywhere, its voltage along it zero).
fn earth_wire_currents(
    cs: &CrossSection,
    currents: &Currents,
    method: ohl_line::electrical::EarthReturn,
    notes: &mut Vec<String>,
) -> EarthWireMap {
    let mut out = EarthWireMap::new();
    let earthed: Vec<&cross_section::CrossWire> =
        cs.wires.iter().filter(|w| w.phase.is_none()).collect();
    if earthed.is_empty() {
        return out;
    }
    let mut ew = Vec::new();
    for w in &earthed {
        let mut quiet = Vec::new();
        match crate::parameters::wire_of(w, &mut quiet) {
            Some(x) => ew.push(x),
            None => {
                notes.push(format!(
                    "span {}–{}: earth wire {} has no resistance or lies on the ground; no earth wire currents in this span",
                    cs.station.from, cs.station.to, w.section
                ));
                return out;
            }
        }
    }
    let mut phases: Vec<(f64, C, Wire)> = Vec::new();
    for w in &cs.wires {
        let Some(p) = &w.phase else { continue };
        let Some((f, i)) = currents.of(p) else {
            continue;
        };
        phases.push((
            f,
            i,
            Wire {
                across: w.offset,
                height: w.height,
                gmr: 0.5 * w.wire.diameter,
                radius: 0.5 * w.wire.diameter,
                resistance: 0.0,
            },
        ));
    }
    let mut freqs: Vec<f64> = phases.iter().map(|p| p.0).collect();
    freqs.sort_by(f64::total_cmp);
    freqs.dedup();
    let ne = ew.len();
    for f in freqs {
        let ph: Vec<&(f64, C, Wire)> = phases.iter().filter(|p| p.0 == f).collect();
        let mut wires = ew.clone();
        wires.extend(ph.iter().map(|p| p.2));
        let z = matrices::impedance(&wires, method, f, cs.ground.soil_resistivity_ohm_m);
        let zee = DMatrix::from_fn(ne, ne, |i, j| z[(i, j)]);
        let zep = DMatrix::from_fn(ne, ph.len(), |i, j| z[(i, ne + j)]);
        let ip = DVector::from_iterator(ph.len(), ph.iter().map(|p| p.1));
        let Some(inv) = zee.try_inverse() else {
            notes.push(format!(
                "span {}–{}: the earth wires' matrix is singular; no earth wire currents",
                cs.station.from, cs.station.to
            ));
            continue;
        };
        let ie = -(inv * zep * ip);
        for (k, w) in earthed.iter().enumerate() {
            out.entry((w.section.clone(), w.span))
                .or_default()
                .push((f, ie[k]));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The run.

/// The line as segments, by frequency, and what was found on the way.
struct Model3d {
    groups: Vec<Group>,
}

/// The sign of section `k`'s span `j` along the line: +1 when its points
/// run from the line span's first structure to its second.
fn span_signs(line: &ohl_line::Line, spans: &[LineSpan]) -> BTreeMap<(usize, usize), f64> {
    let mut out = BTreeMap::new();
    for s in spans {
        for &(k, j) in &s.wires {
            let first = &line.sections[k].points[j].structure;
            out.insert((k, j), if *first == s.from { 1.0 } else { -1.0 });
        }
    }
    out
}

fn build_3d(
    src: &Sources<'_>,
    currents: &Currents,
    signs: &BTreeMap<(usize, usize), f64>,
    earth: &EarthWireMap,
    with_earth_wires: bool,
) -> Result<Model3d, String> {
    let line = src.line;
    let phases = line.section_phases().map_err(|e| e.to_string())?;
    let mut groups: Vec<Group> = Vec::new();
    let mut push = |f: f64, seg: Segment| match groups.iter_mut().find(|g| g.frequency_hz == f) {
        Some(g) => g.segments.push(seg),
        None => groups.push(Group {
            frequency_hz: f,
            segments: vec![seg],
        }),
    };
    for (k, section) in line.sections.iter().enumerate() {
        let Some(shape) = src.shapes.iter().find(|s| s.section == section.id) else {
            continue;
        };
        let phase = phases
            .get(&section.id)
            .copied()
            .flatten()
            .map(|at| cross_section::phase_data(line, section, at));
        for (j, sp) in shape.spans.iter().enumerate() {
            if sp.points.len() < 2 {
                continue;
            }
            let sign = signs.get(&(k, j)).copied().unwrap_or(1.0);
            let flows: Vec<(f64, C)> = match &phase {
                Some(p) => currents.of(p).into_iter().collect(),
                None if with_earth_wires => earth
                    .get(&(section.id.clone(), j))
                    .cloned()
                    .unwrap_or_default(),
                None => Vec::new(),
            };
            if flows.is_empty() {
                continue;
            }
            let subs = section.subconductor_polylines(&sp.points);
            let n = subs.len().max(1) as f64;
            for (f, i) in flows {
                let i = i * (sign / n);
                for poly in &subs {
                    for w in poly.windows(2) {
                        push(
                            f,
                            Segment {
                                a: w[0],
                                b: w[1],
                                current: i,
                            },
                        );
                    }
                }
            }
        }
    }
    Ok(Model3d { groups })
}

/// The 2D groups of a cross-section: each wire's subconductors as
/// infinite conductors along the span's axis.
fn build_2d(
    line: &ohl_line::Line,
    cs: &CrossSection,
    currents: &Currents,
    earth: &EarthWireMap,
    with_earth_wires: bool,
) -> Vec<Group2d> {
    let mut groups: Vec<Group2d> = Vec::new();
    for w in &cs.wires {
        let flows: Vec<(f64, C)> = match &w.phase {
            Some(p) => currents.of(p).into_iter().collect(),
            None if with_earth_wires => earth
                .get(&(w.section.clone(), w.span))
                .cloned()
                .unwrap_or_default(),
            None => Vec::new(),
        };
        let offsets = line
            .sections
            .iter()
            .find(|s| s.id == w.section)
            .map(|s| s.bundle_offsets())
            .unwrap_or_else(|| vec![[0.0, 0.0]]);
        let n = offsets.len().max(1) as f64;
        for (f, i) in flows {
            let g = match groups.iter_mut().position(|g| g.frequency_hz == f) {
                Some(k) => k,
                None => {
                    groups.push(Group2d {
                        frequency_hz: f,
                        conductors: Vec::new(),
                    });
                    groups.len() - 1
                }
            };
            for o in &offsets {
                groups[g].conductors.push(Conductor2d {
                    // Left is positive in the 2D frame.
                    y: -(w.offset + o[0]),
                    z: w.elevation - cs.ground.elevation + o[1],
                    current: i / n,
                });
            }
        }
    }
    groups
}

/// The lowest point of a span's lowest phase conductor along its axis, m
/// (the span's middle when it has no phase conductor).
fn lowest_station(src: &Sources<'_>, span: &LineSpan, axis: &Axis) -> f64 {
    let line = src.line;
    let phases = line.section_phases().unwrap_or_default();
    let mut best: Option<(f64, f64)> = None;
    for &(k, j) in &span.wires {
        let section = &line.sections[k];
        if !matches!(phases.get(&section.id), Some(Some(_))) {
            continue;
        }
        let Some(sp) = src
            .shapes
            .iter()
            .find(|s| s.section == section.id)
            .and_then(|s| s.spans.get(j))
        else {
            continue;
        };
        let p = sp.low_point;
        let (t, s) = axis.local(&p);
        let h = p[2] - axis.ground(src.ground, t, s);
        if best.is_none_or(|b| h < b.0) {
            best = Some((h, t));
        }
    }
    best.map_or(0.5 * axis.length, |b| b.1.clamp(0.0, axis.length))
}

/// Line angles at the structures between consecutive spans, degrees.
fn line_angles(spans: &[LineSpan], axes: &[Option<Axis>]) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    for i in 1..spans.len() {
        if spans[i - 1].to != spans[i].from {
            continue;
        }
        let (Some(a), Some(b)) = (&axes[i - 1], &axes[i]) else {
            continue;
        };
        let c = (a.u[0] * b.u[0] + a.u[1] * b.u[1]).clamp(-1.0, 1.0);
        out.insert(spans[i].from.clone(), c.acos().to_degrees());
    }
    out
}

/// Whether the wires of a span sit differently relative to each other at
/// its two structures: the largest change, m, of a wire's place (across
/// the axis and up) relative to the wires' centre.
fn arrangement_change(src: &Sources<'_>, span: &LineSpan, axis: &Axis) -> f64 {
    let line = src.line;
    let mut ends: Vec<((f64, f64), (f64, f64))> = Vec::new();
    for &(k, j) in &span.wires {
        let section = &line.sections[k];
        let Some(sp) = src
            .shapes
            .iter()
            .find(|s| s.section == section.id)
            .and_then(|s| s.spans.get(j))
            .filter(|s| s.points.len() >= 2)
        else {
            continue;
        };
        let (mut a, mut b) = (sp.points[0], sp.points[sp.points.len() - 1]);
        if axis.local(&a).0 > axis.local(&b).0 {
            std::mem::swap(&mut a, &mut b);
        }
        ends.push((
            (axis.local(&a).1, a[2] - axis.a[2]),
            (axis.local(&b).1, b[2] - axis.b[2]),
        ));
    }
    if ends.is_empty() {
        return 0.0;
    }
    let n = ends.len() as f64;
    let ca = ends
        .iter()
        .fold((0.0, 0.0), |s, e| (s.0 + e.0.0 / n, s.1 + e.0.1 / n));
    let cb = ends
        .iter()
        .fold((0.0, 0.0), |s, e| (s.0 + e.1.0 / n, s.1 + e.1.1 / n));
    ends.iter()
        .map(|(a, b)| ((a.0 - ca.0) - (b.0 - cb.0)).hypot((a.1 - ca.1) - (b.1 - cb.1)))
        .fold(0.0, f64::max)
}

fn check_cancel(progress: Option<&Progress>) -> Result<(), String> {
    match progress {
        Some(p) if p.cancel.load(Ordering::Relaxed) => Err("stopped".into()),
        _ => Ok(()),
    }
}

/// Values at many points on all cores (in order).
fn compute_all(
    points: &[[f64; 3]],
    f: &(dyn Fn([f64; 3]) -> (f64, f64) + Sync),
    progress: Option<&Progress>,
) -> Result<Vec<(f64, f64)>, String> {
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .clamp(1, 16);
    let chunk = points.len().div_ceil(threads).max(1);
    let mut out = vec![(0.0, 0.0); points.len()];
    let stopped = AtomicBool::new(false);
    std::thread::scope(|scope| {
        for (ps, os) in points.chunks(chunk).zip(out.chunks_mut(chunk)) {
            let stopped = &stopped;
            scope.spawn(move || {
                for (p, o) in ps.iter().zip(os.iter_mut()) {
                    if check_cancel(progress).is_err() {
                        stopped.store(true, Ordering::Relaxed);
                        return;
                    }
                    *o = f(*p);
                    if let Some(pr) = progress {
                        pr.done.fetch_add(1, Ordering::Relaxed);
                    }
                }
            });
        }
    });
    if stopped.load(Ordering::Relaxed) {
        return Err("stopped".into());
    }
    Ok(out)
}

/// The magnetic field of the line solved in one weather case (`src`), as
/// `settings` ask. `progress`: to follow and stop it from another thread.
pub fn run(
    src: &Sources<'_>,
    settings: &Settings,
    progress: Option<&Progress>,
) -> Result<MagneticField, String> {
    let line = src.line;
    if !(settings.step > 0.0 && settings.width >= 0.0 && settings.height.is_finite()) {
        return Err("a step above 0 m and a width of at least 0 m".into());
    }
    if let Some(l) = settings.limit_ut
        && !(l.is_finite() && l > 0.0)
    {
        return Err(format!("a limit above 0 µT, not {l}"));
    }
    let mut notes = Vec::new();
    let currents = Currents::new(line, &settings.currents, &mut notes)?;
    let spans = line_spans(line);
    if spans.is_empty() {
        return Err("the line has no spans".into());
    }
    let axes: Vec<Option<Axis>> = spans.iter().map(|s| Axis::of(line, s).ok()).collect();
    let signs = span_signs(line, &spans);
    // Distance along the line to each span's start.
    let mut along = Vec::with_capacity(spans.len());
    let mut total = 0.0;
    for a in &axes {
        along.push(total);
        total += a.as_ref().map_or(0.0, |a| a.length);
    }
    // Earth wire currents, per span from its mean cross-section.
    let (means, mean_notes) = cross_section::span_cross_sections(src, line.electrical.mean_height);
    notes.extend(mean_notes);
    let mut earth = EarthWireMap::new();
    let mut earth_rows = Vec::new();
    if settings.earth_wires {
        for (span, cs) in &means {
            let m = earth_wire_currents(cs, &currents, line.electrical.earth_return, &mut notes);
            for ((section, _), list) in &m {
                for (f, i) in list {
                    earth_rows.push(EarthWireCurrent {
                        section: section.clone(),
                        from: span.from.clone(),
                        to: span.to.clone(),
                        frequency_hz: *f,
                        amps: i.norm(),
                        angle_deg: if i.norm() > 1e-9 {
                            i.arg().to_degrees()
                        } else {
                            0.0
                        },
                    });
                }
            }
            earth.extend(m);
        }
        notes.push(
            "earth wire currents: induced by the phases on an infinitely long earth wire earthed everywhere (I_e = −inv(Z_ee) · Z_ep · I_p per span, from its mean cross-section) — an approximation: the towers' earthing and the earth wires' ends are not modelled".into(),
        );
    } else if line.sections.iter().any(|s| s.system.is_none()) {
        notes.push("earth wires carry no current (earth wire currents off)".into());
    }
    if line.systems.iter().any(|s| s.kind == SystemKind::Dc) {
        notes.push("DC systems left out (a static field; not computed)".into());
    }
    if line
        .systems
        .iter()
        .any(|s| s.kind == SystemKind::Ac && s.frequency_hz.is_none())
    {
        notes.push(format!(
            "a system gives no frequency: {DEFAULT_FREQUENCY_HZ} Hz taken"
        ));
    }
    notes.push(
        "the ground's return currents left out (their image lies at the complex depth, hundreds of metres down at power frequency); jumpers left out; balanced phases, no harmonics".into(),
    );
    let model3d = match settings.model {
        Model::ThreeD => Some(build_3d(
            src,
            &currents,
            &signs,
            &earth,
            settings.earth_wires,
        )?),
        Model::TwoD => None,
    };
    let angles = line_angles(&spans, &axes);
    // The stations.
    let mut stations: Vec<(usize, f64)> = Vec::new();
    if settings.stations.is_empty() {
        for (i, s) in spans.iter().enumerate() {
            if let Some(a) = &axes[i] {
                stations.push((i, lowest_station(src, s, a)));
            }
        }
    } else {
        for st in &settings.stations {
            let i = spans
                .iter()
                .position(|s| {
                    (s.from == st.from && s.to == st.to) || (s.from == st.to && s.to == st.from)
                })
                .ok_or_else(|| format!("no span between structures {} and {}", st.from, st.to))?;
            let a = axes[i]
                .as_ref()
                .ok_or_else(|| format!("span {}–{}: no axis", st.from, st.to))?;
            let t = match st.fraction {
                Some(f) if (0.0..=1.0).contains(&f) => f * a.length,
                Some(f) => return Err(format!("a place in the span from 0 to 1, not {f}")),
                None => lowest_station(src, &spans[i], a),
            };
            stations.push((i, t));
        }
    }
    let n_across = (2.0 * settings.width / settings.step).round() as usize + 1;
    let offsets: Vec<f64> = (0..n_across)
        .map(|k| -settings.width + k as f64 * settings.step)
        .collect();
    let mut grid_specs = Vec::new();
    for (from, to) in &settings.grids {
        let i = spans
            .iter()
            .position(|s| (&s.from == from && &s.to == to) || (&s.from == to && &s.to == from))
            .ok_or_else(|| format!("grid: no span between structures {from} and {to}"))?;
        grid_specs.push(i);
    }
    if let Some(p) = progress {
        let grid_points: usize = grid_specs
            .iter()
            .filter_map(|&i| axes[i].as_ref())
            .map(|a| ((a.length / settings.step).floor() as usize + 1) * n_across)
            .sum();
        p.total.store(
            stations.len() * n_across + settings.points.len() + grid_points,
            Ordering::Relaxed,
        );
        p.done.store(0, Ordering::Relaxed);
    }
    let limit = settings.limit_ut;
    let mut profiles = Vec::new();
    for &(i, t) in &stations {
        check_cancel(progress)?;
        let span = &spans[i];
        let axis = axes[i].as_ref().expect("an axis");
        let mut warnings = Vec::new();
        let station_cs =
            match cross_section::cross_section(src, span, StationAt::Fraction(t / axis.length)) {
                Ok(c) => c,
                Err(e) if settings.model == Model::ThreeD => {
                    warnings.push(format!("no cross-section at the station ({e}): no reach"));
                    CrossSection {
                        station: cross_section::Station {
                            from: span.from.clone(),
                            to: span.to.clone(),
                            span_length: axis.length,
                            at: StationAt::Fraction(t / axis.length),
                        },
                        ground: cross_section::Ground {
                            elevation: axis.ground(src.ground, t, 0.0),
                            slope_across: 0.0,
                            soil_resistivity_ohm_m: 0.0,
                            soil_resistivity_assumed: false,
                        },
                        wires: Vec::new(),
                        other_lines: Vec::new(),
                        notes: Vec::new(),
                    }
                }
                Err(e) => return Err(e),
            };
        let places: Vec<([f64; 3], f64)> = offsets
            .iter()
            .map(|&s| {
                let p = axis.plan(t, s);
                let g = axis.ground(src.ground, t, s);
                ([p[0], p[1], g + settings.height], g)
            })
            .collect();
        let values = match &model3d {
            Some(m) => {
                let pts: Vec<[f64; 3]> = places.iter().map(|p| p.0).collect();
                compute_all(&pts, &|p| field_3d(&m.groups, p), progress)?
            }
            None => {
                let cs2 = match settings.heights_2d {
                    Heights2d::Station => station_cs.clone(),
                    Heights2d::Mean => means
                        .iter()
                        .find(|(s, _)| s == span)
                        .map(|(_, c)| c.clone())
                        .ok_or_else(|| {
                            format!("span {}–{}: no mean cross-section", span.from, span.to)
                        })?,
                };
                let groups = build_2d(line, &cs2, &currents, &earth, settings.earth_wires);
                let g0 = station_cs.ground.elevation;
                let out = offsets
                    .iter()
                    .zip(&places)
                    .map(|(&s, p)| field_2d(&groups, -s, p.0[2] - g0))
                    .collect();
                if let Some(pr) = progress {
                    pr.done.fetch_add(offsets.len(), Ordering::Relaxed);
                }
                out
            }
        };
        let points: Vec<FieldPoint> = offsets
            .iter()
            .zip(&places)
            .zip(&values)
            .map(|((&s, (p, g)), &(b, m))| FieldPoint {
                name: String::new(),
                x: p[0],
                y: p[1],
                z: p[2],
                offset: s,
                height: p[2] - g,
                b_ut: b * 1e6,
                major_ut: m * 1e6,
                margin_ut: limit.map(|l| l - b * 1e6),
            })
            .collect();
        let (max_offset, max_ut) =
            points
                .iter()
                .map(|p| (p.offset, p.b_ut))
                .fold(
                    (0.0, f64::NEG_INFINITY),
                    |a, b| if b.1 > a.1 { b } else { a },
                );
        let mut extent = None;
        let mut reach = None;
        if let Some(l) = limit {
            let above: Vec<&FieldPoint> = points.iter().filter(|p| p.b_ut >= l).collect();
            if let (Some(first), Some(last)) = (above.first(), above.last()) {
                let beyond = points.first().is_some_and(|p| p.b_ut >= l)
                    || points.last().is_some_and(|p| p.b_ut >= l);
                extent = Some(LimitExtent {
                    left: first.offset,
                    right: last.offset,
                    beyond,
                });
                if beyond {
                    warnings.push(format!(
                        "the field is at or above the limit at an end of the profile: the limit line reaches beyond ±{} m",
                        settings.width
                    ));
                }
                let wires: Vec<(f64, f64)> = station_cs
                    .wires
                    .iter()
                    .map(|w| (w.offset, w.elevation))
                    .collect();
                reach = above
                    .iter()
                    .map(|p| {
                        wires
                            .iter()
                            .map(|w| (p.offset - w.0).hypot(p.z - w.1))
                            .fold(f64::INFINITY, f64::min)
                    })
                    .fold(None, |a: Option<f64>, d| Some(a.map_or(d, |x| x.max(d))));
            }
        }
        // The modelled length (3D): five times the reach before and after.
        let before = along[i] + t;
        let after = total - before;
        let need = MODELLED_LENGTH_FACTOR * reach.unwrap_or(settings.width);
        if settings.model == Model::ThreeD && before.min(after) < need {
            warnings.push(format!(
                "the line is modelled {before:.0} m before and {after:.0} m after the station, less than {MODELLED_LENGTH_FACTOR} × {} = {need:.0} m (the line's end is near: the field there is the line's own, without what continues beyond it)",
                match reach {
                    Some(r) => format!("the limit line's reach {r:.1} m"),
                    None => format!("the profile's half width {} m", settings.width),
                }
            ));
        }
        if settings.model == Model::TwoD {
            let at = axis.plan(t, 0.0);
            let within = reach.unwrap_or(settings.width).max(settings.width);
            for (id, angle) in &angles {
                if *angle <= LINE_ANGLE_2D_DEG {
                    continue;
                }
                let Some(s) = line.structure(id) else {
                    continue;
                };
                let d = (s.position[0] - at[0]).hypot(s.position[1] - at[1]);
                if d <= within {
                    warnings.push(format!(
                        "2D not valid here: line angle of {angle:.1}° at structure {id}, {d:.0} m from the station (within {within:.0} m; more than {LINE_ANGLE_2D_DEG}°) — use 3D"
                    ));
                }
            }
            let change = arrangement_change(src, span, axis);
            if change > ARRANGEMENT_TOLERANCE_M {
                warnings.push(format!(
                    "2D not valid here: the wires are arranged differently at {} and {} (a wire's place relative to the others changes by {change:.2} m) — use 3D",
                    span.from, span.to
                ));
            }
        }
        profiles.push(Profile {
            from: span.from.clone(),
            to: span.to.clone(),
            at: t,
            span_length: axis.length,
            along_line: before,
            points,
            max_ut,
            max_offset,
            limit: extent,
            reach,
            warnings,
        });
    }
    if settings.model == Model::TwoD {
        notes.push(
            "2D: crossings with other lines are not checked (other lines are not modelled yet)"
                .into(),
        );
    }
    // Grids (3D; in 2D the profile repeated along the span).
    let mut grids = Vec::new();
    for &i in &grid_specs {
        check_cancel(progress)?;
        let span = &spans[i];
        let Some(axis) = axes[i].as_ref() else {
            continue;
        };
        let rows = (axis.length / settings.step).floor() as usize + 1;
        let mut places = Vec::new();
        for r in 0..rows {
            let t = r as f64 * settings.step;
            for &s in &offsets {
                let p = axis.plan(t, s);
                let g = axis.ground(src.ground, t, s);
                places.push(([p[0], p[1], g + settings.height], s, g, t));
            }
        }
        let values = match &model3d {
            Some(m) => {
                let pts: Vec<[f64; 3]> = places.iter().map(|p| p.0).collect();
                compute_all(&pts, &|p| field_3d(&m.groups, p), progress)?
            }
            None => {
                let mut out = Vec::new();
                for r in 0..rows {
                    let t = r as f64 * settings.step;
                    let cs = cross_section::cross_section(
                        src,
                        span,
                        StationAt::Fraction((t / axis.length).min(1.0)),
                    )?;
                    let groups = build_2d(line, &cs, &currents, &earth, settings.earth_wires);
                    for k in 0..offsets.len() {
                        let p = &places[r * offsets.len() + k];
                        out.push(field_2d(&groups, -p.1, p.0[2] - cs.ground.elevation));
                    }
                }
                out
            }
        };
        grids.push(FieldGrid {
            from: span.from.clone(),
            to: span.to.clone(),
            rows,
            columns: offsets.len(),
            points: places
                .iter()
                .zip(&values)
                .map(|(p, &(b, m))| FieldPoint {
                    name: String::new(),
                    x: p.0[0],
                    y: p.0[1],
                    z: p.0[2],
                    offset: p.1,
                    height: p.0[2] - p.2,
                    b_ut: b * 1e6,
                    major_ut: m * 1e6,
                    margin_ut: limit.map(|l| l - b * 1e6),
                })
                .collect(),
        });
    }
    // Given points.
    let mut points = Vec::new();
    if !settings.points.is_empty() {
        let mut located = Vec::new();
        for gp in &settings.points {
            // The span it lies beside: its foot on the axis within the
            // span, the nearest such; else the nearest structure's span.
            let mut best: Option<(f64, usize, f64, f64)> = None;
            for (i, a) in axes.iter().enumerate() {
                let Some(a) = a else { continue };
                let (t, s) = a.local(&[gp.x, gp.y, 0.0]);
                let tc = t.clamp(0.0, a.length);
                let d = (t - tc).hypot(s);
                if best.is_none_or(|b| d < b.0) {
                    best = Some((d, i, tc, s));
                }
            }
            let (_, i, t, s) = best.ok_or("no span to place a point beside")?;
            let axis = axes[i].as_ref().expect("an axis");
            let g = axis.ground(src.ground, t, s);
            let z = match (gp.z, gp.height) {
                (Some(z), _) => z,
                (None, Some(h)) => g + h,
                (None, None) => g + settings.height,
            };
            located.push((gp, i, t, s, g, z));
        }
        let values = match &model3d {
            Some(m) => {
                let pts: Vec<[f64; 3]> = located.iter().map(|l| [l.0.x, l.0.y, l.5]).collect();
                compute_all(&pts, &|p| field_3d(&m.groups, p), progress)?
            }
            None => {
                let mut out = Vec::new();
                for &(_, i, t, s, _, z) in &located {
                    let axis = axes[i].as_ref().expect("an axis");
                    let cs = cross_section::cross_section(
                        src,
                        &spans[i],
                        StationAt::Fraction(t / axis.length),
                    )?;
                    let groups = build_2d(line, &cs, &currents, &earth, settings.earth_wires);
                    out.push(field_2d(&groups, -s, z - cs.ground.elevation));
                }
                out
            }
        };
        for (l, (b, m)) in located.iter().zip(values) {
            points.push(FieldPoint {
                name: l.0.name.clone(),
                x: l.0.x,
                y: l.0.y,
                z: l.5,
                offset: l.3,
                height: l.5 - l.4,
                b_ut: b * 1e6,
                major_ut: m * 1e6,
                margin_ut: limit.map(|x| x - b * 1e6),
            });
        }
    }
    let used = circuits(line)
        .into_iter()
        .map(|c| {
            let (amps, sign) = currents.by_key.get(&c).copied().unwrap_or((0.0, 1.0));
            let sys = line.systems.iter().find(|s| s.id == c.system);
            CurrentUsed {
                circuit: c.label(),
                system: c.system.clone(),
                amps,
                reverse: sign < 0.0,
                frequency_hz: sys
                    .and_then(|s| s.frequency_hz)
                    .unwrap_or(DEFAULT_FREQUENCY_HZ),
                source: String::new(),
            }
        })
        .collect();
    if let Some(p) = progress {
        p.done
            .store(p.total.load(Ordering::Relaxed), Ordering::Relaxed);
    }
    Ok(MagneticField {
        model: settings.model,
        height: settings.height,
        limit_ut: limit,
        currents: used,
        earth_wires: earth_rows,
        profiles,
        grids,
        points,
        notes,
    })
}

// ---------------------------------------------------------------------------
// Points in, CSV out.

/// Points from CSV text: a header naming the columns `x`, `y` and `z`
/// (elevation) or `height` (above the ground), optionally `name`; comma or
/// semicolon separated; `#` lines are comments.
pub fn parse_points(text: &str) -> Result<Vec<GivenPoint>, String> {
    let mut lines = text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty() && !l.trim_start().starts_with('#'));
    let (_, header) = lines.next().ok_or("no header line (x, y, z or height)")?;
    let sep = if header.contains(';') { ';' } else { ',' };
    let cols: Vec<String> = header.split(sep).map(|c| c.trim().to_lowercase()).collect();
    let col = |n: &str| cols.iter().position(|c| c == n);
    let (Some(cx), Some(cy)) = (col("x"), col("y")) else {
        return Err("the header needs columns x and y".into());
    };
    let (cz, ch, cn) = (col("z"), col("height"), col("name"));
    if cz.is_none() && ch.is_none() {
        return Err("the header needs a column z (elevation) or height (above the ground)".into());
    }
    let mut out = Vec::new();
    for (n, l) in lines {
        let f: Vec<&str> = l.split(sep).map(str::trim).collect();
        let num = |c: Option<usize>| -> Result<Option<f64>, String> {
            match c.and_then(|c| f.get(c)).filter(|v| !v.is_empty()) {
                None => Ok(None),
                Some(v) => v
                    .parse::<f64>()
                    .map(Some)
                    .map_err(|_| format!("line {}: \"{v}\" is not a number", n + 1)),
            }
        };
        let x = num(Some(cx))?.ok_or_else(|| format!("line {}: no x", n + 1))?;
        let y = num(Some(cy))?.ok_or_else(|| format!("line {}: no y", n + 1))?;
        let (z, height) = (num(cz)?, num(ch)?);
        if z.is_none() && height.is_none() {
            return Err(format!("line {}: neither z nor height", n + 1));
        }
        out.push(GivenPoint {
            name: cn
                .and_then(|c| f.get(c))
                .map_or_else(|| format!("P{}", out.len() + 1), |s| (*s).to_owned()),
            x,
            y,
            z,
            height,
        });
    }
    Ok(out)
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_owned()
    }
}

/// The results as CSV: one row per profile point, grid point and given
/// point.
pub fn csv(m: &MagneticField) -> String {
    let mut out = String::from(
        "kind,from,to,station_m,name,x,y,z,offset_m,height_m,b_ut,b_major_ut,limit_ut,margin_ut\n",
    );
    let num = |x: Option<f64>| x.map_or(String::new(), |v| format!("{v:.4}"));
    let row =
        |out: &mut String, kind: &str, from: &str, to: &str, at: Option<f64>, p: &FieldPoint| {
            let _ = writeln!(
                out,
                "{kind},{},{},{},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.5},{:.5},{},{}",
                csv_field(from),
                csv_field(to),
                num(at),
                csv_field(&p.name),
                p.x,
                p.y,
                p.z,
                p.offset,
                p.height,
                p.b_ut,
                p.major_ut,
                num(m.limit_ut),
                num(p.margin_ut),
            );
        };
    for pr in &m.profiles {
        for p in &pr.points {
            row(&mut out, "profile", &pr.from, &pr.to, Some(pr.at), p);
        }
    }
    for g in &m.grids {
        for p in &g.points {
            row(&mut out, "grid", &g.from, &g.to, None, p);
        }
    }
    for p in &m.points {
        row(&mut out, "point", "", "", None, p);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A long segment's field is the infinite wire's \\( \mu_0 I/(2\pi d) \\)
    /// (2·10⁻⁷ T at 1 m for 1 A), pointing by the right-hand rule.
    #[test]
    fn long_segment_is_the_infinite_wire() {
        let b = segment_field([0.0, 0.0, -1e6], [0.0, 0.0, 1e6], [2.0, 0.0, 0.0]);
        assert!((b[1] - 1e-7).abs() < 1e-17, "{b:?}");
        assert!(b[0].abs() < 1e-20 && b[2].abs() < 1e-20);
        let l = line_field(0.0, 0.0, 0.0, 2.0);
        // Current along +x, point up (+z): field along −y.
        assert!((l[0] + 1e-7).abs() < 1e-17 && l[1].abs() < 1e-20);
    }

    /// A finite segment seen from its perpendicular bisector at distance
    /// d: \\( \frac{\mu_0}{4\pi d}\cdot\frac{2(L/2)}{\sqrt{(L/2)^2 + d^2}} \\)
    /// (by hand); split in two halves, the same.
    #[test]
    fn finite_segment_by_hand_and_split() {
        let (l, d) = (10.0, 3.0);
        let b = segment_field([-5.0, 0.0, 0.0], [5.0, 0.0, 0.0], [0.0, d, 0.0]);
        let hand = 1e-7 / d * l / (25.0f64 + d * d).sqrt();
        assert!((b[2] - hand).abs() < 1e-18, "{} {hand}", b[2]);
        let p = [1.3, 2.1, -0.7];
        let whole = segment_field([-5.0, 0.0, 0.0], [5.0, 0.0, 0.0], p);
        let h1 = segment_field([-5.0, 0.0, 0.0], [0.4, 0.0, 0.0], p);
        let h2 = segment_field([0.4, 0.0, 0.0], [5.0, 0.0, 0.0], p);
        for k in 0..3 {
            assert!((whole[k] - h1[k] - h2[k]).abs() < 1e-20);
        }
        // Reversed, the opposite.
        let r = segment_field([5.0, 0.0, 0.0], [-5.0, 0.0, 0.0], p);
        for k in 0..3 {
            assert!((whole[k] + r[k]).abs() < 1e-20);
        }
    }

    /// The ellipse: a linear field's major axis is its rms; a circular
    /// one's is \\( B/\sqrt2 \\).
    #[test]
    fn ellipse_axes() {
        let (rms, major) = rms_and_major(&[C::new(3.0, 0.0), C::new(4.0, 0.0)]);
        assert!((rms - 5.0).abs() < 1e-12 && (major - 5.0).abs() < 1e-12);
        let (rms, major) = rms_and_major(&[C::new(1.0, 0.0), C::new(0.0, 1.0)]);
        assert!((rms - 2f64.sqrt()).abs() < 1e-12 && (major - 1.0).abs() < 1e-12);
    }

    /// Balanced three-phase flat line, far away: \\( \sqrt3\mu_0 I s/(2\pi d^2) \\)
    /// (the dipole term), both from the 2D sum and from long segments.
    #[test]
    fn flat_line_far_field() {
        let (i, s, d) = (1000.0, 10.0, 5000.0);
        let phases = [(-s, 0.0), (0.0, -120.0), (s, 120.0)];
        let g2 = [Group2d {
            frequency_hz: 50.0,
            conductors: phases
                .iter()
                .map(|&(y, a)| Conductor2d {
                    y,
                    z: 20.0,
                    current: C::from_polar(i, f64::to_radians(a)),
                })
                .collect(),
        }];
        let (b, _) = field_2d(&g2, d, 20.0);
        let far = 3f64.sqrt() * MU0 * i * s / (2.0 * PI * d * d);
        assert!((b / far - 1.0).abs() < 2.0 * s / d, "{b} {far}");
        let g3 = [Group {
            frequency_hz: 50.0,
            segments: g2[0]
                .conductors
                .iter()
                .map(|c| Segment {
                    a: [-1e7, c.y, c.z],
                    b: [1e7, c.y, c.z],
                    current: c.current,
                })
                .collect(),
        }];
        let (b3, _) = field_3d(&g3, [0.0, d, 20.0]);
        assert!((b3 / b - 1.0).abs() < 1e-6, "{b3} {b}");
    }

    #[test]
    fn points_from_csv() {
        let p = parse_points("# houses\nname;x;y;height\nA;10;20;1.5\nB;11.5;-3;\n").unwrap_err();
        assert!(p.contains("line 4"), "{p}");
        let p = parse_points("x,y,z\n1,2,3\n").unwrap();
        assert_eq!(p[0].name, "P1");
        assert_eq!(p[0].z, Some(3.0));
    }
}
