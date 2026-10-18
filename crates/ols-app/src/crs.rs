// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Terrain › Coordinate system: say which system the project's coordinates
//! are in (Define), convert points between systems (Calculator), move the
//! whole project into another system (Reproject, into a new project file).

use std::path::{Path, PathBuf};

use eframe::egui;
use ols_crs::{Crs, Heights, Transformation};

use crate::model::Model;

/// How a coordinate system is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Category {
    /// From the EPSG list.
    #[default]
    Epsg,
    /// A proj4 string.
    Proj4,
    /// A `.prj` file (WKT or proj4).
    Prj,
}

impl Category {
    const ALL: [Category; 3] = [Category::Epsg, Category::Proj4, Category::Prj];

    fn name(self) -> &'static str {
        match self {
            Category::Epsg => "EPSG",
            Category::Proj4 => "proj4 string",
            Category::Prj => "PRJ file (WKT or proj4)",
        }
    }
}

/// Chooses a coordinate system: from the EPSG list (filtered), as a proj4
/// string or from a `.prj` file. `spec` is what [`Crs::parse`] takes.
#[derive(Debug, Clone, Default)]
pub struct Picker {
    /// How it is given.
    pub category: Category,
    /// Show only EPSG systems whose code or name contains this.
    pub filter: String,
    /// The proj4 string typed.
    pub proj4: String,
    /// The system chosen.
    pub spec: String,
    /// Why the last PRJ file could not be read.
    prj_error: Option<String>,
}

impl Picker {
    /// Starting at `spec` (the EPSG list for an EPSG code or nothing, else
    /// the proj4 string).
    pub fn new(spec: &str) -> Self {
        let spec = spec.trim();
        let proj4 = spec.starts_with('+');
        Self {
            category: if proj4 {
                Category::Proj4
            } else {
                Category::Epsg
            },
            proj4: if proj4 {
                spec.to_owned()
            } else {
                String::new()
            },
            spec: if Crs::parse(spec).is_ok() {
                spec.to_owned()
            } else {
                String::new()
            },
            ..Self::default()
        }
    }

    /// The system chosen, if valid.
    pub fn crs(&self) -> Result<Crs, String> {
        if self.spec.is_empty() {
            return Err("choose a coordinate system".into());
        }
        Crs::parse(&self.spec)
    }

