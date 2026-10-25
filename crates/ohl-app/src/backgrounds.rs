// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Terrain › Backgrounds: orthophotos (plan view) and DXF drawings (plan
//! and 3D views) kept in the project ([`ohl_background`]): reading them
//! with the project, drawing them, importing and removing them.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use eframe::egui;
use ohl_background::{Drawing, DrawingInfo, ImageInfo};

use crate::model::Model;
use crate::scene::Layer;

/// A drawing read, with the extent of each of its lines (to leave out
/// what is off screen).
#[derive(Debug, Clone)]
pub struct LoadedDrawing {
    /// What is known of it.
    pub info: DrawingInfo,
    /// The drawing.
    pub drawing: Arc<Drawing>,
    /// Lowest and highest (x, y) of each line.
    pub boxes: Vec<([f64; 2], [f64; 2])>,
}

impl LoadedDrawing {
    /// `drawing` with the extent of each of its lines.
    pub fn new(info: DrawingInfo, drawing: Drawing) -> Self {
        let boxes = drawing
            .lines
            .iter()
            .map(|l| {
                l.points.iter().fold(
                    ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]),
                    |(lo, hi), p| {
                        (
                            [lo[0].min(p[0]), lo[1].min(p[1])],
                            [hi[0].max(p[0]), hi[1].max(p[1])],
                        )
                    },
                )
            })
            .collect();
        Self {
            info,
            drawing: Arc::new(drawing),
            boxes,
        }
    }
}

/// The backgrounds of the project.
#[derive(Debug, Clone, Default)]
pub struct Loaded {
    /// Images (their tiles are read when drawn).
    pub images: Vec<ImageInfo>,
    /// Drawings.
    pub drawings: Vec<LoadedDrawing>,
    /// The kind of each image layer given one (the others: orthophotos).
    pub kinds: std::collections::BTreeMap<String, ohl_background::LayerKind>,
}

impl Loaded {
    /// Image layer `layer`'s kind.
    pub fn kind(&self, layer: &str) -> ohl_background::LayerKind {
        self.kinds.get(layer).copied().unwrap_or_default()
    }

    /// The image layers that are maps (2026-10-25, roadmap O16), in
    /// their order.
    pub fn map_layers(&self) -> Vec<String> {
        self.image_layers()
            .into_iter()
            .map(|(l, _)| l)
            .filter(|l| self.kind(l) == ohl_background::LayerKind::Map)
            .collect()
    }

    /// The layers the route and situation plans and the sheets' key map
    /// take when none are named: the maps, when the project has any (none:
    /// every layer).
    pub fn plan_layers(&self) -> Option<Vec<String>> {
        let maps = self.map_layers();
        (!maps.is_empty()).then_some(maps)
    }

    /// The image layers, with their number of images.
    pub fn image_layers(&self) -> Vec<(String, usize)> {
        let mut out: Vec<(String, usize)> = Vec::new();
        for i in &self.images {
            match out.last_mut() {
                Some((l, n)) if *l == i.layer => *n += 1,
                _ => out.push((i.layer.clone(), 1)),
            }
        }
        out
    }

    /// Whether there is anything.
    pub fn is_empty(&self) -> bool {
        self.images.is_empty() && self.drawings.is_empty()
    }
}

/// The key of an image layer among the backgrounds hidden.
pub fn image_key(layer: &str) -> String {
    format!("image:{layer}")
}

/// The key of a drawing among the backgrounds hidden.
pub fn drawing_key(name: &str) -> String {
    format!("drawing:{name}")
}

/// Reads the backgrounds of project `file`; what went wrong goes to `log`.
pub fn load(file: &Path, log: &mut Vec<String>) -> Loaded {
    if !ohl_core::files::is_project_file(file) || !file.exists() {
        return Loaded::default();
    }
    let store = match ohl_background::Store::open(file) {
        Ok(s) => s,
        Err(e) => {
            log.push(format!("backgrounds not read: {e}"));
            return Loaded::default();
        }
    };
    let images = store.images().unwrap_or_else(|e| {
        log.push(format!("images not read: {e}"));
        Vec::new()
    });
    let kinds = store.layer_kinds().unwrap_or_else(|e| {
        log.push(format!("image layers' kinds not read: {e}"));
        Default::default()
    });
    let mut drawings = Vec::new();
    for info in store.drawings().unwrap_or_default() {
        match store.drawing(&info.name) {
            Ok(d) => drawings.push(LoadedDrawing::new(info, d)),
            Err(e) => log.push(format!("drawing {} not read: {e}", info.name)),
        }
    }
    if !images.is_empty() || !drawings.is_empty() {
        log.push(format!(
            "backgrounds: {} image(s), {} drawing(s)",
            images.len(),
            drawings.len()
        ));
    }
    Loaded {
        images,
        drawings,
        kinds,
    }
}

