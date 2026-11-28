// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The map: 2D, read-only, simplified (owner's answer 4, 2026-11-28).
//! Routes coloured by voltage, owner or state; shared towers marked;
//! structures as dots past a zoom (asked for the area in view); overlays
//! of the assignments, the working sets in use, the locks and the stale
//! results. No background by default: region outlines the admin imports
//! from a GeoJSON file, kept with this app's settings on this computer
//! (never on the server; owner's answer 5); and (stage C, the owner's
//! question 1 of stages A–B) the owner's own map server, a URL the admin
//! types — off and empty by default, so no request goes anywhere until
//! then: one image of the view, in the network's own coordinate system
//! ([`tile_url`]), fetched on a thread of its own once the view rests.
//! Fed by the server's route summaries, never whole lines. A click picks a
//! route or a structure (the Assignments page's scope); in drawing mode
//! (stage C) each click adds a corner of a polygon, the region of an
//! assignment's scope.

use std::collections::BTreeSet;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use ohl_server::admin::{AdminMap, MapDot, MapRoute};
use serde::{Deserialize, Serialize};

/// The view's width below which structures are shown as dots, m.
pub const DOTS_BELOW_M: f64 = 40_000.0;

/// The settings key of the region outlines.
pub const REGIONS_KEY: &str = "ohl-server.regions";

/// The settings key of the background map server (its URL and whether it
/// is shown).
pub const BACKGROUND_KEY: &str = "ohl-server.background";

/// The view rests this long before its background image is asked for.
pub const BACKGROUND_REST: Duration = Duration::from_millis(400);

/// A background image is this large at most, bytes.
pub const BACKGROUND_MAX_BYTES: u64 = 32 << 20;

/// The owner's own map server (off and empty by default).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Background {
    /// Its URL template: `{x0}`, `{y0}`, `{x1}`, `{y1}` (the view in the
    /// network's coordinates, m), `{width}`, `{height}` (pixels), `{crs}`
    /// (the network's coordinate system), e.g. a WMS GetMap request.
    pub url: String,
    /// Shown.
    pub on: bool,
}

/// The URL of the background image of `view` (x0, y0, x1, y1) at
/// `size` pixels: `template` filled in; refused unless http(s).
pub fn tile_url(
    template: &str,
    view: [f64; 4],
    size: [u32; 2],
    crs: &str,
) -> Result<String, String> {
    let t = template.trim();
    let lower = t.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return Err("the map server's URL starts with https:// (or http://)".into());
    }
    let crs: String = crs
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '.' | '-' | '_'))
        .collect();
    Ok(t.replace("{x0}", &format!("{:.2}", view[0]))
        .replace("{y0}", &format!("{:.2}", view[1]))
        .replace("{x1}", &format!("{:.2}", view[2]))
        .replace("{y1}", &format!("{:.2}", view[3]))
        .replace("{width}", &size[0].to_string())
        .replace("{height}", &size[1].to_string())
        .replace("{crs}", &crs))
}

/// Fetches and decodes the image at `url` (15 s, [`BACKGROUND_MAX_BYTES`]
/// at most, no redirect followed).
pub fn fetch_image(url: &str) -> Result<egui::ColorImage, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .max_redirects(0)
        .build()
        .into();
    let bytes = agent
        .get(url)
        .call()
        .map_err(|e| format!("the map server: {e}"))?
        .body_mut()
        .with_config()
        .limit(BACKGROUND_MAX_BYTES)
        .read_to_vec()
        .map_err(|e| format!("the map server: {e}"))?;
    let img = image::load_from_memory(&bytes)
        .map_err(|e| format!("the map server's answer is no image: {e}"))?
        .to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Ok(egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}

/// The background as drawn: its image and the view it shows.
struct Shown {
    texture: egui::TextureHandle,
    view: [f64; 4],
}

/// A background image being fetched: its answer, the view it shows.
type Fetching = (mpsc::Receiver<Result<egui::ColorImage, String>>, [f64; 4]);

/// The background image being fetched, and when the view came to rest.
#[derive(Default)]
struct BackgroundState {
    shown: Option<Shown>,
    fetching: Option<Fetching>,
    rested: Option<([f64; 4], [u32; 2], Instant)>,
    /// The last error (said under the map).
    error: Option<String>,
    /// Requests made (the tests: none while the URL is empty).
    requests: usize,
}

