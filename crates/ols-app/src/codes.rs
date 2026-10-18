// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Criteria › Design code: a design-code plugin (Plugins › Plugin manager)
//! turns the user's parameter file into weather cases and load cases,
//! kept in the project's libraries (or next to a line file) and used for
//! the model. The plugin runs on a worker thread; its own exit clauses
//! (ADR-0003) stop it.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use ols_io::codes::Generated;

use crate::app::Ready;
use crate::library::{LibraryKind, PageAction};

/// A design-code plugin that can be chosen.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    /// Its name.
    pub name: String,
    /// Its directory.
    pub dir: PathBuf,
    /// One line from its manifest.
    pub description: String,
    /// Why it cannot be used (unusable or turned off), if so.
    pub problem: Option<String>,
}

/// The design-code plugins of the plugin manager (installed and folders
/// added), those that cannot be used with why.
pub fn choices(plugins: &crate::plugins::Plugins) -> Vec<Choice> {
    plugins
        .listed
        .iter()
        .filter(|l| l.found.provides(ols_plugins::DESIGN_CODE))
        .map(|l| {
            let c = ols_io::codes::CodePlugin::from(&l.found);
            let problem = l.problem().or_else(|| {
                (!plugins.enabled(l)).then(|| "turned off in the plugin manager".to_owned())
            });
            Choice {
                name: c.name,
                dir: c.dir,
                description: c.description,
                problem,
            }
        })
        .collect()
}

enum Reply {
    Generated(Result<Box<Generated>, String>),
    Template(Result<PathBuf, String>),
}

/// The Criteria › Design code window.
#[derive(Default)]
pub struct DesignCodeWindow {
    /// The window is open.
    pub open: bool,
    /// The plugin chosen (its directory).
    pub plugin: Option<PathBuf>,
    /// The parameter file.
    pub params: Option<PathBuf>,
    /// Name of both files written (without extension; default the
    /// plugin's name).
    pub file_name: String,
    /// The last thing done, error or not.
    pub status: Option<(String, bool)>,
    /// The plugin's notes on the last generation.
    pub notes: Vec<String>,
    job: Option<Receiver<Reply>>,
}

impl DesignCodeWindow {
    /// A plugin call is running.
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// Runs the chosen plugin on the parameter file (on a worker thread).
    pub fn generate(&mut self) {
        let (Some(dir), Some(params)) = (self.plugin.clone(), self.params.clone()) else {
            return;
        };
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let result = std::fs::read_to_string(&params)
                .map_err(|e| format!("{}: {e}", params.display()))
                .and_then(|text| {
                    let host = ols_plugins::WasmHost::new().map_err(|e| e.to_string())?;
                    ols_io::codes::generate(&host, &dir, &text).map(Box::new)
                });
            let _ = tx.send(Reply::Generated(result));
        });
        self.job = Some(rx);
        self.status = Some(("generating…".into(), false));
        self.notes.clear();
    }

    /// Writes the chosen plugin's parameter template to `to` (on a worker
    /// thread).
    pub fn save_template(&mut self, to: PathBuf) {
        let Some(dir) = self.plugin.clone() else {
            return;
        };
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let result = ols_plugins::WasmHost::new()
                .map_err(|e| e.to_string())
                .and_then(|host| ols_io::codes::template(&host, &dir))
                .and_then(|text| {
                    std::fs::write(&to, text).map_err(|e| format!("{}: {e}", to.display()))
                })
                .map(|()| to);
            let _ = tx.send(Reply::Template(result));
        });
        self.job = Some(rx);
    }

    /// The worker's answer, if it came (waits up to `wait`).
    fn poll(&mut self, wait: std::time::Duration) -> Option<Reply> {
        let reply = self.job.as_ref()?.recv_timeout(wait).ok()?;
        self.job = None;
        Some(reply)
    }

    /// The name of the files, checked.
    fn base_name(&self) -> Result<String, String> {
        let n = self.file_name.trim();
        let ok = !n.is_empty()
            && !n.starts_with('.')
            && n.chars()
                .all(|c| c.is_alphanumeric() || " -_.()".contains(c));
        if ok {
            Ok(n.to_owned())
        } else {
            Err(format!(
                "file name \"{n}\": letters, digits, space - _ . ( )"
            ))
        }
    }
}

