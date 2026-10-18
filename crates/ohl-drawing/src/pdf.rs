// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Vector PDF of sheets: one page per [`Drawing`], at true size. Lines,
//! circles and clip regions are PDF paths; texts use the standard
//! Helvetica fonts (no font is embedded; characters outside Windows-1252
//! become `?`); logos are embedded as images.

use std::fmt::Write as _;

use crate::model::{Anchor, Color, Drawing, Item, Pen};

/// PDF points per mm.
const PT: f64 = 72.0 / 25.4;

/// Logo resolution, pixels per mm (≈ 300 dpi).
const LOGO_PX_PER_MM: f64 = 12.0;

/// Advance widths of Helvetica (per 1000 em) for characters 32–126.
const HELVETICA: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278,
    278, // ' '–'/'
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, // '0'–'9'
    278, 278, 584, 584, 584, 556, 1015, // ':'–'@'
    667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, // 'A'–'M'
    722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, // 'N'–'Z'
    278, 278, 278, 469, 556, 333, // '['–'`'
    556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, // 'a'–'m'
    556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, // 'n'–'z'
    334, 260, 334, 584, // '{'–'~'
];

/// The Windows-1252 byte of `c` (`?` when it has none).
fn win_ansi(c: char) -> u8 {
    match c {
        ' '..='~' => c as u8,
        '\u{a0}'..='\u{ff}' => c as u32 as u8,
        '€' => 0x80,
        '…' => 0x85,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        _ => b'?',
    }
}

/// Width of byte `b` in Helvetica, per 1000 em.
fn width(b: u8) -> f64 {
    f64::from(match b {
        32..=126 => HELVETICA[usize::from(b - 32)],
        0x96 => 556,
        0x97 => 1000,
        0xb7 => 278,
        0xd7 => 584,
        0xb0 => 400,
        _ => 556,
    })
}

/// Width of `text` at font size `size` (bold ≈ 6 % wider).
pub fn text_width(text: &str, size: f64, bold: bool) -> f64 {
    let w: f64 = text.chars().map(|c| width(win_ansi(c))).sum();
    w / 1000.0 * size * if bold { 1.06 } else { 1.0 }
}

fn rgb(c: Color) -> String {
    format!(
        "{:.3} {:.3} {:.3}",
        f64::from(c.0) / 255.0,
        f64::from(c.1) / 255.0,
        f64::from(c.2) / 255.0
    )
}

fn n(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_owned() }
}

/// An image to embed: size in pixels, RGB and alpha samples.
struct Raster {
    width: u32,
    height: u32,
    rgb: Vec<u8>,
    alpha: Vec<u8>,
}

/// A logo's box drawn from its data URI (`None`: it cannot be drawn).
fn logo_raster(href: &str, w_mm: f64, h_mm: f64) -> Option<Raster> {
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w_mm}mm" height="{h_mm}mm" viewBox="0 0 {w_mm} {h_mm}"><image width="{w_mm}" height="{h_mm}" href="{}" preserveAspectRatio="xMinYMid meet"/></svg>"#,
        crate::model::escape(href)
    );
    let (pixmap, _) = crate::png::rasterize(&svg, LOGO_PX_PER_MM, 4.0e6).ok()?;
    let mut rgb = Vec::with_capacity(pixmap.data().len() / 4 * 3);
    let mut alpha = Vec::with_capacity(pixmap.data().len() / 4);
    for p in pixmap.pixels() {
        let c = p.demultiply();
        rgb.extend([c.red(), c.green(), c.blue()]);
        alpha.push(c.alpha());
    }
    alpha.iter().any(|a| *a > 0).then(|| Raster {
        width: pixmap.width(),
        height: pixmap.height(),
        rgb,
        alpha,
    })
}

/// A page's content stream and its images.
struct Content {
    ops: Vec<u8>,
    images: Vec<Raster>,
}

impl Content {
    fn op(&mut self, text: &str) {
        self.ops.extend_from_slice(text.as_bytes());
        self.ops.push(b'\n');
    }

