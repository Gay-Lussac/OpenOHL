// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Clearances: the check run every frame, the windows' requests, the rule
//! files, the Clearances panel.

use super::*;

/// Clearances: takes a finished check, starts one when the model changed
/// (with "check after edits" on) or no check ran yet. Its violations are
/// drawn while the Clearances tab is open (docked or moved too), taken off
/// the views once it is closed, back when it is opened again (as the
/// vegetation panel's layers).
pub(super) fn clearance_frame(ctx: &egui::Context, r: &mut Ready, tab_open: bool) {
    let arrived = r.clearances.poll();
    let shown = tab_open && r.clearances.report.is_some();
    if (arrived || r.clearances_drawn != Some(shown))
        && (shown || r.marks_from == MarksFrom::Clearances)
    {
        r.marks_from = MarksFrom::Clearances;
        let f = if shown { r.clearances.findings() } else { &[] };
        r.view3d.marks = f.iter().map(|f| (f.at, f.target)).collect();
        r.plan.marks = f.iter().map(|f| f.at).collect();
        r.profile.marks.clone_from(&r.view3d.marks);
        r.view3d.dirty.insert(Layer::Clearances);
    }
    r.clearances_drawn = Some(shown);
    // Checks run on demand (Run, Checks menu), or after every edit
    // when asked.
    if r.clearances.stale && r.clearances.auto && r.clearances.report.is_some() {
        start_clearance_check(ctx, r);
    }
}

/// What every clearance shown is measured to.
pub(crate) const SURFACE_NOTE: &str =
    "Clearances to the conductor's surface (iced in a case with ice).";

/// What a vertical or radial clearance window asks: its violations drawn
/// (layer "Clearance violations"), a span gone to.
impl Ready {
    /// What the Distance between conductors page asks: a report saved, the
    /// result's weather case shown, its span gone to, its line drawn.
    pub(super) fn phase_distance_out(&mut self, out: crate::phase_distance::Out) {
        if let Some((what, command)) = out.apply {
            self.perform(crate::edit::Action::Command { what, command });
        }
        if let Some(name) = &out.case
            && let Some(k) = self.model.cases.iter().position(|c| &c.case.name == name)
            && k != self.case
        {
            self.set_case(k);
        }
        if let Some(target) = out.go {
            self.go_to(target);
        }
        if let Some(m) = out.measure {
            // The view on the line's middle, close enough to read.
            let mid = [0, 1, 2].map(|i| (m.0[i] + m.1[i]) / 2.0);
            if let Some(cam) = self.view3d.camera.as_mut() {
                cam.target = mid;
                cam.distance = cam.distance.min(60.0);
            }
            self.plan.centre_on(mid);
            self.view3d.measure = Some(m);
        }
    }

    /// What the Performed checks page asks: a report opened in its check's
    /// page, a change applied.
    pub(crate) fn check_library_out(&mut self, out: crate::check_library::Out) {
        if let Some(r) = out.open {
            match r.check.as_str() {
                crate::phase_distance::CHECK_ID => {
                    if let Err(e) = self.phase_distance.load(&r, &self.model) {
                        self.model.log.push(e);
                    }
                }
                crate::vegetation::CHECK_ID => {
                    if let Err(e) = self.vegetation.load_saved(&r, &self.model) {
                        self.status = Some((e, true));
                    }
                }
                crate::safety_volume::CHECK_ID => {
                    if let Err(e) = self.safety_volume.load_saved(&r, &self.model) {
                        self.status = Some((e, true));
                    }
                }
                other => self
                    .model
                    .log
                    .push(format!("no page for the check \"{other}\" here")),
            }
        }
        if let Some((what, command)) = out.apply {
            self.perform(crate::edit::Action::Command { what, command });
        }
    }
}

