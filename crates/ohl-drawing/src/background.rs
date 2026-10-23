// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The plan's backgrounds: the project's drawings (DXF) and orthophotos
//! under the plan of the sheets, straightened along the route like
//! everything else drawn there (a point at station `s` and offset `o`,
//! [`ohl_line::route::project`], is drawn at `s` along the sheet, `o`
//! across it). Display only: nothing computed depends on them.
//!
//! - **Drawings**: their lines (and, on request, points and texts) within
//!   [`crate::Options::background_half_width`] of the axis, each segment
//!   cut into pieces of at most [`DENSIFY_M`] before it is straightened
//!   (a long straight line beside a bend follows the bend on paper), cut
//!   where it leaves that width, and on each sheet clipped to the plan
//!   panel; layer `OHL_BACKGROUND_DRAWINGS`, under everything else.
//! - **Orthophotos**: resampled into each sheet's plan panel, pixel by
//!   pixel: each pixel's (station, offset) is taken back to the ground
//!   ([`ohl_line::route::point_at`], per column of pixels the centreline's
//!   segment at that station and its normal), and the photo read there
//!   ([`PhotoSource`]); lightened by a white veil, embedded as one JPEG
//!   (layer `OHL_BACKGROUND_PHOTO`; SVG, PDF, PNG — DXF R12 has no
//!   images).
//!
//! Where the route bends, the plan is straightened segment by segment
//! like the wires and the structures: on the outside of a bend a wedge of
//! ground is not shown, on the inside a wedge is shown twice (the wider
//! the panel and the sharper the bend, the larger).

use std::sync::Arc;

use ohl_line::Point;
use ohl_line::route::{point_at, project};

use crate::layout::Frame;
use crate::model::{Anchor, Circle, Color, Image, Item, Layer, Path, Pen, Text, clip_polyline};
use crate::style::Sample;
use crate::{Options, RouteDrawing};

/// Longest piece of a drawing's line straightened as one straight piece,
/// m (on the ground).
pub const DENSIFY_M: f64 = 2.0;

/// Pixels of a sheet's orthophoto at most (a lower resolution beyond).
pub const MAX_PHOTO_PIXELS: f64 = 40.0e6;

/// Orthophotos read at points: the colour at each point (in the
/// project's coordinate system) for pixels of `metres_per_pixel` on the
/// ground; none where there is no photo.
pub trait PhotoSource: Send + Sync {
    /// Fills `out` (as long as `points`) with the colour at each point.
    fn sample(&self, points: &[[f64; 2]], metres_per_pixel: f64, out: &mut [Option<[u8; 3]>]);
}

impl PhotoSource for ohl_background::Mosaic {
    fn sample(&self, points: &[[f64; 2]], metres_per_pixel: f64, out: &mut [Option<[u8; 3]>]) {
        ohl_background::Mosaic::sample(self, points, metres_per_pixel, out);
    }
}

/// The orthophotos drawn under the plan: their name (the image layers,
/// for the legend) and where they are read.
#[derive(Clone)]
pub struct PlanPhoto {
    /// What the legend calls them.
    pub name: String,
    /// Where they are read.
    pub source: Arc<dyn PhotoSource>,
}

impl std::fmt::Debug for PlanPhoto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlanPhoto")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl PartialEq for PlanPhoto {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && std::ptr::addr_eq(Arc::as_ptr(&self.source), Arc::as_ptr(&other.source))
    }
}

/// A text of a drawing along the route.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanText {
    /// Its insertion point: station, offset, m.
    pub at: [f64; 2],
    /// Letter height on the ground, m.
    pub height: f64,
    /// The text (its first line).
    pub text: String,
    /// Its colour.
    pub color: Color,
}

/// One of the project's drawings near the route, straightened along it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlanDrawing {
    /// Its name (the project's).
    pub name: String,
    /// Its lines: runs of (station, offset), m, and their colour.
    pub lines: Vec<(Vec<[f64; 2]>, Color)>,
    /// Its points: (station, offset), colour.
    pub points: Vec<([f64; 2], Color)>,
    /// Its texts.
    pub texts: Vec<PlanText>,
}

