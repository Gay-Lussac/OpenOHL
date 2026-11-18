// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A synthetic country (ADR-0013, phase 1): a deterministic network of
//! invented lines to measure the session, and later the server, on.
//!
//! Everything is invented — coordinates, names, terrain. By default it has
//! the owner's size (12 000 pylons carrying on average 2.5 electrical
//! systems): routes as straight legs between angle points over a smooth
//! synthetic heightfield, suspension structures between strain
//! structures, a share of routes branching off another and sharing its
//! towers for a few legs (towers of several lines), one section per phase
//! and earth wire between strain structures, jumpers at strain structures.
//! LiDAR is made only for a handful of sample tiles ([`lidar_tile`]): its
//! size for the whole country is extrapolated from them, and said so.
//!
//! **Deterministic**: the same settings give the same line, bit for bit
//! ([`Country::hash`], tested per seed). Only `+ − × ÷` and `sqrt` (exact
//! in IEEE arithmetic) shape the geometry; the one angle (a structure's
//! orientation, `atan2`) is rounded to 10⁻⁶ degree and every coordinate to
//! the millimetre, so a last-bit difference of a mathematical library
//! cannot change the output.
//!
//! Cost: about a second for the default size (12 000 pylons). Memory: the
//! line (≈ 0.3 GB at the default size while it is built as JSON).

use ohl_line::Line;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// The owner's size (2026-11-16): pylons.
pub const DEFAULT_PYLONS: usize = 12_000;
/// The owner's size: electrical systems per pylon, on average.
pub const DEFAULT_SYSTEMS: f64 = 2.5;
/// The plane's origin (an invented national grid), m.
pub const ORIGIN: [f64; 2] = [500_000.0, 100_000.0];
/// The country's extent from [`ORIGIN`], m (routes start inside it).
pub const EXTENT: [f64; 2] = [350_000.0, 220_000.0];
/// The coordinate system's name in the line file.
pub const CRS: &str = "synthetic national grid (invented)";
/// The conductor and earth wire (the shipped example cable).
pub const CABLE: &str = "acsr-drake-26-7";
/// A level-0 tile's width, m (the project grid's default, ADR-0008).
pub const TILE: f64 = 256.0;

/// What to generate.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// Pylons (structures) in the country.
    pub pylons: usize,
    /// Electrical systems per pylon, on average (a pylon shared by two
    /// routes carries both routes' systems).
    pub systems: f64,
    /// The seed: another seed, another country of the same size.
    pub seed: u64,
    /// Spans per tension section, on average (2.0 by default: the plan's
    /// estimate scaled from a real line — ≈ 60 000 sections, ≈ 55 000
    /// jumpers for 12 000 pylons; real long lines often have longer
    /// sections, which only makes fewer, longer sections).
    pub section_spans: f64,
    /// Share of the routes that branch off another one and share its
    /// towers for 2 to 5 legs (multi-circuit towers of two lines).
    pub branching: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            pylons: DEFAULT_PYLONS,
            systems: DEFAULT_SYSTEMS,
            seed: 1,
            section_spans: 2.0,
            branching: 0.7,
        }
    }
}

impl Settings {
    /// Refuses settings that cannot make a country.
    pub fn check(&self) -> Result<(), String> {
        if !(2..=1_000_000).contains(&self.pylons) {
            return Err(format!("pylons {}: 2 to 1 000 000", self.pylons));
        }
        if !(self.systems.is_finite() && (1.0..=8.0).contains(&self.systems)) {
            return Err(format!("systems {}: 1 to 8 per pylon", self.systems));
        }
        if !(self.section_spans.is_finite() && (1.0..=20.0).contains(&self.section_spans)) {
            return Err(format!(
                "section spans {}: 1 to 20 on average",
                self.section_spans
            ));
        }
        if !(self.branching.is_finite() && (0.0..=0.9).contains(&self.branching)) {
            return Err(format!("branching {}: 0 to 0.9", self.branching));
        }
        Ok(())
    }
}

