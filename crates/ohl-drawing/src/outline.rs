// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! An SVG image as outlines: its paths (and texts, as their glyphs'
//! outlines) as polylines on paper, fitted into a box as the sheets fit
//! images (proportions kept, at the box's left, centred up and down). DXF
//! R12 has no images: SVG logos go there as these outlines.

use resvg::usvg;
use resvg::usvg::tiny_skia_path::PathSegment;

/// Pieces a curve is cut into.
const CURVE_STEPS: usize = 8;

/// Polylines with whether each is closed and its colour.
pub(crate) type Outlines = Vec<(Vec<[f64; 2]>, bool, crate::model::Color)>;

/// The outlines of the SVG image `href` (a data URI) fitted into the box
/// at `at` (top left, mm), `w` × `h` mm, as closed or open polylines with
/// their colour; `None`: not an SVG, or it does not read.
pub(crate) fn svg_outlines(href: &str, at: [f64; 2], w: f64, h: f64) -> Option<Outlines> {
    let data = href.strip_prefix("data:image/svg+xml;base64,")?;
    let bytes = crate::unbase64(data)?;
    let options = usvg::Options::default();
    let tree = usvg::Tree::from_data(&bytes, &options).ok()?;
    let size = tree.size();
    let (sw, sh) = (f64::from(size.width()), f64::from(size.height()));
    if !(sw > 0.0 && sh > 0.0) {
        return None;
    }
    let k = (w / sw).min(h / sh);
    let origin = [at[0], at[1] + (h - sh * k) / 2.0];
    let mut out = Vec::new();
    group(tree.root(), &mut |p: &usvg::Path| {
        let t = p.abs_transform();
        let map = |x: f32, y: f32| {
            let mut q = usvg::tiny_skia_path::Point::from_xy(x, y);
            t.map_point(&mut q);
            [
                origin[0] + f64::from(q.x) * k,
                origin[1] + f64::from(q.y) * k,
            ]
        };
        let color = match p
            .fill()
            .map(|f| f.paint())
            .or(p.stroke().map(|s| s.paint()))
        {
            Some(usvg::Paint::Color(c)) => crate::model::Color(c.red, c.green, c.blue),
            _ => crate::model::Color::BLACK,
        };
        let mut run: Vec<[f64; 2]> = Vec::new();
        let mut last = [0.0f32, 0.0f32];
        let flush = |run: &mut Vec<[f64; 2]>, closed: bool, out: &mut Vec<_>| {
            if run.len() > 1 {
                out.push((std::mem::take(run), closed, color));
            } else {
                run.clear();
            }
        };
        for seg in p.data().segments() {
            match seg {
                PathSegment::MoveTo(q) => {
                    flush(&mut run, false, &mut out);
                    run.push(map(q.x, q.y));
                    last = [q.x, q.y];
                }
                PathSegment::LineTo(q) => {
                    run.push(map(q.x, q.y));
                    last = [q.x, q.y];
                }
                PathSegment::QuadTo(c, q) => {
                    for i in 1..=CURVE_STEPS {
                        let s = i as f32 / CURVE_STEPS as f32;
                        let u = 1.0 - s;
                        run.push(map(
                            u * u * last[0] + 2.0 * u * s * c.x + s * s * q.x,
                            u * u * last[1] + 2.0 * u * s * c.y + s * s * q.y,
                        ));
                    }
                    last = [q.x, q.y];
                }
                PathSegment::CubicTo(c1, c2, q) => {
                    for i in 1..=CURVE_STEPS {
                        let s = i as f32 / CURVE_STEPS as f32;
                        let u = 1.0 - s;
                        run.push(map(
                            u * u * u * last[0]
                                + 3.0 * u * u * s * c1.x
                                + 3.0 * u * s * s * c2.x
                                + s * s * s * q.x,
                            u * u * u * last[1]
                                + 3.0 * u * u * s * c1.y
                                + 3.0 * u * s * s * c2.y
                                + s * s * s * q.y,
                        ));
                    }
                    last = [q.x, q.y];
                }
                PathSegment::Close => flush(&mut run, true, &mut out),
            }
        }
        flush(&mut run, false, &mut out);
    });
    Some(out)
}

/// Every path of group `g` and its subgroups (texts as their glyphs).
fn group(g: &usvg::Group, f: &mut dyn FnMut(&usvg::Path)) {
    for node in g.children() {
        match node {
            usvg::Node::Group(sub) => group(sub, f),
            usvg::Node::Path(p) => f(p),
            usvg::Node::Text(t) => group(t.flattened(), f),
            usvg::Node::Image(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_images_become_outlines() {
        // A 20 × 10 rectangle in a 40 × 20 image, fitted into 20 × 20 mm:
        // half size, centred up and down (5 mm down).
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect x="10" y="5" width="20" height="10" fill="#ff0000"/></svg>"##;
        let href = format!(
            "data:image/svg+xml;base64,{}",
            crate::base64(svg.as_bytes())
        );
        let lines = svg_outlines(&href, [100.0, 50.0], 20.0, 20.0).unwrap();
        assert_eq!(lines.len(), 1);
        let (pts, closed, color) = &lines[0];
        assert!(closed);
        assert_eq!(*color, crate::model::Color(255, 0, 0));
        let xs: Vec<f64> = pts.iter().map(|p| p[0]).collect();
        let ys: Vec<f64> = pts.iter().map(|p| p[1]).collect();
        let min = |v: &[f64]| v.iter().copied().fold(f64::INFINITY, f64::min);
        let max = |v: &[f64]| v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        assert!((min(&xs) - 105.0).abs() < 1e-4 && (max(&xs) - 115.0).abs() < 1e-4);
        assert!((min(&ys) - 57.5).abs() < 1e-4 && (max(&ys) - 62.5).abs() < 1e-4);
        assert!(svg_outlines("data:image/png;base64,AAAA", [0.0; 2], 1.0, 1.0).is_none());
    }
}