/// One of the project's drawings around the route, for the key map: its
/// lines in project coordinates (north up), thinned to the key map's
/// scale.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct KeyMapDrawing {
    /// Its name (the project's).
    pub name: String,
    /// Its lines: runs of (x, y), m, and their colour.
    pub lines: Vec<(Vec<[f64; 2]>, Color)>,
}

/// The square around the route the key map's backgrounds are taken
/// within: (lowest x, y; side, m) — centred on the route's box, 1.6 times
/// its longer side (a key map wider than tall, or taller than wide,
/// fitted to the route, shows that much around it).
pub(crate) fn key_square(centre: &[Point]) -> Option<([f64; 2], f64)> {
    if centre.len() < 2 {
        return None;
    }
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in centre {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let side = 1.6 * (hi[0] - lo[0]).max(hi[1] - lo[1]).max(100.0);
    let c = [0.5 * (lo[0] + hi[0]), 0.5 * (lo[1] + hi[1])];
    Some(([c[0] - side / 2.0, c[1] - side / 2.0], side))
}

/// Where a point lies along `centre`: (station, offset), and whether it
/// lies beside the centreline (taken back from there it is where it was:
/// not beyond the ends, not in the wedge outside a bend).
fn along(centre: &[Point], p: [f64; 2], tolerance: f64) -> ([f64; 2], bool) {
    let (s, o) = project(centre, [p[0], p[1], 0.0]);
    let back = point_at(centre, s, o);
    ([s, o], (back[0] - p[0]).hypot(back[1] - p[1]) < tolerance)
}

/// The runs of polyline `points` (project coordinates) within
/// `half_width` of `centre`, straightened: (station, offset).
fn straighten(
    centre: &[Point],
    points: &[[f64; 2]],
    half_width: f64,
    bounds: ([f64; 2], [f64; 2]),
) -> Vec<Vec<[f64; 2]>> {
    let tolerance = 0.01 * half_width.max(1.0);
    let (lo, hi) = bounds;
    let mut runs: Vec<Vec<[f64; 2]>> = Vec::new();
    let mut run: Vec<[f64; 2]> = Vec::new();
    // The last point straightened: (station, offset), beside.
    let mut last: Option<([f64; 2], bool)> = None;
    let flush = |run: &mut Vec<[f64; 2]>, runs: &mut Vec<Vec<[f64; 2]>>| {
        if run.len() >= 2 {
            runs.push(std::mem::take(run));
        } else {
            run.clear();
        }
    };
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        // A segment away from the corridor's box: not straightened.
        if a[0].max(b[0]) < lo[0]
            || a[0].min(b[0]) > hi[0]
            || a[1].max(b[1]) < lo[1]
            || a[1].min(b[1]) > hi[1]
        {
            flush(&mut run, &mut runs);
            last = None;
            continue;
        }
        let n = ((b[0] - a[0]).hypot(b[1] - a[1]) / DENSIFY_M)
            .ceil()
            .max(1.0) as usize;
        let first = if last.is_some() { 1 } else { 0 };
        for k in first..=n {
            let t = k as f64 / n as f64;
            let q = [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])];
            let (so, beside) = along(centre, q, tolerance);
            let inside = beside && so[1].abs() <= half_width;
            if let Some((prev, prev_beside)) = last {
                let was_inside = prev_beside && prev[1].abs() <= half_width;
                // A jump along the route (the inside of a bend): a break.
                let jump = (so[0] - prev[0]).abs() > 4.0 * DENSIFY_M;
                if jump {
                    flush(&mut run, &mut runs);
                } else if beside && prev_beside && was_inside != inside {
                    // Where it crosses the edge of the width.
                    let outside = if inside { prev[1] } else { so[1] };
                    let edge = half_width.copysign(outside);
                    let u = ((edge - prev[1]) / (so[1] - prev[1])).clamp(0.0, 1.0);
                    let cross = [prev[0] + u * (so[0] - prev[0]), edge];
                    if inside {
                        run.push(cross);
                    } else {
                        run.push(cross);
                        flush(&mut run, &mut runs);
                    }
                }
            }
            if inside {
                run.push(so);
            } else {
                flush(&mut run, &mut runs);
            }
            last = Some((so, beside));
        }
    }
    flush(&mut run, &mut runs);
    runs
}

