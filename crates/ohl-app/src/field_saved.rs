// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A field window's run kept in Checks › Performed checks (owner,
//! 2026-11-01: "the results need to be saveable in the results library
//! for the checks"): its inputs as the window keeps them in the project,
//! the field (profiles, grids, points, cuts, the volumes' statistics) and,
//! up to [`MAX_SAVED_TRIANGLES`], the volumes' surfaces packed as the
//! safety volumes' are. Opened again from the library, the run is shown
//! with what changed on the line since. Shared by the Magnetic and the
//! Electric field windows.

use std::sync::Arc;

use eframe::egui;
use ohl_electrical::contour::Mesh;
use ohl_session::reports::{self, CheckReport, ReportStatus};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::model::Model;
use crate::safety_volume::saved::{pack, unpack};

/// The magnetic field's id in the library of performed checks.
pub const MAGNETIC_CHECK_ID: &str = "ohl.magnetic-field";
/// The electric field's.
pub const ELECTRIC_CHECK_ID: &str = "ohl.electric-field";
/// The version of the saved parameters and results.
pub const SAVED_VERSION: u32 = 1;
/// A run with more triangles than this is saved without its surfaces
/// (run again to see them).
pub use crate::safety_volume::MAX_SAVED_TRIANGLES;

/// The Save row's state.
#[derive(Debug, Clone, Default)]
pub struct Saving {
    /// The name typed.
    pub name: String,
    /// Asked to replace this report (a second click replaces it).
    overwrite: Option<String>,
    /// The last save's outcome (error or not).
    message: Option<(String, bool)>,
    /// The run shown came from the library: what changed on the line since.
    pub stale: Option<String>,
}

impl Saving {
    /// The Save row; the name to save under when Save was clicked (and a
    /// second time for a report of that name).
    pub fn ui(&mut self, ui: &mut egui::Ui, model: &Model) -> Option<String> {
        let mut out = None;
        ui.horizontal(|ui| {
            ui.label("Save in the library of performed checks as");
            ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(160.0));
            let name = self.name.trim().to_owned();
            let exists = model.session().is_some_and(|s| s.report(&name).is_some());
            let asked = self.overwrite.as_deref() == Some(name.as_str());
            let label = if exists && asked {
                "Replace it"
            } else {
                "Save in Performed checks"
            };
            if ui
                .button(label)
                .on_hover_text(
                    "Kept with the project line; reopened from Checks › Performed checks",
                )
                .clicked()
            {
                if name.is_empty() {
                    self.message = Some(("give it a name".into(), true));
                } else if exists && !asked {
                    self.message = Some((
                        format!("\"{name}\" exists: click again to replace it"),
                        true,
                    ));
                    self.overwrite = Some(name);
                } else {
                    self.overwrite = None;
                    out = Some(name);
                }
            }
        });
        if let Some((m, error)) = &self.message {
            crate::menu::message(
                ui,
                egui::RichText::new(m).small(),
                error.then_some(ui.visuals().warn_fg_color),
            );
        }
        out
    }

    /// The outcome of a save.
    pub fn outcome(&mut self, name: &str, r: &Result<(), String>) {
        self.message = Some(match r {
            Ok(()) => (format!("saved as \"{name}\""), false),
            Err(e) => (e.clone(), true),
        });
    }
}

/// The results of a run: its weather case, the field (the volumes'
/// surfaces taken out: `meshes`, packed when they fit); its triangles.
pub fn results<F: Serialize>(
    case: &str,
    field: &F,
    meshes: &[(f64, Arc<Mesh>)],
) -> Result<(Value, usize), String> {
    let triangles: usize = meshes.iter().map(|m| m.1.triangles.len()).sum();
    let packed = if triangles <= MAX_SAVED_TRIANGLES {
        Some(
            meshes
                .iter()
                .map(|(l, m)| Ok(json!({ "level": l, "mesh": pack(&m.vertices, &m.triangles)? })))
                .collect::<Result<Vec<Value>, String>>()?,
        )
    } else {
        None
    };
    let field = serde_json::to_value(field).map_err(|e| e.to_string())?;
    Ok((
        json!({ "case": case, "field": field, "meshes": packed }),
        triangles,
    ))
}

