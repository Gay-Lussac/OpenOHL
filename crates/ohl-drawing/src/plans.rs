// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The plans beside the plan-and-profile sheets (owner, 2026-10-23/24,
//! roadmap O14). The **phase plan** (redrawn after a utility's, owner
//! 2026-10-25): every wire a track along the page at its place across the
//! line, in its phase's colour and its circuit's dashes; each run of
//! structures with the same arrangement one column with the structure
//! seen from above; tracks crossing where the phases change places,
//! turning away where a circuit leaves or joins; the circuits' table.
//! Taken from the line as it is drawn (its wires' circuits and phases at
//! the structures): nothing typed twice.

use crate::Page;
use crate::furniture;
use crate::layout::{framed_page, plan_path, plan_text};
use crate::model::{Anchor, Item, Layer};
use crate::{Options, RouteDrawing, Sheet};

/// A wire's position at a structure.
#[derive(Debug, Clone, PartialEq)]
pub struct WirePosition {
    /// Across the line from the structure's axis, m (positive to the
    /// right looking ahead).
    pub offset: f64,
    /// Above the structure's base, m.
    pub height: f64,
    /// The circuit.
    pub circuit: String,
    /// The phase label (empty: none given).
    pub phase: String,
    /// The wire's colour.
    pub color: String,
    /// An earth wire (no voltage).
    pub earth: bool,
}

/// What hangs on one structure, seen along the line.
#[derive(Debug, Clone, PartialEq)]
pub struct Arrangement {
    /// The structure.
    pub structure: String,
    /// Its station.
    pub station: f64,
    /// Its stick figure (or members): polylines of (offset, height), m.
    pub figure: Vec<Vec<[f64; 2]>>,
    /// The wire positions, highest first, then left to right.
    pub wires: Vec<WirePosition>,
}

impl Arrangement {
    /// What the arrangement is, regardless of the structure's size: for
    /// each wire its level (from the top, heights within 1 m one level),
    /// its place on the level from the left, its circuit and phase.
    pub fn signature(&self) -> Vec<(usize, usize, String, String)> {
        let mut levels: Vec<f64> = Vec::new();
        for w in &self.wires {
            if !levels.iter().any(|z| (z - w.height).abs() < 1.0) {
                levels.push(w.height);
            }
        }
        levels.sort_by(|a, b| b.total_cmp(a));
        let level = |h: f64| levels.iter().position(|z| (z - h).abs() < 1.0).unwrap_or(0);
        let mut out = Vec::new();
        for (k, _) in levels.iter().enumerate() {
            let mut on: Vec<&WirePosition> =
                self.wires.iter().filter(|w| level(w.height) == k).collect();
            on.sort_by(|a, b| a.offset.total_cmp(&b.offset));
            for (p, w) in on.into_iter().enumerate() {
                out.push((k, p, w.circuit.clone(), w.phase.clone()));
            }
        }
        out
    }
}

/// The arrangement at every structure on the line (not those beside it),
/// in station order: the wires drawn solid (the case shown) leaving it
/// ahead along the route (the last structure: those arriving), their
/// clamps' positions relative to its axis and base.
pub fn arrangements(d: &RouteDrawing) -> Vec<Arrangement> {
    let mut marks: Vec<&crate::Mark> = d.structures.iter().filter(|m| !m.away).collect();
    marks.sort_by(|a, b| a.station.total_cmp(&b.station));
    let n = marks.len();
    marks
        .into_iter()
        .enumerate()
        .map(|(idx, m)| {
            let ahead = idx + 1 < n;
            let rel = |p: &[f64; 3]| [p[1] - m.offset, p[2] - m.base];
            let mut wires: Vec<WirePosition> = Vec::new();
            for t in d.wires.iter().filter(|t| t.dash.is_none()) {
                let (Some(first), Some(last)) = (t.points.first(), t.points.last()) else {
                    continue;
                };
                // A span: not a jumper (at a dead end with long strings
                // a jumper reaches metres along the line).
                if t.label.starts_with("jumper") || (last[0] - first[0]).abs() < 1.0 {
                    continue;
                }
                for (end, other) in [(first, last), (last, first)] {
                    // Its clamp: at a dead end a few metres along the line
                    // from the structure (its tension string).
                    if (end[0] - m.station).abs() > NEAR {
                        continue;
                    }
                    // The span ahead (the last structure: behind).
                    if (other[0] > end[0]) != ahead {
                        continue;
                    }
                    let [offset, height] = rel(end);
                    // One wire once (a strain structure's two clamps of a
                    // phase a few decimetres apart: one).
                    let same = wires.iter().any(|w| {
                        (w.offset - offset).abs() < 0.5
                            && (w.height - height).abs() < 1.0
                            && w.circuit == t.circuit
                            && w.phase == t.phase
                    });
                    if !same {
                        wires.push(WirePosition {
                            offset,
                            height,
                            circuit: t.circuit.clone(),
                            phase: t.phase.clone(),
                            color: t.color.clone(),
                            earth: t.voltage_kv.is_none_or(|v| v <= 0.0),
                        });
                    }
                }
            }
            wires.sort_by(|a, b| {
                b.height
                    .total_cmp(&a.height)
                    .then(a.offset.total_cmp(&b.offset))
            });
            let body = if m.members.is_empty() {
                &m.sticks
            } else {
                &m.members
            };
            let figure = body.iter().map(|l| l.iter().map(rel).collect()).collect();
            Arrangement {
                structure: m.id.clone(),
                station: m.station,
                figure,
                wires,
            }
        })
        .collect()
}

/// One column of the phase plan: a run of structures in a row with the
/// same arrangement (one structure, or "first–last"), drawn as its first.
#[derive(Debug, Clone, PartialEq)]
pub struct PhaseColumn {
    /// The first structure (index in the arrangements).
    pub first: usize,
    /// The last.
    pub last: usize,
}

