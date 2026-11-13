// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **String builder** (owner 2026-10-31, "the builder window with a
//! torus"): Line › Structures › String builder…, and the String shapes and
//! String library tables' **String builder** button. An insulator
//! assembly's **volumes on the strings** ([`ohl_structures::StringVolumes`])
//! built from basic shapes — sphere, cylinder, box, cone and torus (a
//! grading ring) —, each grounded, electrified or insulated, as the
//! internal clearance check measures them (`ohl_clearance::internal`).
//!
//! The window works on a copy of the assembly's volumes (its **draft**):
//! shapes added (one at a time, or the common fittings in one click),
//! duplicated, removed, their fields edited with each field explained
//! beside it; **presets** to start from (an I string, a V string's legs, a
//! double string); a **preview** in the string's own axes (front: across;
//! side: along; the string vertical, its top at the top, the wire point at
//! the bottom; the shapes in their state's colours as the 3D view draws
//! them). **Save to the library** writes the assembly's file through the
//! same path as the String shapes table (one undoable change,
//! [`crate::app::Ready::write_insulator_files`]); **Revert** takes the
//! library's volumes again. A draft without changes follows the library
//! (an undo, the tables).
//!
//! The presets' and fittings' sizes are **starting values, INVENTED**
//! (no manufacturer's): set them to the real fittings.

use std::path::PathBuf;

use eframe::egui;
use ohl_structures::{
    Along, Arrangement, AssemblyKind, ElectricalState, Freedom, ShapeKind, StringShape,
    StringVolumes,
};

use crate::check_help as help;
use crate::model::Model;

/// The window's id (its body and footer).
const WINDOW: &str = "string-builder";

/// The colours of the electrical states, as the 3D view draws the volumes
/// (`crate::scene`): grounded grey, electrified orange, insulated pale.
pub fn state_colour(state: ElectricalState) -> egui::Color32 {
    match state {
        ElectricalState::Grounded => egui::Color32::from_rgb(150, 150, 155),
        ElectricalState::Electrified => egui::Color32::from_rgb(240, 150, 60),
        ElectricalState::Insulated => egui::Color32::from_rgb(225, 220, 180),
    }
}

/// An assembly's volumes as edited.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    /// The assembly (its library label).
    pub id: String,
    /// Its volumes as edited.
    pub volumes: StringVolumes,
    /// Its volumes in the library when taken (unchanged: the draft follows
    /// the library).
    pub from: StringVolumes,
    /// Its string's length, m (each leg's: two strings to one clamp).
    pub length: f64,
    /// Two strings to one clamp (a V string: each leg carries the shapes).
    pub legs: bool,
}

impl Draft {
    /// Changed from the library.
    pub fn changed(&self) -> bool {
        self.volumes != self.from
    }
}

/// A preset to start from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    /// One suspension string: top fitting, insulator, arcing horns, clamp.
    IString,
    /// Each leg of a V string: top fitting, insulator, clamp, grading ring.
    VString,
    /// Two strings side by side across, 0.45 m apart: each its top fitting
    /// and insulator; one yoke, clamp and grading ring for both.
    Double,
}

impl Preset {
    /// Every preset, in the order offered.
    pub const ALL: [Preset; 3] = [Preset::IString, Preset::VString, Preset::Double];

    /// Its button's name.
    pub fn name(self) -> &'static str {
        match self {
            Preset::IString => "I string",
            Preset::VString => "V string",
            Preset::Double => "Double string",
        }
    }

    /// What it gives (hover).
    pub fn help(self) -> &'static str {
        match self {
            Preset::IString => {
                "One string: a grounded top fitting, the insulator (insulated), a grounded \
                 tower-side and an electrified line-side arcing horn, an electrified clamp. \
                 Replaces the shapes; INVENTED sizes to be set."
            }
            Preset::VString => {
                "The shapes of each leg of a V string (two strings to one clamp: the line places \
                 both legs, each carries the assembly's shapes): a grounded top fitting, the \
                 insulator, an electrified clamp and grading ring at the wire point. Replaces \
                 the shapes; INVENTED sizes to be set."
            }
            Preset::Double => {
                "Two strings across, 0.45 m apart: each its grounded top fitting and insulator; \
                 one electrified yoke, clamp and grading ring shared by both. Replaces the \
                 shapes and sets two strings; INVENTED sizes to be set."
            }
        }
    }
}

/// A common fitting added in one click.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fitting {
    /// A grounded box at the string's top.
    Top,
    /// The insulator itself: an insulated cylinder over most of the length.
    Insulator,
    /// Arcing horns: a grounded cone at the top, an electrified one at the
    /// wire end, beside the string.
    Horns,
    /// An electrified box at the wire point.
    Clamp,
    /// An electrified torus around the string near the wire point.
    GradingRing,
    /// An electrified plate at the wire end, shared by several strings.
    Yoke,
}

impl Fitting {
    /// Every fitting, in the order offered.
    pub const ALL: [Fitting; 6] = [
        Fitting::Top,
        Fitting::Insulator,
        Fitting::Horns,
        Fitting::Clamp,
        Fitting::GradingRing,
        Fitting::Yoke,
    ];