    /// The chooser. `id` keeps two of them apart in one window.
    pub fn ui(&mut self, ui: &mut egui::Ui, id: &str) {
        ui.label("Coordinate system category");
        egui::ComboBox::from_id_salt(format!("{id}-category"))
            .selected_text(self.category.name())
            .width(260.0)
            .show_ui(ui, |ui| {
                for c in Category::ALL {
                    ui.selectable_value(&mut self.category, c, c.name());
                }
            });
        match self.category {
            Category::Epsg => {
                ui.horizontal(|ui| {
                    ui.label("Show only systems containing");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.filter)
                            .id_salt(format!("{id}-filter"))
                            .hint_text("e.g. LV95, UTM 32N, 2056")
                            .desired_width(160.0),
                    );
                });
                let wanted = self.filter.trim().to_lowercase();
                let rows: Vec<&ols_crs::Known> = ols_crs::known()
                    .iter()
                    .filter(|k| {
                        wanted.is_empty()
                            || k.name.to_lowercase().contains(&wanted)
                            || k.code.to_string().contains(&wanted)
                    })
                    .collect();
                let chosen = self.spec.to_ascii_uppercase();
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt(format!("{id}-list"))
                        .min_scrolled_height(180.0)
                        .max_height(180.0)
                        .auto_shrink([false, false])
                        .show_rows(ui, 18.0, rows.len(), |ui, range| {
                            for k in &rows[range] {
                                let spec = k.spec();
                                let text = format!(
                                    "{k}{}",
                                    if k.geographic {
                                        "  (longitude, latitude)"
                                    } else {
                                        ""
                                    }
                                );
                                if ui.selectable_label(chosen == spec, text).clicked() {
                                    self.spec = spec;
                                }
                            }
                        });
                });
                ui.label(
                    egui::RichText::new(format!("{} systems listed", rows.len()))
                        .small()
                        .weak(),
                );
            }
            Category::Proj4 => {
                let r = ui.add(
                    egui::TextEdit::multiline(&mut self.proj4)
                        .id_salt(format!("{id}-proj4"))
                        .hint_text(
                            "+proj=tmerc +lat_0=… +lon_0=… +k=… +x_0=… +y_0=… +ellps=… +units=m",
                        )
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
                if r.changed() {
                    self.spec = self.proj4.trim().to_owned();
                }
            }
            Category::Prj => {
                if ui
                    .button("Select PRJ file…")
                    .on_hover_text(
                        "WKT naming its EPSG code or the name of an EPSG system, or a proj4 \
                         string",
                    )
                    .clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("Projection", &["prj", "wkt", "txt"])
                        .pick_file()
                {
                    self.read_prj(&path);
                }
                if let Some(e) = &self.prj_error {
                    crate::menu::error_label(ui, e);
                }
            }
        }
        let shown = if self.spec.is_empty() {
            egui::RichText::new("Chosen: none").weak()
        } else {
            match Crs::parse(&self.spec) {
                Ok(c) => egui::RichText::new(format!(
                    "Chosen: {}{}",
                    ols_crs::describe(&self.spec),
                    if c.is_geographic() {
                        " — geographic (degrees)"
                    } else {
                        ""
                    }
                ))
                .strong(),
                Err(e) => egui::RichText::new(e).color(ui.visuals().error_fg_color),
            }
        };
        ui.label(shown);
    }

    /// Takes the system of a `.prj` file.
    pub fn read_prj(&mut self, path: &Path) {
        match std::fs::read_to_string(path)
            .map_err(|e| format!("{}: {e}", path.display()))
            .and_then(|t| ols_crs::from_prj(&t))
        {
            Ok(spec) => {
                self.spec = spec;
                self.prj_error = None;
            }
            Err(e) => self.prj_error = Some(e),
        }
    }
}

/// Terrain › Coordinate system › Define: the system the project's
/// coordinates are in (nothing moves).
#[derive(Debug, Clone, Default)]
pub struct Define {
    /// The chooser.
    pub picker: Picker,
}

/// Terrain › Coordinate system › Calculator: points from one system to
/// another.
#[derive(Debug, Clone)]
pub struct Calculator {
    /// From.
    pub from: Picker,
    /// To.
    pub to: Picker,
    /// Heights through the datum shift (ellipsoidal) or kept.
    pub ellipsoidal: bool,
    /// Points typed: one per line, `X Y [Z]`.
    pub input: String,
    /// The converted points (or why not).
    pub output: String,
}

impl Calculator {
    /// From the project's system to WGS84 longitude, latitude.
    pub fn new(model: &Model) -> Self {
        Self {
            from: Picker::new(&model.line.crs),
            to: Picker::new("EPSG:4326"),
            ellipsoidal: false,
            input: String::new(),
            output: String::new(),
        }
    }

    /// Converts the points typed.
    pub fn compute(&mut self) {
        self.output = match convert(
            &self.from.spec,
            &self.to.spec,
            self.ellipsoidal,
            &self.input,
        ) {
            Ok(text) => text,
            Err(e) => format!("— {e}"),
        };
    }
}