impl RouteDrawing {
    /// Adds the project's drawing `drawing` named `name` under the plan:
    /// its lines (each piece of at most [`DENSIFY_M`]), points and texts
    /// within `half_width` of the route's centreline, straightened along
    /// it (lines cut where they leave the width); what lies on layers
    /// switched off in the drawing is left out.
    ///
    /// Cost: one projection onto the centreline per piece of line within
    /// the centreline's box widened by `half_width` (≈ 1 µs per piece and
    /// segment of the centreline).
    pub fn add_background_drawing(
        &mut self,
        name: &str,
        drawing: &ohl_background::Drawing,
        half_width: f64,
    ) {
        let centre = &self.centreline;
        if centre.len() < 2 || half_width <= 0.0 {
            return;
        }
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for p in centre {
            for k in 0..2 {
                lo[k] = lo[k].min(p[k] - half_width);
                hi[k] = hi[k].max(p[k] + half_width);
            }
        }
        let off = |layer: u32| drawing.layers.get(layer as usize).is_some_and(|l| l.off);
        let color = |c: [u8; 3]| Color(c[0], c[1], c[2]);
        let mut out = PlanDrawing {
            name: name.to_owned(),
            ..PlanDrawing::default()
        };
        for l in &drawing.lines {
            if off(l.layer) || l.points.len() < 2 {
                continue;
            }
            let mut pts: Vec<[f64; 2]> = l.points.iter().map(|p| [p[0], p[1]]).collect();
            if l.closed {
                pts.push(pts[0]);
            }
            for run in straighten(centre, &pts, half_width, (lo, hi)) {
                out.lines.push((run, color(l.color)));
            }
        }
        let tolerance = 0.01 * half_width.max(1.0);
        let near = |p: [f64; 3]| -> Option<[f64; 2]> {
            if p[0] < lo[0] || p[0] > hi[0] || p[1] < lo[1] || p[1] > hi[1] {
                return None;
            }
            let (so, beside) = along(centre, [p[0], p[1]], tolerance);
            (beside && so[1].abs() <= half_width).then_some(so)
        };
        for m in &drawing.points {
            if let Some(at) = near(m.at).filter(|_| !off(m.layer)) {
                out.points.push((at, color(m.color)));
            }
        }
        for t in &drawing.texts {
            let text = t.text.lines().next().unwrap_or("").trim();
            if text.is_empty() || off(t.layer) {
                continue;
            }
            if let Some(at) = near(t.at) {
                out.texts.push(PlanText {
                    at,
                    height: t.height,
                    text: text.to_owned(),
                    color: color(t.color),
                });
            }
        }
        if !(out.lines.is_empty() && out.points.is_empty() && out.texts.is_empty()) {
            Arc::make_mut(&mut self.background).push(out);
        }
    }

    /// Adds the project's drawing `drawing` named `name` under the key
    /// map: its lines within the square around the route
    /// ([`key_square`]), north up, each line's points closer than 1/3000
    /// of the square to the last one kept left out (≈ 0.04 mm on a key
    /// map 110 mm wide); what lies on layers switched off is left out.
    ///
    /// Cost: one pass over the drawing's points.
    pub fn add_key_map_drawing(&mut self, name: &str, drawing: &ohl_background::Drawing) {
        let Some((lo, side)) = key_square(&self.centreline) else {
            return;
        };
        let hi = [lo[0] + side, lo[1] + side];
        let tol = side / 3000.0;
        let off = |layer: u32| drawing.layers.get(layer as usize).is_some_and(|l| l.off);
        let mut out = KeyMapDrawing {
            name: name.to_owned(),
            lines: Vec::new(),
        };
        for l in &drawing.lines {
            if off(l.layer) || l.points.len() < 2 {
                continue;
            }
            let (mut a, mut b) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
            for p in &l.points {
                for k in 0..2 {
                    a[k] = a[k].min(p[k]);
                    b[k] = b[k].max(p[k]);
                }
            }
            if b[0] < lo[0] || a[0] > hi[0] || b[1] < lo[1] || a[1] > hi[1] {
                continue;
            }
            let mut pts: Vec<[f64; 2]> = Vec::new();
            for p in &l.points {
                let q = [p[0], p[1]];
                if pts
                    .last()
                    .is_none_or(|r: &[f64; 2]| (q[0] - r[0]).hypot(q[1] - r[1]) >= tol)
                {
                    pts.push(q);
                }
            }
            if l.closed && pts.len() > 2 {
                pts.push(pts[0]);
            }
            if pts.len() > 1 {
                out.lines
                    .push((pts, Color(l.color[0], l.color[1], l.color[2])));
            }
        }
        if !out.lines.is_empty() {
            Arc::make_mut(&mut self.key_background).push(out);
        }
    }

