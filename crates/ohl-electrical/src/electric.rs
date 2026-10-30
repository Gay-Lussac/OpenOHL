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
    /// The potential at `p` of the pieces (per C/m), V:
    /// \\( \frac{1}{4\pi\varepsilon_0}\left(\operatorname{asinh}\frac{L - t}{\rho}
    /// + \operatorname{asinh}\frac{t}{\rho}\right) \\) per piece.
    fn potential(&self, p: [f64; 3]) -> f64 {
        let mut sum = 0.0;
        for k in 0..self.len.len() {
            let (rx, ry, rz) = (p[0] - self.x[k], p[1] - self.y[k], p[2] - self.z[k]);
            let (ux, uy, uz) = (self.ux[k], self.uy[k], self.uz[k]);
            let t = rx * ux + ry * uy + rz * uz;
            let rho = (rx * rx + ry * ry + rz * rz - t * t).max(1e-12).sqrt();
            sum += ((self.len[k] - t) / rho).asinh() + (t / rho).asinh();
        }
        K * sum
    }

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

    /// Every straight piece (start, end).
    pub fn segments(&self) -> Vec<[[f64; 3]; 2]> {
        self.groups
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
    /// The ground: a tiled conducting floor around each profile, group of
    /// points, cut and volume (the default; owner, 2026-10-29: "keep the
    /// tiled implementation"), or a plane under each point (quick: not
    /// conservative on bumps, see the theory).
    pub ground_model: GroundModel,
    /// With the tiled floor: the tiles' side under the cuts across the
    /// line, m (owner, 2026-10-29: coarser by default, a parameter).
    pub cut_tile: f64,
    /// With the tiled floor: the tiles' side under the volumes, m (the
    /// floor along the volume's whole run and [`VOLUME_MARGIN`] beyond).
    pub volume_tile: f64,
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
            ground_model: GroundModel::Tiled(TiledFloor::default()),
            cut_tile: 8.0,
            volume_tile: 8.0,
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
    /// How far the terrain departs from the plane fitted under the
    /// largest value's point, within three ground radii of it, m (the
    /// plane ground's weakness there; 0 on level or evenly sloping
    /// ground).
    pub ground_departure: f64,
    /// The tiles of the floor, with the tiled ground.
    pub tiles: Option<usize>,
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
    let mut freqs: Vec<f64> = volts.iter().flatten().map(|v| v.0).collect();
    freqs.sort_by(f64::total_cmp);
    freqs.dedup();
    let mut out = Vec::new();
    for f in freqs {
        let v: Vec<C> = volts
            .iter()
            .map(|x| match x {
                Some((g, v)) if *g == f => *v,
                _ => C::new(0.0, 0.0),
            })
            .collect();
        let Some(q) = charges(&wires, &v) else {
            notes.push(format!(
                "span {}–{}: the potential coefficients are singular; no charges",
                cs.station.from, cs.station.to
            ));
            return Vec::new();
        };
        out.push((f, q));
    }
    out
}

/// The charges per metre of parallel wires over a perfectly conducting
/// earth at the voltages to earth `volts` (rms phasors, V), C/m:
/// \( q = P^{-1} V \) with the potential coefficients of
/// [`matrices::potential`] (a bundle as its equivalent wire). `None`: the
/// coefficients are singular.
pub fn charges(wires: &[Wire], volts: &[C]) -> Option<Vec<C>> {
    let inv = matrices::complex(&matrices::potential(wires).try_inverse()?);
    let v = DVector::from_iterator(volts.len(), volts.iter().copied());
    Some((&inv * v).iter().copied().collect())
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
    run_keeping(src, settings, progress).map(|r| r.0)
}

