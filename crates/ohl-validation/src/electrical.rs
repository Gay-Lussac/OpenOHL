// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Calculations for the electrical validation cases
//! (`benchmarks/electrical/`): the earth return terms of two wires, and the
//! line parameters of a set of parallel wires. Adapters only: they call
//! `ohl-electrical` as the line parameters do.

use std::f64::consts::PI;

use ohl_core::units::{Length, ResistancePerLength};
use ohl_electrical::earth;
use ohl_electrical::matrices::{self, Wire};
use ohl_line::electrical::EarthReturn;

use crate::{Calculation, Inputs, Outputs, Registry};

pub(crate) fn register(registry: &mut Registry) {
    registry.register(Box::new(EarthTerms));
    registry.register(Box::new(Parameters));
    registry.register(Box::new(MagneticSegments));
    registry.register(Box::new(Magnetic2d));
    registry.register(Box::new(Fields2d));
    registry.register(Box::new(LightningCut));
    registry.register(Box::new(SurfaceGradient2d));
    registry.register(Box::new(CylinderGradient));
}

/// `electrical.lightning_cut`: one phase in one cross-section of the
/// lightning shielding check: `shields` (`[[across, z], …]`, m), `phase`
/// (`[across, z, outer radius]`), level ground at `ground` (m), `method`
/// `"angle"` (`outer_angle`, `inner_angle`, °) or `"sphere"`
/// (`striking_distance`, m; `ground_factor`, default 1). Outputs: `depth`
/// (° beyond the angle, or m of the sphere's reach into the phase; ≤ 0
/// covered), `exposed` (1 or 0), and `needed_deg` (angle) or `r_max_m`
/// (sphere: the largest striking distance still exposing it).
struct LightningCut;

impl Calculation for LightningCut {
    fn id(&self) -> &'static str {
        "electrical.lightning_cut"
    }

    fn description(&self) -> &'static str {
        "Lightning shielding of one phase in one cross-section: shielding angle or rolling sphere, its margin"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        use ohl_clearance::lightning::{Method, StrikingDistance};
        use ohl_electrical::lightning::{CutWire, check_place};
        let list = inputs
            .get("shields")
            .and_then(|v| v.as_array())
            .ok_or("input \"shields\": a list of [across, z]")?;
        let mut shields = Vec::new();
        for w in list {
            let xy = numbers(Some(w), "shield", 2)?;
            shields.push(CutWire {
                s: xy[0],
                z: xy[1],
                r: 0.0,
            });
        }
        let ph = numbers(inputs.get("phase"), "phase", 3)?;
        let p = CutWire {
            s: ph[0],
            z: ph[1],
            r: ph[2],
        };
        let g = float(inputs, "ground")?;
        let ground: Vec<[f64; 2]> = (-1000..=1000).map(|k| [k as f64, g]).collect();
        let method = match inputs.get("method").and_then(|v| v.as_str()) {
            Some("angle") => Method::Angle {
                outer: float(inputs, "outer_angle")?,
                inner: float(inputs, "inner_angle")?,
            },
            Some("sphere") => Method::Sphere {
                distance: StrikingDistance::Typed(float(inputs, "striking_distance")?),
                ground_factor: if inputs.contains_key("ground_factor") {
                    float(inputs, "ground_factor")?
                } else {
                    1.0
                },
            },
            _ => return Err("input \"method\": \"angle\" or \"sphere\"".into()),
        };
        let (depth, margin) = check_place(p, &shields, &ground, method);
        let mut out = Outputs::new();
        out.insert("depth".into(), depth);
        out.insert("exposed".into(), if depth > 1e-6 { 1.0 } else { 0.0 });
        match method {
            Method::Angle { .. } => {
                out.insert("needed_deg".into(), margin.unwrap_or(f64::NAN));
            }
            Method::Sphere { .. } => {
                out.insert("r_max_m".into(), margin.unwrap_or(0.0));
            }
        }
        Ok(out)
    }
}

