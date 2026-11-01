// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The ground a field run stands on (owner, 2026-11-01: "make it an
//! option to choose which TIN layer it uses and if none exist which survey
//! data to use to make it"): the surface of the checks (the default), a
//! terrain surface (TIN layer) of the project, or the survey's ground
//! points (the point cloud's classes with the role *ground*, the lowest
//! per metre within a corridor along the spans) made into a surface for
//! the run. It gives the tiled floor's tiles their elevations, the
//! profiles their height above the ground and the cuts and volumes their
//! bottom. Shared by the Magnetic and the Electric field windows.

use std::sync::Arc;

use eframe::egui;
use ohl_terrain::Terrain;

use crate::model::Model;

/// Where the ground comes from.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldGround {
    /// The surface of the checks (Line › Terrain), else the surface shown.
    #[default]
    Checks,
    /// A terrain surface of the project, by name.
    Surface(String),
    /// The point cloud's ground points within this distance of the spans,
    /// m, made into a surface.
    Survey(f64),
}

/// The survey's corridor by default, m.
pub const SURVEY_CORRIDOR: f64 = 250.0;

/// What a run reads its ground from, taken now (read in the background).
#[derive(Debug, Clone)]
pub enum Source {
    /// Held by the model.
    Held(Option<Arc<Terrain>>),
    /// A surface of the project file.
    Surface(std::path::PathBuf, String),
    /// The point cloud's ground classes in a corridor of the line.
    Survey {
        cloud: String,
        classes: Vec<u8>,
        corridor: ohl_lidar::Corridor,
    },
}

/// The choice in words.
fn words(g: &FieldGround, model: &Model) -> String {
    match g {
        FieldGround::Checks => match &model.check_surface {
            Some(n) => format!("the surface of the checks ({n})"),
            None => "the surface of the checks (the surface shown)".into(),
        },
        FieldGround::Surface(n) => format!("terrain surface {n}"),
        FieldGround::Survey(d) => format!("the survey's ground points within {d:.0} m"),
    }
}

/// The choice: the surface of the checks, a terrain surface (none in the
/// project: said), the survey (no point cloud: said).
pub fn ui(ui: &mut egui::Ui, id: &str, model: &Model, g: &mut FieldGround) {
    let names = crate::surfaces::names(model);
    ui.horizontal_wrapped(|ui| {
        ui.label("Ground from");
        egui::ComboBox::from_id_salt((id, "field-ground"))
            .selected_text(words(g, model))
            .show_ui(ui, |ui| {
                ui.selectable_value(g, FieldGround::Checks, words(&FieldGround::Checks, model));
                for n in &names {
                    let v = FieldGround::Surface(n.clone());
                    let w = words(&v, model);
                    ui.selectable_value(g, v, w);
                }
                if names.is_empty() {
                    ui.label(egui::RichText::new("no terrain surface in the project").weak());
                }
                if model.cloud.is_some() {
                    let v = FieldGround::Survey(SURVEY_CORRIDOR);
                    let w = words(&v, model);
                    if ui
                        .selectable_label(matches!(g, FieldGround::Survey(_)), w)
                        .clicked()
                        && !matches!(g, FieldGround::Survey(_))
                    {
                        *g = v;
                    }
                } else {
                    ui.label(egui::RichText::new("no survey point cloud").weak());
                }
            });
        if let FieldGround::Survey(d) = g {
            ui.label("within");
            crate::style::field(
                ui,
                egui::DragValue::new(d)
                    .range(20.0..=2000.0)
                    .speed(5.0)
                    .suffix(" m"),
            );
            ui.label("of the spans");
        }
    })
    .response
    .on_hover_text(
        "The ground under the profiles, cuts and volumes and, in the electric field, the tiled \
         floor's tiles: the surface of the checks, a terrain surface (TIN layer) of the project, \
         or the survey's ground points made into a surface for the run (the lowest per metre)",
    );
}

/// The source of `g` in `model`, to read in the background.
pub fn source(model: &Model, g: &FieldGround) -> Source {
    match g {
        FieldGround::Checks => Source::Held(model.ground_for_checks()),
        FieldGround::Surface(n) => {
            // Held already: shared.
            let (held, _) = model.held_surfaces(std::slice::from_ref(n));
            match held.into_iter().next() {
                Some((_, t)) => Source::Held(Some(t)),
                None => Source::Surface(model.sources.file.clone(), n.clone()),
            }
        }
        FieldGround::Survey(d) => match &model.cloud {
            Some(cloud) => Source::Survey {
                cloud: cloud.clone(),
                classes: model.scheme.codes_with_role("ground"),
                corridor: ohl_lidar::Corridor::along_spans(&model.line, *d),
            },
            None => Source::Held(model.ground_for_checks()),
        },
    }
}

impl Source {
    /// The ground, and a note of where it came from (none: held).
    pub fn read(self) -> Result<(Option<Arc<Terrain>>, Option<String>), String> {
        match self {
            Source::Held(t) => Ok((t, None)),
            Source::Surface(file, name) => {
                let t = crate::surfaces::load(&file, &name)?;
                Ok((
                    Some(Arc::new(t)),
                    Some(format!("ground: the terrain surface {name}")),
                ))
            }
            Source::Survey {
                cloud,
                classes,
                corridor,
            } => {
                if classes.is_empty() {
                    return Err(
                        "ground from the survey: no LiDAR class with the role ground".into(),
                    );
                }
                let source = ohl_pointcloud::open_source(&cloud)?;
                let mut cells = ohl_lidar::LowestPerCell::new(Some(1.0));
                // Exit clause: ten minutes is far beyond a line's corridor.
                let budget =
                    ohl_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(600));
                source
                    .visit(
                        Some(&corridor),
                        Some(&classes),
                        &budget,
                        &mut |x, y, z, _| cells.add(x, y, z),
                    )
                    .map_err(|e| e.to_string())?;
                let n = cells.points().len();
                if n < 3 {
                    return Err(
                        "ground from the survey: fewer than 3 ground points along the spans".into(),
                    );
                }
                let t = cells.terrain(Some(50.0)).map_err(|e| e.to_string())?;
                Ok((
                    Some(Arc::new(t)),
                    Some(format!(
                        "ground: the survey's ground points, the lowest per metre ({n} points) along the spans"
                    )),
                ))
            }
        }
    }
}
