// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The drawing model shared by every output format: paths, texts, circles
//! and images on layers, with pens (colour, width, dash) and clip regions.
//!
//! A [`Drawing`] is either a **sheet** (mm, y down from the top left
//! corner, like SVG) or **model space** (project units, y up). SVG, PDF and
//! PNG are written from sheets; DXF from both.

use std::fmt::Write as _;

/// A colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8);

impl Color {
    /// Black.
    pub const BLACK: Self = Self(0, 0, 0);

    /// From `#rrggbb` or `#rgb`.
    pub fn parse(text: &str) -> Option<Self> {
        let h = text.trim().strip_prefix('#')?;
        let digit = |k: usize, n: usize| u8::from_str_radix(h.get(k..k + n)?, 16).ok();
        match h.len() {
            6 => Some(Self(digit(0, 2)?, digit(2, 2)?, digit(4, 2)?)),
            3 => Some(Self(
                digit(0, 1)? * 17,
                digit(1, 1)? * 17,
                digit(2, 1)? * 17,
            )),
            _ => None,
        }
    }

    /// From `#rrggbb` or `#rgb`, black when it is neither.
    pub fn hex_or_black(text: &str) -> Self {
        Self::parse(text).unwrap_or(Self::BLACK)
    }

    /// As `#rrggbb`.
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}

/// What a drawn item is: its layer in DXF, its group elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Layer {
    /// Sheet border and title block lines.
    Frame,
    /// Title block and heading texts.
    Title,
    /// Logos.
    Logo,
    /// Station and height grid lines.
    Grid,
    /// Grid labels and panel names.
    Labels,
    /// Plan and profile panel borders.
    Panels,
    /// Route centreline in the plan.
    Centreline,
    /// Ground along the centreline.
    Ground,
    /// Side ground profiles.
    GroundSide,
    /// Required-clearance line above the ground.
    Clearance,
    /// Structures: symbols, stick drawings, names, stations.
    Structures,
    /// Wires and jumpers.
    Wires,
    /// Violation markers.
    Markers,
    /// Match lines at the sheet's ends.
    Match,
}

impl Layer {
    /// Every layer.
    pub const ALL: [Self; 14] = [
        Self::Frame,
        Self::Title,
        Self::Logo,
        Self::Grid,
        Self::Labels,
        Self::Panels,
        Self::Centreline,
        Self::Ground,
        Self::GroundSide,
        Self::Clearance,
        Self::Structures,
        Self::Wires,
        Self::Markers,
        Self::Match,
    ];

    /// Layer name in DXF.
    pub fn name(self) -> &'static str {
        match self {
            Self::Frame => "OHL_FRAME",
            Self::Title => "OHL_TITLE",
            Self::Logo => "OHL_LOGO",
            Self::Grid => "OHL_GRID",
            Self::Labels => "OHL_LABELS",
            Self::Panels => "OHL_PANELS",
            Self::Centreline => "OHL_CENTRELINE",
            Self::Ground => "OHL_GROUND",
            Self::GroundSide => "OHL_GROUND_SIDE",
            Self::Clearance => "OHL_CLEARANCE",
            Self::Structures => "OHL_STRUCTURES",
            Self::Wires => "OHL_WIRES",
            Self::Markers => "OHL_MARKERS",
            Self::Match => "OHL_MATCH",
        }
    }

    /// The layer's own colour (items carry their own colour too).
    pub fn color(self) -> Color {
        match self {
            Self::Frame | Self::Title | Self::Logo | Self::Labels | Self::Structures => {
                Color::BLACK
            }
            Self::Grid => Color(0xdd, 0xdd, 0xdd),
            Self::Panels => Color(0x88, 0x88, 0x88),
            Self::Centreline => Color(0x99, 0x99, 0x99),
            Self::Ground => Color(0x7a, 0x4a, 0x1a),
            Self::GroundSide => Color(0xb0, 0x88, 0x50),
            Self::Clearance => Color(0xdd, 0x22, 0x22),
            Self::Wires => Color(0x00, 0x66, 0xcc),
            Self::Markers => Color(0xdd, 0x00, 0x00),
            Self::Match => Color(0x00, 0x66, 0xcc),
        }
    }
}

