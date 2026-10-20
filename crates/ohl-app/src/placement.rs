// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Add structure on an alignment point, on the alignment's other points
//! too: the structure window's **Also on other points of this alignment**
//! (it replaces the former Structures › Place on alignment points…
//! command). The structure of the window stands on the point clicked; the
//! points ticked here get one each, in the same command (one undo): the
//! window's type (and its extension and height adjustment) or a row's own
//! type (e.g. a strain type at the angle points), each tied to its point,
//! turned along the line there (the bisector at an angle); then they stay
//! as placed.

use eframe::egui;

use crate::editors::StructureForm;
use crate::model::Model;

/// A row of the table: another point of the alignment.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaceRow {
    /// Its index on the alignment.
    pub index: usize,
    /// Its name, as the views write it.
    pub name: String,
    /// Its position.
    pub position: [f64; 3],
    /// The structure on it already.
    pub under: Option<String>,
    /// A structure is put on it.
    pub on: bool,
    /// The type it takes (none: the window's).
    pub structure_type: Option<String>,
}

/// The other points of the alignment the window's structure stands on.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MorePoints {
    /// The alignment.
    pub alignment: String,
    /// The point the window's structure stands on (not a row).
    pub own: usize,
    /// The other points, in order.
    pub rows: Vec<PlaceRow>,
}

impl MorePoints {
    /// For a structure on point `own` of `alignment`: its other points,
    /// none ticked.
    pub fn new(model: &Model, alignment: &str, own: usize) -> Self {
        let mut m = Self {
            alignment: alignment.to_owned(),
            own,
            rows: Vec::new(),
        };
        m.rows = m.read(model, &[]);
        m
    }

    fn read(&self, model: &Model, keep: &[PlaceRow]) -> Vec<PlaceRow> {
        crate::editors::alignment_points(&model.line)
            .into_iter()
            .filter(|p| p.0 == self.alignment && p.1 != self.own)
            .map(|p| {
                let before = keep.iter().find(|r| r.index == p.1);
                PlaceRow {
                    index: p.1,
                    name: p.4,
                    position: p.2,
                    on: p.3.is_none() && before.is_some_and(|r| r.on),
                    under: p.3,
                    structure_type: before.and_then(|r| r.structure_type.clone()),
                }
            })
            .collect()
    }

    /// Follows the window's point (`on`: alignment, index): the rows read
    /// again when it moved, their ticks and types kept on the same
    /// alignment.
    pub fn follow(&mut self, model: &Model, on: &(String, usize)) {
        if self.alignment == on.0 && self.own == on.1 {
            return;
        }
        let keep = if self.alignment == on.0 {
            std::mem::take(&mut self.rows)
        } else {
            Vec::new()
        };
        self.alignment = on.0.clone();
        self.own = on.1;
        self.rows = self.read(model, &keep);
    }

    /// The rows ticked (free points only).
    pub fn ticked(&self) -> impl Iterator<Item = &PlaceRow> {
        self.rows.iter().filter(|r| r.on && r.under.is_none())
    }

    /// The structures of the rows ticked, after the window's `form` (its
    /// type, extension, height adjustment and base on the ground; ids
    /// after `taken`'s last, each taken in turn): (id, command) each.
    pub fn commands(
        &self,
        model: &Model,
        form: &StructureForm,
        taken: &mut Vec<String>,
    ) -> Result<Vec<(String, ohl_session::Command)>, String> {
        let stored = model
            .stored_line()
            .ok_or("read-only: open a project to edit")?;
        let mut out = Vec::new();
        for r in self.ticked() {
            let own_type = r.structure_type.is_some();
            let ty = r
                .structure_type
                .clone()
                .or_else(|| form.structure_type.clone())
                .ok_or_else(|| {
                    format!(
                        "point {} ({}): choose its type, or a type for the structure above",
                        r.index + 1,
                        r.name
                    )
                })?;
            let mut f = StructureForm::new_at(model, r.position);
            let like = taken.last().map_or("T0", |s| s.as_str()).to_owned();
            f.id = crate::editors::next_id(taken.iter().map(String::as_str), &like);
            f.structure_type = Some(ty);
            // The window's type: its extension and foundations too; a
            // row's own type: its base.
            f.extension = if own_type {
                None
            } else {
                form.extension.clone()
            };
            f.height_adjust = if own_type { 0.0 } else { form.height_adjust };
            f.along_line = true;
            f.place_on(model, (self.alignment.clone(), r.index));
            if !form.on_ground {
                // The base at the point's own height.
                f.position = r.position;
            }
            f.on_ground = form.on_ground;
            let (_, c) = f.command(Some(stored))?;
            taken.push(f.id.clone());
            out.push((f.id.clone(), c));
        }
        Ok(out)
    }

    /// The table: a tick and a type for each other point.
    pub fn ui(&mut self, ui: &mut egui::Ui, model: &Model) {
        let types: Vec<String> = model
            .types
            .as_ref()
            .map(|(l, _)| l.iter().map(|t| t.id.clone()).collect())
            .unwrap_or_default();
        ui.label(
            egui::RichText::new(
                "The points ticked get a structure too, in the same command (one undo): the \
                 type above, or a row's own (a strain type at an angle point). Each stands on \
                 its point, tied to it, turned along the line (the bisector at an angle); then \
                 it stays as placed — Properties › Placement… › Put it on the line again turns \
                 it again.",
            )
            .small()
            .weak(),
        );
        ui.horizontal(|ui| {
            if ui.small_button("All").clicked() {
                for r in &mut self.rows {
                    r.on = r.under.is_none();
                }
            }
            if ui.small_button("None").clicked() {
                for r in &mut self.rows {
                    r.on = false;
                }
            }
        });
        egui::ScrollArea::vertical()
            .id_salt("more-points")
            .max_height(240.0)
            .show(ui, |ui| {
                egui::Grid::new("more-points-rows")
                    .num_columns(4)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("#");
                        ui.strong("point");
                        ui.strong("put");
                        ui.strong("type");
                        ui.end_row();
                        for r in &mut self.rows {
                            ui.label((r.index + 1).to_string());
                            ui.label(format!(
                                "{} — E {:.1}, N {:.1}",
                                r.name, r.position[0], r.position[1]
                            ));
                            match &r.under {
                                Some(s) => {
                                    ui.label(egui::RichText::new(format!("under {s}")).weak());
                                }
                                None => {
                                    ui.checkbox(&mut r.on, "");
                                }
                            }
                            ui.add_enabled_ui(r.on && r.under.is_none(), |ui| {
                                egui::ComboBox::from_id_salt(("more-points-type", r.index))
                                    .selected_text(
                                        r.structure_type.as_deref().unwrap_or("(the type above)"),
                                    )
                                    .height(300.0)
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut r.structure_type,
                                            None,
                                            "(the type above)",
                                        );
                                        for t in &types {
                                            ui.selectable_value(
                                                &mut r.structure_type,
                                                Some(t.clone()),
                                                t,
                                            );
                                        }
                                    });
                            });
                            ui.end_row();
                        }
                    });
            });
    }
}
