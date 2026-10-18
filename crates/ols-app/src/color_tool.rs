// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The colour tool: a small window to choose a colour, by a colour wheel
//! (hue and saturation, a brightness slider) or by sliders (RGB, HSV, hex),
//! with the old and the new colour side by side. Below, the user's palette:
//! named colours with what each is used for, picked with a click; colours
//! are added, edited, moved and deleted there, and the palette is exported
//! and imported as a small file (`.ohlcolors`, TOML):
//!
//! ```toml
//! format = "ohl-colors"
//! version = 1
//!
//! [[color]]
//! name = "Phase 1"
//! color = "#e4572e"
//! description = "380 kV circuits"
//! ```
//!
//! The palette is this user's (remembered on this computer, not in the
//! project). The tables open the tool from their colour cells; anything
//! else with a colour can open it the same way ([`ColorTool::show`]).

use std::path::Path;

use eframe::egui;
use ols_core::text::{format_rgb, parse_rgb};

/// The file format of an exported palette.
pub const PALETTE_FILE_FORMAT: &str = "ohl-colors";
/// Its version.
pub const PALETTE_FILE_VERSION: i64 = 1;
/// Its file extension.
pub const PALETTE_FILE_EXTENSION: &str = "ohlcolors";

/// A colour of the palette.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteEntry {
    /// Its name.
    pub name: String,
    /// The colour.
    pub color: [u8; 3],
    /// What it is used for.
    pub description: String,
}

/// How colours are chosen: by the wheel or by sliders.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    /// Hue and saturation on a wheel, brightness by a slider.
    #[default]
    Wheel,
    /// RGB and HSV sliders.
    Sliders,
}

/// The user's colours, and how they like to choose one.
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    /// The colours, in order.
    pub entries: Vec<PaletteEntry>,
    /// Wheel or sliders.
    pub mode: Mode,
}

impl Default for Palette {
    /// The colours the program uses by default: phases, wires without a
    /// phase number, electrical systems.
    fn default() -> Self {
        let mut entries: Vec<PaletteEntry> = ols_line::DEFAULT_PHASE_COLORS
            .iter()
            .enumerate()
            .map(|(k, c)| PaletteEntry {
                name: format!("Phase {}", k + 1),
                color: parse_rgb(c).unwrap_or([128, 128, 128]),
                description: format!("default colour of phase {} (e.g. L{})", k + 1, k + 1),
            })
            .collect();
        entries.push(PaletteEntry {
            name: "Neutral".into(),
            color: parse_rgb(ols_line::NEUTRAL_COLOR).unwrap_or([128, 128, 128]),
            description: "default colour of wires without a phase number".into(),
        });
        entries.extend(
            ols_line::electrical::SYSTEM_COLORS
                .iter()
                .enumerate()
                .map(|(k, c)| PaletteEntry {
                    name: format!("System {}", k + 1),
                    color: parse_rgb(c).unwrap_or([128, 128, 128]),
                    description: format!(
                        "default colour of the electrical system in place {}",
                        k + 1
                    ),
                }),
        );
        Self {
            entries,
            mode: Mode::Wheel,
        }
    }
}

impl Palette {
    /// The palette as a file (`.ohlcolors`).
    pub fn to_toml(&self) -> String {
        let mut doc = toml_edit::DocumentMut::new();
        doc["format"] = toml_edit::value(PALETTE_FILE_FORMAT);
        doc["version"] = toml_edit::value(PALETTE_FILE_VERSION);
        let mut array = toml_edit::ArrayOfTables::new();
        for e in &self.entries {
            let mut t = toml_edit::Table::new();
            t["name"] = toml_edit::value(e.name.as_str());
            t["color"] = toml_edit::value(format_rgb(e.color));
            t["description"] = toml_edit::value(e.description.as_str());
            array.push(t);
        }
        doc.insert("color", toml_edit::Item::ArrayOfTables(array));
        doc.to_string()
    }