/// How a line is drawn. Width and dashes are in mm on the sheet (also in
/// model space, where [`Drawing::line_scale`] turns them into model units).
#[derive(Debug, Clone, PartialEq)]
pub struct Pen {
    /// Colour.
    pub color: Color,
    /// Width, mm.
    pub width: f64,
    /// Dash pattern (on, off, …), mm; `None`: solid.
    pub dash: Option<Vec<f64>>,
}

impl Pen {
    /// A solid pen.
    pub fn new(color: Color, width: f64) -> Self {
        Self {
            color,
            width,
            dash: None,
        }
    }

    /// The pen dashed with `pattern` (SVG `stroke-dasharray` text, e.g.
    /// `"3 1.5"`; empty or invalid: solid).
    pub fn dashed(mut self, pattern: &str) -> Self {
        let dash: Vec<f64> = pattern
            .split([' ', ','])
            .filter(|t| !t.is_empty())
            .filter_map(|t| t.parse().ok())
            .filter(|v: &f64| v.is_finite() && *v >= 0.0)
            .collect();
        self.dash = (!dash.is_empty() && dash.iter().any(|v| *v > 0.0)).then_some(dash);
        self
    }
}

/// A polyline.
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    /// Layer.
    pub layer: Layer,
    /// Points.
    pub points: Vec<[f64; 2]>,
    /// Joined back to the first point.
    pub closed: bool,
    /// Pen.
    pub pen: Pen,
    /// What it is (a tooltip in SVG).
    pub title: Option<String>,
}

/// Where a text sits on its point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// The point is the text's start.
    Start,
    /// The point is the text's middle.
    Middle,
    /// The point is the text's end.
    End,
}

/// A line of text; its point is on the baseline.
#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    /// Layer.
    pub layer: Layer,
    /// Point.
    pub at: [f64; 2],
    /// The text.
    pub text: String,
    /// Font size (em), mm.
    pub size: f64,
    /// Where the point is along the text.
    pub anchor: Anchor,
    /// Bold.
    pub bold: bool,
    /// Colour.
    pub color: Color,
}

/// A circle.
#[derive(Debug, Clone, PartialEq)]
pub struct Circle {
    /// Layer.
    pub layer: Layer,
    /// Centre.
    pub centre: [f64; 2],
    /// Radius.
    pub radius: f64,
    /// Outline; `None`: none.
    pub stroke: Option<Pen>,
    /// Fill; `None`: none.
    pub fill: Option<Color>,
    /// What it is (a tooltip in SVG).
    pub title: Option<String>,
}

/// An image (a logo), fitted into its box keeping its proportions, at the
/// box's left.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    /// Layer.
    pub layer: Layer,
    /// Top left corner.
    pub at: [f64; 2],
    /// Box width.
    pub width: f64,
    /// Box height.
    pub height: f64,
    /// The image as a data URI (SVG, PNG or JPEG).
    pub href: String,
}

/// Items drawn only inside a rectangle.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    /// Rectangle: left, top, width, height.
    pub rect: [f64; 4],
    /// Items.
    pub items: Vec<Item>,
}

/// One thing drawn.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// A polyline.
    Path(Path),
    /// A text.
    Text(Text),
    /// A circle.
    Circle(Circle),
    /// An image.
    Image(Image),
    /// Items clipped to a rectangle.
    Clip(Clip),
}

/// Units and axes of a drawing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Space {
    /// A sheet: mm, y down from the top left corner.
    Sheet,
    /// Model space: metres, y up (project coordinates, or station and
    /// exaggerated height).
    Model,
}

