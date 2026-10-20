// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! What a sheet template puts around the plan and the profile: the frame,
//! the title block, the revision table, notes, the legend, the key map,
//! graphic scales, north arrows and the band rows under the profile.

use std::collections::BTreeMap;

use crate::layout::Frame;
use crate::model::{Anchor, Circle, Clip, Color, Image, Item, Layer, Path, Pen, Text};
use crate::style::{Legend, Sample};
use crate::template::{Align, BandKind, Column, SheetTemplate, fill};
use crate::{Logo, Options, RouteDrawing, format_station};

/// A rectangle: left, top, width, height (mm).
pub(crate) type Rect = [f64; 4];

/// The values of a sheet's fields: computed and given, by name.
pub(crate) struct Values {
    /// Texts by name.
    pub text: BTreeMap<String, String>,
    /// Numbers by name (formatted by `{name:03}` / `{name:.2}`).
    pub number: BTreeMap<String, f64>,
    /// Stations by name (`{from:km}` / `{from:m}`).
    pub station: BTreeMap<String, f64>,
    /// The template: what an empty field is written as.
    pub template: std::sync::Arc<SheetTemplate>,
}

impl Values {
    /// The value of `name` written with `fmt`; an empty one as the
    /// template says ([`SheetTemplate::if_empty`]): never an empty gap.
    pub(crate) fn get(&self, name: &str, fmt: &str) -> Option<String> {
        let v = self.raw(name, fmt)?;
        if v.trim().is_empty() {
            return Some(self.template.if_empty(name).unwrap_or(v));
        }
        Some(v)
    }

    fn raw(&self, name: &str, fmt: &str) -> Option<String> {
        if let Some(s) = self.station.get(name) {
            return Some(match fmt {
                "m" => format!("{s:.1}"),
                _ => format_station(*s),
            });
        }
        if let Some(v) = self.number.get(name) {
            return Some(crate::template::format_number(*v, fmt));
        }
        let t = self.text.get(name)?;
        if name == "date" {
            return Some(crate::template::format_date(t, fmt));
        }
        Some(t.clone())
    }

    /// `text` filled.
    pub(crate) fn fill(&self, text: &str) -> String {
        fill(text, &|n, f| self.get(n, f))
    }
}

fn text_item(at: [f64; 2], size: f64, anchor: Anchor, bold: bool, color: Color, t: &str) -> Item {
    text_on(Layer::Title, at, size, anchor, bold, color, t)
}

fn text_on(
    layer: Layer,
    at: [f64; 2],
    size: f64,
    anchor: Anchor,
    bold: bool,
    color: Color,
    t: &str,
) -> Item {
    Item::Text(Text {
        layer,
        at,
        text: t.to_owned(),
        size,
        anchor,
        bold,
        color,
    })
}

fn rect_path(layer: Layer, r: Rect, pen: Pen) -> Item {
    Item::Path(Path {
        layer,
        points: vec![
            [r[0], r[1]],
            [r[0] + r[2], r[1]],
            [r[0] + r[2], r[1] + r[3]],
            [r[0], r[1] + r[3]],
        ],
        closed: true,
        pen,
        title: None,
    })
}

fn line(layer: Layer, a: [f64; 2], b: [f64; 2], pen: Pen) -> Item {
    Item::Path(Path {
        layer,
        points: vec![a, b],
        closed: false,
        pen,
        title: None,
    })
}

/// The width of the export's logos side by side (with their gaps), mm.
pub(crate) fn logos_width(logos: &[Logo]) -> f64 {
    if logos.is_empty() {
        0.0
    } else {
        logos.iter().map(|l| l.width + 4.0).sum::<f64>() + 2.0
    }
}

/// The title block's columns' widths for a block `width` mm wide.
pub(crate) fn column_widths(t: &SheetTemplate, width: f64, logos: &[Logo]) -> Vec<f64> {
    let cols = &t.title_block.columns;
    let fixed: f64 = cols
        .iter()
        .map(|c| match c {
            Column::Fixed(w) => *w,
            Column::Logos => logos_width(logos),
            Column::Rest => 0.0,
        })
        .sum();
    let rest = cols.iter().filter(|c| **c == Column::Rest).count().max(1) as f64;
    cols.iter()
        .map(|c| match c {
            Column::Fixed(w) => *w,
            Column::Logos => logos_width(logos),
            Column::Rest => ((width - fixed) / rest).max(0.0),
        })
        .collect()
}

/// The lines of `text` broken to `width` mm at `size` (words kept whole).
pub(crate) fn wrap(text: &str, size: f64, bold: bool, width: f64) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            let candidate = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if !line.is_empty() && crate::pdf::text_width(&candidate, size, bold) > width {
                out.push(std::mem::replace(&mut line, word.to_owned()));
            } else {
                line = candidate;
            }
        }
        out.push(line);
    }
    out
}