/// The counts of a generated country.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Summary {
    /// Seed.
    pub seed: u64,
    /// Pylons (structures).
    pub pylons: usize,
    /// Routes (each its own design line here).
    pub routes: usize,
    /// Route length, km (towers shared by two routes counted once).
    pub km: f64,
    /// Electrical systems (of all routes).
    pub systems: usize,
    /// Systems per pylon, on average.
    pub systems_per_pylon: f64,
    /// Pylons carrying wires of two or more routes.
    pub shared_pylons: usize,
    /// Their share of all pylons.
    pub shared_share: f64,
    /// Strain pylons (angle, in-line strain and dead-end).
    pub strain_pylons: usize,
    /// Alignment points (angle points).
    pub alignment_points: usize,
    /// Tension sections (phases and earth wires).
    pub sections: usize,
    /// Wire spans (a section's spans, summed).
    pub wire_spans: usize,
    /// Jumpers.
    pub jumpers: usize,
    /// The line file's JSON, bytes.
    pub json_bytes: usize,
    /// SHA-256 of the line file's JSON ([`Country::hash`]).
    pub hash: String,
}

/// A generated country: one line holding every route (as one project
/// would hold it today), and what LiDAR sample tiles to make.
#[derive(Debug, Clone)]
pub struct Country {
    /// The line (validated).
    pub line: Line,
    /// Its counts.
    pub summary: Summary,
    /// Lower-left corners of LiDAR sample tiles (level-0 tiles of the
    /// grid holding sample pylons, spread over the country), m.
    pub sample_tiles: Vec<[f64; 2]>,
}