/// The columns of the phase plan of `a`: each run of structures in a row
/// whose arrangements are the same ([`Arrangement::signature`]) one
/// column; a change (a transposition, a circuit leaving or joining,
/// another arm) starts the next.
pub fn phase_columns(a: &[Arrangement]) -> Vec<PhaseColumn> {
    let mut out: Vec<PhaseColumn> = Vec::new();
    for (k, x) in a.iter().enumerate() {
        match out.last_mut() {
            Some(c) if a[c.first].signature() == x.signature() => c.last = k,
            _ => out.push(PhaseColumn { first: k, last: k }),
        }
    }
    out
}

/// How far along the line a wire's end may be from its structure, m (a
/// dead end's clamp at the end of its tension string; spans are longer).
const NEAR: f64 = 15.0;

/// A column's width on paper (the structure's figure), mm.
const COLUMN: f64 = 14.0;

/// The space between two columns (the spans, where tracks cross), mm.
const GAP: f64 = 24.0;

/// The room at each end of a page (the tracks' ends, their labels), mm.
const END: f64 = 40.0;

/// The least distance between two tracks, mm.
const LANE: f64 = 2.4;

/// The phases' colours, by the phase label's place among its circuit's.
const PHASE_COLORS: [&str; 3] = ["#d62728", "#2ca02c", "#1f5fd6"];

/// The earth wires' colour.
const EARTH_COLOR: &str = "#444444";

/// The dash patterns of the circuits' tracks, by the circuit's place in
/// the table (repeating), mm.
const CIRCUIT_DASHES: [&[f64]; 6] = [
    &[],
    &[3.0, 1.2],
    &[4.0, 1.0, 1.0, 1.0],
    &[1.0, 0.8],
    &[6.0, 1.5],
    &[4.0, 1.0, 1.0, 1.0, 1.0, 1.0],
];

/// The y of each wire of `x` on paper, mm from the axis (positive: to the
/// right looking ahead, down the page): its offset at `k` mm per m; wires
/// above one another (offsets within 0.75 m) spread about their mean,
/// the highest first, and every two tracks at least [`LANE`] apart.
fn lanes(x: &Arrangement, k: f64) -> Vec<f64> {
    let n = x.wires.len();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| {
        x.wires[i]
            .offset
            .total_cmp(&x.wires[j].offset)
            .then(x.wires[j].height.total_cmp(&x.wires[i].height))
    });
    let mut y = vec![0.0; n];
    let mut g = 0;
    while g < n {
        let mut e = g + 1;
        while e < n && x.wires[order[e]].offset - x.wires[order[e - 1]].offset < 0.75 {
            e += 1;
        }
        let mut group: Vec<usize> = order[g..e].to_vec();
        // The highest first: on the side the group leans to, outward.
        group.sort_by(|&i, &j| x.wires[j].height.total_cmp(&x.wires[i].height));
        let mean = group.iter().map(|&i| x.wires[i].offset).sum::<f64>() / group.len() as f64;
        let m = group.len() as f64;
        for (r, &i) in group.iter().enumerate() {
            y[i] = mean * k + (r as f64 - (m - 1.0) / 2.0) * LANE;
        }
        g = e;
    }
    // At least a lane apart, pushed outward from the middle.
    let mut by: Vec<usize> = (0..n).collect();
    by.sort_by(|&i, &j| y[i].total_cmp(&y[j]));
    for w in 1..n {
        let (a, b) = (by[w - 1], by[w]);
        if y[b] - y[a] < LANE {
            y[b] = y[a] + LANE;
        }
    }
    y
}

/// Each wire's track key at a structure: its circuit, phase and which of
/// the wires of that circuit and phase it is (from the left).
fn track_keys(x: &Arrangement) -> Vec<(String, String, usize)> {
    let mut order: Vec<usize> = (0..x.wires.len()).collect();
    order.sort_by(|&i, &j| x.wires[i].offset.total_cmp(&x.wires[j].offset));
    let mut keys = vec![(String::new(), String::new(), 0); x.wires.len()];
    let mut seen: Vec<(String, String)> = Vec::new();
    for i in order {
        let w = &x.wires[i];
        let id = (w.circuit.clone(), w.phase.clone());
        let n = seen.iter().filter(|s| **s == id).count();
        seen.push(id.clone());
        keys[i] = (id.0, id.1, n);
    }
    keys
}

/// A track's pen: the wire's colour, its circuit's dashes.
fn track_pen(color: &str, dash: &[f64], width: f64) -> crate::model::Pen {
    crate::model::Pen {
        color: crate::model::Color::hex_or_black(color),
        width,
        dash: (!dash.is_empty()).then(|| dash.to_vec()),
    }
}

fn track(items: &mut Vec<Item>, points: Vec<[f64; 2]>, pen: crate::model::Pen) {
    items.push(Item::Path(crate::model::Path {
        layer: Layer::Wires,
        points,
        closed: false,
        pen,
        title: None,
    }));
}

/// An open arrowhead at `tip`, pointing away from `from`.
fn arrow(items: &mut Vec<Item>, from: [f64; 2], tip: [f64; 2], color: &str) {
    let (dx, dy) = (tip[0] - from[0], tip[1] - from[1]);
    let l = dx.hypot(dy).max(1e-9);
    let (ux, uy) = (dx / l, dy / l);
    let (a, w) = (1.6, 0.8);
    for s in [-1.0, 1.0] {
        items.push(plan_path(
            Layer::Wires,
            vec![
                [tip[0] - ux * a - uy * w * s, tip[1] - uy * a + ux * w * s],
                tip,
            ],
            color,
            0.25,
        ));
    }
}