/// Tiles to read per frame at most (the rest on the next frames).
const LOADS_PER_FRAME: usize = 12;

/// Tiles kept on the graphics card at most.
const KEPT: usize = 800;

type TileKey = (i64, u8, u32, u32);

/// The image tiles of the plan view, read from the project as they come
/// into view and kept (the least recently drawn dropped first).
#[derive(Default)]
pub struct PlanCache {
    file: PathBuf,
    store: Option<ohl_background::Store>,
    textures: HashMap<TileKey, Option<egui::TextureHandle>>,
    used: VecDeque<TileKey>,
}

impl std::fmt::Debug for PlanCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlanCache")
            .field("tiles", &self.textures.len())
            .finish_non_exhaustive()
    }
}

impl PlanCache {
    fn texture(
        &mut self,
        ctx: &egui::Context,
        key: TileKey,
        loads: &mut usize,
    ) -> Option<egui::TextureHandle> {
        if let Some(t) = self.textures.get(&key) {
            return t.clone();
        }
        if *loads == 0 {
            return None;
        }
        *loads -= 1;
        let store = self.store.as_ref()?;
        let tex = store
            .tile(key.0, key.1, key.2, key.3)
            .ok()
            .flatten()
            .and_then(|data| ohl_background::decode_tile(&data).ok())
            .map(|img| {
                let size = [img.width() as usize, img.height() as usize];
                let color = egui::ColorImage::from_rgb(size, img.as_raw());
                ctx.load_texture(
                    format!("bg-{}-{}-{}-{}", key.0, key.1, key.2, key.3),
                    color,
                    egui::TextureOptions::LINEAR,
                )
            });
        self.textures.insert(key, tex.clone());
        self.used.push_back(key);
        while self.used.len() > KEPT {
            if let Some(old) = self.used.pop_front() {
                self.textures.remove(&old);
            }
        }
        tex
    }

    /// Draws the backgrounds shown under the plan: images (a coarse level
    /// at once, the level of the zoom as it is read), then drawings.
    /// `to_screen` maps project coordinates; `scale`: pixels per metre.
    #[allow(clippy::too_many_arguments)]
    pub fn paint(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        model: &Model,
        visible: &HashSet<Layer>,
        rect: egui::Rect,
        to_screen: &dyn Fn([f64; 3]) -> egui::Pos2,
        centre: [f64; 2],
        scale: f64,
    ) {
        let bg = &model.backgrounds;
        let half = [
            f64::from(rect.width()) / 2.0 / scale,
            f64::from(rect.height()) / 2.0 / scale,
        ];
        let view = (
            [centre[0] - half[0], centre[1] - half[1]],
            [centre[0] + half[0], centre[1] + half[1]],
        );
        let meets = |lo: [f64; 2], hi: [f64; 2]| {
            lo[0] <= view.1[0] && hi[0] >= view.0[0] && lo[1] <= view.1[1] && hi[1] >= view.0[1]
        };
        if visible.contains(&Layer::Orthophotos) && !bg.images.is_empty() {
            if self.file != model.sources.file || self.store.is_none() {
                self.file = model.sources.file.clone();
                self.store = ohl_background::Store::open(&self.file).ok();
                self.textures.clear();
                self.used.clear();
            }
            let mut loads = LOADS_PER_FRAME;
            let mut waiting = false;
            let rect_of = |lo: [f64; 2], hi: [f64; 2]| {
                egui::Rect::from_two_pos(
                    to_screen([lo[0], lo[1], 0.0]),
                    to_screen([hi[0], hi[1], 0.0]),
                )
            };
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            for img in &bg.images {
                let (lo, hi) = img.bounds();
                if model.hidden_backgrounds.contains(&image_key(&img.layer)) || !meets(lo, hi) {
                    continue;
                }
                let level = img.level_for(1.0 / scale);
                let top = img.levels.saturating_sub(1);
                // The levels drawn: the coarsest (one tile, read first) under
                // the one of the zoom.
                for l in if level == top {
                    vec![top]
                } else {
                    vec![top, level]
                } {
                    let f = img.georef.px * f64::from(1u32 << l) * f64::from(ohl_background::TILE);
                    let g = img.georef.py * f64::from(1u32 << l) * f64::from(ohl_background::TILE);
                    let (nx, ny) = img.tiles(l);
                    let c0 = ((view.0[0] - img.georef.x0) / f).floor().max(0.0) as i64;
                    let c1 =
                        (((view.1[0] - img.georef.x0) / f).floor() as i64).min(i64::from(nx) - 1);
                    let r0 = ((img.georef.y0 - view.1[1]) / g).floor().max(0.0) as i64;
                    let r1 =
                        (((img.georef.y0 - view.0[1]) / g).floor() as i64).min(i64::from(ny) - 1);
                    for ty in r0..=r1 {
                        for tx in c0..=c1 {
                            let key = (img.id, l, tx as u32, ty as u32);
                            if let Some(t) = self.texture(ctx, key, &mut loads) {
                                let (lo, hi, _) = img.tile_bounds(l, tx as u32, ty as u32);
                                painter.image(t.id(), rect_of(lo, hi), uv, egui::Color32::WHITE);
                            } else if !self.textures.contains_key(&key) {
                                // Not read yet: on a next frame.
                                waiting = true;
                            }
                        }
                    }
                }
            }
            if waiting {
                ctx.request_repaint();
            }
        }
        if visible.contains(&Layer::Drawings) {
            for d in &bg.drawings {
                if model
                    .hidden_backgrounds
                    .contains(&drawing_key(&d.info.name))
                {
                    continue;
                }
                paint_drawing(painter, d, &meets, to_screen, scale);
            }
        }
    }
}

