// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Terrain › Survey data › Import LAS/LAZ and Export LAS/LAZ: the windows
//! and the jobs they start in the background (a progress bar; the view
//! stays usable). An import goes into the project's point cloud (or a new
//! one) and the project is opened again to show it.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use eframe::egui;

use crate::model::Model;

/// The Import LAS/LAZ window.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportDialog {
    /// Tiles chosen.
    pub files: Vec<PathBuf>,
    /// Point cloud of the project to add them to (existing or new).
    pub cloud: String,
    /// Coordinate system stored with a new cloud.
    pub crs: String,
    /// Chunk size of a new cloud, m.
    pub chunk: f64,
    /// Class scheme (`.olsc`) stored with a new cloud.
    pub scheme: Option<PathBuf>,
    /// Why it cannot start.
    pub error: Option<String>,
}

/// What the Export LAS/LAZ window writes.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportDialog {
    /// Only the classes shown (else every class).
    pub only_shown: bool,
    /// Only around the line: its structures' area grown by `margin` (else
    /// the whole survey).
    pub around_line: bool,
    /// Margin around the line, m.
    pub margin: f64,
    /// Write the class scheme next to it (`.olsc`).
    pub scheme: bool,
    /// Why it cannot start.
    pub error: Option<String>,
}

impl Default for ExportDialog {
    fn default() -> Self {
        Self {
            only_shown: false,
            around_line: false,
            margin: 100.0,
            scheme: true,
            error: None,
        }
    }
}

/// A message of a job.
enum Msg {
    Progress(String, f32),
    Done(Result<String, String>),
}

/// An import or export running in the background.
pub struct Job {
    /// What it does.
    pub title: String,
    rx: Receiver<Msg>,
    /// Latest progress: text, fraction done.
    pub progress: (String, f32),
    /// Finished: its report or error.
    pub done: Option<Result<String, String>>,
    /// Open the project again when it succeeded (an import).
    pub reopen: bool,
}

impl std::fmt::Debug for Job {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Job").field("title", &self.title).finish()
    }
}

impl Job {
    fn start(
        title: String,
        reopen: bool,
        ctx: &egui::Context,
        work: impl FnOnce(&dyn Fn(String, f32)) -> Result<String, String> + Send + 'static,
    ) -> Self {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let report = |text: String, f: f32| {
                let _ = tx.send(Msg::Progress(text, f));
                ctx.request_repaint();
            };
            let result = work(&report);
            let _ = tx.send(Msg::Done(result));
            ctx.request_repaint();
        });
        Self {
            title,
            rx,
            progress: (String::new(), 0.0),
            done: None,
            reopen,
        }
    }

    /// Takes the messages since the last frame.
    pub fn poll(&mut self) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Progress(t, f) => self.progress = (t, f),
                Msg::Done(r) => self.done = Some(r),
            }
        }
    }
}

/// The project file and point clouds of a model (`Err`: why there is no
/// project to import into).
fn project_of(model: &Model) -> Result<(PathBuf, Vec<String>), String> {
    model.can_edit()?;
    let file = model.sources.file.clone();
    if file.extension().is_none_or(|e| e != "ols") {
        return Err("point clouds are imported into a project (.ols)".into());
    }
    let clouds = ols_pointcloud::Store::open(&file)
        .and_then(|s| s.clouds())
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|c| c.name)
        .collect();
    Ok((file, clouds))
}

impl ImportDialog {
    /// The window for `model`'s project.
    pub fn new(model: &Model) -> Self {
        let clouds = project_of(model).map(|p| p.1).unwrap_or_default();
        Self {
            files: Vec::new(),
            cloud: clouds.first().cloned().unwrap_or_else(|| "lidar".into()),
            crs: model.line.crs.clone(),
            chunk: ols_pointcloud::DEFAULT_CHUNK,
            // The class scheme this user uses (Class colours), if a file.
            scheme: model
                .sources
                .class_scheme
                .clone()
                .filter(|p| !p.as_os_str().is_empty()),
            error: project_of(model).err(),
        }
    }