/// `text` broken to `width` mm at `size`, the size made smaller (down to
/// 40 %) until every line fits `width` and the lines fit `height`: the
/// size and the lines. A text that does not fit even then keeps the
/// smallest size.
pub(crate) fn fit_text(
    text: &str,
    size: f64,
    bold: bool,
    width: f64,
    height: f64,
) -> (f64, Vec<String>) {
    let mut s = size;
    loop {
        let lines = wrap(text, s, bold, width);
        let widest = lines
            .iter()
            .map(|l| crate::pdf::text_width(l, s, bold))
            .fold(0.0, f64::max);
        let tall = 1.25 * s * (lines.len() as f64 - 1.0) + 0.72 * s;
        if (widest <= width + 1e-9 && tall <= height + 1e-9) || s <= 0.4 * size {
            return (s, lines);
        }
        s = (s * 0.92).max(0.4 * size);
    }
}

/// One line of `text` in `width` mm: the size made smaller (down to 60 %),
/// then the text shortened with "…" if it still does not fit.
pub(crate) fn fit_line(text: &str, size: f64, bold: bool, width: f64) -> (f64, String) {
    let w = crate::pdf::text_width(text, size, bold);
    if w <= width {
        return (size, text.to_owned());
    }
    let s = (size * width / w * 0.999).max(0.6 * size);
    if crate::pdf::text_width(text, s, bold) <= width {
        return (s, text.to_owned());
    }
    let mut t: String = text.to_owned();
    while t.chars().count() > 1 && crate::pdf::text_width(&format!("{t}…"), s, bold) > width {
        t.pop();
    }
    (s, t.trim_end().to_owned() + "…")
}

/// Draws the title block of template `t` into `r`: its outline (its grid
/// when asked), each cell's text (lines centred up and down) or image.
pub(crate) fn title_block(
    items: &mut Vec<Item>,
    t: &SheetTemplate,
    r: Rect,
    values: &Values,
    logos: &[Logo],
) {
    let tb = &t.title_block;
    let color = Color::hex_or_black(&t.text.colour);
    let widths = column_widths(t, r[2], logos);
    let xs: Vec<f64> = std::iter::once(r[0])
        .chain(widths.iter().scan(r[0], |x, w| {
            *x += w;
            Some(*x)
        }))
        .collect();
    let ys: Vec<f64> = std::iter::once(r[1])
        .chain(tb.rows.iter().scan(r[1], |y, h| {
            *y += h.0;
            Some(*y)
        }))
        .collect();
    let pen = t.pen(Layer::Frame, Pen::new(Color::BLACK, tb.line.0));
    items.push(rect_path(Layer::Frame, r, pen.clone()));
    if tb.grid {
        // Lines between the cells, not through a spanning cell.
        let thin = Pen::new(pen.color, (pen.width * 0.5).max(0.13));
        let covered = |row: usize, col: usize| {
            tb.cells.iter().find(|c| {
                row >= c.at[0]
                    && row < c.at[0] + c.span[0]
                    && col >= c.at[1]
                    && col < c.at[1] + c.span[1]
            })
        };
        for row in 0..tb.rows.len() {
            for col in 0..widths.len() {
                // The line under (row, col) unless one cell covers both.
                if row + 1 < tb.rows.len() {
                    let same = match (covered(row, col), covered(row + 1, col)) {
                        (Some(a), Some(b)) => std::ptr::eq(a, b),
                        _ => false,
                    };
                    if !same {
                        items.push(line(
                            Layer::Frame,
                            [xs[col], ys[row + 1]],
                            [xs[col + 1], ys[row + 1]],
                            thin.clone(),
                        ));
                    }
                }
                if col + 1 < widths.len() {
                    let same = match (covered(row, col), covered(row, col + 1)) {
                        (Some(a), Some(b)) => std::ptr::eq(a, b),
                        _ => false,
                    };
                    if !same {
                        items.push(line(
                            Layer::Frame,
                            [xs[col + 1], ys[row]],
                            [xs[col + 1], ys[row + 1]],
                            thin.clone(),
                        ));
                    }
                }
            }
        }
    }
    for c in &tb.cells {
        let [row, col] = c.at;
        let cell = [
            xs[col],
            ys[row],
            xs[col + c.span[1]] - xs[col],
            ys[row + c.span[0]] - ys[row],
        ];
        let pad = 2.0;
        if let Some(image) = &c.image {
            if image == "{logos}" {
                let mut x = cell[0] + 3.0;
                for logo in logos {
                    items.push(Item::Image(Image {
                        layer: Layer::Logo,
                        at: [x, cell[1] + 3.0],
                        width: logo.width,
                        height: (cell[3] - 6.0).max(1.0),
                        href: logo.href.clone(),
                    }));
                    x += logo.width + 4.0;
                }
            } else if let Some(href) = t.resolved.get(image) {
                items.push(Item::Image(Image {
                    layer: Layer::Logo,
                    at: [cell[0] + pad, cell[1] + pad],
                    width: (cell[2] - 2.0 * pad).max(1.0),
                    height: (cell[3] - 2.0 * pad).max(1.0),
                    href: href.clone(),
                }));
            }
        }
        if c.text.is_empty() {
            continue;
        }
        let filled = values.fill(&c.text);
        let (size, lines) = fit_text(
            &filled,
            c.size.map_or(3.0, |s| s.0),
            c.bold,
            cell[2] - 2.0 * pad,
            cell[3] - 1.0,
        );
        let step = 1.25 * size;
        let block = step * (lines.len() as f64 - 1.0) + 0.72 * size;
        let first = cell[1] + (cell[3] - block) / 2.0 + 0.72 * size;
        let (x, anchor) = match c.align {
            Align::Left => (cell[0] + pad, Anchor::Start),
            Align::Centre => (cell[0] + cell[2] / 2.0, Anchor::Middle),
            Align::Right => (cell[0] + cell[2] - pad, Anchor::End),
        };
        for (k, l) in lines.iter().enumerate() {
            if l.trim().is_empty() {
                continue;
            }
            items.push(text_on(
                if c.text.contains('{') {
                    Layer::TitleFields
                } else {
                    Layer::Title
                },
                [x, first + step * k as f64],
                size,
                anchor,
                c.bold,
                color,
                l,
            ));
        }
    }
}

