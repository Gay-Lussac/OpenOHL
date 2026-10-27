// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! File › Import › **Another project into this one…** (owner,
//! 2026-10-25): another program's file read as a project (a project
//! plugin), then brought into the project open — into the project line
//! you work on, or a new project line made from it; each of its routes
//! said to be one of the project's or new; what appears twice listed, each
//! settled (keep the project's, take the imported one, keep both); the
//! towers in both listed, one line each, opened to their sets, each set
//! of the import settled (add or leave out; at another place than the
//! project's: keep the project's, take the imported one, or add it under
//! a new number); then its library files and its line written
//! ([`ohl_io::import_into`]).

use std::collections::BTreeMap;
use std::path::PathBuf;

use eframe::egui;
use ohl_io::import_into::{ImportOptions, ImportPlan, Resolution, SetState, Twice};
use ohl_io::projects::ImportedProject;

use crate::app::Ready;

/// The window's state.
#[derive(Default)]
pub struct ImportInto {
    /// Shown.
    pub open: bool,
    /// The file to read.
    file: Option<PathBuf>,
    /// Format id (empty: from the extension).
    format: String,
    /// Options, `KEY=VALUE` separated by commas.
    options: String,
    /// Clip the sections in at their stringing condition.
    clip_in: bool,
    /// The project read.
    read: Option<Box<ImportedProject>>,
    /// Into a new project line made from the one you work on.
    into_new: bool,
    /// Its name.
    new_name: String,
    /// The routes, the tolerance and the choices.
    how: ImportOptions,
    /// The plan, and the line and libraries revision it was made for.
    plan: Option<(Result<ImportPlan, String>, (u64, u64))>,
    /// Asked to make the plan again.
    stale: bool,
    /// What went wrong, or was done.
    status: Option<(String, bool)>,
}

impl ImportInto {
    /// Opens the window (a new import).
    pub fn show(&mut self) {
        *self = Self {
            open: true,
            how: ImportOptions {
                tolerance: ohl_io::import_into::DEFAULT_TOLERANCE,
                ..Default::default()
            },
            ..Self::default()
        };
    }
}

/// The library files of the project line open, of the kinds `kinds`.
fn project_libraries(model: &crate::model::Model, kinds: &[&str]) -> Vec<(String, String, String)> {
    let Some(s) = model.session() else {
        return Vec::new();
    };
    kinds
        .iter()
        .flat_map(|k| {
            s.library_files(k)
                .unwrap_or_default()
                .into_iter()
                .map(move |(n, t)| ((*k).to_owned(), n, t))
        })
        .collect()
}