/// Where the files go: the project's libraries (working copies, kept in
/// the project when stored), else next to the line file.
pub fn targets(r: &Ready, base: &str) -> (PathBuf, PathBuf) {
    let (weather, loads) = match (&r.model.library_dir, &r.model.editing) {
        (Some(work), Ok(_)) => (work.join("weather"), work.join("loadcases")),
        _ => {
            let dir = r
                .model
                .sources
                .file
                .parent()
                .map_or_else(PathBuf::new, Path::to_path_buf);
            (dir.clone(), dir)
        }
    };
    (
        weather.join(format!("{base}.ohlwc")),
        loads.join(format!("{base}.ohllc")),
    )
}

/// Writes generated cases as `base`.ohlwc / .ohllc (see [`targets`]),
/// and the clearance presets as `<plugin>-<name>`.ohlr / .ohlwc (the
/// project's rule files and weather case files, else next to the line
/// file); keeps them in the project and uses the cases for the model (the
/// libraries read again, the cases solved). Returns what was done.
pub fn apply(r: &mut Ready, g: &Generated, base: &str) -> Result<String, String> {
    let (weather, loads) = targets(r, base);
    g.write(&weather, &loads)?;
    let weather_dir = weather
        .parent()
        .map_or_else(PathBuf::new, Path::to_path_buf);
    let rules_dir = match (&r.model.library_dir, &r.model.editing) {
        (Some(work), Ok(_)) => work.join("rules"),
        _ => weather_dir.clone(),
    };
    let presets = g.write_presets(&rules_dir, &weather_dir)?;
    r.model.sources.load_cases = Some(loads);
    r.library_saved(
        LibraryKind::WeatherCases,
        PageAction::UseForModel(weather.clone()),
    );
    if let Some((e, true)) = &r.status {
        return Err(e.clone());
    }
    // Structure loads computed before used the old cases.
    r.loads.results = None;
    let mut done = format!(
        "{}: {} weather cases and {} load cases in {} — used for the model",
        g.plugin,
        g.weather.cases.len(),
        g.load_cases.cases.len(),
        weather
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned())
    );
    if !presets.is_empty() {
        let names: Vec<String> = presets
            .iter()
            .map(|(p, _)| {
                p.file_name()
                    .map_or(String::new(), |n| n.to_string_lossy().into_owned())
            })
            .collect();
        done.push_str(&format!(
            "; clearance presets {} (Vertical / Radial clearances… › Load preset…)",
            names.join(", ")
        ));
    }
    Ok(done)
}

