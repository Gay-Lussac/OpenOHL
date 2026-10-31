// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The report of a run's results (`ohl-field-results-set`, 2026-10-31:
//! one run of both fields, one report — the owner: "Ok for 1 but keep it
//! clean"): `buildings.csv` with the magnetic and the electric field's
//! columns side by side, one `points.csv`, per cut one CSV grid and one
//! SVG drawing per field (`cut-LABEL-B.csv`, `cut-LABEL-B.svg`,
//! `cut-LABEL-E.…`), the plan files (`buildings.geojson`, optionally
//! `buildings.dxf`) and `notes.txt` once. A run of one field (`ohl
//! electrical magnetic` or `electric`) gives that field's columns and
//! files alone.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde_json::Value;

use crate::analysis::PresetOut;
use crate::geom::{P2, line_interval};
use crate::params::Params;

/// A report file.
#[derive(Debug, Clone, PartialEq)]
pub struct File {
    /// Its name, without a folder.
    pub name: String,
    /// Its media type.
    pub media_type: String,
    /// Its bytes.
    pub bytes: Vec<u8>,
}

fn file(name: String, media_type: &str, text: String) -> File {
    File {
        name,
        media_type: media_type.into(),
        bytes: text.into_bytes(),
    }
}

/// A cut of the results.
struct ResultCut {
    label: String,
    alignment: String,
    station: f64,
    angle_point: bool,
    from: String,
    to: String,
    at: f64,
    origin: P2,
    right: P2,
    offsets: Vec<f64>,
    elevations: Vec<f64>,
    ground: Vec<Option<f64>>,
    values: Vec<f64>,
    isolines: Vec<(f64, Vec<Vec<P2>>)>,
}

fn floats(v: &Value) -> Vec<f64> {
    v.as_array()
        .into_iter()
        .flatten()
        .map(|x| x.as_f64().unwrap_or(f64::NAN))
        .collect()
}

fn pair(v: &Value) -> P2 {
    let f = floats(v);
    [
        f.first().copied().unwrap_or(0.0),
        f.get(1).copied().unwrap_or(0.0),
    ]
}

impl ResultCut {
    fn parse(c: &Value) -> Self {
        Self {
            label: c["label"].as_str().unwrap_or("").to_owned(),
            alignment: c["alignment"].as_str().unwrap_or("").to_owned(),
            station: c["station"].as_f64().unwrap_or(f64::NAN),
            angle_point: c["angle_point"].as_bool().unwrap_or(false),
            from: c["from"].as_str().unwrap_or("").to_owned(),
            to: c["to"].as_str().unwrap_or("").to_owned(),
            at: c["at"].as_f64().unwrap_or(f64::NAN),
            origin: pair(&c["plane"]["origin"]),
            right: pair(&c["plane"]["right"]),
            offsets: floats(&c["offsets"]),
            elevations: floats(&c["elevations"]),
            ground: c["ground"]
                .as_array()
                .into_iter()
                .flatten()
                .map(Value::as_f64)
                .collect(),
            values: floats(&c["values"]),
            isolines: c["isolines"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|l| {
                    let lines = l["lines"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|line| line.as_array().into_iter().flatten().map(pair).collect())
                        .collect();
                    (l["level"].as_f64().unwrap_or(0.0), lines)
                })
                .collect(),
        }
    }

    fn value(&self, row: usize, col: usize) -> f64 {
        self.values
            .get(row * self.offsets.len() + col)
            .copied()
            .unwrap_or(f64::NAN)
    }

    fn step(&self) -> f64 {
        match (self.offsets.first(), self.offsets.get(1)) {
            (Some(a), Some(b)) if b > a => b - a,
            _ => 1.0,
        }
    }

    fn span(&self) -> (f64, f64) {
        (
            self.offsets.first().copied().unwrap_or(0.0),
            self.offsets.last().copied().unwrap_or(0.0),
        )
    }

    /// The ground at offset `s` (straight between the columns), if known.
    fn ground_at(&self, s: f64) -> Option<f64> {
        let k = self.offsets.iter().position(|o| *o >= s)?;
        let g = |i: usize| self.ground.get(i).copied().flatten();
        if k == 0 {
            return g(0);
        }
        let (a, b) = (self.offsets[k - 1], self.offsets[k]);
        let (ga, gb) = (g(k - 1)?, g(k)?);
        let t = if b > a { (s - a) / (b - a) } else { 0.0 };
        Some(ga + t * (gb - ga))
    }

    /// The largest value with offset in `s0`–`s1` and elevation in
    /// `z0`–`z1`, each widened by half a step (the grid's nearest points
    /// outside count: conservative).
    fn largest_in(&self, s0: f64, s1: f64, z0: f64, z1: f64) -> Option<f64> {
        let h = self.step() / 2.0 + 1e-9;
        let mut best: Option<f64> = None;
        for (r, z) in self.elevations.iter().enumerate() {
            if *z < z0 - h || *z > z1 + h {
                continue;
            }
            for (c, s) in self.offsets.iter().enumerate() {
                if *s < s0 - h || *s > s1 + h {
                    continue;
                }
                let v = self.value(r, c);
                if v.is_finite() {
                    best = Some(best.map_or(v, |b: f64| b.max(v)));
                }
            }
        }
        best
    }
}