/// [`run`], and the 3D model's charges kept (3D only) for cuts computed
/// later at other places: [`Charges3d::field`] at a point with the ground
/// plane [`GroundPlane::fitted`] under it (over
/// [`Settings::ground_radius`]).
pub fn run_keeping(
    src: &Sources<'_>,
    settings: &Settings,
    progress: Option<&Progress>,
) -> Result<(ElectricField, Option<Charges3d>), String> {
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
        // Every floor of the run announced now, so that the time left
        // counts them from the start (a floor of thousands of tiles takes
        // seconds, its profile's points milliseconds).
        if let (Some(_), GroundModel::Tiled(floor)) = (&model3d, settings.ground_model) {
            for _ in &stations {
                p.plan_floor(floor.tiles(settings.width));
            }
            for c in &settings.cuts {
                let cut_floor = TiledFloor {
                    fine: settings.cut_tile,
                    coarse: settings.cut_tile.max(floor.coarse),
                    ..floor
                };
                p.plan_floor(cut_floor.tiles(c.width));
            }
            for v in &settings.volumes {
                if let Ok(run) = magnetic::run_spans(&spans, &v.from, &v.to)
                    && let Ok(path) = magnetic::run_path(src, &spans, &run)
                {
                    p.plan_floor(Floor::tiles_under(
                        path.length(),
                        v.width,
                        settings.volume_tile,
                    ));
                }
            }
        }
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
        let mut tiles = None;
        let values: Vec<f64> = match (&model3d, settings.ground_model) {
            (Some(m), GroundModel::Tiled(floor)) => {
                let fl = with_floor(
                    progress,
                    &format!("profile {}–{}", span.from, span.to),
                    floor.tiles(settings.width),
                    || Floor::solve(m, Frame::of(axis), t, settings.width, &floor, &ground),
                )?;
                tiles = Some(fl.tiles());
                let pts: Vec<[f64; 3]> = places.iter().map(|p| p.0).collect();
                let f =
                    |ps: &[[f64; 3]]| -> Vec<f64> { ps.iter().map(|&p| fl.field(p, m)).collect() };
                compute_all(&pts, &f, progress)?
            }
            (Some(_), GroundModel::Plane) => {
                let pts: Vec<[f64; 3]> = places.iter().map(|p| p.0).collect();
                compute_all(&pts, &e_at, progress)?
            }
            (None, _) => {
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
        let ground_departure = {
            let p = axis.plan(t, max_offset);
            let plane = GroundPlane::fitted(&ground, p[0], p[1], radius);
            let n = plane.normal;
            let reach = 3.0 * radius;
            let mut worst: f64 = 0.0;
            for a in -6..=6 {
                for b in -6..=6 {
                    let (x, y) = (p[0] + reach * a as f64 / 6.0, p[1] + reach * b as f64 / 6.0);
                    // The plane's elevation at (x, y).
                    let zp = plane.point[2]
                        - (n[0] * (x - plane.point[0]) + n[1] * (y - plane.point[1])) / n[2];
                    worst = worst.max((ground(x, y) - zp).abs());
                }
            }
            worst
        };
        profiles.push(Profile {
            from: span.from.clone(),
            to: span.to.clone(),
            at: t,
            points,
            max_kv_m,
            max_offset,
            ground_departure,
            tiles,
        });
    }
    // Given points (3D; in 2D at the nearest span's cross-section there).
    // On the tiled floor: the points near one another along a span share
    // its floor (each group's floor around its middle, its fine tiles
    // reaching its farthest point across).
    let mut floor_e: BTreeMap<usize, f64> = BTreeMap::new();
    if let (Some(m), GroundModel::Tiled(floor)) = (&model3d, settings.ground_model) {
        // Per (span, stretch along it): (point, station, offset, place).
        type Near = Vec<(usize, f64, f64, [f64; 3])>;
        let mut groups: BTreeMap<(usize, i64), Near> = BTreeMap::new();
        for (n, gp) in settings.points.iter().enumerate() {
            let mut best: Option<(f64, usize, f64, f64)> = None;
            for (i, a) in axes.iter().enumerate() {
                let Some(a) = a else { continue };
                let (t, s) = a.local(&[gp.x, gp.y, 0.0]);
                let tc = t.clamp(0.0, a.length);
                let d = (t - tc).hypot(s);
                if best.is_none_or(|b| d < b.0) {
                    best = Some((d, i, t, s));
                }
            }
            let Some((_, i, t, s)) = best else { continue };
            let g = ground(gp.x, gp.y);
            let z = match (gp.z, gp.height) {
                (Some(z), _) => z,
                (None, Some(h)) => g + h,
                (None, None) => g + settings.height,
            };
            let bucket = (t / (2.0 * floor.fine_along)).floor() as i64;
            groups
                .entry((i, bucket))
                .or_default()
                .push((n, t, s, [gp.x, gp.y, z]));
        }
        if let Some(p) = progress {
            for list in groups.values() {
                let across = list.iter().map(|p| p.2.abs()).fold(0.0, f64::max);
                p.plan_floor(floor.tiles(across));
            }
        }
        for ((i, _), list) in groups {
            magnetic::check_cancel(progress)?;
            let axis = axes[i].as_ref().expect("an axis");
            let t0 = list.iter().map(|p| p.1).sum::<f64>() / list.len() as f64;
            let across = list.iter().map(|p| p.2.abs()).fold(0.0, f64::max);
            let fl = with_floor(progress, "given points", floor.tiles(across), || {
                Floor::solve(m, Frame::of(axis), t0, across, &floor, &ground)
            })?;
            for (n, _, _, p) in list {
                floor_e.insert(n, fl.field(p, m));
            }
        }
    }
    let mut points = Vec::new();
    for (n, gp) in settings.points.iter().enumerate() {
        let g = ground(gp.x, gp.y);
        let z = match (gp.z, gp.height) {
            (Some(z), _) => z,
            (None, Some(h)) => g + h,
            (None, None) => g + settings.height,
        };
        let e = match &model3d {
            Some(_) if floor_e.contains_key(&n) => floor_e[&n],
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
            let values: CutValues = match (&model3d, settings.ground_model) {
                (Some(m), GroundModel::Tiled(floor)) => {
                    let floor = TiledFloor {
                        fine: settings.cut_tile,
                        coarse: settings.cut_tile.max(floor.coarse),
                        ..floor
                    };
                    let fl = with_floor(
                        progress,
                        &format!("cut {}–{}", c.from, c.to),
                        floor.tiles(c.width),
                        || Floor::solve(m, Frame::of(axis), t, c.width, &floor, &ground),
                    )?;
                    let f = |ps: &[[f64; 3]]| -> Vec<f64> {
                        ps.iter().map(|&p| fl.field(p, m) / 1e3).collect()
                    };
                    field_views::vertical_cut(&cut, &f, Some(&g), progress)?
                }
                _ => field_views::vertical_cut(&cut, &kv, Some(&g), progress)?,
            };
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
            let values: VolumeValues = match (&model3d, settings.ground_model) {
                (Some(m), GroundModel::Tiled(_)) => {
                    let tiles = Floor::tiles_under(
                        bounds.path.length(),
                        bounds.width,
                        settings.volume_tile,
                    );
                    let fl = with_floor(progress, "volume", tiles, || {
                        Floor::under(m, &bounds.path, bounds.width, settings.volume_tile, &ground)
                    })?;
                    notes.push(format!(
                        "volume {}–{}: a tiled floor of {} tiles of {} m along it and {VOLUME_MARGIN} m beyond",
                        spans[run[0]].from,
                        spans[run[run.len() - 1]].to,
                        fl.tiles(),
                        settings.volume_tile
                    ));
                    let f = |ps: &[[f64; 3]]| -> Vec<f64> {
                        ps.iter().map(|&p| fl.field(p, m) / 1e3).collect()
                    };
                    field_views::volume(&bounds, &settings.levels, &f, &pieces, progress)?
                }
                _ => field_views::volume(&bounds, &settings.levels, &kv, &pieces, progress)?,
            };
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
    Ok((
        ElectricField {
            model: settings.model,
            height: settings.height,
            limit_kv_m: limit,
            charges: rows,
            profiles,
            points,
            cuts,
            volumes,
            notes,
        },
        model3d,
    ))
}

// ---------------------------------------------------------------------------
// The reference ground: a tiled conducting floor.

/// The ground as a conducting floor of tiles (owner, 2026-10-29: "a
/// comparison with a tiled floor … in a valley area as well as flatland").
/// Around a profile's station, square tiles of side `fine` within
/// `fine_along` along the line and the profile's width and `fine_margin`
/// across it, of side `coarse` beyond, to `reach` every way; each tile a
/// quadrilateral on the terrain carrying a uniform charge per square metre,
/// solved so that the floor is at zero potential at every tile's centre
/// under the line's charges (\\( A\,\sigma = -\varphi_{line} \\)). The
/// field: the line's charges and the tiles'. A reference for the plane
/// under each point: it follows hollows and ridges; its own errors are the
/// tiles' size and the floor's end at `reach` (measured on level ground,
/// where the plane is exact).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct TiledFloor {
    /// Fine tiles' side, m.
    pub fine: f64,
    /// Coarse tiles' side, m (a whole multiple of `fine`).
    pub coarse: f64,
    /// Fine tiles this far along the line either side of the station, m.
    pub fine_along: f64,
    /// And this far beyond the profile's width across, m.
    pub fine_margin: f64,
    /// The floor reaches this far every way, m.
    pub reach: f64,
}

impl Default for TiledFloor {
    fn default() -> Self {
        Self {
            fine: 2.0,
            coarse: 8.0,
            fine_along: 30.0,
            fine_margin: 10.0,
            reach: 200.0,
        }
    }
}

/// The ground model.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum GroundModel {
    /// A plane fitted under each point, the charges' images in it.
    Plane,
    /// The profiles over a tiled conducting floor ([`TiledFloor`]).
    Tiled(TiledFloor),
}

/// A floor's frame in plan: stations along `ahead` (unit) from `origin`,
/// offsets to its right.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    /// Station 0, offset 0.
    pub origin: [f64; 2],
    /// Ahead (unit).
    pub ahead: [f64; 2],
}

