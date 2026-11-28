// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The map: 2D, read-only, simplified (owner's answer 4, 2026-11-28).
//! Routes coloured by voltage, owner or state; shared towers marked;
//! structures as dots past a zoom (asked for the area in view); overlays
//! of the assignments, the working sets in use, the locks and the stale
//! results. No background by default: region outlines the admin imports
//! from a GeoJSON file, kept with this app's settings on this computer
//! (never on the server; owner's answer 5). Fed by the server's route
//! summaries, never whole lines. A click picks a route or a structure (the
//! Assignments page's scope).

use std::collections::BTreeSet;

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use ohl_server::admin::{AdminMap, MapDot, MapRoute};
use serde::{Deserialize, Serialize};

/// The view's width below which structures are shown as dots, m.
pub const DOTS_BELOW_M: f64 = 40_000.0;

/// The settings key of the region outlines.
pub const REGIONS_KEY: &str = "ohl-server.regions";

/// What the routes are coloured by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ColourBy {
    /// Their highest nominal voltage.
    #[default]
    Voltage,
    /// Their line's systems' owner.
    Owner,
    /// Their state: results out of date, in use, locks, else nothing.
    State,
}

/// A region outline (imported, plan coordinates of the network).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Region {
    /// Its name (the feature's `name`, else its number).
    pub name: String,
    /// Its rings (outer and holes alike, drawn as outlines).
    pub rings: Vec<Vec<[f64; 2]>>,
}

/// What the map picked.
#[derive(Debug, Clone, PartialEq)]
pub enum Picked {
    /// A route.
    Route {
        /// Its line.
        model: String,
        /// Its id.
        route: String,
    },
    /// A structure.
    Structure {
        /// Its line.
        model: String,
        /// Its id.
        id: String,
    },
}

/// What the map highlights (the Assignments page's selection, "see as").
#[derive(Debug, Clone, Default)]
pub struct Highlight {
    /// Routes highlighted.
    pub routes: BTreeSet<String>,
    /// Whole lines highlighted.
    pub lines: BTreeSet<String>,
    /// The whole network.
    pub network: bool,
    /// Region scopes (outlined, filled).
    pub regions: Vec<Vec<[f64; 2]>>,
    /// Only these routes are seen ("see as"; none: every route).
    pub only: Option<BTreeSet<String>>,
}

/// The map's state (the view, the overlays, the outlines).
#[derive(Debug, Clone)]
pub struct MapView {
    /// The view's centre, m.
    pub centre: [f64; 2],
    /// Pixels per metre (0: fit at the next draw).
    pub scale: f64,
    /// What the routes are coloured by.
    pub colour_by: ColourBy,
    /// Overlays shown.
    pub assignments: bool,
    /// See `assignments`.
    pub working_sets: bool,
    /// See `assignments`.
    pub locks: bool,
    /// See `assignments`.
    pub stale: bool,
    /// The region outlines.
    pub regions: Vec<Region>,
    /// What was picked last.
    pub picked: Option<Picked>,
    /// The area in view (x0, y0, x1, y1), as last drawn.
    pub view: Option<[f64; 4]>,
    /// The last frame's drawing time, ms (measured: the 12k country).
    pub draw_ms: f64,
}

impl Default for MapView {
    fn default() -> Self {
        Self {
            centre: [0.0, 0.0],
            scale: 0.0,
            colour_by: ColourBy::default(),
            assignments: true,
            working_sets: true,
            locks: true,
            stale: true,
            regions: Vec::new(),
            picked: None,
            view: None,
            draw_ms: 0.0,
        }
    }
}

/// A voltage's colour (kV; none: grey).
pub fn voltage_colour(kv: Option<f64>) -> Color32 {
    match kv {
        None => Color32::GRAY,
        Some(v) if v < 60.0 => Color32::from_rgb(0, 150, 136),
        Some(v) if v < 150.0 => Color32::from_rgb(30, 110, 220),
        Some(v) if v < 250.0 => Color32::from_rgb(230, 140, 0),
        Some(_) => Color32::from_rgb(210, 40, 40),
    }
}

/// The voltage legend: (words, colour).
pub const VOLTAGE_LEGEND: [(&str, Option<f64>); 5] = [
    ("below 60 kV", Some(50.0)),
    ("60 – 150 kV", Some(110.0)),
    ("150 – 250 kV", Some(220.0)),
    ("250 kV and more", Some(380.0)),
    ("not given", None),
];