fn color(c: [u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(c[0], c[1], c[2])
}

/// Draws a drawing's lines, points and texts in plan: what is off screen,
/// on a hidden layer or smaller than a pixel is left out; texts from 6
/// pixels high.
fn paint_drawing(
    painter: &egui::Painter,
    d: &LoadedDrawing,
    meets: &dyn Fn([f64; 2], [f64; 2]) -> bool,
    to_screen: &dyn Fn([f64; 3]) -> egui::Pos2,
    scale: f64,
) {
    let off: Vec<bool> = d.drawing.layers.iter().map(|l| l.off).collect();
    let hidden = |layer: u32| off.get(layer as usize).copied().unwrap_or(false);
    for (l, (lo, hi)) in d.drawing.lines.iter().zip(&d.boxes) {
        if hidden(l.layer) || !meets(*lo, *hi) {
            continue;
        }
        let size = ((hi[0] - lo[0]).max(hi[1] - lo[1])) * scale;
        if size < 0.5 {
            continue;
        }
        // Points less than a pixel apart are one.
        let mut pts: Vec<egui::Pos2> = Vec::with_capacity(l.points.len() + 1);
        for p in &l.points {
            let q = to_screen(*p);
            if pts.last().is_none_or(|last| last.distance(q) >= 1.0) {
                pts.push(q);
            }
        }
        if l.closed
            && let Some(first) = pts.first().copied()
        {
            pts.push(first);
        }
        if pts.len() >= 2 {
            painter.add(egui::Shape::line(
                pts,
                egui::Stroke::new(1.0, color(l.color)),
            ));
        }
    }
    if scale > 0.5 {
        for m in &d.drawing.points {
            if !hidden(m.layer) && meets([m.at[0], m.at[1]], [m.at[0], m.at[1]]) {
                painter.circle_filled(to_screen(m.at), 1.5, color(m.color));
            }
        }
    }
    for t in &d.drawing.texts {
        let px = t.height * scale;
        if hidden(t.layer) || !(6.0..=400.0).contains(&px) {
            continue;
        }
        let reach = t.height * t.text.len() as f64;
        if !meets(
            [t.at[0] - reach, t.at[1] - reach],
            [t.at[0] + reach, t.at[1] + reach],
        ) {
            continue;
        }
        // Letter height ≈ 0.7 of the font size.
        let galley = painter.layout_no_wrap(
            t.text.clone(),
            egui::FontId::proportional((px / 0.7) as f32),
            color(t.color),
        );
        // The insertion point is the text's lower left: the galley's top
        // left is its height above, turned with the text.
        let a = -(t.angle_deg.to_radians()) as f32;
        let h = galley.size().y;
        let base = to_screen(t.at);
        let pos = base + egui::vec2(h * a.sin(), -h * a.cos());
        painter.add(egui::epaint::TextShape::new(pos, galley, color(t.color)).with_angle(a));
    }
}

/// The drawings' lines and points for the 3D view, at their own heights
/// (texts are drawn in plan only).
pub fn scene(model: &Model, s: &mut ohl_render::Scene) {
    for d in &model.backgrounds.drawings {
        if model
            .hidden_backgrounds
            .contains(&drawing_key(&d.info.name))
        {
            continue;
        }
        let off: Vec<bool> = d.drawing.layers.iter().map(|l| l.off).collect();
        let hidden = |layer: u32| off.get(layer as usize).copied().unwrap_or(false);
        for l in &d.drawing.lines {
            if hidden(l.layer) || l.points.len() < 2 {
                continue;
            }
            let c = [l.color[0], l.color[1], l.color[2], 255];
            if l.closed {
                let mut pts = l.points.clone();
                pts.push(pts[0]);
                s.polyline(&pts, c, 1.0);
            } else {
                s.polyline(&l.points, c, 1.0);
            }
        }
        for m in &d.drawing.points {
            if !hidden(m.layer) {
                s.point(m.at, [m.color[0], m.color[1], m.color[2], 255], 3.0);
            }
        }
    }
}

/// The external converter for ECW files (GDAL's `gdal_translate` with the
/// ECW driver), as the user gave it; remembered on this computer.
fn converter_cell() -> &'static Mutex<String> {
    static CELL: OnceLock<Mutex<String>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(String::new()))
}

