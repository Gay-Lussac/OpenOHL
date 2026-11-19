// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The context around a working set on a network (country server, phase
//! 6): the tiles of the network's grid around the camera, as the server
//! packs them — per structure a stick, per section its wires solved in the
//! network's context case, the routes as polylines — read-only, kept in a
//! cache bounded in bytes (least recently used first, never a tile in
//! view), asked again when a change touched them.
//!
//! **A drawing, never a result**: the context's wires are each section
//! solved alone in the context case (15 °C still air by default, a network
//! setting): no jumper loads, no coupled assemblies, no flexibility. They
//! are drawn grey, never listed in a table, never used by a check (the
//! checks on a working set pull their neighbours in from the server,
//! phase 10). What the replica holds itself is not drawn again.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use eframe::egui;
use ohl_server::{HeldTile, TileKey, TilePackage};

/// Bytes of tile packages kept at most (an estimate of their size).
pub const CACHE_BYTES: usize = 64 << 20;

/// Tiles around the camera on every side (level 2: 1 024 m each, so a
/// view of about 5 km across, 25 tiles).
pub const RADIUS: i64 = 2;

/// The context's colours: structures, wires, routes (grey, faded).
pub const STRUCTURE: [u8; 4] = [150, 150, 150, 170];
pub const WIRE: [u8; 4] = [175, 175, 175, 140];
pub const ROUTE: [u8; 4] = [120, 145, 165, 150];

/// A tile kept.
struct Held {
    pkg: TilePackage,
    bytes: usize,
    used: u64,
}

/// The context's tiles.
#[derive(Default)]
pub struct ContextTiles {
    tiles: BTreeMap<TileKey, Held>,
    bytes: usize,
    clock: u64,
    /// Asked for last (in view).
    asked: BTreeSet<TileKey>,
    /// Changed on the server: asked again.
    stale: BTreeSet<TileKey>,
    /// Changes when a tile came or went (the drawing is made again).
    pub revision: u64,
    /// The weather case of the wires (from the packages).
    pub case: String,
}

impl std::fmt::Debug for ContextTiles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextTiles")
            .field("tiles", &self.tiles.len())
            .field("bytes", &self.bytes)
            .finish_non_exhaustive()
    }
}

/// A package's size, estimated (its points and texts).
fn size(p: &TilePackage) -> usize {
    256 + p
        .structures
        .iter()
        .map(|s| 96 + 48 * s.stick.len())
        .sum::<usize>()
        + p.wires
            .iter()
            .map(|w| {
                64 + w
                    .spans
                    .iter()
                    .map(|s| 48 + 24 * s.points.len())
                    .sum::<usize>()
            })
            .sum::<usize>()
        + p.routes
            .iter()
            .map(|r| 64 + 24 * r.points.len())
            .sum::<usize>()
}

impl ContextTiles {
    /// Tiles kept, bytes (estimated).
    pub fn size(&self) -> (usize, usize) {
        (self.tiles.len(), self.bytes)
    }

    /// A package came: kept (the oldest beyond [`CACHE_BYTES`] let go, never
    /// one asked for).
    pub fn put(&mut self, p: TilePackage) {
        self.clock += 1;
        if !p.case.is_empty() {
            self.case = p.case.clone();
        }
        self.stale.remove(&p.tile);
        let bytes = size(&p);
        if let Some(old) = self.tiles.insert(
            p.tile,
            Held {
                pkg: p,
                bytes,
                used: self.clock,
            },
        ) {
            self.bytes -= old.bytes;
        }
        self.bytes += bytes;
        while self.bytes > CACHE_BYTES {
            let oldest = self
                .tiles
                .iter()
                .filter(|(k, _)| !self.asked.contains(k))
                .min_by_key(|(_, h)| h.used)
                .map(|(k, _)| *k);
            let Some(k) = oldest else { break };
            if let Some(h) = self.tiles.remove(&k) {
                self.bytes -= h.bytes;
            }
        }
        self.revision += 1;
    }

    /// Tiles changed on the server (a change touched them): asked again.
    pub fn changed(&mut self, tiles: &[TileKey]) {
        for t in tiles {
            if self.tiles.contains_key(t) {
                self.stale.insert(*t);
            }
        }
    }