/// The window.
pub fn window(ctx: &egui::Context, r: &mut Ready, plugins: &mut crate::plugins::Plugins) {
    let w = &mut r.import_into;
    if !w.open {
        return;
    }
    plugins.want_formats();
    if let Some(read) = plugins.project_read.take() {
        match read {
            Ok(p) => {
                w.new_name = format!(
                    "import {}",
                    w.file
                        .as_ref()
                        .and_then(|f| f.file_stem())
                        .map_or(String::new(), |s| s.to_string_lossy().into_owned())
                );
                w.read = Some(p);
                w.stale = true;
                w.status = None;
            }
            Err(e) => w.status = Some((e, true)),
        }
    }
    // The plan, again when asked or when the project changed.
    let stamp = (r.model.line_version, r.model.revisions.libraries);
    if let Some(p) = &w.read
        && (w.stale || w.plan.as_ref().is_none_or(|(_, s)| *s != stamp))
    {
        // The project's types, insulators and wires too: its towers are
        // compared as they are in effect, its cables by id.
        let mut kinds: Vec<&str> = p.libraries.iter().map(|(k, ..)| k.as_str()).collect();
        kinds.extend(["structures", "insulators", "cables"]);
        kinds.sort_unstable();
        kinds.dedup();
        let libs = project_libraries(&r.model, &kinds);
        // The line as stored (typed structures not merged with their
        // types), as it is written back.
        let stored = r
            .model
            .session()
            .map_or_else(|| r.model.line.clone(), |s| s.export());
        w.plan = Some((ohl_io::import_into::plan(&stored, &libs, p, &w.how), stamp));
        w.stale = false;
    }
    let formats: Vec<crate::plugins::FormatInfo> = plugins
        .formats
        .as_ref()
        .map(|f| f.projects.clone())
        .unwrap_or_default();
    let current = r.model.project_line();
    let routes: Vec<String> = r.model.line.routes.iter().map(|x| x.id.clone()).collect();
    let editable = r.model.can_edit();
    let mut open = true;
    let mut go = false;
    egui::Window::new("Import another project into this one")
        .id(egui::Id::new("import-into-project"))
        .open(&mut open)
        .default_width(760.0)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(
                    "Another program's file (a project plugin reads it) brought into this \
                     project: you say which project line it goes into and whether its routes \
                     are this project's, and settle what appears in both. Nothing is changed \
                     until Import.",
                )
                .small()
                .weak(),
            );
            if let Err(why) = &editable {
                crate::menu::error_label(ui, format!("read-only: {why}"));
            }
            // 1. The file.
            ui.separator();
            ui.label(egui::RichText::new("1. The file").strong());
            if formats.is_empty() {
                crate::menu::error_label(
                    ui,
                    "No project format: install a project plugin (File › Plugins › Plugin manager).",
                );
            }
            ui.horizontal(|ui| {
                if ui.button("Choose the file…").clicked()
                    && let Some(p) = rfd::FileDialog::new().pick_file()
                {
                    w.file = Some(p);
                    w.read = None;
                    w.plan = None;
                }
                ui.label(
                    w.file
                        .as_ref()
                        .map_or("no file".into(), |p| p.display().to_string()),
                );
            });
            ui.horizontal(|ui| {
                ui.label("format");
                egui::ComboBox::from_id_salt("import-into-format")
                    .selected_text(if w.format.is_empty() {
                        "from the extension"
                    } else {
                        w.format.as_str()
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut w.format, String::new(), "from the extension");
                        for f in &formats {
                            ui.selectable_value(&mut w.format, f.id.clone(), &f.id)
                                .on_hover_text(format!(
                                    "(.{}) {} — {}",
                                    f.extension, f.description, f.origin
                                ));
                        }
                    });
                ui.label("options");
                ui.add(
                    egui::TextEdit::singleline(&mut w.options)
                        .hint_text("KEY=VALUE, …")
                        .desired_width(200.0),
                );
            });
            ui.checkbox(
                &mut w.clip_in,
                "Clip the sections in at their stringing condition",
            );
            ui.horizontal(|ui| {
                if plugins.reading_project {
                    ui.spinner();
                    ui.label("reading…");
                } else if ui
                    .add_enabled(w.file.is_some(), egui::Button::new("Read the file"))
                    .clicked()
                    && let Some(file) = &w.file
                {
                    match ohl_io::parse_options(&w.options) {
                        Ok(options) => plugins.read_project(crate::plugins::ProjectRead {
                            path: file.clone(),
                            format: (!w.format.is_empty()).then(|| w.format.clone()),
                            options,
                            clip_in: w.clip_in,
                        }),
                        Err(e) => w.status = Some((e, true)),
                    }
                }
            });
            let Some(read) = &w.read else {
                if let Some((t, e)) = &w.status {
                    crate::menu::message(ui, t.as_str(), e.then_some(ui.visuals().error_fg_color));
                }
                return;
            };
            ui.label(format!(
                "Read: {} structures, {} sections, {} jumpers, {} routes, {} library files.",
                read.line.structures.len(),
                read.line.sections.len(),
                read.line.jumpers.len(),
                read.line.routes.len(),
                read.libraries.len()
            ));

            // 2. Where it goes.
            ui.separator();
            ui.label(egui::RichText::new("2. Into which project line?").strong());
            ui.radio_value(
                &mut w.into_new,
                false,
                format!("{current} (the line you work on; one change, undoable)"),
            );
            ui.horizontal(|ui| {
                ui.radio_value(
                    &mut w.into_new,
                    true,
                    format!("a new project line made from {current}:"),
                )
                .on_hover_text(
                    "To look at it apart first; Versions › Bring in changes takes it into another \
                     line later",
                );
                ui.add_enabled(
                    w.into_new,
                    egui::TextEdit::singleline(&mut w.new_name).desired_width(220.0),
                );
            });

            // 3. The routes.
            ui.separator();
            ui.label(egui::RichText::new("3. Its routes").strong());
            for ir in &read.line.routes {
                let now = match w.how.routes.get(&ir.id) {
                    Some(x) => x.clone(),
                    None => routes.contains(&ir.id).then(|| ir.id.clone()),
                };
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "{}{}",
                        ir.id,
                        if ir.name.is_empty() || ir.name == ir.id {
                            String::new()
                        } else {
                            format!(" ({})", ir.name)
                        }
                    ));
                    ui.label("is");
                    egui::ComboBox::from_id_salt(("import-into-route", &ir.id))
                        .selected_text(match &now {
                            Some(e) => format!("this project's route {e}"),
                            None => "a new route".into(),
                        })
                        .show_ui(ui, |ui| {
                            let mut pick = |ui: &mut egui::Ui, v: Option<String>, text: String| {
                                if ui.selectable_label(now == v, text).clicked() {
                                    w.how.routes.insert(ir.id.clone(), v);
                                    w.stale = true;
                                }
                            };
                            for e in &routes {
                                pick(ui, Some(e.clone()), format!("this project's route {e}"));
                            }
                            pick(ui, None, "a new route".into());
                        });
                });
            }
            ui.horizontal(|ui| {
                ui.label("Alignment points and structures nearer than");
                if ui
                    .add(
                        egui::DragValue::new(&mut w.how.tolerance)
                            .range(0.01..=50.0)
                            .speed(0.05)
                            .suffix(" m"),
                    )
                    .changed()
                {
                    w.stale = true;
                }
                ui.label("in plan are the same place.");
            });

            // 4. What appears twice.
            ui.separator();
            ui.label(egui::RichText::new("4. What appears in both").strong());
            let Some((plan, _)) = &w.plan else {
                return;
            };
            let plan = match plan {
                Ok(p) => p,
                Err(e) => {
                    crate::menu::error_label(ui, e.as_str());
                    return;
                }
            };
            let words = |m: &BTreeMap<&'static str, usize>| -> String {
                if m.is_empty() {
                    return "nothing".into();
                }
                m.iter()
                    .map(|(k, n)| format!("{n} {k}{}", if *n == 1 { "" } else { "s" }))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            ui.label(format!("Added: {}.", words(&plan.added)));
            ui.label(format!(
                "The same in both (left as they are): {}.",
                words(&plan.identical)
            ));
            if !plan.missing_types.is_empty() {
                crate::menu::error_label(
                    ui,
                    format!(
                        "After the import these structures' types are in no structure type \
                         file of the project: {}",
                        plan.missing_types
                            .iter()
                            .map(|(s, t)| format!("{s} ({t})"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                );
            }
            if !plan.shared.is_empty() {
                towers(ui, plan, &mut w.how, &mut w.stale);
            }
            if plan.duplicates.is_empty() {
                ui.label("Nothing appears in both with other values.");
            } else {
                ui.horizontal(|ui| {
                    ui.label("All:");
                    for res in Resolution::ALL {
                        if ui.button(res.words()).clicked() {
                            for d in plan.duplicates.iter().filter(|d| d.choices().contains(&res)) {
                                w.how.choices.insert(d.key.clone(), res);
                            }
                            w.stale = true;
                        }
                    }
                });
                egui::ScrollArea::vertical()
                    .id_salt("import-into-duplicates")
                    .max_height(260.0)
                    .show(ui, |ui| {
                        egui::Grid::new("import-into-duplicates-grid")
                            .striped(true)
                            .num_columns(5)
                            .spacing([10.0, 4.0])
                            .show(ui, |ui| {
                                for h in ["", "this project's", "imported", "what differs", ""] {
                                    ui.strong(h);
                                }
                                ui.end_row();
                                for d in &plan.duplicates {
                                    ui.label(d.kind);
                                    ui.label(&d.existing);
                                    ui.label(match d.twice {
                                        Twice::SameId => d.imported.clone(),
                                        Twice::SamePlace => {
                                            format!("{} (at the same place)", d.imported)
                                        }
                                    });
                                    ui.label(d.differences.join(", "));
                                    egui::ComboBox::from_id_salt(("import-into-choice", &d.key))
                                        .selected_text(d.resolution.words())
                                        .show_ui(ui, |ui| {
                                            for &res in d.choices() {
                                                if ui
                                                    .selectable_label(
                                                        d.resolution == res,
                                                        res.words(),
                                                    )
                                                    .clicked()
                                                {
                                                    w.how.choices.insert(d.key.clone(), res);
                                                    w.stale = true;
                                                }
                                            }
                                        });
                                    ui.end_row();
                                }
                            });
                    });
            }
            if !plan.renamed.is_empty() || !plan.notes.is_empty() {
                egui::CollapsingHeader::new(format!(
                    "Ids given to imported objects ({}) and notes ({})",
                    plan.renamed.len(),
                    plan.notes.len()
                ))
                .id_salt("import-into-notes")
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("import-into-notes-scroll")
                        .max_height(160.0)
                        .show(ui, |ui| {
                            for (k, from, to) in &plan.renamed {
                                ui.label(
                                    egui::RichText::new(format!("{k} {from} is now {to}")).small(),
                                );
                            }
                            for n in &plan.notes {
                                ui.label(egui::RichText::new(n).small());
                            }
                        });
                });
            }
            ui.separator();
            ui.horizontal(|ui| {
                go = ui
                    .add_enabled(
                        editable.is_ok() && !(w.into_new && w.new_name.trim().is_empty()),
                        egui::Button::new("Import"),
                    )
                    .clicked();
                if ui.button("Cancel").clicked() {
                    w.open = false;
                }
            });
            if let Some((t, e)) = &w.status {
                crate::menu::message(ui, t.as_str(), e.then_some(ui.visuals().error_fg_color));
            }
        });
    if !open {
        r.import_into.open = false;
    }
    if go {
        let result = apply(r);
        r.import_into.status = Some(match &result {
            Ok(m) => (m.clone(), false),
            Err(e) => (e.clone(), true),
        });
        if result.is_ok() {
            r.import_into.open = false;
            r.status = Some((r.import_into.status.clone().unwrap().0, false));
        }
    }
}