    /// The colours of a palette file; every problem found when it does not
    /// read (one per line).
    pub fn parse(text: &str) -> Result<Vec<PaletteEntry>, String> {
        let doc: toml_edit::DocumentMut = text
            .parse()
            .map_err(|e: toml_edit::TomlError| format!("not a TOML file: {}", e.message()))?;
        // The header as every format's (both optional, no newer version).
        let version = match doc.get("version") {
            None => None,
            Some(v) => Some(
                v.as_integer()
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or("version: give a whole number")?,
            ),
        };
        ols_core::files::check_header(
            doc.get("format").and_then(|v| v.as_str()),
            version,
            PALETTE_FILE_FORMAT,
            PALETTE_FILE_VERSION as u32,
            "colour palette",
        )?;
        let Some(item) = doc.get("color") else {
            return Ok(Vec::new());
        };
        let Some(array) = item.as_array_of_tables() else {
            return Err("color: write each colour as a [[color]] table".into());
        };
        let mut entries = Vec::new();
        let mut errors = Vec::new();
        for (k, t) in array.iter().enumerate() {
            let n = k + 1;
            let name = t.get("name").and_then(|v| v.as_str()).map(str::trim);
            let name = match name {
                Some(s) if !s.is_empty() => s.to_owned(),
                _ => {
                    errors.push(format!("colour {n}: name is missing"));
                    String::new()
                }
            };
            let what = if name.is_empty() {
                format!("colour {n}")
            } else {
                format!("colour {n} ({name})")
            };
            let color = match t.get("color") {
                None => {
                    errors.push(format!("{what}: color is missing (write #rrggbb)"));
                    None
                }
                Some(v) => match v.as_str().and_then(|s| parse_rgb(s.trim())) {
                    Some(c) => Some(c),
                    None => {
                        errors.push(format!(
                            "{what}: color {} — write #rrggbb, e.g. #4c9be8",
                            v.to_string().trim()
                        ));
                        None
                    }
                },
            };
            let description = match t.get("description") {
                None => String::new(),
                Some(v) => match v.as_str() {
                    Some(s) => s.trim().to_owned(),
                    None => {
                        errors.push(format!("{what}: description is not text"));
                        String::new()
                    }
                },
            };
            if let Some(color) = color
                && !name.is_empty()
            {
                entries.push(PaletteEntry {
                    name,
                    color,
                    description,
                });
            }
        }
        if errors.is_empty() {
            Ok(entries)
        } else {
            Err(errors.join("\n"))
        }
    }

    /// Reads a palette file: its colours, or what is wrong with it.
    pub fn read(path: &Path) -> Result<Vec<PaletteEntry>, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text).map_err(|e| format!("{}:\n{e}", path.display()))
    }

    /// Writes the palette to a file.
    pub fn write(&self, path: &Path) -> Result<(), String> {
        std::fs::write(path, self.to_toml()).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Adds the colours not in it yet (same name and colour); how many.
    pub fn add(&mut self, entries: Vec<PaletteEntry>) -> usize {
        let mut n = 0;
        for e in entries {
            if !self
                .entries
                .iter()
                .any(|x| x.name == e.name && x.color == e.color)
            {
                self.entries.push(e);
                n += 1;
            }
        }
        n
    }

    /// Saved with the window layout (this user, this computer).
    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        storage.set_string("color-palette", self.to_toml());
        storage.set_string(
            "color-tool-mode",
            match self.mode {
                Mode::Wheel => "wheel",
                Mode::Sliders => "sliders",
            }
            .into(),
        );
    }

    /// As saved; the default palette when none was (or it does not read).
    pub fn load(storage: &dyn eframe::Storage) -> Self {
        let mut p = Self::default();
        if let Some(entries) = storage
            .get_string("color-palette")
            .and_then(|t| Self::parse(&t).ok())
        {
            p.entries = entries;
        }
        if storage.get_string("color-tool-mode").as_deref() == Some("sliders") {
            p.mode = Mode::Sliders;
        }
        p
    }
}

/// RGB → hue (degrees, 0–360), saturation and value (0–1).
pub fn rgb_to_hsv(c: [u8; 3]) -> [f64; 3] {
    let [r, g, b] = c.map(|x| f64::from(x) / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d <= 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let s = if max <= 0.0 { 0.0 } else { d / max };
    [h.rem_euclid(360.0), s, max]
}

/// Hue (degrees), saturation and value (0–1) → RGB.
pub fn hsv_to_rgb([h, s, v]: [f64; 3]) -> [u8; 3] {
    let h = h.rem_euclid(360.0) / 60.0;
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let c = v * s;
    let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r, g, b].map(|t| ((t + m) * 255.0).round().clamp(0.0, 255.0) as u8)
}

