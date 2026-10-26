// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Line › Terrain › Survey data › Class colours: the display colour of the LiDAR
//! points by class number, from a class scheme file (`.ohlc`: names,
//! roles, colours). Every class of the point cloud is listed with its
//! points; a colour is picked per class (none: from its roles). The scheme
//! is this user's choice (remembered on this computer): for this project
//! or for every project; it can be saved to a file to share.

use std::collections::BTreeSet;
use std::path::PathBuf;

use eframe::egui;
use ohl_core::text::format_rgb;
use ohl_lidar::ClassScheme;
use ohl_lidar::classes::ClassDef;

use crate::model::Model;

/// What the window asks of the application.
pub enum ClassAction {
    /// The colours changed: the points are drawn again.
    Recolor,
    /// Use this scheme file for this project (`None`: the point cloud's
    /// own), opened again.
    UseForProject(Option<PathBuf>),
    /// Use this scheme file for every project (`None`: none), and for this
    /// one.
    UseForAll(Option<PathBuf>),
}

/// The window's state.
#[derive(Default)]
pub struct ClassWindow {
    /// The last thing done.
    pub status: Option<(String, bool)>,
    /// Changes not saved to the scheme file.
    pub dirty: bool,
}

impl ClassWindow {
    /// The window; `None` when nothing to do.
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        open: &mut bool,
        model: &mut Model,
        default_scheme: Option<&PathBuf>,
    ) -> Option<ClassAction> {
        let mut action = None;
        let file = model.sources.class_scheme.clone();
        egui::Window::new("Class colours")
            .open(open)
            .default_width(720.0)
            .show(ctx, |ui| {
                ui.label(format!(
                    "Scheme: {} — {}",
                    model.scheme.name,
                    file.as_ref()
                        .map_or("the point cloud's own".into(), |p| p.display().to_string())
                ));
                ui.label(
                    egui::RichText::new(format!(
                        "For every project: {}. Your choice, remembered on this computer (not in \
                         the project).",
                        default_scheme.map_or("the point cloud's own scheme".into(), |p| p
                            .display()
                            .to_string())
                    ))
                    .small()
                    .weak(),
                );
                ui.horizontal(|ui| {
                    if ui
                        .button("Open a scheme…")
                        .on_hover_text("A class scheme file (.ohlc): names, roles, colours")
                        .clicked()
                        && let Some(p) = rfd::FileDialog::new()
                            .add_filter(
                                "Class scheme",
                                &ohl_core::files::open_extensions(
                                    ohl_lidar::classes::CLASS_FILE_EXTENSION,
                                ),
                            )
                            .pick_file()
                    {
                        match ClassScheme::load(&p) {
                            Ok(_) => action = Some(ClassAction::UseForProject(Some(p))),
                            Err(e) => self.status = Some((e, true)),
                        }
                    }
                    if ui
                        .add_enabled(file.is_some() && self.dirty, egui::Button::new("Save"))
                        .on_disabled_hover_text("Save as… a file first")
                        .clicked()
                        && let Some(p) = &file
                    {
                        self.save(model, p.clone());
                    }
                    if ui.button("Save as…").clicked()
                        && let Some(p) = rfd::FileDialog::new()
                            .add_filter("Class scheme", &[ohl_lidar::classes::CLASS_FILE_EXTENSION])
                            .set_file_name(format!("{}.ohlc", model.scheme.id))
                            .save_file()
                        && self.save(model, p.clone())
                    {
                        action = Some(ClassAction::UseForProject(Some(p)));
                    }
                    if ui
                        .add_enabled(file.is_some(), egui::Button::new("Use for every project"))
                        .clicked()
                    {
                        action = Some(ClassAction::UseForAll(file.clone()));
                    }
                    if ui
                        .add_enabled(file.is_some(), egui::Button::new("Back to the cloud's own"))
                        .on_hover_text("This project: the class scheme stored with its point cloud")
                        .clicked()
                    {
                        action = Some(ClassAction::UseForProject(None));
                    }
                });
                ui.separator();
                let counts: std::collections::HashMap<u8, u64> =
                    model.class_counts.iter().copied().collect();
                let codes: BTreeSet<u8> = model
                    .class_counts
                    .iter()
                    .map(|c| c.0)
                    .chain(model.scheme.classes.iter().map(|c| c.code))
                    .collect();
                let mut changed = false;
                egui::ScrollArea::vertical()
                    .max_height(440.0)
                    .show(ui, |ui| {
                        egui::Grid::new("class-colours")
                            .striped(true)
                            .spacing([12.0, 3.0])
                            .show(ui, |ui| {
                                for h in ["class", "name", "roles", "points", "colour", ""] {
                                    ui.strong(h);
                                }
                                ui.end_row();
                                for code in codes {
                                    let def = model.scheme.class(code).cloned();
                                    ui.label(code.to_string());
                                    ui.label(
                                        def.as_ref()
                                            .map_or("(not in the scheme)", |d| d.name.as_str()),
                                    );
                                    ui.label(
                                        egui::RichText::new(
                                            def.as_ref()
                                                .map_or(String::new(), |d| d.roles.join(", ")),
                                        )
                                        .small(),
                                    );
                                    ui.label(
                                        counts.get(&code).map_or("—".into(), |n| n.to_string()),
                                    );
                                    let mut col = crate::scene::class_color(&model.scheme, code)
                                        .unwrap_or([128, 128, 128]);
                                    let own = def.as_ref().and_then(ClassDef::rgb).is_some();
                                    if ui.color_edit_button_srgb(&mut col).changed() {
                                        set_color(&mut model.scheme, code, Some(format_rgb(col)));
                                        changed = true;
                                    }
                                    if ui
                                        .add_enabled(own, egui::Button::new("↺").small())
                                        .on_hover_text("The colour of its roles")
                                        .clicked()
                                    {
                                        set_color(&mut model.scheme, code, None);
                                        changed = true;
                                    }
                                    ui.end_row();
                                }
                            });
                    });
                if changed {
                    self.dirty = true;
                    action = Some(ClassAction::Recolor);
                }
                if let Some((t, e)) = &self.status {
                    if *e {
                        crate::menu::error_label(ui, t);
                    } else {
                        ui.label(t);
                    }
                }
            });
        action
    }

    /// Writes the scheme (with its colours) to `path`.
    fn save(&mut self, model: &Model, path: PathBuf) -> bool {
        match std::fs::write(&path, model.scheme.to_toml()) {
            Ok(()) => {
                self.dirty = false;
                self.status = Some((format!("saved {}", path.display()), false));
                true
            }
            Err(e) => {
                self.status = Some((format!("{}: {e}", path.display()), true));
                false
            }
        }
    }
}

/// Sets (or clears) the colour of class `code`, adding it to the scheme if
/// it is not there.
pub fn set_color(scheme: &mut ClassScheme, code: u8, color: Option<String>) {
    match scheme.classes.iter_mut().find(|c| c.code == code) {
        Some(c) => c.color = color,
        None => {
            scheme.classes.push(ClassDef {
                code,
                name: format!("Class {code}"),
                roles: Vec::new(),
                description: String::new(),
                color,
            });
            scheme.classes.sort_by_key(|c| c.code);
        }
    }
}
