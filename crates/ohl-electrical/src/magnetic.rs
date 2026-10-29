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
use std::sync::atomic::Ordering;

use nalgebra::{Complex, DMatrix, DVector};
use ohl_line::electrical::SystemKind;
use serde::Serialize;

use crate::cross_section::{
    self, Axis, CrossSection, LineSpan, PhaseData, Sources, StationAt, line_spans,
};
use crate::earth::MU0;
use crate::field_views::{self, CutValues, VerticalCut, VolumeBox, VolumeValues, compute_all};
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
    /// With `earth_wires`: the earth wires as a network over the line,
    /// each structure leading current into the ground through its
    /// earthing resistance, and the structures' currents as current
    /// paths (3D; [`TowerCurrent`]); else an infinitely long earth wire
    /// earthed everywhere.
    pub towers: bool,
    /// The profiles' height above the ground, m.
    pub height: f64,
    /// The profiles reach this far to either side of the axis, m.
    pub width: f64,
    /// Between profile points, m.
    pub step: f64,
    /// The profiles; empty: every span at its lowest point.
    pub stations: Vec<StationSpec>,
    /// Grids on a horizontal plane at `height` over these spans (3D),
    /// `grid_step` apart both ways, `width` to either side.
    pub grids: Vec<(String, String)>,
    /// Between the grids' points, m; `None`: `step`.
    pub grid_step: Option<f64>,
    /// Given points.
    pub points: Vec<GivenPoint>,
    /// A limit, µT (optional; none built in).
    pub limit_ut: Option<f64>,
    /// The longest straight piece of a wire, m (3D): the solver's pieces
    /// cut into equal parts no longer than this, placed on the curve
    /// through the solver's points ([`refine`]); `None`: the solver's
    /// pieces as they are.
    pub piece_length: Option<f64>,
    /// The jumpers carry their phase's current (3D; from the section
    /// whose current comes into the structure to the one it leaves by).
    pub jumpers: bool,
    /// Cuts and volumes take the wires within this plan distance of
    /// them only, m (3D; `None`: the whole line). The field with the whole
    /// line is computed at some of their points and the difference said
    /// ([`FieldCut::left_out`]).
    pub modelled_within: Option<f64>,
    /// Vertical cuts across the line (3D).
    pub cuts: Vec<CutSpec>,
    /// Volumes over spans (3D): the isosurfaces of `levels`.
    pub volumes: Vec<VolumeSpec>,
    /// The levels drawn (isolines, isosurfaces), µT, ascending: display
    /// only (none built in).
    pub levels: Vec<f64>,
}

/// A vertical cut across the line at a place in a span: offsets to
/// `width` either side of the axis, from the lowest ground under it to
/// `above` over its highest wire.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CutSpec {
    /// A structure of the span.
    pub from: String,
    /// The other.
    pub to: String,
    /// The place from the first structure (0–1); `None`: the lowest
    /// point of the span's lowest phase conductor.
    pub fraction: Option<f64>,
    /// To either side, m.
    pub width: f64,
    /// Above the highest wire crossing the cut, m.
    pub above: f64,
    /// Between the grid's points, m.
    pub step: f64,
}

/// A volume over a span: `width` to either side of its axis, from the
/// lowest ground under it to `above` over its highest wire, the fine
/// lattice `step` apart.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VolumeSpec {
    /// A structure of the span.
    pub from: String,
    /// The other.
    pub to: String,
    /// To either side, m.
    pub width: f64,
    /// Above the highest wire, m.
    pub above: f64,
    /// The fine lattice's step, m.
    pub step: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model: Model::ThreeD,
            heights_2d: Heights2d::Station,
            currents: Vec::new(),
            earth_wires: false,
            towers: false,
            height: 1.0,
            width: 50.0,
            step: 1.0,
            stations: Vec::new(),
            grids: Vec::new(),
            grid_step: None,
            points: Vec::new(),
            limit_ut: None,
            piece_length: None,
            modelled_within: None,
            jumpers: true,
            cuts: Vec::new(),
            volumes: Vec::new(),
            levels: Vec::new(),
        }
    }
}

pub use crate::field_views::Progress;

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
    /// Per level of the settings: how far its isolines lie off it
    /// ([`field_views::deviation`], 3D).
    pub deviation: Vec<Option<field_views::Deviation>>,
}

impl FieldGrid {
    /// The isolines of `level` (µT), 3D: straight across a cell, the
    /// places between its corners interpolated.
    pub fn isolines(&self, level: f64) -> Vec<Vec<[f64; 3]>> {
        let (nr, nc) = (self.rows, self.columns);
        if nr < 2 || nc < 2 || self.points.len() != nr * nc {
            return Vec::new();
        }
        let values: Vec<f64> = self.points.iter().map(|p| p.b_ut).collect();
        let at = |c: usize, r: usize| {
            let p = &self.points[r * nc + c];
            [p.x, p.y, p.z]
        };
        crate::contour::isolines(&values, nc, nr, level)
            .into_iter()
            .map(|line| {
                line.into_iter()
                    .map(|[c, r]| {
                        let (c0, r0) = (
                            (c.floor() as usize).min(nc - 2),
                            (r.floor() as usize).min(nr - 2),
                        );
                        let (fc, fr) = (c - c0 as f64, r - r0 as f64);
                        let (a, b, cc, d) = (
                            at(c0, r0),
                            at(c0 + 1, r0),
                            at(c0, r0 + 1),
                            at(c0 + 1, r0 + 1),
                        );
                        std::array::from_fn(|k| {
                            let lo = a[k] + fc * (b[k] - a[k]);
                            let hi = cc[k] + fc * (d[k] - cc[k]);
                            lo + fr * (hi - lo)
                        })
                    })
                    .collect()
            })
            .collect()
    }
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
    /// The structures' currents into the ground (the earth wire network).
    pub towers: Vec<TowerCurrent>,
    /// The profiles.
    pub profiles: Vec<Profile>,
    /// The grids.
    pub grids: Vec<FieldGrid>,
    /// The given points.
    pub points: Vec<FieldPoint>,
    /// The vertical cuts (B, µT), each with its span.
    pub cuts: Vec<FieldCut>,
    /// The volumes' isosurfaces (levels in µT), each with its span.
    pub volumes: Vec<FieldVolume>,
    /// The wires' straight pieces (3D).
    pub pieces: Option<Pieces>,
    /// What was assumed or left out.
    pub notes: Vec<String>,
}

/// A vertical cut's values and its span.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldCut {
    /// The span's first structure.
    pub from: String,
    /// Its second.
    pub to: String,
    /// The station, m from the first structure.
    pub at: f64,
    /// The values, µT.
    pub values: CutValues,
    /// The straight pieces taken (with `modelled_within`).
    pub pieces: usize,
    /// With `modelled_within`: how much the wires left out change the
    /// field, at up to [`field_views::CHECKED`] of the cut's points (the
    /// relative difference to the whole line's field).
    pub left_out: Option<field_views::Deviation>,
}

/// A volume's surfaces and its span.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldVolume {
    /// The span's first structure.
    pub from: String,
    /// Its second.
    pub to: String,
    /// The surfaces (levels in µT).
    pub values: VolumeValues,
    /// The straight pieces taken (with `modelled_within`).
    pub pieces: usize,
    /// With `modelled_within`: how much the wires left out change the
    /// field at up to [`field_views::CHECKED`] points of the surfaces.
    pub left_out: Option<field_views::Deviation>,
}