/// Converts points given one per line (`X Y [Z]`, separated by spaces,
/// commas, semicolons or tabs; empty lines and lines starting with `#`
/// kept as they are) from `from` to `to`. Geographic output in degrees
/// with 9 decimals (0.1 mm), projected with 4.
pub fn convert(from: &str, to: &str, ellipsoidal: bool, input: &str) -> Result<String, String> {
    let (a, b) = (Crs::parse(from)?, Crs::parse(to)?);
    let t = Transformation::new(
        &a,
        &b,
        if ellipsoidal {
            Heights::Ellipsoidal
        } else {
            Heights::Keep
        },
    );
    let digits = if b.is_geographic() { 9 } else { 4 };
    let mut out = Vec::new();
    for (n, line) in input.lines().enumerate() {
        let text = line.trim();
        if text.is_empty() || text.starts_with('#') {
            out.push(line.to_owned());
            continue;
        }
        let values: Vec<f64> = text
            .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
            .filter(|s| !s.is_empty())
            .map(str::parse)
            .collect::<Result<_, _>>()
            .map_err(|_| format!("line {}: \"{text}\" is not X Y [Z]", n + 1))?;
        if !(values.len() == 2 || values.len() == 3) {
            return Err(format!("line {}: give X Y or X Y Z", n + 1));
        }
        let p = t
            .point([values[0], values[1], values.get(2).copied().unwrap_or(0.0)])
            .map_err(|e| format!("line {}: {e}", n + 1))?;
        out.push(if values.len() == 3 {
            format!("{:.digits$} {:.digits$} {:.4}", p[0], p[1], p[2])
        } else {
            format!("{:.digits$} {:.digits$}", p[0], p[1])
        });
    }
    Ok(out.join("\n"))
}

/// Terrain › Coordinate system › Reproject: the project moved into another
/// system, written as a new project file.
#[derive(Debug, Clone)]
pub struct Reproject {
    /// The target.
    pub to: Picker,
    /// The new project file.
    pub path: String,
    /// Why it cannot start.
    pub error: Option<String>,
}

impl Reproject {
    /// Into the system chosen, as a file next to the project's.
    pub fn new() -> Self {
        Self {
            to: Picker::default(),
            path: String::new(),
            error: None,
        }
    }
}

/// The new project's file: `<name> <EPSG code>.ohl` next to `file`.
pub fn default_path(file: &Path, spec: &str) -> PathBuf {
    let stem = file
        .file_stem()
        .map_or_else(|| "project".into(), |s| s.to_string_lossy().into_owned());
    let tag: String = spec
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .take(4)
        .collect::<Vec<_>>()
        .join(" ");
    file.with_file_name(format!("{stem} {tag}.ohl"))
}

/// The coordinate system windows and the reprojection running.
#[derive(Debug, Default)]
pub struct Windows {
    /// Define, when open.
    pub define: Option<Define>,
    /// Calculator, when open.
    pub calculator: Option<Calculator>,
    /// Reproject, when open.
    pub reproject: Option<Reproject>,
    /// The reprojection running, and the new file.
    pub job: Option<(crate::survey_io::Job, PathBuf)>,
}

/// What the windows ask of the application.
pub enum Request {
    /// Define: set the line's coordinate system (an edit).
    Define(String),
    /// Open this project (the reprojected one).
    Open(PathBuf),
}