/// The phase plan of route `d` on pages of `page` (the template's frame
/// and title block), after a utility's phase plan (owner, 2026-10-25):
/// every wire a track along the page in its phase's colour and its
/// circuit's dashes, at its place across the line; each run of
/// structures with the same arrangement one column (its id, or
/// "first–last") with the structure seen from above — an arm per level
/// across the tracks, a dot where each wire hangs, the earth at its foot;
/// tracks crossing between columns where the phases change places; a
/// circuit leaving or joining the line a track turning away with an
/// arrow and its name; at the line's ends each track's phase and its
/// circuit's name. The table of the circuits (owner, line type, name,
/// number; the owner and number when given) under the tracks. A
/// schematic: drawn in the case shown.
pub fn phase_plan(d: &RouteDrawing, o: &Options, page: &Page) -> Vec<Sheet> {
    let mut a = arrangements(d);
    // The phases' colours, as phase plans have them: each circuit's
    // phase labels in order red, green, blue (then again); the earth
    // wires dark grey.
    let mut labels: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for w in a.iter().flat_map(|x| &x.wires) {
        let l = labels.entry(w.circuit.clone()).or_default();
        if !l.contains(&w.phase) {
            l.push(w.phase.clone());
        }
    }
    for l in labels.values_mut() {
        l.sort();
    }
    for w in a.iter_mut().flat_map(|x| &mut x.wires) {
        w.color = if w.earth {
            EARTH_COLOR.to_owned()
        } else {
            let k = labels[&w.circuit]
                .iter()
                .position(|p| *p == w.phase)
                .unwrap_or(0);
            PHASE_COLORS[k % PHASE_COLORS.len()].to_owned()
        };
    }
    let columns = phase_columns(&a);
    if columns.is_empty() {
        return Vec::new();
    }
    let t = &o.template;
    let size = 1.8;
    // The circuits drawn, in the line's order (then as they appear).
    let mut circuits: Vec<crate::CircuitInfo> = Vec::new();
    for c in &d.circuits {
        if a.iter().flat_map(|x| &x.wires).any(|w| w.circuit == c.name) {
            circuits.push(c.clone());
        }
    }
    for w in a.iter().flat_map(|x| &x.wires) {
        if !w.circuit.is_empty() && !circuits.iter().any(|c| c.name == w.circuit) {
            circuits.push(crate::CircuitInfo {
                name: w.circuit.clone(),
                ..Default::default()
            });
        }
    }
    let dash_of = |c: &str| -> &'static [f64] {
        circuits
            .iter()
            .position(|x| x.name == c)
            .map_or(&[], |k| CIRCUIT_DASHES[k % CIRCUIT_DASHES.len()])
    };
    // The table of the circuits: its columns (the owner's and the
    // number's when any is given) and height.
    let owners = circuits.iter().any(|c| !c.owner.is_empty());
    let numbers = circuits.iter().any(|c| !c.number.is_empty());
    let row_h = 5.0;
    let table_h = if circuits.is_empty() {
        0.0
    } else {
        7.0 + row_h * (circuits.len() + 1) as f64
    };
    let (_, room, _) = framed_page(
        d,
        o,
        page,
        (1, 1),
        (0.0, d.length),
        (String::new(), String::new()),
        ("–", "plan_phase"),
    );
    // The band of the tracks: under the structures' ids, over the table.
    let band = [room[1] + 12.0, room[1] + room[3] - table_h - 8.0];
    let middle = (band[0] + band[1]) / 2.0;
    let widest = a
        .iter()
        .flat_map(|x| &x.wires)
        .map(|w| w.offset.abs())
        .fold(1.0f64, f64::max);
    let k = ((band[1] - band[0]) / 2.0 - 6.0).max(5.0) / widest;
    let k = k.min(6.0);
    let ys: Vec<Vec<f64>> = a.iter().map(|x| lanes(x, k)).collect();
    let keys: Vec<Vec<(String, String, usize)>> = a.iter().map(track_keys).collect();
    // Columns per page.
    let per_page = (((room[2] - 2.0 * END + GAP) / (COLUMN + GAP)).floor() as usize).max(1);
    let pages = columns.len().div_ceil(per_page);
    let label_of = |c: &PhaseColumn| {
        if c.first == c.last {
            a[c.first].structure.clone()
        } else {
            format!("{}–{}", a[c.first].structure, a[c.last].structure)
        }
    };
    let mut out = Vec::new();
    for page_no in 0..pages {
        let on = &columns[page_no * per_page..((page_no + 1) * per_page).min(columns.len())];
        let first = &a[on[0].first];
        let last = &a[on[on.len() - 1].last];
        let (mut drawing, _, _) = framed_page(
            d,
            o,
            page,
            (page_no + 1, pages),
            (first.station, last.station),
            (first.structure.clone(), last.structure.clone()),
            ("–", "plan_phase"),
        );
        let items = &mut drawing.items;
        let x_of = |j: usize| room[0] + END + j as f64 * (COLUMN + GAP) + COLUMN / 2.0;
        let line_start = page_no == 0;
        let line_end = page_no + 1 == pages;
        for (j, c) in on.iter().enumerate() {
            let xc = x_of(j);
            let (xl, xr) = (xc - COLUMN / 2.0, xc + COLUMN / 2.0);
            let x = &a[c.first];
            let y = |i: usize| middle + ys[c.first][i];
            // The tracks through the column.
            for (i, w) in x.wires.iter().enumerate() {
                track(
                    items,
                    vec![[xl, y(i)], [xr, y(i)]],
                    track_pen(&w.color, dash_of(&w.circuit), 0.3),
                );
            }
            // To the next column: matched by track key; the others turn
            // away (leaving) or come in (joining).
            let next = on.get(j + 1).map(|n| (n, x_of(j + 1)));
            let ahead = columns.get(page_no * per_page + j + 1);
            let mut turned: Vec<(String, [f64; 2])> = Vec::new();
            for (i, w) in x.wires.iter().enumerate() {
                let key = &keys[c.first][i];
                let to = ahead.and_then(|n| keys[n.first].iter().position(|q| q == key));
                let pen = track_pen(&w.color, dash_of(&w.circuit), 0.3);
                match (to, next) {
                    (Some(q), Some((n, xn))) => track(
                        items,
                        vec![[xr, y(i)], [xn - COLUMN / 2.0, middle + ys[n.first][q]]],
                        pen,
                    ),
                    // On the next page: to the page's edge.
                    (Some(_), None) => track(
                        items,
                        vec![[xr, y(i)], [room[0] + room[2] - 4.0, y(i)]],
                        pen,
                    ),
                    (None, _) if ahead.is_none() => {}
                    (None, _) => {
                        let s = if w.offset < 0.0 { -1.0 } else { 1.0 };
                        let tip = [xr + GAP * 0.55, y(i) + s * GAP * 0.45];
                        track(items, vec![[xr, y(i)], tip], pen);
                        arrow(items, [xr, y(i)], tip, &w.color);
                        turned.push((w.circuit.clone(), tip));
                    }
                }
            }
            // Wires joining here: coming in from beside the line.
            let before = (page_no * per_page + j).checked_sub(1).map(|p| &columns[p]);
            if let Some(b) = before {
                for (i, w) in x.wires.iter().enumerate() {
                    if keys[b.first].contains(&keys[c.first][i]) {
                        continue;
                    }
                    let s = if w.offset < 0.0 { -1.0 } else { 1.0 };
                    let tip = [xl - GAP * 0.55, y(i) + s * GAP * 0.45];
                    track(
                        items,
                        vec![tip, [xl, y(i)]],
                        track_pen(&w.color, dash_of(&w.circuit), 0.3),
                    );
                    arrow(items, [xl, y(i)], tip, &w.color);
                    turned.push((w.circuit.clone(), tip));
                }
                // From the previous page: from the page's edge.
                if j == 0 {
                    for (i, w) in x.wires.iter().enumerate() {
                        if keys[b.first].contains(&keys[c.first][i]) {
                            track(
                                items,
                                vec![[room[0] + 4.0, y(i)], [xl, y(i)]],
                                track_pen(&w.color, dash_of(&w.circuit), 0.3),
                            );
                        }
                    }
                }
            }
            // The names of the circuits turning away, once each, beyond
            // their arrows.
            let mut named: Vec<String> = Vec::new();
            for (circuit, _) in &turned {
                if named.contains(circuit) || circuit.is_empty() {
                    continue;
                }
                named.push(circuit.clone());
                let tips: Vec<[f64; 2]> = turned
                    .iter()
                    .filter(|(c, _)| c == circuit)
                    .map(|(_, p)| *p)
                    .collect();
                let cx = tips.iter().map(|p| p[0]).sum::<f64>() / tips.len() as f64;
                let below = tips.iter().map(|p| p[1]).sum::<f64>() / tips.len() as f64 > middle;
                let cy = if below {
                    tips.iter().map(|p| p[1]).fold(f64::MIN, f64::max) + size * 1.6
                } else {
                    tips.iter().map(|p| p[1]).fold(f64::MAX, f64::min) - size * 0.8
                };
                items.push(plan_text(
                    Layer::Labels,
                    [cx, cy],
                    size,
                    Anchor::Middle,
                    circuit.clone(),
                    false,
                ));
            }
            draw_column(items, x, &ys[c.first], [xl, middle]);
            // The column's structures, above the tracks.
            let label = label_of(c);
            for (n, line) in furniture::wrap(&label, size * 1.15, true, COLUMN + GAP - 4.0)
                .iter()
                .enumerate()
            {
                items.push(plan_text(
                    Layer::Structures,
                    [xc, room[1] + 5.0 + n as f64 * size * 1.5],
                    size * 1.15,
                    Anchor::Middle,
                    line.clone(),
                    true,
                ));
            }
        }
        // The line's ends (arrows, each track's phase, the circuits'
        // names) and the pages' joins (the phase only).
        let ends = [
            (true, &a[on[0].first], x_of(0) - COLUMN / 2.0, line_start),
            (
                false,
                &a[on[on.len() - 1].first],
                x_of(on.len() - 1) + COLUMN / 2.0,
                line_end,
            ),
        ];
        for (left, x, edge, at_line_end) in ends {
            let i0 = if left {
                on[0].first
            } else {
                on[on.len() - 1].first
            };
            let s = if left { -1.0 } else { 1.0 };
            let tip = edge + s * 8.0;
            for (i, w) in x.wires.iter().enumerate() {
                let yy = middle + ys[i0][i];
                if at_line_end {
                    track(
                        items,
                        vec![[edge, yy], [tip, yy]],
                        track_pen(&w.color, dash_of(&w.circuit), 0.3),
                    );
                    arrow(items, [edge, yy], [tip, yy], &w.color);
                }
                let at = if at_line_end {
                    tip + s * 1.2
                } else {
                    edge + s * 1.2
                };
                let at = if !at_line_end && !left {
                    room[0] + room[2] - 3.0
                } else if !at_line_end {
                    room[0] + 3.0
                } else {
                    at
                };
                if !w.phase.is_empty() {
                    items.push(plan_text(
                        Layer::Labels,
                        [at, yy + 0.35 * size],
                        size * 0.9,
                        if left { Anchor::End } else { Anchor::Start },
                        w.phase.clone(),
                        false,
                    ));
                }
            }
            if !at_line_end {
                continue;
            }
            // The circuits' names beyond their phases, at their tracks'
            // middle.
            let mut named: Vec<&str> = Vec::new();
            for w in &x.wires {
                if w.circuit.is_empty() || named.contains(&w.circuit.as_str()) {
                    continue;
                }
                named.push(&w.circuit);
                let mine: Vec<f64> = x
                    .wires
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| v.circuit == w.circuit)
                    .map(|(i, _)| middle + ys[i0][i])
                    .collect();
                let cy = mine.iter().sum::<f64>() / mine.len() as f64;
                let lines = furniture::wrap(&w.circuit, size, false, END - 14.0);
                for (n, line) in lines.iter().enumerate() {
                    items.push(plan_text(
                        Layer::Labels,
                        [
                            tip + s * 7.0,
                            cy + 0.35 * size
                                + (n as f64 - (lines.len() as f64 - 1.0) / 2.0) * size * 1.3,
                        ],
                        size,
                        if left { Anchor::End } else { Anchor::Start },
                        line.clone(),
                        false,
                    ));
                }
            }
        }
        // The table of the circuits.
        if !circuits.is_empty() {
            let mut cols: Vec<(&str, f64)> = Vec::new();
            if owners {
                cols.push(("phase_owner", 34.0));
            }
            cols.push(("phase_line_type", 22.0));
            cols.push(("phase_circuit", 62.0));
            if numbers {
                cols.push(("phase_number", 22.0));
            }
            let width: f64 = cols.iter().map(|c| c.1).sum();
            let x0 = room[0] + 2.0;
            let y0 = room[1] + room[3] - table_h;
            items.push(plan_text(
                Layer::Labels,
                [x0, y0 + 4.5],
                size * 1.2,
                Anchor::Start,
                t.legend.label("phase_circuits", &[]),
                true,
            ));
            let top = y0 + 7.0;
            let rows = circuits.len() + 1;
            for r in 0..=rows {
                let yy = top + r as f64 * row_h;
                items.push(plan_path(
                    Layer::Labels,
                    vec![[x0, yy], [x0 + width, yy]],
                    "#000",
                    if r <= 1 || r == rows { 0.3 } else { 0.15 },
                ));
            }
            let mut cx = x0;
            for (n, (key, w)) in cols.iter().enumerate() {
                items.push(plan_path(
                    Layer::Labels,
                    vec![[cx, top], [cx, top + rows as f64 * row_h]],
                    "#000",
                    if n == 0 { 0.3 } else { 0.15 },
                ));
                items.push(plan_text(
                    Layer::Labels,
                    [cx + 1.2, top + row_h - 1.5],
                    size,
                    Anchor::Start,
                    t.legend.label(key, &[]),
                    true,
                ));
                for (r, c) in circuits.iter().enumerate() {
                    let yy = top + (r + 1) as f64 * row_h;
                    let text = match *key {
                        "phase_owner" => c.owner.clone(),
                        "phase_circuit" => c.name.clone(),
                        "phase_number" => c.number.clone(),
                        _ => {
                            // A sample of its tracks: each phase's colour.
                            let mut colors: Vec<&str> = Vec::new();
                            for w in a.iter().flat_map(|x| &x.wires) {
                                if w.circuit == c.name && !colors.contains(&w.color.as_str()) {
                                    colors.push(&w.color);
                                }
                            }
                            let m = colors.len().max(1) as f64;
                            for (q, color) in colors.iter().enumerate() {
                                let ly = yy + row_h / 2.0 + (q as f64 - (m - 1.0) / 2.0) * 0.9;
                                track(
                                    items,
                                    vec![[cx + 2.0, ly], [cx + w - 2.0, ly]],
                                    track_pen(color, dash_of(&c.name), 0.3),
                                );
                            }
                            String::new()
                        }
                    };
                    if !text.is_empty() {
                        items.push(plan_text(
                            Layer::Labels,
                            [cx + 1.2, yy + row_h - 1.5],
                            size,
                            Anchor::Start,
                            text,
                            false,
                        ));
                    }
                }
                cx += w;
            }
            items.push(plan_path(
                Layer::Labels,
                vec![[cx, top], [cx, top + rows as f64 * row_h]],
                "#000",
                0.3,
            ));
        }
        out.push(Sheet {
            number: page_no + 1,
            from: first.station,
            to: last.station,
            structures: (first.structure.clone(), last.structure.clone()),
            page: page.clone(),
            drawing,
            warnings: Vec::new(),
        });
    }
    out
}