/// `electrical.fields_2d`: the electric field strength and the magnetic
/// flux density on a profile across a line's cross-section by the
/// program's 2D model, from inputs as hvlbuzz (FKH) takes them, to compare
/// with it: `systems` (`{ subconductors, spacing, diameter (m), kv (between
/// phases), amps, bundle_angle (deg, the first subconductor's from the
/// bundle's centre), phases = [[x, y], …] }`, the phases at 0°, −120°,
/// +120°, …), `earth_wires` (`{ x, y, diameter }`, at earth potential, no
/// induced current), `height` (m above the ground) and `offsets` (m).
/// Outputs per offset k (`_00`, `_01`, …): `e_v_m_k` (rms of the vector,
/// V/m) and `b_ut_k` (µT); and their largest values `e_max_v_m`,
/// `b_max_ut`. The charges: one per bundle at its centre
/// ([`ohl_electrical::electric::charges`]); the currents: each
/// subconductor's.
struct Fields2d;

impl Calculation for Fields2d {
    fn id(&self) -> &'static str {
        "electrical.fields_2d"
    }

    fn description(&self) -> &'static str {
        "Electric and magnetic field on a profile across a cross-section (2D), inputs as hvlbuzz takes them"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        use nalgebra::Complex;
        use ohl_electrical::electric::{charges, line_field};
        use ohl_electrical::magnetic::{Conductor2d, Group2d, field_2d};
        let height = float(inputs, "height")?;
        let offsets: Vec<f64> = inputs
            .get("offsets")
            .and_then(|v| v.as_array())
            .ok_or("input \"offsets\": a list of numbers")?
            .iter()
            .filter_map(|x| x.as_float().or_else(|| x.as_integer().map(|i| i as f64)))
            .collect();
        let mut wires = Vec::new();
        let mut volts = Vec::new();
        let mut conductors = Vec::new();
        for t in tables(inputs, "systems")? {
            let n = number(t, "subconductors")?.max(1.0) as usize;
            let spacing = number(t, "spacing")?;
            let d = number(t, "diameter")?;
            let kv = number(t, "kv")?;
            let amps = number(t, "amps")?;
            let beta = if t.contains_key("bundle_angle") {
                number(t, "bundle_angle")?.to_radians()
            } else {
                0.0
            };
            let phases = t
                .get("phases")
                .and_then(|v| v.as_array())
                .ok_or("input \"phases\": a list of [x, y]")?;
            let np = phases.len().max(1) as f64;
            let rb = if n > 1 {
                spacing / 2.0 / (PI / n as f64).sin()
            } else {
                0.0
            };
            for (k, ph) in phases.iter().enumerate() {
                let xy = numbers(Some(ph), "phase", 2)?;
                let angle = -2.0 * PI * k as f64 / np;
                let (_, radius) = matrices::bundle(n as u32, Some(spacing), d / 2.0, d / 2.0);
                wires.push(Wire {
                    across: xy[0],
                    height: xy[1],
                    gmr: radius,
                    radius,
                    resistance: 0.0,
                });
                volts.push(Complex::from_polar(kv * 1e3 / 3f64.sqrt(), angle));
                for i in 0..n {
                    let a = beta + 2.0 * PI * i as f64 / n as f64;
                    conductors.push(Conductor2d {
                        y: xy[0] + rb * a.cos(),
                        z: xy[1] + rb * a.sin(),
                        current: Complex::from_polar(amps / n as f64, angle),
                    });
                }
            }
        }
        if let Some(list) = inputs.get("earth_wires").and_then(|v| v.as_array()) {
            for e in list {
                let t = e.as_table().ok_or("earth_wires: tables")?;
                let r = number(t, "diameter")? / 2.0;
                wires.push(Wire {
                    across: number(t, "x")?,
                    height: number(t, "y")?,
                    gmr: r,
                    radius: r,
                    resistance: 0.0,
                });
                volts.push(Complex::new(0.0, 0.0));
            }
        }
        let q = charges(&wires, &volts).ok_or("the potential coefficients are singular")?;
        let group = [Group2d {
            frequency_hz: 50.0,
            conductors,
        }];
        let mut out = Outputs::new();
        let (mut e_max, mut b_max) = (0.0f64, 0.0f64);
        for (k, &x) in offsets.iter().enumerate() {
            let mut e = [Complex::new(0.0, 0.0); 2];
            for (w, qk) in wires.iter().zip(&q) {
                let f = line_field(w.across, w.height, x, height);
                e[0] += qk * f[0];
                e[1] += qk * f[1];
            }
            let e = (e[0].norm_sqr() + e[1].norm_sqr()).sqrt();
            let (b, _) = field_2d(&group, x, height);
            out.insert(format!("e_v_m_{k:02}"), e);
            out.insert(format!("b_ut_{k:02}"), b * 1e6);
            e_max = e_max.max(e);
            b_max = b_max.max(b * 1e6);
        }
        out.insert("e_max_v_m".into(), e_max);
        out.insert("b_max_ut".into(), b_max);
        Ok(out)
    }
}