/// The wires' straight pieces in a 3D model, and how far they may lie off
/// the wires.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Pieces {
    /// Straight pieces (every subconductor's).
    pub count: usize,
    /// The longest, m.
    pub longest: f64,
    /// The most a wire lies off its pieces, m: the solver's bound
    /// ([`ohl_line::solve::SpanShape::polyline_error`]) over the cut
    /// pieces (a sagitta falls with the square of the piece's length),
    /// plus the interpolation's.
    pub off_wire: f64,
    /// Of which the placing of the cut pieces' points on the curve
    /// through the solver's points (an estimate, [`refine`]), m.
    pub interpolation: f64,
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
    /// Which way the current flows in each section and span.
    directions: Directions,
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
        Ok(Self {
            by_key,
            directions: Directions::default(),
        })
    }

    /// A phase's current in a section's span from the span's first
    /// structure to its second (the cross-sections' frame), A, and its
    /// frequency.
    fn in_span(&self, p: &PhaseData, section: &str, span: usize) -> Option<(f64, C)> {
        let (f, i) = self.of(p)?;
        Some((f, i * self.directions.span_frame(section, span)))
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

/// What the earth wires of a span see at one frequency, from its (mean)
/// cross-section: their keys (section id, span index), their impedance
/// matrix per metre with earth return \( Z_{ee} \) and the voltage the
/// phases induce along them per metre \( Z_{ep} I_p \) (the primitive
/// matrices of [`matrices::impedance`]).
struct EarthTerms {
    frequency_hz: f64,
    keys: Vec<(String, usize)>,
    zee: DMatrix<C>,
    emf: DVector<C>,
}

/// The earth wires' terms of a span ([`EarthTerms`]), per frequency.
fn earth_terms(
    cs: &CrossSection,
    currents: &Currents,
    method: ohl_line::electrical::EarthReturn,
    notes: &mut Vec<String>,
) -> Vec<EarthTerms> {
    let earthed: Vec<&cross_section::CrossWire> =
        cs.wires.iter().filter(|w| w.phase.is_none()).collect();
    if earthed.is_empty() {
        return Vec::new();
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
                return Vec::new();
            }
        }
    }
    let mut phases: Vec<(f64, C, Wire)> = Vec::new();
    for w in &cs.wires {
        let Some(p) = &w.phase else { continue };
        let Some((f, i)) = currents.in_span(p, &w.section, w.span) else {
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
    let mut out = Vec::new();
    for f in freqs {
        let ph: Vec<&(f64, C, Wire)> = phases.iter().filter(|p| p.0 == f).collect();
        let mut wires = ew.clone();
        wires.extend(ph.iter().map(|p| p.2));
        let z = matrices::impedance(&wires, method, f, cs.ground.soil_resistivity_ohm_m);
        let zee = DMatrix::from_fn(ne, ne, |i, j| z[(i, j)]);
        let zep = DMatrix::from_fn(ne, ph.len(), |i, j| z[(i, ne + j)]);
        let ip = DVector::from_iterator(ph.len(), ph.iter().map(|p| p.1));
        out.push(EarthTerms {
            frequency_hz: f,
            keys: earthed
                .iter()
                .map(|w| (w.section.clone(), w.span))
                .collect(),
            zee,
            emf: zep * ip,
        });
    }
    out
}

/// The earth wires' induced currents in a span from its (mean)
/// cross-section, per frequency: \( I_e = -Z_{ee}^{-1} Z_{ep} I_p \)
/// with the primitive matrices of [`matrices::impedance`] (an infinitely
/// long earth wire earthed everywhere, its voltage along it zero).
fn earth_wire_currents(
    cs: &CrossSection,
    currents: &Currents,
    method: ohl_line::electrical::EarthReturn,
    notes: &mut Vec<String>,
) -> EarthWireMap {
    let mut out = EarthWireMap::new();
    for t in earth_terms(cs, currents, method, notes) {
        let Some(inv) = t.zee.clone().try_inverse() else {
            notes.push(format!(
                "span {}–{}: the earth wires' matrix is singular; no earth wire currents",
                cs.station.from, cs.station.to
            ));
            continue;
        };
        let ie = -(inv * &t.emf);
        for (k, key) in t.keys.iter().enumerate() {
            out.entry(key.clone())
                .or_default()
                .push((t.frequency_hz, ie[k]));
        }
    }
    out
}

/// A structure's current from the earth wires into the ground (the earth
/// wire network, [`earth_wire_network`]).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TowerCurrent {
    /// The structure.
    pub structure: String,
    /// Frequency, Hz.
    pub frequency_hz: f64,
    /// Current into the ground, A (rms).
    pub amps: f64,
    /// Its phase angle, degrees.
    pub angle_deg: f64,
    /// The earthing resistance taken, Ω.
    pub resistance_ohm: f64,
    /// The resistance is the assumed default.
    pub assumed: bool,
    /// The current as a phasor, A (into the ground).
    #[serde(skip)]
    pub current: C,
}

/// The earth wires as a network over the whole line (owner, 2026-10-29:
/// the towers' steel): each structure bonding its earth wires, at one
/// voltage, and leading current into the ground through its earthing
/// resistance \( R_n \); each span's earth wires carrying
/// \( I_e = (Z_{ee} L)^{-1} \big( (V_a - V_b)\,\mathbf 1 - Z_{ep} I_p L \big) \)
/// from its first structure \( a \) to its second \( b \) (\( L \) the
/// span's length). Kirchhoff's current law at every structure
/// (\( \sum I_e \) out \( + V_n / R_n = 0 \)) gives the structures'
/// voltages, then the earth wires' currents per span and the towers'
/// \( V_n / R_n \). With every \( R_n \to 0 \) it is the infinitely long
/// earth wire earthed everywhere ([`earth_wire_currents`]).
fn earth_wire_network(
    line: &ohl_line::Line,
    means: &[(LineSpan, CrossSection)],
    axes: &[(LineSpan, f64)],
    currents: &Currents,
    notes: &mut Vec<String>,
) -> (EarthWireMap, Vec<TowerCurrent>) {
    let method = line.electrical.earth_return;
    // Per span: its structures, length and terms.
    let mut spans = Vec::new();
    for (span, cs) in means {
        let Some(length) = axes.iter().find(|(s, _)| s == span).map(|a| a.1) else {
            continue;
        };
        for t in earth_terms(cs, currents, method, notes) {
            spans.push((span.from.clone(), span.to.clone(), length, t));
        }
    }
    let mut freqs: Vec<f64> = spans.iter().map(|s| s.3.frequency_hz).collect();
    freqs.sort_by(f64::total_cmp);
    freqs.dedup();
    let mut map = EarthWireMap::new();
    let mut towers = Vec::new();
    let mut assumed = 0usize;
    for f in freqs {
        let these: Vec<_> = spans.iter().filter(|s| s.3.frequency_hz == f).collect();
        let mut nodes: Vec<String> = Vec::new();
        for s in &these {
            for id in [&s.0, &s.1] {
                if !nodes.contains(id) {
                    nodes.push(id.clone());
                }
            }
        }
        let n = nodes.len();
        let at = |id: &str| nodes.iter().position(|x| x == id).expect("a node");
        let mut a = DMatrix::<C>::zeros(n, n);
        let mut rhs = DVector::<C>::zeros(n);
        let mut res = Vec::with_capacity(n);
        for (k, id) in nodes.iter().enumerate() {
            let (r, is_assumed) = line.earthing_resistance(id);
            assumed += usize::from(is_assumed);
            a[(k, k)] += C::new(1.0 / r, 0.0);
            res.push((r, is_assumed));
        }
        // Per span: y = 1ᵀ (Z L)⁻¹ 1, g = 1ᵀ (Z L)⁻¹ Z_ep I_p L, and
        // (Z L)⁻¹ kept for the currents.
        let mut solved = Vec::new();
        for s in &these {
            let (from, to, len, t) = (&s.0, &s.1, s.2, &s.3);
            let Some(inv) = (t.zee.clone() * C::new(len, 0.0)).try_inverse() else {
                notes.push(format!(
                    "span {from}–{to}: the earth wires' matrix is singular; no earth wire currents"
                ));
                continue;
            };
            let ones = DVector::from_element(t.keys.len(), C::new(1.0, 0.0));
            let emf = &t.emf * C::new(len, 0.0);
            let y = (ones.transpose() * &inv * &ones)[(0, 0)];
            let g = (ones.transpose() * &inv * &emf)[(0, 0)];
            let (i, j) = (at(from), at(to));
            a[(i, i)] += y;
            a[(j, j)] += y;
            a[(i, j)] -= y;
            a[(j, i)] -= y;
            rhs[i] += g;
            rhs[j] -= g;
            solved.push((i, j, inv, emf, ones, t));
        }
        let Some(v) = a.lu().solve(&rhs) else {
            notes.push(format!(
                "the earth wire network at {f} Hz could not be solved; no earth wire currents"
            ));
            continue;
        };
        for (i, j, inv, emf, ones, t) in solved {
            let ie = inv * (ones * (v[i] - v[j]) - emf);
            for (k, key) in t.keys.iter().enumerate() {
                map.entry(key.clone()).or_default().push((f, ie[k]));
            }
        }
        for (k, id) in nodes.iter().enumerate() {
            let it = v[k] / res[k].0;
            towers.push(TowerCurrent {
                structure: id.clone(),
                frequency_hz: f,
                amps: it.norm(),
                angle_deg: if it.norm() > 1e-9 {
                    it.arg().to_degrees()
                } else {
                    0.0
                },
                resistance_ohm: res[k].0,
                assumed: res[k].1,
                current: it,
            });
        }
    }
    if assumed > 0 {
        let (r, _) = line.electrical.earthing_resistance();
        notes.push(format!(
            "earthing resistance not given at {} structure(s): {r} Ω taken (the line's, or assumed when the line gives none — set them for the real line)",
            assumed
        ));
    }
    (map, towers)
}