    /// Its button's name.
    pub fn name(self) -> &'static str {
        match self {
            Fitting::Top => "+ Top fitting",
            Fitting::Insulator => "+ Insulator",
            Fitting::Horns => "+ Arcing horns",
            Fitting::Clamp => "+ Clamp",
            Fitting::GradingRing => "+ Grading ring",
            Fitting::Yoke => "+ Yoke",
        }
    }

    /// What it adds (hover).
    pub fn help(self) -> &'static str {
        match self {
            Fitting::Top => {
                "A grounded box at the string's top (shackle, socket): at the structure's \
                 potential, a target of the electrified parts"
            }
            Fitting::Insulator => {
                "The insulator: an insulated cylinder over the string's length less 0.3 m — \
                 drawn, not a clearance target (its length is insulation)"
            }
            Fitting::Horns => {
                "Two cones beside the string, 0.25 m across: grounded at the top, electrified at \
                 the wire end, their tips towards each other (the arcing gap between them)"
            }
            Fitting::Clamp => {
                "An electrified box at the wire point (the suspension clamp): measured against \
                 the structure and the grounded shapes"
            }
            Fitting::GradingRing => {
                "An electrified torus around the string 0.15 m above the wire point (a corona or \
                 grading ring): 0.5 m across, a tube of 60 mm"
            }
            Fitting::Yoke => {
                "An electrified plate at the wire end, one for all the strings of the assembly \
                 (shared): as wide as the strings' spacing and 0.15 m"
            }
        }
    }

    /// Its shapes on a string of `length` m in an assembly of `strings`
    /// strings `spacing` m apart (INVENTED starting sizes).
    pub fn shapes(self, length: f64, strings: u32, spacing: f64) -> Vec<StringShape> {
        use ElectricalState::{Electrified, Grounded, Insulated};
        let shape = |name: &str, kind, state, at| StringShape {
            at,
            ..StringShape::new(name, kind, state)
        };
        match self {
            Fitting::Top => {
                let mut s = shape("top fitting", ShapeKind::Box, Grounded, Along::Metres(0.05));
                (s.width, s.depth, s.length) = (0.12, 0.08, 0.1);
                vec![s]
            }
            Fitting::Insulator => {
                let mut s = shape(
                    "insulator",
                    ShapeKind::Cylinder,
                    Insulated,
                    Along::Fraction(0.5),
                );
                s.diameter = 0.2;
                s.length = (length - 0.3).max(length * 0.5);
                vec![s]
            }
            Fitting::Horns => {
                let mut top = shape(
                    "tower-side horn",
                    ShapeKind::Cone,
                    Grounded,
                    Along::Metres(0.2_f64.min(length / 4.0)),
                );
                (top.diameter, top.end_diameter, top.length) =
                    (0.03, 0.0, 0.3_f64.min(length / 3.0));
                top.offset = [0.25, 0.0];
                let mut bottom = top.clone();
                bottom.name = "line-side horn".into();
                bottom.state = Electrified;
                bottom.at = Along::Metres((length - 0.2_f64.min(length / 4.0)).max(0.0));
                (bottom.diameter, bottom.end_diameter) = (0.0, 0.03);
                vec![top, bottom]
            }
            Fitting::Clamp => {
                let mut s = shape("clamp", ShapeKind::Box, Electrified, Along::Fraction(1.0));
                (s.width, s.depth, s.length) = (0.1, 0.3, 0.12);
                s.shared = strings > 1;
                vec![s]
            }
            Fitting::GradingRing => {
                let mut s = shape(
                    "grading ring",
                    ShapeKind::Torus,
                    Electrified,
                    Along::Metres((length - 0.15).max(0.0)),
                );
                s.diameter = if strings > 1 { spacing + 0.3 } else { 0.5 };
                s.tube = 0.06;
                s.shared = strings > 1;
                vec![s]
            }
            Fitting::Yoke => {
                let mut s = shape(
                    "yoke",
                    ShapeKind::Box,
                    Electrified,
                    Along::Metres((length - 0.05).max(0.0)),
                );
                s.width = if strings > 1 { spacing + 0.15 } else { 0.15 };
                (s.depth, s.length) = (0.1, 0.08);
                s.shared = strings > 1;
                vec![s]
            }
        }
    }
}

/// `shape` with a name not yet taken in `shapes` ("clamp", else "clamp 2",
/// "clamp 3", …).
fn free_name(shapes: &[StringShape], mut shape: StringShape) -> StringShape {
    let base = shape.name.clone();
    let mut k = 2;
    while shapes.iter().any(|s| s.name == shape.name) {
        shape.name = format!("{base} {k}");
        k += 1;
    }
    shape
}

/// The volumes of `preset` on a string of `length` m (the freedoms kept
/// from `was`).
pub fn preset(p: Preset, length: f64, was: &StringVolumes) -> StringVolumes {
    let (strings, spacing) = match p {
        Preset::Double => (2, 0.45),
        _ => (1, 0.0),
    };
    let fittings: &[Fitting] = match p {
        Preset::IString => &[
            Fitting::Top,
            Fitting::Insulator,
            Fitting::Horns,
            Fitting::Clamp,
        ],
        Preset::VString => &[
            Fitting::Top,
            Fitting::Insulator,
            Fitting::Clamp,
            Fitting::GradingRing,
        ],
        Preset::Double => &[
            Fitting::Top,
            Fitting::Insulator,
            Fitting::Yoke,
            Fitting::Clamp,
            Fitting::GradingRing,
        ],
    };
    let mut shapes = Vec::new();
    for f in fittings {
        for s in f.shapes(length, strings, spacing) {
            shapes.push(free_name(&shapes, s));
        }
    }
    StringVolumes {
        shapes,
        strings,
        arrangement: Arrangement::Across,
        spacing,
        ..was.clone()
    }
}

/// What the window asks its caller.
#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    /// Nothing.
    None,
    /// Write this assembly's file (path, text) into the string library.
    Save(PathBuf, String),
    /// Open Checks › Internal clearances….
    InternalClearances,
}

/// The String builder window.
pub struct StringBuilder {
    /// Shown.
    pub open: bool,
    /// The assembly to edit (its library label).
    pub assembly: String,
    /// Its volumes as edited.
    pub draft: Option<Draft>,
    /// The shape whose fields are shown.
    pub selected: Option<usize>,
    /// The kind of the next shape of "+ Shape".
    add_kind: ShapeKind,
    /// The last save's outcome, or why not.
    pub message: Option<(String, bool)>,
}

impl Default for StringBuilder {
    fn default() -> Self {
        Self {
            open: false,
            assembly: String::new(),
            draft: None,
            selected: None,
            add_kind: ShapeKind::Sphere,
            message: None,
        }
    }
}

impl StringBuilder {
    /// Opens the window on assembly `id` (empty: the one shown before, else
    /// the library's first).
    pub fn open_on(&mut self, id: &str) {
        self.open = true;
        if !id.is_empty() && id != self.assembly {
            self.assembly = id.to_owned();
            self.draft = None;
            self.selected = None;
            self.message = None;
        }
    }