impl Frame {
    fn plan(&self, t: f64, s: f64) -> [f64; 2] {
        // Right of ahead: (ahead.y, −ahead.x).
        [
            self.origin[0] + t * self.ahead[0] + s * self.ahead[1],
            self.origin[1] + t * self.ahead[1] - s * self.ahead[0],
        ]
    }

    fn of(axis: &Axis) -> Self {
        Self {
            origin: axis.plan(0.0, 0.0),
            ahead: axis.u,
        }
    }

    /// A vertical cut's frame: station 0 on it, its offsets to the right.
    pub fn of_cut(origin: [f64; 2], right: [f64; 2]) -> Self {
        Self {
            origin,
            ahead: [-right[1], right[0]],
        }
    }
}

/// A tiled floor solved under the line's charges: its tiles and their
/// charges per square metre (per frequency).
pub struct Floor {
    panels: Vec<Panel>,
    sigma: Vec<Vec<C>>,
}

impl Floor {
    /// The floor of `settings` around station `t0` of `frame`, its fine
    /// tiles to `across` either side (plus the floor's margin), solved
    /// under `charges`; the ground's elevation from `ground`.
    pub fn solve(
        charges: &Charges3d,
        frame: Frame,
        t0: f64,
        across: f64,
        floor: &TiledFloor,
        ground: &dyn Fn(f64, f64) -> f64,
    ) -> Result<Self, String> {
        let panels = floor_panels(frame, t0, across, floor, ground);
        let sigma = solve_floor(&panels, charges)?;
        Ok(Self { panels, sigma })
    }