/// An owner's colour (a hash of the name; empty: grey).
pub fn owner_colour(owner: &str) -> Color32 {
    if owner.is_empty() {
        return Color32::GRAY;
    }
    const PALETTE: [Color32; 8] = [
        Color32::from_rgb(31, 119, 180),
        Color32::from_rgb(255, 127, 14),
        Color32::from_rgb(44, 160, 44),
        Color32::from_rgb(214, 39, 40),
        Color32::from_rgb(148, 103, 189),
        Color32::from_rgb(140, 86, 75),
        Color32::from_rgb(227, 119, 194),
        Color32::from_rgb(23, 190, 207),
    ];
    let h = owner
        .bytes()
        .fold(5381u32, |h, b| h.wrapping_mul(33) ^ u32::from(b));
    PALETTE[h as usize % PALETTE.len()]
}

/// The state colours: out of date, in use, locked, else.
pub const STALE: Color32 = Color32::from_rgb(220, 40, 40);
/// See [`STALE`].
pub const IN_USE: Color32 = Color32::from_rgb(240, 190, 0);
/// See [`STALE`].
pub const LOCKED: Color32 = Color32::from_rgb(150, 80, 200);
/// See [`STALE`].
pub const QUIET: Color32 = Color32::from_rgb(120, 140, 170);
/// The assignments' highlight.
pub const ASSIGNED: Color32 = Color32::from_rgb(0, 200, 120);

fn route_colour(r: &MapRoute, by: ColourBy) -> Color32 {
    match by {
        ColourBy::Voltage => voltage_colour(r.voltage_kv),
        ColourBy::Owner => owner_colour(&r.owner),
        ColourBy::State => {
            if r.stale.is_some() {
                STALE
            } else if !r.in_use.is_empty() {
                IN_USE
            } else if r.locks > 0 {
                LOCKED
            } else {
                QUIET
            }
        }
    }
}

/// Region outlines read from GeoJSON text (a FeatureCollection, a Feature
/// or a bare geometry: polygons, multi-polygons and lines; coordinates in
/// the network's own system, as the file gives them).
pub fn regions_from_geojson(text: &str) -> Result<Vec<Region>, String> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("not GeoJSON: {e}"))?;
    fn ring(c: &serde_json::Value) -> Option<Vec<[f64; 2]>> {
        c.as_array()?
            .iter()
            .map(|p| {
                let p = p.as_array()?;
                Some([p.first()?.as_f64()?, p.get(1)?.as_f64()?])
            })
            .collect()
    }
    fn rings(g: &serde_json::Value) -> Vec<Vec<[f64; 2]>> {
        let c = &g["coordinates"];
        let list = |v: &serde_json::Value| -> Vec<Vec<[f64; 2]>> {
            v.as_array()
                .map(|a| a.iter().filter_map(ring).collect())
                .unwrap_or_default()
        };
        match g["type"].as_str().unwrap_or_default() {
            "Polygon" | "MultiLineString" => list(c),
            "MultiPolygon" => c
                .as_array()
                .map(|a| a.iter().flat_map(list).collect())
                .unwrap_or_default(),
            "LineString" => ring(c).into_iter().collect(),
            "GeometryCollection" => g["geometries"]
                .as_array()
                .map(|a| a.iter().flat_map(rings).collect())
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }
    let features: Vec<&serde_json::Value> = match v["type"].as_str() {
        Some("FeatureCollection") => v["features"]
            .as_array()
            .map(|a| a.iter().collect())
            .unwrap_or_default(),
        _ => vec![&v],
    };
    let mut out = Vec::new();
    for (k, f) in features.iter().enumerate() {
        let (g, name) = if f["type"] == "Feature" {
            (
                &f["geometry"],
                f["properties"]["name"]
                    .as_str()
                    .map_or_else(|| format!("region {}", k + 1), str::to_owned),
            )
        } else {
            (*f, format!("region {}", k + 1))
        };
        let rings: Vec<Vec<[f64; 2]>> = rings(g)
            .into_iter()
            .filter(|r| r.len() >= 2 && r.iter().all(|p| p[0].is_finite() && p[1].is_finite()))
            .collect();
        if !rings.is_empty() {
            out.push(Region { name, rings });
        }
    }
    if out.is_empty() {
        return Err("no polygon or line in the file".into());
    }
    Ok(out)
}

impl MapView {
    /// The outlines kept with the app's settings.
    pub fn load(&mut self, storage: Option<&dyn eframe::Storage>) {
        if let Some(r) = storage
            .and_then(|s| s.get_string(REGIONS_KEY))
            .and_then(|t| serde_json::from_str(&t).ok())
        {
            self.regions = r;
        }
    }