    /// The window; `Some(job)` once started.
    pub fn ui(&mut self, ctx: &egui::Context, model: &Model, open: &mut bool) -> Option<Job> {
        let mut job = None;
        let project = project_of(model);
        egui::Window::new("Import LAS/LAZ")
            .open(open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                let (file, clouds) = match &project {
                    Ok(p) => p.clone(),
                    Err(e) => {
                        ui.colored_label(egui::Color32::LIGHT_RED, e);
                        return;
                    }
                };
                ui.horizontal(|ui| {
                    if ui.button("Choose files…").clicked()
                        && let Some(f) = rfd::FileDialog::new()
                            .add_filter("LAS / LAZ", &["las", "laz"])
                            .pick_files()
                    {
                        self.files = f;
                    }
                    if ui.button("Choose a folder…").clicked()
                        && let Some(d) = rfd::FileDialog::new().pick_folder()
                    {
                        match ols_lidar::tiles_in(&d) {
                            Ok(t) => self.files = t,
                            Err(e) => self.error = Some(e.to_string()),
                        }
                    }
                });
                ui.label(match self.files.len() {
                    0 => "no file chosen".to_owned(),
                    1 => self.files[0].display().to_string(),
                    n => format!("{n} files in {}", self.files[0].parent().unwrap_or(Path::new("")).display()),
                });
                ui.separator();
                let new = !clouds.contains(&self.cloud);
                egui::Grid::new("import-las").show(ui, |ui| {
                    ui.label("Into the point cloud");
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("import-cloud")
                            .selected_text(&self.cloud)
                            .show_ui(ui, |ui| {
                                for c in &clouds {
                                    ui.selectable_value(&mut self.cloud, c.clone(), c);
                                }
                            });
                        ui.add(egui::TextEdit::singleline(&mut self.cloud).desired_width(120.0))
                            .on_hover_text("A name not in the list makes a new point cloud");
                    });
                    ui.end_row();
                    if new {
                        ui.label("Coordinate system");
                        ui.text_edit_singleline(&mut self.crs);
                        ui.end_row();
                        ui.label("Chunk size, m");
                        ui.add(egui::DragValue::new(&mut self.chunk).range(8.0..=1024.0))
                            .on_hover_text("64 m: four chunks per 256 m tile of the grid");
                        ui.end_row();
                        ui.label("Class scheme (.olsc)");
                        ui.horizontal(|ui| {
                            ui.label(self.scheme.as_ref().map_or("ASPRS (default)".into(), |p| {
                                p.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned())
                            }));
                            if ui.small_button("Choose…").clicked() {
                                self.scheme = rfd::FileDialog::new()
                                    .add_filter("Class scheme", &["olsc"])
                                    .pick_file();
                            }
                        });
                        ui.end_row();
                    }
                });
                ui.label(
                    egui::RichText::new(if new {
                        "A new point cloud of the project."
                    } else {
                        "The points are added to this point cloud (blocks already there are merged)."
                    })
                    .small()
                    .weak(),
                );
                if let Some(e) = &self.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, e);
                }
                if ui
                    .add_enabled(!self.files.is_empty() && !self.cloud.trim().is_empty(), egui::Button::new("Import"))
                    .clicked()
                {
                    let scheme = match &self.scheme {
                        Some(p) => match std::fs::read_to_string(p)
                            .map_err(|e| e.to_string())
                            .and_then(|t| ols_lidar::ClassScheme::parse(&t).map(|_| t))
                        {
                            Ok(t) => Some(t),
                            Err(e) => {
                                self.error = Some(format!("class scheme: {e}"));
                                return;
                            }
                        },
                        None => None,
                    };
                    let (files, name, crs, chunk) =
                        (self.files.clone(), self.cloud.trim().to_owned(), self.crs.clone(), self.chunk);
                    job = Some(Job::start(
                        format!("Importing {} files into \"{name}\"", files.len()),
                        true,
                        ctx,
                        move |report| import(&file, &files, &name, &crs, chunk, scheme.as_deref(), report),
                    ));
                }
            });
        job
    }
}

/// Imports `files` into point cloud `name` of project `file`.
///
/// Expected cost: ~1 min per 10⁸ points (LAZ decompression); exit clause
/// 2 h.
pub(crate) fn import(
    file: &Path,
    files: &[PathBuf],
    name: &str,
    crs: &str,
    chunk: f64,
    scheme: Option<&str>,
    report: &dyn Fn(String, f32),
) -> Result<String, String> {
    let started = std::time::Instant::now();
    let mut store = ols_pointcloud::Store::open(file).map_err(|e| e.to_string())?;
    let budget = ols_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(7200));
    let total = files.len();
    let (cloud, stats) = store
        .import_tiles(
            name,
            files,
            crs,
            scheme,
            chunk,
            ols_pointcloud::DEFAULT_SCALE,
            &budget,
            |k, tile, n| {
                report(
                    format!(
                        "{}/{total} {} ({n} points)",
                        k + 1,
                        tile.file_name()
                            .map_or(String::new(), |f| f.to_string_lossy().into_owned())
                    ),
                    (k + 1) as f32 / total as f32,
                );
            },
        )
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "{} points from {} files imported into \"{}\" in {:.0?}; it now has {} points",
        stats.points_read,
        stats.read,
        cloud.name,
        started.elapsed(),
        cloud.points
    ))
}