/// A run's case, field and surfaces read back from [`results`] (no
/// surfaces: saved without them).
#[allow(clippy::type_complexity)]
pub fn read<F: DeserializeOwned>(
    v: &Value,
) -> Result<(String, F, Vec<(f64, Arc<Mesh>)>, bool), String> {
    let case = v
        .get("case")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let field = serde_json::from_value(v.get("field").cloned().unwrap_or(Value::Null))
        .map_err(|e| format!("the saved field: {e}"))?;
    let mut meshes = Vec::new();
    let mut with = false;
    if let Some(list) = v.get("meshes").and_then(Value::as_array) {
        with = true;
        for m in list {
            let level = m.get("level").and_then(Value::as_f64).unwrap_or(0.0);
            let text = m.get("mesh").and_then(Value::as_str).unwrap_or_default();
            let (vertices, triangles) = unpack(text)?;
            meshes.push((
                level,
                Arc::new(Mesh {
                    vertices,
                    triangles,
                }),
            ));
        }
    }
    Ok((case, field, meshes, with))
}

/// The parameters of a run: the window's inputs, with the calculation
/// option of the ice and wind.
pub fn parameters<S: Serialize>(inputs: &S, loads_per: ohl_line::LoadsPer) -> Value {
    json!({
        "version": SAVED_VERSION,
        "loads_per": loads_per.as_str(),
        "inputs": serde_json::to_value(inputs).unwrap_or(Value::Null),
    })
}

/// The inputs of saved `parameters` (none: unreadable, the window's
/// defaults kept).
pub fn inputs<S: DeserializeOwned>(parameters: &Value) -> Option<S> {
    serde_json::from_value(parameters.get("inputs")?.clone()).ok()
}

/// A report named `name` of check `check`.
pub fn report(
    model: &Model,
    (check, title): (&str, &str),
    name: &str,
    summary: String,
    parameters: Value,
    results: Value,
) -> Result<CheckReport, String> {
    let editing = model
        .editing
        .as_ref()
        .map_err(|e| format!("read-only: {e}"))?;
    let s = &editing.session;
    Ok(CheckReport {
        format: reports::REPORT_FILE_FORMAT.into(),
        version: reports::REPORT_FILE_VERSION,
        id: name.to_owned(),
        check: check.into(),
        title: title.into(),
        time: s.now().map_err(|e| e.to_string())?,
        user: editing.user.clone(),
        app: format!("OHL-Design {}", env!("CARGO_PKG_VERSION")),
        plugin: None,
        status: ReportStatus::Draft,
        note: s.report(name).map(|o| o.note).unwrap_or_default(),
        state: s.check_state().map_err(|e| e.to_string())?,
        summary,
        parameters,
        results,
    })
}

/// What changed since saved report `r` was made, in words (`None`:
/// nothing): the line, the calculation options.
pub fn changed_since(r: &CheckReport, model: &Model) -> Option<String> {
    let mut line = None;
    if let Some(s) = model.session()
        && let Ok(now) = s.check_state()
        && !reports::changed_since(&r.state, &now).is_empty()
    {
        line = Some("the line".to_owned());
    }
    match (
        line,
        crate::revision::saved_loads_per_changed(&r.parameters, model),
    ) {
        (Some(a), Some(b)) => Some(format!("{a} and {b}")),
        (a, b) => a.or(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_reads_back_as_saved() {
        let mesh = Mesh {
            vertices: vec![[1.0, 2.0, 3.0], [4.5, 2.0, 3.0], [1.0, 7.25, 3.0]],
            triangles: vec![[0, 1, 2]],
        };
        let field = json!({ "height": 1.0, "profiles": [] });
        let (v, triangles) = results("hot", &field, &[(1.0, Arc::new(mesh.clone()))]).unwrap();
        assert_eq!(triangles, 1);
        let (case, back, meshes, with): (String, Value, _, bool) = read(&v).unwrap();
        assert_eq!(case, "hot");
        assert_eq!(back, field);
        assert!(with);
        assert_eq!(meshes.len(), 1);
        assert_eq!(meshes[0].0, 1.0);
        assert_eq!(*meshes[0].1, mesh);
    }
}