    /// Draws the orthophotos of `source` (called `name` in the legend)
    /// under the plan, resampled along the route on each sheet (none: no
    /// orthophoto).
    pub fn set_photo(&mut self, name: &str, source: Option<Arc<dyn PhotoSource>>) {
        self.photo = source.map(|source| PlanPhoto {
            name: name.to_owned(),
            source,
        });
    }
}

/// The colour a drawing's own colour is drawn in: white (unseen on paper)
/// as black.
fn on_paper(c: Color) -> Color {
    if c.0 > 0xf0 && c.1 > 0xf0 && c.2 > 0xf0 {
        Color::BLACK
    } else {
        c
    }
}

/// The project's drawings in rectangle `rect` (paper, mm: the plan panel
/// or the part beyond the match line) of frame `f`, clipped to it, their
/// legend entries added; nothing unless the options ask.
pub(crate) fn drawing_items(d: &RouteDrawing, o: &Options, f: &Frame, rect: [f64; 4]) -> Vec<Item> {
    let choice = o.plan_drawings();
    if !choice.on() || d.background.is_empty() {
        return Vec::new();
    }
    let spec = &o.template.plan_background;
    let base = spec.pen.over(o.template.pen(
        Layer::BackgroundDrawings,
        Pen::new(Color(0x8c, 0x8c, 0x8c), 0.13),
    ));
    let paper = |p: &[f64; 2]| [f.x(p[0]), f.plan_y(p[1])];
    let mut out = Vec::new();
    for drawing in d.background.iter().filter(|x| choice.takes(&x.name)) {
        let before = out.len();
        for (line, color) in &drawing.lines {
            let pts: Vec<[f64; 2]> = line.iter().map(paper).collect();
            let pen = if spec.own_colours {
                Pen {
                    color: on_paper(*color),
                    ..base.clone()
                }
            } else {
                base.clone()
            };
            for run in clip_polyline(&pts, rect) {
                out.push(Item::Path(Path {
                    layer: Layer::BackgroundDrawings,
                    points: run,
                    closed: false,
                    pen: pen.clone(),
                    title: None,
                }));
            }
        }
        if spec.points {
            for (p, color) in &drawing.points {
                let at = paper(p);
                if crate::model::inside(at, rect) {
                    out.push(Item::Circle(Circle {
                        layer: Layer::BackgroundDrawings,
                        centre: at,
                        radius: 0.25,
                        stroke: None,
                        fill: Some(if spec.own_colours {
                            on_paper(*color)
                        } else {
                            base.color
                        }),
                        title: None,
                    }));
                }
            }
        }
        if spec.texts {
            for t in &drawing.texts {
                let at = paper(&t.at);
                // At its size on the ground (along the sheet's scale),
                // left out when too small to read.
                let size = t.height * f.h;
                if size < 0.5 || !crate::model::inside(at, rect) {
                    continue;
                }
                out.push(Item::Text(Text {
                    layer: Layer::BackgroundDrawings,
                    at,
                    text: t.text.clone(),
                    size: size.min(10.0),
                    anchor: Anchor::Start,
                    bold: false,
                    color: if spec.own_colours {
                        on_paper(t.color)
                    } else {
                        base.color
                    },
                }));
            }
        }
        if out.len() > before {
            f.legend.borrow_mut().add(
                Sample::Line(base.clone()),
                o.template
                    .legend
                    .label("background_drawing", &[("name", &drawing.name)]),
            );
        }
    }
    out
}

