// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! As-built wires: catenaries fitted to LiDAR points of wire classes, one
//! per span, compared with the model — and the conductor temperature at
//! the time of the survey, from the model's sag–temperature relation.
//!
//! **Points of a span** ([`WireCollector`]): points of the wire classes
//! within `lateral` (plan) of the span's vertical plane — the plane of the
//! modelled wire's chord in a still-air case — within `vertical` of the
//! modelled wire's height there, and away from the ends by `end_margin` of
//! the span (clamps, jumpers, insulators). Each point becomes (s, z): its
//! horizontal distance along the chord from the first clamp, and its
//! elevation. Points of a bundle's subconductors are fitted together (the
//! fit runs through their middle); still air at the survey is assumed.
//!
//! **Fit** ([`fit_catenary`]): z(s) = z₀ + c·(cosh((s − s₀)/c) − 1), the
//! catenary with low point (s₀, z₀) and catenary constant c = H/w, by
//! least squares — a parabola for the start, then Gauss–Newton with
//! Levenberg–Marquardt damping (Marquardt 1963), exit clauses on
//! iterations and time; points further than three RMS from the first fit
//! are dropped and the fit repeated once.
//!
//! **Sag** of the fitted wire: the largest vertical distance below the
//! chord between its ends at s = 0 and s = L — the model's definition
//! ([`ols_line::solve::SpanShape::sag`]). Where the catenary's slope
//! equals the chord's, sinh((s − s₀)/c) = Δz/L, so the sag is found in
//! closed form.
//!
//! **Temperature** ([`estimate_temperature`]): the section is solved in
//! still air at trial temperatures (its stringing, creep and clamping as
//! modelled) until the span's sag matches the fitted sag — regula falsi
//! (Illinois variant) on sag(T), which grows with T.

use ols_cables::Cable;
use ols_core::budget::Budget;
use ols_core::units::Temperature;
use ols_line::{Line, Section};
use ols_weather::WeatherCase;

use crate::checks::{Wire, WireSet};
use crate::geometry::P3;

/// Which points belong to a span.
#[derive(Debug, Clone, PartialEq)]
pub struct FitParams {
    /// Largest plan distance from the span's vertical plane, m.
    pub lateral: f64,
    /// Largest height difference from the modelled wire, m.
    pub vertical: f64,
    /// Share of the span left out at each end (0–0.45).
    pub end_margin: f64,
    /// Fewest points for a fit.
    pub min_points: usize,
}

impl Default for FitParams {
    fn default() -> Self {
        Self {
            lateral: 1.0,
            vertical: 3.0,
            end_margin: 0.05,
            min_points: 20,
        }
    }
}

/// A catenary in a span's vertical plane: z(s) = z₀ + c·(cosh((s − s₀)/c) − 1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaneCatenary {
    /// Catenary constant H/w, m.
    pub c: f64,
    /// Low point: distance along the span, m.
    pub s0: f64,
    /// Low point: elevation, m.
    pub z0: f64,
}

impl PlaneCatenary {
    /// Elevation at `s`.
    pub fn z(&self, s: f64) -> f64 {
        self.z0 + self.c * (((s - self.s0) / self.c).cosh() - 1.0)
    }

    /// Largest vertical distance below the chord from s = `a` to s = `b`.
    pub fn sag(&self, a: f64, b: f64) -> f64 {
        let (za, zb) = (self.z(a), self.z(b));
        let m = (zb - za) / (b - a);
        let s = (self.s0 + self.c * m.asinh()).clamp(a, b);
        za + m * (s - a) - self.z(s)
    }
}

/// A fitted catenary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CatenaryFit {
    /// The curve.
    pub curve: PlaneCatenary,
    /// Root mean square of the vertical residuals, m.
    pub rms: f64,
    /// Points used.
    pub points: usize,
    /// Points dropped as outliers.
    pub dropped: usize,
    /// Iterations of the last fit.
    pub iterations: u64,
}