    /// The floor under a volume: square tiles of side `tile` along `path`
    /// (its stations beyond its ends too), `width` and
    /// [`VOLUME_MARGIN`] either side, and as far beyond its ends.
    pub fn under(
        charges: &Charges3d,
        path: &field_views::Path,
        width: f64,
        tile: f64,
        ground: &dyn Fn(f64, f64) -> f64,
    ) -> Result<Self, String> {
        if !(tile.is_finite() && tile > 0.0) {
            return Err(format!("a tile above 0 m, not {tile}"));
        }
        let place = |t: f64, s: f64| {
            let p = path.place_beyond(t, s);
            [p[0], p[1], ground(p[0], p[1])]
        };
        let (t0, t1) = (-VOLUME_MARGIN, path.length() + VOLUME_MARGIN);
        let half = width + VOLUME_MARGIN;
        let (nt, ns) = (
            ((t1 - t0) / tile).ceil() as usize,
            (2.0 * half / tile).ceil() as usize,
        );
        let mut panels = Vec::with_capacity(nt * ns);
        for a in 0..nt {
            for b in 0..ns {
                let (t, s) = (t0 + a as f64 * tile, -half + b as f64 * tile);
                panels.push(panel(&place, t, s, tile));
            }
        }
        let sigma = solve_floor(&panels, charges)?;
        Ok(Self { panels, sigma })
    }

    /// The tiles of the floor [`Floor::under`] lays under a volume.
    pub fn tiles_under(length: f64, width: f64, tile: f64) -> usize {
        let nt = ((length + 2.0 * VOLUME_MARGIN) / tile).ceil() as usize;
        let ns = (2.0 * (width + VOLUME_MARGIN) / tile).ceil() as usize;
        nt * ns
    }

    /// E (rms) at `p`, V/m: the line's charges and the floor's.
    pub fn field(&self, p: [f64; 3], charges: &Charges3d) -> f64 {
        floor_field(p, &self.panels, &self.sigma, charges)
    }