// ---------------------------------------------------------------------------
// The run.

/// A polyline carrying one current: its points, its pieces' directions
/// and lengths.
#[derive(Debug, Clone, Default)]
struct Poly {
    x: Vec<f64>,
    y: Vec<f64>,
    z: Vec<f64>,
    ux: Vec<f64>,
    uy: Vec<f64>,
    uz: Vec<f64>,
    len: Vec<f64>,
    current: C,
}

impl Poly {
    fn new(points: &[[f64; 3]], current: C) -> Self {
        let mut p = Poly {
            current,
            ..Default::default()
        };
        for q in points {
            p.x.push(q[0]);
            p.y.push(q[1]);
            p.z.push(q[2]);
        }
        for w in points.windows(2) {
            let d = sub(w[1], w[0]);
            let l = dot(d, d).sqrt();
            let u = if l < 1e-12 {
                [0.0; 3]
            } else {
                d.map(|x| x / l)
            };
            p.ux.push(u[0]);
            p.uy.push(u[1]);
            p.uz.push(u[2]);
            p.len.push(if l < 1e-12 { 0.0 } else { l });
        }
        p
    }

    /// \( \sum \) of [`segment_field`] over the pieces at `p` (per ampere),
    /// each vertex's distance computed once for the two pieces meeting
    /// there; `inv`: room for the inverse distances.
    fn field(&self, p: [f64; 3], inv: &mut Vec<f64>) -> [f64; 3] {
        inv.clear();
        for k in 0..self.x.len() {
            let (dx, dy, dz) = (p[0] - self.x[k], p[1] - self.y[k], p[2] - self.z[k]);
            inv.push(1.0 / (dx * dx + dy * dy + dz * dz).sqrt());
        }
        let (mut sx, mut sy, mut sz) = (0.0, 0.0, 0.0);
        for k in 0..self.len.len() {
            let (rx, ry, rz) = (p[0] - self.x[k], p[1] - self.y[k], p[2] - self.z[k]);
            let (ux, uy, uz) = (self.ux[k], self.uy[k], self.uz[k]);
            let t = rx * ux + ry * uy + rz * uz;
            let (dx, dy, dz) = (rx - t * ux, ry - t * uy, rz - t * uz);
            let rho2 = dx * dx + dy * dy + dz * dz;
            if rho2 < 1e-18 || self.len[k] == 0.0 {
                continue;
            }
            let f = (t * inv[k] - (t - self.len[k]) * inv[k + 1]) / rho2;
            sx += (uy * dz - uz * dy) * f;
            sy += (uz * dx - ux * dz) * f;
            sz += (ux * dy - uy * dx) * f;
        }
        let m = MU0 / (4.0 * PI);
        [sx * m, sy * m, sz * m]
    }
}

/// The line's wires in 3D as straight pieces, by frequency, ready to sum:
/// the same sum as [`field_3d`] over [`segment_field`], arranged per
/// polyline (each vertex's distance computed once; equal to it within
/// rounding).
#[derive(Debug, Clone, Default)]
pub struct Wires3d {
    groups: Vec<(f64, Vec<Poly>)>,
    /// The pieces and how far they may lie off the wires.
    pub pieces: Option<Pieces>,
}

impl Wires3d {
    fn push(&mut self, frequency_hz: f64, points: &[[f64; 3]], current: C) {
        if points.len() < 2 {
            return;
        }
        let poly = Poly::new(points, current);
        match self.groups.iter_mut().find(|g| g.0 == frequency_hz) {
            Some(g) => g.1.push(poly),
            None => self.groups.push((frequency_hz, vec![poly])),
        }
    }

    /// B (rms) and its ellipse's semi-major axis at `p`, T, each
    /// frequency's added as the root of the sum of squares.
    pub fn field(&self, p: [f64; 3]) -> (f64, f64) {
        let mut inv = Vec::new();
        let mut sum = (0.0, 0.0);
        for (_, polys) in &self.groups {
            let mut b = [C::new(0.0, 0.0); 3];
            for poly in polys {
                let f = poly.field(p, &mut inv);
                for k in 0..3 {
                    b[k] += poly.current * f[k];
                }
            }
            let (rms, major) = rms_and_major(&b);
            sum.0 += rms * rms;
            sum.1 += major * major;
        }
        (sum.0.sqrt(), sum.1.sqrt())
    }

    /// [`Wires3d::field`] at each of `ps`, each wire's pieces taken for
    /// the whole block (they stay in the processor's cache: some times
    /// quicker than point by point on a long line).
    pub fn field_block(&self, ps: &[[f64; 3]]) -> Vec<(f64, f64)> {
        let mut inv = Vec::new();
        let mut sum = vec![(0.0, 0.0); ps.len()];
        let mut b = vec![[C::new(0.0, 0.0); 3]; ps.len()];
        for (_, polys) in &self.groups {
            b.fill([C::new(0.0, 0.0); 3]);
            for poly in polys {
                for (p, bk) in ps.iter().zip(b.iter_mut()) {
                    let f = poly.field(*p, &mut inv);
                    for k in 0..3 {
                        bk[k] += poly.current * f[k];
                    }
                }
            }
            for (bk, s) in b.iter().zip(sum.iter_mut()) {
                let (rms, major) = rms_and_major(bk);
                s.0 += rms * rms;
                s.1 += major * major;
            }
        }
        sum.into_iter().map(|s| (s.0.sqrt(), s.1.sqrt())).collect()
    }