/// One field's results.
struct FieldIn {
    /// `B` or `E`.
    q: String,
    /// Its unit as the results give it (`µT`, `kV/m`).
    unit: String,
    /// Its unit in a column name (`uT`, `kV_m`).
    unit_col: &'static str,
    /// The magnetic field (a point's cycle's largest too).
    magnetic: bool,
    /// The limit of the parameters, if any.
    limit: Option<f64>,
    /// Digits of its values.
    digits: usize,
    cuts: Vec<ResultCut>,
    points: Vec<Value>,
    notes: Vec<String>,
}

impl FieldIn {
    fn parse(v: &Value, params: &Params) -> Result<Self, String> {
        if v["format"] != "ohl-field-results" {
            return Err("results: a field that is not ohl-field-results".into());
        }
        let q = v["quantity"].as_str().unwrap_or("B").to_owned();
        let magnetic = q == "B";
        Ok(Self {
            unit: v["unit"]
                .as_str()
                .unwrap_or(if magnetic { "µT" } else { "kV/m" })
                .to_owned(),
            unit_col: if magnetic { "uT" } else { "kV_m" },
            limit: if magnetic {
                params.limit_ut
            } else {
                params.limit_kv_m
            },
            digits: if magnetic { 3 } else { 4 },
            cuts: v["cuts"]
                .as_array()
                .into_iter()
                .flatten()
                .map(ResultCut::parse)
                .collect(),
            points: v["points"].as_array().cloned().unwrap_or_default(),
            notes: v["notes"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|n| n.as_str().unwrap_or("").to_owned())
                .collect(),
            magnetic,
            q,
        })
    }
}

/// The fields of `results`: an `ohl-field-results-set` (one document per
/// field), or a single `ohl-field-results` document.
fn fields_of(results: &str, params: &Params) -> Result<Vec<FieldIn>, String> {
    let v: Value = serde_json::from_str(results).map_err(|e| format!("results: {e}"))?;
    let docs: Vec<&Value> = match v["format"].as_str() {
        Some("ohl-field-results-set") => v["fields"].as_array().into_iter().flatten().collect(),
        Some("ohl-field-results") => vec![&v],
        _ => return Err("results: not an ohl-field-results-set".into()),
    };
    if docs.is_empty() {
        return Err("results: no field".into());
    }
    docs.into_iter()
        .map(|d| FieldIn::parse(d, params))
        .collect()
}

/// A building of the preset: its outline and its summary note's fields.
struct BuildingIn {
    id: String,
    ring: Vec<P2>,
    ground: f64,
    eaves: f64,
    top: f64,
    fields: BTreeMap<String, String>,
}

/// A building's section in a cut: offsets and elevations.
#[derive(Clone, Copy)]
struct Section {
    s0: f64,
    s1: f64,
    z0: f64,
    eaves: f64,
    z1: f64,
}

/// The summary note's fields: `building B001: points 425; area 96.0 m²; …`.
fn summary_fields(notes: &[String]) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for n in notes {
        let Some(rest) = n.strip_prefix("building ") else {
            continue;
        };
        let Some((id, fields)) = rest.split_once(": ") else {
            continue;
        };
        let mut map = BTreeMap::new();
        for f in fields.split("; ") {
            if let Some((k, v)) = f.split_once(' ') {
                map.insert(k.to_owned(), v.to_owned());
            }
        }
        out.insert(id.to_owned(), map);
    }
    out
}

/// A wire crossing a cut: its label, offset, elevation, an earth wire.
type Crossing = (String, f64, f64, bool);

/// A cut's wires from its note: `cut LABEL: wire North L1 -5.00 415.20; wire EW 0.00 430.00 earth`.
fn crossings(notes: &[String]) -> BTreeMap<String, Vec<Crossing>> {
    let mut out = BTreeMap::new();
    for n in notes {
        let Some(rest) = n.strip_prefix("cut ") else {
            continue;
        };
        let Some((label, list)) = rest.split_once(": ") else {
            continue;
        };
        let mut wires = Vec::new();
        for w in list.split("; ") {
            let Some(w) = w.strip_prefix("wire ") else {
                continue;
            };
            let (w, earth) = match w.strip_suffix(" earth") {
                Some(x) => (x, true),
                None => (w, false),
            };
            let mut it = w.rsplitn(3, ' ');
            let (Some(z), Some(s), Some(name)) = (it.next(), it.next(), it.next()) else {
                continue;
            };
            if let (Ok(s), Ok(z)) = (s.parse::<f64>(), z.parse::<f64>()) {
                wires.push((name.to_owned(), s, z, earth));
            }
        }
        out.insert(label.to_owned(), wires);
    }
    out
}