impl Country {
    /// SHA-256 (hex) of the line file's JSON as `serde_json` writes it:
    /// the same for the same settings on every computer.
    pub fn hash(&self) -> String {
        let text = serde_json::to_string(&self.line).unwrap_or_default();
        hex(&Sha256::digest(text.as_bytes()))
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A small deterministic random generator (SplitMix64): the same numbers
/// on every computer, whatever library versions.
#[derive(Debug, Clone)]
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    fn f64(&mut self) -> f64 {
        (self.next() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Uniform in [a, b).
    fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.f64()
    }

    /// Uniform integer in a..=b.
    fn int(&mut self, a: usize, b: usize) -> usize {
        a + (self.next() % (b - a + 1) as u64) as usize
    }
}

/// A hash of lattice point (i, j) and the seed, uniform in [0, 1).
fn lattice(seed: u64, i: i64, j: i64) -> f64 {
    let mut r = Rng(seed
        ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (j as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F));
    r.next();
    r.f64()
}

/// Value noise in [0, 1] with lattice spacing `cell`, m (smoothstep
/// between lattice values: arithmetic only).
fn noise(seed: u64, x: f64, y: f64, cell: f64) -> f64 {
    let (u, v) = (x / cell, y / cell);
    let (i, j) = (u.floor(), v.floor());
    let (fu, fv) = (u - i, v - j);
    let s = |t: f64| t * t * (3.0 - 2.0 * t);
    let (su, sv) = (s(fu), s(fv));
    let (i, j) = (i as i64, j as i64);
    let a = lattice(seed, i, j);
    let b = lattice(seed, i + 1, j);
    let c = lattice(seed, i, j + 1);
    let d = lattice(seed, i + 1, j + 1);
    let ab = a + (b - a) * su;
    let cd = c + (d - c) * su;
    ab + (cd - ab) * sv
}

/// The synthetic ground height at (x, y), m: hills of a few km and
/// smaller relief, 300 to ≈ 1 400 m.
pub fn ground(seed: u64, x: f64, y: f64) -> f64 {
    300.0
        + 900.0 * noise(seed ^ 0x51, x, y, 9000.0)
        + 150.0 * noise(seed ^ 0x52, x, y, 1500.0)
        + 15.0 * noise(seed ^ 0x53, x, y, 160.0)
}

/// Rounded to the millimetre.
fn mm(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

type V2 = [f64; 2];

fn unit(v: V2) -> V2 {
    let n = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if n > 0.0 {
        [v[0] / n, v[1] / n]
    } else {
        [1.0, 0.0]
    }
}

/// `v` turned by the angle whose half-tangent is `t` (exact rational
/// rotation: cos = (1 − t²)/(1 + t²), sin = 2t/(1 + t²)).
fn turn(v: V2, t: f64) -> V2 {
    let c = (1.0 - t * t) / (1.0 + t * t);
    let s = 2.0 * t / (1.0 + t * t);
    unit([v[0] * c - v[1] * s, v[0] * s + v[1] * c])
}

/// A random direction (uniform enough: a point in the unit disc).
fn direction(rng: &mut Rng) -> V2 {
    loop {
        let v = [rng.range(-1.0, 1.0), rng.range(-1.0, 1.0)];
        let n = v[0] * v[0] + v[1] * v[1];
        if n > 0.01 && n <= 1.0 {
            return unit(v);
        }
    }
}

struct Tower {
    id: String,
    at: V2,
    strain: bool,
    /// The routes on it, in the order they came, with their systems.
    routes: Vec<(usize, usize)>,
}

struct RouteGen {
    code: String,
    voltage: f64,
    systems: usize,
    towers: Vec<usize>,
    points: Vec<usize>,
}

struct PointGen {
    id: String,
    at: V2,
    tower: usize,
}

/// Height of the lowest cross-arm and the suspension string's weight, by
/// voltage (kV).
fn arms(voltage: f64) -> (f64, f64) {
    if voltage >= 300.0 {
        (30.0, 1500.0)
    } else if voltage >= 200.0 {
        (25.0, 900.0)
    } else if voltage >= 100.0 {
        (20.0, 500.0)
    } else {
        (16.0, 300.0)
    }
}

/// Generates the country of `settings`.
pub fn country(settings: &Settings) -> Result<Country, String> {
    settings.check()?;
    let seed = settings.seed;
    let mut rng = Rng(seed.wrapping_mul(0x2545_F491_4F6C_DD1D) ^ 0x0f0f_1234);
    let mut towers: Vec<Tower> = Vec::new();
    let mut points: Vec<PointGen> = Vec::new();
    let mut routes: Vec<RouteGen> = Vec::new();
    // A route's systems: 1 + three trials, so the mean is `per_route`;
    // branching routes add their systems to shared towers, so the routes'
    // own mean is a little lower than the pylons'.
    let per_route = (settings.systems / (1.0 + 0.32 * settings.branching)).clamp(1.0, 4.0);
    let p_system = (per_route - 1.0) / 3.0;
    let voltages = [(50.0, 0.15), (110.0, 0.30), (220.0, 0.30), (380.0, 0.25)];
    while towers.len() < settings.pylons {
        let r = routes.len();
        let code = format!("L{:03}", r + 1);
        let mut u = rng.f64();
        let mut voltage = 380.0;
        for (v, p) in voltages {
            if u < p {
                voltage = v;
                break;
            }
            u -= p;
        }
        let systems = 1 + (0..3).filter(|_| rng.f64() < p_system).count();
        let mut route = RouteGen {
            code: code.clone(),
            voltage,
            systems,
            towers: Vec::new(),
            points: Vec::new(),
        };
        let target = rng.int(40, 160);
        // Branch off an earlier route (one that does not branch itself),
        // sharing its towers from one angle point to a later one.
        let parents: Vec<usize> = (0..r)
            .filter(|&k| {
                routes[k].points.len() >= 3 && towers[routes[k].towers[0]].routes[0].0 == k
            })
            .collect();
        let mut heading;
        let mut at;
        let mut own = 0usize;
        if !parents.is_empty() && rng.f64() < settings.branching {
            let a = parents[rng.int(0, parents.len() - 1)];
            let n = routes[a].points.len();
            let p0 = rng.int(0, n - 2);
            let p1 = (p0 + rng.int(3, 6)).min(n - 1);
            let t0 = points[routes[a].points[p0]].tower;
            let t1 = points[routes[a].points[p1]].tower;
            let k0 = routes[a].towers.iter().position(|&t| t == t0).unwrap_or(0);
            let k1 = routes[a].towers.iter().position(|&t| t == t1).unwrap_or(0);
            route.towers = routes[a].towers[k0..=k1].to_vec();
            route.points = routes[a].points[p0..=p1].to_vec();
            for &t in &route.towers {
                towers[t].routes.push((r, systems));
            }
            let before = towers[routes[a].towers[k1 - 1]].at;
            at = towers[t1].at;
            let along = unit([at[0] - before[0], at[1] - before[1]]);
            let side = if rng.f64() < 0.5 { -1.0 } else { 1.0 };
            heading = turn(along, side * rng.range(0.3, 0.55));
        } else {
            at = [
                ORIGIN[0] + rng.range(0.0, EXTENT[0]),
                ORIGIN[1] + rng.range(0.0, EXTENT[1]),
            ];
            heading = direction(&mut rng);
            own += 1;
            let t = towers.len();
            towers.push(Tower {
                id: format!("{code}-T{own:03}"),
                at: [mm(at[0]), mm(at[1])],
                strain: true,
                routes: vec![(r, systems)],
            });
            route.towers.push(t);
            points.push(PointGen {
                id: format!("{code}-P001"),
                at: towers[t].at,
                tower: t,
            });
            route.points.push(points.len() - 1);
        }
        // Own legs: straight between angle points, 2 to 8 spans each.
        while route.towers.len() < target && towers.len() < settings.pylons {
            let spans = rng
                .int(2, 8)
                .min(target - route.towers.len())
                .min(settings.pylons - towers.len())
                .max(1);
            let inline = if spans > 1 {
                ((spans as f64 / settings.section_spans - 1.0) / (spans as f64 - 1.0))
                    .clamp(0.0, 1.0)
            } else {
                0.0
            };
            for k in 0..spans {
                let length = rng.range(250.0, 450.0);
                at = [at[0] + heading[0] * length, at[1] + heading[1] * length];
                let last = k + 1 == spans;
                let strain = last || rng.f64() < inline;
                own += 1;
                towers.push(Tower {
                    id: format!("{code}-T{own:03}"),
                    at: [mm(at[0]), mm(at[1])],
                    strain,
                    routes: vec![(r, systems)],
                });
                route.towers.push(towers.len() - 1);
            }
            let t = towers.len() - 1;
            points.push(PointGen {
                id: format!("{code}-P{:03}", route.points.len() + 1),
                at: towers[t].at,
                tower: t,
            });
            route.points.push(points.len() - 1);
            heading = turn(heading, rng.range(-0.35, 0.35));
        }
        if route.towers.len() < 2 {
            // The last route got no tower of its own: drop it.
            for &t in &route.towers {
                towers[t].routes.retain(|&(k, _)| k != r);
            }
            if towers.len() >= settings.pylons {
                break;
            }
            continue;
        }
        routes.push(route);
    }
    build(settings, &towers, &points, &routes)
}

/// The tower's direction along the line (the bisector of the first
/// route's spans at it).
fn bisector(towers: &[Tower], routes: &[RouteGen], t: usize) -> V2 {
    let r = &routes[towers[t].routes[0].0];
    let k = r.towers.iter().position(|&x| x == t).unwrap_or(0);
    let at = towers[t].at;
    let back = (k > 0).then(|| {
        let p = towers[r.towers[k - 1]].at;
        unit([at[0] - p[0], at[1] - p[1]])
    });
    let ahead = (k + 1 < r.towers.len()).then(|| {
        let n = towers[r.towers[k + 1]].at;
        unit([n[0] - at[0], n[1] - at[1]])
    });
    match (back, ahead) {
        (Some(b), Some(a)) => unit([a[0] + b[0], a[1] + b[1]]),
        (Some(b), None) => b,
        (None, Some(a)) => a,
        (None, None) => [1.0, 0.0],
    }
}

/// Whether route `r` holds its wires in strain at tower `t`: the tower is
/// a strain tower, or the route starts or ends there.
fn strained(towers: &[Tower], route: &RouteGen, t: usize) -> bool {
    towers[t].strain || route.towers.first() == Some(&t) || route.towers.last() == Some(&t)
}

/// The first system slot of route `r` on tower `t` (the systems of the
/// routes before it on the tower come first) and its place among the
/// tower's routes.
fn slot(tower: &Tower, r: usize) -> (usize, usize) {
    let mut first = 0;
    for (k, &(q, n)) in tower.routes.iter().enumerate() {
        if q == r {
            return (first, k);
        }
        first += n;
    }
    (first, tower.routes.len())
}

/// The wire point set of `phase` (0..3) of system `s` of route `r` on
/// tower `t` (base: suspension, or the strain point back), and the earth
/// wire's (phase `None`). The strain point ahead is `500 +` it.
fn set_of(tower: &Tower, r: usize, s: usize, phase: Option<usize>) -> u32 {
    let (first, place) = slot(tower, r);
    match phase {
        Some(p) => (10 * (first + s) + p + 1) as u32,
        None => (900 + place) as u32,
    }
}

const AHEAD: u32 = 500;

fn r3(p: V2, z: f64) -> Value {
    json!([mm(p[0]), mm(p[1]), mm(z)])
}

fn structure(seed: u64, towers: &[Tower], routes: &[RouteGen], t: usize) -> Value {
    let tower = &towers[t];
    let d = bisector(towers, routes, t);
    let n = [-d[1], d[0]];
    let z0 = (ground(seed, tower.at[0], tower.at[1]) * 100.0).round() / 100.0;
    let orientation = (d[1].atan2(d[0]).to_degrees() - 90.0) * 1e6;
    let orientation = orientation.round() / 1e6;
    let (base_h, _) = arms(routes[tower.routes[0].0].voltage);
    let slots: usize = tower.routes.iter().map(|&(_, n)| n).sum();
    let top = base_h + 7.0 * slots.div_ceil(2) as f64 + 4.0;
    let mut pts = Vec::new();
    let mut strings = Vec::new();
    let mut wires = Vec::new();
    let off = |p: V2, lateral: f64, along: f64| -> V2 {
        [
            p[0] + n[0] * lateral + d[0] * along,
            p[1] + n[1] * lateral + d[1] * along,
        ]
    };
    for (place, &(r, k)) in tower.routes.iter().enumerate() {
        let route = &routes[r];
        let (_, weight) = arms(route.voltage);
        let strain = strained(towers, route, t);
        let first = route.towers.first() == Some(&t);
        let last = route.towers.last() == Some(&t);
        // Conductors (phase Some) and the route's earth wire (None).
        let mut holds: Vec<(u32, f64, f64, f64)> = Vec::new();
        for s in 0..k {
            for p in 0..3 {
                let base = set_of(tower, r, s, Some(p));
                let g = (base as usize - 1) / 10;
                let side = if g.is_multiple_of(2) { -1.0 } else { 1.0 };
                let lateral = side * (4.0 + 3.5 * p as f64);
                let h = base_h + 7.0 * (g / 2) as f64;
                holds.push((base, lateral, h, weight));
            }
        }
        holds.push((set_of(tower, r, 0, None), 2.0 * place as f64, top, 100.0));
        for (base, lateral, h, weight) in holds {
            let arm = off(tower.at, lateral, 0.0);
            pts.push(json!({"label": format!("A{base}"), "position": r3(arm, z0 + h)}));
            if strain {
                let length = mm((2.5f64 * 2.5 + 0.5 * 0.5).sqrt());
                for (set, along, here) in [(base, -2.5, !first), (AHEAD + base, 2.5, !last)] {
                    if !here {
                        continue;
                    }
                    let w = off(tower.at, lateral, along);
                    wires.push(json!({"label": format!("W{set}"), "position": r3(w, z0 + h - 0.5), "set": set}));
                    strings.push(json!({"label": format!("S{set}"), "top": format!("A{base}"),
                        "wire": format!("W{set}"), "length": length, "weight": 1.5 * weight, "strain": true}));
                }
            } else {
                let length = if weight > 100.0 { 3.5 } else { 0.5 };
                wires.push(json!({"label": format!("W{base}"), "position": r3(arm, z0 + h - length), "set": base}));
                strings.push(
                    json!({"label": format!("S{base}"), "top": format!("A{base}"),
                    "wire": format!("W{base}"), "length": length, "weight": weight}),
                );
            }
        }
    }
    json!({
        "id": tower.id,
        "position": r3(tower.at, z0),
        "orientation_deg": orientation,
        "points": pts,
        "strings": strings,
        "wires": wires,
    })
}

fn build(
    settings: &Settings,
    towers: &[Tower],
    points: &[PointGen],
    routes: &[RouteGen],
) -> Result<Country, String> {
    let seed = settings.seed;
    let mut summary = Summary {
        seed,
        pylons: towers.len(),
        routes: routes.len(),
        ..Summary::default()
    };
    let structures: Vec<Value> = (0..towers.len())
        .map(|t| structure(seed, towers, routes, t))
        .collect();
    let mut sections = Vec::new();
    let mut jumpers = Vec::new();
    let mut systems = Vec::new();
    let mut spans_m = 0.0;
    for (r, route) in routes.iter().enumerate() {
        for s in 0..route.systems {
            systems.push(json!({
                "id": format!("{}-{}", route.code, s + 1),
                "name": format!("{} system {} ({} kV)", route.code, s + 1, route.voltage),
                "kind": "ac", "voltage_kv": route.voltage, "frequency_hz": 50.0,
                "phases": [{"label": "L1", "angle_deg": 0.0}, {"label": "L2", "angle_deg": -120.0},
                           {"label": "L3", "angle_deg": 120.0}],
            }));
        }
        for w in route.towers.windows(2) {
            // Spans of a shared stretch belong to the first route there.
            let (a, b) = (&towers[w[0]], &towers[w[1]]);
            if a.routes[0].0 == r || b.routes[0].0 == r {
                spans_m += (b.at[0] - a.at[0]).hypot(b.at[1] - a.at[1]);
            }
        }
        let ends: Vec<usize> = (0..route.towers.len())
            .filter(|&k| strained(towers, route, route.towers[k]))
            .collect();
        for (iv, pair) in ends.windows(2).enumerate() {
            let (i, j) = (pair[0], pair[1]);
            let tension = match route.voltage as u32 {
                380 => 30_000.0,
                220 => 25_000.0,
                110 => 20_000.0,
                _ => 15_000.0,
            };
            let mut phases: Vec<(Option<usize>, usize, String)> = Vec::new();
            for s in 0..route.systems {
                for (p, label) in ["L1", "L2", "L3"].iter().enumerate() {
                    phases.push((Some(p), s, (*label).to_owned()));
                }
            }
            phases.push((None, 0, String::new()));
            for (phase, s, label) in phases {
                let pts: Vec<Value> = (i..=j)
                    .map(|k| {
                        let t = route.towers[k];
                        let base = set_of(&towers[t], r, s, phase);
                        let set = if k == i { AHEAD + base } else { base };
                        json!({"structure": towers[t].id, "set": set})
                    })
                    .collect();
                summary.wire_spans += j - i;
                let mut sec = json!({
                    "id": match phase {
                        Some(_) => format!("{}-{}-{}-{:03}", route.code, s + 1, label, iv + 1),
                        None => format!("{}-E-{:03}", route.code, iv + 1),
                    },
                    "cable": CABLE,
                    "points": pts,
                    "stringing": {"kind": "tension",
                        "horizontal_tension": if phase.is_some() { tension } else { 0.6 * tension },
                        "temperature": 15.0},
                });
                if phase.is_some() {
                    sec["circuit_name"] = json!(format!("{} system {}", route.code, s + 1));
                    sec["phase_label"] = json!(label);
                    sec["system"] = json!(format!("{}-{}", route.code, s + 1));
                }
                sections.push(sec);
                // A jumper at the section's end on a strain tower the
                // route passes (not its last).
                if let Some(p) = phase
                    && j + 1 < route.towers.len()
                {
                    let t = route.towers[j];
                    let base = set_of(&towers[t], r, s, Some(p));
                    jumpers.push(json!({
                        "id": format!("{}-J{}", towers[t].id, base),
                        "cable": CABLE,
                        "from": {"structure": towers[t].id, "set": base},
                        "to": {"structure": towers[t].id, "set": AHEAD + base},
                        "sag": 1.5, "temperature": 15.0,
                    }));
                }
            }
        }
    }
    summary.sections = sections.len();
    summary.jumpers = jumpers.len();
    summary.systems = systems.len();
    summary.km = (spans_m / 100.0).round() / 10.0;
    let slots: usize = towers
        .iter()
        .map(|t| t.routes.iter().map(|&(_, n)| n).sum::<usize>())
        .sum();
    summary.systems_per_pylon = (slots as f64 / towers.len() as f64 * 1000.0).round() / 1000.0;
    summary.shared_pylons = towers.iter().filter(|t| t.routes.len() > 1).count();
    summary.shared_share =
        (summary.shared_pylons as f64 / towers.len() as f64 * 10_000.0).round() / 10_000.0;
    summary.strain_pylons = (0..towers.len())
        .filter(|&t| {
            towers[t]
                .routes
                .iter()
                .any(|&(r, _)| strained(towers, &routes[r], t))
        })
        .count();
    summary.alignment_points = points.len();
    let alignment_points: Vec<Value> = points
        .iter()
        .map(|p| {
            let z = (ground(seed, p.at[0], p.at[1]) * 100.0).round() / 100.0;
            json!({"id": p.id, "position": r3(p.at, z), "structure": towers[p.tower].id})
        })
        .collect();
    let line = json!({
        "format": "ohl-line",
        "version": ohl_line::LINE_FILE_VERSION,
        "name": format!("Synthetic country (seed {seed}, invented)"),
        "crs": CRS,
        "routes": routes.iter().map(|r| json!({"id": r.code,
            "name": format!("Synthetic line {} ({} kV)", r.code, r.voltage)})).collect::<Vec<_>>(),
        "alignments": routes.iter().map(|r| json!({"id": r.code, "name": r.code, "route": r.code,
            "points": r.points.iter().map(|&p| points[p].id.clone()).collect::<Vec<_>>()}))
            .collect::<Vec<_>>(),
        "alignment_points": alignment_points,
        "structures": structures,
        "sections": sections,
        "jumpers": jumpers,
        "systems": systems,
    });
    let line: Line =
        serde_json::from_value(line).map_err(|e| format!("synthetic line not read: {e}"))?;
    line.validate()
        .map_err(|e| format!("synthetic line not valid: {e}"))?;
    // Sample tiles: the level-0 tiles of pylons spread over the country.
    let samples = 4.min(towers.len());
    let sample_tiles = (0..samples)
        .map(|k| {
            let at = towers[k * towers.len() / samples + towers.len() / (2 * samples)].at;
            [(at[0] / TILE).floor() * TILE, (at[1] / TILE).floor() * TILE]
        })
        .collect();
    let mut country = Country {
        line,
        summary,
        sample_tiles,
    };
    let text = serde_json::to_string(&country.line).map_err(|e| e.to_string())?;
    country.summary.json_bytes = text.len();
    country.summary.hash = hex(&Sha256::digest(text.as_bytes()));
    Ok(country)
}

/// LiDAR sample: synthetic points of the level-0 tile with lower-left
/// corner `corner`, `density` points per m² (ground, and vegetation where
/// the forest noise says so). Deterministic for the seed and the tile.
/// Far less structured than a real survey (no buildings, no wires): the
/// compression measured on it is a guide, not a promise.
pub fn lidar_tile(seed: u64, corner: [f64; 2], density: f64) -> Vec<ohl_lidar::LidarPoint> {
    let count = (density * TILE * TILE).round().max(0.0) as usize;
    let mut rng = Rng(seed
        ^ (corner[0] as u64).wrapping_mul(31)
        ^ (corner[1] as u64).wrapping_mul(1_000_003));
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let x = corner[0] + rng.range(0.0, TILE);
        let y = corner[1] + rng.range(0.0, TILE);
        let g = ground(seed, x, y) + rng.range(-0.05, 0.05);
        let forest = noise(seed ^ 0x77, x, y, 120.0) > 0.6;
        let (z, class, returns) = if forest && rng.f64() < 0.7 {
            (g + rng.range(2.0, 28.0), 5u8, 2u8)
        } else {
            (g, 2u8, 1u8)
        };
        out.push(ohl_lidar::LidarPoint {
            x,
            y,
            z,
            class,
            intensity: rng.int(20, 220) as u16,
            return_number: 1,
            number_of_returns: returns,
        });
    }
    out
}

pub mod project;

#[cfg(test)]
mod tests;