/// A drawing in one format-independent form.
#[derive(Debug, Clone, PartialEq)]
pub struct Drawing {
    /// Sheet or model space.
    pub space: Space,
    /// Sheet width, mm (model space: extent, m).
    pub width: f64,
    /// Sheet height, mm (model space: extent, m).
    pub height: f64,
    /// Model units per mm of pen widths, dashes and text sizes (1 on a
    /// sheet; in model space the scale denominator / 1000).
    pub line_scale: f64,
    /// Items, drawn in order.
    pub items: Vec<Item>,
}

impl Drawing {
    /// An empty sheet of `width` × `height` mm.
    pub fn sheet(width: f64, height: f64) -> Self {
        Self {
            space: Space::Sheet,
            width,
            height,
            line_scale: 1.0,
            items: Vec::new(),
        }
    }

    /// Adds a polyline.
    pub fn path(&mut self, layer: Layer, points: Vec<[f64; 2]>, pen: Pen) -> &mut Path {
        self.items.push(Item::Path(Path {
            layer,
            points,
            closed: false,
            pen,
            title: None,
        }));
        match self.items.last_mut() {
            Some(Item::Path(p)) => p,
            _ => unreachable!("just pushed"),
        }
    }

    /// Every item, also those inside clip regions, with its clip rectangle.
    pub fn flat(&self) -> Vec<(&Item, Option<[f64; 4]>)> {
        let mut out = Vec::new();
        for item in &self.items {
            match item {
                Item::Clip(c) => out.extend(c.items.iter().map(|i| (i, Some(c.rect)))),
                other => out.push((other, None)),
            }
        }
        out
    }

    /// This sheet scaled into model space: a sheet point (x, y) goes to
    /// (`origin[0]` + x·`scale`, `origin[1]` − y·`scale`); pens and texts
    /// follow.
    pub fn to_model(&self, scale: f64, origin: [f64; 2]) -> Self {
        let p = |q: [f64; 2]| [origin[0] + q[0] * scale, origin[1] - q[1] * scale];
        fn map(item: &Item, p: &dyn Fn([f64; 2]) -> [f64; 2], scale: f64) -> Item {
            match item {
                Item::Path(x) => Item::Path(Path {
                    points: x.points.iter().map(|q| p(*q)).collect(),
                    ..x.clone()
                }),
                Item::Text(x) => Item::Text(Text {
                    at: p(x.at),
                    size: x.size * scale,
                    ..x.clone()
                }),
                Item::Circle(x) => Item::Circle(Circle {
                    centre: p(x.centre),
                    radius: x.radius * scale,
                    ..x.clone()
                }),
                Item::Image(x) => {
                    let a = p(x.at);
                    Item::Image(Image {
                        at: a,
                        width: x.width * scale,
                        height: x.height * scale,
                        ..x.clone()
                    })
                }
                Item::Clip(c) => {
                    let a = p([c.rect[0], c.rect[1] + c.rect[3]]);
                    Item::Clip(Clip {
                        rect: [a[0], a[1], c.rect[2] * scale, c.rect[3] * scale],
                        items: c.items.iter().map(|i| map(i, p, scale)).collect(),
                    })
                }
            }
        }
        Self {
            space: Space::Model,
            width: self.width * scale,
            height: self.height * scale,
            line_scale: scale,
            items: self.items.iter().map(|i| map(i, &p, scale)).collect(),
        }
    }

    /// The drawing as SVG (a sheet; units mm).
    pub fn svg(&self) -> String {
        let mut out = String::new();
        let (w, h) = (self.width, self.height);
        let _ = writeln!(
            out,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.1}mm" height="{h:.1}mm" viewBox="0 0 {w:.1} {h:.1}" font-family="Helvetica, Arial, sans-serif">"#
        );
        let _ = writeln!(out, r#"<rect width="100%" height="100%" fill="white"/>"#);
        let mut clips = 0;
        for item in &self.items {
            svg_item(&mut out, item, &mut clips);
        }
        out.push_str("</svg>\n");
        out
    }
}

