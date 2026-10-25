// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The structure window: the form, its command, the window drawn with its
//! preview and compass.

use super::*;

impl StructureForm {
    /// A new structure at `at`: the type and orientation of the nearest
    /// structure, the id after the last one's.
    pub fn new_at(model: &Model, at: [f64; 3]) -> Self {
        let line = &model.line;
        let nearest = line.structures.iter().min_by(|a, b| {
            let d = |s: &&Structure| (s.position[0] - at[0]).hypot(s.position[1] - at[1]);
            d(a).total_cmp(&d(b))
        });
        let like = line.structures.last().map_or("T0", |s| s.id.as_str());
        let id = next_id(line.structures.iter().map(|s| s.id.as_str()), like);
        let structure_type = nearest
            .and_then(|s| s.structure_type.clone())
            .or_else(|| model.types.as_ref()?.0.iter().next().map(|t| t.id.clone()));
        // On an alignment point near where it was asked for (5 m), not
        // taken yet: there, tied to it.
        let on_alignment = alignment_points(line)
            .into_iter()
            .filter(|p| p.3.is_none())
            .map(|p| ((p.2[0] - at[0]).hypot(p.2[1] - at[1]), p))
            .filter(|(d, _)| *d < 5.0)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, p)| (p.0, p.1));
        let along = line_orientation(line, at, on_alignment.as_ref());
        let mut f = Self {
            new: true,
            id,
            structure_type,
            extension: None,
            height_adjust: 0.0,
            orientation_deg: along
                .or_else(|| nearest.map(|s| s.orientation_deg))
                .unwrap_or(0.0),
            position: at,
            on_ground: model.ground_at(at[0], at[1]).is_some(),
            description: String::new(),
            points: Vec::new(),
            on_alignment: None,
            along_line: along.is_some(),
            routes: Vec::new(),
            weather_groups: Vec::new(),
            new_route: String::new(),
            make_route: None,
            message: None,
            more: None,
        };
        if let Some(on) = on_alignment {
            f.place_on(model, on);
        }
        f
    }

    /// Puts it on alignment point `on` (route, index): its position (the
    /// base on the ground there when there is ground), and the line's
    /// orientation there when it follows the line.
    pub fn place_on(&mut self, model: &Model, on: (String, usize)) {
        if let Some(p) = alignment_points(&model.line)
            .into_iter()
            .find(|p| p.0 == on.0 && p.1 == on.1)
        {
            self.position = p.2;
            if let Some(z) = model.ground_at(p.2[0], p.2[1]) {
                self.position[2] = z;
                self.on_ground = true;
            }
        }
        self.on_alignment = Some(on);
        self.follow_line(model);
    }

    /// Put it on the line again: on its alignment point (the one it is
    /// tied to, else the nearest free one within 5 m), turned along the
    /// line there. Whether there was one.
    pub fn put_on_line(&mut self, model: &Model) -> bool {
        let on = self.on_alignment.clone().or_else(|| {
            alignment_points(&model.line)
                .into_iter()
                .filter(|p| p.3.is_none() || p.3.as_deref() == Some(self.id.as_str()))
                .map(|p| {
                    (
                        (p.2[0] - self.position[0]).hypot(p.2[1] - self.position[1]),
                        p,
                    )
                })
                .filter(|(d, _)| *d < 5.0)
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, p)| (p.0, p.1))
        });
        let Some(on) = on else {
            return false;
        };
        self.along_line = true;
        self.on_ground = model
            .ground_at(self.position[0], self.position[1])
            .is_some();
        self.place_on(model, on);
        true
    }

    /// Its orientation from the line, when it follows it.
    pub fn follow_line(&mut self, model: &Model) {
        if self.along_line
            && let Some(o) =
                line_orientation(&model.line, self.position, self.on_alignment.as_ref())
        {
            self.orientation_deg = o;
        }
    }

    /// Structure `id` as stored.
    pub fn of(model: &Model, id: &str) -> Option<Self> {
        let s = model.line.structure(id)?;
        let local = |p| to_local(s.position, s.orientation_deg, p);
        Some(Self {
            new: false,
            id: id.to_owned(),
            structure_type: s.structure_type.clone(),
            extension: s.extension.clone(),
            height_adjust: s.height_adjust,
            orientation_deg: s.orientation_deg,
            position: s.position,
            on_ground: false,
            description: s.description.clone(),
            points: model
                .stored_line()
                .and_then(|l| l.structure(id))
                .map(|st| {
                    st.points
                        .iter()
                        .map(|p| PointRow {
                            description: p.description.clone(),
                            label: p.label.clone(),
                            at: local(p.position),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            // The alignment point it stands on (tied), if any; it stays as
            // placed until put on the line again.
            on_alignment: alignment_points(&model.line)
                .into_iter()
                .find(|p| p.3.as_deref() == Some(id))
                .map(|p| (p.0, p.1)),
            along_line: false,
            routes: s.routes.clone(),
            weather_groups: s.weather_groups.clone(),
            new_route: String::new(),
            make_route: None,
            message: None,
            more: None,
        })
    }

    /// The command of the window: the structure, and (new on an alignment
    /// point) one on each other point of the alignment ticked, in one
    /// command ([`crate::placement`]).
    pub fn command_with_more(&self, model: &Model) -> Result<(String, Command), String> {
        let stored = model.stored_line();
        let (what, command) = self.command(stored)?;
        let more = match (&self.more, &self.on_alignment) {
            (Some(m), Some(on)) if self.new && m.alignment == on.0 && m.own == on.1 => m,
            _ => return Ok((what, command)),
        };
        if more.ticked().next().is_none() {
            return Ok((what, command));
        }
        let mut taken: Vec<String> = stored
            .map(|l| l.structures.iter().map(|s| s.id.clone()).collect())
            .unwrap_or_default();
        taken.push(self.id.trim().to_owned());
        let others = more.commands(model, self, &mut taken)?;
        let ids: Vec<&str> = others.iter().map(|(id, _)| id.as_str()).collect();
        let what = format!(
            "{what}, and {} more on alignment {}: {}",
            others.len(),
            more.alignment,
            ids.join(", ")
        );
        let mut commands = vec![command];
        commands.extend(others.into_iter().map(|(_, c)| c));
        Ok((what, Command::Batch { commands }))
    }

    pub(crate) fn command(&self, stored: Option<&Line>) -> Result<(String, Command), String> {
        let (what, command) = self.structure_command(stored)?;
        // A route made with it: first, in the same command.
        let Some(name) = &self.make_route else {
            return Ok((what, command));
        };
        let stored = stored.ok_or("read-only: open a project to edit")?;
        let (_, route) = new_route(stored, name, name)?;
        Ok((
            format!("{what} on new route {name}"),
            Command::Batch {
                commands: vec![route, command],
            },
        ))
    }

    pub(super) fn structure_command(
        &self,
        stored: Option<&Line>,
    ) -> Result<(String, Command), String> {
        let id = self.id.trim().to_owned();
        if id.is_empty() {
            return Err("the structure needs an id".into());
        }
        let s = if self.new {
            if stored.is_some_and(|l| l.structure(&id).is_some()) {
                return Err(format!("structure {id} exists already"));
            }
            Structure {
                orientation_deg: self.orientation_deg,
                structure_type: self.structure_type.clone(),
                extension: self.extension.clone(),
                height_adjust: self.height_adjust,
                description: self.description.trim().to_owned(),
                points: self.global_points(),
                routes: self.routes.clone(),
                weather_groups: self.weather_groups.clone(),
                ..Structure::new(id.clone(), self.position)
            }
        } else {
            let old = stored
                .and_then(|l| l.structure(&id))
                .ok_or_else(|| format!("no structure {id}"))?;
            let mut s = old.moved_to(self.position);
            if s.orientation_deg != self.orientation_deg {
                // Its own points and wire points turn with it about its base.
                let (position, from) = (s.position, s.orientation_deg);
                let rot = |p: [f64; 3]| {
                    to_global(position, self.orientation_deg, to_local(position, from, p))
                };
                for p in s
                    .points
                    .iter_mut()
                    .map(|p| &mut p.position)
                    .chain(s.wires.iter_mut().map(|w| &mut w.position))
                {
                    *p = rot(*p);
                }
                s.orientation_deg = self.orientation_deg;
            }
            if s.structure_type != self.structure_type {
                // The type's stick drawing comes with the new type.
                s.members.clear();
                s.omit.clear();
                s.omit_sets.clear();
            }
            s.structure_type = self.structure_type.clone();
            s.extension = self.extension.clone();
            s.height_adjust = self.height_adjust;
            s.description = self.description.trim().to_owned();
            s.routes = self.routes.clone();
            s.weather_groups.clone_from(&self.weather_groups);
            // Its own points as in the window (in its axes, turned and
            // moved with it); a linked one keeps its joint, its offset
            // following a move (owner decision 2026-10-21).
            let mut points = self.global_points();
            for p in &mut points {
                if let Some(q) = s.points.iter().find(|q| q.label == p.label) {
                    p.joint.clone_from(&q.joint);
                    p.offset = q.offset;
                    p.unlinked = q.unlinked;
                }
            }
            for p in &s.points {
                if !points.iter().any(|q| q.label == p.label)
                    && s.strings.iter().any(|x| x.top == p.label)
                {
                    points.push(p.clone());
                }
            }
            s.points = points;
            ohl_line::links::follow_moves(old, &mut s);
            s
        };
        let labels: std::collections::BTreeSet<&str> =
            self.points.iter().map(|p| p.label.trim()).collect();
        if labels.len() != self.points.len() || labels.contains("") {
            return Err("every point needs a label of its own".into());
        }
        let what = if self.new {
            format!("added structure {id}")
        } else {
            format!("changed structure {id}")
        };
        let put = Command::Put {
            kind: Kind::Structure,
            id: id.clone(),
            value: serde_json::to_value(&s).map_err(|e| e.to_string())?,
            ord: None,
        };
        // On an alignment point: tied to it (one command).
        let Some((route, index)) = &self.on_alignment else {
            return Ok((what, put));
        };
        let stored = stored.ok_or("read-only: open a project to edit")?;
        let mut p = stored_alignment(stored, route)
            .and_then(|a| a.get(*index).cloned())
            .ok_or_else(|| format!("alignment {route} has no point {}", index + 1))?;
        if let Some(other) = p.structure.as_ref().filter(|o| **o != id) {
            return Err(format!("alignment point {} is under {other} already", p.id));
        }
        p.structure = Some(id.clone());
        p.position = [s.position[0], s.position[1], p.position[2]];
        Ok((
            format!("{what} on alignment point {}", p.id),
            Command::Batch {
                commands: vec![
                    put,
                    Command::Put {
                        kind: Kind::AlignmentPoint,
                        id: p.id.clone(),
                        value: serde_json::to_value(&p).map_err(|e| e.to_string())?,
                        ord: None,
                    },
                ],
            },
        ))
    }

    /// Its own points in project coordinates.
    pub(super) fn global_points(&self) -> Vec<StructurePoint> {
        self.points
            .iter()
            .map(|p| StructurePoint {
                description: p.description.clone(),
                label: p.label.trim().to_owned(),
                position: to_global(self.position, self.orientation_deg, p.at),
                ..Default::default()
            })
            .collect()
    }
}

/// The structure window; whether New pylon from a template was asked for.
pub(super) fn structure_ui(
    ui: &mut egui::Ui,
    f: &mut StructureForm,
    model: &Model,
    extra: &mut FormExtra<'_>,
) -> bool {
    let types: Vec<&ohl_structures::StructureType> = model
        .types
        .as_ref()
        .map(|(s, _)| s.iter().collect())
        .unwrap_or_default();
    let mut new_pylon = false;
    // The structure picker (opened from the type's button): its choice
    // goes in the form.
    if let Some(picker) = extra.picker.as_mut() {
        let id = egui::Id::new("structure-picker-editor");
        match picker.show(ui.ctx(), id, model, extra.rs, extra.picker_preview) {
            Some(crate::structure_picker::Outcome::Choose {
                structure_type,
                extension,
            }) => {
                f.structure_type = structure_type;
                f.extension = extension;
                *extra.picker = None;
            }
            Some(crate::structure_picker::Outcome::Cancel) => *extra.picker = None,
            None => {}
        }
    }
    if types.is_empty() && f.new {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            "No structure type in the library yet: without a type the structure has no \
             strings, so no wire can hang on it (only strings added by hand). Make a structure \
             type first.",
        );
        new_pylon = ui
            .button("New structure from a template…")
            .on_hover_text("Tools › New structure from a template…: this window stays open")
            .clicked();
        ui.separator();
    }
    let help = |ui: &mut egui::Ui, text: &str| {
        ui.label(egui::RichText::new(text).small().weak());
    };
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(330.0);
            egui::Grid::new("structure-form").num_columns(2).show(ui, |ui| {
                ui.label("id");
                ui.add_enabled(f.new, egui::TextEdit::singleline(&mut f.id).desired_width(140.0));
                ui.end_row();
                ui.label("type").on_hover_text(
                    "A structure type of the library (its points and strings come from it), \
                     or none: its points given here, its strings added with Add string",
                );
                if crate::structure_picker::button(
                    ui,
                    model,
                    f.structure_type.as_deref(),
                    None,
                    "(none: points given here)",
                    200.0,
                )
                .clicked()
                {
                    *extra.picker = Some(crate::structure_picker::Picker::new(
                        format!("Structure type — {}", f.id),
                        f.structure_type.as_deref(),
                        f.extension.as_deref(),
                        true,
                        true,
                    ));
                }
                ui.end_row();
                let extensions: Vec<String> = f
                    .structure_type
                    .as_deref()
                    .and_then(|id| types.iter().find(|t| t.id == id))
                    .map(|t| t.extensions.iter().map(|e| e.name.clone()).collect())
                    .unwrap_or_default();
                if !extensions.is_empty() {
                    ui.label("extension");
                    egui::ComboBox::from_id_salt("structure-extension")
                        .selected_text(f.extension.as_deref().unwrap_or("(base)"))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut f.extension, None, "(base)");
                            for e in &extensions {
                                ui.selectable_value(&mut f.extension, Some(e.clone()), e);
                            }
                        });
                    ui.end_row();
                }
                if f.structure_type.is_some() {
                    ui.label("height adjustment");
                    value(ui, &mut f.height_adjust, 0.01, " m")
                        .on_hover_text("Height of the type's feet above the ground, e.g. for foundations");
                    ui.end_row();
                }
                ui.label("description");
                ui.add(
                    egui::TextEdit::multiline(&mut f.description)
                        .desired_rows(2)
                        .desired_width(200.0)
                        .hint_text("what it is (free text)"),
                );
                ui.end_row();
                // Its routes: from the alignment it stands on, or its own.
                ui.label("routes").on_hover_text(
                    "The routes it is on. Standing on an alignment (a point, or within 5 m of \
                     it): that alignment's routes, by itself. Away from every alignment (a \
                     crossing line): tick its route(s) here",
                );
                ui.vertical(|ui| {
                    let line = model.stored_line().unwrap_or(&model.line);
                    let mut line_with = line.clone();
                    if let Some(r) = &f.make_route {
                        line_with.routes.push(ohl_line::Route::new(r, r));
                    }
                    crate::alignment_tools::route_ticks(
                        ui,
                        "structure-routes",
                        &line_with,
                        &mut f.routes,
                        &mut f.new_route,
                        &mut f.make_route,
                    );
                    let found = if f.on_alignment.is_some() || !f.routes.is_empty() {
                        Vec::new()
                    } else {
                        let mut probe = line.clone();
                        let mut s = ohl_line::Structure::new("(new)".to_owned(), f.position);
                        s.routes.clear();
                        probe.structures.push(s);
                        probe.routes_of("(new)")
                    };
                    if !f.routes.is_empty() {
                        ui.label(egui::RichText::new("its own (ticked)").small().weak());
                    } else if f.on_alignment.is_some() {
                        ui.label(
                            egui::RichText::new("those of the alignment point it stands on")
                                .small()
                                .weak(),
                        );
                    } else if found.is_empty() {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            egui::RichText::new(
                                "on no alignment: tick its route(s) (or put it on an alignment point)",
                            )
                            .small(),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new(format!("{} (from the alignment)", found.join(", ")))
                                .small()
                                .weak(),
                        );
                    }
                });
                ui.end_row();
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.strong("Where it stands");
                if !f.new
                    && ui
                        .button("Put it on the line again")
                        .on_hover_text(
                            "On its alignment point (or the nearest free one within 5 m), turned \
                             along the line there (the bisector at an angle). Placed, a structure \
                             stays as it is when the alignment changes",
                        )
                        .clicked()
                    && !f.put_on_line(model)
                {
                    f.message = Some("no alignment point within 5 m of it".into());
                }
            });
            if let Some(m) = &f.message {
                ui.colored_label(ui.visuals().warn_fg_color, egui::RichText::new(m).small());
            }
            let points = alignment_points(&model.line);
            let mut on = f.on_alignment.is_some();
            ui.horizontal(|ui| {
                ui.radio_value(&mut on, false, "at coordinates");
                ui.add_enabled_ui(!points.is_empty(), |ui| {
                    ui.radio_value(&mut on, true, "on an alignment point")
                        .on_disabled_hover_text("The line has no alignment points yet");
                });
            });
            if on && f.on_alignment.is_none() {
                if let Some(p) = points.iter().find(|p| p.3.is_none()) {
                    f.place_on(model, (p.0.clone(), p.1));
                }
            } else if !on {
                f.on_alignment = None;
            }
            match f.on_alignment.clone() {
                Some((route, k)) => {
                    // The point's name first (as written in the views),
                    // then its alignment and place there.
                    let text = |p: &ListedPoint| {
                        format!(
                            "{} — {} #{} — E {:.1}, N {:.1}",
                            p.4,
                            p.0,
                            p.1 + 1,
                            p.2[0],
                            p.2[1]
                        )
                    };
                    let shown = points
                        .iter()
                        .find(|p| p.0 == route && p.1 == k)
                        .map_or(String::new(), text);
                    let mut chosen = None;
                    egui::ComboBox::from_id_salt("structure-alignment-point")
                        .selected_text(shown)
                        .width(300.0)
                        .height(300.0)
                        .show_ui(ui, |ui| {
                            for p in &points {
                                let taken = p.3.as_ref().filter(|s| **s != f.id);
                                let label = match taken {
                                    Some(s) => format!("{} (under {s})", text(p)),
                                    None => text(p),
                                };
                                if ui
                                    .add_enabled(
                                        taken.is_none(),
                                        egui::Button::selectable(p.0 == route && p.1 == k, label),
                                    )
                                    .clicked()
                                {
                                    chosen = Some((p.0.clone(), p.1));
                                }
                            }
                        });
                    if let Some(c) = chosen {
                        // The base kept at the point's height when so chosen.
                        let ground = f.on_ground;
                        f.place_on(model, c.clone());
                        if !ground && let Some(p) = points.iter().find(|p| p.0 == c.0 && p.1 == c.1) {
                            f.on_ground = false;
                            f.position = p.2;
                        }
                    }
                    help(ui, "It stands on the point, tied to it: the alignment follows it when it moves.");
                    if ui
                        .checkbox(&mut f.on_ground, "base on the ground")
                        .on_hover_text("Unticked: the base at the alignment point's own height")
                        .changed()
                        && !f.on_ground
                        && let Some(on) = &f.on_alignment
                        && let Some(p) = points.iter().find(|p| p.0 == on.0 && p.1 == on.1)
                    {
                        f.position = p.2;
                    }
                    // New: the alignment's other points too, one command.
                    if f.new
                        && let Some(on) = f.on_alignment.clone()
                    {
                        let more = f
                            .more
                            .get_or_insert_with(|| crate::placement::MorePoints::new(model, &on.0, on.1));
                        more.follow(model, &on);
                        let n = more.ticked().count();
                        egui::CollapsingHeader::new(format!(
                            "Also on other points of this alignment ({n} ticked)"
                        ))
                        .id_salt("structure-more-points")
                        .show(ui, |ui| more.ui(ui, model));
                    }
                }
                None => {
                    let before = f.position;
                    egui::Grid::new("structure-position").num_columns(2).show(ui, |ui| {
                        for (k, name) in ["E", "N", "z"].into_iter().enumerate() {
                            ui.label(name);
                            ui.add_enabled_ui(k < 2 || !f.on_ground, |ui| {
                                value(ui, &mut f.position[k], 0.1, " m");
                            });
                            ui.end_row();
                        }
                    });
                    ui.checkbox(&mut f.on_ground, "base on the ground");
                    if f.position != before {
                        f.follow_line(model);
                    }
                }
            }
            if f.on_ground
                && let Some(z) = model.ground_at(f.position[0], f.position[1])
            {
                f.position[2] = z;
            }
            ui.separator();
            ui.strong("Orientation");
            ui.horizontal(|ui| {
                if ui
                    .checkbox(&mut f.along_line, "along the line")
                    .on_hover_text(
                        "Ahead along the alignment; its cross-arms (right to left) on the \
                         bisector of the line angle there",
                    )
                    .changed()
                {
                    f.follow_line(model);
                }
                ui.add_enabled_ui(!f.along_line, |ui| {
                    value(ui, &mut f.orientation_deg, 0.1, "°").on_hover_text(
                        "Direction of its right (local x: the cross-arms), counter-clockwise from east",
                    );
                });
            });
            help(ui, "Its right (the cross-arms), counter-clockwise from east; ahead is a quarter turn to the left of it.");
        });
        // The preview, in its own axes, with the compass.
        extra.preview.shows(&format!("{} {}", f.id, f.new));
        let (key, labels) = structure_preview_key(f, model);
        let size = egui::vec2(360.0, 360.0);
        extra.preview.ui(
            ui,
            extra.rs,
            size,
            key,
            || structure_scene(f, model),
            &labels,
        );
    });
    if f.structure_type.is_none() {
        ui.separator();
        ui.strong("Points where strings hang");
        help(
            ui,
            "In its axes: across (to the right looking ahead), along (ahead), up from its base. \
             Strings are added on them with the Add string tool (or Edit on a string).",
        );
        let used: Vec<String> = model
            .line
            .structure(&f.id)
            .map(|s| s.strings.iter().map(|x| x.top.clone()).collect())
            .unwrap_or_default();
        let mut remove = None;
        egui::Grid::new("structure-points")
            .num_columns(2)
            .show(ui, |ui| {
                for (k, p) in f.points.iter_mut().enumerate() {
                    ui.add(egui::TextEdit::singleline(&mut p.label).desired_width(90.0));
                    ui.horizontal(|ui| {
                        for (i, name) in ["across", "along", "up"].iter().enumerate() {
                            ui.label(*name);
                            value(ui, &mut p.at[i], 0.05, " m");
                        }
                        ui.add(
                            egui::TextEdit::singleline(&mut p.description)
                                .desired_width(140.0)
                                .hint_text("description"),
                        );
                        if used.contains(&p.label) {
                            ui.label(egui::RichText::new("strings hang from it").small().weak());
                        } else if ui
                            .small_button("×")
                            .on_hover_text("Take the point away")
                            .clicked()
                        {
                            remove = Some(k);
                        }
                    });
                    ui.end_row();
                }
            });
        if let Some(k) = remove {
            f.points.remove(k);
        }
        if ui.small_button("+ point").clicked() {
            let taken: Vec<&str> = f.points.iter().map(|p| p.label.as_str()).collect();
            let label = next_id(
                taken.iter().copied(),
                f.points.last().map_or("A0", |p| p.label.as_str()),
            );
            let at = f
                .points
                .last()
                .map_or([0.0, 0.0, 20.0], |p| [-p.at[0], p.at[1], p.at[2]]);
            f.points.push(PointRow {
                description: Default::default(),
                label,
                at,
            });
        }
    }
    new_pylon
}