    /// The draft of the assembly shown, taken from the library when there
    /// is none, it is of another assembly, or it has no changes and the
    /// library's changed (an undo, a table).
    fn follow(&mut self, model: &Model) {
        let Some((_, library)) = model.types.as_ref() else {
            self.draft = None;
            return;
        };
        if library.get(&self.assembly).is_none() {
            self.assembly = library
                .iter()
                .next()
                .map(|a| a.id.clone())
                .unwrap_or_default();
        }
        let Some(a) = library.get(&self.assembly) else {
            self.draft = None;
            return;
        };
        let stale = match &self.draft {
            Some(d) => d.id != a.id || (!d.changed() && d.from != a.volumes),
            None => true,
        };
        if stale {
            self.draft = Some(Draft {
                id: a.id.clone(),
                volumes: a.volumes.clone(),
                from: a.volumes.clone(),
                length: a.length.si(),
                legs: a.kind == AssemblyKind::VString,
            });
            let n = a.volumes.shapes.len();
            self.selected = self.selected.filter(|k| *k < n).or((n > 0).then_some(0));
        }
    }

    /// The assembly's file with the draft's volumes (path, text), or what
    /// is wrong.
    pub fn file(&self, model: &Model) -> Result<(PathBuf, String), String> {
        model.can_edit().map_err(|e| format!("read-only: {e}"))?;
        let draft = self.draft.as_ref().ok_or("no assembly")?;
        let library = model
            .types
            .as_ref()
            .map(|(_, i)| i)
            .ok_or("no string library")?;
        let was = library
            .get(&draft.id)
            .ok_or_else(|| format!("{}: not in the string library", draft.id))?;
        let mut a = was.clone();
        a.volumes = draft.volumes.clone();
        a.validate().map_err(|e| format!("{}: {e}", draft.id))?;
        let path = library.path(&draft.id).map_or_else(
            || {
                crate::library::LibraryKind::Insulators
                    .new_file_dir(&model.sources)
                    .join(format!(
                        "{}.{}",
                        draft.id,
                        ohl_structures::insulators::INSULATOR_FILE_EXTENSION
                    ))
            },
            std::path::Path::to_path_buf,
        );
        Ok((path, a.to_toml()))
    }

    /// After a save asked by [`Out::Save`]: kept (`ok`), the draft is the
    /// library's; the message said.
    pub fn saved(&mut self, ok: bool, message: Option<(String, bool)>) {
        if ok && let Some(d) = self.draft.as_mut() {
            d.from = d.volumes.clone();
        }
        self.message = message.or(Some((if ok { "saved" } else { "not saved" }.into(), !ok)));
    }

    /// Adds `shapes` (names made free), the last one selected.
    pub fn add(&mut self, shapes: Vec<StringShape>) {
        let Some(d) = self.draft.as_mut() else {
            return;
        };
        for s in shapes {
            let s = free_name(&d.volumes.shapes, s);
            d.volumes.shapes.push(s);
        }
        self.selected = d.volumes.shapes.len().checked_sub(1);
    }

    /// Adds a common fitting.
    pub fn add_fitting(&mut self, f: Fitting) {
        let Some(d) = self.draft.as_ref() else {
            return;
        };
        let shapes = f.shapes(d.length, d.volumes.strings, d.volumes.spacing);
        self.add(shapes);
    }

    /// Replaces the shapes by a preset's.
    pub fn apply_preset(&mut self, p: Preset) {
        if let Some(d) = self.draft.as_mut() {
            d.volumes = preset(p, d.length, &d.volumes);
            self.selected = (!d.volumes.shapes.is_empty()).then_some(0);
        }
    }

    /// The window, when open.
    pub fn show(&mut self, ctx: &egui::Context, model: &Model) -> Out {
        if !self.open {
            return Out::None;
        }
        self.follow(model);
        let mut open = true;
        let mut out = Out::None;
        help::window(
            ctx,
            crate::dockable::Panel::StringBuilder,
            crate::dockable::Panel::StringBuilder.name(),
            [1000.0, 780.0],
            &mut open,
            |ui| {
                help::body(ui, WINDOW, |ui| self.body(ui, model));
                help::footer(ui, WINDOW, |ui| self.footer(ui, model, &mut out));
            },
        );
        self.open = open;
        out
    }