impl std::fmt::Debug for BackgroundState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackgroundState")
            .field("shown", &self.shown.as_ref().map(|s| s.view))
            .field("requests", &self.requests)
            .finish_non_exhaustive()
    }
}

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
    /// The owner's own map server.
    pub background: Background,
    bg: BackgroundState,
    /// A polygon being drawn (its corners), the region of an assignment's
    /// scope; none: clicks pick.
    pub drawing: Option<Vec<[f64; 2]>>,
}

impl Clone for BackgroundState {
    fn clone(&self) -> Self {
        Self::default()
    }
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
            background: Background::default(),
            bg: BackgroundState::default(),
            drawing: None,
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
    /// The outlines and the background kept with the app's settings.
    pub fn load(&mut self, storage: Option<&dyn eframe::Storage>) {
        if let Some(r) = storage
            .and_then(|s| s.get_string(REGIONS_KEY))
            .and_then(|t| serde_json::from_str(&t).ok())
        {
            self.regions = r;
        }
        if let Some(b) = storage
            .and_then(|s| s.get_string(BACKGROUND_KEY))
            .and_then(|t| serde_json::from_str(&t).ok())
        {
            self.background = b;
        }
    }

    /// Keeps the outlines and the background with the app's settings.
    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        if let Ok(t) = serde_json::to_string(&self.regions) {
            storage.set_string(REGIONS_KEY, t);
        }
        if let Ok(t) = serde_json::to_string(&self.background) {
            storage.set_string(BACKGROUND_KEY, t);
        }
    }

    /// Background images asked for so far.
    pub fn background_requests(&self) -> usize {
        self.bg.requests
    }

    /// The background's last error.
    pub fn background_error(&self) -> Option<&str> {
        self.bg.error.as_deref()
    }

    /// A background image is shown.
    pub fn background_shown(&self) -> bool {
        self.bg.shown.is_some()
    }

    /// The background: the image of the view asked for once it rests (a
    /// thread of its own), and drawn under everything when it came.
    fn background_ui(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        rect: Rect,
        view: [f64; 4],
        crs: &str,
    ) {
        if !self.background.on || self.background.url.trim().is_empty() {
            self.bg.shown = None;
            self.bg.rested = None;
            return;
        }
        if let Some((rx, v)) = &self.bg.fetching {
            match rx.try_recv() {
                Ok(Ok(img)) => {
                    let texture = ui.ctx().load_texture(
                        "ohl-server-background",
                        img,
                        egui::TextureOptions::LINEAR,
                    );
                    self.bg.shown = Some(Shown { texture, view: *v });
                    self.bg.error = None;
                    self.bg.fetching = None;
                }
                Ok(Err(e)) => {
                    self.bg.error = Some(e);
                    self.bg.fetching = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => self.bg.fetching = None,
            }
        }
        if let Some(s) = &self.bg.shown {
            let a = self.screen(rect, [s.view[0], s.view[3]]);
            let b = self.screen(rect, [s.view[2], s.view[1]]);
            painter.image(
                s.texture.id(),
                Rect::from_two_pos(a, b),
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        let ppp = ui.ctx().pixels_per_point();
        let size = [
            (rect.width() * ppp).round().clamp(1.0, 4096.0) as u32,
            (rect.height() * ppp).round().clamp(1.0, 4096.0) as u32,
        ];
        let same = |a: [f64; 4], b: [f64; 4]| {
            let tol = (a[2] - a[0]).abs().max(1.0) * 1e-3;
            a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() <= tol)
        };
        let now = Instant::now();
        match self.bg.rested {
            Some((v, s, _)) if same(v, view) && s == size => {}
            _ => self.bg.rested = Some((view, size, now)),
        }
        let Some((v, s, since)) = self.bg.rested else {
            return;
        };
        let have = self.bg.shown.as_ref().is_some_and(|x| same(x.view, v))
            || self.bg.fetching.as_ref().is_some_and(|x| same(x.1, v));
        if have || self.bg.fetching.is_some() {
            return;
        }
        if now.duration_since(since) < BACKGROUND_REST {
            ui.ctx().request_repaint_after(BACKGROUND_REST);
            return;
        }
        match tile_url(&self.background.url, v, s, crs) {
            Ok(url) => {
                let (tx, rx) = mpsc::channel();
                let ctx = ui.ctx().clone();
                self.bg.requests += 1;
                let spawned = std::thread::Builder::new()
                    .name("ohl-server-app-background".into())
                    .spawn(move || {
                        let _ = tx.send(fetch_image(&url));
                        ctx.request_repaint();
                    });
                match spawned {
                    Ok(_) => self.bg.fetching = Some((rx, v)),
                    Err(e) => self.bg.error = Some(e.to_string()),
                }
            }
            Err(e) => {
                self.bg.error = Some(e);
                // Not asked again until the view moves.
                self.bg.rested = None;
                self.bg.shown = None;
            }
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
        // The owner's own map server's image, under everything.
        self.background_ui(ui, &painter, rect, view, &map.crs);
        // The region outlines (no background otherwise).
        for r in &self.regions {
            for ring in &r.rings {
                let pts: Vec<Pos2> = ring.iter().map(|p| self.screen(rect, *p)).collect();
                painter.add(egui::Shape::line(pts, Stroke::new(1.0, Color32::DARK_GRAY)));
            }
        }
        // The assignments' regions (a drawn polygon may be concave:
        // filled only when convex, always outlined).
        if self.assignments {
            for poly in &hl.regions {
                let pts: Vec<Pos2> = poly.iter().map(|p| self.screen(rect, *p)).collect();
                if pts.len() >= 3 {
                    if convex(poly) {
                        painter.add(egui::Shape::convex_polygon(
                            pts.clone(),
                            ASSIGNED.gamma_multiply(0.15),
                            Stroke::NONE,
                        ));
                    }
                    painter.add(egui::Shape::closed_line(pts, Stroke::new(1.5, ASSIGNED)));
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
        // The polygon being drawn: its corners, the edge to the pointer.
        if let Some(poly) = &self.drawing {
            let mut pts: Vec<Pos2> = poly.iter().map(|p| self.screen(rect, *p)).collect();
            if let Some(h) = resp.hover_pos() {
                pts.push(h);
            }
            if pts.len() >= 2 {
                painter.add(egui::Shape::closed_line(
                    pts.clone(),
                    Stroke::new(2.0, ASSIGNED),
                ));
            }
            for p in pts.iter().take(poly.len()) {
                painter.circle_filled(*p, 3.5, ASSIGNED);
            }
        }
        // A click adds a corner (drawing), else picks the nearest
        // structure, else route (6 px).
        if resp.clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            let w = self.world(rect, p);
            if let Some(poly) = &mut self.drawing {
                poly.push(w);
            } else {
                self.picked = self.pick(rect, p, map, if small { dots } else { &[] });
            }
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

    /// Centres the view on `p` (the shared towers' list), zoomed in.
    pub fn centre_on(&mut self, p: [f64; 2]) {
        self.centre = p;
        self.scale = self.scale.max(0.2);
    }
}

/// Whether polygon `poly` is convex (its turns all one way).
pub fn convex(poly: &[[f64; 2]]) -> bool {
    let n = poly.len();
    let mut sign = 0.0f64;
    for i in 0..n {
        let (a, b, c) = (poly[i], poly[(i + 1) % n], poly[(i + 2) % n]);
        let z = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
        if z != 0.0 {
            if sign != 0.0 && z.signum() != sign {
                return false;
            }
            sign = z.signum();
        }
    }
    true
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
    fn the_background_url_is_filled_in_and_must_be_http() {
        let u = tile_url(
            "https://maps.example/wms?BBOX={x0},{y0},{x1},{y1}&WIDTH={width}&HEIGHT={height}&CRS={crs}",
            [1.0, 2.0, 3.5, 4.0],
            [800, 600],
            "EPSG:2056",
        )
        .unwrap();
        assert_eq!(
            u,
            "https://maps.example/wms?BBOX=1.00,2.00,3.50,4.00&WIDTH=800&HEIGHT=600&CRS=EPSG:2056"
        );
        assert!(tile_url("file:///etc/passwd", [0.0; 4], [1, 1], "").is_err());
        assert!(tile_url("", [0.0; 4], [1, 1], "").is_err());
        assert!(convex(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]));
        assert!(!convex(&[
            [0.0, 0.0],
            [2.0, 0.0],
            [2.0, 2.0],
            [1.0, 0.5],
            [0.0, 2.0]
        ]));
    }

    #[test]
    fn colours_tell_voltages_and_owners_apart() {
        assert_ne!(voltage_colour(Some(50.0)), voltage_colour(Some(380.0)));
        assert_eq!(voltage_colour(None), Color32::GRAY);
        assert_eq!(owner_colour("grid east"), owner_colour("grid east"));
        assert_eq!(owner_colour(""), Color32::GRAY);
    }
}