/// Draws the revision table into `r`: a heading row and the last
/// revisions, newest at the top.
pub(crate) fn revisions(
    items: &mut Vec<Item>,
    t: &SheetTemplate,
    r: Rect,
    values: &Values,
    o: &Options,
) {
    let spec = &t.revisions;
    let color = Color::hex_or_black(&t.text.colour);
    let pen = t.pen(Layer::Revisions, Pen::new(Color::BLACK, 0.25));
    items.push(rect_path(Layer::Revisions, r, pen.clone()));
    let h = spec.row_height.0;
    let size = spec.size.0;
    let rows: Vec<&crate::Revision> = o.fields.revisions.iter().rev().take(spec.rows).collect();
    // Rows from the top: the newest first, the heading at the bottom (next
    // to the title block).
    let heading_y = r[1] + r[3] - h;
    let mut x = r[0];
    for col in &spec.columns {
        // A column past the table's width (the room inside the frame) is
        // cut at it; every text kept inside its column.
        let w = col.width.0.min(r[0] + r[2] - x);
        if w <= 1.0 {
            break;
        }
        items.push(line(
            Layer::Revisions,
            [x, r[1]],
            [x, r[1] + r[3]],
            pen.clone(),
        ));
        let (s, title) = fit_line(&col.title, size, true, w - 2.0);
        items.push(text_on(
            Layer::Revisions,
            [x + 1.0, heading_y + h / 2.0 + 0.36 * s],
            s,
            Anchor::Start,
            true,
            color,
            &title,
        ));
        for (k, rev) in rows.iter().enumerate() {
            let y = heading_y - h * (k as f64 + 1.0);
            let text = fill(&col.value, &|n, f| {
                rev.get(n).map(str::to_owned).or_else(|| values.get(n, f))
            });
            let (s, text) = fit_line(&text, size, false, w - 2.0);
            items.push(text_on(
                Layer::Revisions,
                [x + 1.0, y + h / 2.0 + 0.36 * s],
                s,
                Anchor::Start,
                false,
                color,
                &text,
            ));
        }
        x += w;
    }
    for k in 0..=rows.len() {
        let y = heading_y - h * k as f64;
        items.push(line(
            Layer::Revisions,
            [r[0], y],
            [r[0] + r[2], y],
            pen.clone(),
        ));
    }
}

/// The height of the revision table, mm.
pub(crate) fn revisions_height(t: &SheetTemplate) -> f64 {
    t.revisions.row_height.0 * (t.revisions.rows as f64 + 1.0)
}

/// Draws the template's notes, placed from the frame `inner`'s edges.
/// Each note is broken to its width, at most to the frame's padding on
/// the right. Returns the rectangle each note covers.
pub(crate) fn notes(
    items: &mut Vec<Item>,
    t: &SheetTemplate,
    inner: Rect,
    values: &Values,
) -> Vec<Rect> {
    let color = Color::hex_or_black(&t.text.colour);
    let mut covered = Vec::new();
    for n in &t.notes {
        let x = if n.at[0].0 >= 0.0 {
            inner[0] + n.at[0].0
        } else {
            inner[0] + inner[2] + n.at[0].0
        };
        let y = if n.at[1].0 >= 0.0 {
            inner[1] + n.at[1].0
        } else {
            inner[1] + inner[3] + n.at[1].0
        };
        let size = n.size.0;
        let width = n.width.0.min(inner[0] + inner[2] - x).max(1.0);
        let lines = wrap(&values.fill(&n.text), size, n.bold, width);
        for (k, l) in lines.iter().enumerate() {
            items.push(text_item(
                [x, y + size + 1.25 * size * k as f64],
                size,
                Anchor::Start,
                n.bold,
                color,
                l,
            ));
        }
        let widest = lines
            .iter()
            .map(|l| crate::pdf::text_width(l, size, n.bold))
            .fold(0.0, f64::max);
        covered.push([
            x,
            y,
            widest,
            size * (1.25 * (lines.len() as f64 - 1.0) + 1.2),
        ]);
    }
    covered
}

/// Whether rectangles `a` and `b` overlap.
pub(crate) fn overlap(a: Rect, b: Rect) -> bool {
    a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
}