    /// Save, Revert, the message.
    fn footer(&mut self, ui: &mut egui::Ui, model: &Model, out: &mut Out) {
        ui.horizontal(|ui| {
            let changed = self.draft.as_ref().is_some_and(Draft::changed);
            let file = self.file(model);
            let why = match (&file, changed) {
                (Err(e), _) => Some(e.clone()),
                (Ok(_), false) => Some("no changes".to_owned()),
                _ => None,
            };
            let save = ui
                .add_enabled(why.is_none(), egui::Button::new("Save to the library"))
                .on_hover_text(
                    "Writes the assembly's file in the string library (as the String shapes \
                     table does): one change, undone with Undo. Every string with this label \
                     carries the shapes",
                )
                .on_disabled_hover_text(why.clone().unwrap_or_default());
            if save.clicked()
                && let Ok((path, text)) = file
            {
                *out = Out::Save(path, text);
            }
            if ui
                .add_enabled(changed, egui::Button::new("Revert"))
                .on_hover_text("The library's shapes again (the changes here dropped)")
                .clicked()
                && let Some(d) = self.draft.as_mut()
            {
                d.volumes = d.from.clone();
                self.message = None;
            }
            if ui
                .button("Internal clearances…")
                .on_hover_text(
                    "Checks › Internal clearances…: the electrified shapes and \
                     the conductors against the structures and the grounded shapes (save first)",
                )
                .clicked()
            {
                *out = Out::InternalClearances;
            }
            if changed {
                ui.label(
                    egui::RichText::new("changes not saved")
                        .small()
                        .color(ui.visuals().warn_fg_color),
                );
            }
        });
        if let Some((m, error)) = &self.message {
            let colour = if *error {
                ui.visuals().error_fg_color
            } else {
                ui.visuals().weak_text_color()
            };
            // A long message scrolls in a box of its own.
            egui::ScrollArea::vertical()
                .id_salt("string-builder-message")
                .max_height(48.0)
                .show(ui, |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(m).small().color(colour)).wrap());
                });
        }
    }

    /// The setup: the assembly, presets and fittings, the shapes, the
    /// selected shape's fields; the preview beside.
    fn body(&mut self, ui: &mut egui::Ui, model: &Model) {
        help::explain(
            ui,
            "An insulator assembly's volumes built from basic shapes, each grounded, electrified \
             or insulated, for the internal clearance check.",
            "The shapes ride on every string with this library label, placed on the string as \
             solved in each weather case (its swing; the freedoms; several strings in one \
             assembly). The internal clearance check measures the electrified shapes and the \
             conductors against the structure's members and the grounded shapes; insulated \
             shapes (the insulator itself) are drawn, not measured. A torus (a grading ring) is \
             measured exactly against points, else by the chords of its circle: never farther \
             than it is, at most 0.5 mm nearer (conservative). A line's string may override a \
             shape by name (String shapes of the line). Built, in active development, not \
             approved yet.",
        );
        let Some((_, library)) = model.types.as_ref() else {
            ui.label("No string library: open a project with structure types and assemblies.");
            return;
        };
        ui.horizontal(|ui| {
            ui.label("Assembly");
            let shown = self.draft.as_ref().map_or(String::new(), |d| {
                library
                    .get(&d.id)
                    .map_or(d.id.clone(), |a| format!("{} — {}", a.id, a.name))
            });
            egui::ComboBox::from_id_salt("string-builder-assembly")
                .selected_text(shown)
                .width(320.0)
                .show_ui(ui, |ui| {
                    for a in library.iter() {
                        let label = format!("{} — {}", a.id, a.name);
                        if ui.selectable_label(a.id == self.assembly, label).clicked() {
                            self.open_on(&a.id.clone());
                        }
                    }
                });
            if self.draft.as_ref().is_some_and(Draft::changed) {
                ui.label(
                    egui::RichText::new("(changed)")
                        .small()
                        .color(ui.visuals().warn_fg_color),
                );
            }
        });
        // An assembly picked from the list: its draft this frame.
        self.follow(model);
        let Some(length) = self.draft.as_ref().map(|d| d.length) else {
            ui.label("The string library has no assembly: add one in the String library table.");
            return;
        };
        let legs = self.draft.as_ref().is_some_and(|d| d.legs);
        help::explain(
            ui,
            &format!(
                "String length {length:.3} m{}: positions from its top (0) to the wire point.",
                if legs { ", each leg of a V" } else { "" }
            ),
            "The length of the assembly's string (String library). A shape's position is its \
             centre along the string from the top: a fraction of the length (0 the top, 1 the \
             wire point) or metres. Two strings to one clamp: each leg carries the shapes.",
        );
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width((ui.available_width() - 360.0).max(380.0));
                self.presets_ui(ui);
                ui.separator();
                self.assembly_ui(ui);
                ui.separator();
                self.shapes_ui(ui);
                ui.separator();
                self.fields_ui(ui, length);
            });
            ui.separator();
            ui.vertical(|ui| {
                if let Some(d) = &self.draft {
                    let clicked = preview(ui, d, self.selected);
                    if clicked.is_some() {
                        self.selected = clicked;
                    }
                }
            });
        });
    }

    /// The presets and the common fittings.
    fn presets_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label("Start from");
            for p in Preset::ALL {
                if ui.button(p.name()).on_hover_text(p.help()).clicked() {
                    self.apply_preset(p);
                }
            }
        });
        help::explain(
            ui,
            "A preset replaces the shapes (Revert brings them back until saved).",
            "Presets give a usual assembly's fittings with INVENTED starting sizes (no \
             manufacturer's): set them to the real ones. The freedoms are kept.",
        );
        ui.horizontal_wrapped(|ui| {
            ui.label("Add");
            for f in Fitting::ALL {
                if ui.button(f.name()).on_hover_text(f.help()).clicked() {
                    self.add_fitting(f);
                }
            }
        });
    }

    /// Several strings and the freedoms.
    fn assembly_ui(&mut self, ui: &mut egui::Ui) {
        let Some(d) = self.draft.as_mut() else {
            return;
        };
        let v = &mut d.volumes;
        egui::Grid::new("string-builder-assembly-grid")
            .num_columns(3)
            .spacing(help::GRID_SPACING)
            .show(ui, |ui| {
                ui.label("Strings");
                let mut n = v.strings;
                crate::style::field(ui, egui::DragValue::new(&mut n).range(1..=4));
                if n != v.strings {
                    v.strings = n;
                    if n == 1 {
                        v.spacing = 0.0;
                        if v.arrangement == Arrangement::Triangle {
                            v.arrangement = Arrangement::Across;
                        }
                    } else if v.spacing <= 0.0 {
                        v.spacing = 0.45;
                    }
                }
                help::explain(
                    ui,
                    "Several strings in one assembly (double, triple).",
                    "The line models them as one string, on their middle; each string's copy of \
                     a shape is moved by its offset, parallel to the modelled string; a shared \
                     shape (a yoke) is placed once, on the axis.",
                );
                ui.end_row();
                if v.strings > 1 {
                    ui.label("Arrangement");
                    egui::ComboBox::from_id_salt("string-builder-arrangement")
                        .selected_text(v.arrangement.as_str())
                        .show_ui(ui, |ui| {
                            for a in Arrangement::ALL {
                                ui.selectable_value(&mut v.arrangement, a, a.as_str());
                            }
                        });
                    help::explain(
                        ui,
                        "Side by side across, one behind the other along, or a triangle (three).",
                        "Across: side by side across the line; along: one behind the other; \
                         triangle: three strings at the corners of an equilateral triangle of \
                         side the spacing, one corner ahead.",
                    );
                    ui.end_row();
                    ui.label("Spacing");
                    metres(ui, &mut v.spacing, 0.01, 50.0);
                    help::explain(
                        ui,
                        "Between the strings' axes.",
                        "The distance between the strings' axes, m.",
                    );
                    ui.end_row();
                }
                for (label, f, short) in [
                    (
                        "Swing across",
                        &mut v.swing_across,
                        "Free: the shapes turn with the string's swing across.",
                    ),
                    (
                        "Swing along",
                        &mut v.swing_along,
                        "Free: they turn with the swing along.",
                    ),
                    (
                        "Twist",
                        &mut v.twist,
                        "Free: across the conductor; blocked: the structure's axes.",
                    ),
                ] {
                    ui.label(label);
                    egui::ComboBox::from_id_salt(("string-builder-freedom", label))
                        .selected_text(f.as_str())
                        .show_ui(ui, |ui| {
                            for x in [Freedom::Free, Freedom::Blocked] {
                                ui.selectable_value(f, x, x.as_str());
                            }
                        });
                    help::explain(
                        ui,
                        short,
                        "Whether the shapes turn with the string: free (the default) with its \
                         solved swing or the conductor's direction; blocked: they keep the \
                         still-air orientation (or the structure's axes for the twist). The \
                         shapes stay on the solved axis either way.",
                    );
                    ui.end_row();
                }
            });
    }

    /// The list of shapes: select, duplicate, remove, add one of a kind.
    fn shapes_ui(&mut self, ui: &mut egui::Ui) {
        let Some(d) = self.draft.as_mut() else {
            return;
        };
        let length = d.length;
        let shapes = &mut d.volumes.shapes;
        ui.strong(format!("Shapes ({})", shapes.len()));
        let mut remove = None;
        let mut duplicate = None;
        egui::Grid::new("string-builder-shapes")
            .num_columns(5)
            .striped(true)
            .spacing(help::GRID_SPACING)
            .show(ui, |ui| {
                for (k, s) in shapes.iter().enumerate() {
                    let swatch = egui::RichText::new("■").color(state_colour(s.state));
                    ui.horizontal(|ui| {
                        ui.label(swatch);
                        if ui
                            .selectable_label(self.selected == Some(k), &s.name)
                            .on_hover_text("Its fields below")
                            .clicked()
                        {
                            self.selected = Some(k);
                        }
                    });
                    ui.label(format!("{} · {}", s.kind.as_str(), s.state.as_str()));
                    ui.label(egui::RichText::new(summary(s, length)).small().weak());
                    if ui
                        .small_button("Copy")
                        .on_hover_text(format!("Duplicate {}", s.name))
                        .clicked()
                    {
                        duplicate = Some(k);
                    }
                    if ui
                        .small_button("🗑")
                        .on_hover_text(format!("Remove {}", s.name))
                        .clicked()
                    {
                        remove = Some(k);
                    }
                    ui.end_row();
                }
            });
        if shapes.is_empty() {
            ui.label(
                egui::RichText::new("No shapes yet: start from a preset, or add some.")
                    .small()
                    .weak(),
            );
        }
        if let Some(k) = duplicate {
            let copy = free_name(shapes, shapes[k].clone());
            shapes.insert(k + 1, copy);
            self.selected = Some(k + 1);
        }
        if let Some(k) = remove {
            shapes.remove(k);
            self.selected = match self.selected {
                Some(s) if s > k => Some(s - 1),
                Some(s) if s == k => (!shapes.is_empty()).then(|| k.min(shapes.len() - 1)),
                s => s,
            };
        }
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("string-builder-add-kind")
                .selected_text(self.add_kind.as_str())
                .width(90.0)
                .show_ui(ui, |ui| {
                    for k in ShapeKind::ALL {
                        ui.selectable_value(&mut self.add_kind, k, k.as_str());
                    }
                });
            if ui
                .button("+ Shape")
                .on_hover_text(
                    "A shape of this kind, electrified, at the string's middle (to be set)",
                )
                .clicked()
            {
                let kind = self.add_kind;
                let s = StringShape::new(kind.as_str(), kind, ElectricalState::Electrified);
                self.add(vec![s]);
            }
        });
    }

    /// The selected shape's fields, each explained.
    fn fields_ui(&mut self, ui: &mut egui::Ui, length: f64) {
        let Some(d) = self.draft.as_mut() else {
            return;
        };
        let several = d.volumes.strings > 1;
        let Some(s) = self.selected.and_then(|k| d.volumes.shapes.get_mut(k)) else {
            return;
        };
        ui.strong(format!("Shape \"{}\"", s.name));
        egui::Grid::new("string-builder-fields")
            .num_columns(3)
            .spacing(help::GRID_SPACING)
            .show(ui, |ui| {
                ui.label("Name");
                ui.add(egui::TextEdit::singleline(&mut s.name).desired_width(150.0));
                help::explain(
                    ui,
                    "Unique on the assembly; a line's string overrides a shape by it.",
                    "A line's string may override a shape of its assembly by name (String shapes \
                     of the line): to leave one out at that string, override it as insulated.",
                );
                ui.end_row();
                ui.label("Kind");
                let mut kind = s.kind;
                egui::ComboBox::from_id_salt("string-builder-kind")
                    .selected_text(kind.as_str())
                    .show_ui(ui, |ui| {
                        for k in ShapeKind::ALL {
                            ui.selectable_value(&mut kind, k, k.as_str());
                        }
                    });
                if kind != s.kind {
                    *s = change_kind(s.clone(), kind);
                }
                help::explain(
                    ui,
                    "Sphere, cylinder, box, cone or torus (a grading ring).",
                    "Its axis (cylinder, cone, box length) lies on the string's axis; a torus \
                     lies square to it, around it. A torus is not convex: it is measured \
                     exactly against points, else by the chords of its circle (never farther \
                     than it is, at most 0.5 mm nearer: conservative).",
                );
                ui.end_row();
                ui.label("State");
                egui::ComboBox::from_id_salt("string-builder-state")
                    .selected_text(s.state.as_str())
                    .show_ui(ui, |ui| {
                        for x in ElectricalState::ALL {
                            ui.selectable_value(&mut s.state, x, x.as_str());
                        }
                    });
                help::explain(
                    ui,
                    "Electrified: line's potential; grounded: structure's; insulated: not measured.",
                    "Electrified (at the line's potential: the clamp, a line-side horn, a grading \
                     ring, the yoke) — measured against the structure and the grounded shapes. \
                     Grounded (at the structure's potential: top fittings, a tower-side horn) — \
                     a target. Insulated (the insulator itself: its length is insulation) — \
                     drawn, not measured.",
                );
                ui.end_row();
                ui.label("Position");
                position_ui(ui, &mut s.at, length);
                help::explain(
                    ui,
                    "Its centre along the string from the top (0) to the wire point.",
                    "A fraction of the string's length (0 the top, 1 the wire point) follows the \
                     length when it changes; metres stay from the top.",
                );
                ui.end_row();
                for (label, value, short, long) in size_fields(s) {
                    ui.label(label);
                    metres(ui, value, 0.001, 49.0);
                    help::explain(ui, short, long);
                    ui.end_row();
                }
                ui.label("Offset across");
                signed_metres(ui, &mut s.offset[0]);
                help::explain(
                    ui,
                    "Its centre off the string's axis, across (a horn beside it).",
                    "In the string's own axes: across is the structure's right (twist blocked) \
                     or across the conductor (free), square to the string.",
                );
                ui.end_row();
                ui.label("Offset along");
                signed_metres(ui, &mut s.offset[1]);
                help::explain(
                    ui,
                    "Its centre off the axis, along (ahead for a hanging string).",
                    "Along = across × the string's axis: ahead for a string hanging straight \
                     down; upwards for a strain string.",
                );
                ui.end_row();
                if several {
                    ui.label("Shared");
                    ui.checkbox(&mut s.shared, "one for the assembly");
                    help::explain(
                        ui,
                        "Shared: placed once on the assembly's axis (a yoke), else on each string.",
                        "Several strings in one assembly: a shared shape is placed once, on the \
                         modelled string's axis; the others are copied onto each string.",
                    );
                    ui.end_row();
                }
            });
        if let Err(e) = s.check() {
            ui.colored_label(ui.visuals().error_fg_color, egui::RichText::new(e).small());
        }
        if let Err(e) = d.volumes.check(length) {
            ui.colored_label(
                ui.visuals().error_fg_color,
                egui::RichText::new(format!("The assembly: {e}")).small(),
            );
        }
    }
}

