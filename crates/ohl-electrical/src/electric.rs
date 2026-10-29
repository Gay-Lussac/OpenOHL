// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The electric field of the line (roadmap O18 step 5; built, in active
//! development, not approved yet; owner, 2026-10-29: "E field comes after
//! this").
//!
//! **Charges.** Per span, from its mean cross-section: the potential
//! coefficients of the line parameters (a perfectly conducting earth, air
//! of permittivity \\( \varepsilon_0 \\)) and the voltages to earth — each
//! phase its system's voltage between phases over \\( \sqrt 3 \\) at its
//! phase angle, the earth wires at zero — give the charge per metre of
//! every wire, \\( q = P^{-1} V \\) (a bundle as its equivalent wire, the
//! charge shared equally by its subconductors).
//!
//! **3D** ([`Model::ThreeD`]): every wire as solved, a bundle
//! subconductor by subconductor, and the jumpers (at their phase's
//! charge per metre), as straight pieces of uniform charge; the field of a
//! piece in closed form ([`segment_field`]). The ground: at every point a
//! plane fitted to the terrain under it ([`Settings::ground_radius`]) and
//! the images of all charges in it (owner, 2026-10-29: continuous along the
//! line, exact for level or evenly sloping ground — a simplification on
//! uneven ground). **2D** ([`Model::TwoD`]): the cross-section's wires as
//! infinite line charges over level ground at the cross-section.
//!
//! Reported: the rms of the vector
//! \\( E = \sqrt{|E_x|^2 + |E_y|^2 + |E_z|^2} \\), kV/m; systems of
//! different frequencies added as \\( \sqrt{\sum_f E_f^2} \\). DC systems
//! left out (a static field and space charge: another calculation).
//! **Not modelled**: the structures (earthed steel: they lower the field
//! near them), trees, buildings and people (objects at earth potential
//! shield the field: conservative away from them, not at their tops),
//! the ground's resistivity (the earth a perfect conductor at power
//! frequency). A limit is optional input (kV/m), never built in.

use std::collections::BTreeMap;
use std::f64::consts::PI;
use std::sync::atomic::Ordering;

use nalgebra::{Complex, DVector};
use ohl_line::electrical::SystemKind;
use serde::Serialize;

use crate::cross_section::{self, Axis, CrossSection, PhaseData, Sources, StationAt};
use crate::earth::EPS0;
use crate::field_views::{self, CutValues, VerticalCut, VolumeBox, VolumeValues, compute_all};
use crate::magnetic::{self, GivenPoint, Model, Progress, StationSpec};
use crate::matrices::{self, Wire};
use crate::parameters::{CircuitKey, DEFAULT_FREQUENCY_HZ};

/// A complex number.
pub type C = Complex<f64>;

/// \\( 1/(4\pi\varepsilon_0) \\), m/F.
const K: f64 = 1.0 / (4.0 * PI * EPS0);

// ---------------------------------------------------------------------------
// The closed forms.

/// The field at `p` of a charge of 1 C/m spread evenly along the straight
/// piece from `a` to `b`, V/m:
///
/// \\[ \vec E = \frac{1}{4\pi\varepsilon_0} \left(
///     \frac{\vec d}{\rho^2}\left(\frac{L - t}{|\vec r_b|} + \frac{t}{|\vec r_a|}\right)
///     + \hat u \left(\frac{1}{|\vec r_b|} - \frac{1}{|\vec r_a|}\right) \right) \\]
///
/// with \\( \hat u \\) the piece's direction, \\( L \\) its length,
/// \\( t = (p - a)\cdot\hat u \\), \\( \vec d \\) the perpendicular from the
/// piece's line to \\( p \\) of length \\( \rho \\), \\( \vec r_a = p - a \\),
/// \\( \vec r_b = p - b \\) (Coulomb's law integrated along the piece).
/// Zero on the piece's line and for a piece of no length.
pub fn segment_field(a: [f64; 3], b: [f64; 3], p: [f64; 3]) -> [f64; 3] {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let len = (ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2]).sqrt();
    if len < 1e-12 {
        return [0.0; 3];
    }
    let u = ab.map(|x| x / len);
    let ra = [p[0] - a[0], p[1] - a[1], p[2] - a[2]];
    let rb = [p[0] - b[0], p[1] - b[1], p[2] - b[2]];
    let t = ra[0] * u[0] + ra[1] * u[1] + ra[2] * u[2];
    let d = [ra[0] - t * u[0], ra[1] - t * u[1], ra[2] - t * u[2]];
    let rho2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    if rho2 < 1e-18 {
        return [0.0; 3];
    }
    let ia = 1.0 / (ra[0] * ra[0] + ra[1] * ra[1] + ra[2] * ra[2]).sqrt();
    let ib = 1.0 / (rb[0] * rb[0] + rb[1] * rb[1] + rb[2] * rb[2]).sqrt();
    let perp = ((len - t) * ib + t * ia) / rho2;
    let axial = ib - ia;
    std::array::from_fn(|k| K * (d[k] * perp + u[k] * axial))
}

/// The field at `(y, z)` of a charge of 1 C/m on an infinite line along x
/// through `(yk, zk)` and its image at `(yk, −zk)` in level ground at
/// z = 0, V/m: \\( \frac{1}{2\pi\varepsilon_0}\left(\frac{\vec r}{r^2} -
/// \frac{\vec r'}{r'^2}\right) \\). Returns (E_y, E_z).
pub fn line_field(yk: f64, zk: f64, y: f64, z: f64) -> [f64; 2] {
    let k = 2.0 * K;
    let (dy, dz) = (y - yk, z - zk);
    let (iy, iz) = (y - yk, z + zk);
    let r2 = (dy * dy + dz * dz).max(1e-18);
    let i2 = (iy * iy + iz * iz).max(1e-18);
    [k * (dy / r2 - iy / i2), k * (dz / r2 - iz / i2)]
}