    /// The wires' pieces within `d` (plan, m) of the band `width` either
    /// side of `path` (a piece by its nearer end, and half its length).
    pub fn within(&self, path: &field_views::Path, width: f64, d: f64) -> Wires3d {
        let near = |x: f64, y: f64, half: f64| (path.distance([x, y]) - width).max(0.0) <= d + half;
        let mut out = Wires3d {
            groups: Vec::new(),
            pieces: self.pieces,
        };
        for (f, polys) in &self.groups {
            for p in polys {
                let mut run: Vec<[f64; 3]> = Vec::new();
                for k in 0..p.len.len() {
                    let half = 0.5 * p.len[k];
                    if near(p.x[k], p.y[k], half) || near(p.x[k + 1], p.y[k + 1], half) {
                        if run.is_empty() {
                            run.push([p.x[k], p.y[k], p.z[k]]);
                        }
                        run.push([p.x[k + 1], p.y[k + 1], p.z[k + 1]]);
                    } else if !run.is_empty() {
                        out.push(*f, &std::mem::take(&mut run), p.current);
                    }
                }
                out.push(*f, &run, p.current);
            }
        }
        out
    }

    /// Every straight piece (start, end).
    pub fn segments(&self) -> Vec<[[f64; 3]; 2]> {
        let mut out = Vec::new();
        for (_, polys) in &self.groups {
            for p in polys {
                for k in 0..p.len.len() {
                    out.push([
                        [p.x[k], p.y[k], p.z[k]],
                        [p.x[k + 1], p.y[k + 1], p.z[k + 1]],
                    ]);
                }
            }
        }
        out
    }

    /// The number of straight pieces.
    pub fn count(&self) -> usize {
        self.groups
            .iter()
            .flat_map(|g| &g.1)
            .map(|p| p.len.len())
            .sum()
    }
}

/// The seconds one straight piece takes at one point on this computer,
/// all cores working (wall time): measured once, on a line of 78 wires ×
/// 640 pieces at 2560 points (some 50 ms) — for the time estimates.
pub fn seconds_per_piece_point() -> f64 {
    static MEASURED: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *MEASURED.get_or_init(|| {
        let mut w = Wires3d::default();
        for k in 0..78 {
            let pts: Vec<[f64; 3]> = (0..=640)
                .map(|i| {
                    let x = i as f64 * 6.25;
                    let u = x % 400.0 - 200.0;
                    [x, k as f64 * 0.5, 30.0 + u * u / 4000.0]
                })
                .collect();
            w.push(50.0, &pts, C::new(1000.0, k as f64));
        }
        let points: Vec<[f64; 3]> = (0..2560)
            .map(|i| [i as f64, 50.0 + (i % 7) as f64, 1.0])
            .collect();
        let start = std::time::Instant::now();
        let v = compute_all(&points, &|ps: &[[f64; 3]]| w.field_block(ps), None);
        std::hint::black_box(v.ok());
        start.elapsed().as_secs_f64() / (w.count() as f64 * points.len() as f64)
    })
}

/// A polyline's pieces cut into equal parts no longer than `max` (m). The
/// new points lie on the quadratic through the piece's ends and a
/// neighbouring point (parameter: the length along the polyline), the
/// two neighbours' quadratics averaged where both exist. Where the two
/// differ by more than `bound` (a kink: a spacer, a clamp, a span point)
/// the piece is cut on its chord (no better than the solver's pieces
/// there, no worse). Returns the points, the largest half difference of
/// the two quadratics used (an estimate of the placing's error, m), and
/// the largest number of parts a piece was cut into.
pub fn refine(points: &[[f64; 3]], max: f64, bound: f64) -> (Vec<[f64; 3]>, f64, usize) {
    if points.len() < 2 || max.is_nan() || max <= 0.0 {
        return (points.to_vec(), 0.0, 1);
    }
    let mut u = vec![0.0];
    for w in points.windows(2) {
        let d = sub(w[1], w[0]);
        u.push(u.last().copied().unwrap_or(0.0) + dot(d, d).sqrt());
    }
    let quad = |i: usize, x: f64| -> [f64; 3] {
        // Through points i, i+1, i+2.
        let (u0, u1, u2) = (u[i], u[i + 1], u[i + 2]);
        let l0 = (x - u1) * (x - u2) / ((u0 - u1) * (u0 - u2));
        let l1 = (x - u0) * (x - u2) / ((u1 - u0) * (u1 - u2));
        let l2 = (x - u0) * (x - u1) / ((u2 - u0) * (u2 - u1));
        std::array::from_fn(|k| l0 * points[i][k] + l1 * points[i + 1][k] + l2 * points[i + 2][k])
    };
    let n = points.len();
    let mut out = vec![points[0]];
    let mut estimate: f64 = 0.0;
    let mut most = 1;
    for k in 0..n - 1 {
        let len = u[k + 1] - u[k];
        let parts = (len / max).ceil().max(1.0) as usize;
        most = most.max(parts);
        let distinct = |i: usize| u[i + 1] > u[i] && u[i + 2] > u[i + 1];
        let left = (k >= 1 && distinct(k - 1)).then(|| k - 1);
        let right = (k + 2 < n && distinct(k)).then_some(k);
        let mut placed = Vec::with_capacity(parts);
        let mut worst: f64 = 0.0;
        for j in 1..parts {
            let x = u[k] + len * j as f64 / parts as f64;
            let chord: [f64; 3] = std::array::from_fn(|c| {
                points[k][c] + (points[k + 1][c] - points[k][c]) * j as f64 / parts as f64
            });
            let p = match (left.map(|i| quad(i, x)), right.map(|i| quad(i, x))) {
                (Some(a), Some(b)) => {
                    let d = sub(a, b);
                    worst = worst.max(0.5 * dot(d, d).sqrt());
                    std::array::from_fn(|c| 0.5 * (a[c] + b[c]))
                }
                (Some(a), None) | (None, Some(a)) => a,
                (None, None) => chord,
            };
            placed.push((p, chord));
        }
        if worst > bound {
            out.extend(placed.iter().map(|q| q.1));
        } else {
            estimate = estimate.max(worst);
            out.extend(placed.iter().map(|q| q.0));
        }
        out.push(points[k + 1]);
    }
    (out, estimate, most)
}

/// Which way the current flows in each section and span (2026-10-29).
///
/// A circuit's current flows on through its sections and the jumpers
/// joining them, so a section's current runs one way along all its points
/// and on into the next one: the sections joined by jumpers are oriented
/// alike (a jumper from one's last point to another's first: the same
/// way; last to last or first to first: opposite ways). Each chain is then
/// turned so that it runs along the line (from the line span's first
/// structure to its second) over most of its length — the "along the
/// line" of the circuits' load flow. Until 2026-10-29 every span took the
/// line spans' order on its own, which at a structure where that order
/// turns sent a current out of the structure by both sections.
#[derive(Debug, Clone, Default)]
struct Directions {
    /// Per (section, span): +1 when the span's points run from the line
    /// span's first structure to its second.
    orient: BTreeMap<(usize, usize), f64>,
    /// Per section: +1 when the circuit's current runs along its points.
    along: Vec<f64>,
    /// Sections by id.
    by_id: BTreeMap<String, usize>,
}