    /// Its tiles.
    pub fn tiles(&self) -> usize {
        self.panels.len()
    }
}

impl TiledFloor {
    /// The tiles of a floor whose fine part reaches `across` either side
    /// of its station (plus the margin).
    pub fn tiles(&self, across: f64) -> usize {
        let n_fine = (self.coarse / self.fine).round().max(1.0) as usize;
        let cells = 2 * (self.reach / self.coarse).ceil() as usize;
        let fine_t = ((2.0 * self.fine_along) / self.coarse).ceil() as usize + 1;
        let fine_s = ((2.0 * (across + self.fine_margin)) / self.coarse).ceil() as usize + 1;
        let fine = fine_t.min(cells) * fine_s.min(cells);
        cells * cells - fine + fine * n_fine * n_fine
    }

    /// About how long a floor of `tiles` takes to solve on this computer,
    /// s: its dense system (some 7 s for 3 700 tiles measured here,
    /// 2026-10-29; the time grows with the tiles' cube).
    pub fn seconds(tiles: usize) -> f64 {
        7.0 * (tiles as f64 / 3700.0).powi(3)
    }
}

/// Runs `solve` (a floor of `tiles`) with `progress` saying so (its
/// time counted apart from the points').
fn with_floor<T>(
    progress: Option<&Progress>,
    what: &str,
    tiles: usize,
    solve: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    if let Some(p) = progress {
        p.floor_begins(tiles);
        p.set_stage(format!("{what}: a tiled floor of {tiles} tiles"));
    }
    let out = solve();
    if let Some(p) = progress {
        p.floor_ends();
        p.set_stage(String::new());
    }
    out
}

/// A volume's floor reaches this far beyond it every way, m.
pub const VOLUME_MARGIN: f64 = 100.0;

/// Quadrature points per tile side.
const Q: usize = 6;

/// A tile of the floor.
struct Panel {
    centre: [f64; 3],
    area: f64,
    size: f64,
    sub: Vec<[f64; 3]>,
    /// Its corners (in order round it).
    corners: [[f64; 3]; 4],
    /// Its unit normal.
    normal: [f64; 3],
}