/// Draws the legend into `r`; the entries that do not fit, warned.
pub(crate) fn legend(
    items: &mut Vec<Item>,
    t: &SheetTemplate,
    r: Rect,
    legend: &Legend,
) -> Vec<String> {
    let spec = &t.legend;
    let color = Color::hex_or_black(&t.text.colour);
    items.push(rect_path(
        Layer::Legend,
        r,
        t.pen(Layer::Legend, Pen::new(Color::BLACK, 0.25)),
    ));
    let size = spec.size.0;
    items.push(text_on(
        Layer::Legend,
        [r[0] + 2.0, r[1] + 1.5 + size],
        size * 1.2,
        Anchor::Start,
        true,
        color,
        &spec.title,
    ));
    let row = 1.6 * size;
    let top = r[1] + 3.0 + 2.0 * size;
    let per_column = (((r[1] + r[3] - 1.0) - top) / row).floor().max(0.0) as usize;
    let col_w = spec.column_width.0;
    let columns = ((r[2] - 2.0) / col_w).floor().max(1.0) as usize;
    let mut left_out = 0;
    for (k, e) in legend.entries.iter().enumerate() {
        let (c, j) = match (k.checked_div(per_column), k.checked_rem(per_column)) {
            (Some(c), Some(j)) => (c, j),
            _ => (usize::MAX, 0),
        };
        if c >= columns {
            left_out += 1;
            continue;
        }
        let x = r[0] + 2.0 + col_w * c as f64;
        let y = top + row * j as f64;
        let mid = y - 0.35 * size;
        match &e.sample {
            Sample::Line(pen) => {
                items.push(line(Layer::Legend, [x, mid], [x + 8.0, mid], pen.clone()))
            }
            Sample::Square(pen) => items.push(rect_path(
                Layer::Legend,
                [x + 3.0, mid - 1.0, 2.0, 2.0],
                pen.clone(),
            )),
            Sample::Circle(pen) => items.push(Item::Circle(Circle {
                layer: Layer::Legend,
                centre: [x + 4.0, mid],
                radius: 1.0,
                stroke: Some(pen.clone()),
                fill: None,
                title: None,
            })),
            Sample::Dots(color, d) => {
                // Three dots, a little larger than drawn to be seen.
                let r = (0.5 * d).max(0.3);
                for dx in [1.5, 4.0, 6.5] {
                    items.push(Item::Circle(Circle {
                        layer: Layer::Legend,
                        centre: [x + dx, mid],
                        radius: r,
                        stroke: None,
                        fill: Some(*color),
                        title: None,
                    }));
                }
            }
        }
        let max = col_w - 11.0;
        let mut text = e.text.clone();
        while text.chars().count() > 3 && crate::pdf::text_width(&text, size, false) > max {
            text.pop();
            while text.ends_with(' ') {
                text.pop();
            }
            if !text.ends_with('…') {
                text.pop();
                text.push('…');
            }
        }
        items.push(Item::Text(Text {
            layer: Layer::Legend,
            at: [x + 10.0, y],
            text,
            size,
            anchor: Anchor::Start,
            bold: false,
            color,
        }));
    }
    if left_out > 0 {
        vec![format!(
            "the legend holds {} of its {} entries: {left_out} left out (a taller or wider legend in \
             the template)",
            legend.entries.len() - left_out,
            legend.entries.len()
        )]
    } else {
        Vec::new()
    }
}

/// A north arrow of `size` mm at `at` (its foot), pointing `dir` (a unit
/// vector on paper), on `layer`.
fn north_arrow(items: &mut Vec<Item>, at: [f64; 2], dir: [f64; 2], size: f64, layer: Layer) {
    let tip = [at[0] + dir[0] * size, at[1] + dir[1] * size];
    let across = [-dir[1], dir[0]];
    let head = 0.3 * size;
    let base = [tip[0] - dir[0] * head, tip[1] - dir[1] * head];
    let pen = Pen::new(Color::BLACK, 0.25);
    items.push(line(layer, at, tip, pen.clone()));
    items.push(Item::Path(Path {
        layer,
        points: vec![
            tip,
            [
                base[0] + across[0] * head * 0.45,
                base[1] + across[1] * head * 0.45,
            ],
            [
                base[0] - across[0] * head * 0.45,
                base[1] - across[1] * head * 0.45,
            ],
        ],
        closed: true,
        pen,
        title: Some("north".into()),
    }));
    items.push(Item::Text(Text {
        layer,
        at: [tip[0] + dir[0] * 2.5, tip[1] + dir[1] * 2.5 + 1.0],
        text: "N".into(),
        size: 2.5,
        anchor: Anchor::Middle,
        bold: true,
        color: Color::BLACK,
    }));
}