impl Directions {
    fn new(line: &ohl_line::Line, spans: &[LineSpan]) -> Self {
        let orient = span_signs(line, spans);
        let n = line.sections.len();
        // Union–find with the parity of each section to its root.
        let mut parent: Vec<usize> = (0..n).collect();
        let mut parity = vec![1.0; n];
        fn find(parent: &mut Vec<usize>, parity: &mut Vec<f64>, x: usize) -> (usize, f64) {
            if parent[x] == x {
                return (x, 1.0);
            }
            let (r, p) = find(parent, parity, parent[x]);
            parity[x] *= p;
            parent[x] = r;
            (r, parity[x])
        }
        let end_of = |p: &ohl_line::SectionPoint| -> Option<(usize, bool)> {
            line.sections.iter().enumerate().find_map(|(k, sec)| {
                let m = sec.points.len();
                if m < 2 {
                    None
                } else if sec.points[m - 1] == *p {
                    Some((k, true))
                } else if sec.points[0] == *p {
                    Some((k, false))
                } else {
                    None
                }
            })
        };
        for j in &line.jumpers {
            let (Some(a), Some(b)) = (j.from.point(), j.to.point()) else {
                continue;
            };
            let (Some((ka, last_a)), Some((kb, last_b))) = (end_of(a), end_of(b)) else {
                continue;
            };
            // s_a = rel · s_b.
            let rel = if last_a != last_b { 1.0 } else { -1.0 };
            let (ra, pa) = find(&mut parent, &mut parity, ka);
            let (rb, pb) = find(&mut parent, &mut parity, kb);
            if ra != rb {
                parent[ra] = rb;
                parity[ra] = rel * pa * pb;
            }
        }
        let place = |id: &str| line.structure(id).map(|s| s.position);
        let mut vote = vec![0.0; n];
        let roots: Vec<(usize, f64)> = (0..n)
            .map(|k| find(&mut parent, &mut parity, k))
            .collect();
        for (&(k, j), &o) in &orient {
            let sec = &line.sections[k];
            let len = match (
                place(&sec.points[j].structure),
                place(&sec.points[j + 1].structure),
            ) {
                (Some(a), Some(b)) => (b[0] - a[0]).hypot(b[1] - a[1]),
                _ => 1.0,
            };
            vote[roots[k].0] += roots[k].1 * o * len;
        }
        let along = (0..n)
            .map(|k| roots[k].1 * if vote[roots[k].0] >= 0.0 { 1.0 } else { -1.0 })
            .collect();
        Self {
            orient,
            along,
            by_id: line
                .sections
                .iter()
                .enumerate()
                .map(|(k, s)| (s.id.clone(), k))
                .collect(),
        }
    }

    /// +1 when the circuit's current runs along section `k`'s points.
    fn along(&self, k: usize) -> f64 {
        self.along.get(k).copied().unwrap_or(1.0)
    }

    /// +1 when section `k`'s span `j` runs from the line span's first
    /// structure to its second.
    fn orient(&self, k: usize, j: usize) -> f64 {
        self.orient.get(&(k, j)).copied().unwrap_or(1.0)
    }

    /// +1 when a circuit's current in a section's span runs from the line
    /// span's first structure to its second.
    fn span_frame(&self, section: &str, span: usize) -> f64 {
        self.by_id
            .get(section)
            .map_or(1.0, |&k| self.along(k) * self.orient(k, span))
    }
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
    earth: &EarthWireMap,
    settings: &Settings,
    notes: &mut Vec<String>,
) -> Result<Wires3d, String> {
    let (with_earth_wires, piece_length) = (settings.earth_wires, settings.piece_length);
    let line = src.line;
    let phases = line.section_phases().map_err(|e| e.to_string())?;
    let mut wires = Wires3d::default();
    let mut info = Pieces {
        count: 0,
        longest: 0.0,
        off_wire: 0.0,
        interpolation: 0.0,
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
            // Along the span's points: a phase's current by its circuit's
            // direction; an earth wire's (in the span's frame) by the
            // span's.
            let sign = if phase.is_some() {
                currents.directions.along(k)
            } else {
                currents.directions.orient(k, j)
            };
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
            let (centre, off) = match piece_length {
                Some(max) => {
                    let (pts, est, parts) = refine(&sp.points, max, sp.polyline_error);
                    info.interpolation = info.interpolation.max(est);
                    (pts, sp.polyline_error / (parts * parts) as f64 + est)
                }
                None => (sp.points.clone(), sp.polyline_error),
            };
            info.off_wire = info.off_wire.max(off);
            let subs = section.subconductor_polylines(&centre);
            let n = subs.len().max(1) as f64;
            for poly in &subs {
                for w in poly.windows(2) {
                    let d = sub(w[1], w[0]);
                    info.longest = info.longest.max(dot(d, d).sqrt());
                }
            }
            for (f, i) in flows {
                let i = i * (sign / n);
                for poly in &subs {
                    wires.push(f, poly, i);
                }
            }
        }
    }
    if settings.jumpers {
        add_jumpers(src, currents, &phases, piece_length, &mut wires, notes);
    }
    info.count = wires.count();
    wires.pieces = Some(info);
    Ok(wires)
}

/// The jumpers as current paths: a jumper between two section ends of
/// one phase carries that phase's current, from the end whose section
/// brings the current into the structure to the end whose section takes
/// it on (by the sections' directions along the line and the circuit's
/// load flow). A jumper on a tap, between different phases or between
/// two ends the current enters (or leaves) by is left out, said.
fn add_jumpers(
    src: &Sources<'_>,
    currents: &Currents,
    phases: &BTreeMap<String, Option<(usize, usize)>>,
    piece_length: Option<f64>,
    wires: &mut Wires3d,
    notes: &mut Vec<String>,
) {
    let line = src.line;
    // A section end at a wire point: the section's index, whether the
    // current comes in there (towards the structure).
    let end_of = |p: &ohl_line::SectionPoint| -> Option<(usize, bool)> {
        line.sections.iter().enumerate().find_map(|(k, sec)| {
            let n = sec.points.len();
            if n < 2 {
                return None;
            }
            // The circuit's current runs along the section's points (or
            // against them): in at the last point when along.
            let along = currents.directions.along(k);
            if sec.points[n - 1] == *p {
                Some((k, along > 0.0))
            } else if sec.points[0] == *p {
                Some((k, along < 0.0))
            } else {
                None
            }
        })
    };
    let mut left_out = Vec::new();
    for shape in src.jumpers {
        let Some(j) = line.jumpers.iter().find(|j| j.id == shape.jumper) else {
            continue;
        };
        if shape.points.len() < 2 {
            continue;
        }
        let (Some(a), Some(b)) = (j.from.point(), j.to.point()) else {
            left_out.push(format!("{} (a tap)", j.id));
            continue;
        };
        let (Some((ka, in_a)), Some((kb, in_b))) = (end_of(a), end_of(b)) else {
            left_out.push(format!("{} (not at a section's end)", j.id));
            continue;
        };
        let phase = |k: usize| {
            let sec = &line.sections[k];
            phases
                .get(&sec.id)
                .copied()
                .flatten()
                .map(|at| cross_section::phase_data(line, sec, at))
        };
        let (Some(pa), Some(pb)) = (phase(ka), phase(kb)) else {
            // Earth wire jumpers: no current of their own (bonding).
            continue;
        };
        if pa.circuit != pb.circuit || pa.system != pb.system || pa.index != pb.index {
            left_out.push(format!("{} (between different phases)", j.id));
            continue;
        }
        if in_a == in_b {
            left_out.push(format!(
                "{} (the current {} by both ends)",
                j.id,
                if in_a { "comes in" } else { "leaves" }
            ));
            continue;
        }
        let Some((f, i)) = currents.of(&pa) else {
            continue;
        };
        // Along the jumper's points (from → to) when it comes in at `from`.
        let i = if in_a { i } else { -i };
        let points = match piece_length {
            Some(max) => refine(&shape.points, max, f64::INFINITY).0,
            None => shape.points.clone(),
        };
        wires.push(f, &points, i);
    }
    if !left_out.is_empty() {
        notes.push(format!(
            "jumpers without a current of their own: {}",
            left_out.join(", ")
        ));
    }
}

