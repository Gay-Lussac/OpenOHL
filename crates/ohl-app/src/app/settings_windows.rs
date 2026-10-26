// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Settings windows: the coordinate system, the bimetallic model, the
//! defaults, the solver, creep, history, the survey's import and export.

use super::*;

/// Line › Criteria › Bimetallic conductor model: what the outer strands of the
/// line's bimetallic cables do above the transition temperature, for the
/// cables whose wire library entry does not say; OK makes it one command
/// (every section is solved again).
/// Line › Terrain › Coordinate system: Define (an edit of the line's settings),
/// Calculator, Reproject (a new project, opened when done).
pub(super) fn crs_windows(ctx: &egui::Context, r: &mut Ready) {
    match r.crs.show(ctx, &r.model) {
        Some(crate::crs::Request::Define { crs, height_datum }) => {
            if let Ok(e) = &r.model.editing {
                let mut line = e.session.line().clone();
                line.crs = crs;
                line.height_datum = height_datum;
                let command = ohl_session::line_settings_change(e.session.line(), &line);
                r.perform(crate::edit::Action::Command {
                    what: "coordinate system".into(),
                    command,
                });
            }
        }
        Some(crate::crs::Request::Open(path)) => r.open_project = Some(path),
        None => {}
    }
}

pub(super) fn bimetallic_window(ctx: &egui::Context, r: &mut Ready) {
    use ohl_cables::{BimetallicBehavior, DEFAULT_MAX_VIRTUAL_COMPRESSION};
    let Some(mut draft) = r.bimetallic_window else {
        return;
    };
    // The line's bimetallic cables: those following this choice, those
    // with their own.
    let (mut follow, mut own) = (Vec::new(), Vec::new());
    for s in &r.model.line.sections {
        if let Some(c) = r.model.library.get(&s.cable).filter(|c| c.core.is_some()) {
            let list = if c.bimetallic_behavior.is_some() {
                &mut own
            } else {
                &mut follow
            };
            if !list.contains(&c.id) {
                list.push(c.id.clone());
            }
        }
    }
    let (mut ok, mut cancel) = (false, false);
    let mut open = true;
    egui::Window::new("Bimetallic conductor model")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(520.0)
        .show(ctx, |ui| {
            ui.label(
                "The outer strands of a bimetallic conductor (e.g. aluminium over steel) expand \
                 more with heat than the core: above a transition temperature they carry no more \
                 tension. Above it:",
            );
            ui.radio_value(
                &mut draft.behavior,
                BimetallicBehavior::BirdCage,
                "The outer strands take no compression (bird cage)",
            );
            ui.radio_value(
                &mut draft.behavior,
                BimetallicBehavior::Compression,
                "The outer strands can go into compression (slightly larger sags when hot)",
            );
            ui.add_enabled_ui(draft.behavior == BimetallicBehavior::Compression, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Largest virtual compressive stress");
                    let mut mpa = draft
                        .max_virtual_compression
                        .map_or(DEFAULT_MAX_VIRTUAL_COMPRESSION, |s| s.si())
                        / 1e6;
                    if ui
                        .add(
                            egui::DragValue::new(&mut mpa)
                                .range(0.0..=1e6)
                                .speed(10.0)
                                .suffix(" MPa"),
                        )
                        .changed()
                    {
                        draft.max_virtual_compression =
                            Some(ohl_core::units::Stress::from_megapascals(mpa));
                    }
                });
                ui.label(
                    egui::RichText::new(
                        "Virtual stress = actual stress × Ao / At (Ao: the outer strands' area, \
                         At: the whole conductor's)",
                    )
                    .small()
                    .weak(),
                );
            });
            ui.separator();
            let list = |v: &Vec<String>| {
                if v.is_empty() {
                    "none".to_owned()
                } else {
                    v.join(", ")
                }
            };
            ui.label(
                egui::RichText::new(format!(
                    "Following this choice: {}. With their own (wire library): {}.",
                    list(&follow),
                    list(&own)
                ))
                .small(),
            );
            if let Err(why) = r.model.can_edit() {
                crate::menu::error_label(ui, format!("read-only: {why}"));
            }
            ui.horizontal(|ui| {
                ok = ui
                    .add_enabled(r.model.can_edit().is_ok(), egui::Button::new("OK"))
                    .clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
    if ok {
        // The default stress needs no value.
        if draft.behavior == BimetallicBehavior::BirdCage
            || draft
                .max_virtual_compression
                .is_some_and(|s| (s.si() - DEFAULT_MAX_VIRTUAL_COMPRESSION).abs() < 1.0)
        {
            draft.max_virtual_compression = None;
        }
        let command = r
            .model
            .editing
            .as_ref()
            .map(|e| e.session.bimetallic_command(draft));
        if let Ok(command) = command {
            r.perform(crate::edit::Action::Command {
                what: "bimetallic conductor model".into(),
                command,
            });
        }
    }
    r.bimetallic_window = (open && !ok && !cancel).then_some(draft);
}

/// Line › Criteria › Calculation options: how the line's calculations are done
/// (one command, undoable; the sections it touches are solved again).
/// The measured cost of the jumpers' pull, for the window (release
/// builds): a test line, and an existing 25 km line where nearly every
/// section ends at a jumper.
pub(super) const PULL_COST: &str = "measured: about 3× the time of the estimate on a test line (31 \
    sections, 21 at 15 jumpers), about 5× on an existing line where nearly every section ends at \
    a jumper (554 sections, 518 jumpers, 99 weather cases: 32 s ➡ 158 s, the sections over the \
    cores); sections away from jumpers are not solved again.";

/// Line › Criteria › Default values: what new objects start with, for the
/// project; exported and imported as a file.
pub(super) fn defaults_window(ctx: &egui::Context, r: &mut Ready) {
    let Some((mut draft, mut message)) = r.defaults_window.take() else {
        return;
    };
    let (mut ok, mut cancel) = (false, false);
    let mut open = true;
    egui::Window::new("Default values")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(560.0)
        .show(ctx, |ui| {
            ui.label(
                "What new objects start with in this project, and what the calculations take \
                 where an object, a cable, a weather or ambient case or a rule file gives no \
                 value of its own. Display settings stay yours (View, Display options).",
            );
            egui::ScrollArea::vertical()
                .max_height(420.0)
                .show(ui, |ui| {
                    egui::Grid::new("defaults")
                        .num_columns(3)
                        .striped(true)
                        .show(ui, |ui| {
                            let program = ohl_line::Defaults::default();
                            let mut section = "";
                            for f in ohl_line::DEFAULT_FIELDS {
                                if f.section != section {
                                    section = f.section;
                                    ui.label(egui::RichText::new(section).strong());
                                    ui.end_row();
                                }
                                ui.label(f.title).on_hover_text(f.help);
                                let mut v = (f.get)(&draft);
                                let high = if f.high.is_finite() { f.high } else { f64::MAX };
                                let speed = (v.abs() * 0.01).max(0.001);
                                let mut d = egui::DragValue::new(&mut v)
                                    .speed(speed)
                                    .range(f.low..=high)
                                    .max_decimals(4);
                                if !f.unit.is_empty() {
                                    d = d.suffix(format!(" {}", f.unit));
                                }
                                if ui.add(d).on_hover_text(f.help).changed() {
                                    (f.set)(&mut draft, v);
                                }
                                let p = (f.get)(&program);
                                ui.label(
                                    egui::RichText::new(if v == p {
                                        "the program's".to_owned()
                                    } else {
                                        format!("program: {p}")
                                    })
                                    .small()
                                    .weak(),
                                );
                                ui.end_row();
                            }
                        });
                });
            if ui
                .add_enabled(!draft.is_default(), egui::Button::new("Program's values"))
                .on_hover_text("Back to the values OHL-Design starts with")
                .clicked()
            {
                draft = ohl_line::Defaults::default();
            }
            ui.separator();
            let ext = ohl_line::DEFAULTS_FILE_EXTENSION;
            ui.horizontal(|ui| {
                if ui
                    .button("Export…")
                    .on_hover_text(format!("Writes these values to a .{ext} file"))
                    .clicked()
                    && let Some(p) = rfd::FileDialog::new()
                        .add_filter("Default values", &[ext])
                        .set_file_name(format!("defaults.{ext}"))
                        .save_file()
                {
                    message = Some(match std::fs::write(&p, draft.to_text()) {
                        Ok(()) => (format!("exported to {}", p.display()), false),
                        Err(e) => (format!("{}: {e}", p.display()), true),
                    });
                }
                if ui
                    .button("Import…")
                    .on_hover_text(format!(
                        "Reads the values of a .{ext} file (OK keeps them for the project)"
                    ))
                    .clicked()
                    && let Some(p) = rfd::FileDialog::new()
                        .add_filter("Default values", &ohl_core::files::open_extensions(ext))
                        .pick_file()
                {
                    message = Some(
                        match std::fs::read_to_string(&p)
                            .map_err(|e| e.to_string())
                            .and_then(|t| ohl_line::Defaults::parse(&t))
                        {
                            Ok(d) => {
                                draft = d;
                                (format!("read {}", p.display()), false)
                            }
                            Err(e) => (format!("{}: {e}", p.display()), true),
                        },
                    );
                }
            });
            if let Some((m, error)) = &message {
                if *error {
                    crate::menu::error_label(ui, m.clone());
                } else {
                    ui.label(egui::RichText::new(m).small().weak());
                }
            }
            if let Err(why) = r.model.can_edit() {
                crate::menu::error_label(ui, format!("read-only: {why}"));
            }
            ui.horizontal(|ui| {
                ok = ui
                    .add_enabled(
                        r.model.can_edit().is_ok()
                            && draft != r.model.line.defaults
                            && draft.validate().is_ok(),
                        egui::Button::new("OK"),
                    )
                    .clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
    if ok {
        let command = r
            .model
            .editing
            .as_ref()
            .map(|e| e.session.defaults_command(draft));
        if let Ok(command) = command {
            r.perform(crate::edit::Action::Command {
                what: "default values".into(),
                command,
            });
        }
    }
    if open && !ok && !cancel {
        r.defaults_window = Some((draft, message));
    }
}

/// A jumper joining two different circuits: the user is told, compares
/// them and chooses the one that stays ([`crate::circuit_join`]).
pub(super) fn circuit_join_window(ctx: &egui::Context, r: &mut Ready) {
    let Some(mut join) = r.circuit_join.take() else {
        return;
    };
    match crate::circuit_join::window(ctx, &mut join) {
        crate::circuit_join::Outcome::Open => r.circuit_join = Some(join),
        crate::circuit_join::Outcome::Cancel => {
            r.status = Some((
                format!("Add jumper {}: cancelled, nothing changed", join.jumper.id),
                false,
            ));
        }
        crate::circuit_join::Outcome::Join => match join.command(&r.model) {
            Ok(command) => {
                let action = crate::edit::Action::Command {
                    what: join.what(),
                    command,
                };
                r.jumper_added(join.jumper.id.clone(), action, join.open_editor);
            }
            Err(e) => r.status = Some((e, true)),
        },
    }
}

pub(super) fn solver_window(ctx: &egui::Context, r: &mut Ready) {
    use ohl_line::JumperLoads;
    let Some(mut draft) = r.solver_window else {
        return;
    };
    let jumpers = r.model.line.jumpers.len();
    let (mut ok, mut cancel) = (false, false);
    let mut open = true;
    egui::Window::new("Calculation options")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(560.0)
        .show(ctx, |ui| {
            ui.strong("Jumpers on the clamps");
            ui.label(
                "What the sections at a jumper's ends carry at their clamps (clamps hanging \
                 from strings; a wire clamped to the structure leaves it to the structure). \
                 The jumper itself is solved between the clamps either way; the ruling-span \
                 method leaves jumpers out.",
            );
            for (j, text, help) in [
                (
                    JumperLoads::Estimate,
                    "An estimate (default): weight, ice and wind",
                    "Each end carries the weight, ice and wind (across the line, from the \
                     weather case's side) of the jumper up to its first helping string, half of \
                     it without",
                ),
                (
                    JumperLoads::Pull,
                    "The jumper's own pull",
                    "The force the solved jumper exerts on each clamp (its tension there, with \
                     its stiffness; not the moments of clamped ends): the sections and the \
                     jumpers are solved in turn until the clamps no longer move",
                ),
            ] {
                ui.radio_value(&mut draft.jumper_loads, j, text)
                    .on_hover_text(help);
            }
            ui.label(
                egui::RichText::new(format!(
                    "{jumpers} jumper(s) on this line. Computation time: the estimate costs \
                     nothing more; the pull solves the sections at jumpers and the jumpers again \
                     in two or three passes — {}",
                    PULL_COST
                ))
                .small()
                .weak(),
            );
            ui.separator();
            ui.strong("Jumpers on taps");
            ui.label(
                "A jumper starting from a clamp on a span or on another jumper (a tap: a hinge, \
                 forces only): what the wire it hangs on carries.",
            );
            let taps = r.model.line.jumpers.iter().filter(|j| j.on_tap()).count();
            for (choice, text, help) in [
                (
                    ohl_line::TapLoads::Loop,
                    "The full loop (default)",
                    "The jumper's own force at its tap put on the wire (or jumper) it hangs on, \
                     both solved in turn until the tap no longer moves",
                ),
                (
                    ohl_line::TapLoads::Once,
                    "One calculation",
                    "The wire carries the jumper's estimate (weight, ice and wind up to its first \
                     support, half without), then the jumper is solved once: no loop",
                ),
            ] {
                ui.radio_value(&mut draft.taps, choice, text)
                    .on_hover_text(help);
            }
            ui.label(
                egui::RichText::new(format!(
                    "{taps} jumper(s) on taps on this line. The loop solves the wires at taps and \
                     their jumpers again in passes (usually two or three); one calculation costs \
                     nothing more."
                ))
                .small()
                .weak(),
            );
            ui.separator();
            ui.strong("Ice and wind on the wires");
            ui.label(
                "Per what metre of the wire the finite elements carry the ice and the wind \
                 (the own weight is per metre of unstressed wire either way: it is conserved).",
            );
            for (choice, text, help) in [
                (
                    ohl_line::LoadsPer::Stretched,
                    "Per stretched metre (default, exact)",
                    "Ice and wind act on the conductor as it is, longer by its elastic strain: \
                     each element carries them times its stretch L/L0, found by a fixed point \
                     (three solves in practice)",
                ),
                (
                    ohl_line::LoadsPer::Unstretched,
                    "Per unstretched metre (as other programs)",
                    "The former treatment, which other programs appear to use: about T/EA, \
                     0.1 %, less ice and wind, so slightly less sag and swing than the exact \
                     treatment (on the unsafe side for clearances). A simplification, for \
                     comparing with other programs",
                ),
            ] {
                ui.radio_value(&mut draft.loads_per, choice, text)
                    .on_hover_text(help);
            }
            if draft.loads_per == ohl_line::LoadsPer::Unstretched {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    egui::RichText::new(
                        "Per unstretched metre: about 0.1 % less ice and wind than the conductor \
                         carries — slightly less sag and swing (unsafe side). Results say which \
                         was used.",
                    )
                    .small(),
                );
            } else {
                ui.label(
                    egui::RichText::new(
                        "The effect of the choice: about T/EA, 0.1 %, of the ice and wind (iced \
                         and windy cases about +0.03 to +0.07 % on the tension, sags within a few \
                         mm on the example line); still air is the same either way. The closed \
                         forms (ruling span) always take the stretched metre.",
                    )
                    .small()
                    .weak(),
                );
            }
            ui.separator();
            ui.strong("Sections in the blocks");
            ui.label(
                "How a section in the blocks shares its wire between its spans in each weather \
                 case (its length is the same either way: every span sagged at the stringing \
                 tension with the strings plumb).",
            );
            for (choice, text, help) in [
                (
                    ohl_line::InTheBlocks::Sheaves,
                    "Over sheaves (default)",
                    "The wire runs over a frictionless sheave at every suspension clamp: its \
                     tension along the wire is the same on both sides, the strings swing along \
                     the line to balance (the sheave's size and friction left out)",
                ),
                (
                    ohl_line::InTheBlocks::EqualH,
                    "Simplified (equal H)",
                    "Every span the same horizontal tension, the strings plumb along the line \
                     (the model before 2026-10-25): on unequal or inclined spans the spans' \
                     tensions differ from the sheaves' by the load per metre times the \
                     difference of their ends' heights above their low points",
                ),
            ] {
                ui.radio_value(&mut draft.blocks, choice, text)
                    .on_hover_text(help);
            }
            ui.label(
                egui::RichText::new(
                    "The effect of the choice on the example line: the spans' horizontal \
                     tensions within about 0.6 %, sags within about 4 cm, clipping offsets \
                     within about 1 mm. Clipped-in sections are not affected.",
                )
                .small()
                .weak(),
            );
            if let Err(why) = r.model.can_edit() {
                crate::menu::error_label(ui, format!("read-only: {why}"));
            }
            ui.horizontal(|ui| {
                ok = ui
                    .add_enabled(
                        r.model.can_edit().is_ok() && draft != r.model.line.solver,
                        egui::Button::new("OK"),
                    )
                    .clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
    if ok {
        let command = r
            .model
            .editing
            .as_ref()
            .map(|e| e.session.solver_command(draft));
        if let Ok(command) = command {
            r.perform(crate::edit::Action::Command {
                what: "calculation options".into(),
                command,
            });
        }
    }
    r.solver_window = (open && !ok && !cancel).then_some(draft);
}

/// Line › Criteria › Creep-Stretch: the weather case for the final after creep
/// and the weather case(s) for the final after load; OK makes it one
/// command (undoable) and every section is solved again.
pub(super) fn creep_stretch_window(ctx: &egui::Context, r: &mut Ready) {
    let Some(mut draft) = r.creep_stretch_window.clone() else {
        return;
    };
    let names: Vec<String> = r.model.cases.iter().map(|c| c.case.name.clone()).collect();
    let (mut ok, mut cancel) = (false, false);
    let mut open = true;
    egui::Window::new("Creep-Stretch")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(520.0)
        .show(ctx, |ui| {
            ui.strong("Weather case for final after creep");
            ui.label(
                egui::RichText::new(
                    "Creep: the wire stretches permanently under tension over time, so its \
                     tensions after creep are lower than initially. Normally an everyday case the \
                     wires see over long periods.",
                )
                .small()
                .weak(),
            );
            egui::ComboBox::from_id_salt("creep-case")
                .selected_text(draft.creep_case.as_deref().unwrap_or("— none —"))
                .width(300.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut draft.creep_case, None, "— none —");
                    for n in &names {
                        ui.selectable_value(&mut draft.creep_case, Some(n.clone()), n);
                    }
                });
            ui.add_space(10.0);
            ui.strong("Weather case(s) for final after load");
            ui.label(
                egui::RichText::new(
                    "A short exposure to an extreme load stretches the wire permanently. With \
                     several cases, the one giving the most stretch is used, for each wire.",
                )
                .small()
                .weak(),
            );
            let mut remove = None;
            egui::Grid::new("load-cases").striped(true).show(ui, |ui| {
                for (k, case) in draft.load_cases.iter_mut().enumerate() {
                    ui.label(format!("{}", k + 1));
                    egui::ComboBox::from_id_salt(("load-case", k))
                        .selected_text(case.as_str())
                        .width(300.0)
                        .show_ui(ui, |ui| {
                            for n in &names {
                                ui.selectable_value(case, n.clone(), n);
                            }
                        });
                    if ui.small_button("🗑").on_hover_text("Remove").clicked() {
                        remove = Some(k);
                    }
                    ui.end_row();
                }
            });
            if let Some(k) = remove {
                draft.load_cases.remove(k);
            }
            if ui.button("+ Add a weather case").clicked()
                && let Some(n) = names.iter().find(|n| !draft.load_cases.contains(n))
            {
                draft.load_cases.push(n.clone());
            }
            ui.separator();
            if let Err(why) = r.model.can_edit() {
                crate::menu::error_label(ui, format!("read-only: {why}"));
            }
            ui.horizontal(|ui| {
                ok = ui
                    .add_enabled(r.model.can_edit().is_ok(), egui::Button::new("OK"))
                    .clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
    if ok {
        draft.load_cases.dedup();
        let command = r
            .model
            .editing
            .as_ref()
            .map(|e| e.session.creep_stretch_command(draft.clone()));
        if let Ok(command) = command {
            r.perform(crate::edit::Action::Command {
                what: "creep-stretch weather cases".into(),
                command,
            });
        }
    }
    r.creep_stretch_window = (open && !ok && !cancel).then_some(draft);
}

/// Versions › History: the operation log; admins reverse entries.
pub(super) fn history_window(ctx: &egui::Context, r: &mut Ready) {
    if !r.history_open {
        return;
    }
    let mut open = true;
    let mut op = None;
    egui::Window::new("History")
        .open(&mut open)
        .default_width(760.0)
        .show(ctx, |ui| op = crate::remote::history_ui(ui, &r.model));
    r.history_open = open;
    if let Some((what, op)) = op {
        r.project_op(&what, op);
    }
}

/// The Import / Export LAS/LAZ windows and the job they started.
pub(super) fn survey_io_windows(ctx: &egui::Context, r: &mut Ready) {
    if let Some(d) = &mut r.survey_import {
        let mut open = true;
        if let Some(job) = d.ui(ctx, &r.model, &mut open) {
            r.survey_job = Some(job);
            open = false;
        }
        if !open {
            r.survey_import = None;
        }
    }
    if let Some(d) = &mut r.survey_export {
        let mut open = true;
        if let Some(job) = d.ui(ctx, &r.model, &mut open) {
            r.survey_job = Some(job);
            open = false;
        }
        if !open {
            r.survey_export = None;
        }
    }
    if let Some(job) = &mut r.survey_job
        && crate::survey_io::job_window(ctx, job)
    {
        // An import succeeded: the project again, with its new points.
        r.reopen = job.reopen && matches!(job.done, Some(Ok(_)));
        r.survey_job = None;
    }
}

/// A LiDAR class: code, name, roles, points loaded, points in the cloud.
pub(super) type SurveyClass = (u8, String, Vec<String>, u64, Option<u64>);

/// The LiDAR classes of the point cloud (every class of the cloud, loaded
/// or not).
pub(super) fn survey_classes(m: &Model) -> Vec<SurveyClass> {
    let mut loaded: std::collections::BTreeMap<u8, u64> = Default::default();
    for (_, c) in &m.points {
        *loaded.entry(*c).or_default() += 1;
    }
    let mut total: std::collections::BTreeMap<u8, Option<u64>> =
        loaded.keys().map(|c| (*c, None)).collect();
    for (c, n) in &m.class_counts {
        total.insert(*c, Some(*n));
    }
    total
        .into_iter()
        .map(|(code, t)| {
            let def = m.scheme.class(code);
            (
                code,
                def.map_or_else(|| "unknown".to_owned(), |c| c.name.clone()),
                def.map_or_else(Vec::new, |c| c.roles.clone()),
                loaded.get(&code).copied().unwrap_or(0),
                t,
            )
        })
        .collect()
}

/// Groups of classes by role, for showing or hiding them together.
pub(super) const SURVEY_GROUPS: [(&str, &[&str]); 6] = [
    ("ground", &["ground"]),
    ("vegetation", &["vegetation"]),
    ("buildings", &["building", "substation", "tank"]),
    (
        "wires and structures",
        &[
            "conductor",
            "wire",
            "jumper",
            "structure",
            "pole",
            "insulator",
        ],
    ),
    ("traffic", &["road", "railway", "ropeway", "bridge"]),
    ("noise", &["noise"]),
];

/// The "Survey data display options" window: a tick per LiDAR class, kept
/// open while classes are ticked and unticked (each change shows at once);
/// all, none, invert, by group, a filter.
/// Height of the class list of the survey window, points (it scrolls).
pub(super) const SURVEY_LIST_HEIGHT: f32 = 340.0;

/// What the "Survey data display options" window changes, kept there
/// until applied: reloading the points at every tick lagged, and a drag
/// over several classes missed some of them.
#[derive(Debug, Clone, PartialEq)]
pub struct SurveyDraft {
    /// Filter of the class list.
    pub filter: String,
    /// Classes hidden.
    pub hidden: std::collections::BTreeSet<u8>,
    /// Points shown near the camera, %.
    pub percent: f64,
}

impl SurveyDraft {
    /// What the model shows now.
    pub fn of(model: &Model) -> Self {
        Self {
            filter: String::new(),
            hidden: model.hidden_classes.clone(),
            percent: model.points_percent,
        }
    }

    /// Whether it differs from what the model shows.
    pub(super) fn differs(&self, model: &Model) -> bool {
        self.hidden != model.hidden_classes || self.percent != model.points_percent
    }
}

pub(super) fn survey_window(ctx: &egui::Context, r: &mut Ready) {
    let Some(mut draft) = r.survey_window.clone() else {
        return;
    };
    let classes = survey_classes(&r.model);
    let mut open = true;
    let (mut apply, mut revert) = (false, false);
    let pending = draft.differs(&r.model);
    let mut changed = false;
    let SurveyDraft {
        filter,
        hidden,
        percent,
    } = &mut draft;
    let paint = &mut r.survey_paint;
    egui::Window::new("Survey data display options")
        .open(&mut open)
        .collapsible(false)
        .resizable([true, false])
        .default_width(520.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Classes shown:");
                if ui.button("All").clicked() {
                    hidden.clear();
                    changed = true;
                }
                if ui.button("None").clicked() {
                    *hidden = classes.iter().map(|c| c.0).collect();
                    changed = true;
                }
                if ui.button("Invert").clicked() {
                    for c in &classes {
                        if !hidden.remove(&c.0) {
                            hidden.insert(c.0);
                        }
                    }
                    changed = true;
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("By kind:");
                for (name, roles) in SURVEY_GROUPS {
                    let members: Vec<u8> = classes
                        .iter()
                        .filter(|c| c.2.iter().any(|r| roles.iter().any(|g| r.contains(g))))
                        .map(|c| c.0)
                        .collect();
                    if members.is_empty() {
                        continue;
                    }
                    let all_shown = members.iter().all(|c| !hidden.contains(c));
                    let mut on = all_shown;
                    if ui
                        .checkbox(&mut on, name)
                        .on_hover_text(format!("{} classes", members.len()))
                        .changed()
                    {
                        for c in members {
                            if on {
                                hidden.remove(&c);
                            } else {
                                hidden.insert(c);
                            }
                        }
                        changed = true;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Filter:");
                ui.text_edit_singleline(filter);
            });
            ui.label(
                egui::RichText::new(
                    "Click a class to show or hide it; press and drag up or down to give \
                     every class passed the same state.",
                )
                .small()
                .weak(),
            );
            ui.separator();
            let f = filter.to_lowercase();
            let (pressed, down) =
                ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down()));
            let pointer = ui.input(|i| i.pointer.interact_pos());
            if !down {
                *paint = None;
            }
            // The list scrolls inside the window (a cloud may have dozens
            // of classes).
            // A fixed height: sized from the space left, the window grew a
            // little every frame until it filled the screen.
            egui::ScrollArea::vertical()
                .auto_shrink([false, true])
                .max_height(SURVEY_LIST_HEIGHT)
                .show(ui, |ui| {
                    for (code, name, roles, loaded, total) in &classes {
                        let text = format!("{code} {name}");
                        if !f.is_empty()
                            && !text.to_lowercase().contains(&f)
                            && !roles.iter().any(|r| r.contains(&f))
                        {
                            continue;
                        }
                        let mut on = !hidden.contains(code);
                        let row = ui
                            .horizontal(|ui| {
                                ui.checkbox(&mut on, "");
                                ui.label(&text);
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{} points",
                                                total.unwrap_or(*loaded)
                                            ))
                                            .small()
                                            .weak(),
                                        );
                                    },
                                );
                            })
                            .response;
                        // The whole row on top of its checkbox: press to
                        // flip it, drag over others to give them its state.
                        let row = ui
                            .interact(
                                row.rect,
                                ui.id().with(("survey-row", code)),
                                egui::Sense::click_and_drag(),
                            )
                            .on_hover_text(roles.join(", "));
                        let shown = !hidden.contains(code);
                        let target = if pressed && row.hovered() {
                            *paint = Some(!shown);
                            Some(!shown)
                        } else {
                            match (*paint, pointer) {
                                (Some(t), Some(p))
                                    if down && p.y >= row.rect.top() && p.y < row.rect.bottom() =>
                                {
                                    Some(t)
                                }
                                _ => None,
                            }
                        };
                        if let Some(t) = target
                            && t != shown
                        {
                            if t {
                                hidden.remove(code);
                            } else {
                                hidden.insert(*code);
                            }
                            changed = true;
                        }
                    }
                });
            ui.separator();
            // How many points: near the camera; farther away the level of
            // detail and the render distance thin them further.
            ui.horizontal(|ui| {
                ui.label("Points shown near the camera:");
                let slider = ui.add(
                    egui::Slider::new(percent, 0.1..=100.0)
                        .logarithmic(true)
                        .suffix(" % of the cloud"),
                );
                changed |= slider.changed();
            });
            ui.label(
                egui::RichText::new(
                    "100 %: every point of the survey near the camera (classes of up to \
                     20 000 points are always whole). Farther away the Level of detail \
                     table's shares and render distance reduce them.",
                )
                .small()
                .weak(),
            );
            ui.separator();
            ui.horizontal(|ui| {
                let pending = pending || changed;
                if ui
                    .add_enabled(pending, egui::Button::new("Apply"))
                    .on_hover_text("Show the classes and the share of points chosen")
                    .clicked()
                {
                    apply = true;
                }
                if ui
                    .add_enabled(pending, egui::Button::new("Revert"))
                    .on_hover_text("Back to what is shown")
                    .clicked()
                {
                    revert = true;
                }
                if pending {
                    ui.colored_label(ui.visuals().warn_fg_color, "changes not applied");
                }
            });
        });
    if revert {
        let filter = draft.filter.clone();
        draft = SurveyDraft::of(&r.model);
        draft.filter = filter;
    }
    if apply {
        if draft.hidden != r.model.hidden_classes {
            r.model.hidden_classes = draft.hidden.clone();
            r.view3d.dirty.insert(Layer::Points);
        }
        if draft.percent != r.model.points_percent {
            // The 3D view reads the points again at the new share (this
            // user's choice, remembered on this computer).
            r.model.points_percent = draft.percent;
        }
    }
    // Closed: changes not applied are dropped.
    r.survey_window = open.then_some(draft);
}