/// A column's structure seen from above, its left edge and the axis at
/// `at`: the earth at its foot (left), the body along the axis, an arm
/// per level (from the lowest, left, to the highest) across the tracks
/// of its wires, a dot where each wire hangs (`ys`: their tracks).
fn draw_column(items: &mut Vec<Item>, x: &Arrangement, ys: &[f64], at: [f64; 2]) {
    let mut levels: Vec<f64> = Vec::new();
    for w in &x.wires {
        if !levels.iter().any(|z| (z - w.height).abs() < 1.0) {
            levels.push(w.height);
        }
    }
    levels.sort_by(f64::total_cmp);
    let n = levels.len().max(1);
    let step = if n > 1 {
        ((COLUMN - 5.0) / (n - 1) as f64).min(2.5)
    } else {
        0.0
    };
    let foot = at[0] + 1.0;
    let x_of = |l: usize| at[0] + 3.5 + l as f64 * step;
    let level = |h: f64| levels.iter().position(|z| (z - h).abs() < 1.0).unwrap_or(0);
    // The earth: a bar across, hatched on its outer side.
    items.push(plan_path(
        Layer::Structures,
        vec![[foot, at[1] - 2.5], [foot, at[1] + 2.5]],
        "#000",
        0.35,
    ));
    for q in 0..4 {
        let yy = at[1] - 2.0 + q as f64 * 1.4;
        items.push(plan_path(
            Layer::Structures,
            vec![[foot, yy], [foot - 1.0, yy + 1.0]],
            "#000",
            0.2,
        ));
    }
    // The body, to the highest arm.
    items.push(plan_path(
        Layer::Structures,
        vec![[foot, at[1]], [x_of(n - 1), at[1]]],
        "#000",
        0.5,
    ));
    for l in 0..levels.len() {
        let on: Vec<f64> = x
            .wires
            .iter()
            .enumerate()
            .filter(|(_, w)| level(w.height) == l)
            .map(|(i, _)| at[1] + ys[i])
            .collect();
        let lo = on.iter().copied().fold(at[1], f64::min);
        let hi = on.iter().copied().fold(at[1], f64::max);
        items.push(plan_path(
            Layer::Structures,
            vec![[x_of(l), lo - 0.6], [x_of(l), hi + 0.6]],
            "#000",
            0.45,
        ));
    }
    for (i, w) in x.wires.iter().enumerate() {
        items.push(Item::Circle(crate::model::Circle {
            layer: Layer::Wires,
            centre: [x_of(level(w.height)), at[1] + ys[i]],
            radius: 0.65,
            stroke: None,
            fill: Some(crate::model::Color::hex_or_black(&w.color)),
            title: None,
        }));
    }
}