/// The rms of a phasor vector, V/m.
fn rms(e: &[C]) -> f64 {
    e.iter().map(|c| c.norm_sqr()).sum::<f64>().sqrt()
}

// ---------------------------------------------------------------------------
// The 3D model.

/// A polyline of uniform charge per metre.
#[derive(Debug, Clone, Default)]
struct Poly {
    x: Vec<f64>,
    y: Vec<f64>,
    z: Vec<f64>,
    ux: Vec<f64>,
    uy: Vec<f64>,
    uz: Vec<f64>,
    len: Vec<f64>,
    charge: C,
}

impl Poly {
    fn new(points: &[[f64; 3]], charge: C) -> Self {
        let mut p = Poly {
            charge,
            ..Default::default()
        };
        for q in points {
            p.x.push(q[0]);
            p.y.push(q[1]);
            p.z.push(q[2]);
        }
        for w in points.windows(2) {
            let d = [w[1][0] - w[0][0], w[1][1] - w[0][1], w[1][2] - w[0][2]];
            let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
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

    /// \\( \sum \\) of [`segment_field`] over the pieces at `p` (per C/m),
    /// each vertex's distance computed once.
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
            let perp = ((self.len[k] - t) * inv[k + 1] + t * inv[k]) / rho2;
            let axial = inv[k + 1] - inv[k];
            sx += dx * perp + ux * axial;
            sy += dy * perp + uy * axial;
            sz += dz * perp + uz * axial;
        }
        [sx * K, sy * K, sz * K]
    }
}

/// The ground under a point as a plane: a point of it and its unit
/// normal (upward).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundPlane {
    /// A point of the plane.
    pub point: [f64; 3],
    /// Its unit normal, upward.
    pub normal: [f64; 3],
}

impl GroundPlane {
    /// The mirror image of `p` in the plane.
    pub fn mirror(&self, p: [f64; 3]) -> [f64; 3] {
        let n = self.normal;
        let h = (p[0] - self.point[0]) * n[0]
            + (p[1] - self.point[1]) * n[1]
            + (p[2] - self.point[2]) * n[2];
        [
            p[0] - 2.0 * h * n[0],
            p[1] - 2.0 * h * n[1],
            p[2] - 2.0 * h * n[2],
        ]
    }

    /// The mirror image of a vector.
    fn reflect(&self, v: [f64; 3]) -> [f64; 3] {
        let n = self.normal;
        let h = v[0] * n[0] + v[1] * n[1] + v[2] * n[2];
        [
            v[0] - 2.0 * h * n[0],
            v[1] - 2.0 * h * n[1],
            v[2] - 2.0 * h * n[2],
        ]
    }

    /// The plane through the ground at (x, y) fitted over `radius`: the
    /// elevation there and the slopes from the elevations `radius` either
    /// side in x and in y.
    pub fn fitted(ground: &dyn Fn(f64, f64) -> f64, x: f64, y: f64, radius: f64) -> Self {
        let z = ground(x, y);
        let r = radius.max(1e-3);
        let gx = (ground(x + r, y) - ground(x - r, y)) / (2.0 * r);
        let gy = (ground(x, y + r) - ground(x, y - r)) / (2.0 * r);
        let n = [-gx, -gy, 1.0];
        let l = (n[0] * n[0] + n[1] * n[1] + 1.0).sqrt();
        Self {
            point: [x, y, z],
            normal: n.map(|c| c / l),
        }
    }
}

/// The line's charges in 3D as straight pieces, by frequency.
#[derive(Debug, Clone, Default)]
pub struct Charges3d {
    groups: Vec<(f64, Vec<Poly>)>,
}

impl Charges3d {
    fn push(&mut self, frequency_hz: f64, points: &[[f64; 3]], charge: C) {
        if points.len() < 2 {
            return;
        }
        let poly = Poly::new(points, charge);
        match self.groups.iter_mut().find(|g| g.0 == frequency_hz) {
            Some(g) => g.1.push(poly),
            None => self.groups.push((frequency_hz, vec![poly])),
        }
    }

    /// Per frequency, the field of the charges alone at `p` (no images).
    fn sums(&self, p: [f64; 3], inv: &mut Vec<f64>) -> Vec<[C; 3]> {
        self.groups
            .iter()
            .map(|(_, polys)| {
                let mut e = [C::new(0.0, 0.0); 3];
                for poly in polys {
                    let f = poly.field(p, inv);
                    for k in 0..3 {
                        e[k] += poly.charge * f[k];
                    }
                }
                e
            })
            .collect()
    }