/// `electrical.surface_gradient_2d`: the conductor surface gradient of a
/// cross-section by the program's charge simulation
/// ([`ohl_electrical::surface_gradient`]), from inputs as hvlbuzz (FKH)
/// takes them (`systems`, `earth_wires` as `electrical.fields_2d`;
/// optional `charges`, line charges per subconductor, default the
/// program's). Outputs per bundle k (the systems' phases in order, then
/// the earth wires; `_00`, `_01`, …), kV/cm rms: `avg_max_kv_cm_k` (the
/// mean over its subconductors of each one's largest gradient),
/// `max_kv_cm_k` (the largest), `markt_mengele_kv_cm_k` (Markt–Mengele's
/// from the bundle's charge of the equivalent wires,
/// [`ohl_electrical::electric::charges`]); and `potential_error` (the
/// simulation's largest potential error between collocation points, a
/// share of the largest voltage).
struct SurfaceGradient2d;

impl Calculation for SurfaceGradient2d {
    fn id(&self) -> &'static str {
        "electrical.surface_gradient_2d"
    }

    fn description(&self) -> &'static str {
        "Conductor surface gradient of a cross-section (2D charge simulation), inputs as hvlbuzz takes them"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        use nalgebra::Complex;
        use ohl_electrical::electric::charges;
        use ohl_electrical::surface_gradient::{
            CHARGES, Cylinder, PLACEMENT, markt_mengele, solve, surface_gradient,
        };
        let n = if inputs.contains_key("charges") {
            float(inputs, "charges")? as usize
        } else {
            CHARGES
        };
        // Per bundle: its cylinders' range, subconductor count, radius,
        // bundle radius.
        let mut bundles: Vec<(std::ops::Range<usize>, u32, f64, f64)> = Vec::new();
        let mut cyl = Vec::new();
        let mut wires = Vec::new();
        let mut volts = Vec::new();
        for t in tables(inputs, "systems")? {
            let nb = number(t, "subconductors")?.max(1.0) as usize;
            let spacing = number(t, "spacing")?;
            let d = number(t, "diameter")?;
            let kv = number(t, "kv")?;
            let beta = if t.contains_key("bundle_angle") {
                number(t, "bundle_angle")?.to_radians()
            } else {
                0.0
            };
            let phases = t
                .get("phases")
                .and_then(|v| v.as_array())
                .ok_or("input \"phases\": a list of [x, y]")?;
            let np = phases.len().max(1) as f64;
            let rb = if nb > 1 {
                spacing / 2.0 / (PI / nb as f64).sin()
            } else {
                0.0
            };
            for (k, ph) in phases.iter().enumerate() {
                let xy = numbers(Some(ph), "phase", 2)?;
                let v = Complex::from_polar(kv * 1e3 / 3f64.sqrt(), -2.0 * PI * k as f64 / np);
                let start = cyl.len();
                for i in 0..nb {
                    let a = beta + 2.0 * PI * i as f64 / nb as f64;
                    cyl.push(Cylinder {
                        y: xy[0] + rb * a.cos(),
                        z: xy[1] + rb * a.sin(),
                        radius: d / 2.0,
                        volts: v,
                    });
                }
                bundles.push((start..cyl.len(), nb as u32, d / 2.0, rb));
                let (_, radius) = matrices::bundle(nb as u32, Some(spacing), d / 2.0, d / 2.0);
                wires.push(Wire {
                    across: xy[0],
                    height: xy[1],
                    gmr: radius,
                    radius,
                    resistance: 0.0,
                });
                volts.push(v);
            }
        }
        if let Some(list) = inputs.get("earth_wires").and_then(|v| v.as_array()) {
            for e in list {
                let t = e.as_table().ok_or("earth_wires: tables")?;
                let r = number(t, "diameter")? / 2.0;
                let (x, y) = (number(t, "x")?, number(t, "y")?);
                let start = cyl.len();
                cyl.push(Cylinder {
                    y: x,
                    z: y,
                    radius: r,
                    volts: Complex::new(0.0, 0.0),
                });
                bundles.push((start..cyl.len(), 1, r, 0.0));
                wires.push(Wire {
                    across: x,
                    height: y,
                    gmr: r,
                    radius: r,
                    resistance: 0.0,
                });
                volts.push(Complex::new(0.0, 0.0));
            }
        }
        let sol = solve(&cyl, n, true, PLACEMENT).ok_or("the charge simulation is singular")?;
        let q = charges(&wires, &volts).ok_or("the potential coefficients are singular")?;
        let vmax = volts.iter().map(|v| v.norm()).fold(0.0, f64::max);
        let mut out = Outputs::new();
        for (k, ((range, nb, r, rb), qk)) in bundles.iter().zip(&q).enumerate() {
            let maxima: Vec<f64> = range
                .clone()
                .map(|j| surface_gradient(std::slice::from_ref(&sol), j, 360).0 / 1e5)
                .collect();
            out.insert(
                format!("avg_max_kv_cm_{k:02}"),
                maxima.iter().sum::<f64>() / maxima.len() as f64,
            );
            out.insert(
                format!("max_kv_cm_{k:02}"),
                maxima.iter().copied().fold(0.0, f64::max),
            );
            out.insert(
                format!("markt_mengele_kv_cm_{k:02}"),
                markt_mengele(qk.norm(), *nb, *r, *rb) / 1e5,
            );
        }
        out.insert(
            "potential_error".into(),
            if vmax > 0.0 {
                sol.potential_error() / vmax
            } else {
                0.0
            },
        );
        Ok(out)
    }
}