/// A file name of `text`: letters, digits, `-`, `_`, `+` and `.` kept,
/// anything else `_`.
pub fn safe_name(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '+' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn csv_field(text: &str) -> String {
    if text.contains([',', '"', '\n']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_owned()
    }
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn opt(v: Option<f64>, digits: usize) -> String {
    v.filter(|x| x.is_finite())
        .map_or(String::new(), |x| format!("{x:.digits$}"))
}

fn max_of(values: impl IntoIterator<Item = Option<f64>>) -> Option<f64> {
    values
        .into_iter()
        .flatten()
        .fold(None, |a: Option<f64>, x| Some(a.map_or(x, |a| a.max(x))))
}

/// A building's section in cut `c`, if the cut crosses its footprint
/// within its width.
fn section_of(c: &ResultCut, b: &BuildingIn) -> Option<Section> {
    let (lo, hi) = c.span();
    let (s0, s1) = line_interval(c.origin, c.right, &b.ring)?;
    if s1 < lo || s0 > hi {
        return None;
    }
    Some(Section {
        s0: s0.max(lo),
        s1: s1.min(hi),
        z0: b.ground,
        eaves: b.eaves.clamp(b.ground, b.top),
        z1: b.top,
    })
}

/// The report files of the run's `results` for `preset`.
pub fn report(params: &Params, preset: &PresetOut, results: &str) -> Result<Vec<File>, String> {
    let fields = fields_of(results, params)?;
    let summaries = summary_fields(&preset.notes);
    let wires = crossings(&preset.notes);
    let buildings: Vec<BuildingIn> = preset
        .outlines
        .iter()
        .map(|o| {
            let fields = summaries.get(&o.id).cloned().unwrap_or_default();
            let eaves = fields
                .get("eaves")
                .and_then(|e| e.split(' ').next())
                .and_then(|e| e.parse().ok())
                .unwrap_or(o.top);
            BuildingIn {
                id: o.id.clone(),
                ring: o.ring.clone(),
                ground: o.ground,
                eaves,
                top: o.top,
                fields,
            }
        })
        .collect();
    let mut warnings = Vec::new();
    let mut files = Vec::new();
    // Per field: the largest value in each building's sections.
    let mut section_max: Vec<BTreeMap<String, f64>> = Vec::new();
    for f in &fields {
        let mut max: BTreeMap<String, f64> = BTreeMap::new();
        for c in &f.cuts {
            if let Some(p) = preset.cuts.iter().find(|p| p.label == c.label)
                && (p.alignment != c.alignment || (p.station - c.station).abs() > 1e-6)
            {
                warnings.push(format!(
                    "cut {}: computed at {} {:.2} m, asked at {} {:.2} m",
                    c.label, c.alignment, c.station, p.alignment, p.station
                ));
            }
            let mut sections: Vec<(&BuildingIn, Section, Option<f64>)> = Vec::new();
            for b in &buildings {
                let Some(sec) = section_of(c, b) else {
                    continue;
                };
                let m = c.largest_in(sec.s0, sec.s1, sec.z0, sec.z1);
                if let Some(m) = m {
                    let e = max.entry(b.id.clone()).or_insert(f64::NEG_INFINITY);
                    *e = e.max(m);
                }
                sections.push((b, sec, m));
            }
            let base = format!("cut-{}-{}", safe_name(&c.label), f.q);
            files.push(file(format!("{base}.csv"), "text/csv", cut_csv(c, f)));
            files.push(file(
                format!("{base}.svg"),
                "image/svg+xml",
                cut_svg(
                    c,
                    f,
                    &sections,
                    wires.get(&c.label).map_or(&[][..], |w| &w[..]),
                ),
            ));
        }
        section_max.push(max);
    }
    files.push(file("points.csv".into(), "text/csv", points_csv(&fields)));
    files.push(file(
        "buildings.csv".into(),
        "text/csv",
        buildings_csv(&fields, &buildings, &section_max, preset),
    ));
    // The plan files: the cuts' places are the same in every field.
    let plan_cuts = &fields[0].cuts;
    files.push(file(
        "buildings.geojson".into(),
        "application/geo+json",
        geojson(&buildings, plan_cuts, preset),
    ));
    if params.dxf {
        files.push(file(
            "buildings.dxf".into(),
            "image/vnd.dxf",
            dxf(&buildings, plan_cuts, preset),
        ));
    }
    let quantities: Vec<String> = fields
        .iter()
        .map(|f| format!("{} ({})", f.q, f.unit))
        .collect();
    let mut text = format!(
        "building-cuts report, {}, preset {}\n\n",
        quantities.join(" and "),
        preset.name
    );
    for w in &warnings {
        let _ = writeln!(text, "WARNING {w}");
    }
    for n in &preset.notes {
        let _ = writeln!(text, "plugin: {n}");
    }
    for f in &fields {
        for n in &f.notes {
            let _ = writeln!(text, "field {}: {n}", f.q);
        }
    }
    files.push(file("notes.txt".into(), "text/plain", text));
    Ok(files)
}

/// Every place, a row: its value in each field (B also the cycle's
/// largest) and, with a limit, the margin.
fn points_csv(fields: &[FieldIn]) -> String {
    let mut pts = String::from("name,building,floor,place,x,y,z,height_m");
    for f in fields {
        let (q, u) = (&f.q, f.unit_col);
        let _ = write!(pts, ",{q}_{u}");
        if f.magnetic {
            let _ = write!(pts, ",{q}_major_{u}");
        }
        if f.limit.is_some() {
            let _ = write!(pts, ",margin_{q}_{u}");
        }
    }
    pts.push('\n');
    // The places in the first field's order; the others by name.
    let by_name: Vec<BTreeMap<&str, &Value>> = fields
        .iter()
        .map(|f| {
            f.points
                .iter()
                .map(|p| (p["name"].as_str().unwrap_or(""), p))
                .collect()
        })
        .collect();
    for p in &fields[0].points {
        let name = p["name"].as_str().unwrap_or("");
        let parts: Vec<&str> = name.split('-').collect();
        let _ = write!(
            pts,
            "{},{},{},{},{},{},{},{}",
            csv_field(name),
            csv_field(parts.first().copied().unwrap_or("")),
            parts.get(1).copied().unwrap_or(""),
            parts.get(2).copied().unwrap_or(""),
            opt(p["x"].as_f64(), 2),
            opt(p["y"].as_f64(), 2),
            opt(p["z"].as_f64(), 2),
            opt(p["height"].as_f64(), 2),
        );
        for (f, map) in fields.iter().zip(&by_name) {
            let q = map.get(name);
            let value = q.and_then(|q| q["value"].as_f64());
            let major = q.and_then(|q| q["major"].as_f64());
            let _ = write!(pts, ",{}", opt(value, f.digits));
            if f.magnetic {
                let _ = write!(pts, ",{}", opt(major, f.digits));
            }
            if let Some(l) = f.limit {
                // The larger of the value and the cycle's largest.
                let worst = max_of([value, major]);
                let _ = write!(pts, ",{}", opt(worst.map(|x| l - x), f.digits));
            }
        }
        pts.push('\n');
    }
    pts
}

/// A row per building: what it is, where, and per field the largest value
/// in its sections and at its places, with a limit the margin.
fn buildings_csv(
    fields: &[FieldIn],
    buildings: &[BuildingIn],
    section_max: &[BTreeMap<String, f64>],
    preset: &PresetOut,
) -> String {
    let mut sum = String::from(
        "id,points,area_m2,centroid_x,centroid_y,ground_z,eaves_z,roof_z,floors,span,alignment,\
         station_from_m,station_to_m,axis_distance_m,conductor_distance_m,cuts",
    );
    for f in fields {
        let (q, u) = (&f.q, f.unit_col);
        let _ = write!(sum, ",max_section_{q}_{u},max_places_{q}_{u}");
        if f.magnetic {
            let _ = write!(sum, ",max_places_{q}_major_{u}");
        }
        if f.limit.is_some() {
            let _ = write!(sum, ",limit_{q}_{u},margin_{q}_{u}");
        }
    }
    sum.push('\n');
    // Per field, per building: the largest at its places (value, cycle).
    let place_max: Vec<(BTreeMap<String, f64>, BTreeMap<String, f64>)> = fields
        .iter()
        .map(|f| {
            let (mut v, mut m) = (BTreeMap::new(), BTreeMap::new());
            for p in &f.points {
                let id = p["name"]
                    .as_str()
                    .unwrap_or("")
                    .split('-')
                    .next()
                    .unwrap_or("")
                    .to_owned();
                for (map, key) in [(&mut v, "value"), (&mut m, "major")] {
                    if let Some(x) = p[key].as_f64() {
                        let e = map.entry(id.clone()).or_insert(f64::NEG_INFINITY);
                        *e = e.max(x);
                    }
                }
            }
            (v, m)
        })
        .collect();
    for b in buildings {
        let f = |k: &str| b.fields.get(k).cloned().unwrap_or_default();
        let first = |k: &str| f(k).split(' ').next().unwrap_or("").to_owned();
        let centroid: Vec<String> = f("centroid").split(' ').map(str::to_owned).collect();
        let stations: Vec<String> = f("stations").split(' ').map(str::to_owned).collect();
        let n_cuts = preset
            .cuts
            .iter()
            .filter(|c| {
                c.label
                    .split('+')
                    .any(|m| m.starts_with(&format!("{}-", b.id)))
            })
            .count();
        let _ = write!(
            sum,
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            csv_field(&b.id),
            first("points"),
            first("area"),
            centroid.first().cloned().unwrap_or_default(),
            centroid.get(1).cloned().unwrap_or_default(),
            first("ground"),
            first("eaves"),
            first("roof"),
            first("floors"),
            csv_field(&f("span")),
            csv_field(&f("alignment")),
            stations.first().cloned().unwrap_or_default(),
            stations.get(1).cloned().unwrap_or_default(),
            first("axis"),
            first("conductor"),
            n_cuts,
        );
        for (k, field) in fields.iter().enumerate() {
            let sec = section_max[k].get(&b.id).copied();
            let pm = place_max[k].0.get(&b.id).copied();
            let mm = place_max[k].1.get(&b.id).copied();
            let _ = write!(sum, ",{},{}", opt(sec, field.digits), opt(pm, field.digits));
            if field.magnetic {
                let _ = write!(sum, ",{}", opt(mm, field.digits));
            }
            if let Some(l) = field.limit {
                // Conservative: the largest of the section and the places
                // (their value and their cycle's largest).
                let worst = max_of([sec, pm, mm]);
                let _ = write!(sum, ",{l},{}", opt(worst.map(|w| l - w), field.digits));
            }
        }
        sum.push('\n');
    }
    sum
}

fn cut_csv(c: &ResultCut, f: &FieldIn) -> String {
    let (q, unit_col, digits) = (&f.q, f.unit_col, f.digits);
    let mut s = format!(
        "# cut {}, alignment {} at {:.2} m{} (span {}–{} at {:.2} m); {q} at every point of the \
         grid, below_ground 1 where under the ground\n\
         offset_m,elevation_m,ground_m,{q}_{unit_col},below_ground\n",
        c.label,
        c.alignment,
        c.station,
        if c.angle_point {
            ", an angle point: along the bisector"
        } else {
            ""
        },
        c.from,
        c.to,
        c.at
    );
    for (r, z) in c.elevations.iter().enumerate() {
        for (k, o) in c.offsets.iter().enumerate() {
            let g = c.ground.get(k).copied().flatten();
            let below = g.is_some_and(|g| *z < g);
            let _ = writeln!(
                s,
                "{o:.2},{z:.2},{},{},{}",
                opt(g, 2),
                opt(Some(c.value(r, k)), digits),
                u8::from(below)
            );
        }
    }
    s
}

/// Colours of the isoline levels, low to high.
const COLOURS: [&str; 8] = [
    "#2b6cb0", "#2f855a", "#b7791f", "#c05621", "#c53030", "#97266d", "#553c9a", "#1a202c",
];

/// `colour` (`#rrggbb`) as a light band: 16 % of it over white (an
/// opaque colour: adjacent cells do not darken where they meet).
fn tint(colour: &str) -> String {
    let c = |k: usize| u8::from_str_radix(colour.get(k..k + 2).unwrap_or("ff"), 16).unwrap_or(255);
    let t = |v: u8| (0.16 * f64::from(v) + 0.84 * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", t(c(1)), t(c(3)), t(c(5)))
}

/// About how wide a text of `chars` characters is at `size` px.
fn text_width(text: &str, size: f64) -> f64 {
    text.chars().count() as f64 * size * 0.6
}

/// A tick spacing (1, 2, 5 × 10ⁿ m) at least `min_px` apart at `scale`.
fn tick(scale: f64, min_px: f64) -> f64 {
    let mut t = 1.0;
    loop {
        for f in [1.0, 2.0, 5.0] {
            if f * t * scale >= min_px {
                return f * t;
            }
        }
        t *= 10.0;
    }
}

/// The 2D cut drawn 1:1 (owner, 2026-10-31: "draw the 2D cut at the right
/// place and show the ground, building, line and calculation results"):
/// the ground profile along the cut, each building's section where the
/// cut crosses its footprint (from its ground to its eaves, the roof
/// above dashed, labelled with its id and its largest value), every
/// conductor and earth wire crossing (labelled), the axis, the field's
/// bands between the levels (light) and its isolines with a legend, the
/// elevation and offset scales. Everything lies inside the viewBox.
fn cut_svg(
    c: &ResultCut,
    f: &FieldIn,
    sections: &[(&BuildingIn, Section, Option<f64>)],
    wires: &[Crossing],
) -> String {
    let (q, unit, digits) = (&f.q, &f.unit, f.digits);
    let (lo, hi) = c.span();
    let zgrid = c.elevations.first().copied().unwrap_or(0.0);
    let zhi = c.elevations.last().copied().unwrap_or(zgrid + 1.0);
    // Some ground shown under the grid's lowest row.
    let zlo = zgrid - (0.06 * (zhi - zgrid)).max(3.0);
    let width_m = (hi - lo).max(1.0);
    let height_m = (zhi - zlo).max(1.0);
    // One scale both ways (1:1), the drawing some 1 000 px wide.
    let scale = (1000.0 / width_m).clamp(3.0, 20.0);
    let largest = max_of(sections.iter().map(|s| s.2));
    let title = format!(
        "Cut {} — {q} in {unit}{}",
        c.label,
        largest.map_or(String::new(), |m| format!(
            "; largest in a building's section {m:.digits$} {unit}"
        ))
    );
    let subtitle = format!(
        "alignment {} at {:.1} m{} (span {}–{} at {:.1} m); looking ahead along the alignment, \
         offsets to the right positive; 1:1",
        c.alignment,
        c.station,
        if c.angle_point {
            ", an angle point: along the bisector"
        } else {
            ""
        },
        c.from,
        c.to,
        c.at
    );
    // The legend's entries (colour or kind, text).
    let mut legend: Vec<(String, String)> = c
        .isolines
        .iter()
        .enumerate()
        .map(|(k, (l, _))| (COLOURS[k % COLOURS.len()].to_owned(), format!("{l} {unit}")))
        .collect();
    for (kind, text) in [
        ("ground", "ground"),
        ("building", "building to its eaves"),
        ("roof", "roof (eaves to top)"),
        ("phase", "phase conductor"),
        ("earth", "earth wire"),
        ("axis", "axis of the alignment"),
    ] {
        legend.push((kind.to_owned(), text.to_owned()));
    }
    let legend_w = legend
        .iter()
        .map(|(_, t)| text_width(t, 11.0))
        .fold(text_width(&format!("{q} levels"), 12.0), f64::max)
        + 50.0;
    let (left, top, bottom) = (64.0, 70.0, 74.0);
    let right = legend_w + 24.0;
    let plot_w = width_m * scale;
    let plot_h = height_m * scale;
    let legend_h = 24.0 + 18.0 * legend.len() as f64;
    let w = (left + plot_w + right)
        .max(left + text_width(&title, 15.0) + 16.0)
        .max(left + text_width(&subtitle, 11.0) + 16.0);
    let h = (top + plot_h + bottom).max(top + legend_h + 16.0);
    let x = |s: f64| left + (s - lo) * scale;
    let y = |z: f64| top + (zhi - z) * scale;
    let mut o = String::new();
    let _ = writeln!(
        o,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.0}\" height=\"{h:.0}\" \
         viewBox=\"0 0 {w:.1} {h:.1}\" font-family=\"sans-serif\" font-size=\"11\">"
    );
    let _ = writeln!(o, "<rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>");
    let _ = writeln!(
        o,
        "<text x=\"{left}\" y=\"24\" font-size=\"15\" font-weight=\"bold\">{}</text>",
        xml(&title)
    );
    let _ = writeln!(
        o,
        "<text x=\"{left}\" y=\"42\" fill=\"#444\">{}</text>",
        xml(&subtitle)
    );
    let _ = writeln!(
        o,
        "<clipPath id=\"plot\"><rect x=\"{:.1}\" y=\"{:.1}\" width=\"{plot_w:.1}\" height=\"{plot_h:.1}\"/></clipPath>",
        x(lo),
        y(zhi)
    );
    // The bands between the levels, above the ground (light; a row's
    // cells of one band joined).
    let levels: Vec<f64> = c.isolines.iter().map(|l| l.0).collect();
    let band = |v: f64| levels.iter().filter(|l| v >= **l).count();
    let (nc, nr) = (c.offsets.len(), c.elevations.len());
    let _ = writeln!(
        o,
        "<g clip-path=\"url(#plot)\" shape-rendering=\"crispEdges\">"
    );
    if !levels.is_empty() && nc > 1 && nr > 1 {
        for r in 0..nr - 1 {
            let (z0, z1) = (c.elevations[r], c.elevations[r + 1]);
            let mut run: Option<(usize, usize)> = None; // (start column, band)
            let flush = |o: &mut String, run: Option<(usize, usize)>, end: usize| {
                if let Some((k0, b)) = run
                    && b > 0
                {
                    let _ = writeln!(
                        o,
                        "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{}\"/>",
                        x(c.offsets[k0]),
                        y(z1),
                        (c.offsets[end] - c.offsets[k0]) * scale,
                        (z1 - z0) * scale,
                        tint(COLOURS[(b - 1) % COLOURS.len()])
                    );
                }
            };
            for k in 0..nc - 1 {
                let v =
                    (c.value(r, k) + c.value(r, k + 1) + c.value(r + 1, k) + c.value(r + 1, k + 1))
                        / 4.0;
                let s = (c.offsets[k] + c.offsets[k + 1]) / 2.0;
                let under = c.ground_at(s).is_some_and(|g| (z0 + z1) / 2.0 < g);
                let b = if under || !v.is_finite() { 0 } else { band(v) };
                match run {
                    Some((_, rb)) if rb == b => {}
                    _ => {
                        flush(&mut o, run, k);
                        run = Some((k, b));
                    }
                }
            }
            flush(&mut o, run, nc - 1);
        }
    }
    let _ = writeln!(o, "</g>");
    // The ground: filled below its profile.
    let ground: Vec<(f64, f64)> = c
        .offsets
        .iter()
        .zip(&c.ground)
        .filter_map(|(s, g)| g.map(|g| (*s, g)))
        .collect();
    if ground.len() > 1 {
        let mut pts: Vec<String> = ground
            .iter()
            .map(|(s, g)| format!("{:.1},{:.1}", x(*s), y(g.max(zlo))))
            .collect();
        let line = pts.join(" ");
        pts.push(format!(
            "{:.1},{:.1}",
            x(ground[ground.len() - 1].0),
            y(zlo)
        ));
        pts.push(format!("{:.1},{:.1}", x(ground[0].0), y(zlo)));
        let _ = writeln!(
            o,
            "<polygon points=\"{}\" fill=\"#d9c7a7\" stroke=\"none\"/>\
             <polyline points=\"{line}\" fill=\"none\" stroke=\"#7b5e3b\" stroke-width=\"2\"/>",
            pts.join(" ")
        );
    }
    // The frame.
    let _ = writeln!(
        o,
        "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{plot_w:.1}\" height=\"{plot_h:.1}\" fill=\"none\" stroke=\"#999\"/>",
        x(lo),
        y(zhi)
    );
    // The scales: elevations on the left, offsets below.
    let tz = tick(scale, 36.0);
    let mut z = (zlo / tz).ceil() * tz;
    while z <= zhi + 1e-9 {
        let _ = writeln!(
            o,
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#999\"/>\
             <text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\" fill=\"#444\">{z:.0}</text>",
            x(lo) - 5.0,
            y(z),
            x(lo),
            y(z),
            x(lo) - 8.0,
            y(z) + 4.0
        );
        z += tz;
    }
    let _ = writeln!(
        o,
        "<text x=\"{:.1}\" y=\"{:.1}\" fill=\"#444\">elevation m</text>",
        8.0,
        y(zhi) - 8.0
    );
    let ts = tick(scale, 48.0);
    let mut s = (lo / ts).ceil() * ts;
    while s <= hi + 1e-9 {
        let _ = writeln!(
            o,
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#999\"/>\
             <text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" fill=\"#444\">{s:.0}</text>",
            x(s),
            y(zlo),
            x(s),
            y(zlo) + 5.0,
            x(s),
            y(zlo) + 18.0
        );
        s += ts;
    }
    let _ = writeln!(
        o,
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" fill=\"#444\">offset from the axis, m (right positive)</text>",
        x((lo + hi) / 2.0),
        y(zlo) + 34.0
    );
    // The axis.
    if lo <= 0.0 && hi >= 0.0 {
        let _ = writeln!(
            o,
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#666\" stroke-dasharray=\"8 5\"/>\
             <text x=\"{:.1}\" y=\"{:.1}\" fill=\"#666\">axis</text>",
            x(0.0),
            y(zhi),
            x(0.0),
            y(zlo),
            x(0.0) + 4.0,
            y(zhi) + 12.0
        );
    }
    // The isolines.
    let _ = writeln!(o, "<g clip-path=\"url(#plot)\">");
    for (k, (_, lines)) in c.isolines.iter().enumerate() {
        let colour = COLOURS[k % COLOURS.len()];
        for line in lines {
            if line.len() < 2 {
                continue;
            }
            let pts: Vec<String> = line
                .iter()
                .map(|p| format!("{:.1},{:.1}", x(p[0]), y(p[1])))
                .collect();
            let _ = writeln!(
                o,
                "<polyline points=\"{}\" fill=\"none\" stroke=\"{colour}\" stroke-width=\"1.6\"/>",
                pts.join(" ")
            );
        }
    }
    let _ = writeln!(o, "</g>");
    // The buildings' sections: ground to eaves, the roof dashed above;
    // their labels staggered where they would overlap.
    let mut taken: Vec<(f64, f64, f64)> = Vec::new(); // x0, x1, y
    for (b, sec, m) in sections {
        let (x0, x1) = (x(sec.s0), x(sec.s1).max(x(sec.s0) + 1.0));
        let _ = writeln!(
            o,
            "<rect x=\"{x0:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"#8a8a8a\" fill-opacity=\"0.45\" stroke=\"#222\" stroke-width=\"1.2\"/>",
            y(sec.eaves),
            x1 - x0,
            ((sec.eaves - sec.z0) * scale).max(1.0)
        );
        if sec.z1 > sec.eaves + 0.05 {
            let _ = writeln!(
                o,
                "<rect x=\"{x0:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"#8a8a8a\" fill-opacity=\"0.15\" stroke=\"#222\" stroke-dasharray=\"4 3\"/>",
                y(sec.z1),
                x1 - x0,
                (sec.z1 - sec.eaves) * scale
            );
        }
        let text = match m {
            Some(m) => format!("{} max {m:.digits$} {unit}", b.id),
            None => b.id.clone(),
        };
        let tw = text_width(&text, 11.0);
        let cx = ((x0 + x1) / 2.0).clamp(x(lo) + tw / 2.0, x(hi) - tw / 2.0);
        let mut ty = y(sec.z1) - 6.0;
        while taken
            .iter()
            .any(|(a, b, yy)| (ty - yy).abs() < 13.0 && cx - tw / 2.0 < *b && cx + tw / 2.0 > *a)
        {
            ty -= 14.0;
        }
        let ty = ty.max(y(zhi) + 12.0);
        taken.push((cx - tw / 2.0, cx + tw / 2.0, ty));
        let _ = writeln!(
            o,
            "<text x=\"{cx:.1}\" y=\"{ty:.1}\" text-anchor=\"middle\" font-weight=\"bold\" fill=\"#111\" \
             stroke=\"#ffffff\" stroke-width=\"3\" paint-order=\"stroke\">{}</text>",
            xml(&text)
        );
    }
    // The wires crossing the cut, labelled outwards from the axis.
    for (name, s, z, earth) in wires {
        let colour = if *earth { "#2f855a" } else { "#9b2c2c" };
        let (tx, anchor) = if *s < -1e-6 {
            (x(*s) - 7.0, "end")
        } else {
            (x(*s) + 7.0, "start")
        };
        let _ = writeln!(
            o,
            "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"4\" fill=\"{colour}\" stroke=\"#fff\"/>\
             <text x=\"{tx:.1}\" y=\"{:.1}\" font-size=\"10\" text-anchor=\"{anchor}\" fill=\"{colour}\" \
             stroke=\"#ffffff\" stroke-width=\"3\" paint-order=\"stroke\">{}</text>",
            x(*s),
            y(*z),
            y(*z) + 3.5,
            xml(name)
        );
    }
    // The legend.
    let lx = left + plot_w + 24.0;
    let _ = writeln!(
        o,
        "<text x=\"{lx:.1}\" y=\"{:.1}\" font-size=\"12\" font-weight=\"bold\">{}</text>",
        top + 4.0,
        xml(&format!("{q} levels"))
    );
    for (k, (kind, text)) in legend.iter().enumerate() {
        let ly = top + 24.0 + 18.0 * k as f64;
        let mark = match kind.as_str() {
            "ground" => format!(
                "<rect x=\"{lx:.1}\" y=\"{:.1}\" width=\"26\" height=\"8\" fill=\"#d9c7a7\" stroke=\"#7b5e3b\"/>",
                ly - 4.0
            ),
            "building" => format!(
                "<rect x=\"{lx:.1}\" y=\"{:.1}\" width=\"26\" height=\"10\" fill=\"#8a8a8a\" fill-opacity=\"0.45\" stroke=\"#222\"/>",
                ly - 5.0
            ),
            "roof" => format!(
                "<rect x=\"{lx:.1}\" y=\"{:.1}\" width=\"26\" height=\"10\" fill=\"#8a8a8a\" fill-opacity=\"0.15\" stroke=\"#222\" stroke-dasharray=\"4 3\"/>",
                ly - 5.0
            ),
            "phase" | "earth" => format!(
                "<circle cx=\"{:.1}\" cy=\"{ly:.1}\" r=\"4\" fill=\"{}\"/>",
                lx + 13.0,
                if kind == "earth" {
                    "#2f855a"
                } else {
                    "#9b2c2c"
                }
            ),
            "axis" => format!(
                "<line x1=\"{lx:.1}\" y1=\"{ly:.1}\" x2=\"{:.1}\" y2=\"{ly:.1}\" stroke=\"#666\" stroke-dasharray=\"8 5\"/>",
                lx + 26.0
            ),
            colour => format!(
                "<rect x=\"{lx:.1}\" y=\"{:.1}\" width=\"26\" height=\"10\" fill=\"{}\"/>\
                 <line x1=\"{lx:.1}\" y1=\"{ly:.1}\" x2=\"{:.1}\" y2=\"{ly:.1}\" stroke=\"{colour}\" stroke-width=\"2\"/>",
                ly - 5.0,
                tint(colour),
                lx + 26.0
            ),
        };
        let _ = writeln!(
            o,
            "{mark}<text x=\"{:.1}\" y=\"{:.1}\">{}</text>",
            lx + 34.0,
            ly + 4.0,
            xml(text)
        );
    }
    // The scale bar.
    let bar = tick(scale, 60.0);
    let by = h - 14.0;
    let _ = writeln!(
        o,
        "<line x1=\"{left}\" y1=\"{by:.1}\" x2=\"{:.1}\" y2=\"{by:.1}\" stroke=\"#000\" stroke-width=\"3\"/>\
         <text x=\"{:.1}\" y=\"{:.1}\">{bar} m</text>",
        left + bar * scale,
        left + bar * scale + 6.0,
        by + 4.0
    );
    o.push_str("</svg>\n");
    o
}

fn geojson(buildings: &[BuildingIn], cuts: &[ResultCut], preset: &PresetOut) -> String {
    let mut features = Vec::new();
    for b in buildings {
        let mut ring: Vec<Value> = b
            .ring
            .iter()
            .map(|p| serde_json::json!([p[0], p[1]]))
            .collect();
        if let Some(first) = ring.first().cloned() {
            ring.push(first);
        }
        let mut props = serde_json::Map::new();
        props.insert("kind".into(), "building".into());
        props.insert("id".into(), b.id.clone().into());
        props.insert("ground".into(), b.ground.into());
        props.insert("top".into(), b.top.into());
        for (k, v) in &b.fields {
            props.insert(k.clone(), v.clone().into());
        }
        features.push(serde_json::json!({
            "type": "Feature",
            "properties": props,
            "geometry": {"type": "Polygon", "coordinates": [ring]},
        }));
    }
    for c in cuts {
        let (lo, hi) = c.span();
        let a = [c.origin[0] + lo * c.right[0], c.origin[1] + lo * c.right[1]];
        let b = [c.origin[0] + hi * c.right[0], c.origin[1] + hi * c.right[1]];
        features.push(serde_json::json!({
            "type": "Feature",
            "properties": {"kind": "cut", "label": c.label, "alignment": c.alignment,
                "station": c.station, "from": c.from, "to": c.to, "at": c.at},
            "geometry": {"type": "LineString", "coordinates": [a, b]},
        }));
    }
    for p in &preset.points {
        features.push(serde_json::json!({
            "type": "Feature",
            "properties": {"kind": "place", "name": p.name, "height": p.height},
            "geometry": {"type": "Point", "coordinates": [p.x, p.y]},
        }));
    }
    serde_json::to_string_pretty(&serde_json::json!({
        "type": "FeatureCollection",
        "name": "building-cuts (coordinates in the line's coordinate system)",
        "features": features,
    }))
    .unwrap_or_default()
}

fn dxf(buildings: &[BuildingIn], cuts: &[ResultCut], preset: &PresetOut) -> String {
    let mut o = String::from("0\nSECTION\n2\nENTITIES\n");
    let polyline = |o: &mut String, layer: &str, pts: &[P2], closed: bool| {
        let _ = write!(
            o,
            "0\nPOLYLINE\n8\n{layer}\n66\n1\n70\n{}\n",
            u8::from(closed)
        );
        for p in pts {
            let _ = write!(
                o,
                "0\nVERTEX\n8\n{layer}\n10\n{:.3}\n20\n{:.3}\n30\n0.0\n",
                p[0], p[1]
            );
        }
        let _ = write!(o, "0\nSEQEND\n8\n{layer}\n");
    };
    let text = |o: &mut String, layer: &str, p: P2, t: &str| {
        let _ = write!(
            o,
            "0\nTEXT\n8\n{layer}\n10\n{:.3}\n20\n{:.3}\n30\n0.0\n40\n1.0\n1\n{t}\n",
            p[0], p[1]
        );
    };
    for b in buildings {
        polyline(&mut o, "BUILDINGS", &b.ring, true);
        if let Some(p) = b.ring.first() {
            text(&mut o, "BUILDINGS", *p, &b.id);
        }
    }
    for c in cuts {
        let (lo, hi) = c.span();
        let a = [c.origin[0] + lo * c.right[0], c.origin[1] + lo * c.right[1]];
        let b = [c.origin[0] + hi * c.right[0], c.origin[1] + hi * c.right[1]];
        polyline(&mut o, "CUTS", &[a, b], false);
        text(&mut o, "CUTS", b, &c.label);
    }
    for p in &preset.points {
        let _ = write!(
            o,
            "0\nPOINT\n8\nPLACES\n10\n{:.3}\n20\n{:.3}\n30\n{:.3}\n",
            p.x, p.y, p.height
        );
    }
    o.push_str("0\nENDSEC\n0\nEOF\n");
    o
}
