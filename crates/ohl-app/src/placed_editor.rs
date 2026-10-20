// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A structure without a type in the pylon editor: its points, wire
//! points, strings and stick drawing written as a structure type in its
//! own axes (a document only this editor holds), edited with the same
//! tools as a type, and put back on the structure on Apply — one command.
//! **Make it a type** saves the document as a structure type of the
//! project and gives the structure that type; **Make it a structure file
//! and edit it** does the same and opens the new file in the structure
//! application (a structure without a file has nothing to open there).

use std::collections::BTreeMap;
use std::path::Path;

use eframe::egui;
use eframe::egui_wgpu;
use ohl_line::{InsulatorString, Line, Structure, StructurePoint, WirePoint};
use ohl_session::{Command, Kind};
use ohl_structures::structures::to_local;
use ohl_structures::{InsulatorLibrary, LibraryItem, StructureType};
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, Value};

/// A structure without a type, being edited.
pub struct PlacedEditor {
    /// The structure.
    pub id: String,
    /// Its points, wire points, strings and sticks as a structure type, in
    /// its axes.
    pub doc: DocumentMut,
    /// The text when opened (or last applied).
    saved: String,
    /// The editor.
    pub workspace: crate::type_workspace::Workspace,
    /// Its strings' axial stiffness (a type has none): kept by label.
    stiffness: BTreeMap<String, f64>,
    /// What the last action did (or why not: `true`).
    pub message: Option<(String, bool)>,
    /// Make it a type: the id being typed.
    pub type_id: Option<String>,
    /// The type made is opened in the structure application.
    pub then_edit: bool,
}

/// The button making a structure without a type a structure file, opened
/// in the structure application.
pub const MAKE_FILE_AND_EDIT: &str = "Make it a structure file and edit it…";

/// What the editor asks of the application.
pub enum PlacedAction {
    /// Put the document on the structure.
    Apply,
    /// Save the document as structure type `id` and give the structure
    /// that type; `edit`: then open it in the structure application.
    MakeType {
        /// The new type's id.
        id: String,
        /// Opened in the structure application.
        edit: bool,
    },
}

/// A position as written in structure types.
fn point(p: [f64; 3]) -> Item {
    let mut a = Array::new();
    for v in p {
        let v = if v.abs() < 5e-4 { 0.0 } else { v };
        a.push(format!("{v:.3} m"));
    }
    Item::Value(Value::Array(a))
}