    /// Keeps the outlines with the app's settings.
    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        if let Ok(t) = serde_json::to_string(&self.regions) {
            storage.set_string(REGIONS_KEY, t);
        }
    }

    /// Fits the view to `b` in `rect`.
    fn fit(&mut self, b: [f64; 4], rect: Rect) {
        self.centre = [(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0];
        let w = (b[2] - b[0]).max(100.0);
        let h = (b[3] - b[1]).max(100.0);
        self.scale = (f64::from(rect.width()) / w).min(f64::from(rect.height()) / h) * 0.9;
    }

    /// Draws `map` (and `dots` past the zoom) in the space left; returns
    /// the area whose structures to ask for, if it changed and is small
    /// enough.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        map: &AdminMap,
        dots: &[MapDot],
        hl: &Highlight,
    ) -> Option<[f64; 4]> {
        let started = std::time::Instant::now();
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
        if self.scale <= 0.0 {
            match map.bounds {
                Some(b) => self.fit(b, rect),
                None => {
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "nothing to show (no route you may see)",
                        egui::FontId::proportional(14.0),
                        ui.visuals().weak_text_color(),
                    );
                    return None;
                }
            }
        }
        // Pan and zoom (the wheel about the pointer).
        if resp.dragged() {
            let d = resp.drag_delta();
            self.centre[0] -= f64::from(d.x) / self.scale;
            self.centre[1] += f64::from(d.y) / self.scale;
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let f = (f64::from(scroll) / 200.0).exp();
                if let Some(p) = resp.hover_pos() {
                    let before = self.world(rect, p);
                    self.scale = (self.scale * f).clamp(1e-6, 50.0);
                    let after = self.world(rect, p);
                    self.centre[0] += before[0] - after[0];
                    self.centre[1] += before[1] - after[1];
                }
            }
        }
        let tl = self.world(rect, rect.left_top());
        let br = self.world(rect, rect.right_bottom());
        let view = [tl[0], br[1], br[0], tl[1]];
        self.view = Some(view);
        let fg = ui.visuals().text_color();
        // The region outlines (no background otherwise).
        for r in &self.regions {
            for ring in &r.rings {
                let pts: Vec<Pos2> = ring.iter().map(|p| self.screen(rect, *p)).collect();
                painter.add(egui::Shape::line(pts, Stroke::new(1.0, Color32::DARK_GRAY)));
            }
        }
        // The assignments' regions.
        if self.assignments {
            for poly in &hl.regions {
                let pts: Vec<Pos2> = poly.iter().map(|p| self.screen(rect, *p)).collect();
                if pts.len() >= 3 {
                    painter.add(egui::Shape::convex_polygon(
                        pts.clone(),
                        ASSIGNED.gamma_multiply(0.15),
                        Stroke::new(1.5, ASSIGNED),
                    ));
                }
            }
        }
        // The routes.
        let seen = |r: &MapRoute| hl.only.as_ref().is_none_or(|o| o.contains(&r.id));
        for r in &map.routes {
            let mut colour = route_colour(r, self.colour_by);
            let mut width = 2.0;
            if !seen(r) {
                colour = colour.gamma_multiply(0.15);
            }
            let assigned = self.assignments
                && (hl.network || hl.routes.contains(&r.id) || hl.lines.contains(&r.model));
            let picked =
                matches!(&self.picked, Some(Picked::Route { route, .. }) if *route == r.id);
            for l in &r.lines {
                let pts: Vec<Pos2> = l.iter().map(|p| self.screen(rect, *p)).collect();
                if pts.len() < 2 {
                    continue;
                }
                if assigned {
                    painter.add(egui::Shape::line(
                        pts.clone(),
                        Stroke::new(7.0, ASSIGNED.gamma_multiply(0.6)),
                    ));
                }
                if self.working_sets && !r.in_use.is_empty() {
                    painter.add(egui::Shape::line(
                        pts.clone(),
                        Stroke::new(5.0, IN_USE.gamma_multiply(0.7)),
                    ));
                }
                if self.stale && r.stale.is_some() {
                    painter.add(egui::Shape::dashed_line(
                        &pts,
                        Stroke::new(4.0, STALE),
                        6.0,
                        4.0,
                    ));
                }
                if picked {
                    width = 4.0;
                    colour = fg;
                }
                painter.add(egui::Shape::line(pts, Stroke::new(width, colour)));
            }
            if self.locks
                && r.locks > 0
                && let Some(p) = r.lines.first().and_then(|l| l.first())
            {
                let at = self.screen(rect, *p);
                painter.circle_filled(at, 4.0, LOCKED);
            }
        }
        // Shared towers, always marked.
        for p in &map.shared {
            let at = self.screen(rect, *p);
            if rect.contains(at) {
                painter.rect_filled(Rect::from_center_size(at, Vec2::splat(5.0)), 0.0, fg);
            }
        }
        // Structures past the zoom.
        let small = (view[2] - view[0]) < DOTS_BELOW_M;
        if small {
            for d in dots {
                let at = self.screen(rect, d.at);
                if !rect.contains(at) {
                    continue;
                }
                let picked = matches!(&self.picked, Some(Picked::Structure { id, model }) if *id == d.id && *model == d.model);
                painter.circle_filled(
                    at,
                    if picked { 4.5 } else { 2.5 },
                    if picked { ASSIGNED } else { fg },
                );
            }
        }
        // A click picks the nearest structure, else route (6 px).
        if resp.clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            self.picked = self.pick(rect, p, map, if small { dots } else { &[] });
        }
        self.draw_ms = started.elapsed().as_secs_f64() * 1e3;
        // Structures asked for once the view is small enough (rounded
        // out to 1 km so a little panning asks nothing).
        small.then(|| {
            let r = |v: f64, up: bool| {
                if up {
                    (v / 1000.0).ceil() * 1000.0
                } else {
                    (v / 1000.0).floor() * 1000.0
                }
            };
            [
                r(view[0], false),
                r(view[1], false),
                r(view[2], true),
                r(view[3], true),
            ]
        })
    }

    fn pick(&self, rect: Rect, p: Pos2, map: &AdminMap, dots: &[MapDot]) -> Option<Picked> {
        let mut best: Option<(f32, Picked)> = None;
        for d in dots {
            let dist = self.screen(rect, d.at).distance(p);
            if dist <= 6.0 && best.as_ref().is_none_or(|b| dist < b.0) {
                best = Some((
                    dist,
                    Picked::Structure {
                        model: d.model.clone(),
                        id: d.id.clone(),
                    },
                ));
            }
        }
        if best.is_some() {
            return best.map(|b| b.1);
        }
        for r in &map.routes {
            for l in &r.lines {
                for w in l.windows(2) {
                    let (a, b) = (self.screen(rect, w[0]), self.screen(rect, w[1]));
                    let dist = segment_distance(p, a, b);
                    if dist <= 6.0 && best.as_ref().is_none_or(|x| dist < x.0) {
                        best = Some((
                            dist,
                            Picked::Route {
                                model: r.model.clone(),
                                route: r.id.clone(),
                            },
                        ));
                    }
                }
            }
        }
        best.map(|b| b.1)
    }

    fn screen(&self, rect: Rect, p: [f64; 2]) -> Pos2 {
        let c = rect.center();
        Pos2::new(
            c.x + ((p[0] - self.centre[0]) * self.scale) as f32,
            c.y - ((p[1] - self.centre[1]) * self.scale) as f32,
        )
    }

    fn world(&self, rect: Rect, p: Pos2) -> [f64; 2] {
        let c = rect.center();
        [
            self.centre[0] + f64::from(p.x - c.x) / self.scale,
            self.centre[1] - f64::from(p.y - c.y) / self.scale,
        ]
    }

    /// The view as a region (a rectangle), for an assignment's scope.
    pub fn view_region(&self) -> Option<Vec<[f64; 2]>> {
        self.view
            .map(|v| vec![[v[0], v[1]], [v[2], v[1]], [v[2], v[3]], [v[0], v[3]]])
    }
}

