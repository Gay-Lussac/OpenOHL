// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! ASCII DXF (AutoCAD R12, `AC1009`) of a [`Drawing`]: LINE, POLYLINE,
//! TEXT and CIRCLE entities on the drawing's layers, a LAYER table with
//! colours, a line type per dash pattern. R12 DXF opens in every CAD
//! program (and imports into DWG-based ones); it has no line weights,
//! true colours or images, so pens become the nearest AutoCAD colour index
//! (ACI), SVG logos are written as their outlines (layer
//! `OHL_TITLE_IMAGE`) and PNG/JPEG logos are left out (said so:
//! [`write_counted`]).
//!
//! A sheet is written in mm (y up from the sheet's bottom left corner);
//! model space in metres as it is. What a clip region hides is cut away
//! (R12 has no clipping): polylines at its edges, texts and circles whose
//! point lies outside.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::model::{Anchor, Color, Drawing, Item, Layer, Space, clip_polyline, inside};

/// The AutoCAD colour index (ACI 1–255) nearest to `c`; black, white and
/// dark greys are 7 (black on white paper, white on a dark screen).
pub fn aci(c: Color) -> u8 {
    let (r, g, b) = (f64::from(c.0), f64::from(c.1), f64::from(c.2));
    let grey = (r - g).abs() < 24.0 && (g - b).abs() < 24.0 && (r - b).abs() < 24.0;
    if grey && (r + g + b) / 3.0 < 96.0 || r + g + b > 3.0 * 245.0 {
        return 7;
    }
    (1..=255u8)
        .filter(|&i| i != 7)
        .min_by(|&i, &j| {
            let d = |k: u8| {
                let [x, y, z] = aci_rgb(k);
                (f64::from(x) - r).powi(2) + (f64::from(y) - g).powi(2) + (f64::from(z) - b).powi(2)
            };
            d(i).total_cmp(&d(j))
        })
        .unwrap_or(7)
}

/// The RGB colour of ACI `i` (approximate: the standard 24 hues × 5 shades
/// × 2 saturations, and greys).
pub fn aci_rgb(i: u8) -> [u8; 3] {
    match i {
        1 => [255, 0, 0],
        2 => [255, 255, 0],
        3 => [0, 255, 0],
        4 => [0, 255, 255],
        5 => [0, 0, 255],
        6 => [255, 0, 255],
        0 | 7 => [255, 255, 255],
        8 => [128, 128, 128],
        9 => [192, 192, 192],
        10..=249 => {
            let hue = f64::from((i - 10) / 10) * 15.0;
            let shade = (i - 10) % 10;
            let value = [1.0, 0.65, 0.5, 0.3, 0.15][usize::from(shade / 2)];
            let saturation = if shade.is_multiple_of(2) { 1.0 } else { 0.5 };
            hsv(hue, saturation, value)
        }
        _ => {
            let v = [51, 80, 105, 130, 190, 255][usize::from(i - 250)];
            [v, v, v]
        }
    }
}