/// What the Wire tensions window asks: go to a span, its preset's
/// weather cases added, its marks drawn (the spans above their allowed
/// tension, else the worst; at the most tensioned support) or taken off.
pub(crate) fn wire_tensions_out(r: &mut Ready, out: crate::wire_tensions::Out) {
    if let Some(target) = out.go {
        r.go_to(target);
    }
    if let Some((plugin, p)) = out.plugin_preset
        && let Some(wc) = &p.weather
        && let Err(e) = crate::rule_editor::preset_cases(r, &plugin, &p.name, &p.weather_text, wc)
    {
        r.status = Some((
            format!("preset {plugin} {}: weather cases: {e}", p.name),
            true,
        ));
    }
    if let Some(marks) = out.marks {
        if marks.is_empty() {
            if r.marks_from == MarksFrom::Tensions {
                r.plan.marks.clear();
                r.view3d.marks.clear();
                r.profile.marks.clear();
                r.view3d.dirty.insert(Layer::Clearances);
            }
        } else {
            r.marks_from = MarksFrom::Tensions;
            r.plan.marks.clone_from(&marks);
            r.view3d.marks = marks.iter().map(|p| (*p, *p)).collect();
            r.profile.marks.clone_from(&r.view3d.marks);
            r.view3d.dirty.insert(Layer::Clearances);
        }
    }
}

pub(crate) fn clearance_zone_out(
    r: &mut Ready,
    measure: ohl_clearance::Measure,
    out: crate::clearance_zones::Out,
) {
    // A preset's weather cases: the model reads its weather cases again.
    if let Some(p) = out.weather_saved {
        r.library_saved(
            crate::library::LibraryKind::WeatherCases,
            crate::library::PageAction::Saved(p),
        );
    }
    if let Some(p) = out.weather_use {
        r.library_saved(
            crate::library::LibraryKind::WeatherCases,
            crate::library::PageAction::UseForModel(p),
        );
    }
    if let Some((plugin, p)) = out.plugin_preset
        && let Some(wc) = &p.weather
        && let Err(e) = crate::rule_editor::preset_cases(r, &plugin, &p.name, &p.weather_text, wc)
    {
        r.status = Some((
            format!("preset {plugin} {}: weather cases: {e}", p.name),
            true,
        ));
    }
    if let Some(anchor) = out.guide {
        r.status = Some(
            match crate::help::open_section(crate::clearance_zones::GUIDE, anchor) {
                Ok(m) => (format!("user guide opened: {m}"), false),
                Err(e) => (e, true),
            },
        );
    }
    if out.hide_marks && r.marks_from == MarksFrom::Zone(measure) {
        // Its window closed: its violations off the views.
        r.plan.marks.clear();
        r.view3d.marks.clear();
        r.profile.marks.clear();
        r.view3d.dirty.insert(Layer::Clearances);
    }
    if let Some(marks) = out.marks {
        r.marks_from = MarksFrom::Zone(measure);
        r.plan.marks = marks.iter().map(|m| m.0).collect();
        r.profile.marks.clone_from(&marks);
        r.view3d.marks = marks;
        r.view3d.dirty.insert(Layer::Clearances);
    }
    if let Some((target, at)) = out.go {
        r.go_to(target);
        if let Some(cam) = r.view3d.camera.as_mut() {
            cam.target = at;
            cam.distance = cam.distance.min(80.0);
        }
        r.plan.centre_on(at);
    }
}

pub(super) fn start_clearance_check(ctx: &egui::Context, r: &mut Ready) {
    r.clearances.set_defaults(&r.model.line.defaults);
    // The check needs its weather case solved, and its ground.
    r.model.ensure_solved(r.case);
    r.model.ensure_check_ground();
    let m = &r.model;
    let Some(solved) = m.cases.get(r.case) else {
        return;
    };
    let scope = m.scope();
    r.clearances.start(ctx, |rules| crate::clearance::Input {
        // The surfaces the ground rules name (read on the check's thread).
        surfaces: crate::model::NamedSurfaces::of(
            m,
            rules.iter().filter_map(|r| r.surface.clone()),
        ),
        line: m.line.clone(),
        case: solved.case.name.clone(),
        // The route worked on only (View › Work on route).
        shapes: match &scope {
            Some(on) => solved
                .shapes
                .iter()
                .filter(|s| on.sections.contains(&s.section))
                .cloned()
                .collect(),
            None => solved.shapes.clone(),
        },
        jumpers: match &scope {
            Some(on) => solved
                .jumpers
                .iter()
                .filter(|j| on.jumpers.contains(&j.jumper))
                .cloned()
                .collect(),
            None => solved.jumpers.clone(),
        },
        rules,
        scheme: m.scheme.clone(),
        cloud: m.cloud.clone(),
        ground: m.ground_for_checks(),
        corridor: m.sources.corridor,
    });
}