/// Hue (degrees) and saturation at point `p` of a wheel at `center` with
/// `radius`: red to the right, the hues counter-clockwise (yellow, green
/// up, cyan left, blue, magenta), grey at the centre, full at the rim (and
/// beyond).
pub fn wheel_hs(center: egui::Pos2, radius: f32, p: egui::Pos2) -> (f64, f64) {
    let dx = f64::from(p.x - center.x);
    let dy = f64::from(center.y - p.y);
    let h = dy.atan2(dx).to_degrees().rem_euclid(360.0);
    let s = ((dx * dx + dy * dy).sqrt() / f64::from(radius.max(1.0))).min(1.0);
    (h, s)
}

/// Where hue `h` and saturation `s` are on the wheel ([`wheel_hs`]).
pub fn wheel_point(center: egui::Pos2, radius: f32, h: f64, s: f64) -> egui::Pos2 {
    let a = h.to_radians();
    let r = f64::from(radius) * s.clamp(0.0, 1.0);
    egui::pos2(
        center.x + (r * a.cos()) as f32,
        center.y - (r * a.sin()) as f32,
    )
}

/// A colour of the program's.
pub fn color32(c: [u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(c[0], c[1], c[2])
}

/// Text readable on `c`: dark on light colours, light on dark ones.
pub fn text_on(c: egui::Color32) -> egui::Color32 {
    let l = 0.2126 * f32::from(c.r()) + 0.7152 * f32::from(c.g()) + 0.0722 * f32::from(c.b());
    if l > 140.0 {
        egui::Color32::from_gray(20)
    } else {
        egui::Color32::from_gray(240)
    }
}

/// What the tool ends with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// OK: this colour.
    Pick([u8; 3]),
    /// Cancel (or closed): no change.
    Cancel,
}

/// An entry being edited: its place, name, description, colour as typed.
struct Draft {
    index: usize,
    name: String,
    description: String,
    hex: String,
}

/// An open colour tool.
pub struct ColorTool {
    /// The window's title.
    pub title: String,
    /// The colour before (none: none set).
    pub old: Option<[u8; 3]>,
    /// The colour chosen.
    pub rgb: [u8; 3],
    /// It as hue (degrees), saturation, value (kept apart: grey keeps its hue).
    pub hsv: [f64; 3],
    hex: String,
    new_name: String,
    new_description: String,
    editing: Option<Draft>,
    /// The entry to delete, asked first.
    delete: Option<usize>,
    /// Colours read from a file: replace the palette or add to it?
    import: Option<(String, Vec<PaletteEntry>)>,
    /// The last thing done (error: true).
    pub status: Option<(String, bool)>,
    /// A palette colour double-clicked: OK.
    ok: bool,
}

impl ColorTool {
    /// A tool starting at `old` (none: grey).
    pub fn new(title: impl Into<String>, old: Option<[u8; 3]>) -> Self {
        let rgb = old.unwrap_or([128, 128, 128]);
        Self {
            title: title.into(),
            old,
            rgb,
            hsv: rgb_to_hsv(rgb),
            hex: format_rgb(rgb),
            new_name: String::new(),
            new_description: String::new(),
            editing: None,
            delete: None,
            import: None,
            status: None,
            ok: false,
        }
    }

    /// Sets the colour (the hue kept for greys, hue and saturation for black).
    pub fn set_rgb(&mut self, c: [u8; 3]) {
        let [h, s, v] = rgb_to_hsv(c);
        self.hsv = if v <= 0.0 {
            [self.hsv[0], self.hsv[1], 0.0]
        } else if s <= 0.0 {
            [self.hsv[0], 0.0, v]
        } else {
            [h, s, v]
        };
        self.rgb = c;
        self.hex = format_rgb(c);
    }

    /// Sets the colour by hue, saturation and value.
    pub fn set_hsv(&mut self, hsv: [f64; 3]) {
        self.hsv = [
            hsv[0].clamp(0.0, 360.0),
            hsv[1].clamp(0.0, 1.0),
            hsv[2].clamp(0.0, 1.0),
        ];
        self.rgb = hsv_to_rgb(self.hsv);
        self.hex = format_rgb(self.rgb);
    }