/// The structures' currents as current paths: down the structure's axis
/// from its highest earth wire point to its foot (a simplification: the
/// current shares itself among the legs and members; the field within a
/// few metres of the structure).
fn add_towers(line: &ohl_line::Line, towers: &[TowerCurrent], wires: &mut Wires3d) {
    for t in towers {
        let Some(st) = line.structure(&t.structure) else {
            continue;
        };
        let top = line
            .sections
            .iter()
            .filter(|s| s.system.is_none())
            .flat_map(|s| &s.points)
            .filter(|p| p.structure == t.structure)
            .filter_map(|p| st.wires.iter().find(|w| w.set == Some(p.set)))
            .map(|w| w.position[2])
            .reduce(f64::max);
        let Some(top) = top else { continue };
        let foot = st.position;
        if top <= foot[2] {
            continue;
        }
        wires.push(t.frequency_hz, &[[foot[0], foot[1], top], foot], t.current);
    }
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
            Some(p) => currents
                .in_span(p, &w.section, w.span)
                .into_iter()
                .collect(),
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

/// The magnetic field of the line solved in one weather case (`src`), as
/// `settings` ask. `progress`: to follow and stop it from another thread.
pub fn run(
    src: &Sources<'_>,
    settings: &Settings,
    progress: Option<&Progress>,
) -> Result<MagneticField, String> {
    run_keeping(src, settings, progress).map(|r| r.0)
}

/// [`run`], and the 3D model's wires kept (3D only) for cuts computed
/// later at other places ([`field_views::vertical_cut`] with
/// [`Wires3d::field`]).
pub fn run_keeping(
    src: &Sources<'_>,
    settings: &Settings,
    progress: Option<&Progress>,
) -> Result<(MagneticField, Option<Wires3d>), String> {
    let line = src.line;
    if !(settings.step > 0.0 && settings.width >= 0.0 && settings.height.is_finite()) {
        return Err("a step above 0 m and a width of at least 0 m".into());
    }
    if let Some(l) = settings.piece_length
        && !(l.is_finite() && l > 0.0)
    {
        return Err(format!("a piece length above 0 m, not {l}"));
    }
    let grid_step = settings.grid_step.unwrap_or(settings.step);
    if grid_step.is_nan() || grid_step <= 0.0 {
        return Err(format!("a grid step above 0 m, not {grid_step}"));
    }
    if settings.model == Model::TwoD && !(settings.cuts.is_empty() && settings.volumes.is_empty()) {
        return Err("cuts and volumes are computed in 3D".into());
    }
    if let Some(l) = settings.limit_ut
        && !(l.is_finite() && l > 0.0)
    {
        return Err(format!("a limit above 0 µT, not {l}"));
    }
    let mut notes = Vec::new();
    let mut currents = Currents::new(line, &settings.currents, &mut notes)?;
    let spans = line_spans(line);
    if spans.is_empty() {
        return Err("the line has no spans".into());
    }
    currents.directions = Directions::new(line, &spans);
    let axes: Vec<Option<Axis>> = spans.iter().map(|s| Axis::of(line, s).ok()).collect();
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
    let mut towers = Vec::new();
    if settings.earth_wires && settings.towers {
        let lengths: Vec<(LineSpan, f64)> = spans
            .iter()
            .zip(&axes)
            .filter_map(|(s, a)| a.as_ref().map(|a| (s.clone(), a.length)))
            .collect();
        let (m, t) = earth_wire_network(line, &means, &lengths, &currents, &mut notes);
        for ((section, j), list) in &m {
            let span = means.iter().find(|(_, cs)| {
                cs.wires
                    .iter()
                    .any(|w| &w.section == section && w.span == *j)
            });
            for (f, i) in list {
                earth_rows.push(EarthWireCurrent {
                    section: section.clone(),
                    from: span.map_or(String::new(), |s| s.0.from.clone()),
                    to: span.map_or(String::new(), |s| s.0.to.clone()),
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
        earth = m;
        towers = t;
        notes.push(
            "earth wire currents: the earth wires as a network over the line, each structure leading current into the ground through its earthing resistance (per span from its mean cross-section); the earth wires end at the line's end structures, earthed there alone (a substation's earthing grid is not modelled)".into(),
        );
    } else if settings.earth_wires {
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
        format!(
            "the ground's return currents left out (their image lies at the complex depth, hundreds of metres down at power frequency); {}; balanced phases, no harmonics",
            if settings.jumpers && settings.model == Model::ThreeD {
                "the jumpers carry their phase's current"
            } else {
                "jumpers left out"
            }
        ),
    );
    let model3d = match settings.model {
        Model::ThreeD => Some(build_3d(src, &currents, &earth, settings, &mut notes).map(
            |mut w| {
                add_towers(line, &towers, &mut w);
                w
            },
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
    let n_grid = (2.0 * settings.width / grid_step).round() as usize + 1;
    let grid_offsets: Vec<f64> = (0..n_grid)
        .map(|k| -settings.width + k as f64 * grid_step)
        .collect();
    let mut grid_specs = Vec::new();
    for (from, to) in &settings.grids {
        grid_specs.push(run_spans(&spans, from, to).map_err(|e| format!("grid: {e}"))?);
    }
    if let Some(p) = progress {
        let grid_points: usize = grid_specs
            .iter()
            .flatten()
            .filter_map(|&i| axes[i].as_ref())
            .map(|a| ((a.length / grid_step).ceil() as usize + 1) * n_grid)
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
                compute_all(&pts, &|ps: &[[f64; 3]]| m.field_block(ps), progress)?
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
    // Grids: in 3D one over each run of spans along the line, laid on
    // its path (continuous across the line angles); in 2D per span, the
    // profile repeated along it.
    let mut grids = Vec::new();
    for run in &grid_specs {
        if let Some(m) = &model3d {
            check_cancel(progress)?;
            let path = run_path(src, &spans, run)?;
            if !path.unfolded(settings.width) {
                return Err(format!(
                    "grid: ±{} m folds over itself at a sharp line angle: a smaller width",
                    settings.width
                ));
            }
            let length = path.length();
            let rows = (length / grid_step).ceil().max(1.0) as usize + 1;
            let ground = |t: f64, s: f64| {
                let p = path.place(t, s);
                src.ground
                    .and_then(|g| g(p[0], p[1]))
                    .unwrap_or_else(|| path.ground(t))
            };
            let mut places = Vec::new();
            for r in 0..rows {
                let t = length * r as f64 / (rows - 1) as f64;
                for &s in &grid_offsets {
                    let p = path.place(t, s);
                    let g = ground(t, s);
                    places.push(([p[0], p[1], g + settings.height], s, g));
                }
            }
            let pts: Vec<[f64; 3]> = places.iter().map(|p| p.0).collect();
            let values = compute_all(&pts, &|ps: &[[f64; 3]]| m.field_block(ps), progress)?;
            let mut grid = FieldGrid {
                from: spans[run[0]].from.clone(),
                to: spans[run[run.len() - 1]].to.clone(),
                rows,
                columns: grid_offsets.len(),
                deviation: Vec::new(),
                points: places
                    .iter()
                    .zip(&values)
                    .map(|(p, &(b, mj))| FieldPoint {
                        name: String::new(),
                        x: p.0[0],
                        y: p.0[1],
                        z: p.0[2],
                        offset: p.1,
                        height: p.0[2] - p.2,
                        b_ut: b * 1e6,
                        major_ut: mj * 1e6,
                        margin_ut: limit.map(|l| l - b * 1e6),
                    })
                    .collect(),
            };
            let ut = |ps: &[[f64; 3]]| -> Vec<f64> {
                m.field_block(ps).iter().map(|b| b.0 * 1e6).collect()
            };
            grid.deviation = settings
                .levels
                .iter()
                .map(|&l| {
                    let pts: Vec<[f64; 3]> = grid.isolines(l).into_iter().flatten().collect();
                    field_views::deviation(&pts, l, &ut)
                })
                .collect();
            grids.push(grid);
            continue;
        }
        for &i in run {
            check_cancel(progress)?;
            let span = &spans[i];
            let Some(axis) = axes[i].as_ref() else {
                continue;
            };
            let rows = (axis.length / grid_step).floor() as usize + 1;
            let mut places = Vec::new();
            for r in 0..rows {
                let t = r as f64 * grid_step;
                for &s in &grid_offsets {
                    let p = axis.plan(t, s);
                    let g = axis.ground(src.ground, t, s);
                    places.push(([p[0], p[1], g + settings.height], s, g, t));
                }
            }
            let values = match &model3d {
                Some(m) => {
                    let pts: Vec<[f64; 3]> = places.iter().map(|p| p.0).collect();
                    compute_all(&pts, &|ps: &[[f64; 3]]| m.field_block(ps), progress)?
                }
                None => {
                    let mut out = Vec::new();
                    for r in 0..rows {
                        let t = r as f64 * grid_step;
                        let cs = cross_section::cross_section(
                            src,
                            span,
                            StationAt::Fraction((t / axis.length).min(1.0)),
                        )?;
                        let groups = build_2d(line, &cs, &currents, &earth, settings.earth_wires);
                        for k in 0..grid_offsets.len() {
                            let p = &places[r * grid_offsets.len() + k];
                            out.push(field_2d(&groups, -p.1, p.0[2] - cs.ground.elevation));
                        }
                    }
                    out
                }
            };
            let mut grid = FieldGrid {
                from: span.from.clone(),
                to: span.to.clone(),
                rows,
                columns: grid_offsets.len(),
                deviation: Vec::new(),
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
            };
            if let Some(m) = &model3d {
                let ut = |ps: &[[f64; 3]]| -> Vec<f64> {
                    m.field_block(ps).iter().map(|b| b.0 * 1e6).collect()
                };
                grid.deviation = settings
                    .levels
                    .iter()
                    .map(|&l| {
                        let pts: Vec<[f64; 3]> = grid.isolines(l).into_iter().flatten().collect();
                        field_views::deviation(&pts, l, &ut)
                    })
                    .collect();
            }
            grids.push(grid);
        }
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
                compute_all(&pts, &|ps: &[[f64; 3]]| m.field_block(ps), progress)?
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
    // Cuts and volumes (3D).
    let mut cuts = Vec::new();
    let mut volumes = Vec::new();
    if let Some(m) = &model3d {
        let find = |from: &str, to: &str| {
            spans
                .iter()
                .position(|s| (s.from == from && s.to == to) || (s.from == to && s.to == from))
                .ok_or_else(|| format!("no span between structures {from} and {to}"))
        };
        let ut_of = |w: &Wires3d, ps: &[[f64; 3]]| -> Vec<f64> {
            w.field_block(ps).iter().map(|b| b.0 * 1e6).collect()
        };
        let full = |ps: &[[f64; 3]]| ut_of(m, ps);
        // The wires a view takes, and how much those left out change the
        // field at some of its points.
        let local = |path: &field_views::Path, width: f64| {
            settings.modelled_within.map(|d| m.within(path, width, d))
        };
        let left_out = |w: &Option<Wires3d>, pts: &[[f64; 3]]| -> Option<field_views::Deviation> {
            let w = w.as_ref()?;
            let every = pts.len().div_ceil(field_views::CHECKED).max(1);
            let picked: Vec<[f64; 3]> = pts.iter().step_by(every).copied().collect();
            let a = compute_all(&picked, &full, None).ok()?;
            let b = compute_all(&picked, &|ps: &[[f64; 3]]| ut_of(w, ps), None).ok()?;
            let mut d: Vec<f64> = a
                .iter()
                .zip(&b)
                .filter(|(x, _)| **x > 0.0)
                .map(|(x, y)| (x - y).abs() / x)
                .collect();
            d.sort_by(f64::total_cmp);
            let at = |q: f64| d[((d.len() - 1) as f64 * q).round() as usize];
            (!d.is_empty()).then(|| field_views::Deviation {
                points: d.len(),
                median: at(0.5),
                p90: at(0.9),
                max: at(1.0),
            })
        };
        for c in &settings.cuts {
            check_cancel(progress)?;
            let i = find(&c.from, &c.to)?;
            let axis = axes[i]
                .as_ref()
                .ok_or_else(|| format!("span {}–{}: no axis", c.from, c.to))?;
            let t = match c.fraction {
                Some(f) if (0.0..=1.0).contains(&f) => f * axis.length,
                Some(f) => return Err(format!("a place in the span from 0 to 1, not {f}")),
                None => lowest_station(src, &spans[i], axis),
            };
            let n = (2.0 * c.width / c.step).ceil().max(1.0) as usize;
            let low = (0..=n)
                .map(|k| {
                    axis.ground(
                        src.ground,
                        t,
                        -c.width + 2.0 * c.width * k as f64 / n as f64,
                    )
                })
                .fold(f64::INFINITY, f64::min);
            let cut = VerticalCut {
                origin: axis.plan(t, 0.0),
                right: axis.right,
                left: -c.width,
                right_offset: c.width,
                bottom: low.floor(),
                top: highest_wire(src, &spans[i]).unwrap_or(low + 50.0) + c.above,
                step: c.step,
            };
            let ground = |x: f64, y: f64| {
                let (t, s) = axis.local(&[x, y, 0.0]);
                Some(axis.ground(src.ground, t, s))
            };
            if let Some(p) = progress {
                p.total.fetch_add(cut.points(), Ordering::Relaxed);
            }
            let a = cut.at(-c.width);
            let b = cut.at(c.width);
            let w = local(
                &field_views::Path::new(vec![[a[0], a[1], 0.0], [b[0], b[1], 0.0]])?,
                0.0,
            );
            let wires = w.as_ref().unwrap_or(m);
            let values = field_views::vertical_cut(
                &cut,
                &|ps: &[[f64; 3]]| ut_of(wires, ps),
                Some(&ground),
                progress,
            )?;
            let pts: Vec<[f64; 3]> = values
                .elevations
                .iter()
                .flat_map(|&z| {
                    values.offsets.iter().map(move |&s| {
                        let p = cut.at(s);
                        [p[0], p[1], z]
                    })
                })
                .collect();
            cuts.push(FieldCut {
                from: spans[i].from.clone(),
                to: spans[i].to.clone(),
                at: t,
                pieces: wires.count(),
                left_out: left_out(&w, &pts),
                values,
            });
        }
        let pieces = if settings.volumes.is_empty() {
            Vec::new()
        } else {
            m.segments()
        };
        for v in &settings.volumes {
            check_cancel(progress)?;
            let run = run_spans(&spans, &v.from, &v.to).map_err(|e| format!("volume: {e}"))?;
            let path = run_path(src, &spans, &run)?;
            let mut low = f64::INFINITY;
            let (nt, ns) = (
                (path.length() / 5.0).ceil() as usize,
                (2.0 * v.width / 5.0).ceil() as usize,
            );
            for a in 0..=nt {
                for b in 0..=ns {
                    let t = path.length() * a as f64 / nt.max(1) as f64;
                    let s = -v.width + 2.0 * v.width * b as f64 / ns.max(1) as f64;
                    let p = path.place(t, s);
                    let g = src
                        .ground
                        .and_then(|g| g(p[0], p[1]))
                        .unwrap_or_else(|| path.ground(t));
                    low = low.min(g);
                }
            }
            let high = run
                .iter()
                .filter_map(|&i| highest_wire(src, &spans[i]))
                .reduce(f64::max);
            let bounds = VolumeBox {
                path,
                width: v.width,
                bottom: low.floor(),
                top: high.unwrap_or(low + 50.0) + v.above,
                step: v.step,
            };
            let w = local(&bounds.path, bounds.width);
            let wires = w.as_ref().unwrap_or(m);
            let values = field_views::volume(
                &bounds,
                &settings.levels,
                &|ps: &[[f64; 3]]| ut_of(wires, ps),
                &pieces,
                progress,
            )?;
            let pts: Vec<[f64; 3]> = values
                .surfaces
                .iter()
                .flat_map(|(_, mesh)| mesh.vertices.iter().copied())
                .collect();
            volumes.push(FieldVolume {
                from: spans[run[0]].from.clone(),
                to: spans[run[run.len() - 1]].to.clone(),
                pieces: wires.count(),
                left_out: left_out(&w, &pts),
                values,
            });
        }
    }
    let pieces = model3d.as_ref().and_then(|m| m.pieces);
    if let Some(p) = &pieces {
        notes.push(format!(
            "3D: {} straight pieces, the longest {:.2} m; a wire lies at most {:.1} mm off its pieces{}",
            p.count,
            p.longest,
            p.off_wire * 1e3,
            if settings.piece_length.is_some() {
                format!(
                    " (cut from the solver's pieces; of which {:.3} mm the estimate of placing the new points on the curve)",
                    p.interpolation * 1e3
                )
            } else {
                " (the solver's pieces)".into()
            }
        ));
    }
    Ok((
        MagneticField {
            model: settings.model,
            height: settings.height,
            limit_ut: limit,
            currents: used,
            earth_wires: earth_rows,
            towers,
            profiles,
            grids,
            points,
            cuts,
            volumes,
            pieces,
            notes,
        },
        model3d,
    ))
}

/// The spans from structure `from` to structure `to` along the line, in
/// order (consecutive spans, each starting where the one before ends); a
/// single span also either way round.
fn run_spans(spans: &[LineSpan], from: &str, to: &str) -> Result<Vec<usize>, String> {
    for (i, first) in spans.iter().enumerate() {
        if first.from != from {
            continue;
        }
        let mut run = vec![i];
        let mut k = i;
        loop {
            if spans[k].to == to {
                return Ok(run);
            }
            if k + 1 >= spans.len() || spans[k + 1].from != spans[k].to {
                break;
            }
            k += 1;
            run.push(k);
        }
    }
    spans
        .iter()
        .position(|s| s.from == to && s.to == from)
        .map(|i| vec![i])
        .ok_or_else(|| format!("no run of spans from structure {from} to {to} along the line"))
}

/// The path along a run of spans: the structures' places and the ground
/// at their feet.
fn run_path(
    src: &Sources<'_>,
    spans: &[LineSpan],
    run: &[usize],
) -> Result<field_views::Path, String> {
    let at = |id: &str| -> Result<[f64; 3], String> {
        src.line
            .structure(id)
            .map(|s| s.position)
            .ok_or_else(|| format!("structure {id} not in the line"))
    };
    let mut points = vec![at(&spans[run[0]].from)?];
    for &i in run {
        points.push(at(&spans[i].to)?);
    }
    field_views::Path::new(points)
}

/// The highest point of a span's wires, m.
fn highest_wire(src: &Sources<'_>, span: &LineSpan) -> Option<f64> {
    span.wires
        .iter()
        .filter_map(|&(k, j)| {
            let id = &src.line.sections[k].id;
            src.shapes
                .iter()
                .find(|s| &s.section == id)
                .and_then(|s| s.spans.get(j))
        })
        .flat_map(|sp| sp.points.iter().map(|p| p[2]))
        .reduce(f64::max)
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
    for c in &m.cuts {
        let v = &c.values;
        let nc = v.offsets.len();
        for (r, &z) in v.elevations.iter().enumerate() {
            for (k, &s) in v.offsets.iter().enumerate() {
                let xy = v.cut.at(s);
                let b = v.values[r * nc + k];
                let p = FieldPoint {
                    name: String::new(),
                    x: xy[0],
                    y: xy[1],
                    z,
                    offset: s,
                    height: v.ground[k].map_or(f64::NAN, |g| z - g),
                    b_ut: b,
                    major_ut: f64::NAN,
                    margin_ut: m.limit_ut.map(|l| l - b),
                };
                row(&mut out, "cut", &c.from, &c.to, Some(c.at), &p);
            }
        }
    }
    out
}

/// The volumes' isosurfaces as a Wavefront OBJ file: one object per
/// volume and level, world coordinates (m).
pub fn obj(m: &MagneticField) -> String {
    let mut out = String::from("# OpenOHL magnetic field isosurfaces (µT)\n");
    let mut base = 1usize;
    for v in &m.volumes {
        for (level, mesh) in &v.values.surfaces {
            let _ = writeln!(out, "o span_{}_{}_{level}uT", v.from, v.to);
            for p in &mesh.vertices {
                let _ = writeln!(out, "v {:.3} {:.3} {:.3}", p[0], p[1], p[2]);
            }
            for t in &mesh.triangles {
                let _ = writeln!(
                    out,
                    "f {} {} {}",
                    base + t[0] as usize,
                    base + t[1] as usize,
                    base + t[2] as usize
                );
            }
            base += mesh.vertices.len();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The per-polyline sum is the sum of [`segment_field`] over its
    /// pieces (within rounding), at points near and far.
    #[test]
    fn polylines_sum_as_segments() {
        let pts: Vec<[f64; 3]> = (0..=40)
            .map(|k| {
                let x = k as f64 * 10.0;
                [x, 0.3 * x, 30.0 + (x - 200.0).powi(2) / 3000.0]
            })
            .collect();
        let i = C::new(700.0, -200.0);
        let mut w = Wires3d::default();
        w.push(50.0, &pts, i);
        let g = Group {
            frequency_hz: 50.0,
            segments: pts
                .windows(2)
                .map(|s| Segment {
                    a: s[0],
                    b: s[1],
                    current: i,
                })
                .collect(),
        };
        for p in [
            [150.0, 50.0, 1.0],
            [10.0, 3.0, 29.0],
            [-500.0, 20.0, 0.0],
            pts[7],
        ] {
            let (a, b) = (w.field(p), field_3d(std::slice::from_ref(&g), p));
            assert!((a.0 - b.0).abs() <= 1e-12 * b.0.max(1e-30), "{a:?} {b:?}");
            assert!((a.1 - b.1).abs() <= 1e-12 * b.1.max(1e-30), "{a:?} {b:?}");
        }
        assert!(seconds_per_piece_point() > 0.0);
    }

    /// Cutting a catenary's pieces: the new points lie on the curve far
    /// closer than the chords; at a kink the chord is kept.
    #[test]
    fn refined_pieces_lie_on_the_curve() {
        let c = 1500.0;
        let z = |x: f64| c * ((x - 200.0) / c).cosh();
        let pts: Vec<[f64; 3]> = (0..=64)
            .map(|k| {
                let x = k as f64 * 400.0 / 64.0;
                [x, 0.0, z(x)]
            })
            .collect();
        let (fine, est, parts) = refine(&pts, 1.0, 0.01);
        assert_eq!(parts, 7);
        assert_eq!(fine.len(), 64 * 7 + 1);
        let worst = fine
            .iter()
            .map(|p| (p[2] - z(p[0])).abs())
            .fold(0.0, f64::max);
        // The chords lie up to 6.25² / (8 · 1500) = 3.3 mm off; the new
        // points some µm.
        assert!(worst < 1e-5, "{worst}");
        assert!(est < 1e-5, "{est}");
        // A kink in the middle: the pieces next to it stay chords.
        let mut kinked = pts.clone();
        for p in kinked.iter_mut().skip(32) {
            p[2] += (p[0] - 200.0) * 0.2;
        }
        let (fine, _, _) = refine(&kinked, 1.0, 0.01);
        let k = fine
            .iter()
            .position(|p| (p[0] - 200.0).abs() < 1e-9)
            .unwrap();
        let (a, m, b) = (fine[k - 7], fine[k - 3], fine[k]);
        let t = (m[0] - a[0]) / (b[0] - a[0]);
        assert!((m[2] - (a[2] + t * (b[2] - a[2]))).abs() < 1e-9);
    }

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