/// The ECW converter given (empty: none).
pub fn converter() -> String {
    converter_cell()
        .lock()
        .map(|c| c.clone())
        .unwrap_or_default()
}

/// Sets the ECW converter.
pub fn set_converter(path: &str) {
    if let Ok(mut c) = converter_cell().lock() {
        *c = path.trim().to_owned();
    }
}

/// A `gdal_translate` found in the usual places, if any.
pub fn find_converter() -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        "gdal_translate.exe"
    } else {
        "gdal_translate"
    };
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    for d in [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/usr/bin",
        "/Library/Frameworks/GDAL.framework/Programs",
        "/Applications/QGIS.app/Contents/MacOS/bin",
        "C:\\OSGeo4W\\bin",
        "C:\\OSGeo4W64\\bin",
    ] {
        dirs.push(PathBuf::from(d));
    }
    dirs.into_iter().map(|d| d.join(exe)).find(|p| p.is_file())
}

/// Image files the importer reads (ECW through the converter).
const IMAGE_EXTENSIONS: [&str; 6] = ["tif", "tiff", "jpg", "jpeg", "png", "ecw"];

fn is_image(p: &Path) -> bool {
    p.extension()
        .is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.to_string_lossy().to_lowercase().as_str()))
}

fn is_ecw(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ecw"))
}

/// Terrain › Backgrounds › Import orthophotos or maps.
#[derive(Debug, Clone)]
pub struct ImageImport {
    /// The files.
    pub files: Vec<PathBuf>,
    /// Their layer.
    pub layer: String,
    /// An orthophoto or a map.
    pub kind: ohl_background::LayerKind,
    /// The ECW converter (when ECW files are given).
    pub converter: String,
    /// Why it cannot start.
    pub error: Option<String>,
}

impl ImageImport {
    /// Empty, into layer `Orthophoto`.
    pub fn new() -> Self {
        let mut converter = converter();
        if converter.is_empty()
            && let Some(p) = find_converter()
        {
            converter = p.display().to_string();
        }
        Self {
            files: Vec::new(),
            layer: "Orthophoto".into(),
            kind: ohl_background::LayerKind::Orthophoto,
            converter,
            error: None,
        }
    }
}

/// Terrain › Backgrounds › Import DXF drawing.
#[derive(Debug, Clone, Default)]
pub struct DrawingImport {
    /// The file.
    pub file: Option<PathBuf>,
    /// Its name in the project.
    pub name: String,
    /// Why it cannot start.
    pub error: Option<String>,
}

/// The backgrounds' windows and the import running.
#[derive(Debug, Default)]
pub struct Windows {
    /// Import orthophotos, when open.
    pub images: Option<ImageImport>,
    /// Import a DXF drawing, when open.
    pub drawing: Option<DrawingImport>,
    /// Remove backgrounds, when open (the item asked about).
    pub remove: Option<Option<String>>,
    /// The import running.
    pub job: Option<crate::survey_io::Job>,
}

/// The project file to import into, or why not.
fn project(model: &Model) -> Result<PathBuf, String> {
    model.can_edit()?;
    let file = model.sources.file.clone();
    if !ohl_core::files::is_project_file(&file) {
        return Err("backgrounds are kept in a project (.ohl)".into());
    }
    Ok(file)
}

/// The EPSG code of a system given as `EPSG:<code>`.
fn epsg(spec: &str) -> Option<u16> {
    spec.trim()
        .to_ascii_lowercase()
        .strip_prefix("epsg:")?
        .trim()
        .parse()
        .ok()
}