/// `s` turned into a shape of `kind`, its sizes kept where the kind takes
/// them, else usual ones (to be set).
pub fn change_kind(mut s: StringShape, kind: ShapeKind) -> StringShape {
    let was = s.kind;
    s.kind = kind;
    if kind.takes_diameter() && s.diameter <= 0.0 {
        s.diameter = if was == ShapeKind::Box {
            s.width.max(0.05)
        } else {
            0.2
        };
    }
    if kind.takes_length() && s.length <= 0.0 {
        s.length = 0.2;
    }
    if kind == ShapeKind::Box {
        if s.width <= 0.0 {
            s.width = s.diameter.max(0.05);
        }
        if s.depth <= 0.0 {
            s.depth = s.diameter.max(0.05);
        }
    }
    if kind == ShapeKind::Torus && (s.tube <= 0.0 || 2.0 * s.tube >= s.diameter) {
        s.tube = (s.diameter / 8.0).max(0.005);
    }
    s.sizes_of_its_kind()
}

/// The size fields of `s`'s kind: (label, value, short, long).
fn size_fields(s: &mut StringShape) -> Vec<(&'static str, &mut f64, &'static str, &'static str)> {
    let kind = s.kind;
    let StringShape {
        diameter,
        end_diameter,
        length,
        width,
        depth,
        tube,
        ..
    } = s;
    match kind {
        ShapeKind::Sphere => vec![("Diameter", diameter, "The sphere's.", "Its diameter, m.")],
        ShapeKind::Cylinder => vec![
            ("Diameter", diameter, "The cylinder's.", "Its diameter, m."),
            (
                "Length",
                length,
                "On the string's axis.",
                "Its length along the string's axis, centred on its position, m.",
            ),
        ],
        ShapeKind::Box => vec![
            ("Width", width, "Across.", "Its size across, m."),
            ("Depth", depth, "Along.", "Its size along, m."),
            (
                "Length",
                length,
                "On the string's axis.",
                "Its size along the string's axis, centred on its position, m.",
            ),
        ],
        ShapeKind::Cone => vec![
            (
                "Diameter",
                diameter,
                "At its end nearer the top.",
                "The cone's diameter at its end nearer the string's top, m (0: a point there).",
            ),
            (
                "End diameter",
                end_diameter,
                "At its end nearer the wire point (0: a point).",
                "The diameter at its other end, m; one of the two above 0.",
            ),
            (
                "Length",
                length,
                "On the string's axis.",
                "Its length along the string's axis, centred on its position, m.",
            ),
        ],
        ShapeKind::Torus => vec![
            (
                "Diameter",
                diameter,
                "The ring's outer diameter (across the whole ring).",
                "The torus's outer diameter, as measured with a rule over the ring, m; its \
                 tube's centre line is of diameter (diameter − tube).",
            ),
            (
                "Tube",
                tube,
                "The tube's diameter (less than half the diameter).",
                "The diameter of the ring's tube, m: less than half the outer diameter (a ring \
                 with a hole).",
            ),
        ],
    }
}