/// `electrical.cylinder_gradient`: the largest surface gradient of a
/// smooth cylinder (radius `radius`, m) at `kv` to earth (kV) whose centre
/// is `height` (m) above a perfectly conducting plane — or, with
/// `opposite = 1`, of one of two cylinders at ±`kv` in free space whose
/// centres are 2 × `height` apart — by the program's charge simulation
/// (optional `charges`, default the program's). Outputs: `e_max_kv_cm`
/// (largest), `e_min_kv_cm` (smallest round it), `angle_deg` (of the
/// largest, from the horizontal), `potential_error` (a share of `kv`).
struct CylinderGradient;

impl Calculation for CylinderGradient {
    fn id(&self) -> &'static str {
        "electrical.cylinder_gradient"
    }

    fn description(&self) -> &'static str {
        "Surface gradient of a cylinder over a conducting plane, or of two at opposite voltages (charge simulation)"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        use nalgebra::Complex;
        use ohl_electrical::surface_gradient::{
            CHARGES, Cylinder, PLACEMENT, solve, surface_gradient,
        };
        let n = if inputs.contains_key("charges") {
            float(inputs, "charges")? as usize
        } else {
            CHARGES
        };
        let h = float(inputs, "height")?;
        let r = float(inputs, "radius")?;
        let v = float(inputs, "kv")? * 1e3;
        let opposite = inputs.contains_key("opposite") && float(inputs, "opposite")? != 0.0;
        let mut cyl = vec![Cylinder {
            y: 0.0,
            z: h,
            radius: r,
            volts: Complex::new(v, 0.0),
        }];
        if opposite {
            cyl.push(Cylinder {
                y: 0.0,
                z: -h,
                radius: r,
                volts: Complex::new(-v, 0.0),
            });
        }
        let sol =
            solve(&cyl, n, !opposite, PLACEMENT).ok_or("the charge simulation is singular")?;
        let (max, angle, _) = surface_gradient(std::slice::from_ref(&sol), 0, 360);
        let min = (0..360)
            .map(|i| sol.surface_field(0, 2.0 * PI * i as f64 / 360.0).0.norm())
            .fold(f64::INFINITY, f64::min);
        let mut out = Outputs::new();
        out.insert("e_max_kv_cm".into(), max / 1e5);
        out.insert("e_min_kv_cm".into(), min / 1e5);
        out.insert("angle_deg".into(), angle);
        out.insert("potential_error".into(), sol.potential_error() / v.abs());
        Ok(out)
    }
}