/// Imports image files into `layer` of project `file` (ECW files through
/// `converter`); a file that fails is reported, the others imported.
pub fn import_images(
    file: &Path,
    layer: &str,
    kind: ohl_background::LayerKind,
    files: &[PathBuf],
    crs: &str,
    converter: &str,
    report: &dyn Fn(String, f32),
) -> Result<String, String> {
    let mut store = ohl_background::Store::open(file).map_err(|e| e.to_string())?;
    store
        .set_layer_kind(layer, kind)
        .map_err(|e| e.to_string())?;
    let scratch = std::env::temp_dir().join(format!("ohl-ecw-{}", std::process::id()));
    let budget = ohl_core::budget::Budget::new();
    let mut done = 0;
    // Files that failed, by reason (one line per reason).
    let mut failed: Vec<(String, Vec<String>)> = Vec::new();
    for (k, f) in files.iter().enumerate() {
        let share = 1.0 / files.len() as f32;
        let name = f
            .file_stem()
            .map_or_else(|| "image".into(), |s| s.to_string_lossy().into_owned());
        report(
            format!("{} ({} of {})", f.display(), k + 1, files.len()),
            k as f32 * share,
        );
        let result = (|| -> Result<(), String> {
            let (path, georef) = if is_ecw(f) {
                if converter.trim().is_empty() {
                    return Err(
                        "an ECW file needs a converter (GDAL's gdal_translate with the ECW \
                         driver): give it in the window"
                            .into(),
                    );
                }
                std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
                let tif = ohl_background::convert_with(
                    Path::new(converter.trim()),
                    f,
                    &scratch,
                    std::time::Duration::from_secs(1800),
                )?;
                // The converted file's own georeferencing, else the ECW's
                // world file.
                let g = match ohl_background::georef_of(&tif) {
                    Ok((g, _)) => g,
                    Err(_) => ohl_background::georef_of(f)?.0,
                };
                (tif, Some(g))
            } else {
                (f.clone(), None)
            };
            if georef.is_none()
                && let Ok((_, Some(code))) = ohl_background::georef_of(&path)
                && let Some(project) = epsg(crs)
                && code != project
            {
                return Err(format!(
                    "the image is in EPSG:{code}, the project in EPSG:{project}: reproject the \
                     image first"
                ));
            }
            store
                .import_image(
                    layer,
                    &name,
                    &path,
                    &f.display().to_string(),
                    crs,
                    georef,
                    &budget,
                    |p| report(format!("{name}: tiles"), (k as f32 + p) * share),
                )
                .map_err(|e| e.to_string())?;
            if path != *f {
                let _ = std::fs::remove_file(&path);
            }
            Ok(())
        })();
        match result {
            Ok(()) => done += 1,
            Err(e) => {
                let file = f.file_name().map_or_else(
                    || f.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                );
                match failed.iter_mut().find(|(why, _)| *why == e) {
                    Some((_, files)) => files.push(file),
                    None => failed.push((e, vec![file])),
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&scratch);
    let lines: Vec<String> = failed
        .iter()
        .map(|(why, files)| {
            let shown: Vec<&str> = files.iter().take(5).map(String::as_str).collect();
            let more = files.len().saturating_sub(shown.len());
            format!(
                "{} file(s): {why} ({}{})",
                files.len(),
                shown.join(", "),
                if more > 0 {
                    format!(" and {more} more")
                } else {
                    String::new()
                }
            )
        })
        .collect();
    if done == 0 && !failed.is_empty() {
        return Err(format!("No image imported.\n{}", lines.join("\n")));
    }
    let mut text = format!("{done} image(s) imported into layer {layer}.");
    if !failed.is_empty() {
        text.push_str(&format!("\nNot imported:\n{}", lines.join("\n")));
    }
    Ok(text)
}

impl Windows {
    /// The open windows. Returns true when the project should be opened
    /// again (an import or a removal done).
    pub fn show(&mut self, ctx: &egui::Context, model: &Model) -> bool {
        let mut reopen = false;
        if let Some(w) = &mut self.images {
            let mut open = true;
            let mut start = false;
            egui::Window::new("Import orthophotos or maps")
                .open(&mut open)
                .default_width(560.0)
                .show(ctx, |ui| {
                    ui.label(
                        "North-up images: GeoTIFF, or TIFF, JPEG, PNG with a world file (.tfw, \
                         .jgw, .pgw, .wld) next to them; ECW through a converter. They are \
                         copied into the project as tiles with a zoom pyramid and drawn under \
                         the plan view.",
                    );
                    ui.horizontal(|ui| {
                        ui.label("They are");
                        let before = w.kind;
                        ui.radio_value(
                            &mut w.kind,
                            ohl_background::LayerKind::Orthophoto,
                            "orthophotos",
                        );
                        ui.radio_value(&mut w.kind, ohl_background::LayerKind::Map, "a map")
                            .on_hover_text(
                                "A map loaded into the project: under the plan view like an \
                                 orthophoto, and taken first by the route and situation plans \
                                 and the sheets' key map",
                            );
                        // The default layer name follows the kind.
                        if w.kind != before && (w.layer == "Orthophoto" || w.layer == "Map") {
                            w.layer = match w.kind {
                                ohl_background::LayerKind::Map => "Map".into(),
                                ohl_background::LayerKind::Orthophoto => "Orthophoto".into(),
                            };
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Add files…").clicked()
                            && let Some(fs) = rfd::FileDialog::new()
                                .add_filter("Images", &IMAGE_EXTENSIONS)
                                .pick_files()
                        {
                            w.files.extend(fs);
                        }
                        if ui.button("Add a folder…").clicked()
                            && let Some(dir) = rfd::FileDialog::new().pick_folder()
                            && let Ok(entries) = std::fs::read_dir(&dir)
                        {
                            let mut fs: Vec<PathBuf> = entries
                                .filter_map(|e| e.ok().map(|e| e.path()))
                                .filter(|p| is_image(p))
                                .collect();
                            fs.sort();
                            w.files.extend(fs);
                        }
                        if !w.files.is_empty() && ui.button("Clear").clicked() {
                            w.files.clear();
                        }
                    });
                    w.files.dedup();
                    egui::ScrollArea::vertical()
                        .max_height(120.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            for f in &w.files {
                                ui.label(egui::RichText::new(f.display().to_string()).small());
                            }
                        });
                    ui.label(format!("{} file(s)", w.files.len()));
                    ui.horizontal(|ui| {
                        ui.label("Layer");
                        ui.add(
                            egui::TextEdit::singleline(&mut w.layer)
                                .id_salt("bg-layer")
                                .desired_width(200.0),
                        )
                        .on_hover_text("Images of one layer are shown or hidden together");
                    });
                    if w.files.iter().any(|f| is_ecw(f)) {
                        ui.separator();
                        ui.label(
                            "ECW files are converted to GeoTIFF first by GDAL's gdal_translate \
                             with the ECW driver, which you install yourself (ECW decoding \
                             needs a licensed SDK: it is not part of OpenOHL).",
                        );
                        ui.horizontal(|ui| {
                            ui.label("Converter");
                            ui.add(
                                egui::TextEdit::singleline(&mut w.converter)
                                    .id_salt("bg-converter")
                                    .hint_text("…/gdal_translate")
                                    .desired_width(300.0),
                            );
                            if ui.button("Find").clicked() {
                                match find_converter() {
                                    Some(p) => w.converter = p.display().to_string(),
                                    None => {
                                        w.error = Some("no gdal_translate found".into());
                                    }
                                }
                            }
                            if ui.button("Browse…").clicked()
                                && let Some(p) = rfd::FileDialog::new().pick_file()
                            {
                                w.converter = p.display().to_string();
                            }
                        });
                    }
                    ui.label(
                        egui::RichText::new(format!(
                            "In the project's coordinate system ({}); a GeoTIFF naming another \
                             EPSG system is refused.",
                            ohl_crs::describe(&model.line.crs)
                        ))
                        .small()
                        .weak(),
                    );
                    if let Some(e) = &w.error {
                        crate::menu::error_label(ui, e);
                    }
                    if let Err(why) = project(model) {
                        crate::menu::error_label(ui, why);
                    }
                    start = ui
                        .add_enabled(
                            !w.files.is_empty() && project(model).is_ok() && self.job.is_none(),
                            egui::Button::new("Import"),
                        )
                        .clicked();
                });
            if start {
                let ecw = w.files.iter().filter(|f| is_ecw(f)).count();
                if w.layer.trim().is_empty() {
                    w.error = Some("give the layer a name".into());
                } else if ecw > 0 && !Path::new(w.converter.trim()).is_file() {
                    w.error = Some(format!(
                        "{ecw} ECW file(s) need a converter: give GDAL's gdal_translate (with the \
                         ECW driver) above, or convert them to GeoTIFF first and add those"
                    ));
                } else if let Ok(file) = project(model) {
                    set_converter(&w.converter);
                    let (layer, kind, files, crs, conv) = (
                        w.layer.trim().to_owned(),
                        w.kind,
                        w.files.clone(),
                        model.line.crs.clone(),
                        w.converter.clone(),
                    );
                    self.job = Some(crate::survey_io::Job::start(
                        format!("Import images into {layer}"),
                        true,
                        ctx,
                        move |report| {
                            import_images(&file, &layer, kind, &files, &crs, &conv, report)
                        },
                    ));
                    open = false;
                }
            }
            if !open {
                self.images = None;
            }
        }
        if let Some(w) = &mut self.drawing {
            let mut open = true;
            let mut start = false;
            egui::Window::new("Import DXF drawing")
                .open(&mut open)
                .default_width(480.0)
                .show(ctx, |ui| {
                    ui.label(
                        "A DXF drawing (e.g. the cadastre) in the project's coordinate system: \
                         lines, polylines, arcs, points and texts by layer, in their colours, \
                         at their own heights — drawn in the plan view and in the 3D view (texts \
                         in plan only). Kept in the project.",
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Choose a DXF file…").clicked()
                            && let Some(p) = rfd::FileDialog::new()
                                .add_filter("DXF", &["dxf"])
                                .pick_file()
                        {
                            if w.name.trim().is_empty() {
                                w.name = p
                                    .file_stem()
                                    .map(|s| s.to_string_lossy().into_owned())
                                    .unwrap_or_default();
                            }
                            w.file = Some(p);
                        }
                        if let Some(f) = &w.file {
                            ui.label(egui::RichText::new(f.display().to_string()).small());
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Name");
                        ui.add(
                            egui::TextEdit::singleline(&mut w.name)
                                .id_salt("bg-dxf-name")
                                .desired_width(220.0),
                        );
                    });
                    if model
                        .backgrounds
                        .drawings
                        .iter()
                        .any(|d| d.info.name == w.name.trim())
                    {
                        ui.label(
                            egui::RichText::new("A drawing of that name is replaced.").small(),
                        );
                    }
                    if let Some(e) = &w.error {
                        crate::menu::error_label(ui, e);
                    }
                    if let Err(why) = project(model) {
                        crate::menu::error_label(ui, why);
                    }
                    start = ui
                        .add_enabled(
                            w.file.is_some() && project(model).is_ok() && self.job.is_none(),
                            egui::Button::new("Import"),
                        )
                        .clicked();
                });
            if start {
                if w.name.trim().is_empty() {
                    w.error = Some("give the drawing a name".into());
                } else if let (Ok(file), Some(dxf)) = (project(model), w.file.clone()) {
                    let (name, crs) = (w.name.trim().to_owned(), model.line.crs.clone());
                    self.job = Some(crate::survey_io::Job::start(
                        format!("Import drawing {name}"),
                        true,
                        ctx,
                        move |report| {
                            report(format!("reading {}", dxf.display()), 0.1);
                            let store =
                                ohl_background::Store::open(&file).map_err(|e| e.to_string())?;
                            let (info, d) = store
                                .import_dxf(&name, &dxf, &crs)
                                .map_err(|e| e.to_string())?;
                            let mut text = format!(
                                "Drawing {name}: {} lines, {} points, {} texts on {} layers.",
                                d.lines.len(),
                                d.points.len(),
                                d.texts.len(),
                                d.layers.len()
                            );
                            if !d.skipped.is_empty() {
                                let s: Vec<String> =
                                    d.skipped.iter().map(|(k, n)| format!("{k} ×{n}")).collect();
                                text.push_str(&format!("\nNot drawn: {}.", s.join(", ")));
                            }
                            if let Some((lo, hi)) = info.bounds {
                                text.push_str(&format!(
                                    "\nExtent: {:.1} … {:.1} E, {:.1} … {:.1} N, heights {:.1} … {:.1}.",
                                    lo[0], hi[0], lo[1], hi[1], lo[2], hi[2]
                                ));
                            }
                            Ok(text)
                        },
                    ));
                    open = false;
                }
            }
            if !open {
                self.drawing = None;
            }
        }
        if let Some(asked) = &mut self.remove {
            let mut open = true;
            let mut removed = false;
            egui::Window::new("Remove backgrounds")
                .open(&mut open)
                .default_width(420.0)
                .show(ctx, |ui| {
                    let bg = &model.backgrounds;
                    if bg.is_empty() {
                        ui.label("The project has no backgrounds.");
                    }
                    let Ok(file) = project(model) else {
                        ui.label("Read-only: nothing can be removed.");
                        return;
                    };
                    let mut row = |ui: &mut egui::Ui, key: String, text: String| {
                        ui.horizontal(|ui| {
                            ui.label(&text);
                            if asked.as_ref() == Some(&key) {
                                if ui.button("Remove: sure").clicked() {
                                    let r =
                                        ohl_background::Store::open(&file).and_then(|s| match key
                                            .split_once(':')
                                        {
                                            Some(("image", l)) => {
                                                s.delete_image_layer(l).map(|_| ())
                                            }
                                            Some((_, n)) => s.delete_drawing(n),
                                            None => Ok(()),
                                        });
                                    if let Err(e) = r {
                                        log::warn!("{e}");
                                    }
                                    removed = true;
                                }
                                if ui.button("Keep").clicked() {
                                    *asked = None;
                                }
                            } else if ui.button("Remove…").clicked() {
                                *asked = Some(key.clone());
                            }
                        });
                    };
                    for (layer, n) in bg.image_layers() {
                        row(
                            ui,
                            image_key(&layer),
                            format!("Orthophotos {layer} ({n} images)"),
                        );
                    }
                    for d in &bg.drawings {
                        row(
                            ui,
                            drawing_key(&d.info.name),
                            format!("Drawing {} ({} items)", d.info.name, d.info.items),
                        );
                    }
                });
            if removed {
                reopen = true;
                open = false;
            }
            if !open {
                self.remove = None;
            }
        }
        if let Some(job) = &mut self.job
            && crate::survey_io::job_window(ctx, job)
        {
            reopen = matches!(job.done, Some(Ok(_)));
            self.job = None;
        }
        reopen
    }
}

/// The Terrain › Backgrounds menu: imports, removal, and what is shown
/// (ticks: the menu stays open).
pub fn menu(ui: &mut egui::Ui, r: &mut crate::app::Ready) {
    let busy = r.backgrounds.job.is_some();
    if ui
        .add_enabled(!busy, egui::Button::new("Import orthophotos or maps…"))
        .on_hover_text(
            "GeoTIFF, or TIFF/JPEG/PNG with a world file; ECW through a converter. A map is \
             taken first by the route and situation plans and the key map",
        )
        .clicked()
    {
        r.backgrounds.images = Some(ImageImport::new());
        ui.close();
    }
    if ui
        .add_enabled(!busy, egui::Button::new("Import DXF drawing…"))
        .on_hover_text("e.g. the cadastre: drawn in the plan and 3D views")
        .clicked()
    {
        r.backgrounds.drawing = Some(DrawingImport::default());
        ui.close();
    }
    if ui
        .add_enabled(
            !r.model.backgrounds.is_empty(),
            egui::Button::new("Remove backgrounds…"),
        )
        .clicked()
    {
        r.backgrounds.remove = Some(None);
        ui.close();
    }
    ui.separator();
    let layer = |ui: &mut egui::Ui, layer: Layer, name: &str, r: &mut crate::app::Ready| {
        let mut on = r.visible.contains(&layer);
        if ui.checkbox(&mut on, name).changed() {
            // Shown or hidden as it is: the 3D view keeps its chunks
            // (indexing a large drawing again takes a while).
            if on {
                r.visible.insert(layer);
            } else {
                r.visible.remove(&layer);
            }
        }
    };
    let layers = r.model.backgrounds.image_layers();
    if layers.is_empty() {
        ui.add_enabled(false, egui::Button::new("Orthophotos: none"));
    } else {
        layer(
            ui,
            Layer::Orthophotos,
            "Orthophotos and maps (plan view)",
            r,
        );
        let mut turn: Option<(String, ohl_background::LayerKind)> = None;
        for (l, n) in layers {
            let key = image_key(&l);
            let kind = r.model.backgrounds.kind(&l);
            let mut on = !r.model.hidden_backgrounds.contains(&key);
            ui.horizontal(|ui| {
                if ui.checkbox(&mut on, format!("   {l} ({n})")).changed() {
                    if on {
                        r.model.hidden_backgrounds.remove(&key);
                    } else {
                        r.model.hidden_backgrounds.insert(key.clone());
                    }
                }
                // Its kind, turned with a click (kept in the project).
                let (now, other) = match kind {
                    ohl_background::LayerKind::Map => {
                        ("map", ohl_background::LayerKind::Orthophoto)
                    }
                    ohl_background::LayerKind::Orthophoto => {
                        ("orthophoto", ohl_background::LayerKind::Map)
                    }
                };
                if ui
                    .add_enabled(project(&r.model).is_ok(), egui::Button::new(now).small())
                    .on_hover_text(format!(
                        "A {now}; click to make it a{} (maps are taken first by the route \
                         and situation plans and the key map)",
                        if other == ohl_background::LayerKind::Map {
                            " map"
                        } else {
                            "n orthophoto"
                        }
                    ))
                    .clicked()
                {
                    turn = Some((l.clone(), other));
                }
            });
        }
        if let Some((l, kind)) = turn {
            let done = project(&r.model).and_then(|file| {
                ohl_background::Store::open(&file)
                    .and_then(|s| s.set_layer_kind(&l, kind))
                    .map_err(|e| e.to_string())
            });
            match done {
                Ok(()) => {
                    r.model.backgrounds.kinds.insert(l, kind);
                }
                Err(e) => r.status = Some((format!("layer {l}: {e}"), true)),
            }
        }
    }
    let names: Vec<String> = r
        .model
        .backgrounds
        .drawings
        .iter()
        .map(|d| d.info.name.clone())
        .collect();
    if names.is_empty() {
        ui.add_enabled(false, egui::Button::new("Drawings: none"));
    } else {
        layer(ui, Layer::Drawings, "Drawings (plan and 3D views)", r);
        for n in names {
            let key = drawing_key(&n);
            let mut on = !r.model.hidden_backgrounds.contains(&key);
            if ui.checkbox(&mut on, format!("   {n}")).changed() {
                if on {
                    r.model.hidden_backgrounds.remove(&key);
                } else {
                    r.model.hidden_backgrounds.insert(key);
                }
                r.view3d.dirty.insert(Layer::Drawings);
            }
        }
    }
}