impl Windows {
    /// The open windows; returns what the application does next.
    pub fn show(&mut self, ctx: &egui::Context, model: &Model) -> Option<Request> {
        let mut request = None;
        if let Some(d) = &mut self.define {
            let mut open = true;
            let (mut ok, mut cancel) = (false, false);
            egui::Window::new("Coordinate system: define")
                .open(&mut open)
                .default_width(560.0)
                .show(ctx, |ui| {
                    ui.label(
                        "The coordinate system the project's coordinates are in. Setting it does \
                         not move the project; it is needed for any cartographic calculation \
                         (imports and exports in other systems, longitude and latitude, \
                         reprojecting).",
                    );
                    ui.label(format!("Now: {}", ols_crs::describe(&model.line.crs)));
                    ui.separator();
                    d.picker.ui(ui, "define");
                    let valid = d.picker.crs();
                    if let Ok(c) = &valid
                        && c.is_geographic()
                    {
                        ui.colored_label(
                            ui.visuals().error_fg_color,
                            "Lines are designed in a projected system in metres: choose a \
                             projected system",
                        );
                    }
                    if let Err(why) = model.can_edit() {
                        crate::menu::error_label(ui, format!("read-only: {why}"));
                    }
                    ui.horizontal(|ui| {
                        ok = ui
                            .add_enabled(
                                model.can_edit().is_ok() && valid.is_ok_and(|c| !c.is_geographic()),
                                egui::Button::new("OK"),
                            )
                            .clicked();
                        cancel = ui.button("Cancel").clicked();
                    });
                });
            if ok {
                request = Some(Request::Define(d.picker.spec.trim().to_owned()));
            }
            if !open || ok || cancel {
                self.define = None;
            }
        }
        if let Some(c) = &mut self.calculator {
            let mut open = true;
            egui::Window::new("Coordinate system calculator")
                .open(&mut open)
                .default_width(760.0)
                .show(ctx, |ui| {
                    let before = (
                        c.from.spec.clone(),
                        c.to.spec.clone(),
                        c.ellipsoidal,
                        c.input.clone(),
                    );
                    ui.columns(2, |cols| {
                        cols[0].strong("From");
                        c.from.ui(&mut cols[0], "calc-from");
                        cols[1].strong("To");
                        c.to.ui(&mut cols[1], "calc-to");
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Swap").clicked() {
                            std::mem::swap(&mut c.from, &mut c.to);
                            // The results become the points.
                            if !c.output.starts_with('—') {
                                c.input = std::mem::take(&mut c.output);
                            }
                        }
                        ui.checkbox(&mut c.ellipsoidal, "Heights are ellipsoidal")
                            .on_hover_text(
                                "Ticked: heights go through the datum shift (GNSS heights). \
                                 Unticked: heights are kept as they are (heights above the \
                                 geoid, as on lines and surveys)",
                            );
                    });
                    ui.label(
                        egui::RichText::new(
                            "One point per line: X Y or X Y Z (east, north; longitude, latitude \
                             in degrees), separated by spaces, commas or tabs",
                        )
                        .small(),
                    );
                    ui.columns(2, |cols| {
                        cols[0].add(
                            egui::TextEdit::multiline(&mut c.input)
                                .id_salt("calc-input")
                                .code_editor()
                                .desired_rows(8)
                                .desired_width(f32::INFINITY),
                        );
                        let mut out = c.output.as_str();
                        cols[1].add(
                            egui::TextEdit::multiline(&mut out)
                                .id_salt("calc-output")
                                .code_editor()
                                .desired_rows(8)
                                .desired_width(f32::INFINITY),
                        );
                        if cols[1].button("Copy").clicked() {
                            cols[1].ctx().copy_text(c.output.clone());
                        }
                    });
                    if before
                        != (
                            c.from.spec.clone(),
                            c.to.spec.clone(),
                            c.ellipsoidal,
                            c.input.clone(),
                        )
                    {
                        c.compute();
                    }
                });
            if !open {
                self.calculator = None;
            }
        }
        if let Some(rp) = &mut self.reproject {
            let mut open = true;
            let mut start = None;
            let mut cancel = false;
            egui::Window::new("Reproject project")
                .open(&mut open)
                .default_width(560.0)
                .show(ctx, |ui| {
                    let from = Crs::parse(&model.line.crs);
                    ui.label(
                        "Moves the whole project into another coordinate system — the working \
                         line, the project lines saved, the terrain surfaces, the point clouds \
                         and the DXF drawings — and writes it as a new project file; this one \
                         is not changed. Orthophotos are left out: import them again in the new \
                         system.",
                    );
                    ui.label(format!("From: {}", ols_crs::describe(&model.line.crs)));
                    if from.is_err() {
                        ui.colored_label(
                            ui.visuals().error_fg_color,
                            "The project's coordinate system is not defined: Terrain › \
                             Coordinate system › Define first",
                        );
                    }
                    ui.separator();
                    ui.strong("To");
                    let before = rp.to.spec.clone();
                    rp.to.ui(ui, "reproject");
                    if rp.to.spec != before && !rp.to.spec.is_empty() {
                        rp.path = default_path(&model.sources.file, &rp.to.spec)
                            .display()
                            .to_string();
                    }
                    ui.horizontal(|ui| {
                        ui.label("New project file");
                        ui.add(
                            egui::TextEdit::singleline(&mut rp.path)
                                .id_salt("reproject-path")
                                .desired_width(320.0),
                        );
                        if ui.button("Choose…").clicked()
                            && let Some(p) = rfd::FileDialog::new()
                                .add_filter(
                                    "OpenOHL project",
                                    &[ols_core::files::PROJECT_EXTENSION],
                                )
                                .set_file_name(
                                    Path::new(&rp.path)
                                        .file_name()
                                        .map_or_else(String::new, |n| {
                                            n.to_string_lossy().into_owned()
                                        }),
                                )
                                .save_file()
                        {
                            rp.path = p
                                .with_extension(ols_core::files::PROJECT_EXTENSION)
                                .display()
                                .to_string();
                        }
                    });
                    ui.label(
                        egui::RichText::new(
                            "Heights are kept (heights above the geoid). Grid distances change \
                             with the projection's scale: check spans and stringing after. The \
                             history of versions stays in the old system.",
                        )
                        .small()
                        .weak(),
                    );
                    if let Some(e) = &rp.error {
                        crate::menu::error_label(ui, e);
                    }
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                from.is_ok() && !rp.to.spec.is_empty() && self.job.is_none(),
                                egui::Button::new("Reproject into the new project"),
                            )
                            .clicked()
                        {
                            start = Some(());
                        }
                        cancel = ui.button("Cancel").clicked();
                    });
                });
            if start.is_some() {
                match check(model, &rp.to.spec, Path::new(rp.path.trim())) {
                    Ok((from, to, path)) => {
                        let job = start_job(ctx, model, from, to, path.clone());
                        self.job = Some((job, path));
                        self.reproject = None;
                    }
                    Err(e) => rp.error = Some(e),
                }
            } else if !open || cancel {
                self.reproject = None;
            }
        }
        if let Some((job, path)) = &mut self.job
            && crate::survey_io::job_window(ctx, job)
        {
            if matches!(job.done, Some(Ok(_))) {
                request = Some(Request::Open(path.clone()));
            }
            self.job = None;
        }
        request
    }
}