/// A field in metres from 0 to `max`.
fn metres(ui: &mut egui::Ui, v: &mut f64, min: f64, max: f64) -> egui::Response {
    let mut shown = *v;
    let r = crate::style::field(
        ui,
        egui::DragValue::new(&mut shown)
            .speed(0.005)
            .range(0.0..=max)
            .max_decimals(4)
            .suffix(" m"),
    );
    if shown != *v {
        *v = if shown > 0.0 { shown.max(min) } else { 0.0 };
    }
    r
}

/// A signed field in metres.
fn signed_metres(ui: &mut egui::Ui, v: &mut f64) -> egui::Response {
    crate::style::field(
        ui,
        egui::DragValue::new(v)
            .speed(0.005)
            .range(-49.0..=49.0)
            .max_decimals(4)
            .suffix(" m"),
    )
}

/// A position: a fraction or metres from the top (switching converts on a
/// string of `length` m).
fn position_ui(ui: &mut egui::Ui, at: &mut Along, length: f64) {
    ui.horizontal(|ui| {
        let metric = matches!(at, Along::Metres(_));
        egui::ComboBox::from_id_salt("string-builder-position")
            .selected_text(if metric { "metres" } else { "fraction" })
            .width(80.0)
            .show_ui(ui, |ui| {
                if ui.selectable_label(!metric, "fraction").clicked() && metric {
                    let m = at.metres(length);
                    *at = Along::Fraction(if length > 0.0 {
                        (m / length).clamp(0.0, 1.0)
                    } else {
                        0.0
                    });
                }
                if ui.selectable_label(metric, "metres").clicked() && !metric {
                    *at = Along::Metres(at.metres(length));
                }
            });
        match at {
            Along::Fraction(x) => {
                crate::style::field(
                    ui,
                    egui::DragValue::new(x)
                        .speed(0.005)
                        .range(0.0..=1.0)
                        .max_decimals(4),
                );
            }
            Along::Metres(m) => {
                crate::style::field(
                    ui,
                    egui::DragValue::new(m)
                        .speed(0.005)
                        .range(0.0..=length.max(0.0))
                        .max_decimals(4)
                        .suffix(" m"),
                );
            }
        }
    });
}