    /// E (rms) at `p` with the charges' images in `plane`, V/m. The
    /// images' field at `p` is the charges' field at the mirror point,
    /// mirrored back (a reflection keeps distances): with the opposite
    /// sign.
    pub fn field(&self, p: [f64; 3], plane: &GroundPlane) -> f64 {
        let mut inv = Vec::new();
        let direct = self.sums(p, &mut inv);
        let mirror = self.sums(plane.mirror(p), &mut inv);
        let mut sum = 0.0;
        for (d, m) in direct.iter().zip(&mirror) {
            let re = plane.reflect([m[0].re, m[1].re, m[2].re]);
            let im = plane.reflect([m[0].im, m[1].im, m[2].im]);
            let e: [C; 3] = std::array::from_fn(|k| d[k] - C::new(re[k], im[k]));
            let r = rms(&e);
            sum += r * r;
        }
        sum.sqrt()
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

// ---------------------------------------------------------------------------
// Inputs.

/// The voltage of a circuit.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CircuitVoltage {
    /// The circuit: its name, or its system's id (`*`: every circuit).
    pub circuit: String,
    /// Voltage between phases, kV (rms); the phases at a third of it
    /// times \\( \sqrt 3 \\) to earth.
    pub kv: f64,
}

/// What to compute.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Settings {
    /// 3D or 2D.
    pub model: Model,
    /// The circuits' voltages; a circuit not given: its system's nominal
    /// voltage (said: an ordinance may ask the highest operating voltage).
    pub voltages: Vec<CircuitVoltage>,
    /// The profiles' height above the ground, m.
    pub height: f64,
    /// The profiles reach this far to either side of the axis, m.
    pub width: f64,
    /// Between profile points, m.
    pub step: f64,
    /// The profiles; empty: every span at its lowest point.
    pub stations: Vec<StationSpec>,
    /// Given points.
    pub points: Vec<GivenPoint>,
    /// A limit, kV/m (optional; none built in).
    pub limit_kv_m: Option<f64>,
    /// The ground plane under a point is fitted over this distance either
    /// side, m (3D).
    pub ground_radius: f64,
    /// The jumpers carry their phase's charge (3D).
    pub jumpers: bool,
    /// The longest straight piece, m (3D; `None`: the solver's).
    pub piece_length: Option<f64>,
    /// Vertical cuts across the line (3D).
    pub cuts: Vec<magnetic::CutSpec>,
    /// Volumes over runs of spans (3D).
    pub volumes: Vec<magnetic::VolumeSpec>,
    /// The levels drawn, kV/m, ascending: display only.
    pub levels: Vec<f64>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model: Model::ThreeD,
            voltages: Vec::new(),
            height: 1.0,
            width: 50.0,
            step: 1.0,
            stations: Vec::new(),
            points: Vec::new(),
            limit_kv_m: None,
            ground_radius: 10.0,
            jumpers: true,
            piece_length: None,
            cuts: Vec::new(),
            volumes: Vec::new(),
            levels: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Results.

/// The field at one point.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldPoint {
    /// Its name (given points), else empty.
    pub name: String,
    /// Easting, m.
    pub x: f64,
    /// Northing, m.
    pub y: f64,
    /// Elevation, m.
    pub z: f64,
    /// To the right of the span's axis, m.
    pub offset: f64,
    /// Above the ground, m.
    pub height: f64,
    /// E (rms of the vector), kV/m.
    pub e_kv_m: f64,
    /// The limit minus E, kV/m (with a limit).
    pub margin_kv_m: Option<f64>,
}

/// A profile across the line.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Profile {
    /// The span's first structure.
    pub from: String,
    /// Its second.
    pub to: String,
    /// The station, m from the first structure.
    pub at: f64,
    /// The points, left to right.
    pub points: Vec<FieldPoint>,
    /// The largest E, kV/m.
    pub max_kv_m: f64,
    /// Where, m to the right of the axis.
    pub max_offset: f64,
}

/// The charge per metre of a wire in a span.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WireCharge {
    /// The section.
    pub section: String,
    /// The span's structures.
    pub from: String,
    /// The second.
    pub to: String,
    /// Frequency, Hz.
    pub frequency_hz: f64,
    /// The voltage to earth, kV (rms).
    pub kv: f64,
    /// Charge per metre, nC/m (rms), the whole bundle's.
    pub nc_per_m: f64,
}

/// The electric field of a line.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ElectricField {
    /// 3D or 2D.
    pub model: Model,
    /// The profiles' height above the ground, m.
    pub height: f64,
    /// The limit, kV/m.
    pub limit_kv_m: Option<f64>,
    /// The wires' charges per span.
    pub charges: Vec<WireCharge>,
    /// The profiles.
    pub profiles: Vec<Profile>,
    /// The given points.
    pub points: Vec<FieldPoint>,
    /// The vertical cuts (kV/m).
    pub cuts: Vec<magnetic::FieldCut>,
    /// The volumes (levels in kV/m).
    pub volumes: Vec<magnetic::FieldVolume>,
    /// What was assumed or left out.
    pub notes: Vec<String>,
}

// ---------------------------------------------------------------------------
// The run.

/// A phase's voltage to earth, V (rms phasor), and its frequency; `None`:
/// DC or no voltage.
fn phase_voltage(
    p: &PhaseData,
    given: &[CircuitVoltage],
    defaulted: &mut BTreeMap<String, f64>,
) -> Option<(f64, C)> {
    if p.kind == SystemKind::Dc {
        return None;
    }
    let key = CircuitKey {
        circuit: p.circuit.clone(),
        system: p.system.clone(),
    };
    let kv = match given
        .iter()
        .rev()
        .find(|g| magnetic::matches(&key, &g.circuit))
    {
        Some(g) => g.kv,
        None => {
            defaulted.insert(key.label(), p.voltage_kv);
            p.voltage_kv
        }
    };
    let f = p.frequency_hz.unwrap_or(DEFAULT_FREQUENCY_HZ);
    let v = kv * 1e3 / 3f64.sqrt();
    Some((f, C::from_polar(v, p.angle_deg.to_radians())))
}