/// A list of numbers (`[x, y, z]`).
fn numbers(v: Option<&toml::Value>, what: &str, n: usize) -> Result<Vec<f64>, String> {
    let list = v
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("input \"{what}\": a list of {n} numbers"))?;
    let out: Vec<f64> = list
        .iter()
        .filter_map(|x| x.as_float().or_else(|| x.as_integer().map(|i| i as f64)))
        .collect();
    if out.len() != n {
        return Err(format!("input \"{what}\": a list of {n} numbers"));
    }
    Ok(out)
}

fn number(t: &toml::Table, name: &str) -> Result<f64, String> {
    let v = t
        .get(name)
        .ok_or_else(|| format!("missing input \"{name}\""))?;
    v.as_float()
        .or_else(|| v.as_integer().map(|i| i as f64))
        .ok_or_else(|| format!("input \"{name}\" must be a number"))
}

/// A current (A, rms) at a phase angle (degrees, default 0) as a phasor.
fn phasor(t: &toml::Table) -> Result<nalgebra::Complex<f64>, String> {
    let angle = if t.contains_key("angle") {
        number(t, "angle")?
    } else {
        0.0
    };
    Ok(nalgebra::Complex::from_polar(
        number(t, "current")?,
        angle.to_radians(),
    ))
}

fn tables<'a>(inputs: &'a Inputs, name: &str) -> Result<Vec<&'a toml::Table>, String> {
    inputs
        .get(name)
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("missing input \"{name}\" (a list of tables)"))?
        .iter()
        .enumerate()
        .map(|(k, v)| v.as_table().ok_or_else(|| format!("{name} {k}: a table")))
        .collect()
}

/// `electrical.magnetic_segments`: the flux density at `point` (`[x, y,
/// z]`, m) of straight `segments` (`{ a = [x, y, z], b = [x, y, z],
/// current = A, angle = deg }`, the current flowing from a to b) by the
/// program's closed form of Biot–Savart for a finite segment, summed as
/// phasors. Outputs in µT: `bx_ut`, `by_ut`, `bz_ut` (each component's
/// rms magnitude), `b_ut` (the rms of the vector), `b_major_ut` (the
/// largest over the cycle).
struct MagneticSegments;