/// A shape's sizes and position in words, for the list.
fn summary(s: &StringShape, length: f64) -> String {
    let at = match s.at {
        Along::Fraction(x) => format!("at {x:.3} ({:.2} m)", x * length),
        Along::Metres(m) => format!("at {m:.3} m"),
    };
    let size = match s.kind {
        ShapeKind::Sphere => format!("Ø {:.3}", s.diameter),
        ShapeKind::Cylinder => format!("Ø {:.3} × {:.3}", s.diameter, s.length),
        ShapeKind::Box => format!("{:.3} × {:.3} × {:.3}", s.width, s.depth, s.length),
        ShapeKind::Cone => format!(
            "Ø {:.3} to {:.3} × {:.3}",
            s.diameter, s.end_diameter, s.length
        ),
        ShapeKind::Torus => format!("Ø {:.3}, tube {:.3}", s.diameter, s.tube),
    };
    let off = if s.offset != [0.0; 2] {
        format!(", off {:.2}/{:.2}", s.offset[0], s.offset[1])
    } else {
        String::new()
    };
    format!(
        "{size} m {at}{off}{}",
        if s.shared { ", shared" } else { "" }
    )
}

/// One shape's copy as drawn in a view: its centre across the view (m,
/// from the assembly's axis), along the string (m, from its top), and the
/// shape.
struct Drawn<'a> {
    x: f64,
    z: f64,
    index: usize,
    shape: &'a StringShape,
}

/// The copies of `d`'s shapes in view `v` (0: front, across; 1: side,
/// along): one per string, a shared one once.
fn drawn(d: &Draft, v: usize) -> Vec<Drawn<'_>> {
    let offsets = d.volumes.string_offsets();
    let mut out = Vec::new();
    for (index, s) in d.volumes.shapes.iter().enumerate() {
        let copies: Vec<[f64; 2]> = if offsets.len() > 1 && !s.shared {
            offsets.clone()
        } else {
            vec![[0.0; 2]]
        };
        for o in copies {
            out.push(Drawn {
                x: o[v] + s.offset[v],
                z: s.at.metres(d.length),
                index,
                shape: s,
            });
        }
    }
    // Insulated first (under the others), then grounded, electrified.
    let order = |s: &StringShape| match s.state {
        ElectricalState::Insulated => 0,
        ElectricalState::Grounded => 1,
        ElectricalState::Electrified => 2,
    };
    out.sort_by_key(|c| order(c.shape));
    out
}

/// Half a shape's extent across view `v` and along the string, m.
fn half_extent(s: &StringShape, v: usize) -> (f64, f64) {
    match s.kind {
        ShapeKind::Sphere => (s.diameter / 2.0, s.diameter / 2.0),
        ShapeKind::Cylinder => (s.diameter / 2.0, s.length / 2.0),
        ShapeKind::Cone => (s.diameter.max(s.end_diameter) / 2.0, s.length / 2.0),
        ShapeKind::Box => (if v == 0 { s.width } else { s.depth } / 2.0, s.length / 2.0),
        ShapeKind::Torus => (s.diameter / 2.0, s.tube / 2.0),
    }
}

/// The preview: the front view (across) and the side view (along) of the
/// assembly in its own axes, the string vertical with its top at the top;
/// the selected shape outlined. A shape clicked: its index.
pub fn preview(ui: &mut egui::Ui, d: &Draft, selected: Option<usize>) -> Option<usize> {
    let mut clicked = None;
    ui.label(
        egui::RichText::new("Preview (front: across · side: along)")
            .small()
            .weak(),
    );
    ui.horizontal_top(|ui| {
        for (v, name) in [(0, "front"), (1, "side")] {
            if let Some(k) = view(ui, d, v, name, selected) {
                clicked = Some(k);
            }
        }
    });
    // The key of the colours.
    ui.horizontal_wrapped(|ui| {
        for s in ElectricalState::ALL {
            ui.label(egui::RichText::new("■").color(state_colour(s)));
            ui.label(egui::RichText::new(s.as_str()).small());
        }
    });
    clicked
}