/// The towers in both: one line each, opened to their sets side by side
/// (the project's and the imported wire point, world, m; how far apart),
/// each set of the import with its choices.
fn towers(ui: &mut egui::Ui, plan: &ImportPlan, how: &mut ImportOptions, stale: &mut bool) {
    ui.label(
        egui::RichText::new(format!(
            "Towers in both ({}): kept once, as this project's (its id, position, orientation \
             and type); the import's sets settled one by one.",
            plan.shared.len()
        ))
        .strong(),
    );
    let at = |p: Option<[f64; 3]>| {
        p.map_or("—".to_owned(), |p| {
            format!("{:.2}, {:.2}, {:.2}", p[0], p[1], p[2])
        })
    };
    egui::ScrollArea::vertical()
        .id_salt("import-into-towers")
        .max_height(260.0)
        .show(ui, |ui| {
            for t in &plan.shared {
                egui::CollapsingHeader::new(t.summary())
                    .id_salt(("import-into-tower", &t.key))
                    .show(ui, |ui| {
                        egui::Grid::new(("import-into-sets", &t.key))
                            .striped(true)
                            .num_columns(5)
                            .spacing([10.0, 4.0])
                            .show(ui, |ui| {
                                for h in ["set", "this project's", "imported", "apart", ""] {
                                    ui.strong(h);
                                }
                                ui.end_row();
                                for s in &t.sets {
                                    ui.label(s.set.to_string());
                                    ui.label(at(s.existing));
                                    ui.label(at(s.imported));
                                    ui.label(
                                        s.distance().map_or(String::new(), |d| format!("{d:.2} m")),
                                    );
                                    match (s.state, s.choice) {
                                        (SetState::OnlyExisting, _) => {
                                            ui.label("this project's only");
                                        }
                                        (SetState::Same, _) => {
                                            ui.label("the same attachment");
                                        }
                                        (_, Some(now)) => {
                                            let text = match s.new_set {
                                                Some(n) => format!("{} ({n})", now.words()),
                                                None => now.words().to_owned(),
                                            };
                                            egui::ComboBox::from_id_salt((
                                                "import-into-set",
                                                &s.key,
                                            ))
                                            .selected_text(text)
                                            .show_ui(
                                                ui,
                                                |ui| {
                                                    for &c in s.choices() {
                                                        if ui
                                                            .selectable_label(now == c, c.words())
                                                            .clicked()
                                                        {
                                                            how.sets.insert(s.key.clone(), c);
                                                            *stale = true;
                                                        }
                                                    }
                                                },
                                            );
                                        }
                                        (_, None) => {
                                            ui.label("");
                                        }
                                    }
                                    ui.end_row();
                                }
                            });
                    });
            }
        });
}