/// `&`, `<`, `>` and `"` escaped for XML.
pub(crate) fn escape(t: &str) -> String {
    t.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn svg_stroke(pen: &Pen) -> String {
    let mut s = format!(
        r#" stroke="{}" stroke-width="{}""#,
        pen.color.hex(),
        pen.width
    );
    if let Some(d) = &pen.dash {
        let d: Vec<String> = d.iter().map(f64::to_string).collect();
        let _ = write!(s, r#" stroke-dasharray="{}""#, d.join(" "));
    }
    s
}

fn svg_title(title: Option<&String>) -> String {
    title.map_or(String::new(), |t| format!("<title>{}</title>", escape(t)))
}

fn svg_item(out: &mut String, item: &Item, clips: &mut usize) {
    match item {
        Item::Path(p) => {
            let title = svg_title(p.title.as_ref());
            let close = if title.is_empty() {
                "/>".to_owned()
            } else {
                format!(
                    ">{title}</{}>",
                    if p.points.len() == 2 && !p.closed {
                        "line"
                    } else {
                        "path"
                    }
                )
            };
            if p.points.len() == 2 && !p.closed {
                let _ = writeln!(
                    out,
                    r#"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}"{}{close}"#,
                    p.points[0][0],
                    p.points[0][1],
                    p.points[1][0],
                    p.points[1][1],
                    svg_stroke(&p.pen)
                );
            } else {
                let mut d = String::new();
                for (k, q) in p.points.iter().enumerate() {
                    let _ = write!(
                        d,
                        "{}{:.2},{:.2} ",
                        if k == 0 { "M" } else { "L" },
                        q[0],
                        q[1]
                    );
                }
                if p.closed {
                    d.push('Z');
                }
                let _ = writeln!(
                    out,
                    r#"<path d="{}" fill="none"{}{close}"#,
                    d.trim_end(),
                    svg_stroke(&p.pen)
                );
            }
        }
        Item::Text(t) => {
            let mut attrs = format!(r#" font-size="{}""#, t.size);
            match t.anchor {
                Anchor::Start => {}
                Anchor::Middle => attrs.push_str(r#" text-anchor="middle""#),
                Anchor::End => attrs.push_str(r#" text-anchor="end""#),
            }
            if t.bold {
                attrs.push_str(r#" font-weight="bold""#);
            }
            if t.color != Color::BLACK {
                let _ = write!(attrs, r#" fill="{}""#, t.color.hex());
            }
            let _ = writeln!(
                out,
                r#"<text x="{:.2}" y="{:.2}"{attrs}>{}</text>"#,
                t.at[0],
                t.at[1],
                escape(&t.text)
            );
        }
        Item::Circle(c) => {
            let fill = c.fill.map_or("none".to_owned(), Color::hex);
            let stroke = c.stroke.as_ref().map_or(String::new(), svg_stroke);
            let title = svg_title(c.title.as_ref());
            let _ = writeln!(
                out,
                r#"<circle cx="{:.2}" cy="{:.2}" r="{}" fill="{fill}"{stroke}{}"#,
                c.centre[0],
                c.centre[1],
                c.radius,
                if title.is_empty() {
                    "/>".to_owned()
                } else {
                    format!(">{title}</circle>")
                }
            );
        }
        Item::Image(i) => {
            let _ = writeln!(
                out,
                r#"<image x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" href="{}" preserveAspectRatio="xMinYMid meet"/>"#,
                i.at[0],
                i.at[1],
                i.width,
                i.height,
                escape(&i.href)
            );
        }
        Item::Clip(c) => {
            *clips += 1;
            let id = format!("clip{clips}");
            let _ = writeln!(
                out,
                r#"<defs><clipPath id="{id}"><rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}"/></clipPath></defs>"#,
                c.rect[0], c.rect[1], c.rect[2], c.rect[3]
            );
            let _ = writeln!(out, r#"<g clip-path="url(#{id})">"#);
            for i in &c.items {
                svg_item(out, i, clips);
            }
            out.push_str("</g>\n");
        }
    }
}

/// The parts of a polyline inside rectangle `rect` (left, top, width,
/// height; Liang–Barsky per segment, joined where they meet).
pub fn clip_polyline(points: &[[f64; 2]], rect: [f64; 4]) -> Vec<Vec<[f64; 2]>> {
    let (x0, y0, x1, y1) = (rect[0], rect[1], rect[0] + rect[2], rect[1] + rect[3]);
    let mut runs: Vec<Vec<[f64; 2]>> = Vec::new();
    let mut open = false;
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let (mut t0, mut t1) = (0.0f64, 1.0f64);
        let mut inside = true;
        for (p, q) in [
            (-dx, a[0] - x0),
            (dx, x1 - a[0]),
            (-dy, a[1] - y0),
            (dy, y1 - a[1]),
        ] {
            if p == 0.0 {
                if q < 0.0 {
                    inside = false;
                    break;
                }
            } else {
                let r = q / p;
                if p < 0.0 {
                    t0 = t0.max(r);
                } else {
                    t1 = t1.min(r);
                }
                if t0 > t1 {
                    inside = false;
                    break;
                }
            }
        }
        if !inside {
            open = false;
            continue;
        }
        let pa = [a[0] + t0 * dx, a[1] + t0 * dy];
        let pb = [a[0] + t1 * dx, a[1] + t1 * dy];
        match runs.last_mut() {
            Some(run) if open && t0 == 0.0 => run.push(pb),
            _ => runs.push(vec![pa, pb]),
        }
        open = t1 == 1.0;
    }
    runs
}

/// Whether point `p` lies inside rectangle `rect` (left, top, width,
/// height).
pub fn inside(p: [f64; 2], rect: [f64; 4]) -> bool {
    p[0] >= rect[0] && p[0] <= rect[0] + rect[2] && p[1] >= rect[1] && p[1] <= rect[1] + rect[3]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_parse() {
        assert_eq!(Color::parse("#ddd"), Some(Color(0xdd, 0xdd, 0xdd)));
        assert_eq!(Color::parse("#7a4a1a"), Some(Color(0x7a, 0x4a, 0x1a)));
        assert_eq!(Color::parse("red"), None);
        assert_eq!(Color(1, 2, 255).hex(), "#0102ff");
    }

    #[test]
    fn polylines_clip_to_rectangles() {
        let r = [0.0, 0.0, 10.0, 10.0];
        // Through the box and out and back in: two runs.
        let runs = clip_polyline(&[[-5.0, 5.0], [5.0, 5.0], [5.0, 15.0], [8.0, 5.0]], r);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0], vec![[0.0, 5.0], [5.0, 5.0], [5.0, 10.0]]);
        assert!((runs[1][0][1] - 10.0).abs() < 1e-12 && runs[1][1] == [8.0, 5.0]);
        assert!(clip_polyline(&[[20.0, 0.0], [30.0, 5.0]], r).is_empty());
        assert!(inside([10.0, 0.0], r) && !inside([10.1, 0.0], r));
    }

    #[test]
    fn dashes_parse() {
        let p = Pen::new(Color::BLACK, 0.3).dashed("4 1 1 1");
        assert_eq!(p.dash, Some(vec![4.0, 1.0, 1.0, 1.0]));
        assert_eq!(Pen::new(Color::BLACK, 0.3).dashed("").dash, None);
    }

    #[test]
    fn sheets_scale_into_model_space() {
        let mut d = Drawing::sheet(100.0, 50.0);
        d.path(
            Layer::Ground,
            vec![[10.0, 40.0], [20.0, 30.0]],
            Pen::new(Color::BLACK, 0.3),
        );
        let m = d.to_model(2.0, [-20.0, 100.0]);
        let Item::Path(p) = &m.items[0] else {
            panic!("a path")
        };
        assert_eq!(p.points, vec![[0.0, 20.0], [20.0, 40.0]]);
        assert_eq!(m.line_scale, 2.0);
        assert!(
            d.svg()
                .contains(r#"<line x1="10.00" y1="40.00" x2="20.00" y2="30.00""#)
        );
    }
}