    /// The tiles of level `level` within [`RADIUS`] of `centre`.
    pub fn around(centre: [f64; 2], grid: f64, level: i8) -> BTreeSet<TileKey> {
        let g = ohl_core::grid::Grid::new(grid).unwrap_or_default();
        let t = g.tile_at(level, centre);
        let mut out = BTreeSet::new();
        for dx in -RADIUS..=RADIUS {
            for dy in -RADIUS..=RADIUS {
                out.insert(TileKey(level, t.x + dx, t.y + dy));
            }
        }
        out
    }

    /// What to ask the server for when the tiles in view are `want`: none
    /// when nothing changed (the same tiles, none stale); else every tile
    /// in view with the revision held (the server sends those it holds
    /// otherwise).
    pub fn interest(&mut self, want: BTreeSet<TileKey>) -> Option<Vec<HeldTile>> {
        let stale = want.iter().any(|t| self.stale.contains(t));
        if want == self.asked && !stale {
            return None;
        }
        self.clock += 1;
        for t in &want {
            if let Some(h) = self.tiles.get_mut(t) {
                h.used = self.clock;
            }
        }
        let held = want
            .iter()
            .map(|t| HeldTile {
                tile: *t,
                rev: self
                    .tiles
                    .get(t)
                    .filter(|_| !self.stale.contains(t))
                    .map(|h| h.pkg.rev),
            })
            .collect();
        // A stale tile asked again at no revision: it comes anew.
        self.stale.retain(|t| !want.contains(t));
        self.asked = want;
        Some(held)
    }

    /// Forgets what was asked (joined again: the server knows nothing of
    /// this client; every tile in view asked anew, with its revision).
    pub fn reset_asked(&mut self) {
        self.asked.clear();
    }

    /// The packages in view.
    pub fn in_view(&self) -> impl Iterator<Item = &TilePackage> {
        self.asked
            .iter()
            .filter_map(|t| self.tiles.get(t).map(|h| &h.pkg))
    }

    /// The routes seen in the context (model, route), with a place of
    /// each: to add one to the working set.
    pub fn routes(&self) -> BTreeMap<(String, String), [f64; 3]> {
        let mut out = BTreeMap::new();
        for p in self.in_view() {
            for r in &p.routes {
                if let Some(at) = r.points.first() {
                    out.entry((r.model.clone(), r.route.clone())).or_insert(*at);
                }
            }
        }
        out
    }

    /// The drawing of the tiles in view, without what `own` says the
    /// replica holds itself (model, kind, id).
    pub fn drawn(&self, own: &dyn Fn(&str, &str, &str) -> bool) -> Drawn {
        let mut d = Drawn {
            case: self.case.clone(),
            ..Drawn::default()
        };
        let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
        for p in self.in_view() {
            for s in &p.structures {
                if own(&s.model, "structure", &s.id)
                    || !seen.insert((s.model.clone(), s.id.clone()))
                {
                    continue;
                }
                for seg in &s.stick {
                    d.lines.push((vec![seg[0], seg[1]], STRUCTURE));
                }
                if s.stick.is_empty() {
                    d.dots.push(s.position);
                }
            }
            for w in &p.wires {
                if own(&w.model, "section", &w.section) {
                    continue;
                }
                // A section crossing several tiles comes with each: drawn
                // once.
                if !seen.insert((w.model.clone(), format!("section {}", w.section))) {
                    continue;
                }
                for s in &w.spans {
                    d.lines.push((s.points.clone(), WIRE));
                }
            }
            for r in &p.routes {
                if own(&r.model, "route", &r.route) {
                    continue;
                }
                d.lines.push((r.points.clone(), ROUTE));
            }
            for l in &p.labels {
                d.labels.push((l.text.clone(), l.at));
            }
        }
        d
    }
}

/// The context as drawn: polylines with their colours, dots, labels.
#[derive(Debug, Clone, Default)]
pub struct Drawn {
    /// Polylines (absolute coordinates, m) and colours.
    pub lines: Vec<(Vec<[f64; 3]>, [u8; 4])>,
    /// Structures without a stick: their places.
    pub dots: Vec<[f64; 3]>,
    /// Route names at their middles.
    pub labels: Vec<(String, [f64; 3])>,
    /// The weather case of the wires.
    pub case: String,
}