/// The charges per metre of a cross-section's wires (the whole bundle's),
/// per frequency: \\( q = P^{-1} V \\), the earth wires at zero voltage and
/// the phases of another frequency at zero too (each frequency apart).
fn cross_section_charges(
    cs: &CrossSection,
    given: &[CircuitVoltage],
    defaulted: &mut BTreeMap<String, f64>,
    notes: &mut Vec<String>,
) -> Vec<(f64, Vec<C>)> {
    let mut wires = Vec::new();
    let mut volts = Vec::new();
    for w in &cs.wires {
        let r = 0.5 * w.wire.diameter;
        if w.height <= r {
            notes.push(format!(
                "section {} span {}: at or below the ground, left out of the charges",
                w.section,
                w.span + 1
            ));
            return Vec::new();
        }
        let (_, radius) = matrices::bundle(w.bundle.count.max(1), w.bundle.spacing, r, r);
        wires.push(Wire {
            across: w.offset,
            height: w.height,
            gmr: radius,
            radius,
            resistance: 0.0,
        });
        volts.push(
            w.phase
                .as_ref()
                .and_then(|p| phase_voltage(p, given, defaulted)),
        );
    }
    if wires.is_empty() {
        return Vec::new();
    }
    let Some(inv) = matrices::potential(&wires).try_inverse() else {
        notes.push(format!(
            "span {}–{}: the potential coefficients are singular; no charges",
            cs.station.from, cs.station.to
        ));
        return Vec::new();
    };
    let inv = matrices::complex(&inv);
    let mut freqs: Vec<f64> = volts.iter().flatten().map(|v| v.0).collect();
    freqs.sort_by(f64::total_cmp);
    freqs.dedup();
    freqs
        .into_iter()
        .map(|f| {
            let v = DVector::from_iterator(
                volts.len(),
                volts.iter().map(|x| match x {
                    Some((g, v)) if *g == f => *v,
                    _ => C::new(0.0, 0.0),
                }),
            );
            let q = &inv * v;
            (f, q.iter().copied().collect())
        })
        .collect()
}

/// Charges per (section id, span index): per frequency, per metre, the
/// whole bundle's.
type ChargeMap = BTreeMap<(String, usize), Vec<(f64, C)>>;

/// The 3D model: every wire's span with its charge per metre (shared by
/// the subconductors), and the jumpers at their phase's.
fn build_3d(
    src: &Sources<'_>,
    charges: &ChargeMap,
    settings: &Settings,
    notes: &mut Vec<String>,
) -> Charges3d {
    let line = src.line;
    let mut out = Charges3d::default();
    for section in &line.sections {
        let Some(shape) = src.shapes.iter().find(|s| s.section == section.id) else {
            continue;
        };
        for (j, sp) in shape.spans.iter().enumerate() {
            let Some(list) = charges.get(&(section.id.clone(), j)) else {
                continue;
            };
            if sp.points.len() < 2 {
                continue;
            }
            let centre = match settings.piece_length {
                Some(max) => magnetic::refine(&sp.points, max, sp.polyline_error).0,
                None => sp.points.clone(),
            };
            let subs = section.subconductor_polylines(&centre);
            let n = subs.len().max(1) as f64;
            for &(f, q) in list {
                for poly in &subs {
                    out.push(f, poly, q / n);
                }
            }
        }
    }
    if settings.jumpers {
        let mut left_out = Vec::new();
        for shape in src.jumpers {
            let Some(j) = line.jumpers.iter().find(|j| j.id == shape.jumper) else {
                continue;
            };
            // The charge per metre of the section at its first end.
            let end = [&j.from, &j.to].into_iter().find_map(|e| {
                let p = e.point()?;
                line.sections.iter().find_map(|sec| {
                    let n = sec.points.len();
                    if n < 2 {
                        return None;
                    }
                    if sec.points[0] == *p {
                        charges.get(&(sec.id.clone(), 0))
                    } else if sec.points[n - 1] == *p {
                        charges.get(&(sec.id.clone(), n - 2))
                    } else {
                        None
                    }
                })
            });
            match end {
                Some(list) => {
                    for &(f, q) in list {
                        out.push(f, &shape.points, q);
                    }
                }
                None => left_out.push(j.id.clone()),
            }
        }
        if !left_out.is_empty() {
            notes.push(format!(
                "jumpers without a charge (not at a section's end): {}",
                left_out.join(", ")
            ));
        }
    }
    out
}

