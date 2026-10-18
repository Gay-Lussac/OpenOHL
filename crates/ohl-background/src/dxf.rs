// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Drawings read from DXF files (ASCII): lines, polylines (with arcs),
//! circles, arcs, ellipses, points and texts, by layer, in their colours;
//! blocks inserted (with their attributes). Coordinates as in the file
//! (the project's system and units). Other entities (hatches, dimensions,
//! splines, faces, …) are counted, not drawn.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

/// A layer of a drawing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawingLayer {
    /// Name.
    pub name: String,
    /// Its colour.
    pub color: [u8; 3],
    /// Switched off in the file.
    #[serde(default)]
    pub off: bool,
}

/// A polyline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Polyline {
    /// Layer (index).
    pub layer: u32,
    /// Colour.
    pub color: [u8; 3],
    /// Points.
    pub points: Vec<[f64; 3]>,
    /// Back to the first point.
    #[serde(default)]
    pub closed: bool,
}

/// A point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mark {
    /// Layer (index).
    pub layer: u32,
    /// Colour.
    pub color: [u8; 3],
    /// Where.
    pub at: [f64; 3],
}

/// A text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Label {
    /// Layer (index).
    pub layer: u32,
    /// Colour.
    pub color: [u8; 3],
    /// Where (its insertion point).
    pub at: [f64; 3],
    /// Letter height, drawing units.
    pub height: f64,
    /// Direction, degrees counter-clockwise from east.
    pub angle_deg: f64,
    /// The text (lines separated by `\n`).
    pub text: String,
}

/// A drawing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Drawing {
    /// Layers.
    pub layers: Vec<DrawingLayer>,
    /// Lines and polylines (arcs and circles as polylines).
    pub lines: Vec<Polyline>,
    /// Points.
    #[serde(default)]
    pub points: Vec<Mark>,
    /// Texts.
    #[serde(default)]
    pub texts: Vec<Label>,
    /// Entities not drawn, by kind.
    #[serde(default)]
    pub skipped: BTreeMap<String, usize>,
}

impl Drawing {
    /// Lowest and highest (x, y, z) of everything drawn.
    pub fn bounds(&self) -> Option<([f64; 3], [f64; 3])> {
        let mut out: Option<([f64; 3], [f64; 3])> = None;
        let all = self
            .lines
            .iter()
            .flat_map(|l| l.points.iter())
            .chain(self.points.iter().map(|m| &m.at))
            .chain(self.texts.iter().map(|t| &t.at));
        for p in all {
            let b = out.get_or_insert((*p, *p));
            for ((lo, hi), v) in b.0.iter_mut().zip(b.1.iter_mut()).zip(p) {
                *lo = lo.min(*v);
                *hi = hi.max(*v);
            }
        }
        out
    }

    /// Moves every point (e.g. into another coordinate system). Text
    /// directions follow the rotation of the grid at each text.
    pub fn transform(&mut self, transform: crate::Move<'_>) -> Result<(), String> {
        let mut pts: Vec<[f64; 3]> = Vec::new();
        for l in &self.lines {
            pts.extend(&l.points);
        }
        pts.extend(self.points.iter().map(|m| m.at));
        for t in &self.texts {
            let a = t.angle_deg.to_radians();
            pts.push(t.at);
            pts.push([t.at[0] + a.cos(), t.at[1] + a.sin(), t.at[2]]);
        }
        transform(&mut pts)?;
        let mut it = pts.into_iter();
        let mut next = || it.next().expect("as many points as gathered");
        for l in &mut self.lines {
            for p in &mut l.points {
                *p = next();
            }
        }
        for m in &mut self.points {
            m.at = next();
        }
        for t in &mut self.texts {
            t.at = next();
            let d = next();
            t.angle_deg = (d[1] - t.at[1]).atan2(d[0] - t.at[0]).to_degrees();
        }
        Ok(())
    }

    /// Number of things drawn.
    pub fn len(&self) -> usize {
        self.lines.len() + self.points.len() + self.texts.len()
    }

    /// Whether nothing is drawn.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The AutoCAD colour index (ACI) as RGB: the seven named colours, two
/// greys, 240 hues in 24 steps of 15° (each at five brightnesses, full and
/// pastel) and six greys.
pub fn aci(index: u16) -> [u8; 3] {
    match index {
        1 => [255, 0, 0],
        2 => [255, 255, 0],
        3 => [0, 255, 0],
        4 => [0, 255, 255],
        5 => [0, 0, 255],
        6 => [255, 0, 255],
        8 => [128, 128, 128],
        9 => [192, 192, 192],
        10..=249 => {
            let hue = f64::from((index - 10) / 10) * 15.0;
            let step = (index % 10) as usize;
            let value = [1.0, 1.0, 0.8, 0.8, 0.6, 0.6, 0.5, 0.5, 0.3, 0.3][step];
            let saturation = if step.is_multiple_of(2) { 1.0 } else { 0.5 };
            hsv(hue, saturation, value)
        }
        250..=255 => {
            let g = [51, 91, 132, 173, 214, 255][usize::from(index - 250)];
            [g, g, g]
        }
        _ => [255, 255, 255],
    }
}

fn hsv(h: f64, s: f64, v: f64) -> [u8; 3] {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [r, g, b].map(|t| ((t + m) * 255.0).round() as u8)
}

/// One record: an entity or table entry and its group codes.
#[derive(Debug, Clone)]
struct Record {
    kind: String,
    codes: Vec<(i32, String)>,
}

impl Record {
    fn str(&self, code: i32) -> Option<&str> {
        self.codes
            .iter()
            .find(|(c, _)| *c == code)
            .map(|(_, v)| v.as_str())
    }