/// The three ways to check clearances, in words (the Checks menu and the
/// Clearances page).
pub const CLEARANCES_ABOUT: &str = "To the ground and LiDAR points: rules typed in the \
     windows below. Between the wires of this line: the rules of a rule file, run by Check \
     wire clearances.";

/// Checks › Wire clearances › Add a rule file: a clearance rule file
/// (.ohlr) chosen on the computer, kept in the project (next to a line
/// file) and chosen on the Clearances page.
pub(super) fn add_rule_file(r: &mut Ready) {
    let Some(from) = rfd::FileDialog::new()
        .add_filter(
            "Clearance rules",
            &ohl_core::files::open_extensions(ohl_clearance::rules::RULE_FILE_EXTENSION),
        )
        .pick_file()
    else {
        return;
    };
    r.status = Some(match keep_rule_file(&mut r.model, &from) {
        Ok(to) => {
            r.clearances
                .refresh(&r.model, r.model.sources.rules.as_deref());
            if let Some(k) = r
                .clearances
                .files
                .iter()
                .position(|p| p.file_name() == to.file_name())
            {
                r.clearances.choose(k);
            }
            r.open_page = Some((Tab::Clearances, None));
            (
                format!("rule file {}", crate::menu::file_label(&r.model, &to)),
                false,
            )
        }
        Err(e) => (e, true),
    });
}

/// Copies rule file `from` among the project's rules (its working copy,
/// kept in the project), else next to the line file; checked first.
pub(super) fn keep_rule_file(
    model: &mut crate::model::Model,
    from: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    ohl_clearance::RuleSet::load(from)?;
    let name = from.file_name().ok_or("no file name")?;
    let dir = match (&model.library_dir, &model.editing) {
        (Some(w), Ok(_)) => w.join("rules"),
        _ => ohl_core::files::dir_of(&model.sources.file),
    };
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let to = dir.join(name);
    std::fs::copy(from, &to).map_err(|e| format!("{}: {e}", to.display()))?;
    model.store_libraries()?;
    Ok(to)
}

/// The Clearances tab: rule file, the violations of the shown case (click
/// one to go there).
pub(super) fn clearance_panel(ui: &mut egui::Ui, r: &mut Ready) {
    // Run in the footer at the bottom left, always visible (owner,
    // 2026-10-21); the rest scrolls above it.
    let mut check =
        crate::check_help::body(ui, "clearances-page", |ui| clearance_panel_body(ui, r));
    if !r.clearances.files.is_empty() {
        crate::check_help::footer(ui, "clearances-page", |ui| {
            let c = &mut r.clearances;
            check |= crate::check_help::run_button(
                ui,
                !c.running(),
                "Check wire clearances: the rules of this file, the weather case shown",
                None,
            );
            ui.checkbox(&mut c.auto, "after every edit");
            if c.running() {
                ui.spinner();
            }
        });
    }
    if check {
        r.clearances.stale = true;
        start_clearance_check(ui.ctx(), r);
    }
}