/// What the structure window's preview shows, as a key (built again when
/// it changes), and its labels.
pub(super) fn structure_preview_key(
    f: &StructureForm,
    model: &Model,
) -> (u64, Vec<crate::preview3d::Label>) {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    format!("{f:?}").hash(&mut h);
    model.line.display.ahead_deg.to_bits().hash(&mut h);
    if let Some(s) = model.line.structure(&f.id) {
        format!("{:?}{:?}", s.strings, s.wires).hash(&mut h);
    }
    let g = structure_geometry(f, model);
    let mut labels = compass_labels(f, model, &g);
    let orange = egui::Color32::from_rgb(255, 160, 80);
    let blue = egui::Color32::from_rgb(110, 200, 255);
    for (l, p) in &g.points {
        labels.push(([p[0], p[1], p[2] + 0.8], l.clone(), orange));
    }
    for (l, p, set) in &g.wires {
        let text = set.map_or(l.clone(), |s| format!("{l} (set {s})"));
        labels.push(([p[0], p[1], p[2] - 0.8], text, blue));
    }
    (h.finish(), labels)
}

/// The structure in its own axes (base at 0): its type's, or its own
/// points with the strings it has.
pub(super) fn structure_geometry(
    f: &StructureForm,
    model: &Model,
) -> crate::type_workspace::Geometry {
    let none = ohl_structures::InsulatorLibrary::default();
    if let Some((types, insulators)) = &model.types
        && let Some(t) = f.structure_type.as_deref().and_then(|id| types.get(id))
    {
        let mut g = crate::type_workspace::geometry(t, insulators);
        // The base at 0.
        let foot = t.foot_level();
        let down = |p: &mut [f64; 3]| p[2] -= foot;
        g.joints.iter_mut().for_each(|j| down(&mut j.1));
        g.members.iter_mut().for_each(|m| {
            down(&mut m.1);
            down(&mut m.2)
        });
        g.points.iter_mut().for_each(|p| down(&mut p.1));
        g.wires.iter_mut().for_each(|w| down(&mut w.1));
        g.strings.iter_mut().for_each(|s| {
            down(&mut s.1);
            down(&mut s.2)
        });
        g.sticks.iter_mut().flatten().for_each(down);
        return g;
    }
    let _ = &none;
    let mut g = crate::type_workspace::Geometry {
        points: f.points.iter().map(|p| (p.label.clone(), p.at)).collect(),
        ..Default::default()
    };
    if let Some(s) = model.line.structure(&f.id) {
        let local = |p| to_local(s.position, s.orientation_deg, p);
        g.wires = s
            .wires
            .iter()
            .map(|w| (w.label.clone(), local(w.position), w.set))
            .collect();
        let at = |l: &str| {
            g.points
                .iter()
                .find(|p| p.0 == l)
                .map(|p| p.1)
                .or_else(|| g.wires.iter().find(|w| w.0 == l).map(|w| w.1))
        };
        g.strings = s
            .strings
            .iter()
            .filter_map(|x| Some((x.label.clone(), at(&x.top)?, at(&x.wire)?)))
            .collect();
    }
    // A mast to the highest point and arms to the points.
    let top = g.points.iter().map(|p| p.1[2]).fold(5.0, f64::max);
    g.sticks.push(vec![[0.0; 3], [0.0, 0.0, top]]);
    for (_, p) in &g.points {
        g.sticks.push(vec![[0.0, 0.0, p[2]], *p]);
    }
    g
}