    /// Reads a palette file to import (asked next: replace or add).
    pub fn import_file(&mut self, path: &Path) {
        match Palette::read(path) {
            Ok(entries) => {
                self.status = None;
                self.import = Some((path.display().to_string(), entries));
            }
            Err(e) => {
                self.import = None;
                self.status = Some((e, true));
            }
        }
    }

    /// Imports the colours read: in place of the palette's, or added.
    pub fn finish_import(&mut self, palette: &mut Palette, replace: bool) {
        let Some((file, entries)) = self.import.take() else {
            return;
        };
        let n = entries.len();
        self.editing = None;
        self.delete = None;
        if replace {
            palette.entries = entries;
            self.status = Some((format!("{n} colours from {file}"), false));
        } else {
            let added = palette.add(entries);
            self.status = Some((
                format!("{added} of {n} colours added from {file} (the others were there)"),
                false,
            ));
        }
    }

    /// The window (identified by `id`); the outcome once OK or Cancel.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        id: egui::Id,
        palette: &mut Palette,
    ) -> Option<Outcome> {
        let mut open = true;
        let mut outcome = None;
        egui::Window::new(self.title.clone())
            .id(id)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(380.0)
            .show(ctx, |ui| {
                outcome = self.ui(ui, id, palette);
            });
        if !open {
            outcome = Some(Outcome::Cancel);
        }
        outcome
    }

    fn ui(&mut self, ui: &mut egui::Ui, id: egui::Id, palette: &mut Palette) -> Option<Outcome> {
        let mut outcome = None;
        ui.horizontal(|ui| {
            ui.selectable_value(&mut palette.mode, Mode::Wheel, "Wheel")
                .on_hover_text("Hue and saturation on a wheel, brightness below");
            ui.selectable_value(&mut palette.mode, Mode::Sliders, "Sliders")
                .on_hover_text("Red, green, blue; hue, saturation, brightness");
        });
        ui.separator();
        match palette.mode {
            Mode::Wheel => self.wheel(ui),
            Mode::Sliders => self.sliders(ui),
        }
        ui.add_space(4.0);
        self.preview(ui);
        ui.separator();
        self.palette_ui(ui, id, palette);
        if std::mem::take(&mut self.ok) {
            outcome = Some(Outcome::Pick(self.rgb));
        }
        if let Some((t, error)) = &self.status {
            let t = egui::RichText::new(t).small();
            if *error {
                crate::menu::error_label(ui, t);
            } else {
                ui.label(t.weak());
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            if ui
                .button("OK")
                .on_hover_text(format!("Use {}", format_rgb(self.rgb)))
                .clicked()
            {
                outcome = Some(Outcome::Pick(self.rgb));
            }
            if ui.button("Cancel").clicked() {
                outcome = Some(Outcome::Cancel);
            }
        });
        outcome
    }

    /// The wheel and its brightness slider.
    fn wheel(&mut self, ui: &mut egui::Ui) {
        let size = 200.0;
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click_and_drag());
        let response = response.on_hover_text("Click or drag: hue round, saturation out");
        let center = rect.center();
        let radius = size / 2.0 - 4.0;
        if (response.dragged() || response.clicked() || response.is_pointer_button_down_on())
            && let Some(p) = response.interact_pointer_pos()
        {
            let (h, s) = wheel_hs(center, radius, p);
            self.set_hsv([h, s, self.hsv[2]]);
        }
        if ui.is_rect_visible(rect) {
            ui.painter()
                .add(wheel_mesh(center, radius, self.hsv[2], 6, 72));
            let at = wheel_point(center, radius, self.hsv[0], self.hsv[1]);
            let ring = text_on(color32(self.rgb));
            ui.painter()
                .circle(at, 5.0, color32(self.rgb), egui::Stroke::new(2.0, ring));
        }
        let mut v = self.hsv[2] * 100.0;
        if ui
            .add(
                egui::Slider::new(&mut v, 0.0..=100.0)
                    .text("brightness %")
                    .max_decimals(1)
                    .clamping(egui::SliderClamping::Edits),
            )
            .changed()
        {
            self.set_hsv([self.hsv[0], self.hsv[1], v / 100.0]);
        }
    }

    /// RGB and HSV sliders.
    fn sliders(&mut self, ui: &mut egui::Ui) {
        let mut rgb = self.rgb;
        let mut changed = false;
        for (k, name) in ["R", "G", "B"].into_iter().enumerate() {
            changed |= ui
                .add(egui::Slider::new(&mut rgb[k], 0..=255).text(name))
                .changed();
        }
        if changed {
            self.set_rgb(rgb);
        }
        ui.add_space(4.0);
        let mut hsv = [self.hsv[0], self.hsv[1] * 100.0, self.hsv[2] * 100.0];
        let mut changed = false;
        changed |= ui
            .add(
                egui::Slider::new(&mut hsv[0], 0.0..=360.0)
                    .text("H °")
                    .max_decimals(1)
                    .clamping(egui::SliderClamping::Edits),
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut hsv[1], 0.0..=100.0)
                    .text("S %")
                    .max_decimals(1)
                    .clamping(egui::SliderClamping::Edits),
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut hsv[2], 0.0..=100.0)
                    .text("V %")
                    .max_decimals(1)
                    .clamping(egui::SliderClamping::Edits),
            )
            .changed();
        if changed {
            self.set_hsv([hsv[0], hsv[1] / 100.0, hsv[2] / 100.0]);
        }
    }

    /// The old and the new colour, and the new one as hex.
    fn preview(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let swatch = |ui: &mut egui::Ui, c: Option<[u8; 3]>, label: &str| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(64.0, 28.0), egui::Sense::hover());
                let p = ui.painter();
                match c {
                    Some(c) => {
                        p.rect_filled(rect, 3.0, color32(c));
                        p.text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            label,
                            egui::FontId::proportional(11.0),
                            text_on(color32(c)),
                        );
                    }
                    None => {
                        let stroke = ui.visuals().widgets.noninteractive.fg_stroke;
                        p.rect_stroke(rect, 3.0, stroke, egui::StrokeKind::Inside);
                        p.text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            format!("{label}: none"),
                            egui::FontId::proportional(11.0),
                            stroke.color,
                        );
                    }
                }
            };
            ui.spacing_mut().item_spacing.x = 0.0;
            swatch(ui, self.old, "old");
            swatch(ui, Some(self.rgb), "new");
            ui.add_space(12.0);
            if ui
                .add(
                    egui::TextEdit::singleline(&mut self.hex)
                        .desired_width(80.0)
                        .hint_text("#rrggbb"),
                )
                .on_hover_text("The new colour as #rrggbb")
                .changed()
                && let Some(c) = parse_rgb(self.hex.trim())
            {
                let hex = self.hex.clone();
                self.set_rgb(c);
                // Keep it as typed (case).
                self.hex = hex;
            }
        });
    }

    /// The palette: its colours, add, export, import.
    fn palette_ui(&mut self, ui: &mut egui::Ui, id: egui::Id, palette: &mut Palette) {
        ui.label(egui::RichText::new("Palette").strong())
            .on_hover_text("Your colours, remembered on this computer (not in the project)");
        enum Act {
            Pick,
            Ok,
            Up,
            Down,
            Edit,
            Delete,
        }
        let mut act: Option<(usize, Act)> = None;
        let mut save_draft = false;
        let mut confirmed_delete = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt(id.with("palette"))
                .max_height(200.0)
                .min_scrolled_height(200.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if palette.entries.is_empty() {
                        ui.label(egui::RichText::new("No colours: add the one above").weak());
                    }
                    let n = palette.entries.len();
                    for (i, e) in palette.entries.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let (rect, swatch) = ui
                                .allocate_exact_size(egui::vec2(22.0, 16.0), egui::Sense::click());
                            ui.painter().rect_filled(rect, 2.0, color32(e.color));
                            if e.color == self.rgb {
                                ui.painter().rect_stroke(
                                    rect.expand(1.0),
                                    2.0,
                                    ui.visuals().selection.stroke,
                                    egui::StrokeKind::Outside,
                                );
                            }
                            let name = ui
                                .selectable_label(e.color == self.rgb, &e.name)
                                .on_hover_text(if e.description.is_empty() {
                                    "Click: take it; double-click: take it and close".to_owned()
                                } else {
                                    format!(
                                        "{} — click: take it; double-click: take it and close",
                                        e.description
                                    )
                                });
                            if swatch.clicked() || name.clicked() {
                                act = Some((i, Act::Pick));
                            }
                            if swatch.double_clicked() || name.double_clicked() {
                                act = Some((i, Act::Ok));
                            }
                            ui.label(egui::RichText::new(format_rgb(e.color)).small().weak());
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .small_button("🗑")
                                        .on_hover_text("Delete it (asked first)")
                                        .clicked()
                                    {
                                        act = Some((i, Act::Delete));
                                    }
                                    if ui.small_button("✏").on_hover_text("Edit it").clicked() {
                                        act = Some((i, Act::Edit));
                                    }
                                    if ui
                                        .add_enabled(i + 1 < n, egui::Button::new("⏷").small())
                                        .on_hover_text("Move it down")
                                        .clicked()
                                    {
                                        act = Some((i, Act::Down));
                                    }
                                    if ui
                                        .add_enabled(i > 0, egui::Button::new("⏶").small())
                                        .on_hover_text("Move it up")
                                        .clicked()
                                    {
                                        act = Some((i, Act::Up));
                                    }
                                },
                            );
                        });
                        if !e.description.is_empty() {
                            ui.horizontal(|ui| {
                                ui.add_space(26.0);
                                ui.label(egui::RichText::new(&e.description).small().weak());
                            });
                        }
                        if self.delete == Some(i) {
                            ui.horizontal(|ui| {
                                ui.add_space(26.0);
                                ui.colored_label(
                                    ui.visuals().warn_fg_color,
                                    format!("Delete \"{}\"?", e.name),
                                );
                                if ui.button("Delete").clicked() {
                                    confirmed_delete = true;
                                }
                                if ui.button("Keep").clicked() {
                                    self.delete = None;
                                }
                            });
                        }
                        if let Some(d) = &mut self.editing
                            && d.index == i
                        {
                            egui::Frame::group(ui.style()).show(ui, |ui| {
                                egui::Grid::new(id.with("edit"))
                                    .num_columns(2)
                                    .show(ui, |ui| {
                                        ui.label("name");
                                        ui.text_edit_singleline(&mut d.name);
                                        ui.end_row();
                                        ui.label("used for");
                                        ui.text_edit_singleline(&mut d.description);
                                        ui.end_row();
                                        ui.label("colour");
                                        ui.horizontal(|ui| {
                                            ui.add(
                                                egui::TextEdit::singleline(&mut d.hex)
                                                    .desired_width(80.0),
                                            );
                                            if ui
                                                .small_button("the new colour")
                                                .on_hover_text("The colour chosen above")
                                                .clicked()
                                            {
                                                d.hex = format_rgb(self.rgb);
                                            }
                                        });
                                        ui.end_row();
                                    });
                                ui.horizontal(|ui| {
                                    if ui.button("Save").clicked() {
                                        save_draft = true;
                                    }
                                    if ui.button("Cancel").clicked() {
                                        act = Some((usize::MAX, Act::Edit));
                                    }
                                });
                            });
                        }
                    }
                });
        });
        if confirmed_delete && let Some(i) = self.delete.take() {
            if i < palette.entries.len() {
                let e = palette.entries.remove(i);
                self.status = Some((format!("deleted \"{}\"", e.name), false));
            }
            self.editing = None;
        }
        if save_draft && let Some(d) = &self.editing {
            match parse_rgb(d.hex.trim()) {
                _ if d.name.trim().is_empty() => {
                    self.status = Some(("give it a name".into(), true));
                }
                None => {
                    self.status = Some((
                        format!("colour \"{}\" — write #rrggbb, e.g. #4c9be8", d.hex),
                        true,
                    ));
                }
                Some(c) => {
                    if let Some(e) = palette.entries.get_mut(d.index) {
                        e.name = d.name.trim().to_owned();
                        e.description = d.description.trim().to_owned();
                        e.color = c;
                    }
                    self.editing = None;
                    self.status = None;
                }
            }
        }
        match act {
            Some((i, Act::Pick)) => {
                let c = palette.entries[i].color;
                self.set_rgb(c);
            }
            Some((i, Act::Ok)) => {
                let c = palette.entries[i].color;
                self.set_rgb(c);
                self.ok = true;
            }
            Some((i, Act::Up)) => {
                palette.entries.swap(i, i - 1);
                self.editing = None;
                self.delete = None;
            }
            Some((i, Act::Down)) => {
                palette.entries.swap(i, i + 1);
                self.editing = None;
                self.delete = None;
            }
            Some((i, Act::Edit)) => {
                self.editing = palette.entries.get(i).map(|e| Draft {
                    index: i,
                    name: e.name.clone(),
                    description: e.description.clone(),
                    hex: format_rgb(e.color),
                });
            }
            Some((i, Act::Delete)) => {
                self.delete = Some(i);
                self.editing = None;
            }
            None => {}
        }
        // Add the new colour.
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.new_name)
                    .hint_text("name")
                    .desired_width(90.0),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.new_description)
                    .hint_text("what it is used for")
                    .desired_width(150.0),
            );
            if ui
                .add_enabled(!self.new_name.trim().is_empty(), egui::Button::new("+ Add"))
                .on_hover_text("Add the new colour to the palette")
                .on_disabled_hover_text("Give it a name first")
                .clicked()
            {
                palette.entries.push(PaletteEntry {
                    name: self.new_name.trim().to_owned(),
                    color: self.rgb,
                    description: self.new_description.trim().to_owned(),
                });
                self.new_name.clear();
                self.new_description.clear();
                self.status = None;
            }
        });
        ui.horizontal(|ui| {
            if ui
                .button("Export…")
                .on_hover_text("Write the palette to a file (.ohlcolors) to share")
                .clicked()
                && let Some(p) = rfd::FileDialog::new()
                    .add_filter("Colour palette", &[PALETTE_FILE_EXTENSION])
                    .set_file_name(format!("palette.{PALETTE_FILE_EXTENSION}"))
                    .save_file()
            {
                self.status = Some(match palette.write(&p) {
                    Ok(()) => (format!("exported to {}", p.display()), false),
                    Err(e) => (e, true),
                });
            }
            if ui
                .button("Import…")
                .on_hover_text("Read a palette file (.ohlcolors): replace yours or add to it")
                .clicked()
                && let Some(p) = rfd::FileDialog::new()
                    .add_filter(
                        "Colour palette",
                        &ols_core::files::open_extensions(PALETTE_FILE_EXTENSION),
                    )
                    .pick_file()
            {
                self.import_file(&p);
            }
            if ui
                .button("Defaults")
                .on_hover_text("Add the program's default colours (those missing)")
                .clicked()
            {
                let n = palette.add(Palette::default().entries);
                self.status = Some((format!("{n} default colours added"), false));
            }
        });
        if let Some((file, entries)) = &self.import {
            let text = format!("{} colours in {file}:", entries.len());
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(ui.visuals().warn_fg_color, text);
            });
            let mut choice = None;
            ui.horizontal(|ui| {
                if ui
                    .button("Replace the palette")
                    .on_hover_text("Yours goes; export it first to keep it")
                    .clicked()
                {
                    choice = Some(true);
                }
                if ui
                    .button("Add to it")
                    .on_hover_text("Those not there yet (same name and colour)")
                    .clicked()
                {
                    choice = Some(false);
                }
                if ui.button("Cancel").clicked() {
                    self.import = None;
                }
            });
            if let Some(replace) = choice {
                self.finish_import(palette, replace);
            }
        }
    }
}

/// The wheel at value `v`: `rings` × `segments` cells, each corner its
/// exact colour.
fn wheel_mesh(center: egui::Pos2, radius: f32, v: f64, rings: u32, segments: u32) -> egui::Shape {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(center, color32(hsv_to_rgb([0.0, 0.0, v])));
    for i in 1..=rings {
        let s = f64::from(i) / f64::from(rings);
        for j in 0..segments {
            let h = 360.0 * f64::from(j) / f64::from(segments);
            mesh.colored_vertex(
                wheel_point(center, radius, h, s),
                color32(hsv_to_rgb([h, s, v])),
            );
        }
    }
    let at = |i: u32, j: u32| 1 + (i - 1) * segments + j % segments;
    for j in 0..segments {
        mesh.add_triangle(0, at(1, j), at(1, j + 1));
    }
    for i in 1..rings {
        for j in 0..segments {
            let (a, b) = (at(i, j), at(i, j + 1));
            let (c, d) = (at(i + 1, j), at(i + 1, j + 1));
            mesh.add_triangle(a, c, d);
            mesh.add_triangle(a, d, b);
        }
    }
    egui::Shape::mesh(mesh)
}