/// The orthophoto of rectangle `rect` (paper, mm: the plan panel) of
/// frame `f`, resampled along the route at the template's resolution
/// (the options' when they give one; lower when it would have more than
/// [`MAX_PHOTO_PIXELS`]), veiled, as a JPEG image; its legend entry
/// added. None unless the options ask, or where no photo lies under the
/// panel.
///
/// Cost: one photo read per pixel (an A1 plan panel of 800 × 150 mm at
/// 200 dpi: ≈ 15 million pixels, ≈ 1–2 s) and the JPEG written.
pub(crate) fn photo_item(d: &RouteDrawing, o: &Options, f: &Frame, rect: [f64; 4]) -> Option<Item> {
    let photo = d.photo.as_ref()?;
    if !o.plan_photos().on() || d.centreline.len() < 2 || rect[2] <= 0.0 || rect[3] <= 0.0 {
        return None;
    }
    let spec = &o.template.plan_background;
    let dpi = o.background_dpi.unwrap_or(spec.dpi).clamp(10.0, 600.0);
    let mut k = dpi / 25.4; // pixels per mm
    if rect[2] * rect[3] * k * k > MAX_PHOTO_PIXELS {
        k = (MAX_PHOTO_PIXELS / (rect[2] * rect[3])).sqrt();
    }
    let (w, h) = (
        (rect[2] * k).ceil().max(1.0) as usize,
        (rect[3] * k).ceil().max(1.0) as usize,
    );
    // A pixel on the ground, the finer of along and across.
    let metres_per_pixel = (rect[2] / w as f64) / f.h.max(f.plan_k);
    // Each column's point on the axis and the normal to its right.
    let columns: Vec<([f64; 2], [f64; 2])> = (0..w)
        .map(|i| {
            let x = rect[0] + (i as f64 + 0.5) * rect[2] / w as f64;
            let s = f.s0 + (x - f.left) / f.h;
            let base = point_at(&d.centreline, s, 0.0);
            let right = point_at(&d.centreline, s, 1.0);
            (base, [right[0] - base[0], right[1] - base[1]])
        })
        .collect();
    let veil = spec.veil;
    let mut rgb = vec![255u8; w * h * 3];
    let mut any = false;
    // In strips of rows (memory: a strip's points, not the whole image's).
    let strip = (1 << 18) / w.max(1) + 1;
    let mut points = Vec::with_capacity(strip * w);
    let mut colours = Vec::with_capacity(strip * w);
    for top in (0..h).step_by(strip) {
        let rows = top..(top + strip).min(h);
        points.clear();
        for j in rows.clone() {
            let y = rect[1] + (j as f64 + 0.5) * rect[3] / h as f64;
            let offset = (y - f.plan_axis) / f.plan_k;
            points.extend(
                columns
                    .iter()
                    .map(|(b, n)| [b[0] + n[0] * offset, b[1] + n[1] * offset]),
            );
        }
        colours.clear();
        colours.resize(points.len(), None);
        photo.source.sample(&points, metres_per_pixel, &mut colours);
        let start = top * w * 3;
        for (q, c) in colours.iter().enumerate() {
            if let Some(c) = c {
                any = true;
                for (ch, v) in c.iter().enumerate() {
                    rgb[start + q * 3 + ch] =
                        (f64::from(*v) * (1.0 - veil) + 255.0 * veil).round() as u8;
                }
            }
        }
    }
    if !any {
        return None;
    }
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, spec.quality)
        .encode(&rgb, w as u32, h as u32, image::ExtendedColorType::Rgb8)
        .ok()?;
    f.legend.borrow_mut().add(
        Sample::Square(Pen::new(Color(0xa0, 0xa0, 0xa0), 0.3)),
        o.template
            .legend
            .label("orthophoto", &[("name", &photo.name)]),
    );
    Some(Item::Image(Image {
        layer: Layer::BackgroundPhoto,
        at: [rect[0], rect[1]],
        width: rect[2],
        height: rect[3],
        href: format!("data:image/jpeg;base64,{}", crate::base64(&jpeg)),
        pixels: Some([w as u32, h as u32]),
    }))
}