/// The compass's radius: beyond the structure's reach in plan.
pub(super) fn compass_radius(g: &crate::type_workspace::Geometry) -> f64 {
    let reach = g
        .points
        .iter()
        .map(|p| p.1)
        .chain(g.wires.iter().map(|w| w.1))
        .chain(g.joints.iter().map(|j| j.1))
        .chain(g.sticks.iter().flatten().copied())
        .map(|p| p[0].hypot(p[1]))
        .fold(3.0, f64::max);
    (reach * 1.25).max(0.35 * height(g))
}

/// The structure's height (its highest point).
pub(super) fn height(g: &crate::type_workspace::Geometry) -> f64 {
    g.points
        .iter()
        .map(|p| p.1)
        .chain(g.wires.iter().map(|w| w.1))
        .chain(g.sticks.iter().flatten().copied())
        .map(|p| p[2])
        .fold(0.0, f64::max)
}

/// Where a plan angle of the project's convention points, in the
/// structure's axes (clockwise seen from above, ahead at the project's
/// angle ahead).
pub(crate) fn compass_direction(angle: f64, ahead: f64) -> [f64; 2] {
    let phi = (angle - ahead).to_radians();
    [phi.sin(), phi.cos()]
}

pub(super) fn compass_labels(
    f: &StructureForm,
    model: &Model,
    g: &crate::type_workspace::Geometry,
) -> Vec<crate::preview3d::Label> {
    compass_texts(
        compass_radius(g),
        model.line.display.ahead_deg,
        Some(f.orientation_deg),
        0.0,
    )
}