/// Solves a 3 × 3 system by Gaussian elimination with partial pivoting.
fn solve3(mut a: [[f64; 3]; 3], mut b: [f64; 3]) -> Option<[f64; 3]> {
    for col in 0..3 {
        let pivot = (col..3).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[pivot][col].abs() < 1e-300 {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        for row in col + 1..3 {
            let f = a[row][col] / a[col][col];
            let pivot_row = a[col];
            for (x, p) in a[row].iter_mut().zip(pivot_row).skip(col) {
                *x -= f * p;
            }
            b[row] -= f * b[col];
        }
    }
    let mut x = [0.0; 3];
    for row in (0..3).rev() {
        let s: f64 = (row + 1..3).map(|k| a[row][k] * x[k]).sum();
        x[row] = (b[row] - s) / a[row][row];
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

/// Least-squares parabola z = a·u² + b·u + e in u = s − mean(s): the start.
fn parabola(s: &[f64], z: &[f64]) -> Option<PlaneCatenary> {
    let mean = s.iter().sum::<f64>() / s.len() as f64;
    let mut m = [[0.0; 3]; 3];
    let mut r = [0.0; 3];
    for (&si, &zi) in s.iter().zip(z) {
        let u = si - mean;
        let row = [u * u, u, 1.0];
        for i in 0..3 {
            for j in 0..3 {
                m[i][j] += row[i] * row[j];
            }
            r[i] += row[i] * zi;
        }
    }
    let [a, b, e] = solve3(m, r)?;
    (a > 0.0).then(|| PlaneCatenary {
        c: 1.0 / (2.0 * a),
        s0: mean - b / (2.0 * a),
        z0: e - b * b / (4.0 * a),
    })
}

fn sse(curve: &PlaneCatenary, s: &[f64], z: &[f64]) -> f64 {
    s.iter()
        .zip(z)
        .map(|(&si, &zi)| (zi - curve.z(si)).powi(2))
        .sum()
}

/// One least-squares fit from `start`; iterations used.
fn refine(
    start: PlaneCatenary,
    s: &[f64],
    z: &[f64],
    budget: &Budget,
) -> Result<(PlaneCatenary, u64), String> {
    let mut run = budget.start();
    let mut p = start;
    let mut err = sse(&p, s, z);
    let mut lambda = 1e-3;
    loop {
        run.step()
            .map_err(|e| format!("catenary fit stopped: {e}"))?;
        // Normal equations Jᵀ J δ = Jᵀ r, r = z − f.
        let mut jtj = [[0.0; 3]; 3];
        let mut jtr = [0.0; 3];
        for (&si, &zi) in s.iter().zip(z) {
            let u = (si - p.s0) / p.c;
            let (sh, ch) = (u.sinh(), u.cosh());
            let j = [ch - 1.0 - u * sh, -sh, 1.0];
            let r = zi - p.z(si);
            for a in 0..3 {
                for b in 0..3 {
                    jtj[a][b] += j[a] * j[b];
                }
                jtr[a] += j[a] * r;
            }
        }
        let mut improved = false;
        for _ in 0..30 {
            let mut m = jtj;
            for (k, row) in m.iter_mut().enumerate() {
                row[k] += lambda * jtj[k][k].max(1e-12);
            }
            let Some(d) = solve3(m, jtr) else {
                lambda *= 10.0;
                continue;
            };
            let trial = PlaneCatenary {
                c: p.c + d[0],
                s0: p.s0 + d[1],
                z0: p.z0 + d[2],
            };
            let e = if trial.c > 0.0 {
                sse(&trial, s, z)
            } else {
                f64::INFINITY
            };
            if e <= err {
                let small = d[0].abs() <= 1e-10 * p.c
                    && d[1].abs() <= 1e-9 * (1.0 + p.s0.abs())
                    && d[2].abs() <= 1e-9 * (1.0 + p.z0.abs());
                let flat = err - e <= 1e-15 * (1.0 + err);
                p = trial;
                err = e;
                lambda = (lambda / 10.0).max(1e-12);
                improved = true;
                if small || flat {
                    return Ok((p, run.iterations()));
                }
                break;
            }
            lambda *= 10.0;
        }
        if !improved {
            // No step lowers the error: a minimum (to rounding).
            return Ok((p, run.iterations()));
        }
    }
}

/// Fits a catenary to points (s, z) of one span. Exit clause: `budget`,
/// one step per iteration (about 10 iterations are usual).
///
/// Cost: per iteration one pass over the points (cosh, sinh): about 50 µs
/// for 1000 points.
pub fn fit_catenary(s: &[f64], z: &[f64], budget: &Budget) -> Result<CatenaryFit, String> {
    if s.len() != z.len() || s.len() < 4 {
        return Err(format!("{} points: too few to fit", s.len().min(z.len())));
    }
    let start = parabola(s, z).ok_or("the points do not hang in a curve")?;
    let (curve, iterations) = refine(start, s, z, budget)?;
    let rms = (sse(&curve, s, z) / s.len() as f64).sqrt();
    // Outliers: further than three RMS (at least 5 cm) from the curve.
    let limit = (3.0 * rms).max(0.05);
    let keep: Vec<usize> = (0..s.len())
        .filter(|&k| (z[k] - curve.z(s[k])).abs() <= limit)
        .collect();
    if keep.len() == s.len() || keep.len() < 4 {
        return Ok(CatenaryFit {
            curve,
            rms,
            points: s.len(),
            dropped: 0,
            iterations,
        });
    }
    let (s2, z2): (Vec<f64>, Vec<f64>) = keep.iter().map(|&k| (s[k], z[k])).unzip();
    let (curve, iterations) = refine(curve, &s2, &z2, budget)?;
    Ok(CatenaryFit {
        curve,
        rms: (sse(&curve, &s2, &z2) / s2.len() as f64).sqrt(),
        points: s2.len(),
        dropped: s.len() - s2.len(),
        iterations,
    })
}

/// A span's vertical plane and its modelled wire in it.
#[derive(Debug, Clone)]
struct Frame {
    origin: P3,
    along: [f64; 2],
    right: [f64; 2],
    length: f64,
    /// The modelled wire: distance along, elevation.
    s: Vec<f64>,
    z: Vec<f64>,
}

impl Frame {
    fn of(points: &[P3]) -> Option<Self> {
        let (a, b) = (*points.first()?, *points.last()?);
        let length = (b[0] - a[0]).hypot(b[1] - a[1]);
        if length <= 0.0 {
            return None;
        }
        let along = [(b[0] - a[0]) / length, (b[1] - a[1]) / length];
        let s: Vec<f64> = points
            .iter()
            .map(|p| (p[0] - a[0]) * along[0] + (p[1] - a[1]) * along[1])
            .collect();
        Some(Self {
            origin: a,
            along,
            right: [along[1], -along[0]],
            length,
            s,
            z: points.iter().map(|p| p[2]).collect(),
        })
    }

    /// The modelled wire's elevation at `s`.
    fn model_z(&self, s: f64) -> f64 {
        let k = self
            .s
            .partition_point(|&v| v < s)
            .clamp(1, self.s.len() - 1);
        let (s0, s1) = (self.s[k - 1], self.s[k]);
        let f = if s1 > s0 { (s - s0) / (s1 - s0) } else { 0.0 };
        self.z[k - 1] + (self.z[k] - self.z[k - 1]) * f
    }

    /// The modelled wire's sag (largest distance below its chord).
    fn model_sag(&self) -> f64 {
        let n = self.s.len();
        let (s0, z0, s1, z1) = (self.s[0], self.z[0], self.s[n - 1], self.z[n - 1]);
        self.s
            .iter()
            .zip(&self.z)
            .map(|(&s, &z)| z0 + (z1 - z0) * (s - s0) / (s1 - s0) - z)
            .fold(0.0, f64::max)
    }

    fn point(&self, s: f64, z: f64) -> P3 {
        [
            self.origin[0] + self.along[0] * s,
            self.origin[1] + self.along[1] * s,
            z,
        ]
    }
}

/// Gathers the points of wire classes span by span: feed points with
/// [`WireCollector::visit`], then [`WireCollector::fit`].
///
/// Cost: per point, the few wire segments of its 10 m cell — about 0.2 µs.
#[derive(Debug, Clone)]
pub struct WireCollector {
    params: FitParams,
    classes: [bool; 256],
    set: WireSet,
    frames: Vec<Option<Frame>>,
    /// Points gathered for each wire: (s, z).
    pub points: Vec<(Vec<f64>, Vec<f64>)>,
    seen: Vec<usize>,
}

/// One span's fitted wire compared with the model.
#[derive(Debug, Clone, PartialEq)]
pub struct SpanFit {
    /// The fit, in the span's plane (s from the first clamp of the model).
    pub fit: CatenaryFit,
    /// Horizontal span length (clamp to clamp, model), m.
    pub length: f64,
    /// Sag of the fitted wire, m.
    pub sag: f64,
    /// Sag of the modelled wire, m.
    pub model_sag: f64,
    /// Fitted minus modelled height at mid-span, m.
    pub mid_difference: f64,
    /// The fitted wire's low point (within the span; else the lower end).
    pub low_point: P3,
    /// Fitted minus modelled low point height, m.
    pub low_difference: f64,
}

impl WireCollector {
    /// A collector for `wires` (a still-air case: their chords' planes)
    /// and points of `classes`.
    pub fn new(wires: Vec<Wire>, classes: &[u8], params: FitParams) -> Self {
        let mut mask = [false; 256];
        for &c in classes {
            mask[c as usize] = true;
        }
        let frames = wires
            .iter()
            .map(|w| {
                if w.section.starts_with("jumper:") {
                    None
                } else {
                    Frame::of(&w.points)
                }
            })
            .collect();
        let n = wires.len();
        Self {
            set: WireSet::new(wires, params.lateral),
            params,
            classes: mask,
            frames,
            points: vec![(Vec::new(), Vec::new()); n],
            seen: Vec::new(),
        }
    }

    /// The wires.
    pub fn wires(&self) -> &[Wire] {
        &self.set.wires
    }

    /// Takes one point into account.
    pub fn visit(&mut self, x: f64, y: f64, z: f64, class: u8) {
        if !self.classes[class as usize] {
            return;
        }
        self.seen.clear();
        for (w, _) in self.set.near(x, y) {
            if self.seen.contains(&w) {
                continue;
            }
            self.seen.push(w);
            let Some(f) = &self.frames[w] else { continue };
            let (dx, dy) = (x - f.origin[0], y - f.origin[1]);
            let s = dx * f.along[0] + dy * f.along[1];
            let d = dx * f.right[0] + dy * f.right[1];
            let m = self.params.end_margin.clamp(0.0, 0.45) * f.length;
            if d.abs() > self.params.lateral || s < m || s > f.length - m {
                continue;
            }
            if (z - f.model_z(s)).abs() > self.params.vertical {
                continue;
            }
            self.points[w].0.push(s);
            self.points[w].1.push(z);
        }
    }

    /// Fits every span with enough points (`None`: too few points); the
    /// fits' exit clause `budget` applies to each span.
    pub fn fit(&self, budget: &Budget) -> Vec<Option<Result<SpanFit, String>>> {
        self.frames
            .iter()
            .zip(&self.points)
            .map(|(f, (s, z))| {
                let f = f.as_ref()?;
                if s.len() < self.params.min_points.max(4) {
                    return None;
                }
                Some(fit_catenary(s, z, budget).map(|fit| {
                    let c = fit.curve;
                    let mid = f.length / 2.0;
                    let low_s = c.s0.clamp(0.0, f.length);
                    let model_low = f.z.iter().copied().fold(f64::INFINITY, f64::min);
                    SpanFit {
                        length: f.length,
                        sag: c.sag(0.0, f.length),
                        model_sag: f.model_sag(),
                        mid_difference: c.z(mid) - f.model_z(mid),
                        low_point: f.point(low_s, c.z(low_s)),
                        low_difference: c.z(low_s) - model_low,
                        fit,
                    }
                }))
            })
            .collect()
    }
}

/// The temperature at which the model's span `span` of `section` sags
/// `sag` (m) in still air, °C, searched within `range`; `Ok(None)` when
/// the sag is outside what the range gives. Exit clause: `budget`, one
/// step per section solve (about 6 are usual).
///
/// Cost: one section solve per step (finite elements: a few ms to tens of
/// ms per section).
pub fn estimate_temperature(
    line: &Line,
    section: &Section,
    cable: &Cable,
    span: usize,
    sag: f64,
    range: (f64, f64),
    budget: &Budget,
) -> Result<Option<f64>, String> {
    let mut run = budget.start();
    let mut sag_at = |t: f64| -> Result<f64, String> {
        run.step()
            .map_err(|e| format!("temperature search stopped: {e}"))?;
        let case = WeatherCase::still("survey", Temperature::from_celsius(t));
        let shape = ols_line::solve::solve_section(
            line,
            section,
            cable,
            &case,
            &ols_wirefe::default_fe_budget(),
        )
        .map_err(|e| e.to_string())?;
        shape
            .spans
            .get(span)
            .map(|s| s.sag - sag)
            .ok_or_else(|| format!("section {} has no span {span}", section.id))
    };
    let (mut a, mut b) = range;
    let (mut fa, mut fb) = (sag_at(a)?, sag_at(b)?);
    if fa > 0.0 || fb < 0.0 {
        return Ok(None);
    }
    // Illinois regula falsi on f(T) = sag(T) − sag.
    let mut side = 0i8;
    for _ in 0..60 {
        let t = if fb != fa {
            b - fb * (b - a) / (fb - fa)
        } else {
            (a + b) / 2.0
        };
        let ft = sag_at(t)?;
        if ft.abs() < 1e-4 || (b - a).abs() < 0.01 {
            return Ok(Some(t));
        }
        if (ft < 0.0) == (fa < 0.0) {
            a = t;
            fa = ft;
            if side == -1 {
                fb /= 2.0;
            }
            side = -1;
        } else {
            b = t;
            fb = ft;
            if side == 1 {
                fa /= 2.0;
            }
            side = 1;
        }
    }
    Ok(Some((a + b) / 2.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::wires_of;
    use std::path::Path;

    /// Deterministic noise in [−1, 1].
    struct Noise(u64);

    impl Noise {
        fn next(&mut self) -> f64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 11) as f64 / (1u64 << 52) as f64 - 1.0
        }
    }

    #[test]
    fn exact_catenary_is_recovered() {
        let truth = PlaneCatenary {
            c: 1400.0,
            s0: 180.0,
            z0: 512.0,
        };
        let s: Vec<f64> = (10..=290).map(f64::from).collect();
        let z: Vec<f64> = s.iter().map(|&v| truth.z(v)).collect();
        let fit = fit_catenary(&s, &z, &Budget::new().max_iterations(200)).unwrap();
        assert!((fit.curve.c - 1400.0).abs() < 1e-4, "{fit:?}");
        assert!((fit.curve.s0 - 180.0).abs() < 1e-6);
        assert!(fit.rms < 1e-8);
        // Sag by hand: the chord from s = 0 to 300 and the parallel tangent.
        let (z0, z1) = (truth.z(0.0), truth.z(300.0));
        let m = (z1 - z0) / 300.0;
        let st = 180.0 + 1400.0 * m.asinh();
        let by_hand = z0 + m * st - truth.z(st);
        assert!((truth.sag(0.0, 300.0) - by_hand).abs() < 1e-12);
        // Roughly L²/(8c) for a level span.
        assert!((truth.sag(0.0, 300.0) - 300.0f64.powi(2) / (8.0 * 1400.0)).abs() < 0.05);
        // Too few points; points that do not hang; the exit clause.
        assert!(fit_catenary(&s[..3], &z[..3], &Budget::new()).is_err());
        let up: Vec<f64> = s.iter().map(|v| -v * v).collect();
        assert!(fit_catenary(&s, &up, &Budget::new()).is_err());
        assert!(fit_catenary(&s, &z, &Budget::new().max_iterations(0)).is_err());
    }

    fn example() -> (Line, ols_cables::Library) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let line =
            Line::load(&root.join("examples/synthetic-line/synthetic-line.ols.json")).unwrap();
        let library = ols_cables::Library::load(&root.join("libraries/cables")).unwrap();
        (line, library)
    }

    fn wires_at(line: &Line, library: &ols_cables::Library, t: f64) -> Vec<Wire> {
        let case = WeatherCase::still("t", Temperature::from_celsius(t));
        let shapes: Vec<_> = ols_line::solve::solve_line(line, library, &case)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        wires_of(line, &shapes, &[])
    }

    #[test]
    fn surveyed_wires_give_sag_and_temperature() {
        // A survey at 35 °C: points every 0.5 m along each modelled span,
        // ±3 cm noise up and across, ground points and stray points of the
        // wire class around them.
        let (line, library) = example();
        let surveyed = wires_at(&line, &library, 35.0);
        let mut noise = Noise(0x9e37_79b9_7f4a_7c15);
        let mut points: Vec<(P3, u8)> = Vec::new();
        for w in &surveyed {
            for seg in w.points.windows(2) {
                let (a, b) = (seg[0], seg[1]);
                let n = ((b[0] - a[0]).hypot(b[1] - a[1]) / 0.5).ceil() as usize;
                for k in 0..n {
                    let t = k as f64 / n as f64;
                    let p = [
                        a[0] + (b[0] - a[0]) * t + 0.03 * noise.next(),
                        a[1] + (b[1] - a[1]) * t + 0.03 * noise.next(),
                        a[2] + (b[2] - a[2]) * t + 0.03 * noise.next(),
                    ];
                    points.push((p, 14));
                    points.push(([p[0], p[1], 480.0], 2));
                }
            }
            // Birds: points of the wire class 2 m above the wire.
            for k in 0..5 {
                let p = w.points[w.points.len() / 2 + k];
                points.push(([p[0], p[1], p[2] + 2.0], 14));
            }
        }
        // The model at 15 °C gives the spans' planes and heights.
        let model = wires_at(&line, &library, 15.0);
        let mut c = WireCollector::new(model.clone(), &[14], FitParams::default());
        for (p, class) in &points {
            c.visit(p[0], p[1], p[2], *class);
        }
        let fits = c.fit(&Budget::new().max_iterations(200));
        let budget = Budget::new().max_iterations(60);
        let mut checked = 0;
        for ((w, truth), fit) in model.iter().zip(&surveyed).zip(&fits) {
            let fit = fit.as_ref().expect("enough points").as_ref().unwrap();
            assert!(fit.fit.dropped >= 5, "{fit:?}");
            assert!(fit.fit.rms < 0.03, "{fit:?}");
            let truth_sag = Frame::of(&truth.points).unwrap().model_sag();
            assert!(
                (fit.sag - truth_sag).abs() < 0.02,
                "{} span {}: {} vs {truth_sag}",
                w.section,
                w.span,
                fit.sag
            );
            // Hotter than the model: lower.
            assert!(fit.mid_difference < 0.0 && fit.sag > fit.model_sag);
            let section = line.sections.iter().find(|s| s.id == w.section).unwrap();
            let cable = library.get(&section.cable).unwrap();
            let t = estimate_temperature(
                &line,
                section,
                cable,
                w.span,
                fit.sag,
                (-20.0, 100.0),
                &budget,
            )
            .unwrap()
            .expect("within the range");
            assert!(
                (t - 35.0).abs() < 2.0,
                "{} span {}: {t} °C",
                w.section,
                w.span
            );
            checked += 1;
        }
        assert_eq!(checked, model.len());
        // Beyond the range: none.
        let w = &model[0];
        let section = line.sections.iter().find(|s| s.id == w.section).unwrap();
        let cable = library.get(&section.cable).unwrap();
        let far = estimate_temperature(&line, section, cable, 0, 100.0, (-20.0, 100.0), &budget);
        assert_eq!(far, Ok(None));
    }
}