impl Calculation for MagneticSegments {
    fn id(&self) -> &'static str {
        "electrical.magnetic_segments"
    }

    fn description(&self) -> &'static str {
        "Magnetic flux density of straight finite segments (Biot–Savart, closed form)"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        use ohl_electrical::magnetic::{Group, Segment, rms_and_major, segment_field};
        let p = numbers(inputs.get("point"), "point", 3)?;
        let p = [p[0], p[1], p[2]];
        let mut segments = Vec::new();
        for (k, t) in tables(inputs, "segments")?.into_iter().enumerate() {
            let a = numbers(t.get("a"), &format!("segment {k} a"), 3)?;
            let b = numbers(t.get("b"), &format!("segment {k} b"), 3)?;
            segments.push(Segment {
                a: [a[0], a[1], a[2]],
                b: [b[0], b[1], b[2]],
                current: phasor(t)?,
            });
        }
        let mut field = [nalgebra::Complex::new(0.0, 0.0); 3];
        for s in &segments {
            let f = segment_field(s.a, s.b, p);
            for k in 0..3 {
                field[k] += s.current * f[k];
            }
        }
        let (rms, major) = rms_and_major(&field);
        // The same through the program's group sum.
        let (b, _) = ohl_electrical::magnetic::field_3d(
            &[Group {
                frequency_hz: 50.0,
                segments,
            }],
            p,
        );
        if (b - rms).abs() > 1e-12 * rms.max(1e-30) {
            return Err(format!("group sum {b} differs from {rms}"));
        }
        Ok([
            ("bx_ut".to_owned(), field[0].norm() * 1e6),
            ("by_ut".to_owned(), field[1].norm() * 1e6),
            ("bz_ut".to_owned(), field[2].norm() * 1e6),
            ("b_ut".to_owned(), rms * 1e6),
            ("b_major_ut".to_owned(), major * 1e6),
        ]
        .into_iter()
        .collect())
    }
}

/// `electrical.magnetic_2d`: the flux density at `point` (`[y, z]`, m) of
/// infinite straight parallel `conductors` (`{ y, z, current = A, angle =
/// deg }`, m) by the program's 2D model. Outputs `b_ut`, `b_major_ut`.
struct Magnetic2d;

impl Calculation for Magnetic2d {
    fn id(&self) -> &'static str {
        "electrical.magnetic_2d"
    }

    fn description(&self) -> &'static str {
        "Magnetic flux density of infinite straight parallel conductors (2D)"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        use ohl_electrical::magnetic::{Conductor2d, Group2d, field_2d};
        let p = numbers(inputs.get("point"), "point", 2)?;
        let mut conductors = Vec::new();
        for t in tables(inputs, "conductors")? {
            conductors.push(Conductor2d {
                y: number(t, "y")?,
                z: number(t, "z")?,
                current: phasor(t)?,
            });
        }
        let (b, major) = field_2d(
            &[Group2d {
                frequency_hz: 50.0,
                conductors,
            }],
            p[0],
            p[1],
        );
        Ok([
            ("b_ut".to_owned(), b * 1e6),
            ("b_major_ut".to_owned(), major * 1e6),
        ]
        .into_iter()
        .collect())
    }
}

fn float(inputs: &Inputs, name: &str) -> Result<f64, String> {
    let v = inputs
        .get(name)
        .ok_or_else(|| format!("missing input \"{name}\""))?;
    v.as_float()
        .or_else(|| v.as_integer().map(|i| i as f64))
        .ok_or_else(|| format!("input \"{name}\" must be a number"))
}

fn text<'a>(table: &'a toml::Table, name: &str) -> Result<&'a str, String> {
    table
        .get(name)
        .ok_or_else(|| format!("missing input \"{name}\""))?
        .as_str()
        .ok_or_else(|| format!("input \"{name}\" must be text"))
}

fn length(table: &toml::Table, name: &str) -> Result<f64, String> {
    text(table, name)?
        .parse::<Length>()
        .map(|l| l.si())
        .map_err(|e| format!("input \"{name}\": {e}"))
}

