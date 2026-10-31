// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The plugin's parameters (TOML), their defaults and their checks.

use serde::Deserialize;

/// How a building's footprint is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OutlineKind {
    /// The convex hull of its points (default).
    #[default]
    Hull,
    /// The boundary of its plan cells, traced and simplified.
    Cells,
}

/// The parameters; every key optional (the default when left out),
/// unknown keys refused.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Params {
    /// Survey codes of building points (`*`, `?`, any case).
    pub building_codes: Vec<String>,
    /// Survey codes of ground points (with the point cloud's ground).
    pub ground_codes: Vec<String>,
    /// The plan grid's cell, m.
    pub cell: f64,
    /// Cells closer than this belong to one building, m.
    pub gap: f64,
    /// Fewer points: no building.
    pub min_points: usize,
    /// A smaller footprint: no building, m².
    pub min_area: f64,
    /// The footprint: `"hull"` or `"cells"`.
    pub outline: OutlineKind,
    /// Corners nearer than this to the outline's simplified line are
    /// dropped (Douglas–Peucker), m.
    pub simplify: f64,
    /// The ground of a building: the ground points this far around its
    /// footprint, m (from, to).
    pub ground_ring: [f64; 2],
    /// The ground's percentile there (0–100).
    pub ground_percentile: f64,
    /// The eaves: this percentile of the building's points' elevations.
    pub eaves_percentile: f64,
    /// The roof: this percentile.
    pub roof_percentile: f64,
    /// One floor, m.
    pub floor_height: f64,
    /// The first floor's place above the ground, m.
    pub first_height: f64,
    /// A building's height when its eaves are less than 2 m above its
    /// ground (a surveyed footprint), m.
    pub default_height: f64,
    /// Buildings farther than this from the line's axis are left out, m.
    pub max_distance: f64,
    /// Cuts of one span closer than this are merged, m.
    pub merge: f64,
    /// A cut through each building's centroid as well.
    pub centroid_cut: bool,
    /// A cut reaches this far beyond the farthest building it crosses, m.
    pub cut_margin: f64,
    /// A cut's least half-width, m.
    pub min_width: f64,
    /// A cut's height above the highest wire, m.
    pub above: f64,
    /// Between a cut's points, m.
    pub step: f64,
    /// Isoline levels of the magnetic field, µT.
    pub levels_ut: Vec<f64>,
    /// Isoline levels of the electric field, kV/m.
    pub levels_kv_m: Vec<f64>,
    /// A limit of the magnetic field, µT (none: no margin; none built in).
    pub limit_ut: Option<f64>,
    /// A limit of the electric field, kV/m (none: no margin).
    pub limit_kv_m: Option<f64>,
    /// A plan DXF (R12) of the outlines and the cuts as well.
    pub dxf: bool,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            building_codes: vec!["BLD*".into()],
            ground_codes: vec!["GND*".into()],
            cell: 0.5,
            gap: 1.5,
            min_points: 20,
            min_area: 10.0,
            outline: OutlineKind::Hull,
            simplify: 0.25,
            ground_ring: [2.0, 5.0],
            ground_percentile: 10.0,
            eaves_percentile: 50.0,
            roof_percentile: 98.0,
            floor_height: 3.0,
            first_height: 1.5,
            default_height: 6.0,
            max_distance: 100.0,
            merge: 2.0,
            centroid_cut: false,
            cut_margin: 10.0,
            min_width: 20.0,
            above: 5.0,
            step: 1.0,
            levels_ut: vec![0.1, 1.0, 10.0, 100.0],
            levels_kv_m: vec![0.1, 1.0, 5.0],
            limit_ut: None,
            limit_kv_m: None,
            dxf: false,
        }
    }
}

/// The template: every parameter with its default.
pub const TEMPLATE: &str = r#"# building-cuts parameters (TOML). Every key is optional: left out, the
# default shown here is taken. Lengths in metres.

# Survey points: the codes of building points and of ground points
# (`*` any text, `?` one character, any case). Building points with a
# breakline id form one outline each, in the surveyed order; without one
# they are clustered like the point cloud's building points.
building_codes = ["BLD*"]
ground_codes = ["GND*"]

# Clustering: a plan grid of `cell`; cells closer than `gap` are one
# building; fewer than `min_points` points or a footprint under
# `min_area` m² is no building.
cell = 0.5
gap = 1.5
min_points = 20
min_area = 10.0

# The footprint: "hull" (convex hull) or "cells" (the cells' boundary,
# traced); corners within `simplify` of the simplified line dropped.
outline = "hull"
simplify = 0.25