/// What a reprojection needs: the two systems and a new file, or why not.
fn check(model: &Model, to: &str, path: &Path) -> Result<(Crs, Crs, PathBuf), String> {
    let e = model.editing.as_ref().map_err(Clone::clone)?;
    e.session
        .require(&e.user, ols_session::Role::Admin)
        .map_err(|_| "only an admin of the project can reproject it".to_owned())?;
    let file = &model.sources.file;
    if !ols_core::files::is_project_file(file) {
        return Err("a project (.ohl) is reprojected; a line file with ohl crs line".into());
    }
    let from = Crs::parse(&model.line.crs)
        .map_err(|e| format!("the project's coordinate system: {e} — define it first"))?;
    let to = Crs::parse(to)?;
    if to.is_geographic() {
        return Err(
            "lines are designed in a projected coordinate system in metres: choose a projected \
             system"
                .into(),
        );
    }
    if from == to {
        return Err("the project is in that system already".into());
    }
    if path.as_os_str().is_empty() {
        return Err("give the new project file".into());
    }
    let path = path.with_extension(ols_core::files::PROJECT_EXTENSION);
    if path.exists() {
        return Err(format!(
            "{} exists already: choose another name (a project is never replaced)",
            path.display()
        ));
    }
    Ok((from, to, path))
}