fn method(inputs: &Inputs) -> Result<EarthReturn, String> {
    let m = inputs
        .get("earth_return")
        .and_then(|v| v.as_str())
        .unwrap_or("carson_series");
    EarthReturn::parse(m).ok_or_else(|| {
        format!("earth_return \"{m}\": carson_series, complex_depth or carson_first_terms")
    })
}

/// `electrical.earth_return`: the earth's finite conductivity term
/// \\( \Delta Z = Z - j\frac{\omega\mu_0}{2\pi}\ln\frac{D'}{d} \\) between a
/// wire at height `hi` and one at `hj`, `x` apart across the line
/// (lengths with units), at `frequency` (Hz) over `resistivity` (Ω·m), by
/// `earth_return`. Outputs `dr_ohm_per_km`, `dx_ohm_per_km` (its real and
/// imaginary parts), Carson's parameter `a` and for Carson's series also
/// `p`, `q`.
struct EarthTerms;

impl Calculation for EarthTerms {
    fn id(&self) -> &'static str {
        "electrical.earth_return"
    }

    fn description(&self) -> &'static str {
        "Earth return term between two wires (Carson's series, complex depth, first terms)"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        let (hi, hj, x) = (
            length(inputs, "hi")?,
            length(inputs, "hj")?,
            length(inputs, "x")?,
        );
        let (f, rho) = (float(inputs, "frequency")?, float(inputs, "resistivity")?);
        let m = method(inputs)?;
        // d = 1 m: the term ln(D'/d) is taken out again below.
        let z = earth::impedance(m, [0.0, hi], [x, hj], 1.0, f, rho);
        let image = (hi + hj).hypot(x);
        let k = 2.0 * PI * f * earth::MU0 / (2.0 * PI);
        let dz = z - nalgebra::Complex::new(0.0, k * image.ln());
        let a = earth::carson_parameter(image, f, rho);
        let mut out: Outputs = [
            ("dr_ohm_per_km".to_owned(), dz.re * 1000.0),
            ("dx_ohm_per_km".to_owned(), dz.im * 1000.0),
            ("a".to_owned(), a),
        ]
        .into_iter()
        .collect();
        if m == EarthReturn::CarsonSeries {
            let (p, q) = earth::carson_pq(a, x.abs().atan2(hi + hj));
            out.insert("p".into(), p);
            out.insert("q".into(), q);
        }
        Ok(out)
    }
}

/// `electrical.parameters`: the per-unit-length phase matrices of a set of
/// parallel wires (`conductors`, each `{ name, x, y, gmr, radius,
/// resistance, earthed?, bundle?, spacing? }`, lengths and resistances
/// with units; `y` the height above the earth) at `frequency` (Hz) over
/// `resistivity` (Ω·m) by `earth_return`, the earthed wires reduced out.
/// Values per `per_length` (default `1 km`):
/// - `z_<i><j>_r`, `z_<i><j>_x` Ω, `p_<i><j>` per µF (the reduced
///   potential coefficients), `c_<i><j>` µF, `b_<i><j>` µS, for the phase
///   wires by name (i ≤ j);
/// - with three phase wires (taken as a, b, c in the order given): the
///   ideally transposed `z1_r`, `z1_x`, `z0_r`, `z0_x`, `c1_uf`, `c0_uf`
///   (the capacitances averaged), `c1_uf_textbook` (the potential
///   coefficients averaged, as textbooks do),
///   `l1_mh` (positive sequence inductance, mH) and as built the sequence
///   matrix's diagonal `z1_built_{r,x}`, `z2_built_{r,x}`, `z0_built_{r,x}`.
struct Parameters;