    fn pen(&mut self, pen: &Pen) {
        let dash = pen.dash.as_ref().map_or(String::new(), |d| {
            d.iter().map(|v| n(v * PT)).collect::<Vec<_>>().join(" ")
        });
        self.op(&format!(
            "{} RG {} w [{dash}] 0 d",
            rgb(pen.color),
            n(pen.width * PT)
        ));
    }
}

/// Bytes of a PDF string literal of `text` (Windows-1252).
fn literal(text: &str) -> Vec<u8> {
    let mut out = vec![b'('];
    for c in text.chars() {
        match win_ansi(c) {
            b @ (b'(' | b')' | b'\\') => out.extend([b'\\', b]),
            b => out.push(b),
        }
    }
    out.push(b')');
    out
}

fn page_content(d: &Drawing) -> Content {
    let height = d.height;
    let p = |q: [f64; 2]| format!("{} {}", n(q[0] * PT), n((height - q[1]) * PT));
    let mut c = Content {
        ops: Vec::new(),
        images: Vec::new(),
    };
    fn item(c: &mut Content, it: &Item, p: &dyn Fn([f64; 2]) -> String, height: f64) {
        match it {
            Item::Path(path) => {
                if path.points.len() < 2 {
                    return;
                }
                c.pen(&path.pen);
                let mut ops = String::new();
                for (k, q) in path.points.iter().enumerate() {
                    let _ = write!(ops, "{} {} ", p(*q), if k == 0 { "m" } else { "l" });
                }
                ops.push_str(if path.closed { "h S" } else { "S" });
                c.op(&ops);
            }
            Item::Circle(circle) => {
                let (x, y, r) = (circle.centre[0], circle.centre[1], circle.radius);
                let k = 0.552_284_75 * r;
                let mut ops = format!("{} m ", p([x + r, y]));
                for (c1, c2, end) in [
                    ([x + r, y + k], [x + k, y + r], [x, y + r]),
                    ([x - k, y + r], [x - r, y + k], [x - r, y]),
                    ([x - r, y - k], [x - k, y - r], [x, y - r]),
                    ([x + k, y - r], [x + r, y - k], [x + r, y]),
                ] {
                    let _ = write!(ops, "{} {} {} c ", p(c1), p(c2), p(end));
                }
                if let Some(pen) = &circle.stroke {
                    c.pen(pen);
                }
                if let Some(fill) = circle.fill {
                    c.op(&format!("{} rg", rgb(fill)));
                }
                ops.push_str(match (&circle.stroke, circle.fill) {
                    (Some(_), Some(_)) => "b",
                    (None, Some(_)) => "h f",
                    (Some(_), None) => "h S",
                    (None, None) => "n",
                });
                c.op(&ops);
            }
            Item::Text(t) => {
                let size = t.size * PT;
                let w = text_width(&t.text, size, t.bold);
                let shift = match t.anchor {
                    Anchor::Start => 0.0,
                    Anchor::Middle => w / 2.0,
                    Anchor::End => w,
                };
                let x = t.at[0] * PT - shift;
                let y = (height - t.at[1]) * PT;
                c.op(&format!(
                    "{} rg BT /{} {} Tf 1 0 0 1 {} {} Tm",
                    rgb(t.color),
                    if t.bold { "F2" } else { "F1" },
                    n(size),
                    n(x),
                    n(y)
                ));
                c.ops.extend(literal(&t.text));
                c.op(" Tj ET");
            }
            Item::Image(img) => {
                if let Some(r) = logo_raster(&img.href, img.width, img.height) {
                    c.images.push(r);
                    c.op(&format!(
                        "q {} 0 0 {} {} {} cm /Im{} Do Q",
                        n(img.width * PT),
                        n(img.height * PT),
                        n(img.at[0] * PT),
                        n((height - img.at[1] - img.height) * PT),
                        c.images.len()
                    ));
                }
            }
            Item::Clip(clip) => {
                let [x, y, w, h] = clip.rect;
                c.op(&format!(
                    "q {} {} {} {} re W n",
                    n(x * PT),
                    n((height - y - h) * PT),
                    n(w * PT),
                    n(h * PT)
                ));
                for i in &clip.items {
                    item(c, i, p, height);
                }
                c.op("Q");
            }
        }
    }
    c.op("1 J 1 j");
    for it in &d.items {
        item(&mut c, it, &p, height);
    }
    c
}