/// Writes the import: the new project line first (when asked), then the
/// library files, then the merged line (one command).
fn apply(r: &mut Ready) -> Result<String, String> {
    let w = &r.import_into;
    let Some(Ok(plan)) = w.plan.as_ref().map(|(p, _)| p.clone()) else {
        return Err("nothing to import".into());
    };
    let file = w
        .file
        .as_ref()
        .and_then(|f| f.file_name())
        .map_or(String::new(), |n| n.to_string_lossy().into_owned());
    if w.into_new {
        let name = w.new_name.trim().to_owned();
        let from = r.model.project_line();
        r.model
            .project_op(
                "project line",
                ohl_server::Op::CreateLine {
                    name: name.clone(),
                    from,
                },
            )
            .map_err(|e| e.to_string())?;
        r.switch_line(&name);
        if r.model.project_line() != name {
            return Err(format!("could not work on the new project line {name}"));
        }
    }
    for (kind, name, text) in &plan.libraries {
        r.model
            .project_op(
                "import",
                ohl_server::Op::PutLibrary {
                    kind: kind.clone(),
                    name: name.clone(),
                    text: text.clone(),
                },
            )
            .map_err(|e| format!("library {kind}/{name}: {e}"))?;
    }
    // The line's types, wires… read again with the files written.
    if !plan.libraries.is_empty() {
        r.model
            .refresh_project_libraries()
            .map_err(|e| format!("the library files written, not read again: {e}"))?;
    }
    // Nothing new in the line (all of it the project's already): no
    // change to make.
    let unchanged = r.model.session().is_some_and(|s| s.export() == plan.line);
    let done = unchanged
        || r.perform(crate::edit::Action::Command {
            what: format!("imported {file} into {}", r.model.project_line()),
            command: ohl_session::Command::ReplaceLine {
                line: Box::new(plan.line.clone()),
            },
        });
    if !done {
        return Err("the line was not changed (see the status bar)".into());
    }
    Ok(format!(
        "imported {file} into {}: {} settled, {} library files written",
        r.model.project_line(),
        plan.duplicates.len(),
        plan.libraries.len()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    fn harness(name: &str) -> Harness<'static, App> {
        let model = crate::tests::synthetic_project(name);
        let mut h = Harness::builder()
            .with_size(egui::vec2(1500.0, 950.0))
            .build_eframe(|_| App::with_model(model, None));
        h.run();
        h
    }

    /// The first section's stringing tension in the line.
    fn tension(r: &Ready) -> f64 {
        match r.model.line.sections[0].stringing {
            ohl_line::Stringing::Tension {
                horizontal_tension, ..
            } => horizontal_tension,
            _ => panic!("the example strings in tension"),
        }
    }

    /// The import (as a project plugin gives it) with its first section
    /// strung 1 kN harder; the choice for it given, if any.
    fn read_harder(r: &mut Ready, choice: Option<Resolution>) -> (String, f64) {
        let mut line = r.model.session().unwrap().export();
        let id = line.sections[0].id.clone();
        let before = tension(r);
        line.sections[0].stringing = ohl_line::Stringing::Tension {
            horizontal_tension: before + 1000.0,
            temperature: 15.0,
        };
        r.import_into.show();
        r.import_into.file = Some(PathBuf::from("other.xml"));
        r.import_into.read = Some(Box::new(ImportedProject {
            line,
            libraries: Vec::new(),
            notes: Vec::new(),
        }));
        if let Some(c) = choice {
            r.import_into.how.choices.insert(format!("section:{id}"), c);
        }
        r.import_into.stale = true;
        (id, before)
    }

    /// The import read (the plugins' formats load meanwhile, a spinner:
    /// steps, not runs): the same line with its first section strung
    /// harder. The window lists it as appearing in both, kept as the
    /// project's by default; "take the imported one" for all and Import:
    /// the section changed, one change, undone at once.
    #[test]
    fn a_section_in_both_is_taken_from_the_import() {
        let mut h = harness("import-into");
        let (_, before) = read_harder(h.state_mut().ready().unwrap(), None);
        h.run_steps(3);
        let plan = |h: &mut Harness<'static, App>| {
            let r = h.state_mut().ready().unwrap();
            r.import_into.plan.as_ref().unwrap().0.clone().unwrap()
        };
        let p = plan(&mut h);
        assert_eq!(p.duplicates.len(), 1, "{:?}", p.duplicates);
        assert_eq!(p.duplicates[0].differences, ["stringing"]);
        h.get_by_label("take the imported one").click();
        h.run_steps(3);
        assert_eq!(
            plan(&mut h).duplicates[0].resolution,
            Resolution::TakeImported
        );
        h.get_by_label("Import").click();
        h.run_steps(3);
        let r = h.state_mut().ready().unwrap();
        assert!(!r.import_into.open, "{:?}", r.import_into.status);
        assert!((tension(r) - before - 1000.0).abs() < 1e-9);
        r.model.undo().unwrap();
        assert!((tension(r) - before).abs() < 1e-9);
    }

    /// Into a new project line made from the one worked on: the line is
    /// made, worked on, and holds the import; the line left is unchanged.
    #[test]
    fn into_a_new_project_line() {
        let mut h = harness("import-into-new");
        let (before_line, before) = {
            let r = h.state_mut().ready().unwrap();
            let line = r.model.project_line();
            let (_, t) = read_harder(r, Some(Resolution::TakeImported));
            r.import_into.into_new = true;
            r.import_into.new_name = "imported".into();
            (line, t)
        };
        h.run_steps(3);
        h.get_by_label("Import").click();
        h.run_steps(3);
        let r = h.state_mut().ready().unwrap();
        assert_eq!(
            r.model.project_line(),
            "imported",
            "{:?}",
            r.import_into.status
        );
        assert!((tension(r) - before - 1000.0).abs() < 1e-9);
        r.switch_line(&before_line);
        assert!((tension(r) - before).abs() < 1e-9);
    }

    /// A tower in both: the import's T3 holds a set the project's has not
    /// (a wire point fixed to it, set 9). The window lists the tower in
    /// one line, opened to its sets side by side (three the same), set 9
    /// to be added by default; Import
    /// adds it to the project's T3 (its own wire point), nothing else
    /// changing.
    #[test]
    fn a_tower_in_both_lists_its_sets() {
        let mut h = harness("import-into-tower");
        {
            let r = h.state_mut().ready().unwrap();
            let mut line = r.model.session().unwrap().export();
            let t3 = line.structures.iter_mut().find(|s| s.id == "T3").unwrap();
            let mut w = t3.wires[0].clone();
            w.label = "W9".into();
            w.set = Some(9);
            w.position[2] += 3.0;
            t3.wires.push(w);
            r.import_into.show();
            r.import_into.file = Some(PathBuf::from("branch.xml"));
            r.import_into.read = Some(Box::new(ImportedProject {
                line,
                libraries: Vec::new(),
                notes: Vec::new(),
            }));
            r.import_into.stale = true;
        }
        h.run_steps(3);
        {
            let r = h.state_mut().ready().unwrap();
            let p = r.import_into.plan.as_ref().unwrap().0.clone().unwrap();
            assert!(p.duplicates.is_empty(), "{:?}", p.duplicates);
            assert_eq!(p.shared.len(), 1);
            assert_eq!(p.shared[0].existing, "T3");
        }
        h.get_by_label_contains("T3: 3 sets in the project's, 4 in the imported")
            .click();
        h.run_steps(3);
        h.get_by_label("9");
        assert_eq!(h.get_all_by_label("the same attachment").count(), 3);
        {
            let r = h.state_mut().ready().unwrap();
            let p = r.import_into.plan.as_ref().unwrap().0.clone().unwrap();
            let s9 = p.shared[0].sets.iter().find(|s| s.set == 9).unwrap();
            assert_eq!(s9.choice, Some(ohl_io::import_into::SetChoice::Add));
        }
        h.get_by_label("Import").click();
        h.run_steps(3);
        let r = h.state_mut().ready().unwrap();
        assert!(!r.import_into.open, "{:?}", r.import_into.status);
        let t3 = r.model.session().unwrap().export();
        let t3 = t3.structure("T3").unwrap();
        assert_eq!(t3.wire_at_set(9).unwrap().label, "W9");
        assert_eq!(t3.wires.len(), 4);
    }
}