/// The compass's texts (radius `r`, at height `z`): the four angles as
/// the project counts them (`ahead`), with ahead, right, back, left; and
/// north when the structure's orientation is known.
pub(crate) fn compass_texts(
    r: f64,
    ahead: f64,
    orientation: Option<f64>,
    z: f64,
) -> Vec<crate::preview3d::Label> {
    let r = r * 1.12;
    let mut out = Vec::new();
    let grey = egui::Color32::from_rgb(200, 200, 200);
    for a in [0.0, 90.0, 180.0, 270.0] {
        let d = compass_direction(a, ahead);
        let word = match ((a - ahead).rem_euclid(360.0)).round() as i64 {
            0 => " ahead",
            90 => " right",
            180 => " back",
            270 => " left",
            _ => "",
        };
        out.push(([d[0] * r, d[1] * r, z], format!("{a:.0}°{word}"), grey));
    }
    if let Some(o) = orientation {
        let o = o.to_radians();
        out.push((
            [o.sin() * r * 0.8, o.cos() * r * 0.8, z],
            "N".into(),
            egui::Color32::from_rgb(120, 170, 255),
        ));
    }
    out
}

/// The compass on the ground (radius `r`, at height `z`): a ring with
/// ticks every 15° as the project counts them, ahead (green) and right
/// (red) from the centre, north (blue) when the orientation is known.
pub(crate) fn draw_compass(
    s: &mut ohl_render::Scene,
    r: f64,
    ahead: f64,
    orientation: Option<f64>,
    z: f64,
) {
    let ring: Vec<[f64; 3]> = (0..=72)
        .map(|k| {
            let t = f64::from(k) * std::f64::consts::TAU / 72.0;
            [r * t.cos(), r * t.sin(), z]
        })
        .collect();
    s.polyline(&ring, [110, 115, 125, 255], 1.5);
    for k in 0..24 {
        let d = compass_direction(f64::from(k) * 15.0, ahead);
        let inner = if k % 6 == 0 {
            0.82
        } else if k % 2 == 0 {
            0.9
        } else {
            0.95
        };
        s.polyline(
            &[
                [d[0] * r * inner, d[1] * r * inner, z],
                [d[0] * r, d[1] * r, z],
            ],
            [150, 155, 165, 255],
            1.5,
        );
    }
    s.polyline(
        &[[0.0, 0.0, z], [0.0, r * 0.75, z]],
        [90, 200, 90, 255],
        3.0,
    );
    s.polyline(
        &[[0.0, 0.0, z], [r * 0.75, 0.0, z]],
        [230, 80, 70, 255],
        3.0,
    );
    if let Some(o) = orientation {
        let o = o.to_radians();
        s.polyline(
            &[[0.0, 0.0, z], [o.sin() * r * 0.6, o.cos() * r * 0.6, z]],
            [120, 170, 255, 255],
            2.0,
        );
    }
}

/// The structure window's preview: the ground's compass, the structure's
/// sticks, points, strings and wire points.
pub(super) fn structure_scene(f: &StructureForm, model: &Model) -> ohl_render::Scene {
    let g = structure_geometry(f, model);
    let mut s = ohl_render::Scene::new([0.0; 3]);
    let r = compass_radius(&g);
    let ahead = model.line.display.ahead_deg;
    draw_compass(&mut s, r, ahead, Some(f.orientation_deg), 0.0);
    for l in &g.sticks {
        s.polyline(l, [215, 215, 220, 255], 2.0);
    }
    for (_, a, b, _) in &g.members {
        s.polyline(&[*a, *b], [215, 215, 220, 255], 1.5);
    }
    for (_, a, b) in &g.strings {
        s.polyline(&[*a, *b], [120, 180, 255, 255], 2.5);
    }
    for (_, p) in &g.points {
        s.point(*p, [255, 150, 60, 255], 9.0);
    }
    for (_, p, _) in &g.wires {
        s.point(*p, [90, 200, 255, 255], 9.0);
    }
    s
}