impl ExportDialog {
    /// The window; `Some(job)` once started.
    pub fn ui(&mut self, ctx: &egui::Context, model: &Model, open: &mut bool) -> Option<Job> {
        let mut job = None;
        egui::Window::new("Export LAS/LAZ")
            .open(open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                let Some((file, name)) = crate::chunks::cloud_of(model) else {
                    ui.colored_label(egui::Color32::LIGHT_RED, "no point cloud in this project");
                    return;
                };
                ui.radio_value(&mut self.only_shown, false, "every class");
                ui.radio_value(
                    &mut self.only_shown,
                    true,
                    format!(
                        "only the classes shown ({} hidden)",
                        model.hidden_classes.len()
                    ),
                );
                ui.separator();
                ui.radio_value(&mut self.around_line, false, "the whole survey");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.around_line, true, "only around the line, ");
                    ui.add_enabled(
                        self.around_line,
                        egui::DragValue::new(&mut self.margin)
                            .range(0.0..=10_000.0)
                            .suffix(" m"),
                    );
                    ui.label("beyond its structures");
                });
                ui.separator();
                ui.checkbox(&mut self.scheme, "the class scheme next to it (.olsc)");
                ui.label(
                    egui::RichText::new(
                        "LAS 1.4, point format 6 (classes 0–255); .laz compressed. Coordinates on \
                         the survey's scale, classes, intensities and returns as imported.",
                    )
                    .small()
                    .weak(),
                );
                if let Some(e) = &self.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, e);
                }
                if ui.button("Choose where and export…").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("LAZ (compressed)", &["laz"])
                        .add_filter("LAS", &["las"])
                        .set_file_name(format!(
                            "{}.laz",
                            name.clone().unwrap_or_else(|| "survey".into())
                        ))
                        .save_file()
                {
                    let classes: Option<Vec<u8>> = self.only_shown.then(|| {
                        (0..=255u8)
                            .filter(|c| !model.hidden_classes.contains(c))
                            .collect()
                    });
                    let area = self
                        .around_line
                        .then(|| line_area(model, self.margin))
                        .flatten();
                    if self.around_line && area.is_none() {
                        self.error = Some("the line has no structures".into());
                        return;
                    }
                    let scheme = self.scheme;
                    job = Some(Job::start(
                        format!(
                            "Exporting to {}",
                            path.file_name()
                                .map_or(String::new(), |n| n.to_string_lossy().into_owned())
                        ),
                        false,
                        ctx,
                        move |report| {
                            export(
                                &file,
                                name.as_deref(),
                                &path,
                                classes.as_deref(),
                                area,
                                scheme,
                                report,
                            )
                        },
                    ));
                }
            });
        job
    }
}

/// The plan area of the line's structures grown by `margin`, m.
fn line_area(model: &Model, margin: f64) -> Option<([f64; 2], [f64; 2])> {
    let b = ols_core::grid::Bounds::of_points(model.line.structures.iter().map(|s| &s.position))?;
    let g = b.grown(margin);
    Some((g.min, g.max))
}

/// Writes point cloud `name` of project `file` to `path` (and its class
/// scheme next to it when asked).
///
/// Expected cost: ~1 µs per point for LAZ; exit clause 2 h.
pub(crate) fn export(
    file: &Path,
    name: Option<&str>,
    path: &Path,
    classes: Option<&[u8]>,
    area: Option<([f64; 2], [f64; 2])>,
    scheme: bool,
    report: &dyn Fn(String, f32),
) -> Result<String, String> {
    let started = std::time::Instant::now();
    let store = ols_pointcloud::Store::open(file).map_err(|e| e.to_string())?;
    let cloud = store.cloud(name).map_err(|e| e.to_string())?;
    let budget = ols_core::budget::Budget::new().time_limit(std::time::Duration::from_secs(7200));
    let stats = store
        .export_las(&cloud, path, classes, area, &budget, |done, total| {
            report(
                format!("{done} of {total} points"),
                if total > 0 {
                    done as f32 / total as f32
                } else {
                    1.0
                },
            );
        })
        .map_err(|e| e.to_string())?;
    let mut extra = String::new();
    if scheme && let Some(text) = &cloud.class_scheme {
        let olsc = path.with_extension("olsc");
        std::fs::write(&olsc, text).map_err(|e| format!("{}: {e}", olsc.display()))?;
        extra = format!(" and its class scheme {}", olsc.display());
    }
    Ok(format!(
        "{} points written to {}{extra} in {:.0?}",
        stats.points_visited,
        path.display(),
        started.elapsed()
    ))
}

/// The job's window: progress, then its report (Close).
pub fn job_window(ctx: &egui::Context, job: &mut Job) -> bool {
    job.poll();
    let mut close = false;
    egui::Window::new(&job.title)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| match &job.done {
            None => {
                ui.add(
                    egui::ProgressBar::new(job.progress.1)
                        .show_percentage()
                        .animate(true),
                );
                ui.label(egui::RichText::new(&job.progress.0).small());
                ui.label(
                    egui::RichText::new("Runs in the background: the views stay usable.")
                        .small()
                        .weak(),
                );
            }
            Some(Ok(report)) => {
                ui.label(report);
                if job.reopen {
                    ui.label("The project is opened again to show the points.");
                }
                close = ui.button("Close").clicked();
            }
            Some(Err(e)) => {
                ui.colored_label(egui::Color32::LIGHT_RED, e);
                close = ui.button("Close").clicked();
            }
        });
    close
}