/// The window, when open.
pub fn window(ctx: &egui::Context, r: &mut Ready, plugins: &crate::plugins::Plugins) {
    if !r.design_code.open {
        return;
    }
    match r.design_code.poll(std::time::Duration::ZERO) {
        Some(Reply::Generated(Ok(g))) => {
            let w = &mut r.design_code;
            let base = w.base_name();
            w.notes = g.notes.clone();
            let result = base.and_then(|b| apply(r, &g, &b));
            r.design_code.status = Some(match result {
                Ok(m) => (m, false),
                Err(e) => (e, true),
            });
        }
        Some(Reply::Generated(Err(e))) => r.design_code.status = Some((e, true)),
        Some(Reply::Template(r2)) => {
            r.design_code.status = Some(match r2 {
                Ok(p) => (format!("template written to {}", p.display()), false),
                Err(e) => (e, true),
            });
        }
        None => {}
    }
    if r.design_code.busy() {
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }
    let list = choices(plugins);
    let in_project = r.model.library_dir.is_some() && r.model.editing.is_ok();
    let w = &mut r.design_code;
    let mut open = true;
    egui::Window::new("Design code")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(560.0)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(
                    "Weather cases and load cases from a design-code plugin and your parameter \
                     file. OpenOHL ships no code tables: every value of the code comes from your \
                     file — start from the plugin's template and replace its placeholders.",
                )
                .small()
                .weak(),
            );
            ui.add_space(6.0);
            if list.is_empty() {
                ui.label(
                    "No design-code plugin: install one or add its folder (Plugins › Plugin \
                     manager), e.g. from plugins/codes.",
                );
            }
            let chosen = list.iter().find(|c| Some(&c.dir) == w.plugin.as_ref());
            egui::ComboBox::from_id_salt("design-code-plugin")
                .selected_text(chosen.map_or("— choose a code —", |c| c.name.as_str()))
                .width(400.0)
                .show_ui(ui, |ui| {
                    for c in &list {
                        let r = ui
                            .add_enabled(
                                c.problem.is_none(),
                                egui::Button::selectable(
                                    w.plugin.as_ref() == Some(&c.dir),
                                    &c.name,
                                ),
                            )
                            .on_hover_text(&c.description)
                            .on_disabled_hover_text(c.problem.clone().unwrap_or_default());
                        if r.clicked() {
                            w.plugin = Some(c.dir.clone());
                            if w.file_name.trim().is_empty() {
                                w.file_name = c.name.clone();
                            }
                        }
                    }
                });
            if let Some(c) = chosen {
                ui.label(egui::RichText::new(&c.description).small());
            }
            ui.horizontal(|ui| {
                if ui.button("Parameter file…").clicked()
                    && let Some(p) = rfd::FileDialog::new()
                        .add_filter("parameter file", &["toml"])
                        .pick_file()
                {
                    w.params = Some(p);
                }
                ui.label(
                    w.params
                        .as_ref()
                        .map_or("none chosen".into(), |p| p.display().to_string()),
                );
            });
            if ui
                .add_enabled(
                    w.plugin.is_some() && !w.busy(),
                    egui::Button::new("Save the plugin's template…"),
                )
                .on_hover_text(
                    "A parameter file to start from: its schema in comments, every value an \
                     invented placeholder",
                )
                .clicked()
                && let Some(to) = rfd::FileDialog::new()
                    .set_file_name(format!("{}-parameters.toml", w.file_name.trim()))
                    .save_file()
            {
                w.save_template(to);
            }
            ui.horizontal(|ui| {
                ui.label("Files");
                ui.add(egui::TextEdit::singleline(&mut w.file_name).desired_width(160.0));
                ui.label(
                    egui::RichText::new(if in_project {
                        ".ohlwc / .ohllc, in the project's libraries (a file of that name is replaced)"
                    } else {
                        ".ohlwc / .ohllc, next to the line file (a file of that name is replaced)"
                    })
                    .small()
                    .weak(),
                );
            });
            ui.separator();
            ui.horizontal(|ui| {
                let ready = w.plugin.is_some() && w.params.is_some() && !w.busy();
                if ui
                    .add_enabled(ready, egui::Button::new("Generate"))
                    .on_hover_text(
                        "Run the code on the parameter file; the cases are used for the model",
                    )
                    .clicked()
                {
                    w.generate();
                }
                if w.busy() {
                    ui.spinner();
                }
            });
            if let Some((m, error)) = &w.status {
                if *error {
                    crate::menu::error_label(ui, m);
                } else {
                    ui.label(m);
                }
            }
            if !w.notes.is_empty() {
                ui.label(egui::RichText::new("Notes of the code").strong());
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for n in &w.notes {
                            ui.label(format!("• {n}"));
                        }
                    });
            }
        });
    if !open {
        r.design_code.open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A generation's reply is taken once; a window without a job is
    /// not busy.
    #[test]
    fn replies_are_taken_once() {
        let mut w = DesignCodeWindow::default();
        assert!(!w.busy());
        let (tx, rx) = channel();
        w.job = Some(rx);
        tx.send(Reply::Generated(Err("x".into()))).unwrap();
        assert!(matches!(
            w.poll(std::time::Duration::from_secs(1)),
            Some(Reply::Generated(Err(_)))
        ));
        assert!(!w.busy());
        assert!(w.poll(std::time::Duration::ZERO).is_none());
    }

    #[test]
    fn file_names_are_checked() {
        let mut w = DesignCodeWindow {
            file_name: "EN 50341 (CH)".into(),
            ..Default::default()
        };
        assert_eq!(w.base_name().unwrap(), "EN 50341 (CH)");
        for bad in ["", "../x", "a/b", ".hidden"] {
            w.file_name = bad.into();
            assert!(w.base_name().is_err(), "{bad}");
        }
    }
}