# Heights: the ground is the `ground_percentile` of the ground points
# `ground_ring` around the footprint; the eaves and the roof are
# percentiles of the building's points; floors = ceil((eaves - ground) /
# floor_height), at least 1; `default_height` high when its eaves are
# less than 2 m above its ground (a surveyed footprint).
ground_ring = [2.0, 5.0]
ground_percentile = 10.0
eaves_percentile = 50.0
roof_percentile = 98.0
floor_height = 3.0
first_height = 1.5
default_height = 6.0

# Buildings farther than `max_distance` from the axis are left out.
max_distance = 100.0

# Cuts: per building the governing cut (closest to a phase conductor) and
# the cut at its point nearest the axis (and its centroid's with
# `centroid_cut`), cuts of one span within `merge` merged; each reaching
# `cut_margin` beyond the buildings it crosses, at least `min_width`
# either side, `above` over the highest wire, points `step` apart.
merge = 2.0
centroid_cut = false
cut_margin = 10.0
min_width = 20.0
above = 5.0
step = 1.0

# Isolines (no limit is built in).
levels_ut = [0.1, 1.0, 10.0, 100.0]
levels_kv_m = [0.1, 1.0, 5.0]

# A limit gives each building's margin (none built in):
# limit_ut = 1.0
# limit_kv_m = 5.0

# A plan DXF (R12) of the outlines and the cuts.
dxf = false
"#;

impl Params {
    /// The parameters of `text` (TOML; empty: the defaults), checked.
    pub fn parse(text: &str) -> Result<Self, String> {
        let p: Params = if text.trim().is_empty() {
            Params::default()
        } else {
            toml::from_str(text).map_err(|e| format!("parameters: {e}"))?
        };
        p.check()?;
        Ok(p)
    }

    /// Whether the values are usable.
    pub fn check(&self) -> Result<(), String> {
        let positive = [
            ("cell", self.cell),
            ("floor_height", self.floor_height),
            ("default_height", self.default_height),
            ("max_distance", self.max_distance),
            ("min_width", self.min_width),
            ("step", self.step),
        ];
        for (k, v) in positive {
            if !(v.is_finite() && v > 0.0) {
                return Err(format!("parameters: {k} = {v}: above 0"));
            }
        }
        let non_negative = [
            ("gap", self.gap),
            ("min_area", self.min_area),
            ("simplify", self.simplify),
            ("first_height", self.first_height),
            ("merge", self.merge),
            ("cut_margin", self.cut_margin),
            ("above", self.above),
        ];
        for (k, v) in non_negative {
            if !(v.is_finite() && v >= 0.0) {
                return Err(format!("parameters: {k} = {v}: 0 or more"));
            }
        }
        for (k, v) in [
            ("ground_percentile", self.ground_percentile),
            ("eaves_percentile", self.eaves_percentile),
            ("roof_percentile", self.roof_percentile),
        ] {
            if !(0.0..=100.0).contains(&v) {
                return Err(format!("parameters: {k} = {v}: from 0 to 100"));
            }
        }
        let [a, b] = self.ground_ring;
        if !(a.is_finite() && b.is_finite() && a >= 0.0 && b > a) {
            return Err(format!(
                "parameters: ground_ring = [{a}, {b}]: from 0 m or more to farther"
            ));
        }
        if self.cell > 10.0 {
            return Err(format!("parameters: cell = {}: at most 10 m", self.cell));
        }
        for l in self.levels_ut.iter().chain(&self.levels_kv_m) {
            if !(l.is_finite() && *l > 0.0) {
                return Err(format!("parameters: a level of {l}: above 0"));
            }
        }
        for l in [self.limit_ut, self.limit_kv_m].into_iter().flatten() {
            if !(l.is_finite() && l > 0.0) {
                return Err(format!("parameters: a limit of {l}: above 0"));
            }
        }
        Ok(())
    }
}

/// Whether `text` matches `pattern` (`*` any text, `?` one character,
/// any case).
pub fn matches(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut i, mut j) = (0usize, 0usize);
    let (mut star, mut mark) = (None::<usize>, 0usize);
    while j < t.len() {
        if i < p.len() && (p[i] == '?' || p[i] == t[j]) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == '*' {
            star = Some(i);
            mark = j;
            i += 1;
        } else if let Some(s) = star {
            i = s + 1;
            mark += 1;
            j = mark;
        } else {
            return false;
        }
    }
    while i < p.len() && p[i] == '*' {
        i += 1;
    }
    i == p.len()
}

/// Whether `code` matches one of `patterns`.
pub fn matches_any(patterns: &[String], code: &str) -> bool {
    patterns.iter().any(|p| matches(p, code))
}