/// Copies the project to `path` and moves the copy, in the background.
fn start_job(
    ctx: &egui::Context,
    model: &Model,
    from: Crs,
    to: Crs,
    path: PathBuf,
) -> crate::survey_io::Job {
    let source = model.sources.file.clone();
    let user = model
        .editing
        .as_ref()
        .map(|e| e.user.clone())
        .unwrap_or_default();
    let title = format!("Reproject into {}", ols_crs::describe(to.name()));
    crate::survey_io::Job::start(title, false, ctx, move |report| {
        let result = reproject_file(&source, &path, &from, &to, &user, report);
        if result.is_err() {
            for end in ["", "-wal", "-shm"] {
                let mut p = path.clone().into_os_string();
                p.push(end);
                let _ = std::fs::remove_file(p);
            }
        }
        result
    })
}

/// Writes project `source` moved from `from` into `to` as project `path`
/// (a new file): every project line and the surfaces
/// ([`ols_session::Session::reproject`]), then each point cloud
/// ([`ols_pointcloud::Store::reproject_cloud`]). Heights kept.
///
/// Cost: copying the file (about as long as reading it) and ~1 µs per
/// LiDAR point. Exit clause: the point clouds' budget (the default, very
/// long).
pub fn reproject_file(
    source: &Path,
    path: &Path,
    from: &Crs,
    to: &Crs,
    user: &str,
    report: &dyn Fn(String, f32),
) -> Result<String, String> {
    let t = Transformation::new(from, to, Heights::Keep);
    let moved = |p: &mut [[f64; 3]]| t.points(p);
    report("copying the project…".into(), 0.0);
    ols_session::Session::open(source)
        .and_then(|s| s.copy_to(path))
        .map_err(|e| e.to_string())?;
    report("moving the project lines and the surfaces…".into(), 0.05);
    let mut session = ols_session::Session::open(path).map_err(|e| e.to_string())?;
    let lines = session
        .reproject(user, to.name(), &moved)
        .map_err(|e| e.to_string())?;
    let surfaces = session.surfaces().map_err(|e| e.to_string())?.len();
    session.checkpoint().map_err(|e| e.to_string())?;
    drop(session);
    let mut store = ols_pointcloud::Store::open(path).map_err(|e| e.to_string())?;
    let clouds = store.clouds().map_err(|e| e.to_string())?;
    let budget = ols_core::budget::Budget::new();
    let mut points = 0;
    for (k, cloud) in clouds.iter().enumerate() {
        let share = 0.9 / clouds.len() as f32;
        let name = cloud.name.clone();
        store
            .reproject_cloud(cloud, to.name(), &moved, &budget, |done, total| {
                report(
                    format!("point cloud {name}: {done} of {total} points"),
                    0.1 + share * (k as f32 + done as f32 / total.max(1) as f32),
                );
            })
            .map_err(|e| e.to_string())?;
        points += cloud.points;
    }
    drop(store);
    report("moving the drawings…".into(), 0.98);
    let images = ols_background::Store::open(path)
        .and_then(|s| s.reproject(to.name(), &moved))
        .map_err(|e| e.to_string())?;
    ols_session::Session::open(path)
        .and_then(|s| s.checkpoint())
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "{} ➡ {}: {lines} project line(s), {surfaces} surface(s) and {} point \
         cloud(s) ({points} points) moved into {}. It opens when you close this window. Check \
         spans and stringing: grid distances change with the projection's scale.",
        ols_crs::describe(from.name()),
        ols_crs::describe(to.name()),
        clouds.len(),
        path.display()
    ) + &if images > 0 {
        format!(
            " The {images} orthophoto(s) are not in the new project: import them again in the \
             new system."
        )
    } else {
        String::new()
    })
}

/// [`check`] for the tests.
#[cfg(test)]
pub fn reproject_check_for_tests(model: &Model, to: &str, path: &Path) -> Result<(), String> {
    check(model, to, path).map(|_| ())
}