/// A tile: the quadrilateral of side `size` from station `t` and offset
/// `s` of `place` (plan station and offset to a point on the ground).
fn panel(place: &dyn Fn(f64, f64) -> [f64; 3], t: f64, s: f64, size: f64) -> Panel {
    let c = [
        place(t, s),
        place(t + size, s),
        place(t + size, s + size),
        place(t, s + size),
    ];
    let d1: [f64; 3] = std::array::from_fn(|k| c[2][k] - c[0][k]);
    let d2: [f64; 3] = std::array::from_fn(|k| c[3][k] - c[1][k]);
    let x = [
        d1[1] * d2[2] - d1[2] * d2[1],
        d1[2] * d2[0] - d1[0] * d2[2],
        d1[0] * d2[1] - d1[1] * d2[0],
    ];
    let area = 0.5 * (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt();
    let mut sub = Vec::with_capacity(Q * Q);
    for i in 0..Q {
        for j in 0..Q {
            sub.push(place(
                t + size * (i as f64 + 0.5) / Q as f64,
                s + size * (j as f64 + 0.5) / Q as f64,
            ));
        }
    }
    let n = if area > 0.0 {
        x.map(|v| v / (2.0 * area))
    } else {
        [0.0, 0.0, 1.0]
    };
    Panel {
        centre: std::array::from_fn(|k| 0.25 * (c[0][k] + c[1][k] + c[2][k] + c[3][k])),
        area,
        size,
        sub,
        corners: c,
        normal: n,
    }
}

/// The floor's tiles around station `t0` of a span's axis.
fn floor_panels(
    frame: Frame,
    t0: f64,
    width: f64,
    floor: &TiledFloor,
    ground: &dyn Fn(f64, f64) -> f64,
) -> Vec<Panel> {
    let place = |t: f64, s: f64| {
        let p = frame.plan(t, s);
        [p[0], p[1], ground(p[0], p[1])]
    };
    let quad = |t: f64, s: f64, size: f64| panel(&place, t, s, size);
    let n_fine = (floor.coarse / floor.fine).round().max(1.0) as usize;
    let fine = floor.coarse / n_fine as f64;
    let cells = (floor.reach / floor.coarse).ceil() as i64;
    let across = width + floor.fine_margin;
    let mut out = Vec::new();
    for a in -cells..cells {
        for b in -cells..cells {
            let (t, s) = (t0 + a as f64 * floor.coarse, b as f64 * floor.coarse);
            let near = t + floor.coarse > t0 - floor.fine_along
                && t < t0 + floor.fine_along
                && s + floor.coarse > -across
                && s < across;
            if near {
                for i in 0..n_fine {
                    for j in 0..n_fine {
                        out.push(quad(t + i as f64 * fine, s + j as f64 * fine, fine));
                    }
                }
            } else {
                out.push(quad(t, s, floor.coarse));
            }
        }
    }
    out
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// The tiles' charges per square metre, per frequency of the line's
/// charges: zero potential at every tile's centre.
fn solve_floor(panels: &[Panel], charges: &Charges3d) -> Result<Vec<Vec<C>>, String> {
    let n = panels.len();
    let w = 1.0 / (Q * Q) as f64;
    // The potential coefficients row by row on all cores.
    let rows: Vec<usize> = (0..n).collect();
    let mut a = nalgebra::DMatrix::<f64>::zeros(n, n);
    let row = |i: usize| -> Vec<f64> {
        let ci = panels[i].centre;
        (0..n)
            .map(|j| {
                let pj = &panels[j];
                if i == j {
                    // A disc of the tile's area, at its centre.
                    (pj.area / PI).sqrt() / (2.0 * EPS0)
                } else if dist(ci, pj.centre) > 4.0 * pj.size {
                    K * pj.area / dist(ci, pj.centre)
                } else {
                    pj.sub
                        .iter()
                        .map(|q| K * pj.area * w / dist(ci, *q).max(1e-6))
                        .sum()
                }
            })
            .collect()
    };
    let filled: Vec<Vec<f64>> = std::thread::scope(|scope| {
        let chunk = n.div_ceil(field_views::threads()).max(1);
        let handles: Vec<_> = rows
            .chunks(chunk)
            .map(|c| scope.spawn(move || c.iter().map(|&i| row(i)).collect::<Vec<_>>()))
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("a row"))
            .collect()
    });
    for (i, r) in filled.iter().enumerate() {
        for (j, v) in r.iter().enumerate() {
            a[(i, j)] = *v;
        }
    }
    let lu = a.lu();
    let mut out = Vec::new();
    for (_, polys) in &charges.groups {
        let phi: Vec<C> = panels
            .iter()
            .map(|p| {
                polys
                    .iter()
                    .map(|poly| poly.charge * poly.potential(p.centre))
                    .sum()
            })
            .collect();
        let re = DVector::from_iterator(n, phi.iter().map(|c| -c.re));
        let im = DVector::from_iterator(n, phi.iter().map(|c| -c.im));
        let (Some(sr), Some(si)) = (lu.solve(&re), lu.solve(&im)) else {
            return Err("the tiled floor's system could not be solved".into());
        };
        out.push((0..n).map(|k| C::new(sr[k], si[k])).collect());
    }
    Ok(out)
}

/// E (rms) at `p` over the tiled floor, V/m: the line's charges and the
/// tiles'.
fn floor_field(p: [f64; 3], panels: &[Panel], sigma: &[Vec<C>], charges: &Charges3d) -> f64 {
    let mut inv = Vec::new();
    let direct = charges.sums(p, &mut inv);
    let w = 1.0 / (Q * Q) as f64;
    let mut sum = 0.0;
    for (g, d) in direct.iter().enumerate() {
        let mut e = *d;
        for (pj, &s) in panels.iter().zip(&sigma[g]) {
            let mut add = |q: [f64; 3], a: f64| {
                let r = [p[0] - q[0], p[1] - q[1], p[2] - q[2]];
                let r3 = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2])
                    .max(1e-12)
                    .powf(1.5);
                for k in 0..3 {
                    e[k] += s * (K * a * r[k] / r3);
                }
            };
            let d = dist(p, pj.centre);
            if d > 4.0 * pj.size {
                add(pj.centre, pj.area);
            } else if d > 1.5 * pj.size {
                for q in &pj.sub {
                    add(*q, pj.area * w);
                }
            } else {
                // Close to the tile: points closer together than the
                // distance to its plane (some per cent off otherwise a
                // metre above a coarse tile).
                let r = [
                    p[0] - pj.centre[0],
                    p[1] - pj.centre[1],
                    p[2] - pj.centre[2],
                ];
                let h = (r[0] * pj.normal[0] + r[1] * pj.normal[1] + r[2] * pj.normal[2])
                    .abs()
                    .max(0.1);
                let k = ((2.0 * pj.size / h).ceil() as usize).clamp(Q, 96);
                let c = &pj.corners;
                let a = pj.area / (k * k) as f64;
                for i in 0..k {
                    let u = (i as f64 + 0.5) / k as f64;
                    for j in 0..k {
                        let v = (j as f64 + 0.5) / k as f64;
                        let q: [f64; 3] = std::array::from_fn(|m| {
                            (1.0 - u) * (1.0 - v) * c[0][m]
                                + u * (1.0 - v) * c[1][m]
                                + u * v * c[2][m]
                                + (1.0 - u) * v * c[3][m]
                        });
                        add(q, a);
                    }
                }
            }
        }
        let r = rms(&e);
        sum += r * r;
    }
    sum.sqrt()
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

    /// Exact: a long wire 20 m above the bottom of a 90° V-valley (its
    /// sides z = |y|): the charge and three images (a 90° wedge). The
    /// plane under each point and the tiled floor against it, 1 m above the
    /// ground across the valley (owner, 2026-10-29: "can you test the per
    /// point plane too?").
    #[test]
    fn a_v_valley_exactly() {
        let (h, q) = (20.0, 1e-6);
        let g = |_x: f64, y: f64| y.abs();
        // The exact 2D field: the charge, its mirrors in each side (−q)
        // and in both (+q).
        let exact = |y: f64, z: f64| {
            let mut e = [0.0; 2];
            for (yk, zk, s) in [
                (0.0, h, 1.0),
                (h, 0.0, -1.0),
                (-h, 0.0, -1.0),
                (0.0, -h, 1.0),
            ] {
                let (dy, dz) = (y - yk, z - zk);
                let r2 = dy * dy + dz * dz;
                e[0] += s * q * 2.0 * K * dy / r2;
                e[1] += s * q * 2.0 * K * dz / r2;
            }
            e[0].hypot(e[1])
        };
        let mut m = Charges3d::default();
        m.push(50.0, &[[-2e4, 0.0, h], [2e4, 0.0, h]], C::new(q, 0.0));
        let axis = Axis {
            a: [-2e4, 0.0, 0.0],
            b: [2e4, 0.0, 0.0],
            u: [1.0, 0.0],
            right: [0.0, -1.0],
            length: 4e4,
        };
        // (A floor smaller than the default: the test stays quick.)
        let floor = TiledFloor {
            fine: 2.0,
            coarse: 8.0,
            fine_along: 16.0,
            fine_margin: 4.0,
            reach: 96.0,
        };
        let panels = floor_panels(Frame::of(&axis), 2e4, 26.0, &floor, &g);
        let sigma = solve_floor(&panels, &m).unwrap();
        let ys: Vec<f64> = (-25..=25).map(|k| k as f64).collect();
        let ex: Vec<f64> = ys.iter().map(|&y| exact(y, y.abs() + 1.0)).collect();
        let top = ex.iter().copied().fold(0.0, f64::max);
        let (mut plane_worst, mut tiled_worst, mut plane_at_top) = (0.0f64, 0.0f64, 0.0);
        for (k, &y) in ys.iter().enumerate() {
            let p = [0.0, y, y.abs() + 1.0];
            let pl = m.field(p, &GroundPlane::fitted(&g, p[0], p[1], 10.0));
            let ti = floor_field(p, &panels, &sigma, &m);
            plane_worst = plane_worst.max((pl - ex[k]).abs() / top);
            tiled_worst = tiled_worst.max((ti - ex[k]).abs() / top);
            if ex[k] == top {
                plane_at_top = (pl - ex[k]) / ex[k];
            }
        }
        eprintln!(
            "V-valley: exact largest {top:.1} V/m; plane under each point off by at most {:.1} % of it ({:+.1} % at the largest); tiled floor at most {:.1} %",
            plane_worst * 100.0,
            plane_at_top * 100.0,
            tiled_worst * 100.0
        );
        // (4.8 % with the default floor, 8.5 % with this smaller one.)
        assert!(tiled_worst < 0.10, "{tiled_worst}");
        assert!(
            plane_worst > tiled_worst,
            "the plane is the less exact here"
        );
    }
}