impl Drawn {
    /// The 3D layer's scene.
    pub fn scene(&self, origin: [f64; 3]) -> ohl_render::Scene {
        let mut s = ohl_render::Scene::new(origin);
        for (pts, col) in &self.lines {
            s.polyline(pts, *col, 1.0);
        }
        for p in &self.dots {
            s.point(*p, STRUCTURE, 4.0);
        }
        s
    }

    /// Paints it in the plan (`to_screen`), what is in `rect` only; a note
    /// in the corner saying it is a drawing.
    pub fn paint_plan(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        to_screen: &dyn Fn([f64; 3]) -> egui::Pos2,
    ) {
        let col = |c: [u8; 4]| egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
        let view = rect.expand(20.0);
        for (pts, c) in &self.lines {
            let s: Vec<egui::Pos2> = pts.iter().map(|p| to_screen(*p)).collect();
            if s.iter().all(|p| !view.contains(*p)) && s.len() < 3 {
                continue;
            }
            painter.add(egui::Shape::line(s, egui::Stroke::new(1.0, col(*c))));
        }
        for p in &self.dots {
            let q = to_screen(*p);
            if view.contains(q) {
                painter.circle_filled(q, 2.0, col(STRUCTURE));
            }
        }
        for (text, at) in &self.labels {
            let q = to_screen(*at);
            if rect.contains(q) {
                painter.text(
                    q,
                    egui::Align2::LEFT_BOTTOM,
                    text,
                    egui::FontId::proportional(11.0),
                    col(ROUTE),
                );
            }
        }
        if !self.lines.is_empty() {
            painter.text(
                rect.left_bottom() + egui::vec2(8.0, -8.0),
                egui::Align2::LEFT_BOTTOM,
                format!(
                    "grey: the network's context, read-only — a drawing ({}), never a result",
                    self.case
                ),
                egui::FontId::proportional(11.0),
                col(STRUCTURE),
            );
        }
    }
}

/// The drawing shared with the views.
pub type Shared = Arc<Drawn>;

#[cfg(test)]
mod tests {
    use super::*;
    use ohl_server::{ContextRoute, ContextStructure, Position};

    fn pkg(x: i64, rev: u32) -> TilePackage {
        TilePackage {
            tile: TileKey(2, x, 0),
            rev: Position { seq: 1, step: rev },
            bounds: [0.0; 4],
            case: "15 °C, no wind".into(),
            structures: vec![ContextStructure {
                model: "A".into(),
                id: format!("T{x}"),
                position: [x as f64 * 1024.0, 0.0, 0.0],
                orientation_deg: 0.0,
                type_name: None,
                height: 20.0,
                stick: vec![[[0.0; 3], [0.0, 0.0, 20.0]]],
            }],
            wires: Vec::new(),
            routes: vec![ContextRoute {
                model: "B".into(),
                route: "R".into(),
                alignment: "R".into(),
                points: vec![[0.0; 3], [1.0, 0.0, 0.0]],
            }],
            labels: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// Asked once per change of view; stale tiles asked again without
    /// their revision; what the replica holds not drawn; the routes seen.
    #[test]
    fn tiles_are_asked_kept_and_drawn() {
        let mut c = ContextTiles::default();
        let want = ContextTiles::around([10.0, 10.0], 256.0, 2);
        assert_eq!(want.len(), 25);
        let first = c.interest(want.clone()).unwrap();
        assert!(first.iter().all(|h| h.rev.is_none()));
        assert!(c.interest(want.clone()).is_none(), "the same view");
        for t in &want {
            let mut p = pkg(t.1, 0);
            p.tile = *t;
            c.put(p);
        }
        assert_eq!(c.size().0, 25);
        let t = *want.iter().next().unwrap();
        c.changed(&[t]);
        let again = c.interest(want.clone()).unwrap();
        assert_eq!(again.iter().filter(|h| h.rev.is_none()).count(), 1);
        assert!(c.interest(want.clone()).is_none());
        let d = c.drawn(&|m, k, _| m == "A" && k == "structure");
        assert!(d.lines.iter().all(|(_, col)| *col == ROUTE));
        assert_eq!(c.routes().len(), 1);
        let all = c.drawn(&|_, _, _| false);
        assert!(all.lines.len() > d.lines.len());
    }
}