    fn f(&self, code: i32) -> Option<f64> {
        self.str(code)?.trim().parse().ok()
    }

    fn i(&self, code: i32) -> Option<i64> {
        self.str(code)?.trim().parse().ok()
    }

    fn point(&self, code: i32) -> Option<[f64; 3]> {
        Some([
            self.f(code)?,
            self.f(code + 10)?,
            self.f(code + 20).unwrap_or(0.0),
        ])
    }

    fn extrusion(&self) -> [f64; 3] {
        match (self.f(210), self.f(220), self.f(230)) {
            (Some(x), Some(y), Some(z)) => [x, y, z],
            _ => [0.0, 0.0, 1.0],
        }
    }
}

/// Text of a DXF file: UTF-8 (2007 and later), else one byte per character
/// (older files, Windows-1252 read as Latin-1); `\U+XXXX` escapes decoded.
fn text_of(bytes: &[u8]) -> String {
    let raw = match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
    };
    if !raw.contains("\\U+") {
        return raw;
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw.as_str();
    while let Some(at) = rest.find("\\U+") {
        out.push_str(&rest[..at]);
        let hex = rest.get(at + 3..at + 7).unwrap_or("");
        match u32::from_str_radix(hex, 16).ok().and_then(char::from_u32) {
            Some(c) if hex.len() == 4 => {
                out.push(c);
                rest = &rest[at + 7..];
            }
            _ => {
                out.push_str("\\U+");
                rest = &rest[at + 3..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn records(text: &str) -> Result<Vec<Record>, String> {
    let mut lines = text.lines();
    let mut out: Vec<Record> = Vec::new();
    let mut n = 0usize;
    while let Some(code) = lines.next() {
        n += 1;
        let Some(value) = lines.next() else {
            break;
        };
        let code: i32 = code.trim().parse().map_err(|_| {
            format!(
                "line {}: \"{}\" is not a group code",
                2 * n - 1,
                code.trim()
            )
        })?;
        let value = value.trim_end_matches('\r');
        if code == 0 {
            out.push(Record {
                kind: value.trim().to_ascii_uppercase(),
                codes: Vec::new(),
            });
        } else if let Some(r) = out.last_mut() {
            r.codes.push((code, value.trim_start().to_owned()));
        }
    }
    Ok(out)
}

/// Placement of a block: world = at + R(rotation) · S · (p − base).
#[derive(Debug, Clone, Copy)]
struct Place {
    m: [[f64; 3]; 3],
    t: [f64; 3],
    color: [u8; 3],
}

impl Place {
    const IDENTITY: Place = Place {
        m: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        t: [0.0; 3],
        color: [255, 255, 255],
    };

    fn apply(&self, p: [f64; 3]) -> [f64; 3] {
        let m = &self.m;
        [0, 1, 2].map(|r| m[r][0] * p[0] + m[r][1] * p[1] + m[r][2] * p[2] + self.t[r])
    }

    /// `self` after `inner` (inner's points placed by self).
    fn then(&self, inner: &Place) -> Place {
        let mut m = [[0.0; 3]; 3];
        for (r, row) in m.iter_mut().enumerate() {
            for (c, v) in row.iter_mut().enumerate() {
                *v = (0..3).map(|k| self.m[r][k] * inner.m[k][c]).sum();
            }
        }
        Place {
            m,
            t: self.apply(inner.t),
            color: inner.color,
        }
    }

    /// Scale applied to lengths in the plane (for text heights).
    fn scale(&self) -> f64 {
        (self.m[0][0] * self.m[1][1] - self.m[0][1] * self.m[1][0])
            .abs()
            .sqrt()
    }

    /// Rotation of the x axis in plan, degrees.
    fn angle(&self) -> f64 {
        self.m[1][0].atan2(self.m[0][0]).to_degrees()
    }
}

/// The object coordinate system of extrusion `n` (arbitrary axis
/// algorithm): a point given in it, in world coordinates.
fn ocs(n: [f64; 3], p: [f64; 3]) -> [f64; 3] {
    if n == [0.0, 0.0, 1.0] {
        return p;
    }
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len == 0.0 {
        return p;
    }
    let n = n.map(|v| v / len);
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let unit = |v: [f64; 3]| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        v.map(|x| x / l)
    };
    let ax = if n[0].abs() < 1.0 / 64.0 && n[1].abs() < 1.0 / 64.0 {
        unit(cross([0.0, 1.0, 0.0], n))
    } else {
        unit(cross([0.0, 0.0, 1.0], n))
    };
    let ay = unit(cross(n, ax));
    [0, 1, 2].map(|k| p[0] * ax[k] + p[1] * ay[k] + p[2] * n[k])
}

/// Points of the arc from `a` to `b` with bulge `bulge` (tan of a quarter
/// of its angle; positive: counter-clockwise), `b` excluded, every 10° at
/// most.
fn bulge_points(a: [f64; 3], b: [f64; 3], bulge: f64, out: &mut Vec<[f64; 3]>) {
    out.push(a);
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let d = dx.hypot(dy);
    if bulge.abs() < 1e-9 || d < 1e-12 {
        return;
    }
    let theta = 4.0 * bulge.atan();
    let offset = d * (1.0 - bulge * bulge) / (4.0 * bulge);
    let (mx, my) = (a[0] + dx / 2.0, a[1] + dy / 2.0);
    let (cx, cy) = (mx - dy / d * offset, my + dx / d * offset);
    let r = (a[0] - cx).hypot(a[1] - cy);
    let start = (a[1] - cy).atan2(a[0] - cx);
    let n = ((theta.abs() / 10f64.to_radians()).ceil() as usize).max(2);
    for k in 1..n {
        let t = k as f64 / n as f64;
        let ang = start + theta * t;
        out.push([
            cx + r * ang.cos(),
            cy + r * ang.sin(),
            a[2] + (b[2] - a[2]) * t,
        ]);
    }
}

/// Exit clause: at most this many steps along one arc, circle or ellipse
/// (5° each over a full turn).
const MAX_ARC_STEPS: usize = 72;

/// Steps of at most 5° along `sweep` (radians), 4 to [`MAX_ARC_STEPS`].
fn arc_steps(sweep: f64) -> usize {
    ((sweep.abs() / 5f64.to_radians()).ceil() as usize).clamp(4, MAX_ARC_STEPS)
}

/// A sweep in (0, `full`]: one turn at most (a crafted file's huge
/// angles, or ones whose difference is outside a turn, wrapped round).
fn one_turn(sweep: f64, full: f64) -> f64 {
    if sweep > 0.0 && sweep <= full * (1.0 + 1e-12) {
        return sweep;
    }
    let s = sweep.rem_euclid(full);
    if s < full * 1e-12 { full } else { s }
}

/// The angles of `r` (`codes`): finite numbers, else an error naming the
/// entity.
fn finite_angles(r: &Record, codes: &[i32]) -> Result<(), String> {
    match codes
        .iter()
        .find(|&&c| r.f(c).is_some_and(|v| !v.is_finite()))
    {
        Some(c) => Err(format!("{}: group code {c} is not a finite number", r.kind)),
        None => Ok(()),
    }
}

fn arc_points(c: [f64; 3], r: f64, from: f64, sweep: f64) -> Vec<[f64; 3]> {
    let n = arc_steps(sweep);
    (0..=n)
        .map(|k| {
            let a = from + sweep * k as f64 / n as f64;
            [c[0] + r * a.cos(), c[1] + r * a.sin(), c[2]]
        })
        .collect()
}

/// MTEXT without its formatting codes.
fn plain_mtext(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '{' | '}' => {}
            '\\' => match it.next() {
                Some('P') | Some('n') => out.push('\n'),
                Some('~') => out.push(' '),
                Some(e @ ('\\' | '{' | '}')) => out.push(e),
                Some('S') => {
                    for c in it.by_ref() {
                        match c {
                            ';' => break,
                            '^' | '#' => out.push('/'),
                            c => out.push(c),
                        }
                    }
                }
                Some(c) if "fFHWQTACcpa".contains(c) => {
                    for c in it.by_ref() {
                        if c == ';' {
                            break;
                        }
                    }
                }
                Some(c) if "LlOoKk".contains(c) => {}
                Some(c) => {
                    out.push('\\');
                    out.push(c);
                }
                None => {}
            },
            c => out.push(c),
        }
    }
    out
}

/// TEXT without its `%%` codes.
fn plain_text(s: &str) -> String {
    s.replace("%%d", "°")
        .replace("%%D", "°")
        .replace("%%c", "Ø")
        .replace("%%C", "Ø")
        .replace("%%p", "±")
        .replace("%%P", "±")
        .replace("%%u", "")
        .replace("%%U", "")
        .replace("%%o", "")
        .replace("%%O", "")
        .replace("%%%", "%")
}

/// Exit clause: at most this many things drawn (a runaway block nesting).
pub const MAX_ITEMS: usize = 20_000_000;

struct Reader<'a> {
    blocks: HashMap<String, (Vec<Record>, [f64; 3])>,
    layers: Vec<DrawingLayer>,
    layer_index: HashMap<String, u32>,
    out: Drawing,
    _text: std::marker::PhantomData<&'a ()>,
}

impl Reader<'_> {
    fn layer(&mut self, name: &str) -> u32 {
        if let Some(&i) = self.layer_index.get(name) {
            return i;
        }
        let i = self.layers.len() as u32;
        self.layers.push(DrawingLayer {
            name: name.to_owned(),
            color: [255, 255, 255],
            off: false,
        });
        self.layer_index.insert(name.to_owned(), i);
        i
    }

    fn color(&self, r: &Record, layer: u32, place: &Place) -> [u8; 3] {
        if let Some(rgb) = r.i(420) {
            return [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8];
        }
        match r.i(62) {
            Some(0) => place.color,
            Some(c) if c != 256 && c != -256 => aci(c.unsigned_abs().min(257) as u16),
            _ => self.layers[layer as usize].color,
        }
    }

    fn full(&self) -> Result<(), String> {
        if self.out.len() > MAX_ITEMS {
            return Err(format!(
                "more than {MAX_ITEMS} things to draw: blocks nested too deeply?"
            ));
        }
        Ok(())
    }

    /// Draws `records` placed by `place`; `depth`: block nesting.
    fn entities(&mut self, records: &[Record], place: &Place, depth: usize) -> Result<(), String> {
        let mut k = 0;
        while k < records.len() {
            let r = &records[k];
            k += 1;
            let layer = self.layer(r.str(8).unwrap_or("0"));
            let color = self.color(r, layer, place);
            let n = r.extrusion();
            let world = |p: [f64; 3]| place.apply(ocs(n, p));
            let line = |points: Vec<[f64; 3]>, closed: bool| Polyline {
                layer,
                color,
                points,
                closed,
            };
            match r.kind.as_str() {
                "LINE" => {
                    if let (Some(a), Some(b)) = (r.point(10), r.point(11)) {
                        // Lines are in world coordinates.
                        let lp = |p| place.apply(p);
                        self.out.lines.push(line(vec![lp(a), lp(b)], false));
                    }
                }
                "LWPOLYLINE" => {
                    let z = r.f(38).unwrap_or(0.0);
                    let mut verts: Vec<([f64; 3], f64)> = Vec::new();
                    let mut x = None;
                    for (c, v) in &r.codes {
                        match c {
                            10 => x = v.trim().parse::<f64>().ok(),
                            20 => {
                                if let (Some(x), Ok(y)) = (x.take(), v.trim().parse::<f64>()) {
                                    verts.push(([x, y, z], 0.0));
                                }
                            }
                            42 => {
                                if let (Some(last), Ok(b)) = (verts.last_mut(), v.trim().parse()) {
                                    last.1 = b;
                                }
                            }
                            _ => {}
                        }
                    }
                    let closed = r.i(70).unwrap_or(0) & 1 == 1;
                    let pts = with_bulges(&verts, closed);
                    self.out
                        .lines
                        .push(line(pts.into_iter().map(world).collect(), closed));
                }
                "POLYLINE" => {
                    let flags = r.i(70).unwrap_or(0);
                    let three_d = flags & 8 != 0;
                    let elevation = r.point(10).map_or(0.0, |p| p[2]);
                    let mut verts: Vec<([f64; 3], f64)> = Vec::new();
                    while k < records.len() && records[k].kind == "VERTEX" {
                        let v = &records[k];
                        k += 1;
                        // Polyface and mesh definition vertices are not
                        // drawn.
                        if v.i(70).unwrap_or(0) & 128 != 0 && v.i(70).unwrap_or(0) & 64 == 0 {
                            continue;
                        }
                        if let Some(mut p) = v.point(10) {
                            if !three_d {
                                p[2] = elevation;
                            }
                            verts.push((p, v.f(42).unwrap_or(0.0)));
                        }
                    }
                    if k < records.len() && records[k].kind == "SEQEND" {
                        k += 1;
                    }
                    if flags & (16 | 64) != 0 {
                        *self
                            .out
                            .skipped
                            .entry("POLYLINE (mesh)".into())
                            .or_default() += 1;
                        continue;
                    }
                    let closed = flags & 1 == 1;
                    let pts = with_bulges(&verts, closed);
                    let pts = if three_d {
                        pts.into_iter().map(|p| place.apply(p)).collect()
                    } else {
                        pts.into_iter().map(world).collect()
                    };
                    self.out.lines.push(line(pts, closed));
                }
                "CIRCLE" | "ARC" => {
                    let (Some(c), Some(radius)) = (r.point(10), r.f(40)) else {
                        continue;
                    };
                    finite_angles(r, &[40, 50, 51])?;
                    let (from, sweep) = if r.kind == "ARC" {
                        let (a, mut b) = (r.f(50).unwrap_or(0.0), r.f(51).unwrap_or(360.0));
                        if b <= a {
                            b += 360.0;
                        }
                        (
                            a.rem_euclid(360.0).to_radians(),
                            one_turn(b - a, 360.0).to_radians(),
                        )
                    } else {
                        (0.0, std::f64::consts::TAU)
                    };
                    let pts = arc_points(c, radius, from, sweep);
                    self.out.lines.push(line(
                        pts.into_iter().map(world).collect(),
                        r.kind == "CIRCLE",
                    ));
                }
                "ELLIPSE" => {
                    let (Some(c), Some(m), Some(ratio)) = (r.point(10), r.point(11), r.f(40))
                    else {
                        continue;
                    };
                    finite_angles(r, &[40, 41, 42])?;
                    let (t0, mut t1) = (
                        r.f(41).unwrap_or(0.0),
                        r.f(42).unwrap_or(std::f64::consts::TAU),
                    );
                    if t1 <= t0 {
                        t1 += std::f64::consts::TAU;
                    }
                    let t0 = t0.rem_euclid(std::f64::consts::TAU);
                    let t1 = t0 + one_turn(t1 - t0, std::f64::consts::TAU);
                    // Minor axis: the extrusion × the major axis, scaled.
                    let minor = [
                        (n[1] * m[2] - n[2] * m[1]) * ratio,
                        (n[2] * m[0] - n[0] * m[2]) * ratio,
                        (n[0] * m[1] - n[1] * m[0]) * ratio,
                    ];
                    let steps = arc_steps(t1 - t0);
                    let pts = (0..=steps)
                        .map(|s| {
                            let t = t0 + (t1 - t0) * s as f64 / steps as f64;
                            place.apply(
                                [0, 1, 2].map(|k| c[k] + m[k] * t.cos() + minor[k] * t.sin()),
                            )
                        })
                        .collect();
                    let whole = (t1 - t0 - std::f64::consts::TAU).abs() < 1e-9;
                    self.out.lines.push(line(pts, whole));
                }
                "POINT" => {
                    if let Some(p) = r.point(10) {
                        self.out.points.push(Mark {
                            layer,
                            color,
                            at: place.apply(p),
                        });
                    }
                }
                "TEXT" | "ATTRIB" | "MTEXT" => {
                    let aligned = r.i(72).unwrap_or(0) != 0 || r.i(73).unwrap_or(0) != 0;
                    let at = if r.kind != "MTEXT" && aligned {
                        r.point(11).or_else(|| r.point(10))
                    } else {
                        r.point(10)
                    };
                    let Some(at) = at else {
                        continue;
                    };
                    let (text, angle) = if r.kind == "MTEXT" {
                        let mut t: String = r
                            .codes
                            .iter()
                            .filter(|(c, _)| *c == 3)
                            .map(|(_, v)| v.as_str())
                            .collect();
                        t.push_str(r.str(1).unwrap_or(""));
                        let angle = match (r.f(11), r.f(21)) {
                            (Some(x), Some(y)) => y.atan2(x).to_degrees(),
                            _ => r.f(50).unwrap_or(0.0),
                        };
                        (plain_mtext(&t), angle)
                    } else {
                        (plain_text(r.str(1).unwrap_or("")), r.f(50).unwrap_or(0.0))
                    };
                    if r.kind == "ATTRIB" && r.i(70).unwrap_or(0) & 1 != 0 {
                        continue; // invisible attribute
                    }
                    if text.trim().is_empty() {
                        continue;
                    }
                    // TEXT and MTEXT are in their OCS, MTEXT in world.
                    let p = if r.kind == "MTEXT" {
                        place.apply(at)
                    } else {
                        world(at)
                    };
                    self.out.texts.push(Label {
                        layer,
                        color,
                        at: p,
                        height: r.f(40).unwrap_or(1.0) * place.scale(),
                        angle_deg: angle + place.angle(),
                        text,
                    });
                }
                "INSERT" => {
                    let name = r.str(2).unwrap_or("").to_owned();
                    let with_attributes = r.i(66).unwrap_or(0) == 1;
                    let mut attributes = Vec::new();
                    if with_attributes {
                        while k < records.len() && records[k].kind == "ATTRIB" {
                            attributes.push(records[k].clone());
                            k += 1;
                        }
                        if k < records.len() && records[k].kind == "SEQEND" {
                            k += 1;
                        }
                    }
                    // Attributes are placed already (world coordinates).
                    self.entities(&attributes, place, depth)?;
                    let Some((block, base)) = self.blocks.get(&name).cloned() else {
                        *self
                            .out
                            .skipped
                            .entry(format!("INSERT of unknown block {name}"))
                            .or_default() += 1;
                        continue;
                    };
                    if depth >= 8 {
                        *self
                            .out
                            .skipped
                            .entry("INSERT nested deeper than 8".into())
                            .or_default() += 1;
                        continue;
                    }
                    let at = r.point(10).unwrap_or([0.0; 3]);
                    let s = [
                        r.f(41).unwrap_or(1.0),
                        r.f(42).unwrap_or(1.0),
                        r.f(43).unwrap_or(1.0),
                    ];
                    let rot = r.f(50).unwrap_or(0.0).to_radians();
                    let (c, si) = (rot.cos(), rot.sin());
                    // In the insert's OCS: R · S · (p − base) + at.
                    let m = [
                        [c * s[0], -si * s[1], 0.0],
                        [si * s[0], c * s[1], 0.0],
                        [0.0, 0.0, s[2]],
                    ];
                    let local = Place {
                        m,
                        t: [0, 1, 2].map(|k| {
                            at[k] - (m[k][0] * base[0] + m[k][1] * base[1] + m[k][2] * base[2])
                        }),
                        color,
                    };
                    // The OCS of the insert, then the outer placement.
                    let o = |p| ocs(n, p);
                    let ocs_place = Place {
                        m: [
                            [
                                o([1.0, 0.0, 0.0])[0],
                                o([0.0, 1.0, 0.0])[0],
                                o([0.0, 0.0, 1.0])[0],
                            ],
                            [
                                o([1.0, 0.0, 0.0])[1],
                                o([0.0, 1.0, 0.0])[1],
                                o([0.0, 0.0, 1.0])[1],
                            ],
                            [
                                o([1.0, 0.0, 0.0])[2],
                                o([0.0, 1.0, 0.0])[2],
                                o([0.0, 0.0, 1.0])[2],
                            ],
                        ],
                        t: [0.0; 3],
                        color,
                    };
                    let inner = place.then(&ocs_place).then(&local);
                    self.entities(&block, &inner, depth + 1)?;
                }
                "VERTEX" | "SEQEND" | "ATTDEF" | "VIEWPORT" => {}
                other => {
                    *self.out.skipped.entry(other.to_owned()).or_default() += 1;
                }
            }
            self.full()?;
        }
        Ok(())
    }
}

/// Polyline points with the arcs of their bulges.
fn with_bulges(verts: &[([f64; 3], f64)], closed: bool) -> Vec<[f64; 3]> {
    let mut pts = Vec::new();
    for (i, (p, b)) in verts.iter().enumerate() {
        match verts
            .get(i + 1)
            .or(if closed { verts.first() } else { None })
        {
            Some((q, _)) if verts.len() > 1 => bulge_points(*p, *q, *b, &mut pts),
            _ => pts.push(*p),
        }
    }
    pts
}

/// Reads a DXF file's content (ASCII DXF, any version).
///
/// Cost: linear in the file (~100 MB/s). Exit clause: [`MAX_ITEMS`].
pub fn read_dxf(bytes: &[u8]) -> Result<Drawing, String> {
    if bytes.starts_with(b"AutoCAD Binary DXF") {
        return Err("a binary DXF file: save it as ASCII DXF".into());
    }
    let text = text_of(bytes);
    let all = records(&text)?;
    let mut reader = Reader {
        blocks: HashMap::new(),
        layers: Vec::new(),
        layer_index: HashMap::new(),
        out: Drawing::default(),
        _text: std::marker::PhantomData,
    };
    let mut section = String::new();
    let mut entities: Vec<Record> = Vec::new();
    let mut block: Option<(String, [f64; 3], Vec<Record>)> = None;
    for r in all {
        match r.kind.as_str() {
            "SECTION" => section = r.str(2).unwrap_or("").to_ascii_uppercase(),
            "ENDSEC" => section.clear(),
            "EOF" => break,
            _ => match section.as_str() {
                "TABLES" if r.kind == "LAYER" => {
                    let name = r.str(2).unwrap_or("0").to_owned();
                    let i = reader.layer(&name);
                    let aci_code = r.i(62).unwrap_or(7);
                    let l = &mut reader.layers[i as usize];
                    l.color = match r.i(420) {
                        Some(rgb) => [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8],
                        None => aci(aci_code.unsigned_abs().min(257) as u16),
                    };
                    l.off = aci_code < 0;
                }
                "BLOCKS" => match r.kind.as_str() {
                    "BLOCK" => {
                        block = Some((
                            r.str(2).unwrap_or("").to_owned(),
                            r.point(10).unwrap_or([0.0; 3]),
                            Vec::new(),
                        ));
                    }
                    "ENDBLK" => {
                        if let Some((name, base, recs)) = block.take() {
                            reader.blocks.insert(name, (recs, base));
                        }
                    }
                    _ => {
                        if let Some((_, _, recs)) = &mut block {
                            recs.push(r);
                        }
                    }
                },
                "ENTITIES" => entities.push(r),
                _ => {}
            },
        }
    }
    reader.entities(&entities, &Place::IDENTITY, 0)?;
    reader.out.layers = reader.layers;
    Ok(reader.out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A DXF file from (group code, value) pairs.
    fn dxf(pairs: &[(i32, &str)]) -> Vec<u8> {
        let mut s = String::new();
        for (c, v) in pairs {
            s.push_str(&format!("{c:>3}\r\n{v}\r\n"));
        }
        s.into_bytes()
    }

    fn sample() -> Vec<u8> {
        let mut p: Vec<(i32, &str)> = vec![
            (0, "SECTION"),
            (2, "TABLES"),
            (0, "TABLE"),
            (2, "LAYER"),
            (0, "LAYER"),
            (2, "Parcels"),
            (62, "3"),
            (0, "LAYER"),
            (2, "Hidden"),
            (62, "-1"),
            (0, "ENDTAB"),
            (0, "ENDSEC"),
            (0, "SECTION"),
            (2, "BLOCKS"),
            (0, "BLOCK"),
            (2, "MARK"),
            (10, "1"),
            (20, "1"),
            (30, "0"),
            (0, "LINE"),
            (8, "0"),
            (62, "0"),
            (10, "1"),
            (20, "1"),
            (11, "2"),
            (21, "1"),
            (0, "ENDBLK"),
            (0, "ENDSEC"),
            (0, "SECTION"),
            (2, "ENTITIES"),
        ];
        p.extend([
            // A parcel: a square with one side a half circle (bulge 1).
            (0, "LWPOLYLINE"),
            (8, "Parcels"),
            (70, "1"),
            (38, "450"),
            (10, "0"),
            (20, "0"),
            (10, "10"),
            (20, "0"),
            (42, "1"),
            (10, "10"),
            (20, "10"),
            (10, "0"),
            (20, "10"),
            (0, "LINE"),
            (8, "Roads"),
            (62, "1"),
            (10, "0"),
            (20, "0"),
            (30, "400"),
            (11, "100"),
            (21, "0"),
            (31, "401"),
            (0, "CIRCLE"),
            (8, "Parcels"),
            (420, "65793"),
            (10, "50"),
            (20, "50"),
            (40, "2"),
            (0, "TEXT"),
            (8, "Parcels"),
            (10, "5"),
            (20, "5"),
            (40, "1.5"),
            (50, "90"),
            (1, "Parcel 12%%d"),
            (0, "MTEXT"),
            (8, "Parcels"),
            (10, "7"),
            (20, "7"),
            (40, "2"),
            (1, "{\\fArial|b0;Two\\Plines}"),
            // The block MARK, twice the size, turned 90°, at (100, 100).
            (0, "INSERT"),
            (8, "Marks"),
            (62, "5"),
            (2, "MARK"),
            (10, "100"),
            (20, "100"),
            (41, "2"),
            (42, "2"),
            (50, "90"),
            (0, "HATCH"),
            (8, "Parcels"),
            (0, "ENDSEC"),
            (0, "EOF"),
        ]);
        dxf(&p)
    }

    #[test]
    fn a_cadastre_like_drawing_is_read() {
        let d = read_dxf(&sample()).unwrap();
        let names: Vec<&str> = d.layers.iter().map(|l| l.name.as_str()).collect();
        // Layers of the table first, then as met (the block's "0" when it is
        // inserted).
        assert_eq!(names, ["Parcels", "Hidden", "Roads", "Marks", "0"]);
        assert_eq!(d.layers[0].color, [0, 255, 0]);
        assert!(d.layers[1].off);
        // The parcel: closed, at its elevation, the half circle bulging
        // east to x = 15.
        let parcel = &d.lines[0];
        assert!(parcel.closed && parcel.color == [0, 255, 0]);
        assert!(parcel.points.iter().all(|p| p[2] == 450.0));
        let east = parcel.points.iter().map(|p| p[0]).fold(f64::MIN, f64::max);
        assert!((east - 15.0).abs() < 1e-9, "{east}");
        assert!(parcel.points.len() > 10);
        // The line in its own colour, with its heights.
        assert_eq!(d.lines[1].color, [255, 0, 0]);
        assert_eq!(
            d.lines[1].points,
            vec![[0.0, 0.0, 400.0], [100.0, 0.0, 401.0]]
        );
        // The circle in its true colour (1, 1, 1).
        assert_eq!(d.lines[2].color, [1, 1, 1]);
        assert!(d.lines[2].closed);
        // Texts without their codes.
        assert_eq!(d.texts[0].text, "Parcel 12°");
        assert_eq!(d.texts[0].angle_deg, 90.0);
        assert_eq!(d.texts[1].text, "Two\nlines");
        // The block: base (1, 1) → (100, 100); its line from (1, 1) to
        // (2, 1) → from (100, 100) two units north; BYBLOCK: the insert's
        // blue.
        let mark = d.lines.last().unwrap();
        assert_eq!(mark.color, [0, 0, 255]);
        let (a, b) = (mark.points[0], mark.points[1]);
        assert!(
            (a[0] - 100.0).abs() < 1e-9 && (a[1] - 100.0).abs() < 1e-9,
            "{a:?}"
        );
        assert!(
            (b[0] - 100.0).abs() < 1e-9 && (b[1] - 102.0).abs() < 1e-9,
            "{b:?}"
        );
        assert_eq!(d.skipped.get("HATCH"), Some(&1));
        let (lo, hi) = d.bounds().unwrap();
        assert_eq!((lo[2], hi[2]), (0.0, 450.0));
    }

    #[test]
    fn transforms_move_everything_and_turn_texts() {
        let mut d = read_dxf(&sample()).unwrap();
        let before = d.clone();
        // A quarter turn about the origin.
        let turn = |p: &mut [[f64; 3]]| -> Result<(), String> {
            for q in p {
                *q = [-q[1], q[0], q[2]];
            }
            Ok(())
        };
        d.transform(&turn).unwrap();
        assert_eq!(d.lines[1].points[1], [0.0, 100.0, 401.0]);
        assert!((d.texts[0].angle_deg - 180.0).abs() < 1e-9);
        assert_eq!(d.len(), before.len());
    }

    #[test]
    fn old_files_and_bad_ones() {
        // Latin-1 and \U+ escapes.
        let mut bytes = dxf(&[
            (0, "SECTION"),
            (2, "ENTITIES"),
            (0, "TEXT"),
            (10, "0"),
            (20, "0"),
            (1, "Gr\\U+00FCn "),
        ]);
        bytes.extend(dxf(&[(0, "TEXT"), (10, "0"), (20, "0")]));
        bytes.extend(b"  1\r\nZ\xfcrich\r\n  0\r\nENDSEC\r\n");
        let d = read_dxf(&bytes).unwrap();
        assert_eq!(d.texts[0].text, "Grün ");
        assert_eq!(d.texts[1].text, "Zürich");
        assert!(read_dxf(b"AutoCAD Binary DXF\r\n\x1a\0").is_err());
        assert!(read_dxf(b"x\nSECTION\n").is_err());
        // A block inserting itself stops.
        let d = read_dxf(&dxf(&[
            (0, "SECTION"),
            (2, "BLOCKS"),
            (0, "BLOCK"),
            (2, "B"),
            (0, "LINE"),
            (10, "0"),
            (20, "0"),
            (11, "1"),
            (21, "0"),
            (0, "INSERT"),
            (2, "B"),
            (10, "1"),
            (20, "0"),
            (0, "ENDBLK"),
            (0, "ENDSEC"),
            (0, "SECTION"),
            (2, "ENTITIES"),
            (0, "INSERT"),
            (2, "B"),
            (0, "ENDSEC"),
        ]))
        .unwrap();
        // The block drawn at nesting depths 1 to 8.
        assert_eq!(d.lines.len(), 8);
        assert_eq!(d.skipped.get("INSERT nested deeper than 8"), Some(&1));
    }

    #[test]
    fn crafted_arc_angles_error_or_stay_one_turn() {
        let entities = |e: &[(i32, &str)]| {
            let mut p = vec![(0, "SECTION"), (2, "ENTITIES")];
            p.extend_from_slice(e);
            p.extend([(0, "ENDSEC"), (0, "EOF")]);
            read_dxf(&dxf(&p))
        };
        let centre = [(10, "0"), (20, "0"), (40, "1")];
        // Infinite or NaN angles: a clean error.
        for (code, v) in [(50, "inf"), (51, "-inf"), (51, "NaN")] {
            let mut e = vec![(0, "ARC")];
            e.extend(centre);
            e.push((code, v));
            assert!(entities(&e).is_err(), "ARC {code} {v}");
        }
        let circle = [(0, "CIRCLE"), (10, "0"), (20, "0"), (40, "inf")];
        assert!(entities(&circle).is_err());
        for (code, v) in [(41, "NaN"), (42, "inf")] {
            let e = [
                (0, "ELLIPSE"),
                (10, "0"),
                (20, "0"),
                (11, "2"),
                (21, "0"),
                (40, "0.5"),
                (code, v),
            ];
            assert!(entities(&e).is_err(), "ELLIPSE {code} {v}");
        }
        // Huge finite angles: one turn at most.
        let mut e = vec![(0, "ARC")];
        e.extend(centre);
        e.extend([(50, "-1e300"), (51, "1e300")]);
        let d = entities(&e).unwrap();
        assert!(d.lines[0].points.len() <= MAX_ARC_STEPS + 1);
        let e = [
            (0, "ELLIPSE"),
            (10, "0"),
            (20, "0"),
            (11, "2"),
            (21, "0"),
            (40, "0.5"),
            (41, "-1e300"),
            (42, "1e300"),
        ];
        let d = entities(&e).unwrap();
        assert!(d.lines[0].points.len() <= MAX_ARC_STEPS + 1);
        // An ordinary arc keeps its sweep: 90° in 5° steps.
        let mut e = vec![(0, "ARC")];
        e.extend(centre);
        e.extend([(50, "0"), (51, "90")]);
        let d = entities(&e).unwrap();
        assert_eq!(d.lines[0].points.len(), 19);
    }

    #[test]
    fn the_colour_index() {
        assert_eq!(aci(1), [255, 0, 0]);
        assert_eq!(aci(7), [255, 255, 255]);
        assert_eq!(aci(10), [255, 0, 0]);
        assert_eq!(aci(11), [255, 128, 128]);
        assert_eq!(aci(90), hsv(120.0, 1.0, 1.0));
        assert_eq!(aci(250), [51, 51, 51]);
    }
}