/// Which plan of the route on a map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapKind {
    /// The route plan: the whole route at a small scale (1:5000 to
    /// 1:25 000), on as few sheets as it needs.
    Route,
    /// Situation plans: the route in parts at a large scale (1:500 to
    /// 1:2000), with the parcels around the line.
    Situation,
}

impl MapKind {
    /// Its name in files: `route-plan` or `situation-plan`.
    pub fn file_name(self) -> &'static str {
        match self {
            MapKind::Route => "route-plan",
            MapKind::Situation => "situation-plan",
        }
    }

    /// Its legend label (the field `{plan_kind}`).
    pub fn label_key(self) -> &'static str {
        match self {
            MapKind::Route => "plan_route",
            MapKind::Situation => "plan_situation",
        }
    }

    /// The scales it is drawn at (1:…).
    pub fn scales(self) -> std::ops::RangeInclusive<f64> {
        match self {
            MapKind::Route => 5000.0..=25_000.0,
            MapKind::Situation => 500.0..=2000.0,
        }
    }
}

/// How a route or situation plan is drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct MapPlan {
    /// Which plan.
    pub kind: MapKind,
    /// Its scale (1:…).
    pub scale: f64,
    /// The corridor's half width drawn along the line, m (none: not).
    pub corridor: Option<f64>,
    /// The project's drawings under it (`map_background`) and its
    /// orthophotos (the route's photo).
    pub drawings: bool,
    /// The orthophotos under it, under a white veil (0 to 1).
    pub photo: Option<f64>,
    /// Each sheet overlaps the one before by this much of its width.
    pub overlap: f64,
}