/// The key map's backgrounds in its rectangle `inner` (paper, mm), north
/// up, the point (`cx`, `cy`) at its centre at `k` mm per m: the
/// orthophotos (resampled at the plan's resolution, at most 150 dpi,
/// under the template's `[key_map] veil`) and the project's drawings
/// ([`RouteDrawing::key_background`], in the plan's pen); each when the
/// options ask. Their legend entries are the plan's (not added again).
///
/// Cost: one photo read per pixel (a key map of 110 × 70 mm at 150 dpi:
/// ≈ 0.27 million pixels) and the drawings' lines in it.
pub(crate) fn key_map_items(
    d: &RouteDrawing,
    o: &Options,
    inner: [f64; 4],
    (cx, cy, k): (f64, f64, f64),
) -> Vec<Item> {
    let mut out = Vec::new();
    let spec = &o.template.key_map;
    if o.key_map_photo_on()
        && let Some(photo) = &d.photo
        && inner[2] > 0.0
        && inner[3] > 0.0
        && k > 0.0
    {
        let dpi = o
            .background_dpi
            .unwrap_or(o.template.plan_background.dpi)
            .clamp(10.0, 150.0);
        let px = dpi / 25.4;
        let (w, h) = (
            (inner[2] * px).ceil().max(1.0) as usize,
            (inner[3] * px).ceil().max(1.0) as usize,
        );
        let metres_per_pixel = (inner[2] / w as f64) / k;
        let mut points = Vec::with_capacity(w * h);
        for j in 0..h {
            let y = inner[1] + (j as f64 + 0.5) * inner[3] / h as f64;
            let gy = cy - (y - (inner[1] + inner[3] / 2.0)) / k;
            for i in 0..w {
                let x = inner[0] + (i as f64 + 0.5) * inner[2] / w as f64;
                points.push([cx + (x - (inner[0] + inner[2] / 2.0)) / k, gy]);
            }
        }
        let mut colours = vec![None; points.len()];
        photo.source.sample(&points, metres_per_pixel, &mut colours);
        if colours.iter().any(Option::is_some) {
            let veil = spec.veil;
            let mut rgb = vec![255u8; w * h * 3];
            for (q, c) in colours.iter().enumerate() {
                if let Some(c) = c {
                    for (ch, v) in c.iter().enumerate() {
                        rgb[q * 3 + ch] =
                            (f64::from(*v) * (1.0 - veil) + 255.0 * veil).round() as u8;
                    }
                }
            }
            let mut jpeg = Vec::new();
            if image::codecs::jpeg::JpegEncoder::new_with_quality(
                &mut jpeg,
                o.template.plan_background.quality,
            )
            .encode(&rgb, w as u32, h as u32, image::ExtendedColorType::Rgb8)
            .is_ok()
            {
                out.push(Item::Image(Image {
                    layer: Layer::KeyMap,
                    at: [inner[0], inner[1]],
                    width: inner[2],
                    height: inner[3],
                    href: format!("data:image/jpeg;base64,{}", crate::base64(&jpeg)),
                    pixels: Some([w as u32, h as u32]),
                }));
            }
        }
    }
    if o.key_map_drawings_on() && !d.key_background.is_empty() {
        let pb = &o.template.plan_background;
        let base = pb.pen.over(o.template.pen(
            Layer::BackgroundDrawings,
            Pen::new(Color(0x8c, 0x8c, 0x8c), 0.13),
        ));
        // Thinner than in the plan: the key map is small.
        let base = Pen {
            width: base.width * 0.6,
            ..base
        };
        let paper = |p: &[f64; 2]| {
            [
                inner[0] + inner[2] / 2.0 + (p[0] - cx) * k,
                inner[1] + inner[3] / 2.0 - (p[1] - cy) * k,
            ]
        };
        for drawing in d.key_background.iter() {
            for (line, color) in &drawing.lines {
                let pts: Vec<[f64; 2]> = line.iter().map(paper).collect();
                let pen = if pb.own_colours {
                    Pen {
                        color: on_paper(*color),
                        ..base.clone()
                    }
                } else {
                    base.clone()
                };
                for run in clip_polyline(&pts, inner) {
                    out.push(Item::Path(Path {
                        layer: Layer::KeyMap,
                        points: run,
                        closed: false,
                        pen: pen.clone(),
                        title: None,
                    }));
                }
            }
        }
    }
    out
}