/// North arrows on the plan of frame `f`: one per straight stretch of the
/// centreline on the sheet (the north turns on the straightened plan at
/// every angle point), at the plan's top edge, pointing where north lies
/// on the straightened plan.
pub(crate) fn plan_north_arrows(items: &mut Vec<Item>, f: &Frame, d: &RouteDrawing, size: f64) {
    let mut s: f64 = 0.0;
    for w in d.centreline.windows(2) {
        let len = (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
        if len <= 0.0 {
            continue;
        }
        let (a, b) = (s.max(f.s0), (s + len).min(f.s1));
        s += len;
        // On this sheet, long enough on paper for an arrow.
        if (b - a) * f.h < 2.5 * size {
            continue;
        }
        let (ux, uy) = ((w[1][0] - w[0][0]) / len, (w[1][1] - w[0][1]) / len);
        // North (0, 1): along the line u_y, right of it −u_x (offsets
        // right go down on paper).
        let v = [uy * f.h, -ux * f.plan_k];
        let n = v[0].hypot(v[1]);
        if n <= 0.0 {
            continue;
        }
        let dir = [v[0] / n, v[1] / n];
        let foot_y = f.plan_top + 2.0 + size * (1.0 + dir[1].min(0.0)).max(0.0) + 0.5;
        let foot = [f.x(0.5 * (a + b)), (f.plan_top + 2.0 + size).max(foot_y)];
        north_arrow(items, foot, dir, size, Layer::Labels);
    }
}

/// A graphic scale `length` m long at `k` mm per m, its left end at `at`
/// (its baseline), horizontal; five alternating parts.
pub(crate) fn scale_bar(items: &mut Vec<Item>, at: [f64; 2], length: f64, k: f64, label: &str) {
    let w = length * k;
    let pen = Pen::new(Color::BLACK, 0.2);
    for i in 0..5 {
        let x = at[0] + w * f64::from(i) / 5.0;
        let part = [x, at[1] - 1.2, w / 5.0, 1.2];
        items.push(rect_path(Layer::Labels, part, pen.clone()));
        if i % 2 == 0 {
            for j in 0..4 {
                let y = part[1] + 0.3 * f64::from(j) + 0.15;
                items.push(line(
                    Layer::Labels,
                    [part[0], y],
                    [part[0] + part[2], y],
                    pen.clone(),
                ));
            }
        }
    }
    for (x, t) in [
        (at[0], "0".to_owned()),
        (
            at[0] + w,
            format!("{} m", crate::template::format_number(length, "")),
        ),
    ] {
        items.push(Item::Text(Text {
            layer: Layer::Labels,
            at: [x, at[1] - 2.0],
            text: t,
            size: 1.8,
            anchor: Anchor::Middle,
            bold: false,
            color: Color::BLACK,
        }));
    }
    items.push(Item::Text(Text {
        layer: Layer::Labels,
        at: [at[0], at[1] + 2.6],
        text: label.to_owned(),
        size: 1.8,
        anchor: Anchor::Start,
        bold: false,
        color: Color::BLACK,
    }));
}

/// A vertical graphic scale `length` m tall at `k` mm per m, its foot at
/// `at`.
pub(crate) fn vertical_scale_bar(items: &mut Vec<Item>, at: [f64; 2], length: f64, k: f64) {
    let h = length * k;
    let pen = Pen::new(Color::BLACK, 0.2);
    for i in 0..5 {
        let y = at[1] - h * f64::from(i + 1) / 5.0;
        items.push(rect_path(
            Layer::Labels,
            [at[0], y, 1.2, h / 5.0],
            pen.clone(),
        ));
        if i % 2 == 0 {
            for j in 0..4 {
                let x = at[0] + 0.3 * f64::from(j) + 0.15;
                items.push(line(Layer::Labels, [x, y], [x, y + h / 5.0], pen.clone()));
            }
        }
    }
    items.push(Item::Text(Text {
        layer: Layer::Labels,
        at: [at[0] + 2.0, at[1] - h + 1.0],
        text: format!("{} m", crate::template::format_number(length, "")),
        size: 1.8,
        anchor: Anchor::Start,
        bold: false,
        color: Color::BLACK,
    }));
}

/// The lengths of the scale bars of `o`: the template's, else round ones
/// about 40 mm along and 20 mm up.
pub(crate) fn scale_lengths(t: &SheetTemplate, o: &Options) -> (f64, f64) {
    let pick = |k: f64, mm: f64| crate::nice_step(mm / k, 1.0);
    let h = pick(1000.0 / o.horizontal_scale, 40.0);
    let v = pick(1000.0 / o.vertical_scale, 20.0);
    (
        t.scale_bar.lengths.first().map_or(h, |l| l.0),
        t.scale_bar.lengths.get(1).map_or(v, |l| l.0),
    )
}

/// Draws the scale bars in the box `r` (the bottom row).
pub(crate) fn scale_bars_in(items: &mut Vec<Item>, t: &SheetTemplate, o: &Options, r: Rect) {
    let (hl, vl) = scale_lengths(t, o);
    scale_bar(
        items,
        [r[0] + 4.0, r[1] + 6.0],
        hl,
        1000.0 / o.horizontal_scale,
        &format!(
            "1:{}",
            crate::template::format_number(o.horizontal_scale, "")
        ),
    );
    vertical_scale_bar(
        items,
        [r[0] + r[2] - 8.0, r[1] + r[3] - 3.0],
        vl,
        1000.0 / o.vertical_scale,
    );
}

/// Draws the key map into `r`: the route (or a window around the sheet)
/// north up at a small scale, the sheet's stretch thick and boxed, the
/// corridor's edges, the structures and their names on request, a north
/// arrow.
pub(crate) fn key_map(
    items: &mut Vec<Item>,
    t: &SheetTemplate,
    r: Rect,
    d: &RouteDrawing,
    o: &Options,
    s0: f64,
    s1: f64,
) {
    let spec = &t.key_map;
    let frame_pen = t.pen(Layer::KeyMap, Pen::new(Color::BLACK, 0.25));
    items.push(rect_path(Layer::KeyMap, r, frame_pen));
    if d.centreline.len() < 2 {
        return;
    }
    // Stations along the centreline: (station, x, y).
    let mut s: f64 = 0.0;
    let mut centre: Vec<[f64; 3]> = Vec::new();
    for (i, p) in d.centreline.iter().enumerate() {
        if i > 0 {
            let q = d.centreline[i - 1];
            s += (p[0] - q[0]).hypot(p[1] - q[1]);
        }
        centre.push([s, p[0], p[1]]);
    }
    let sheet: Vec<[f64; 3]> = crate::layout::station_runs(&centre, s0, s1)
        .into_iter()
        .flatten()
        .collect();
    let extent_of = |pts: &[[f64; 3]]| {
        pts.iter().fold(
            [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ],
            |b, p| {
                [
                    b[0].min(p[1]),
                    b[1].min(p[2]),
                    b[2].max(p[1]),
                    b[3].max(p[2]),
                ]
            },
        )
    };
    let mut ext = if spec.extent == "sheet" && !sheet.is_empty() {
        let e = extent_of(&sheet);
        let m = spec.margin.0;
        [e[0] - m, e[1] - m, e[2] + m, e[3] + m]
    } else {
        extent_of(&centre)
    };
    let corridor = o.corridor.filter(|_| spec.corridor);
    if let Some(w) = corridor
        && spec.extent != "sheet"
    {
        ext = [ext[0] - w, ext[1] - w, ext[2] + w, ext[3] + w];
    }
    let inner = [r[0] + 2.0, r[1] + 2.0, r[2] - 4.0, r[3] - 4.0];
    let (ew, eh) = ((ext[2] - ext[0]).max(1.0), (ext[3] - ext[1]).max(1.0));
    let k = match spec.scale {
        Some(den) => 1000.0 / den,
        None => (inner[2] / ew).min(inner[3] / eh),
    };
    let cx = 0.5 * (ext[0] + ext[2]);
    let cy = 0.5 * (ext[1] + ext[3]);
    let map = |x: f64, y: f64| {
        [
            inner[0] + inner[2] / 2.0 + (x - cx) * k,
            inner[1] + inner[3] / 2.0 - (y - cy) * k,
        ]
    };
    let mut drawn = Vec::new();
    drawn.push(Item::Path(Path {
        layer: Layer::KeyMap,
        points: centre.iter().map(|p| map(p[1], p[2])).collect(),
        closed: false,
        pen: Pen::new(Color(0x55, 0x55, 0x55), 0.25),
        title: Some("route".into()),
    }));
    if let Some(w) = corridor {
        for side in [-1.0, 1.0] {
            let pts: Vec<[f64; 2]> = centre
                .iter()
                .map(|p| {
                    let q = ohl_line::route::point_at(&d.centreline, p[0], side * w);
                    map(q[0], q[1])
                })
                .collect();
            drawn.push(Item::Path(Path {
                layer: Layer::KeyMap,
                points: pts,
                closed: false,
                // Each edge in its pen (near, far), as in the plan.
                pen: t
                    .corridor
                    .edge_pen(
                        side * w,
                        Pen::new(Color(0x88, 0x88, 0x88), 0.13).dashed("1 1"),
                    )
                    .0,
                title: Some("corridor".into()),
            }));
        }
    }
    let hl = spec.highlight.over(Pen::new(Color(0xcc, 0x22, 0x22), 0.7));
    if sheet.len() > 1 {
        drawn.push(Item::Path(Path {
            layer: Layer::KeyMap,
            points: sheet.iter().map(|p| map(p[1], p[2])).collect(),
            closed: false,
            pen: hl.clone(),
            title: Some("this sheet".into()),
        }));
        let e = extent_of(&sheet);
        let (a, b) = (map(e[0], e[3]), map(e[2], e[1]));
        drawn.push(rect_path(
            Layer::KeyMap,
            [a[0] - 1.0, a[1] - 1.0, b[0] - a[0] + 2.0, b[1] - a[1] + 2.0],
            Pen::new(hl.color, 0.2),
        ));
    }
    for m in &d.structures {
        let p = map(m.xy[0], m.xy[1]);
        drawn.push(Item::Circle(Circle {
            layer: Layer::KeyMap,
            centre: p,
            radius: 0.35,
            stroke: None,
            fill: Some(Color(0x33, 0x33, 0x33)),
            title: None,
        }));
        if spec.structures {
            drawn.push(Item::Text(Text {
                layer: Layer::KeyMap,
                at: [p[0] + 0.8, p[1] - 0.8],
                text: m.id.clone(),
                size: 1.5,
                anchor: Anchor::Start,
                bold: false,
                color: Color::BLACK,
            }));
        }
    }
    items.push(Item::Clip(Clip {
        rect: inner,
        items: drawn,
    }));
    if spec.north_arrow {
        north_arrow(
            items,
            [r[0] + r[2] - 5.0, r[1] + r[3] - 9.0],
            [0.0, -1.0],
            6.0,
            Layer::KeyMap,
        );
    }
    items.push(Item::Text(Text {
        layer: Layer::KeyMap,
        at: [r[0] + 1.5, r[1] + r[3] - 1.5],
        text: format!(
            "1:{}",
            crate::template::format_number((1000.0 / k).round(), "")
        ),
        size: 1.6,
        anchor: Anchor::Start,
        bold: false,
        color: Color::BLACK,
    }));
}

/// Draws the band rows of template `t` under the profile of frame `f`,
/// from `top`; their labels in the room left of the panels. `sheet`: the
/// sheet's number (per-sheet fields).
pub(crate) fn bands(
    items: &mut Vec<Item>,
    t: &SheetTemplate,
    f: &Frame,
    d: &RouteDrawing,
    o: &Options,
    top: f64,
    sheet: usize,
) {
    let color = Color::hex_or_black(&t.text.colour);
    let pen = t.pen(Layer::Bands, Pen::new(Color(0x88, 0x88, 0x88), 0.25));
    let tick = Pen::new(Color(0x88, 0x88, 0x88), 0.13);
    let size = t.text.size.0 * 0.8;
    let mut y = top;
    let within = |s: f64| s >= f.s0 - 1e-6 && s <= f.s1 + 1e-6;
    // Structures on the line on this sheet, in station order.
    let mut on: Vec<&crate::Mark> = d.structures.iter().filter(|m| within(m.station)).collect();
    on.sort_by(|a, b| a.station.total_cmp(&b.station));
    let run: Vec<&crate::Mark> = on.iter().copied().filter(|m| !m.away).collect();
    let ground_at = |s: f64| {
        d.ground.iter().find_map(|r| {
            r.windows(2).find_map(|w| {
                (s >= w[0][0] - 1e-9 && s <= w[1][0] + 1e-9).then(|| {
                    let k = if w[1][0] > w[0][0] {
                        (s - w[0][0]) / (w[1][0] - w[0][0])
                    } else {
                        0.0
                    };
                    w[0][1] + k * (w[1][1] - w[0][1])
                })
            })
        })
    };
    for b in &t.bands {
        let h = b.height.0;
        let row = [f.left, y, f.width(), h];
        items.push(rect_path(Layer::Bands, row, pen.clone()));
        items.push(Item::Text(Text {
            layer: Layer::Bands,
            at: [f.left - 1.5, y + h / 2.0 + 0.36 * size],
            text: b.label.clone(),
            size,
            anchor: Anchor::End,
            bold: false,
            color,
        }));
        let dec = b.decimals.unwrap_or(1);
        let mid = y + h / 2.0;
        // A value written across the row (turned when the row is tall
        // enough is not needed: written level, centred).
        let put = |items: &mut Vec<Item>, s: f64, t: String, ticked: bool| {
            let x = f.x(s);
            if ticked {
                items.push(line(Layer::Bands, [x, y], [x, y + h], tick.clone()));
            }
            // At the row's ends the value stays inside it.
            let (at, anchor) = if x < f.left + 6.0 {
                (x + 0.8, Anchor::Start)
            } else if x > f.left + f.width() - 6.0 {
                (x - 0.8, Anchor::End)
            } else {
                (x, Anchor::Middle)
            };
            items.push(Item::Text(Text {
                layer: Layer::Bands,
                at: [at, mid + 0.36 * size],
                text: t,
                size,
                anchor,
                bold: false,
                color,
            }));
        };
        match b.kind {
            BandKind::Structures => {
                let format = b.format.as_deref().unwrap_or("{name} {type}");
                for m in &on {
                    let lines = structure_entry(format, m, dec);
                    let x = f.x(m.station);
                    items.push(line(Layer::Bands, [x, y], [x, y + h], tick.clone()));
                    // Two lines (a line break in the format) one above
                    // the other, centred in the row.
                    let n = lines.len() as f64;
                    for (k, l) in lines.into_iter().enumerate() {
                        let dy = (k as f64 - 0.5 * (n - 1.0)) * 1.2 * size;
                        let (at, anchor) = if x < f.left + 6.0 {
                            (x + 0.8, Anchor::Start)
                        } else if x > f.left + f.width() - 6.0 {
                            (x - 0.8, Anchor::End)
                        } else {
                            (x, Anchor::Middle)
                        };
                        items.push(Item::Text(Text {
                            layer: Layer::Bands,
                            at: [at, mid + dy + 0.36 * size],
                            text: l,
                            size,
                            anchor,
                            bold: false,
                            color,
                        }));
                    }
                }
            }
            BandKind::Stations => {
                for m in &on {
                    put(items, m.station, format_station(m.station), true);
                }
            }
            BandKind::GroundHeight => {
                let mut at: Vec<f64> = on.iter().map(|m| m.station).collect();
                if let Some(every) = b.every.filter(|e| e.0 > 0.0) {
                    let first = (f.s0 / every.0).ceil();
                    let mut k = 0.0;
                    while (first + k) * every.0 <= f.s1 + 1e-9 && k < 10_000.0 {
                        let s = (first + k) * every.0;
                        // Not on top of a structure's value.
                        if at.iter().all(|a| (a - s).abs() * f.h > 6.0) {
                            at.push(s);
                        }
                        k += 1.0;
                    }
                }
                for s in at {
                    if let Some(z) = ground_at(s) {
                        put(items, s, format!("{z:.dec$}"), true);
                    }
                }
            }
            BandKind::SpanLength => {
                for w in run.windows(2) {
                    put(
                        items,
                        0.5 * (w[0].station + w[1].station),
                        format!("{:.dec$}", w[1].station - w[0].station),
                        false,
                    );
                    let x = f.x(w[0].station);
                    items.push(line(Layer::Bands, [x, y], [x, y + h], tick.clone()));
                }
            }
            BandKind::StructureHeight => {
                for m in &on {
                    let top = m
                        .members
                        .iter()
                        .chain(&m.pole)
                        .chain(&m.sticks)
                        .flatten()
                        .map(|p| p[2])
                        .fold(m.top, f64::max);
                    put(items, m.station, format!("{:.dec$}", top - m.base), true);
                }
            }
            BandKind::LineAngle => {
                // The deflection at each alignment point inside the
                // centreline, degrees (right positive).
                let mut s = 0.0;
                let pts = &d.centreline;
                for i in 1..pts.len().saturating_sub(1) {
                    s += (pts[i][0] - pts[i - 1][0]).hypot(pts[i][1] - pts[i - 1][1]);
                    if !within(s) {
                        continue;
                    }
                    let a = (pts[i][1] - pts[i - 1][1]).atan2(pts[i][0] - pts[i - 1][0]);
                    let b2 = (pts[i + 1][1] - pts[i][1]).atan2(pts[i + 1][0] - pts[i][0]);
                    let mut dev = (a - b2).to_degrees();
                    while dev > 180.0 {
                        dev -= 360.0;
                    }
                    while dev < -180.0 {
                        dev += 360.0;
                    }
                    put(items, s, format!("{dev:+.dec$}°"), true);
                }
            }
            BandKind::LowestWireHeight => {
                for w in run.windows(2) {
                    let (a, bb) = (w[0].station, w[1].station);
                    let low = d
                        .wires
                        .iter()
                        .filter(|t| t.dash.is_none())
                        .flat_map(|t| t.points.iter())
                        .filter(|p| p[0] > a + 1e-6 && p[0] < bb - 1e-6)
                        .min_by(|p, q| p[2].total_cmp(&q[2]));
                    if let Some(p) = low {
                        put(items, p[0], format!("{:.dec$}", p[2]), true);
                    }
                }
            }
            BandKind::GroundClearance => {
                for (s, c) in d.clearances.iter().filter(|(s, _)| within(*s)) {
                    put(items, *s, format!("{c:.dec$}"), true);
                }
            }
            BandKind::Text => {
                let key = b.field.as_deref().unwrap_or("");
                let value = o
                    .fields
                    .per_sheet
                    .get(key)
                    .and_then(|m| m.get(&sheet))
                    .or_else(|| o.fields.values.get(key))
                    .cloned()
                    .or_else(|| {
                        t.fields
                            .iter()
                            .find(|x| x.key == key)
                            .map(|x| x.default.clone())
                    })
                    .unwrap_or_default();
                items.push(Item::Text(Text {
                    layer: Layer::Bands,
                    at: [f.left + 2.0, mid + 0.36 * size],
                    text: value,
                    size,
                    anchor: Anchor::Start,
                    bold: false,
                    color,
                }));
            }
        }
        y += h;
    }
}

/// The entry of structure `m` in a structures band row, by `format`
/// (`{name}`, `{type}`, `{type_id}`, `{extension}`, `{station}`,
/// `{height}` with `dec` decimals): its lines, what is not known left out
/// (spaces closed up), empty lines dropped.
pub(crate) fn structure_entry(format: &str, m: &crate::Mark, dec: usize) -> Vec<String> {
    let top = m
        .members
        .iter()
        .chain(&m.pole)
        .chain(&m.sticks)
        .flatten()
        .map(|p| p[2])
        .fold(m.top, f64::max);
    let text = fill(format, &|n, _| {
        Some(match n {
            "name" => m.id.clone(),
            "type" => m.type_name.clone().unwrap_or_default(),
            "type_id" => m.type_id.clone().unwrap_or_default(),
            "extension" => m.extension.clone().unwrap_or_default(),
            "station" => format_station(m.station),
            "height" => format!("{:.dec$}", top - m.base),
            _ => return None,
        })
    });
    text.split('\n')
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect()
}

/// The height of the template's band rows, mm (none when off).
pub(crate) fn bands_height(t: &SheetTemplate, o: &Options) -> f64 {
    if o.bands {
        t.bands.iter().map(|b| b.height.0).sum()
    } else {
        0.0
    }
}