impl MapPlan {
    /// The plan of `kind` at its usual scale (route 1:10 000, situation
    /// 1:1000), drawings and photo on, the corridor at `corridor` m.
    pub fn new(kind: MapKind, corridor: Option<f64>) -> Self {
        Self {
            kind,
            scale: match kind {
                MapKind::Route => 10_000.0,
                MapKind::Situation => 1000.0,
            },
            corridor,
            drawings: true,
            photo: Some(0.4),
            overlap: 0.1,
        }
    }
}

/// The centreline with no piece longer than `step`, m.
fn densified(c: &[[f64; 3]], step: f64) -> Vec<[f64; 2]> {
    let mut out: Vec<[f64; 2]> = Vec::new();
    for w in c.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = (b[0] - a[0]).hypot(b[1] - a[1]);
        let n = (len / step).ceil().max(1.0) as usize;
        for k in 0..n {
            let t = k as f64 / n as f64;
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
    }
    if let Some(l) = c.last() {
        out.push([l[0], l[1]]);
    }
    out
}

/// Where the sheets of a map go: their middles (x, y), m. The route is
/// walked and each sheet takes as much of it as fits within its room
/// (`w` × `h` m less a margin), the next starting `overlap` of its length
/// back.
fn map_frames(line: &[[f64; 2]], w: f64, h: f64, overlap: f64) -> Vec<[f64; 2]> {
    let (mw, mh) = (w * 0.9, h * 0.9);
    let mut out = Vec::new();
    let mut start = 0usize;
    while start + 1 < line.len() {
        let (mut lo, mut hi) = (line[start], line[start]);
        let mut end = start;
        while end + 1 < line.len() {
            let p = line[end + 1];
            let (nlo, nhi) = (
                [lo[0].min(p[0]), lo[1].min(p[1])],
                [hi[0].max(p[0]), hi[1].max(p[1])],
            );
            if nhi[0] - nlo[0] > mw || nhi[1] - nlo[1] > mh {
                break;
            }
            (lo, hi) = (nlo, nhi);
            end += 1;
        }
        out.push([(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0]);
        if end + 1 >= line.len() {
            break;
        }
        let back = (((end - start) as f64) * overlap).round() as usize;
        start = (end - back).max(start + 1);
    }
    out
}

/// A round length for the scale bar: the largest of 1, 2, 5 × 10ⁿ m at
/// most `max` mm long at `k` mm per m.
fn round_length(max: f64, k: f64) -> f64 {
    let mut best = 1.0;
    for e in 0..6 {
        for m in [1.0, 2.0, 5.0] {
            let l = m * 10f64.powi(e);
            if l * k <= max {
                best = l;
            }
        }
    }
    best
}

/// The route or situation plan of route `d` on pages of `page`: north
/// up at `m.scale`, the sheets along the route (overlapping), each with
/// the project's drawings and orthophotos as asked, the corridor, the
/// line's axis, its wires, its structures numbered, a north arrow, a
/// scale bar and a legend, in the template's frame and title block.
pub fn map_plan(d: &RouteDrawing, o: &Options, page: &Page, m: &MapPlan) -> Vec<Sheet> {
    if d.centreline.len() < 2 {
        return Vec::new();
    }
    let k = 1000.0 / m.scale; // mm per m
    let scale_text = format!("1:{}", crate::template::format_number(m.scale, ""));
    let (_, room, _) = framed_page(
        d,
        o,
        page,
        (1, 1),
        (0.0, d.length),
        (String::new(), String::new()),
        (&scale_text, m.kind.label_key()),
    );
    let (w, h) = (room[2] / k, room[3] / k);
    let line = densified(&d.centreline, w.min(h) / 8.0);
    let frames = map_frames(&line, w, h, m.overlap.clamp(0.0, 0.5));
    let n = frames.len();
    let t = &o.template;
    let size = 1.8;
    let mut out = Vec::new();
    for (q, c) in frames.iter().enumerate() {
        let mid = [room[0] + room[2] / 2.0, room[1] + room[3] / 2.0];
        let to = |p: [f64; 2]| [mid[0] + (p[0] - c[0]) * k, mid[1] - (p[1] - c[1]) * k];
        // The stations the sheet shows (for its title block).
        let inside = |p: [f64; 2]| crate::model::inside(to(p), room);
        let on: Vec<&crate::Mark> = d
            .structures
            .iter()
            .filter(|s| !s.away && inside(s.xy))
            .collect();
        let s0 = on.iter().map(|s| s.station).fold(f64::INFINITY, f64::min);
        let s1 = on
            .iter()
            .map(|s| s.station)
            .fold(f64::NEG_INFINITY, f64::max);
        let first = on.iter().min_by(|a, b| a.station.total_cmp(&b.station));
        let last = on.iter().max_by(|a, b| a.station.total_cmp(&b.station));
        let (mut drawing, _, _) = framed_page(
            d,
            o,
            page,
            (q + 1, n),
            (
                if s0.is_finite() { s0 } else { 0.0 },
                if s1.is_finite() { s1 } else { d.length },
            ),
            (
                first.map_or(String::new(), |s| s.id.clone()),
                last.map_or(String::new(), |s| s.id.clone()),
            ),
            (&scale_text, m.kind.label_key()),
        );
        let items = &mut drawing.items;
        let mut legend: Vec<(String, String)> = Vec::new();
        if let (Some(veil), Some(photo)) = (m.photo, &d.photo)
            && let Some(img) = crate::background::north_up_photo(
                photo,
                o,
                room,
                (c[0], c[1], k),
                veil,
                Layer::BackgroundPhoto,
            )
        {
            items.push(img);
            legend.push(("#bbb".into(), photo.name.clone()));
        }
        let clip = |pts: Vec<[f64; 2]>| crate::model::clip_polyline(&pts, room);
        if m.drawings {
            for dr in d.map_background.iter() {
                for (l, color) in &dr.lines {
                    for run in clip(l.iter().map(|p| to(*p)).collect()) {
                        items.push(Item::Path(crate::model::Path {
                            layer: Layer::BackgroundDrawings,
                            points: run,
                            closed: false,
                            pen: crate::model::Pen::new(*color, 0.13),
                            title: None,
                        }));
                    }
                }
                for (at, height, text, color) in &dr.texts {
                    let p = to(*at);
                    if crate::model::inside(p, room) {
                        items.push(Item::Text(crate::model::Text {
                            layer: Layer::BackgroundDrawings,
                            at: p,
                            text: text.clone(),
                            size: (height * k).clamp(1.2, 3.0),
                            anchor: Anchor::Start,
                            bold: false,
                            color: *color,
                        }));
                    }
                }
                legend.push(("#8c8c8c".into(), dr.name.clone()));
            }
        }
        // The corridor's edges either side of the axis.
        if let Some(half) = m.corridor.filter(|x| *x > 0.0) {
            for side in [-1.0, 1.0] {
                let edge: Vec<[f64; 2]> = line
                    .windows(2)
                    .map(|p| {
                        let (dx, dy) = (p[1][0] - p[0][0], p[1][1] - p[0][1]);
                        let l = dx.hypot(dy).max(1e-9);
                        to([
                            p[0][0] + side * half * dy / l,
                            p[0][1] - side * half * dx / l,
                        ])
                    })
                    .collect();
                for run in clip(edge) {
                    items.push(plan_path(Layer::Corridor, run, "#2a7", 0.25));
                }
            }
            legend.push((
                "#2a7".into(),
                t.legend.label(
                    "corridor_near",
                    &[
                        ("side", "±"),
                        ("width", &crate::template::format_number(half, "")),
                    ],
                ),
            ));
        }
        for run in clip(line.iter().map(|p| to(*p)).collect()) {
            items.push(plan_path(Layer::Centreline, run, "#000", 0.25));
        }
        legend.push(("#000".into(), t.legend.label("axis", &[])));
        for tr in d.wires.iter().filter(|x| x.dash.is_none()) {
            for run in clip(tr.xy.iter().map(|p| to(*p)).collect()) {
                items.push(plan_path(Layer::Wires, run, &tr.color, 0.18));
            }
        }
        for s in &d.structures {
            let p = to(s.xy);
            if !crate::model::inside(p, room) {
                continue;
            }
            let r = 0.9;
            items.push(plan_path(
                Layer::Structures,
                vec![
                    [p[0] - r, p[1] - r],
                    [p[0] + r, p[1] - r],
                    [p[0] + r, p[1] + r],
                    [p[0] - r, p[1] + r],
                    [p[0] - r, p[1] - r],
                ],
                "#000",
                0.3,
            ));
            items.push(plan_text(
                Layer::Labels,
                [p[0] + 1.6, p[1] - 1.4],
                size,
                Anchor::Start,
                s.id.clone(),
                true,
            ));
        }
        // North arrow (top right), scale bar (bottom left), legend.
        furniture::north_arrow(
            items,
            [room[0] + room[2] - 10.0, room[1] + 18.0],
            [0.0, -1.0],
            12.0,
            Layer::Labels,
        );
        let len = round_length(room[2] / 4.0, k);
        furniture::scale_bar(
            items,
            [room[0] + 6.0, room[1] + room[3] - 6.0],
            len,
            k,
            &format!("1:{}", crate::template::format_number(m.scale, "")),
        );
        let mut y = room[1] + room[3] - 16.0 - legend.len() as f64 * size * 1.6;
        for (color, name) in &legend {
            items.push(plan_path(
                Layer::Legend,
                vec![[room[0] + 6.0, y - 0.6], [room[0] + 14.0, y - 0.6]],
                color,
                0.5,
            ));
            items.push(plan_text(
                Layer::Legend,
                [room[0] + 16.0, y],
                size,
                Anchor::Start,
                name.clone(),
                false,
            ));
            y += size * 1.6;
        }
        out.push(Sheet {
            number: q + 1,
            from: if s0.is_finite() { s0 } else { 0.0 },
            to: if s1.is_finite() { s1 } else { d.length },
            structures: (
                first.map_or(String::new(), |s| s.id.clone()),
                last.map_or(String::new(), |s| s.id.clone()),
            ),
            page: page.clone(),
            drawing,
            warnings: Vec::new(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::main_route;

    /// The example line's phase plan: every structure on the line has its
    /// wires; each run with the same arrangement one column, every
    /// structure in one; a transposition starts a column; the sheets
    /// carry the structures' ids and the circuits' table.
    #[test]
    fn the_phase_plan_draws_runs_and_changes() {
        let d = main_route(&Options::default());
        let a = arrangements(&d);
        assert!(a.len() >= 3, "{}", a.len());
        assert!(a.iter().all(|x| !x.wires.is_empty()), "{a:?}");
        let cols = phase_columns(&a);
        assert_eq!(cols[0].first, 0);
        assert_eq!(cols.last().unwrap().last, a.len() - 1);
        for w in cols.windows(2) {
            assert_eq!(w[0].last + 1, w[1].first);
        }
        // The same arrangements throughout: one column.
        let same: Vec<Arrangement> = (0..4)
            .map(|k| Arrangement {
                structure: format!("T{k}"),
                ..a[0].clone()
            })
            .collect();
        assert_eq!(
            phase_columns(&same),
            vec![PhaseColumn { first: 0, last: 3 }]
        );
        // Two wires swapped at T2 (a transposition): T0–T1, T2, T3.
        let mut swapped = same.clone();
        let w = &mut swapped[2].wires;
        if w.len() >= 2 {
            let (p0, p1) = (w[0].phase.clone(), w[1].phase.clone());
            w[0].phase = p1;
            w[1].phase = p0;
            if w[0].phase != w[1].phase {
                assert_eq!(
                    phase_columns(&swapped),
                    vec![
                        PhaseColumn { first: 0, last: 1 },
                        PhaseColumn { first: 2, last: 2 },
                        PhaseColumn { first: 3, last: 3 },
                    ]
                );
            }
        }
        // Every track at least a lane apart.
        for x in &a {
            let mut y = lanes(x, 4.0);
            y.sort_by(f64::total_cmp);
            assert!(y.windows(2).all(|p| p[1] - p[0] >= LANE - 1e-9), "{y:?}");
        }
        let sheets = phase_plan(&d, &Options::default(), &crate::page_format("A3").unwrap());
        assert!(!sheets.is_empty());
        let svg: String = sheets.iter().map(Sheet::svg).collect();
        assert!(svg.contains(&a[0].structure), "the first's id");
        if !d.circuits.is_empty() {
            assert!(svg.contains(">Circuits<"), "the table");
            assert!(svg.contains(&d.circuits[0].name), "a circuit's name");
        }
    }

    /// The route and situation plans of the example line: every structure
    /// on a sheet, north up; the situation plan at 1:1000 needs more
    /// sheets than the route plan at 1:10 000; the sheets overlap; a
    /// drawing near the line is drawn, its text too.
    #[test]
    fn the_route_and_situation_plans_cover_the_line() {
        let mut d = main_route(&Options::default());
        let mut survey = ohl_background::Drawing::default();
        let p0 = d.structures[0].xy;
        survey.lines.push(ohl_background::dxf::Polyline {
            layer: 0,
            color: [10, 20, 30],
            points: vec![
                [p0[0] - 20.0, p0[1] - 20.0, 0.0],
                [p0[0] + 20.0, p0[1] + 20.0, 0.0],
            ],
            closed: false,
        });
        survey.texts.push(ohl_background::dxf::Label {
            layer: 0,
            color: [10, 20, 30],
            at: [p0[0] + 5.0, p0[1] + 5.0, 0.0],
            height: 2.0,
            angle_deg: 0.0,
            text: "Parcel 4711".into(),
        });
        d.add_map_drawing("survey", &survey, 60.0, 0.05);
        assert_eq!(d.map_background.len(), 1);
        let page = crate::page_format("A3").unwrap();
        let o = Options::default();
        let route = map_plan(&d, &o, &page, &MapPlan::new(MapKind::Route, Some(50.0)));
        let situation = map_plan(&d, &o, &page, &MapPlan::new(MapKind::Situation, Some(50.0)));
        assert!(!route.is_empty());
        assert!(
            situation.len() > route.len(),
            "{} {}",
            situation.len(),
            route.len()
        );
        for plan in [&route, &situation] {
            let svg: String = plan.iter().map(Sheet::svg).collect();
            for m in d.structures.iter().filter(|m| !m.away) {
                assert!(svg.contains(&format!(">{}<", m.id)), "{} missing", m.id);
            }
        }
        let first: String = situation[0].svg();
        assert!(first.contains(">Parcel 4711<"), "the drawing's text");
        // Its own scale in the title block, not the sheets'.
        assert!(first.contains(">1:1000<"), "the plan's scale");
    }
}