/// A PDF file of `pages` (sheets), each at its own size; `title` goes into
/// the document information.
pub fn write(pages: &[&Drawing], title: &str) -> Vec<u8> {
    let mut objects: Vec<Vec<u8>> = Vec::new();
    // 1 catalog, 2 page tree, 3–4 fonts, 5 information; then per page.
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    objects.push(Vec::new()); // the page tree, once the pages are known
    for font in ["Helvetica", "Helvetica-Bold"] {
        objects.push(
            format!(
                "<< /Type /Font /Subtype /Type1 /BaseFont /{font} /Encoding /WinAnsiEncoding >>"
            )
            .into_bytes(),
        );
    }
    let mut info = b"<< /Producer (OpenOHL) /Title ".to_vec();
    info.extend(literal(title));
    info.extend(b" >>");
    objects.push(info);
    let stream = |dict: &str, data: &[u8]| {
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(data, 6);
        let mut o = format!(
            "<< {dict} /Filter /FlateDecode /Length {} >>\nstream\n",
            packed.len()
        )
        .into_bytes();
        o.extend(packed);
        o.extend(b"\nendstream");
        o
    };
    let mut kids = Vec::new();
    for d in pages {
        let content = page_content(d);
        let page_id = objects.len() + 1;
        let content_id = page_id + 1;
        let mut xobjects = String::new();
        let mut image_objects = Vec::new();
        let mut next = content_id + 1;
        for (k, r) in content.images.iter().enumerate() {
            let (image_id, mask_id) = (next, next + 1);
            next += 2;
            let _ = write!(xobjects, "/Im{} {image_id} 0 R ", k + 1);
            image_objects.push(stream(
                &format!(
                    "/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /SMask {mask_id} 0 R",
                    r.width, r.height
                ),
                &r.rgb,
            ));
            image_objects.push(stream(
                &format!(
                    "/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceGray /BitsPerComponent 8",
                    r.width, r.height
                ),
                &r.alpha,
            ));
        }
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> /XObject << {xobjects}>> >> /Contents {content_id} 0 R >>",
                n(d.width * PT),
                n(d.height * PT)
            )
            .into_bytes(),
        );
        objects.push(stream("", &content.ops));
        objects.extend(image_objects);
        kids.push(format!("{page_id} 0 R"));
    }
    objects[1] = format!(
        "<< /Type /Pages /Kids [{}] /Count {} >>",
        kids.join(" "),
        kids.len()
    )
    .into_bytes();

    let mut out = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for (k, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", k + 1).into_bytes());
        out.extend(o);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    let mut table = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for o in offsets {
        let _ = writeln!(table, "{o:010} 00000 n ");
    }
    let _ = write!(
        table,
        "trailer\n<< /Size {} /Root 1 0 R /Info 5 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    );
    out.extend(table.into_bytes());
    out
}

/// What a PDF file holds, read back from its structure (for tests and
/// checks of written files): the page sizes in points, once every object
/// of the cross-reference table is found where it says and every stream
/// has its length and unpacks.
pub fn read_back(pdf: &[u8]) -> Result<Vec<[f64; 2]>, String> {
    // Bytes above 127 (in streams) as one character each: offsets stay.
    let text: String = pdf
        .iter()
        .map(|&b| if b < 128 { b as char } else { '.' })
        .collect();
    let start = text.rfind("startxref").ok_or("no startxref")?;
    let xref: usize = text[start + 9..]
        .split_whitespace()
        .next()
        .and_then(|t| t.parse().ok())
        .ok_or("bad startxref")?;
    if !text[xref..].starts_with("xref") {
        return Err("startxref does not point at the xref table".into());
    }
    let mut lines = text[xref..].lines().skip(1);
    let count: usize = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|t| t.parse().ok())
        .ok_or("bad xref header")?;
    for (k, line) in lines.skip(1).take(count.saturating_sub(1)).enumerate() {
        let offset: usize = line[..10].parse().map_err(|_| "bad xref entry")?;
        if !text
            .get(offset..)
            .unwrap_or("")
            .starts_with(&format!("{} 0 obj", k + 1))
        {
            return Err(format!("object {} is not at {offset}", k + 1));
        }
    }
    for part in text.match_indices("/Length ").map(|(i, _)| i) {
        let length: usize = text[part + 8..]
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .and_then(|t| t.parse().ok())
            .ok_or("bad stream length")?;
        let data = text[part..].find("stream\n").ok_or("no stream")? + part + 7;
        if !text
            .get(data + length..)
            .is_some_and(|t| t.starts_with("\nendstream"))
        {
            return Err(format!("stream at {data} is not {length} bytes long"));
        }
        miniz_oxide::inflate::decompress_to_vec_zlib(&pdf[data..data + length])
            .map_err(|e| format!("stream at {data}: {e:?}"))?;
    }
    let mut pages = Vec::new();
    for part in text.split("/MediaBox [0 0 ").skip(1) {
        let v: Vec<f64> = part
            .split(']')
            .next()
            .unwrap_or("")
            .split_whitespace()
            .filter_map(|t| t.parse().ok())
            .collect();
        if v.len() == 2 {
            pages.push([v[0], v[1]]);
        }
    }
    Ok(pages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Clip, Layer, Text};
    use crate::tests::main_route;
    use crate::{Logo, Options, page_format, sheets};

    #[test]
    fn texts_measure_and_encode() {
        assert!((text_width("Hi", 10.0, false) - (7.22 + 2.22)).abs() < 1e-9);
        assert_eq!(win_ansi('–'), 0x96);
        assert_eq!(win_ansi('×'), 0xd7);
        assert_eq!(win_ansi('Ω'), b'?');
        assert_eq!(literal("a(b)\\"), b"(a\\(b\\)\\\\)".to_vec());
    }

    #[test]
    fn pages_read_back() {
        let mut d = Drawing::sheet(297.0, 210.0);
        d.items.push(Item::Clip(Clip {
            rect: [10.0, 10.0, 100.0, 50.0],
            items: vec![Item::Text(Text {
                layer: Layer::Title,
                at: [20.0, 20.0],
                text: "Sheet 1 / 2".into(),
                size: 4.5,
                anchor: Anchor::End,
                bold: true,
                color: Color::BLACK,
            })],
        }));
        let pdf = write(&[&d, &d], "Test");
        assert!(pdf.starts_with(b"%PDF-1.4"));
        let pages = read_back(&pdf).unwrap();
        assert_eq!(pages.len(), 2);
        assert!((pages[0][0] - 841.89).abs() < 0.01 && (pages[0][1] - 595.276).abs() < 0.01);
    }

    #[test]
    fn route_sheets_with_logos() {
        // A 2 × 1 px red PNG as the logo.
        let mut pixmap = resvg::tiny_skia::Pixmap::new(2, 1).unwrap();
        pixmap.fill(resvg::tiny_skia::Color::from_rgba8(255, 0, 0, 255));
        let png = crate::base64(&pixmap.encode_png().unwrap());
        let o = Options {
            vertical_scale: 500.0,
            logos: vec![Logo {
                href: format!("data:image/png;base64,{png}"),
                width: 20.0,
            }],
            ..Options::default()
        };
        let d = main_route(&o);
        let s = sheets(&d, &o, &page_format("A3").unwrap());
        let pdf = write(&s.iter().map(|s| &s.drawing).collect::<Vec<_>>(), "main");
        assert_eq!(read_back(&pdf).unwrap().len(), 2);
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/Subtype /Image") && text.contains("/SMask"));
        assert!(text.contains("/BaseFont /Helvetica-Bold"));
    }
}