/// One view of the preview ([`preview`]).
fn view(
    ui: &mut egui::Ui,
    d: &Draft,
    v: usize,
    name: &str,
    selected: Option<usize>,
) -> Option<usize> {
    let size = egui::vec2(150.0, 420.0);
    let (response, painter) = ui.allocate_painter(size, egui::Sense::click());
    let label = format!("String builder preview, {name} view");
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, &label));
    let visuals = ui.visuals();
    let rect = response.rect;
    painter.rect_filled(rect, 3.0, visuals.extreme_bg_color);
    let line = visuals.text_color();
    let faint = visuals.weak_text_color();
    let copies = drawn(d, v);
    // The extent to fit: the string and every shape, a margin round.
    let (mut wide, mut top, mut bottom) = (0.3_f64, -0.1_f64, d.length + 0.1);
    for o in d.volumes.string_offsets() {
        wide = wide.max(o[v].abs() + 0.1);
    }
    for c in &copies {
        let (hx, hz) = half_extent(c.shape, v);
        wide = wide.max(c.x.abs() + hx + 0.05);
        top = top.min(c.z - hz - 0.05);
        bottom = bottom.max(c.z + hz + 0.05);
    }
    let inner = rect.shrink2(egui::vec2(8.0, 18.0));
    let k = (f64::from(inner.height()) / (bottom - top))
        .min(f64::from(inner.width()) / (2.0 * wide))
        .max(1e-6);
    let mid = inner.center().x;
    let at =
        |x: f64, z: f64| egui::pos2(mid + (x * k) as f32, inner.top() + ((z - top) * k) as f32);
    let px = |m: f64| (m * k) as f32;
    painter.text(
        egui::pos2(rect.left() + 4.0, rect.top() + 8.0),
        egui::Align2::LEFT_CENTER,
        name,
        egui::FontId::proportional(10.0),
        faint,
    );
    // The strings' axes, the top and the wire point.
    let offsets = d.volumes.string_offsets();
    for o in &offsets {
        help::dashed(&painter, at(o[v], 0.0), at(o[v], d.length), faint);
    }
    painter.line_segment(
        [at(-wide, 0.0), at(wide, 0.0)],
        egui::Stroke::new(2.0, line),
    );
    painter.text(
        at(wide, 0.0) + egui::vec2(-2.0, -7.0),
        egui::Align2::RIGHT_CENTER,
        "top",
        egui::FontId::proportional(9.0),
        faint,
    );
    painter.circle_stroke(
        at(0.0, d.length),
        3.0,
        egui::Stroke::new(1.5, visuals.hyperlink_color),
    );
    painter.text(
        at(0.0, d.length) + egui::vec2(5.0, 9.0),
        egui::Align2::LEFT_CENTER,
        "wire point",
        egui::FontId::proportional(9.0),
        faint,
    );
    let mut hits: Vec<(egui::Rect, usize)> = Vec::new();
    for c in &copies {
        let s = c.shape;
        let fill = state_colour(s.state).gamma_multiply(0.85);
        let stroke = if selected == Some(c.index) {
            egui::Stroke::new(2.0, visuals.selection.stroke.color)
        } else {
            egui::Stroke::new(1.0, fill.gamma_multiply(0.6))
        };
        let (hx, hz) = half_extent(s, v);
        let bounds = egui::Rect::from_two_pos(at(c.x - hx, c.z - hz), at(c.x + hx, c.z + hz));
        match s.kind {
            ShapeKind::Sphere => {
                painter.circle(at(c.x, c.z), px(hx), fill, stroke);
            }
            ShapeKind::Cylinder | ShapeKind::Box => {
                painter.rect(bounds, 1.0, fill, stroke, egui::StrokeKind::Middle);
            }
            ShapeKind::Cone => {
                let (a, b) = (s.diameter / 2.0, s.end_diameter / 2.0);
                let points = vec![
                    at(c.x - a, c.z - hz),
                    at(c.x + a, c.z - hz),
                    at(c.x + b, c.z + hz),
                    at(c.x - b, c.z + hz),
                ];
                painter.add(egui::Shape::convex_polygon(points, fill, stroke));
            }
            ShapeKind::Torus => {
                // Seen edge-on: its outline, a band with round ends; the
                // tube's two sections drawn.
                let r = px(s.tube / 2.0);
                painter.rect(bounds, r, fill, stroke, egui::StrokeKind::Middle);
                let major = (s.diameter - s.tube) / 2.0;
                for side in [-1.0, 1.0] {
                    painter.circle_stroke(
                        at(c.x + side * major, c.z),
                        r,
                        egui::Stroke::new(1.0, line.gamma_multiply(0.5)),
                    );
                }
            }
        }
        hits.push((bounds.expand(2.0), c.index));
    }
    if response.clicked()
        && let Some(p) = response.interact_pointer_pos()
    {
        // The topmost drawn under the click.
        return hits.iter().rev().find(|(r, _)| r.contains(p)).map(|h| h.1);
    }
    response.on_hover_text(
        "The assembly in its own axes, in still air: the string vertical, its top at the top, the \
         wire point at the bottom. Click a shape to edit it.",
    );
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each preset's shapes are valid on a 3 m string and its names
    /// unique; the double string's shared ones are shared; the fittings'
    /// names made free when added twice; a kind changed keeps a valid
    /// shape (a box into a torus into a cone).
    #[test]
    fn presets_fittings_and_kinds() {
        for p in Preset::ALL {
            let v = preset(p, 3.0, &StringVolumes::default());
            v.check(3.0).unwrap_or_else(|e| panic!("{p:?}: {e}"));
            assert!(!v.shapes.is_empty());
            for s in &v.shapes {
                s.check().unwrap();
            }
        }
        let double = preset(Preset::Double, 3.0, &StringVolumes::default());
        assert_eq!(double.strings, 2);
        let ring = double
            .shapes
            .iter()
            .find(|s| s.name == "grading ring")
            .unwrap();
        assert!(ring.shared && ring.kind == ShapeKind::Torus);
        let mut b = StringBuilder {
            draft: Some(Draft {
                id: "a".into(),
                volumes: StringVolumes::default(),
                from: StringVolumes::default(),
                length: 3.0,
                legs: false,
            }),
            ..Default::default()
        };
        b.add_fitting(Fitting::GradingRing);
        b.add_fitting(Fitting::GradingRing);
        let names: Vec<&str> = b
            .draft
            .as_ref()
            .unwrap()
            .volumes
            .shapes
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(names, ["grading ring", "grading ring 2"]);
        assert_eq!(b.selected, Some(1));
        assert!(b.draft.as_ref().unwrap().changed());
        let mut s = StringShape::new("x", ShapeKind::Box, ElectricalState::Grounded);
        for k in [
            ShapeKind::Torus,
            ShapeKind::Cone,
            ShapeKind::Sphere,
            ShapeKind::Box,
        ] {
            s = change_kind(s, k);
            assert_eq!(s.kind, k);
            s.check().unwrap_or_else(|e| panic!("{k:?}: {e}"));
        }
    }
}