/// The electric field of the line solved in one weather case (`src`), as
/// `settings` ask. `progress`: to follow and stop it from another thread.
pub fn run(
    src: &Sources<'_>,
    settings: &Settings,
    progress: Option<&Progress>,
) -> Result<ElectricField, String> {
    let line = src.line;
    if !(settings.step > 0.0 && settings.width >= 0.0 && settings.height.is_finite()) {
        return Err("a step above 0 m and a width of at least 0 m".into());
    }
    if let Some(l) = settings.limit_kv_m
        && !(l.is_finite() && l > 0.0)
    {
        return Err(format!("a limit above 0 kV/m, not {l}"));
    }
    if !(settings.ground_radius.is_finite() && settings.ground_radius > 0.0) {
        return Err("a ground plane radius above 0 m".into());
    }
    if settings.model == Model::TwoD && !(settings.cuts.is_empty() && settings.volumes.is_empty()) {
        return Err("cuts and volumes are computed in 3D".into());
    }
    let mut notes = Vec::new();
    let spans = cross_section::line_spans(line);
    if spans.is_empty() {
        return Err("the line has no spans".into());
    }
    let axes: Vec<Option<Axis>> = spans.iter().map(|s| Axis::of(line, s).ok()).collect();
    // Charges per span from its mean cross-section.
    let (means, mean_notes) = cross_section::span_cross_sections(src, line.electrical.mean_height);
    notes.extend(mean_notes);
    let mut defaulted = BTreeMap::new();
    let mut charges = ChargeMap::new();
    let mut rows = Vec::new();
    for (span, cs) in &means {
        for (f, q) in cross_section_charges(cs, &settings.voltages, &mut defaulted, &mut notes) {
            for (w, qk) in cs.wires.iter().zip(q) {
                charges
                    .entry((w.section.clone(), w.span))
                    .or_default()
                    .push((f, qk));
                rows.push(WireCharge {
                    section: w.section.clone(),
                    from: span.from.clone(),
                    to: span.to.clone(),
                    frequency_hz: f,
                    kv: w
                        .phase
                        .as_ref()
                        .and_then(|p| phase_voltage(p, &settings.voltages, &mut BTreeMap::new()))
                        .filter(|v| v.0 == f)
                        .map_or(0.0, |v| v.1.norm() / 1e3),
                    nc_per_m: qk.norm() * 1e9,
                });
            }
        }
    }
    for (c, kv) in &defaulted {
        notes.push(format!(
            "{c}: no voltage given, its system's nominal {kv} kV taken (an ordinance may ask the highest operating voltage: give it)"
        ));
    }
    if line.systems.iter().any(|s| s.kind == SystemKind::Dc) {
        notes.push("DC systems left out (a static field with space charge; not computed)".into());
    }
    notes.push(
        "the structures, trees and buildings not modelled (earthed objects lower the field near them and raise it at their tops); the earth a perfect conductor".into(),
    );
    let model3d =
        (settings.model == Model::ThreeD).then(|| build_3d(src, &charges, settings, &mut notes));
    if let Some(m) = &model3d {
        notes.push(format!(
            "3D: {} straight pieces of charge; the ground under each point a plane fitted over ±{} m, the charges' images in it (exact for level or evenly sloping ground, a simplification on uneven ground)",
            m.count(),
            settings.ground_radius
        ));
    }
    // The ground: the terrain's, else straight between the structures'
    // feet and level across, by the nearest span.
    let ground = |x: f64, y: f64| -> f64 {
        if let Some(z) = src.ground.and_then(|g| g(x, y)) {
            return z;
        }
        let mut best: Option<(f64, f64)> = None;
        for a in axes.iter().flatten() {
            let (t, s) = a.local(&[x, y, 0.0]);
            let tc = t.clamp(0.0, a.length);
            let d = (t - tc).hypot(s);
            if best.is_none_or(|b| d < b.0) {
                best = Some((d, a.ground(None, tc, 0.0)));
            }
        }
        best.map_or(0.0, |b| b.1)
    };
    let radius = settings.ground_radius;
    let e_at = |ps: &[[f64; 3]]| -> Vec<f64> {
        let m = model3d.as_ref().expect("3D");
        ps.iter()
            .map(|&p| m.field(p, &GroundPlane::fitted(&ground, p[0], p[1], radius)))
            .collect()
    };
    let limit = settings.limit_kv_m;
    // Stations.
    let mut stations: Vec<(usize, f64)> = Vec::new();
    if settings.stations.is_empty() {
        for (i, s) in spans.iter().enumerate() {
            if let Some(a) = &axes[i] {
                stations.push((i, magnetic::lowest_station(src, s, a)));
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
                None => magnetic::lowest_station(src, &spans[i], a),
            };
            stations.push((i, t));
        }
    }
    let n_across = (2.0 * settings.width / settings.step).round() as usize + 1;
    let offsets: Vec<f64> = (0..n_across)
        .map(|k| -settings.width + k as f64 * settings.step)
        .collect();
    if let Some(p) = progress {
        p.total.store(
            stations.len() * n_across + settings.points.len(),
            Ordering::Relaxed,
        );
        p.done.store(0, Ordering::Relaxed);
    }
    let mut profiles = Vec::new();
    for &(i, t) in &stations {
        magnetic::check_cancel(progress)?;
        let span = &spans[i];
        let axis = axes[i].as_ref().expect("an axis");
        let places: Vec<([f64; 3], f64)> = offsets
            .iter()
            .map(|&s| {
                let p = axis.plan(t, s);
                let g = axis.ground(src.ground, t, s);
                ([p[0], p[1], g + settings.height], g)
            })
            .collect();
        let values: Vec<f64> = match &model3d {
            Some(_) => {
                let pts: Vec<[f64; 3]> = places.iter().map(|p| p.0).collect();
                compute_all(&pts, &e_at, progress)?
            }
            None => {
                let cs =
                    cross_section::cross_section(src, span, StationAt::Fraction(t / axis.length))?;
                let lines = charges_2d(&cs, &charges);
                let g0 = cs.ground.elevation;
                if let Some(pr) = progress {
                    pr.done.fetch_add(offsets.len(), Ordering::Relaxed);
                }
                offsets
                    .iter()
                    .zip(&places)
                    .map(|(&s, p)| field_2d(&lines, -s, p.0[2] - g0))
                    .collect()
            }
        };
        let points: Vec<FieldPoint> = offsets
            .iter()
            .zip(&places)
            .zip(&values)
            .map(|((&s, (p, g)), &e)| FieldPoint {
                name: String::new(),
                x: p[0],
                y: p[1],
                z: p[2],
                offset: s,
                height: p[2] - g,
                e_kv_m: e / 1e3,
                margin_kv_m: limit.map(|l| l - e / 1e3),
            })
            .collect();
        let (max_offset, max_kv_m) =
            points
                .iter()
                .map(|p| (p.offset, p.e_kv_m))
                .fold(
                    (0.0, f64::NEG_INFINITY),
                    |a, b| if b.1 > a.1 { b } else { a },
                );
        profiles.push(Profile {
            from: span.from.clone(),
            to: span.to.clone(),
            at: t,
            points,
            max_kv_m,
            max_offset,
        });
    }
    // Given points (3D; in 2D at the nearest span's cross-section there).
    let mut points = Vec::new();
    for gp in &settings.points {
        let g = ground(gp.x, gp.y);
        let z = match (gp.z, gp.height) {
            (Some(z), _) => z,
            (None, Some(h)) => g + h,
            (None, None) => g + settings.height,
        };
        let e = match &model3d {
            Some(_) => e_at(&[[gp.x, gp.y, z]])[0],
            None => {
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
                let a = axes[i].as_ref().expect("an axis");
                let cs = cross_section::cross_section(
                    src,
                    &spans[i],
                    StationAt::Fraction(t / a.length),
                )?;
                field_2d(&charges_2d(&cs, &charges), -s, z - cs.ground.elevation)
            }
        };
        points.push(FieldPoint {
            name: gp.name.clone(),
            x: gp.x,
            y: gp.y,
            z,
            offset: 0.0,
            height: z - g,
            e_kv_m: e / 1e3,
            margin_kv_m: limit.map(|l| l - e / 1e3),
        });
    }
    // Cuts and volumes (3D).
    let mut cuts = Vec::new();
    let mut volumes = Vec::new();
    if model3d.is_some() {
        let kv = |ps: &[[f64; 3]]| -> Vec<f64> { e_at(ps).into_iter().map(|e| e / 1e3).collect() };
        let find = |from: &str, to: &str| {
            spans
                .iter()
                .position(|s| (s.from == from && s.to == to) || (s.from == to && s.to == from))
                .ok_or_else(|| format!("no span between structures {from} and {to}"))
        };
        for c in &settings.cuts {
            magnetic::check_cancel(progress)?;
            let i = find(&c.from, &c.to)?;
            let axis = axes[i]
                .as_ref()
                .ok_or_else(|| format!("span {}–{}: no axis", c.from, c.to))?;
            let t = match c.fraction {
                Some(f) if (0.0..=1.0).contains(&f) => f * axis.length,
                Some(f) => return Err(format!("a place in the span from 0 to 1, not {f}")),
                None => magnetic::lowest_station(src, &spans[i], axis),
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
                top: magnetic::highest_wire(src, &spans[i]).unwrap_or(low + 50.0) + c.above,
                step: c.step,
            };
            let g = |x: f64, y: f64| Some(ground(x, y));
            let values: CutValues = field_views::vertical_cut(&cut, &kv, Some(&g), progress)?;
            cuts.push(magnetic::FieldCut {
                from: spans[i].from.clone(),
                to: spans[i].to.clone(),
                at: t,
                pieces: model3d.as_ref().map_or(0, Charges3d::count),
                left_out: None,
                values,
            });
        }
        let pieces: Vec<[[f64; 3]; 2]> = if settings.volumes.is_empty() {
            Vec::new()
        } else {
            model3d
                .as_ref()
                .map(|m| {
                    m.groups
                        .iter()
                        .flat_map(|g| &g.1)
                        .flat_map(|p| {
                            (0..p.len.len()).map(move |k| {
                                [
                                    [p.x[k], p.y[k], p.z[k]],
                                    [p.x[k + 1], p.y[k + 1], p.z[k + 1]],
                                ]
                            })
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        for v in &settings.volumes {
            magnetic::check_cancel(progress)?;
            let run =
                magnetic::run_spans(&spans, &v.from, &v.to).map_err(|e| format!("volume: {e}"))?;
            let path = magnetic::run_path(src, &spans, &run)?;
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
                    low = low.min(ground(p[0], p[1]));
                }
            }
            let high = run
                .iter()
                .filter_map(|&i| magnetic::highest_wire(src, &spans[i]))
                .reduce(f64::max);
            let bounds = VolumeBox {
                path,
                width: v.width,
                bottom: low.floor(),
                top: high.unwrap_or(low + 50.0) + v.above,
                step: v.step,
            };
            let values: VolumeValues =
                field_views::volume(&bounds, &settings.levels, &kv, &pieces, progress)?;
            volumes.push(magnetic::FieldVolume {
                from: spans[run[0]].from.clone(),
                to: spans[run[run.len() - 1]].to.clone(),
                pieces: pieces.len(),
                left_out: None,
                values,
            });
        }
    }
    if let Some(p) = progress {
        p.done
            .store(p.total.load(Ordering::Relaxed), Ordering::Relaxed);
    }
    Ok(ElectricField {
        model: settings.model,
        height: settings.height,
        limit_kv_m: limit,
        charges: rows,
        profiles,
        points,
        cuts,
        volumes,
        notes,
    })
}

/// A 2D line charge: across, height, per frequency the charge per metre.
type LineCharge = (f64, f64, Vec<(f64, C)>);

/// A cross-section's line charges for the 2D model: (across, height above
/// the cross-section's ground, per frequency the charge per metre), each
/// bundle's charge shared by its subconductors.
fn charges_2d(cs: &CrossSection, charges: &ChargeMap) -> Vec<LineCharge> {
    let mut out = Vec::new();
    for w in &cs.wires {
        let Some(list) = charges.get(&(w.section.clone(), w.span)) else {
            continue;
        };
        out.push((
            // Left is positive in the 2D frame.
            -w.offset,
            w.elevation - cs.ground.elevation,
            list.clone(),
        ));
    }
    out
}

/// E (rms) at `(y, z)` of 2D line charges over level ground, V/m.
fn field_2d(lines: &[LineCharge], y: f64, z: f64) -> f64 {
    let mut by_f: Vec<(f64, [C; 2])> = Vec::new();
    for (yk, zk, list) in lines {
        let f = line_field(*yk, *zk, y, z);
        for &(freq, q) in list {
            let e = match by_f.iter_mut().find(|e| e.0 == freq) {
                Some(e) => e,
                None => {
                    by_f.push((freq, [C::new(0.0, 0.0); 2]));
                    by_f.last_mut().expect("pushed")
                }
            };
            e.1[0] += q * f[0];
            e.1[1] += q * f[1];
        }
    }
    by_f.iter()
        .map(|(_, e)| e[0].norm_sqr() + e[1].norm_sqr())
        .sum::<f64>()
        .sqrt()
}

// ---------------------------------------------------------------------------
// Kept for later, CSV out (the OHL-Design window, 2026-10-29).

/// [`run`], and the 3D model's charges kept (3D only) for the field
/// computed later at other places: [`Charges3d::field`] with
/// [`GroundPlane::fitted`] over [`Settings::ground_radius`] on the ground
/// of [`feet_ground`] where no terrain gives it. The charges are built
/// again as the run builds them (the mean cross-sections a second time:
/// quick beside the points).
pub fn run_keeping(
    src: &Sources<'_>,
    settings: &Settings,
    progress: Option<&Progress>,
) -> Result<(ElectricField, Option<Charges3d>), String> {
    let field = run(src, settings, progress)?;
    if settings.model != Model::ThreeD {
        return Ok((field, None));
    }
    let (means, _) = cross_section::span_cross_sections(src, src.line.electrical.mean_height);
    let (mut notes, mut defaulted) = (Vec::new(), BTreeMap::new());
    let mut charges = ChargeMap::new();
    for (_, cs) in &means {
        for (f, q) in cross_section_charges(cs, &settings.voltages, &mut defaulted, &mut notes) {
            for (w, qk) in cs.wires.iter().zip(q) {
                charges
                    .entry((w.section.clone(), w.span))
                    .or_default()
                    .push((f, qk));
            }
        }
    }
    let model = build_3d(src, &charges, settings, &mut notes);
    Ok((field, Some(model)))
}

/// The ground [`run`] takes where no terrain gives it: straight between
/// the structures' feet and level across, by the nearest span. Owned (for
/// the field computed later, on another thread).
pub fn feet_ground(line: &ohl_line::Line) -> impl Fn(f64, f64) -> f64 + Send + Sync + 'static {
    let axes: Vec<Axis> = cross_section::line_spans(line)
        .iter()
        .filter_map(|s| Axis::of(line, s).ok())
        .collect();
    move |x: f64, y: f64| {
        let mut best: Option<(f64, f64)> = None;
        for a in &axes {
            let (t, s) = a.local(&[x, y, 0.0]);
            let tc = t.clamp(0.0, a.length);
            let d = (t - tc).hypot(s);
            if best.is_none_or(|b| d < b.0) {
                best = Some((d, a.ground(None, tc, 0.0)));
            }
        }
        best.map_or(0.0, |b| b.1)
    }
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_owned()
    }
}

/// The results as CSV: one row per profile point, given point and cut
/// point (kV/m).
pub fn csv(e: &ElectricField) -> String {
    use std::fmt::Write;
    let mut out = String::from(
        "kind,from,to,station_m,name,x,y,z,offset_m,height_m,e_kv_m,limit_kv_m,margin_kv_m\n",
    );
    let num = |x: Option<f64>| x.map_or(String::new(), |v| format!("{v:.4}"));
    let row =
        |out: &mut String, kind: &str, from: &str, to: &str, at: Option<f64>, p: &FieldPoint| {
            let _ = writeln!(
                out,
                "{kind},{},{},{},{},{:.3},{:.3},{:.3},{:.3},{:.3},{:.5},{},{}",
                csv_field(from),
                csv_field(to),
                num(at),
                csv_field(&p.name),
                p.x,
                p.y,
                p.z,
                p.offset,
                p.height,
                p.e_kv_m,
                num(e.limit_kv_m),
                num(p.margin_kv_m),
            );
        };
    for pr in &e.profiles {
        for p in &pr.points {
            row(&mut out, "profile", &pr.from, &pr.to, Some(pr.at), p);
        }
    }
    for p in &e.points {
        row(&mut out, "point", "", "", None, p);
    }
    for c in &e.cuts {
        let v = &c.values;
        let nc = v.offsets.len();
        for (r, &z) in v.elevations.iter().enumerate() {
            for (k, &s) in v.offsets.iter().enumerate() {
                let xy = v.cut.at(s);
                let kv = v.values[r * nc + k];
                let p = FieldPoint {
                    name: String::new(),
                    x: xy[0],
                    y: xy[1],
                    z,
                    offset: s,
                    height: v.ground[k].map_or(f64::NAN, |g| z - g),
                    e_kv_m: kv,
                    margin_kv_m: e.limit_kv_m.map(|l| l - kv),
                };
                row(&mut out, "cut", &c.from, &c.to, Some(c.at), &p);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A long piece's field is the infinite line charge's
    /// \\( \lambda/(2\pi\varepsilon_0\rho) \\), across it.
    #[test]
    fn long_piece_is_the_infinite_line() {
        let e = segment_field([-1e6, 0.0, 0.0], [1e6, 0.0, 0.0], [0.0, 3.0, 4.0]);
        let want = 2.0 * K / 5.0;
        let got = (e[0] * e[0] + e[1] * e[1] + e[2] * e[2]).sqrt();
        assert!((got - want).abs() < 1e-9 * want, "{got} {want}");
        assert!(e[0].abs() < 1e-9 * want);
        assert!((e[1] / got - 0.6).abs() < 1e-9 && (e[2] / got - 0.8).abs() < 1e-9);
    }

    /// The closed form against Coulomb's law integrated numerically
    /// (Simpson, 20 000 intervals), beside and beyond a piece.
    #[test]
    fn piece_against_numerical_integration() {
        let (a, b): ([f64; 3], [f64; 3]) = ([1.0, 2.0, 3.0], [11.0, -4.0, 8.0]);
        for p in [[4.0, 5.0, 1.0], [15.0, -6.0, 11.0], [0.0, 0.0, 0.0]] {
            let n = 20_000;
            let len = ((b[0] - a[0]) * (b[0] - a[0])
                + (b[1] - a[1]) * (b[1] - a[1])
                + (b[2] - a[2]) * (b[2] - a[2]))
                .sqrt();
            let mut s = [0.0; 3];
            for i in 0..=n {
                let t = i as f64 / n as f64;
                let w = if i == 0 || i == n {
                    1.0
                } else if i % 2 == 1 {
                    4.0
                } else {
                    2.0
                };
                let x: [f64; 3] = std::array::from_fn(|k| a[k] + t * (b[k] - a[k]));
                let r: [f64; 3] = std::array::from_fn(|k| p[k] - x[k]);
                let d3 = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).powf(1.5);
                for k in 0..3 {
                    s[k] += w * r[k] / d3;
                }
            }
            let h = len / n as f64 / 3.0;
            let num: [f64; 3] = std::array::from_fn(|k| K * s[k] * h);
            let e = segment_field(a, b, p);
            for k in 0..3 {
                assert!(
                    (e[k] - num[k]).abs() < 1e-8 * num[k].abs().max(1.0),
                    "{e:?} {num:?}"
                );
            }
        }
    }

    /// One wire over level ground: \\( q = 2\pi\varepsilon_0 V / \ln(2h/r) \\)
    /// from the potential coefficients, and under it at the ground
    /// \\( E = q/(\pi\varepsilon_0 h) \\), straight down — in 2D and from a
    /// long straight 3D wire with its image in the ground plane.
    #[test]
    fn one_wire_over_level_ground() {
        let (h, r, v) = (20.0, 0.015, 100e3);
        let p = crate::earth::potential_coefficient([0.0, h], [0.0, h], r);
        let q = v / p;
        assert!((q - 2.0 * PI * EPS0 * v / (2.0 * h / r).ln()).abs() < 1e-12 * q);
        let want = q / (PI * EPS0 * h);
        let e2 = line_field(0.0, h, 0.0, 0.0);
        assert!(e2[0].abs() < 1e-9 && (e2[1] * q + want).abs() < 1e-9 * want);
        let mut m = Charges3d::default();
        m.push(50.0, &[[-1e5, 0.0, h], [1e5, 0.0, h]], C::new(q, 0.0));
        let plane = GroundPlane {
            point: [0.0, 0.0, 0.0],
            normal: [0.0, 0.0, 1.0],
        };
        let e3 = m.field([0.0, 0.0, 0.0], &plane);
        assert!((e3 - want).abs() < 1e-6 * want, "{e3} {want}");
        // A tilted plane: the same wire and point turned with it give the
        // same field.
        let th: f64 = 0.3;
        let rot = |p: [f64; 3]| {
            [
                p[0],
                p[1] * th.cos() - p[2] * th.sin(),
                p[1] * th.sin() + p[2] * th.cos(),
            ]
        };
        let mut mt = Charges3d::default();
        mt.push(
            50.0,
            &[rot([-1e5, 0.0, h]), rot([1e5, 0.0, h])],
            C::new(q, 0.0),
        );
        let tilted = GroundPlane {
            point: [0.0, 0.0, 0.0],
            normal: rot([0.0, 0.0, 1.0]),
        };
        let et = mt.field(rot([0.0, 5.0, 1.0]), &tilted);
        let el = m.field([0.0, 5.0, 1.0], &plane);
        assert!((et - el).abs() < 1e-9 * el, "{et} {el}");
    }

    /// A plane fitted to an evenly sloping ground is that slope.
    #[test]
    fn a_fitted_plane() {
        let g = |x: f64, y: f64| 100.0 + 0.1 * x - 0.2 * y;
        let p = GroundPlane::fitted(&g, 10.0, 5.0, 10.0);
        assert!((p.point[2] - 100.0).abs() < 1e-12);
        let n = p.normal;
        assert!((n[0] / n[2] + 0.1).abs() < 1e-12 && (n[1] / n[2] - 0.2).abs() < 1e-12);
        let m = p.mirror([10.0, 5.0, 110.0]);
        assert!(((m[2] - 100.0) + 10.0 * n[2] * n[2]).abs() < 1e-9 || m[2] < 100.0);
    }
}