fn segment_distance(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = if ab.length_sq() > 0.0 {
        ((p - a).dot(ab) / ab.length_sq()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a + ab * t).distance(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geojson_polygons_lines_and_names_are_read() {
        let text = r#"{"type":"FeatureCollection","features":[
            {"type":"Feature","properties":{"name":"north"},
             "geometry":{"type":"Polygon","coordinates":[[[0,0],[10,0],[10,10],[0,0]]]}},
            {"type":"Feature","properties":{},
             "geometry":{"type":"MultiPolygon","coordinates":[[[[1,1],[2,1],[2,2]]],[[[5,5],[6,5],[6,6]]]]}},
            {"type":"Feature","geometry":{"type":"Point","coordinates":[1,1]}}]}"#;
        let r = regions_from_geojson(text).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].name, "north");
        assert_eq!(r[1].name, "region 2");
        assert_eq!(r[1].rings.len(), 2);
        assert!(regions_from_geojson("{\"type\":\"Point\",\"coordinates\":[0,0]}").is_err());
        assert!(regions_from_geojson("not json").is_err());
    }

    #[test]
    fn colours_tell_voltages_and_owners_apart() {
        assert_ne!(voltage_colour(Some(50.0)), voltage_colour(Some(380.0)));
        assert_eq!(voltage_colour(None), Color32::GRAY);
        assert_eq!(owner_colour("grid east"), owner_colour("grid east"));
        assert_eq!(owner_colour(""), Color32::GRAY);
    }
}