/// An id a structure type may have (letters, digits, - _ .).
pub fn type_id(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Structure `s` (its own items) as a structure type in its axes.
pub fn as_type(s: &Structure) -> DocumentMut {
    let local = |p: [f64; 3]| to_local(s.position, s.orientation_deg, p);
    let mut doc = DocumentMut::new();
    doc.insert("format", toml_edit::value("ohl-structure"));
    // Version 4 only where a string gives limits (an older program reads
    // the rest).
    let limits = s.strings.iter().any(|x| !x.limits().is_empty());
    doc.insert("version", toml_edit::value(if limits { 4 } else { 2 }));
    doc.insert("id", toml_edit::value(type_id(&s.id)));
    doc.insert("name", toml_edit::value(format!("structure {}", s.id)));
    doc.insert("kind", toml_edit::value("other"));
    let text = |t: &mut Table, k: &str, v: &str| {
        if !v.trim().is_empty() {
            t.insert(k, toml_edit::value(v.trim()));
        }
    };
    let mut points = ArrayOfTables::new();
    for p in &s.points {
        let mut t = Table::new();
        t.insert("label", toml_edit::value(p.label.clone()));
        t.insert("point", point(local(p.position)));
        text(&mut t, "description", &p.description);
        points.push(t);
    }
    let mut wires = ArrayOfTables::new();
    for w in &s.wires {
        let mut t = Table::new();
        t.insert("label", toml_edit::value(w.label.clone()));
        if let Some(set) = w.set {
            t.insert("set", toml_edit::value(i64::from(set)));
        }
        t.insert("point", point(local(w.position)));
        if w.weight > 0.0 {
            t.insert("weight", toml_edit::value(format!("{} N", w.weight)));
        }
        text(&mut t, "description", &w.description);
        wires.push(t);
    }
    let mut strings = ArrayOfTables::new();
    for x in &s.strings {
        let mut t = Table::new();
        t.insert("label", toml_edit::value(x.label.clone()));
        t.insert("top", toml_edit::value(x.top.clone()));
        t.insert("wire", toml_edit::value(x.wire.clone()));
        if let Some(i) = &x.insulator {
            t.insert("insulator", toml_edit::value(i.clone()));
        }
        t.insert("weight", toml_edit::value(format!("{} N", x.weight)));
        if x.wind_area > 0.0 {
            t.insert("wind_area", toml_edit::value(format!("{} m²", x.wind_area)));
        }
        if let Some(c) = x.compression {
            t.insert("compression", toml_edit::value(format!("{c} N")));
        }
        if x.strain {
            t.insert("strain", toml_edit::value(true));
        }
        if let Some(l) = x.max_load {
            t.insert("max_load", toml_edit::value(format!("{l} N")));
        }
        for (k, a) in [
            ("max_swing", x.max_swing_deg),
            ("max_swing_across", x.max_swing_across_deg),
            ("max_swing_along", x.max_swing_along_deg),
        ] {
            if let Some(a) = a {
                t.insert(k, toml_edit::value(format!("{a} deg")));
            }
        }
        text(&mut t, "description", &x.description);
        strings.push(t);
    }
    let mut body = ArrayOfTables::new();
    for m in &s.members {
        let mut a = Array::new();
        for p in m {
            let q = local(*p);
            let mut v = Array::new();
            for c in q {
                v.push(format!("{:.3} m", if c.abs() < 5e-4 { 0.0 } else { c }));
            }
            a.push(Value::Array(v));
        }
        let mut t = Table::new();
        t.insert("points", Item::Value(Value::Array(a)));
        body.push(t);
    }
    for (k, a) in [
        ("point", points),
        ("wire", wires),
        ("string", strings),
        ("body", body),
    ] {
        if !a.is_empty() {
            doc.insert(k, Item::ArrayOfTables(a));
        }
    }
    doc
}

impl PlacedEditor {
    /// Structure `id` of `line` (one without a type).
    pub fn open(line: &Line, id: &str) -> Result<Self, String> {
        let s = line
            .structure(id)
            .ok_or_else(|| format!("no structure {id}"))?;
        if s.structure_type.is_some() {
            return Err(format!(
                "{id} has a type: edit the type (Strings and points…)"
            ));
        }
        let doc = as_type(s);
        let mut workspace = crate::type_workspace::Workspace::default();
        workspace.on = true;
        // Its own button: a file first (Make it a structure file and edit it).
        workspace.app_button = false;
        workspace.ahead = line.display.ahead_deg;
        Ok(Self {
            id: id.to_owned(),
            saved: doc.to_string(),
            doc,
            workspace,
            stiffness: s
                .strings
                .iter()
                .filter_map(|x| Some((x.label.clone(), x.stiffness?)))
                .collect(),
            message: None,
            type_id: None,
            then_edit: false,
        })
    }

    /// Changes not applied.
    pub fn dirty(&self) -> bool {
        self.doc.to_string() != self.saved
    }

    /// The type the document reads as (or why not).
    fn read(&self) -> Result<StructureType, String> {
        StructureType::parse(Path::new("structure.ohls"), &self.doc.to_string())
            .map_err(|e| e.to_string())
    }

    /// The structure as stored, with the document's points, wire points,
    /// strings and sticks.
    pub fn structure(
        &self,
        stored: &Line,
        insulators: &InsulatorLibrary,
    ) -> Result<Structure, String> {
        let t = self.read()?;
        let mut s = stored
            .structure(&self.id)
            .ok_or_else(|| format!("no structure {}", self.id))?
            .clone();
        let placed = t.place_strings(s.position, s.orientation_deg, None, insulators)?;
        s.points = placed
            .points
            .into_iter()
            .map(|p| StructurePoint {
                label: p.label,
                position: p.position,
                description: p.description,
            })
            .collect();
        s.wires = placed
            .wires
            .into_iter()
            .map(|w| WirePoint {
                label: w.label,
                position: w.position,
                set: w.set,
                weight: w.weight,
                description: w.description,
            })
            .collect();
        s.strings = placed
            .strings
            .into_iter()
            .map(|x| InsulatorString {
                stiffness: self.stiffness.get(&x.label).copied(),
                wind_area: x.wind_area,
                label: x.label,
                top: x.top,
                wire: x.wire,
                insulator: x.insulator,
                weight: x.weight,
                compression: x.compression,
                strain: x.strain,
                description: x.description,
                max_load: x.limits.max_load,
                max_swing_deg: x.limits.max_swing_deg,
                max_swing_across_deg: x.limits.max_swing_across_deg,
                max_swing_along_deg: x.limits.max_swing_along_deg,
                // From its wire point as placed here.
                ..Default::default()
            })
            .collect();
        s.members = if t.members.is_empty() && t.body.is_empty() {
            Vec::new()
        } else {
            t.place_stick(s.position, s.orientation_deg, None)?
        };
        Ok(s)
    }

    /// The command putting the document on the structure.
    pub fn command(
        &self,
        stored: &Line,
        insulators: &InsulatorLibrary,
    ) -> Result<(String, Command), String> {
        let s = self.structure(stored, insulators)?;
        let put = Command::Put {
            kind: Kind::Structure,
            id: self.id.clone(),
            value: serde_json::to_value(&s).map_err(|e| e.to_string())?,
            ord: None,
        };
        // Sets renumbered (on the same wire point): the sections and jumpers
        // strung there follow, in the same command (swaps too).
        let before = stored.structure(&self.id);
        let renumbered: std::collections::BTreeMap<u32, u32> = before
            .map(|b| {
                b.wires
                    .iter()
                    .filter_map(|w| {
                        let old = w.set?;
                        let new = s.wire(&w.label)?.set?;
                        (old != new).then_some((old, new))
                    })
                    .collect()
            })
            .unwrap_or_default();
        if renumbered.is_empty() {
            return Ok((
                format!("changed the strings and points of {}", self.id),
                put,
            ));
        }
        let remap = |p: &mut ohl_line::SectionPoint| -> bool {
            match renumbered.get(&p.set) {
                Some(&new) if p.structure == self.id => {
                    p.set = new;
                    true
                }
                _ => false,
            }
        };
        let mut commands = vec![put];
        for sec in &stored.sections {
            let mut sec = sec.clone();
            let mut moved = false;
            for p in &mut sec.points {
                moved |= remap(p);
            }
            if moved {
                commands.push(Command::Put {
                    kind: Kind::Section,
                    id: sec.id.clone(),
                    value: serde_json::to_value(&sec).map_err(|e| e.to_string())?,
                    ord: None,
                });
            }
        }
        for j in &stored.jumpers {
            let mut j = j.clone();
            let a = j.from.point_mut().is_some_and(remap);
            let b = j.to.point_mut().is_some_and(remap);
            if a || b {
                commands.push(Command::Put {
                    kind: Kind::Jumper,
                    id: j.id.clone(),
                    value: serde_json::to_value(&j).map_err(|e| e.to_string())?,
                    ord: None,
                });
            }
        }
        let list: Vec<String> = renumbered.iter().map(|(a, b)| format!("{a}➡{b}")).collect();
        Ok((
            format!(
                "changed the strings and points of {}; sets {} (their wires follow)",
                self.id,
                list.join(", ")
            ),
            Command::Batch { commands },
        ))
    }

    /// The document as structure type `id` (its text).
    pub fn type_text(&self, id: &str) -> String {
        let mut doc = self.doc.clone();
        doc.insert("id", toml_edit::value(id));
        doc.insert("name", toml_edit::value(id));
        doc.insert(
            "source",
            toml_edit::value(format!("made from structure {}", self.id)),
        );
        doc.to_string()
    }

    /// **Make it a structure file and edit it**: the type id asked for
    /// (the structure's own, made fit), then the type saved, used and
    /// opened in the structure application.
    pub fn ask_file_and_edit(&mut self) {
        self.type_id = Some(type_id(&self.id));
        self.then_edit = true;
        self.message = Some((
            format!(
                "structure {} has no structure file: give the new structure type an id, then                  Save, use and edit",
                self.id
            ),
            false,
        ));
    }

    /// Applied: the text kept as the one to compare with.
    pub fn applied(&mut self) {
        self.saved = self.doc.to_string();
    }

    /// The editor: Apply, Revert, Make it a type…; then the pylon
    /// workspace on the document.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        rs: Option<&egui_wgpu::RenderState>,
        insulators: &InsulatorLibrary,
        line: &Line,
    ) -> Option<PlacedAction> {
        let mut out = None;
        let readable = self.read();
        ui.horizontal_wrapped(|ui| {
            ui.strong(format!("Structure {}", self.id));
            ui.label(
                egui::RichText::new("its own points, wire points and strings, in its axes").weak(),
            );
            ui.separator();
            let dirty = self.dirty();
            if ui
                .add_enabled(dirty && readable.is_ok(), egui::Button::new("Apply"))
                .on_hover_text("Put them on the structure (one command, one undo)")
                .clicked()
            {
                out = Some(PlacedAction::Apply);
            }
            if ui
                .add_enabled(dirty, egui::Button::new("Revert"))
                .on_hover_text("Back to the structure as it is")
                .clicked()
                && let Some(s) = line.structure(&self.id)
            {
                self.doc = as_type(s);
                self.workspace.redraw();
            }
            if dirty {
                ui.colored_label(ui.visuals().warn_fg_color, "* not applied");
            }
            ui.separator();
            if ui
                .button("Make it a type…")
                .on_hover_text(
                    "Save it as a structure type of the project, and give the structure that \
                     type (its points and strings then come from the type)",
                )
                .clicked()
            {
                self.type_id = Some(type_id(&self.id));
                self.then_edit = false;
            }
            if ui
                .button(MAKE_FILE_AND_EDIT)
                .on_hover_text(
                    "It has no structure file, so OHL-Structure has nothing to open: save \
                     it as a structure type of the project (the structure takes that type) \
                     and open that file in OHL-Structure",
                )
                .clicked()
            {
                self.ask_file_and_edit();
            }
        });
        if let Some(id) = &mut self.type_id {
            let mut cancel = false;
            ui.horizontal(|ui| {
                ui.label("type id");
                ui.add(egui::TextEdit::singleline(id).desired_width(160.0));
                let label = if self.then_edit {
                    "Save, use and edit"
                } else {
                    "Save and use"
                };
                if ui
                    .add_enabled(
                        readable.is_ok() && !id.trim().is_empty(),
                        egui::Button::new(label),
                    )
                    .clicked()
                {
                    out = Some(PlacedAction::MakeType {
                        id: type_id(id),
                        edit: self.then_edit,
                    });
                }
                cancel = ui.button("Cancel").clicked();
            });
            if cancel {
                self.type_id = None;
            }
        }
        if let Err(e) = &readable {
            crate::menu::error_label(ui, egui::RichText::new(e).small());
        }
        if let Some((m, error)) = &self.message {
            let t = egui::RichText::new(m).small();
            if *error {
                crate::menu::error_label(ui, t);
            } else {
                ui.colored_label(crate::menu::ok_color(ui), t);
            }
        }
        ui.separator();
        self.workspace.ahead = line.display.ahead_deg;
        let note = format!(
            "structure {} of the line (Apply puts the changes on it)",
            self.id
        );
        self.workspace.ui(ui, &mut self.doc, rs, insulators, &note);
        out
    }
}