/// The Clearances tab above its footer: true to run the check (Run again).
pub(super) fn clearance_panel_body(ui: &mut egui::Ui, r: &mut Ready) -> bool {
    r.clearances.set_defaults(&r.model.line.defaults);
    if r.clearances.files.is_empty() {
        ui.label(egui::RichText::new("No clearance rule file in the project yet.").strong());
        ui.label(
            "The clearances between the wires of this line and to exclusion zones are \
             checked by the rules of a clearance rule file (.ohlr): add one, then Check wire \
             clearances (Checks menu or the quick bar). Other lines: `ohl line --other-line`.",
        );
        if ui
            .button("Add a rule file…")
            .on_hover_text(
                "A clearance rule file (.ohlr) kept in the project; examples/starter/rules has \
                 one with invented values to learn with",
            )
            .clicked()
        {
            add_rule_file(r);
        }
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(
                "To the ground and the LiDAR points: the vertical and radial clearance windows \
                 (rules typed there, starting with the ground's).",
            )
            .small()
            .weak(),
        );
        ui.horizontal(|ui| {
            if ui.button("Vertical clearances…").clicked() {
                r.vertical_clearances.show(&r.model);
            }
            if ui.button("Radial clearances…").clicked() {
                r.radial_clearances.show(&r.model);
            }
        });
        return false;
    }
    let c = &mut r.clearances;
    let name = |p: &std::path::Path| {
        p.file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned())
    };
    let mut chosen = c.chosen;
    egui::ComboBox::from_id_salt("rule-file")
        .selected_text(
            chosen
                .and_then(|k| c.files.get(k))
                .map_or("choose rules…".into(), |p| name(p)),
        )
        .width(220.0)
        .show_ui(ui, |ui| {
            for (k, p) in c.files.iter().enumerate() {
                ui.selectable_value(&mut chosen, Some(k), name(p))
                    .on_hover_text(p.display().to_string());
            }
        });
    if chosen != c.chosen
        && let Some(k) = chosen
    {
        c.choose(k);
    }
    let mut check = false;
    match &c.rules {
        Some(Err(e)) => {
            crate::menu::error_label(ui, e);
        }
        Some(Ok(rules)) => {
            ui.label(egui::RichText::new(format!("{} rules", rules.len())).small());
        }
        None => {}
    }
    crate::rule_editor::ui(ui, r);
    let c = &mut r.clearances;
    if c.stale
        && !c.running()
        && c.report.is_some()
        && crate::revision::banner(ui, "the model or the case shown", true)
    {
        check = true;
    }
    let mut go: Option<crate::clearance::Finding> = None;
    match &c.report {
        Some(Err(e)) => {
            crate::menu::error_label(ui, e);
        }
        Some(Ok(rep)) => {
            let n = rep.findings.len();
            let head = format!(
                "case {}: {n} violation{} ({:.1?})",
                rep.case,
                if n == 1 { "" } else { "s" },
                rep.took
            );
            if n == 0 {
                ui.colored_label(crate::menu::ok_color(ui), head);
            } else {
                ui.colored_label(ui.visuals().error_fg_color, head);
            }
            for (rule, text) in &rep.rules {
                ui.label(egui::RichText::new(format!("{rule}: {text}")).small());
            }
            for note in &rep.notes {
                ui.label(egui::RichText::new(note).small().weak());
            }
            ui.label(egui::RichText::new(SURFACE_NOTE).small().weak());
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for f in &rep.findings {
                    let other = f
                        .other
                        .as_ref()
                        .map_or(String::new(), |o| format!(" – {}", o.describe()));
                    let class = f.class.map_or(String::new(), |k| {
                        format!(
                            ", class {k}{}",
                            r.model
                                .scheme
                                .class(k)
                                .map_or(String::new(), |d| format!(" {}", d.name))
                        )
                    });
                    let text = format!(
                        "{}{other} — {}: {} {:.2} m < {:.2} m{class}{}",
                        f.wire.describe(),
                        f.rule,
                        f.measure,
                        f.distance,
                        f.required,
                        if f.count > 1 {
                            format!(" ({} below)", f.count)
                        } else {
                            String::new()
                        }
                    );
                    let selected = r.selection == f.wire;
                    if ui
                        .selectable_label(selected, egui::RichText::new(text).small())
                        .clicked()
                    {
                        go = Some(f.clone());
                    }
                }
            });
        }
        None if c.running() => {
            ui.label("checking…");
        }
        None => {}
    }
    if let Some(f) = go {
        r.selection = f.wire;
        r.view3d.dirty.insert(Layer::Selection);
        if let Some(cam) = r.view3d.camera.as_mut() {
            cam.target = f.at;
            cam.distance = cam.distance.min(80.0);
        }
        r.plan.centre_on(f.at);
    }
    check
}