impl Calculation for Parameters {
    fn id(&self) -> &'static str {
        "electrical.parameters"
    }

    fn description(&self) -> &'static str {
        "Phase matrices and sequence values of parallel wires (earth wires reduced out)"
    }

    fn run(&self, inputs: &Inputs) -> Result<Outputs, String> {
        let (f, rho) = (float(inputs, "frequency")?, float(inputs, "resistivity")?);
        let m = method(inputs)?;
        let per = match inputs.get("per_length") {
            Some(_) => length(inputs, "per_length")?,
            None => 1000.0,
        };
        let list = inputs
            .get("conductors")
            .and_then(|v| v.as_array())
            .ok_or("missing input \"conductors\" (a list of tables)")?;
        let mut wires = Vec::new();
        let mut names = Vec::new();
        let mut phases = Vec::new();
        for (k, c) in list.iter().enumerate() {
            let t = c
                .as_table()
                .ok_or_else(|| format!("conductor {k}: a table"))?;
            let r = text(t, "resistance")?
                .parse::<ResistancePerLength>()
                .map_err(|e| format!("conductor {k}: resistance: {e}"))?
                .si();
            let n = t
                .get("bundle")
                .and_then(|v| v.as_integer())
                .unwrap_or(1)
                .max(1) as u32;
            let spacing = if t.contains_key("spacing") {
                Some(length(t, "spacing")?)
            } else {
                None
            };
            let (gmr, radius) =
                matrices::bundle(n, spacing, length(t, "gmr")?, length(t, "radius")?);
            wires.push(Wire {
                across: length(t, "x")?,
                height: length(t, "y")?,
                gmr,
                radius,
                resistance: r / f64::from(n),
            });
            names.push(text(t, "name")?.to_owned());
            if !t.get("earthed").and_then(|v| v.as_bool()).unwrap_or(false) {
                phases.push(k);
            }
        }
        let z = matrices::kron(&matrices::impedance(&wires, m, f, rho), &phases)
            .ok_or("the earthed wires' matrix is singular")?;
        let p = matrices::kron(&matrices::complex(&matrices::potential(&wires)), &phases)
            .ok_or("the earthed wires' potential coefficients are singular")?
            .map(|x| x.re);
        let c = p
            .clone()
            .try_inverse()
            .ok_or("the potential coefficients are singular")?;
        let omega = 2.0 * PI * f;
        let mut out = Outputs::new();
        for i in 0..phases.len() {
            for j in i..phases.len() {
                let key = format!("{}{}", names[phases[i]], names[phases[j]]);
                out.insert(format!("z_{key}_r"), z[(i, j)].re * per);
                out.insert(format!("z_{key}_x"), z[(i, j)].im * per);
                out.insert(format!("p_{key}"), p[(i, j)] / per * 1e-6);
                out.insert(format!("c_{key}"), c[(i, j)] * per * 1e6);
                out.insert(format!("b_{key}"), omega * c[(i, j)] * per * 1e6);
            }
        }
        if phases.len() == 3 {
            let (z1, z0) = matrices::transposed(&z);
            let (c1, c0) = matrices::transposed(&matrices::complex(&c));
            let s = matrices::sequence(&z);
            for (name, v) in [
                ("z1_r", z1.re),
                ("z1_x", z1.im),
                ("z0_r", z0.re),
                ("z0_x", z0.im),
                ("z1_built_r", s[(1, 1)].re),
                ("z1_built_x", s[(1, 1)].im),
                ("z2_built_r", s[(2, 2)].re),
                ("z2_built_x", s[(2, 2)].im),
                ("z0_built_r", s[(0, 0)].re),
                ("z0_built_x", s[(0, 0)].im),
            ] {
                out.insert(name.into(), v * per);
            }
            out.insert("c1_uf".into(), c1.re * per * 1e6);
            // The textbooks' transposition: the potential coefficients
            // averaged (equal charges along the line), then inverted.
            let (p1, _) = matrices::transposed(&matrices::complex(&p));
            out.insert("c1_uf_textbook".into(), 1.0 / p1.re * per * 1e6);
            out.insert("c0_uf".into(), c0.re * per * 1e6);
            out.insert("l1_mh".into(), z1.im / omega * per * 1e3);
        }
        Ok(out)
    }
}