fn hsv(h: f64, s: f64, v: f64) -> [u8; 3] {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let byte = |t: f64| ((t + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    [byte(r), byte(g), byte(b)]
}

/// DXF text in R12's own codes (the file's code page is ANSI_1252):
/// ASCII as is (`%` as `%%%`), `°` `±` `⌀` as `%%d` `%%p` `%%c`, other
/// Windows-1252 characters as `%%nnn` (their code, three digits), others
/// `?`. (R12 knows no `\U+XXXX`.)
pub(crate) fn dxf_text(t: &str) -> String {
    let mut out = String::new();
    for c in t.chars() {
        match c {
            '%' => out.push_str("%%%"),
            ' '..='~' => out.push(c),
            '\n' | '\r' | '\t' => out.push(' '),
            '°' => out.push_str("%%d"),
            '±' => out.push_str("%%p"),
            '⌀' => out.push_str("%%c"),
            c => match crate::pdf::win_ansi(c) {
                b'?' => out.push('?'),
                b => {
                    let _ = write!(out, "%%{b:03}");
                }
            },
        }
    }
    out
}

struct Writer {
    out: String,
}

impl Writer {
    fn pair(&mut self, code: i32, value: impl std::fmt::Display) {
        let _ = write!(self.out, "{code:>3}\n{value}\n");
    }
    fn num(&mut self, code: i32, v: f64) {
        let v = if v.abs() < 5e-5 { 0.0 } else { v };
        self.pair(code, format!("{v:.4}"));
    }
    fn point(&mut self, code: i32, p: [f64; 2]) {
        self.num(code, p[0]);
        self.num(code + 10, p[1]);
        self.num(code + 20, 0.0);
    }
}

/// The drawing as R12 ASCII DXF.
pub fn write(d: &Drawing) -> String {
    write_counted(d).0
}

/// The drawing as R12 ASCII DXF, and how many raster images (PNG, JPEG)
/// were left out (R12 has no images; SVG images are written as their
/// outlines).
pub fn write_counted(d: &Drawing) -> (String, usize) {
    let sheet = d.space == Space::Sheet;
    let to_dxf = |p: [f64; 2]| if sheet { [p[0], d.height - p[1]] } else { p };
    // Line types: one per dash pattern, in order of first use.
    let mut dashes: Vec<Vec<f64>> = Vec::new();
    let mut pen_dash = |dash: &Option<Vec<f64>>| -> String {
        match dash {
            None => "CONTINUOUS".to_owned(),
            Some(pattern) => {
                let k = dashes.iter().position(|p| p == pattern).unwrap_or_else(|| {
                    dashes.push(pattern.clone());
                    dashes.len() - 1
                });
                format!("OHL_DASH{}", k + 1)
            }
        }
    };
    // Entities first (they name the line types used).
    let mut e = Writer { out: String::new() };
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    let mut see = |p: [f64; 2]| {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    };
    let mut images = 0;
    for (item, clip) in d.flat() {
        match item {
            Item::Path(p) => {
                let runs = match clip {
                    Some(r) => clip_polyline(
                        &if p.closed {
                            let mut q = p.points.clone();
                            q.extend(p.points.first().copied());
                            q
                        } else {
                            p.points.clone()
                        },
                        r,
                    ),
                    None => vec![p.points.clone()],
                };
                let closed = p.closed && clip.is_none();
                let ltype = pen_dash(&p.pen.dash);
                for run in runs.into_iter().filter(|r| r.len() >= 2) {
                    let pts: Vec<[f64; 2]> = run.into_iter().map(to_dxf).collect();
                    pts.iter().for_each(|q| see(*q));
                    if pts.len() == 2 && !closed {
                        e.pair(0, "LINE");
                        e.pair(8, p.layer.name());
                        e.pair(6, &ltype);
                        e.pair(62, aci(p.pen.color));
                        e.point(10, pts[0]);
                        e.point(11, pts[1]);
                    } else {
                        e.pair(0, "POLYLINE");
                        e.pair(8, p.layer.name());
                        e.pair(6, &ltype);
                        e.pair(62, aci(p.pen.color));
                        e.pair(66, 1);
                        e.point(10, [0.0, 0.0]);
                        e.pair(70, i32::from(closed));
                        for q in pts {
                            e.pair(0, "VERTEX");
                            e.pair(8, p.layer.name());
                            e.point(10, q);
                        }
                        e.pair(0, "SEQEND");
                        e.pair(8, p.layer.name());
                    }
                }
            }
            Item::Text(t) => {
                if clip.is_some_and(|r| !inside(t.at, r)) || t.text.trim().is_empty() {
                    continue;
                }
                let at = to_dxf(t.at);
                see(at);
                e.pair(0, "TEXT");
                e.pair(8, t.layer.name());
                e.pair(62, aci(t.color));
                e.point(10, at);
                e.num(40, t.size * 0.7);
                e.pair(1, dxf_text(&t.text));
                let justify = match t.anchor {
                    Anchor::Start => 0,
                    Anchor::Middle => 1,
                    Anchor::End => 2,
                };
                if justify != 0 {
                    e.pair(72, justify);
                    e.point(11, at);
                }
            }
            Item::Circle(c) => {
                if clip.is_some_and(|r| !inside(c.centre, r)) {
                    continue;
                }
                let at = to_dxf(c.centre);
                see(at);
                let color = c
                    .stroke
                    .as_ref()
                    .map(|p| p.color)
                    .or(c.fill)
                    .unwrap_or(Color::BLACK);
                e.pair(0, "CIRCLE");
                e.pair(8, c.layer.name());
                e.pair(62, aci(color));
                e.point(10, at);
                e.num(40, c.radius);
            }
            Item::Image(img) => {
                match crate::outline::svg_outlines(&img.href, img.at, img.width, img.height) {
                    Some(lines) => {
                        for (pts, closed, color) in lines {
                            let pts: Vec<[f64; 2]> = pts.into_iter().map(to_dxf).collect();
                            pts.iter().for_each(|q| see(*q));
                            e.pair(0, "POLYLINE");
                            e.pair(8, Layer::TitleImage.name());
                            e.pair(6, "CONTINUOUS");
                            e.pair(62, aci(color));
                            e.pair(66, 1);
                            e.point(10, [0.0, 0.0]);
                            e.pair(70, i32::from(closed));
                            for q in pts {
                                e.pair(0, "VERTEX");
                                e.pair(8, Layer::TitleImage.name());
                                e.point(10, q);
                            }
                            e.pair(0, "SEQEND");
                            e.pair(8, Layer::TitleImage.name());
                        }
                    }
                    None => images += 1,
                }
            }
            Item::Clip(_) => {}
        }
    }
    if sheet {
        see([0.0, 0.0]);
        see([d.width, d.height]);
    }
    if !lo[0].is_finite() {
        (lo, hi) = ([0.0; 2], [0.0; 2]);
    }

    let mut w = Writer { out: String::new() };
    w.pair(999, "OpenOHL drawing (DXF R12)");
    if images > 0 {
        w.pair(
            999,
            format!("{images} raster image(s) (logos) not written: R12 has no images"),
        );
    }
    w.pair(0, "SECTION");
    w.pair(2, "HEADER");
    w.pair(9, "$ACADVER");
    w.pair(1, "AC1009");
    w.pair(9, "$DWGCODEPAGE");
    w.pair(3, "ANSI_1252");
    w.pair(9, "$INSBASE");
    w.point(10, [0.0, 0.0]);
    w.pair(9, "$EXTMIN");
    w.point(10, lo);
    w.pair(9, "$EXTMAX");
    w.point(10, hi);
    w.pair(9, "$LIMMIN");
    w.num(10, lo[0]);
    w.num(20, lo[1]);
    w.pair(9, "$LIMMAX");
    w.num(10, hi[0]);
    w.num(20, hi[1]);
    w.pair(9, "$LTSCALE");
    w.num(40, d.line_scale);
    w.pair(9, "$MEASUREMENT");
    w.pair(70, 1);
    w.pair(9, "$INSUNITS");
    w.pair(70, if sheet { 4 } else { 6 });
    w.pair(0, "ENDSEC");

    w.pair(0, "SECTION");
    w.pair(2, "TABLES");
    w.pair(0, "TABLE");
    w.pair(2, "LTYPE");
    w.pair(70, dashes.len() + 1);
    w.pair(0, "LTYPE");
    w.pair(2, "CONTINUOUS");
    w.pair(70, 0);
    w.pair(3, "Solid line");
    w.pair(72, 65);
    w.pair(73, 0);
    w.num(40, 0.0);
    for (k, pattern) in dashes.iter().enumerate() {
        // Pairs of on, off; an odd pattern repeats (as in SVG).
        let mut elements = pattern.clone();
        if !elements.len().is_multiple_of(2) {
            elements.extend(pattern.iter().copied());
        }
        w.pair(0, "LTYPE");
        w.pair(2, format!("OHL_DASH{}", k + 1));
        w.pair(70, 0);
        let text: Vec<String> = pattern.iter().map(f64::to_string).collect();
        w.pair(3, format!("dashes {} mm", text.join(" ")));
        w.pair(72, 65);
        w.pair(73, elements.len());
        w.num(40, elements.iter().sum());
        for (j, v) in elements.iter().enumerate() {
            w.num(49, if j.is_multiple_of(2) { *v } else { -*v });
        }
    }
    w.pair(0, "ENDTAB");
    w.pair(0, "TABLE");
    w.pair(2, "LAYER");
    w.pair(70, Layer::ALL.len() + 1);
    w.pair(0, "LAYER");
    w.pair(2, "0");
    w.pair(70, 0);
    w.pair(62, 7);
    w.pair(6, "CONTINUOUS");
    for layer in Layer::ALL {
        w.pair(0, "LAYER");
        w.pair(2, layer.name());
        w.pair(70, 0);
        w.pair(62, aci(layer.color()));
        w.pair(6, "CONTINUOUS");
    }
    w.pair(0, "ENDTAB");
    w.pair(0, "TABLE");
    w.pair(2, "STYLE");
    w.pair(70, 1);
    w.pair(0, "STYLE");
    w.pair(2, "STANDARD");
    w.pair(70, 0);
    w.num(40, 0.0);
    w.num(41, 1.0);
    w.num(50, 0.0);
    w.pair(71, 0);
    w.num(42, 2.5);
    w.pair(3, "txt");
    w.pair(4, "");
    w.pair(0, "ENDTAB");
    w.pair(0, "ENDSEC");

    w.pair(0, "SECTION");
    w.pair(2, "BLOCKS");
    w.pair(0, "ENDSEC");
    w.pair(0, "SECTION");
    w.pair(2, "ENTITIES");
    w.out.push_str(&e.out);
    w.pair(0, "ENDSEC");
    w.pair(0, "EOF");
    (w.out, images)
}

/// A DXF file read back as group code / value pairs, for tests and
/// checks of written files.
pub fn read_pairs(text: &str) -> Result<Vec<(i32, String)>, String> {
    let lines: Vec<&str> = text.lines().collect();
    if !lines.len().is_multiple_of(2) {
        return Err("odd number of lines".into());
    }
    lines
        .chunks(2)
        .enumerate()
        .map(|(k, c)| {
            let code = c[0]
                .trim()
                .parse::<i32>()
                .map_err(|_| format!("line {}: group code \"{}\"", 2 * k + 1, c[0]))?;
            Ok((code, c[1].to_owned()))
        })
        .collect()
}

/// Entity counts by type in the ENTITIES section of a DXF file.
pub fn entity_counts(pairs: &[(i32, String)]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    let mut in_entities = false;
    let mut previous_was_section = false;
    for (code, value) in pairs {
        if *code == 2 && previous_was_section {
            in_entities = value == "ENTITIES";
        }
        previous_was_section = *code == 0 && value == "SECTION";
        if *code == 0 && value == "ENDSEC" {
            in_entities = false;
        }
        if in_entities && *code == 0 {
            *counts.entry(value.clone()).or_insert(0) += 1;
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Clip, Path, Pen};
    use crate::tests::main_route;
    use crate::{Options, page_format, plan_model, profile_model, sheets};

    /// The values of `code` in the entities of type `entity`, by entity.
    fn values(pairs: &[(i32, String)], entity: &str, code: i32) -> Vec<Vec<f64>> {
        let mut out: Vec<Vec<f64>> = Vec::new();
        let mut current: Option<Vec<f64>> = None;
        for (c, v) in pairs {
            if *c == 0 {
                if let Some(x) = current.take() {
                    out.push(x);
                }
                if v == entity {
                    current = Some(Vec::new());
                }
            } else if *c == code
                && let Some(x) = current.as_mut()
            {
                x.push(v.trim().parse().unwrap());
            }
        }
        out
    }

    #[test]
    fn aci_colours() {
        assert_eq!(aci(Color(255, 0, 0)), 1);
        assert_eq!(aci(Color(0, 0, 255)), 5);
        assert_eq!(aci(Color::BLACK), 7);
        assert_eq!(aci(Color(0x33, 0x33, 0x33)), 7);
        assert_eq!(aci(Color(0x80, 0x80, 0x80)), 8);
        assert_eq!(aci_rgb(10), [255, 0, 0]);
        assert_eq!(aci_rgb(11), [255, 128, 128]);
        assert_eq!(aci_rgb(250), [51, 51, 51]);
    }

    #[test]
    fn sheets_clip_and_flip() {
        let mut d = Drawing::sheet(100.0, 50.0);
        d.items.push(Item::Clip(Clip {
            rect: [10.0, 10.0, 20.0, 20.0],
            items: vec![Item::Path(Path {
                layer: Layer::Wires,
                points: vec![[0.0, 20.0], [40.0, 20.0]],
                closed: false,
                pen: Pen::new(Color(255, 0, 0), 0.3).dashed("3 1.5"),
                title: None,
            })],
        }));
        let text = write(&d);
        let pairs = read_pairs(&text).unwrap();
        let lines = values(&pairs, "LINE", 10);
        assert_eq!(lines, vec![vec![10.0]]);
        // y flipped: 50 − 20.
        assert_eq!(values(&pairs, "LINE", 21), vec![vec![30.0]]);
        assert!(text.contains("OHL_DASH1") && text.contains("AC1009"));
        assert!(pairs.last().is_some_and(|p| p.1 == "EOF"));
        assert_eq!(dxf_text("1+250 – T3 ×2"), "1+250 %%150 T3 %%2152");
        assert_eq!(dxf_text("5° ±1 % Ω"), "5%%d %%p1 %%% ?");
    }

    #[test]
    fn route_drawings_read_back() {
        let o = Options {
            clearance: Some(7.0),
            vertical_scale: 500.0,
            ..Options::default()
        };
        let d = main_route(&o);
        // A sheet in mm: inside the A3 sheet, every layer listed.
        let sheet = &sheets(&d, &o, &page_format("A3").unwrap())[0];
        let text = write(&sheet.drawing);
        let pairs = read_pairs(&text).unwrap();
        let counts = entity_counts(&pairs);
        assert!(counts["POLYLINE"] > 10 && counts["TEXT"] > 10, "{counts:?}");
        assert_eq!(counts["POLYLINE"], counts["SEQEND"]);
        for layer in Layer::ALL {
            assert!(
                pairs.iter().any(|(c, v)| *c == 2 && v == layer.name()),
                "{}",
                layer.name()
            );
        }
        for x in values(&pairs, "VERTEX", 10).into_iter().flatten() {
            assert!((0.0..=420.0).contains(&x));
        }

        // The plan in project coordinates: the structures at their places.
        let plan = write(&plan_model(&d, &o));
        let pairs = read_pairs(&plan).unwrap();
        let texts = values(&pairs, "TEXT", 10);
        for m in &d.structures {
            assert!(
                texts.iter().any(|t| (t[0] - (m.xy[0] + 3.0)).abs() < 1e-3),
                "{}",
                m.id
            );
        }
        assert!(plan.contains("$INSUNITS\n 70\n6"));

        // The profile by station and exaggerated height (×4 at 1:2000 / 1:500):
        // T2's mast stands at station 310.
        let profile = write(&profile_model(&d, &o));
        let pairs = read_pairs(&profile).unwrap();
        let t2 = &d.structures[1];
        let masts: Vec<Vec<f64>> = values(&pairs, "LINE", 10)
            .into_iter()
            .zip(values(&pairs, "LINE", 20))
            .map(|(x, y)| vec![x[0], y[0]])
            .collect();
        assert!(
            masts
                .iter()
                .any(|p| (p[0] - t2.station).abs() < 1e-3 && (p[1] - 4.0 * t2.base).abs() < 1e-2),
            "{masts:?}"
        );
    }
}
