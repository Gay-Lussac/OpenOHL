// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The report of one field's results (`ohl-field-results`): the host
//! calls it once per run, with the magnetic field's results or the
//! electric field's. Every file carrying field values is named with the
//! quantity (`buildings-B.csv`, `buildings-E.csv`, …), so running both
//! fields into one folder keeps both; the files without field values
//! (`buildings.geojson`, `buildings.dxf`) are the same bytes from either
//! run.

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

/// A building of the preset: its outline and its summary note's fields.
struct BuildingIn {
    id: String,
    ring: Vec<P2>,
    ground: f64,
    top: f64,
    fields: BTreeMap<String, String>,
}

/// A building's section in a cut: offsets and elevations.
#[derive(Clone, Copy)]
struct Section {
    s0: f64,
    s1: f64,
    z0: f64,
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

/// A cut's wires from its note: `cut LABEL: wire NL1 -5.00 415.20; wire EW 0.00 430.00 earth`.
fn crossings(notes: &[String]) -> BTreeMap<String, Vec<(String, f64, f64, bool)>> {
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

/// The report files of `results` (one field) for `preset`.
pub fn report(params: &Params, preset: &PresetOut, results: &str) -> Result<Vec<File>, String> {
    let v: Value = serde_json::from_str(results).map_err(|e| format!("results: {e}"))?;
    if v["format"] != "ohl-field-results" {
        return Err("results: not ohl-field-results".into());
    }
    let q = v["quantity"].as_str().unwrap_or("B").to_owned();
    let unit = v["unit"].as_str().unwrap_or("").to_owned();
    let magnetic = q == "B";
    let unit_col = if magnetic { "uT" } else { "kV_m" };
    let limit = if magnetic {
        params.limit_ut
    } else {
        params.limit_kv_m
    };
    let digits = if magnetic { 3 } else { 4 };
    let summaries = summary_fields(&preset.notes);
    let wires = crossings(&preset.notes);
    let buildings: Vec<BuildingIn> = preset
        .outlines
        .iter()
        .map(|o| BuildingIn {
            id: o.id.clone(),
            ring: o.ring.clone(),
            ground: o.ground,
            top: o.top,
            fields: summaries.get(&o.id).cloned().unwrap_or_default(),
        })
        .collect();
    let cuts: Vec<ResultCut> = v["cuts"]
        .as_array()
        .into_iter()
        .flatten()
        .map(ResultCut::parse)
        .collect();
    let mut warnings = Vec::new();
    let mut files = Vec::new();
    let mut section_max: BTreeMap<String, f64> = BTreeMap::new();
    for c in &cuts {
        if let Some(p) = preset.cuts.iter().find(|p| p.label == c.label)
            && p.from == c.to
            && p.to == c.from
            && (p.fraction - 0.5).abs() > 1e-9
        {
            warnings.push(format!(
                "cut {}: the engine measured it from {} (asked from {}): it lies at {:.1} m from \
                 {}, not where the plugin placed it",
                c.label, c.from, p.from, c.at, c.from
            ));
        }
        let (lo, hi) = c.span();
        let mut sections: Vec<(&BuildingIn, Section, Option<f64>)> = Vec::new();
        for b in &buildings {
            let Some((s0, s1)) = line_interval(c.origin, c.right, &b.ring) else {
                continue;
            };
            if s1 < lo || s0 > hi {
                continue;
            }
            let sec = Section {
                s0: s0.max(lo),
                s1: s1.min(hi),
                z0: b.ground,
                z1: b.top,
            };
            let m = c.largest_in(sec.s0, sec.s1, sec.z0, sec.z1);
            if let Some(m) = m {
                let e = section_max.entry(b.id.clone()).or_insert(f64::NEG_INFINITY);
                *e = e.max(m);
            }
            sections.push((b, sec, m));
        }
        let base = format!("cut-{}-{q}", safe_name(&c.label));
        files.push(file(
            format!("{base}.csv"),
            "text/csv",
            cut_csv(c, &q, unit_col, digits),
        ));
        files.push(file(
            format!("{base}.svg"),
            "image/svg+xml",
            cut_svg(
                c,
                &q,
                &unit,
                &sections,
                wires.get(&c.label).map_or(&[][..], |w| &w[..]),
                digits,
            ),
        ));
    }
    // The places.
    let mut point_max: BTreeMap<String, f64> = BTreeMap::new();
    let mut major_max: BTreeMap<String, f64> = BTreeMap::new();
    let mut pts = String::new();
    let _ = write!(
        pts,
        "name,building,floor,place,x,y,z,height_m,{q}_{unit_col}"
    );
    if magnetic {
        let _ = write!(pts, ",{q}_major_{unit_col}");
    }
    if limit.is_some() {
        let _ = write!(pts, ",margin_{unit_col}");
    }
    pts.push('\n');
    for p in v["points"].as_array().into_iter().flatten() {
        let name = p["name"].as_str().unwrap_or("");
        let parts: Vec<&str> = name.split('-').collect();
        let id = parts.first().copied().unwrap_or("").to_owned();
        let value = p["value"].as_f64();
        let major = p["major"].as_f64();
        if let Some(x) = value {
            let e = point_max.entry(id.clone()).or_insert(f64::NEG_INFINITY);
            *e = e.max(x);
        }
        if let Some(x) = major {
            let e = major_max.entry(id.clone()).or_insert(f64::NEG_INFINITY);
            *e = e.max(x);
        }
        let _ = write!(
            pts,
            "{},{},{},{},{},{},{},{},{}",
            csv_field(name),
            csv_field(&id),
            parts.get(1).copied().unwrap_or(""),
            parts.get(2).copied().unwrap_or(""),
            opt(p["x"].as_f64(), 2),
            opt(p["y"].as_f64(), 2),
            opt(p["z"].as_f64(), 2),
            opt(p["height"].as_f64(), 2),
            opt(value, digits),
        );
        if magnetic {
            let _ = write!(pts, ",{}", opt(major, digits));
        }
        if let Some(l) = limit {
            // The larger of the value and the cycle's largest.
            let worst = match (value, major) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            };
            let _ = write!(pts, ",{}", opt(worst.map(|x| l - x), digits));
        }
        pts.push('\n');
    }
    // The summary.
    let mut sum = String::new();
    let _ = write!(
        sum,
        "id,points,area_m2,centroid_x,centroid_y,ground_z,eaves_z,roof_z,floors,span,\
         station_from_m,station_to_m,axis_distance_m,conductor_distance_m,cuts,\
         max_section_{q}_{unit_col},max_places_{q}_{unit_col}"
    );
    if magnetic {
        let _ = write!(sum, ",max_places_{q}_major_{unit_col}");
    }
    if limit.is_some() {
        let _ = write!(sum, ",limit_{unit_col},margin_{unit_col}");
    }
    sum.push('\n');
    for b in &buildings {
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
        let sec = section_max.get(&b.id).copied();
        let pm = point_max.get(&b.id).copied();
        let mm = major_max.get(&b.id).copied();
        let _ = write!(
            sum,
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
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
            stations.first().cloned().unwrap_or_default(),
            stations.get(1).cloned().unwrap_or_default(),
            first("axis"),
            first("conductor"),
            n_cuts,
            opt(sec, digits),
            opt(pm, digits),
        );
        if magnetic {
            let _ = write!(sum, ",{}", opt(mm, digits));
        }
        if let Some(l) = limit {
            // Conservative: the largest of the section and the places
            // (their value and their cycle's largest).
            let worst = [sec, pm, mm]
                .into_iter()
                .flatten()
                .fold(None, |a: Option<f64>, x| Some(a.map_or(x, |a| a.max(x))));
            let _ = write!(sum, ",{l},{}", opt(worst.map(|w| l - w), digits));
        }
        sum.push('\n');
    }
    files.push(file(format!("buildings-{q}.csv"), "text/csv", sum));
    files.push(file(format!("points-{q}.csv"), "text/csv", pts));
    files.push(file(
        "buildings.geojson".into(),
        "application/geo+json",
        geojson(&buildings, &cuts, preset),
    ));
    if params.dxf {
        files.push(file(
            "buildings.dxf".into(),
            "image/vnd.dxf",
            dxf(&buildings, &cuts, preset),
        ));
    }
    let mut text = format!(
        "building-cuts report, {q} ({unit}), preset {}\n\n",
        preset.name
    );
    for w in &warnings {
        let _ = writeln!(text, "WARNING {w}");
    }
    for n in &preset.notes {
        let _ = writeln!(text, "plugin: {n}");
    }
    for n in v["notes"].as_array().into_iter().flatten() {
        let _ = writeln!(text, "field: {}", n.as_str().unwrap_or(""));
    }
    files.push(file(format!("notes-{q}.txt"), "text/plain", text));
    Ok(files)
}

fn cut_csv(c: &ResultCut, q: &str, unit_col: &str, digits: usize) -> String {
    let mut s = format!(
        "# cut {}, span {}–{} at {:.2} m; {q} at every point of the grid, below_ground 1 where under the ground\n\
         offset_m,elevation_m,ground_m,{q}_{unit_col},below_ground\n",
        c.label, c.from, c.to, c.at
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

fn cut_svg(
    c: &ResultCut,
    q: &str,
    unit: &str,
    sections: &[(&BuildingIn, Section, Option<f64>)],
    wires: &[(String, f64, f64, bool)],
    digits: usize,
) -> String {
    let (lo, hi) = c.span();
    let zlo = c.elevations.first().copied().unwrap_or(0.0);
    let zhi = c.elevations.last().copied().unwrap_or(zlo + 1.0);
    let width_m = (hi - lo).max(1.0);
    let scale = (900.0 / width_m).clamp(2.0, 20.0);
    let (left, top, right, bottom) = (60.0, 50.0, 170.0, 70.0);
    let w = left + width_m * scale + right;
    let h = top + (zhi - zlo).max(1.0) * scale + bottom;
    let x = |s: f64| left + (s - lo) * scale;
    let y = |z: f64| top + (zhi - z) * scale;
    let mut o = String::new();
    let _ = writeln!(
        o,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.0}\" height=\"{h:.0}\" \
         viewBox=\"0 0 {w:.1} {h:.1}\" font-family=\"sans-serif\" font-size=\"11\">"
    );
    let _ = writeln!(o, "<rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>");
    let largest = sections
        .iter()
        .filter_map(|s| s.2)
        .fold(None, |a: Option<f64>, v| Some(a.map_or(v, |a| a.max(v))));
    let _ = writeln!(
        o,
        "<text x=\"{left}\" y=\"20\" font-size=\"14\">{}</text>",
        xml(&format!(
            "Cut {}: span {}–{} at {:.1} m; {q} in {unit}{}",
            c.label,
            c.from,
            c.to,
            c.at,
            largest.map_or(String::new(), |m| format!(
                "; largest in a building's section {m:.digits$} {unit}"
            ))
        ))
    );
    let _ = writeln!(
        o,
        "<text x=\"{left}\" y=\"36\" fill=\"#555\">{}</text>",
        xml("looking from the first structure to the second; offsets to the right positive, 1:1")
    );
    // The frame.
    let _ = writeln!(
        o,
        "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"none\" stroke=\"#bbb\"/>",
        x(lo),
        y(zhi),
        width_m * scale,
        (zhi - zlo) * scale
    );
    // The axis.
    if lo <= 0.0 && hi >= 0.0 {
        let _ = writeln!(
            o,
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#888\" stroke-dasharray=\"6 4\"/>",
            x(0.0),
            y(zhi),
            x(0.0),
            y(zlo)
        );
    }
    // The ground.
    let ground: Vec<String> = c
        .offsets
        .iter()
        .zip(&c.ground)
        .filter_map(|(s, g)| g.map(|g| format!("{:.1},{:.1}", x(*s), y(g))))
        .collect();
    if ground.len() > 1 {
        let _ = writeln!(
            o,
            "<polyline points=\"{}\" fill=\"none\" stroke=\"#7b5e3b\" stroke-width=\"2\"/>",
            ground.join(" ")
        );
    }
    // The isolines.
    for (k, (level, lines)) in c.isolines.iter().enumerate() {
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
                "<polyline points=\"{}\" fill=\"none\" stroke=\"{colour}\" stroke-width=\"1.5\"/>",
                pts.join(" ")
            );
        }
        let ly = top + 20.0 * k as f64;
        let lx = left + width_m * scale + 20.0;
        let _ = writeln!(
            o,
            "<line x1=\"{lx:.1}\" y1=\"{ly:.1}\" x2=\"{:.1}\" y2=\"{ly:.1}\" stroke=\"{colour}\" stroke-width=\"2\"/>\
             <text x=\"{:.1}\" y=\"{:.1}\">{}</text>",
            lx + 24.0,
            lx + 30.0,
            ly + 4.0,
            xml(&format!("{level} {unit}"))
        );
    }
    // The buildings' sections.
    for (b, s, m) in sections {
        let _ = writeln!(
            o,
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"#999\" fill-opacity=\"0.25\" stroke=\"#333\"/>",
            x(s.s0),
            y(s.z1),
            ((s.s1 - s.s0) * scale).max(1.0),
            ((s.z1 - s.z0) * scale).max(1.0)
        );
        let _ = writeln!(
            o,
            "<text x=\"{:.1}\" y=\"{:.1}\">{}</text>",
            x(s.s0),
            y(s.z1) - 4.0,
            xml(&match m {
                Some(m) => format!("{} (largest {m:.digits$} {unit})", b.id),
                None => b.id.clone(),
            })
        );
    }
    // The wires crossing the cut.
    for (name, s, z, earth) in wires {
        let colour = if *earth { "#2f855a" } else { "#9b2c2c" };
        let _ = writeln!(
            o,
            "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"3.5\" fill=\"{colour}\"/>\
             <text x=\"{:.1}\" y=\"{:.1}\" font-size=\"9\" fill=\"{colour}\">{}</text>",
            x(*s),
            y(*z),
            x(*s) + 5.0,
            y(*z) - 4.0,
            xml(name)
        );
    }
    // The scale: 10 m (5 m when wide on the page).
    let bar = if scale > 30.0 { 5.0 } else { 10.0 };
    let by = h - 25.0;
    let _ = writeln!(
        o,
        "<line x1=\"{left}\" y1=\"{by:.1}\" x2=\"{:.1}\" y2=\"{by:.1}\" stroke=\"#000\" stroke-width=\"3\"/>\
         <text x=\"{:.1}\" y=\"{:.1}\">{bar} m</text>",
        left + bar * scale,
        left + bar * scale + 6.0,
        by + 4.0
    );
    let _ = writeln!(
        o,
        "<text x=\"{:.1}\" y=\"{:.1}\" fill=\"#555\">{}</text>",
        left + bar * scale + 60.0,
        by + 4.0,
        xml(&format!(
            "offsets {lo:.0} to {hi:.0} m, elevations {zlo:.0} to {zhi:.0} m"
        ))
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
            "properties": {"kind": "cut", "label": c.label, "from": c.from, "to": c.to, "at": c.at},
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
