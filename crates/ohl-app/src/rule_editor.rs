// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Clearances tab's **rule editor**: the rules of the rule file chosen
//! (ground, LiDAR classes, the line's own wires, other lines), each
//! distance set as one distance, a table by voltage or a base plus an
//! amount per kV ([`crate::requirement_ui`]), rules added and removed;
//! and **Load preset…**: the presets of clearance rules of the enabled
//! design-code plugins (their optional `rule-presets` export), previewed,
//! then merged into the file (a rule of the same name replaced, the
//! others added) or replacing its rules.
//!
//! The file is edited in place (`toml_edit`): its comments, the fields
//! the editor does not show and the order stay as they are. Each change
//! is written once the mouse is released, when every rule is complete;
//! for a project's rule file it is kept in the project as one logged
//! command (Undo takes it back, as any rule edit).

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use ohl_clearance::RuleSet;
use ohl_clearance::rules::Requirement;
use ohl_io::codes::RulePresetChecked;
use toml_edit::{DocumentMut, Item};

use crate::app::Ready;

/// A rule as the editor shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct EditRule {
    /// Its name.
    pub name: String,
    /// Its target as written (`ground`, `points`, `same_line`,
    /// `other_lines`).
    pub target: String,
    /// Vertical clearance.
    pub vertical: Option<Requirement>,
    /// Radial clearance.
    pub radial: Option<Requirement>,
}

/// The rule presets of one design-code plugin, or why there are none.
#[derive(Debug, Clone)]
pub struct PluginPresets {
    /// The plugin's name.
    pub plugin: String,
    /// Its presets.
    pub presets: Result<Vec<RulePresetChecked>, String>,
}

/// How a preset goes into the rule file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Load {
    /// A rule of the same name replaced, the others added.
    Merge,
    /// The file's rules replaced by the preset's.
    Replace,
}

/// The "Load preset…" window, and the presets every window offers.
#[derive(Default)]
pub struct PresetsWindow {
    /// Shown.
    pub open: bool,
    /// The plugins were asked (read again with [`PresetsWindow::read`]).
    pub asked: bool,
    /// The presets read, per plugin.
    pub list: Vec<PluginPresets>,
    /// The preset shown (plugin, preset).
    pub chosen: Option<(usize, usize)>,
    job: Option<Receiver<Vec<PluginPresets>>>,
}

impl PresetsWindow {
    /// Reads the presets of the plugins in `dirs` (name, directory) on a
    /// worker thread, with the parameter file `params` (none: empty).
    pub fn read(&mut self, dirs: Vec<(String, PathBuf)>, params: Option<PathBuf>) {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let text = params
                .and_then(|p| std::fs::read_to_string(p).ok())
                .unwrap_or_default();
            let host = ohl_plugins::WasmHost::new();
            let out = dirs
                .into_iter()
                .map(|(plugin, dir)| PluginPresets {
                    plugin,
                    presets: match &host {
                        Ok(h) => ohl_io::codes::rule_presets(h, &dir, &text),
                        Err(e) => Err(e.to_string()),
                    },
                })
                .collect();
            let _ = tx.send(out);
        });
        self.job = Some(rx);
        self.list.clear();
        self.chosen = None;
    }

    /// Whether the plugins are being read.
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// Takes the worker's answer (waits up to `wait`); true when it came.
    pub fn poll(&mut self, wait: std::time::Duration) -> bool {
        let Some(rx) = &self.job else {
            return false;
        };
        match rx.recv_timeout(wait) {
            Ok(list) => {
                self.list = list;
                self.job = None;
                true
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => false,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                self.job = None;
                true
            }
        }
    }

    /// The preset shown.
    pub fn preset(&self) -> Option<(&str, &RulePresetChecked)> {
        let (p, k) = self.chosen?;
        let pp = self.list.get(p)?;
        let preset = pp.presets.as_ref().ok()?.get(k)?;
        Some((pp.plugin.as_str(), preset))
    }
}

/// The editor of the rule file chosen in the Clearances tab.
#[derive(Default)]
pub struct RuleEditor {
    /// The file edited.
    pub file: Option<PathBuf>,
    doc: Option<DocumentMut>,
    /// Its rules, in the file's order.
    pub rules: Vec<EditRule>,
    /// Why the file cannot be edited, if so.
    pub error: Option<String>,
    /// The last thing done.
    pub status: Option<(String, bool)>,
    /// A change not written yet.
    pub dirty: bool,
    confirm: Option<usize>,
    /// When the file was read or written (undo writes it again).
    stamp: Option<std::time::SystemTime>,
    /// The "Load preset…" window.
    pub presets: PresetsWindow,
}

/// A requirement as a TOML value (`"7 m"`, an inline table).
fn requirement_item(r: &Requirement) -> Item {
    let doc: DocumentMut = format!("x = {}", r.to_toml())
        .parse()
        .expect("a requirement's text reads");
    doc["x"].clone()
}

fn modified(p: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

impl RuleEditor {
    /// Reads rule file `file` (when another file is chosen, or the file
    /// changed on disk: an undo).
    pub fn follow(&mut self, file: Option<&Path>, defaults: &ohl_clearance::rules::RuleDefaults) {
        let same = self.file.as_deref() == file;
        if same && (self.dirty || file.is_none_or(|f| modified(f) == self.stamp)) {
            return;
        }
        self.file = file.map(Path::to_path_buf);
        self.confirm = None;
        if !same {
            self.status = None;
        }
        self.load(defaults);
    }

    fn load(&mut self, defaults: &ohl_clearance::rules::RuleDefaults) {
        (self.doc, self.rules, self.error) = (None, Vec::new(), None);
        let Some(file) = &self.file else {
            return;
        };
        self.stamp = modified(file);
        let text = match std::fs::read_to_string(file) {
            Ok(t) => t,
            Err(e) => {
                self.error = Some(format!("{}: {e}", file.display()));
                return;
            }
        };
        match self.take(&text, defaults) {
            Ok(()) => {}
            Err(e) => self.error = Some(e),
        }
    }

    /// Takes a rule file's text.
    fn take(
        &mut self,
        text: &str,
        defaults: &ohl_clearance::rules::RuleDefaults,
    ) -> Result<(), String> {
        let set = RuleSet::parse_with(text, defaults)?;
        let doc: DocumentMut = text.parse().map_err(|e| format!("{e}"))?;
        let targets: Vec<String> = doc
            .get("rule")
            .and_then(Item::as_array_of_tables)
            .map(|a| {
                a.iter()
                    .map(|t| {
                        t.get("target")
                            .and_then(Item::as_str)
                            .unwrap_or("")
                            .to_owned()
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.rules = set
            .rules
            .into_iter()
            .zip(targets)
            .map(|(r, target)| EditRule {
                name: r.name,
                target,
                vertical: r.vertical,
                radial: r.radial,
            })
            .collect();
        self.doc = Some(doc);
        Ok(())
    }

    /// What keeps the rules from being written, if anything.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        for r in &self.rules {
            if r.vertical.is_none() && r.radial.is_none() {
                out.push(format!(
                    "\"{}\": give a vertical and/or a radial clearance",
                    r.name
                ));
            }
            for (what, q) in [("vertical", &r.vertical), ("radial", &r.radial)] {
                if let Some(p) = q.as_ref().and_then(Requirement::problem) {
                    out.push(format!("\"{}\": {what}: {p}", r.name));
                }
            }
        }
        out
    }

    /// The rules written into the document (names and distances; the
    /// rest of each table kept, a value's trailing comment too).
    fn sync(&mut self) {
        let Some(doc) = self.doc.as_mut() else {
            return;
        };
        let Some(list) = doc.get_mut("rule").and_then(Item::as_array_of_tables_mut) else {
            return;
        };
        for (t, r) in list.iter_mut().zip(&self.rules) {
            if t.get("name").and_then(Item::as_str) != Some(r.name.as_str()) {
                t["name"] = toml_edit::value(r.name.clone());
            }
            for (key, q) in [("vertical", &r.vertical), ("radial", &r.radial)] {
                match q {
                    Some(q) => {
                        let old = t.get(key);
                        // Unchanged: left as written.
                        let same = old
                            .and_then(|i| {
                                let text = format!("format = \"ohl-clearance-rules\"\nversion = 1\n[[rule]]\nname = \"x\"\ntarget = \"ground\"\n{key} = {}\n", i.to_string().trim());
                                RuleSet::parse(&text).ok()
                            })
                            .is_some_and(|s| {
                                let x = &s.rules[0];
                                (if key == "vertical" { &x.vertical } else { &x.radial }).as_ref()
                                    == Some(q)
                            });
                        if same {
                            continue;
                        }
                        let mut item = requirement_item(q);
                        if let (Some(Item::Value(old)), Item::Value(new)) = (old, &mut item) {
                            *new.decor_mut() = old.decor().clone();
                        }
                        t[key] = item;
                    }
                    None => {
                        t.remove(key);
                    }
                }
            }
        }
    }

    /// The file's text as edited.
    pub fn text(&mut self) -> Option<String> {
        self.sync();
        self.doc.as_ref().map(DocumentMut::to_string)
    }

    /// Writes the file (when every rule is complete); a project's file is
    /// kept in the project (one logged command). True when written.
    pub fn save(&mut self, model: &mut crate::model::Model) -> bool {
        let problems = self.problems();
        if !problems.is_empty() {
            self.status = Some((format!("not written yet: {}", problems.join("; ")), true));
            return false;
        }
        let (Some(file), Some(text)) = (self.file.clone(), self.text()) else {
            return false;
        };
        if let Err(e) = RuleSet::parse_with(&text, &(&model.line.defaults).into()) {
            self.status = Some((format!("not written: {e}"), true));
            return false;
        }
        if let Err(e) = std::fs::write(&file, &text) {
            self.status = Some((format!("{}: {e}", file.display()), true));
            return false;
        }
        self.stamp = modified(&file);
        let in_project = model
            .library_dir
            .as_ref()
            .is_some_and(|w| file.starts_with(w));
        self.status = Some(if in_project {
            match model.store_libraries() {
                Ok(_) => ("kept in the project (Undo takes it back)".to_owned(), false),
                Err(e) => (format!("keeping the rules in the project: {e}"), true),
            }
        } else {
            (format!("written to {}", file.display()), false)
        });
        true
    }

    /// Adds a rule of `target` (its distance to set).
    pub fn add_rule(&mut self, target: &str) {
        let Some(doc) = self.doc.as_mut() else {
            return;
        };
        let name = format!("Rule {}", self.rules.len() + 1);
        let mut t = toml_edit::Table::new();
        t["name"] = toml_edit::value(name.clone());
        t["target"] = toml_edit::value(target);
        let list = doc
            .entry("rule")
            .or_insert_with(|| Item::ArrayOfTables(toml_edit::ArrayOfTables::new()));
        if let Some(list) = list.as_array_of_tables_mut() {
            list.push(t);
        }
        let wire = matches!(target, "same_line" | "other_lines");
        self.rules.push(EditRule {
            name,
            target: target.to_owned(),
            vertical: (!wire).then_some(Requirement::Fixed(0.0)),
            radial: wire.then_some(Requirement::Fixed(0.0)),
        });
    }

    /// Removes rule `k`.
    pub fn remove_rule(&mut self, k: usize) {
        if let Some(list) = self
            .doc
            .as_mut()
            .and_then(|d| d.get_mut("rule"))
            .and_then(Item::as_array_of_tables_mut)
            && k < list.len()
        {
            list.remove(k);
            self.rules.remove(k);
        }
    }

    /// Takes preset `p` into the document: merged (a rule of the same name
    /// replaced, the others added) or replacing the rules (and the file's
    /// `[wind]` / `[check]` by the preset's, if it has them). The
    /// preset's tables go in as it wrote them, comments included.
    pub fn load_preset(
        &mut self,
        p: &RulePresetChecked,
        how: Load,
        defaults: &ohl_clearance::rules::RuleDefaults,
    ) -> Result<String, String> {
        self.sync();
        let Some(doc) = self.doc.as_mut() else {
            return Err("no rule file to load into".into());
        };
        let source: DocumentMut = p.rules_text.parse().map_err(|e| format!("{e}"))?;
        let empty = toml_edit::ArrayOfTables::new();
        let theirs = source
            .get("rule")
            .and_then(Item::as_array_of_tables)
            .unwrap_or(&empty);
        let (mut replaced, mut added) = (0, 0);
        match how {
            Load::Replace => {
                // Their places in the preset's text mean nothing here:
                // after the file's own tables.
                let mut list = theirs.clone();
                for t in list.iter_mut() {
                    t.set_position(None);
                }
                doc["rule"] = Item::ArrayOfTables(list);
                for key in ["wind", "check"] {
                    if let Some(x) = source.get(key) {
                        let mut x = x.clone();
                        if let Some(t) = x.as_table_mut() {
                            t.set_position(None);
                        }
                        doc[key] = x;
                    }
                }
                added = theirs.len();
            }
            Load::Merge => {
                let list = doc
                    .entry("rule")
                    .or_insert_with(|| Item::ArrayOfTables(toml_edit::ArrayOfTables::new()))
                    .as_array_of_tables_mut()
                    .ok_or("rule is not a list of tables")?;
                for t in theirs.iter() {
                    let name = t.get("name").and_then(Item::as_str);
                    let at = list
                        .iter()
                        .position(|o| o.get("name").and_then(Item::as_str) == name);
                    let mut t = t.clone();
                    match at.and_then(|k| list.get_mut(k)) {
                        Some(o) => {
                            // In the replaced rule's place.
                            t.set_position(o.position());
                            *o = t;
                            replaced += 1;
                        }
                        None => {
                            t.set_position(None);
                            list.push(t);
                            added += 1;
                        }
                    }
                }
            }
        }
        let text = doc.to_string();
        self.take(&text, defaults)?;
        Ok(match how {
            Load::Replace => format!("rules replaced by the preset's {added}"),
            Load::Merge => format!("{replaced} rule(s) replaced, {added} added"),
        })
    }
}

/// The Clearances tab's editor of the rule file chosen.
pub fn ui(ui: &mut egui::Ui, r: &mut Ready) {
    let file = r
        .clearances
        .chosen
        .and_then(|k| r.clearances.files.get(k))
        .cloned();
    let defaults: ohl_clearance::rules::RuleDefaults = (&r.model.line.defaults).into();
    r.rule_editor.follow(file.as_deref(), &defaults);
    let voltages = ohl_clearance::zone::line_voltages(&r.model.line);
    let editable = r.model.can_edit().is_ok() || r.model.library_dir.is_none();
    egui::CollapsingHeader::new(egui::RichText::new("Edit rules").strong())
        .id_salt("rule-editor")
        .show(ui, |ui| {
            let e = &mut r.rule_editor;
            ui.label(
                egui::RichText::new(
                    "Each distance: one distance, a table \"up to kV: distance\" or a base plus \
                     an amount per kV, so one rule covers every voltage (the preview gives the \
                     distance at the project's voltages; wire rules take the pair's voltage). \
                     Written when the mouse is released and every rule is complete.",
                )
                .small()
                .weak(),
            );
            if let Some(err) = &e.error {
                crate::menu::error_label(ui, err.as_str());
                return;
            }
            if e.file.is_none() {
                ui.label("Choose a rule file above.");
            }
            ui.horizontal(|ui| {
                ui.add_enabled_ui(editable && e.doc.is_some(), |ui| {
                    ui.menu_button("Add rule", |ui| {
                        for (target, what) in [
                            ("ground", "to the ground"),
                            ("same_line", "between the wires of this line"),
                            ("other_lines", "to the wires of other lines"),
                        ] {
                            if ui.button(what).clicked() {
                                e.add_rule(target);
                                e.dirty = true;
                                ui.close();
                            }
                        }
                        ui.label(
                            egui::RichText::new(
                                "Rules to LiDAR classes: the Vertical / Radial clearances \
                                 windows, or a preset.",
                            )
                            .small()
                            .weak(),
                        );
                    });
                });
                if ui
                    .button("Load preset…")
                    .on_hover_text(
                        "Presets of clearance rules from the design-code plugins turned on \
                         (Plugins › Plugin manager): previewed, then merged into this file or \
                         replacing its rules",
                    )
                    .clicked()
                {
                    e.presets.open = true;
                    e.presets.list.clear();
                }
            });
            if e.doc.is_none() {
                return;
            }
            let mut remove = None;
            egui::ScrollArea::vertical()
                .id_salt("rule-editor-rules")
                .max_height(260.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    // One block per rule (the Clearances tab is narrow).
                    for (k, rule) in e.rules.iter_mut().enumerate() {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                e.dirty |= ui
                                    .add_enabled(
                                        editable,
                                        egui::TextEdit::singleline(&mut rule.name)
                                            .desired_width(150.0),
                                    )
                                    .changed();
                                ui.label(
                                    egui::RichText::new(rule.target.replace('_', " "))
                                        .small()
                                        .weak(),
                                );
                                if ui
                                    .add_enabled(editable, egui::Button::new("Remove").small())
                                    .clicked()
                                {
                                    remove = Some(k);
                                }
                            });
                            for (m, q) in [
                                ("vertical", &mut rule.vertical),
                                ("radial", &mut rule.radial),
                            ] {
                                ui.add_enabled_ui(editable, |ui| {
                                    ui.vertical(|ui| {
                                        let mut on = q.is_some();
                                        if ui
                                            .checkbox(&mut on, m)
                                            .on_hover_text(format!("A {m} clearance"))
                                            .changed()
                                        {
                                            *q = on.then_some(Requirement::Fixed(0.0));
                                            e.dirty = true;
                                        }
                                        if let Some(q) = q.as_mut() {
                                            e.dirty |=
                                                crate::requirement_ui::requirement_ui_stacked(
                                                    ui,
                                                    ("rule-editor", m, k),
                                                    q,
                                                    &voltages,
                                                );
                                        }
                                    });
                                });
                            }
                        });
                    }
                });
            if remove.is_some() {
                e.confirm = remove;
            }
            if let Some(k) = e.confirm
                && k < e.rules.len()
            {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!("Remove the rule \"{}\"?", e.rules[k].name),
                    );
                    if ui.button("Remove it").clicked() {
                        e.remove_rule(k);
                        e.confirm = None;
                        e.dirty = true;
                    }
                    if ui.button("Keep").clicked() {
                        e.confirm = None;
                    }
                });
            }
            if let Some((text, error)) = &e.status {
                if *error {
                    crate::menu::error_label(ui, text.as_str());
                } else {
                    crate::menu::message(ui, egui::RichText::new(text.as_str()).small(), None);
                }
            }
        });
    // Written once the mouse is released.
    if r.rule_editor.dirty && !ui.ctx().input(|i| i.pointer.any_down()) {
        r.rule_editor.dirty = false;
        if r.rule_editor.save(&mut r.model)
            && let Some(k) = r.clearances.chosen
        {
            r.clearances.choose(k);
        }
    }
}

/// The presets of every enabled design-code plugin, read once (on a
/// worker thread) for every window offering them: the rule editor's Load
/// preset…, the vertical and radial clearance windows, the vegetation
/// panel ([`preset_dropdown`]).
pub fn ensure_presets(ctx: &egui::Context, r: &mut Ready, plugins: &crate::plugins::Plugins) {
    let w = &mut r.rule_editor.presets;
    if w.list.is_empty() && !w.busy() && !w.asked {
        let dirs: Vec<(String, PathBuf)> = crate::codes::choices(plugins)
            .into_iter()
            .filter(|c| c.problem.is_none())
            .map(|c| (c.name, c.dir))
            .collect();
        w.asked = true;
        if !dirs.is_empty() {
            w.read(dirs, r.design_code.params.clone());
        }
    }
    w.poll(std::time::Duration::ZERO);
    if w.busy() {
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }
}

/// A dropdown of the presets of every enabled design-code plugin
/// (`list`), grouped by plugin, each with its description on hover; only
/// those `keep` keeps are offered (a plugin with none says so). Shown
/// even with one preset. True when another was chosen.
pub fn preset_dropdown(
    ui: &mut egui::Ui,
    id: &str,
    list: &[PluginPresets],
    keep: impl Fn(&RulePresetChecked) -> bool,
    chosen: &mut Option<(usize, usize)>,
) -> bool {
    let before = *chosen;
    let shown = chosen
        .and_then(|(p, k)| {
            let pp = list.get(p)?;
            let x = pp.presets.as_ref().ok()?.get(k)?;
            Some(format!("{}: {}", pp.plugin, x.name))
        })
        .unwrap_or_else(|| "Choose a preset…".into());
    egui::ComboBox::from_id_salt(id)
        .selected_text(shown)
        .width(300.0)
        .height(420.0)
        .show_ui(ui, |ui| {
            if list.is_empty() {
                ui.label(
                    egui::RichText::new(
                        "No design-code plugin turned on (Plugins › Plugin manager)",
                    )
                    .weak(),
                );
            }
            for (p, pp) in list.iter().enumerate() {
                ui.label(egui::RichText::new(&pp.plugin).strong());
                match &pp.presets {
                    Err(e) => {
                        crate::menu::error_label(ui, e.as_str());
                    }
                    Ok(presets) => {
                        let mut any = false;
                        for (k, x) in presets.iter().enumerate().filter(|(_, x)| keep(x)) {
                            any = true;
                            let r = ui
                                .selectable_label(*chosen == Some((p, k)), format!("   {}", x.name))
                                .on_hover_text(if x.description.is_empty() {
                                    x.name.as_str()
                                } else {
                                    x.description.as_str()
                                });
                            if r.clicked() {
                                *chosen = Some((p, k));
                            }
                        }
                        if !any {
                            ui.label(egui::RichText::new("   no preset for this").small().weak());
                        }
                    }
                }
            }
        });
    *chosen != before
}

/// The "Load preset…" window: the rule presets of the enabled design-code
/// plugins (read with the Design code window's parameter file, if one is
/// chosen), one previewed, then merged or replacing the rules.
pub fn presets_window(ctx: &egui::Context, r: &mut Ready, plugins: &crate::plugins::Plugins) {
    if !r.rule_editor.presets.open {
        return;
    }
    ensure_presets(ctx, r, plugins);
    let voltages = ohl_clearance::zone::line_voltages(&r.model.line);
    let target = r
        .rule_editor
        .file
        .as_ref()
        .and_then(|f| f.file_name())
        .map(|n| n.to_string_lossy().into_owned());
    let mut open = true;
    let mut load = None;
    egui::Window::new("Rule presets")
        .open(&mut open)
        .default_width(720.0)
        .show(ctx, |ui| {
            let w = &mut r.rule_editor.presets;
            ui.label(
                egui::RichText::new(
                    "Presets of clearance rules from the design-code plugins turned on, read with \
                     the parameter file of Criteria › Design code (if one is chosen). Check every \
                     value against the regulation you work to.",
                )
                .small()
                .weak(),
            );
            if w.busy() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("reading the plugins…");
                });
            }
            ui.horizontal(|ui| {
                ui.label("Preset");
                preset_dropdown(
                    ui,
                    "rule-preset-choice",
                    &w.list,
                    |p| !p.rules.rules.is_empty(),
                    &mut w.chosen,
                );
            });
            ui.separator();
            ui.vertical(|ui| {
                let Some((plugin, p)) = w.preset() else {
                    ui.label("Choose a preset to see its rules.");
                    return;
                };
                ui.strong(format!("{plugin}: {}", p.name));
                crate::menu::message(ui, p.description.as_str(), None);
                egui::ScrollArea::vertical()
                    .id_salt("rule-preset-preview")
                    .max_height(300.0)
                    .show(ui, |ui| {
                        for rule in &p.rules.rules {
                            ui.label(egui::RichText::new(&rule.name).strong());
                            for (m, q) in [("vertical", &rule.vertical), ("radial", &rule.radial)] {
                                if let Some(q) = q {
                                    ui.label(format!("{m}: {q}"));
                                    ui.label(
                                        egui::RichText::new(crate::requirement_ui::preview_text(
                                            q, &voltages,
                                        ))
                                        .small(),
                                    );
                                }
                            }
                            if !rule.cases.is_empty() {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "cases: {}",
                                        rule.cases.join(", ")
                                    ))
                                    .small()
                                    .weak(),
                                );
                            }
                        }
                    });
                if let Some(wc) = &p.weather {
                    let names: Vec<&str> = wc.cases.iter().map(|c| c.name.as_str()).collect();
                    ui.label(
                        egui::RichText::new(format!(
                            "its weather cases (added to the model where missing): {}",
                            names.join(", ")
                        ))
                        .small(),
                    );
                }
                ui.separator();
                match &target {
                    Some(file) => {
                        ui.horizontal(|ui| {
                            if ui
                                .button(format!("Merge into {file}"))
                                .on_hover_text("A rule of the same name replaced, the others added")
                                .clicked()
                            {
                                load = Some(Load::Merge);
                            }
                            if ui.button(format!("Replace the rules of {file}")).clicked() {
                                load = Some(Load::Replace);
                            }
                        });
                    }
                    None => {
                        ui.label("Choose a rule file in the Clearances tab first (or add one).");
                    }
                }
            });
        });
    if !open {
        r.rule_editor.presets.open = false;
    }
    if let Some(how) = load {
        apply_preset(r, how);
    }
}

/// Loads the preset shown into the rule file chosen (one write, one
/// command), and its weather cases missing from the model into the
/// model's weather cases.
pub fn apply_preset(r: &mut Ready, how: Load) {
    let Some((plugin, p)) = r
        .rule_editor
        .presets
        .preset()
        .map(|(a, b)| (a.to_owned(), b.clone()))
    else {
        return;
    };
    let defaults: ohl_clearance::rules::RuleDefaults = (&r.model.line.defaults).into();
    let done = r.rule_editor.load_preset(&p, how, &defaults);
    let mut text = match done {
        Ok(m) => format!("preset {plugin} {}: {m}", p.name),
        Err(e) => {
            r.rule_editor.status = Some((e, true));
            return;
        }
    };
    if r.rule_editor.save(&mut r.model) {
        if let Some(k) = r.clearances.chosen {
            r.clearances.choose(k);
        }
    } else {
        return;
    }
    if let Some(wc) = &p.weather {
        match preset_cases(r, &plugin, &p.name, &p.weather_text, wc) {
            Ok(m) if !m.is_empty() => text = format!("{text}; {m}"),
            Ok(_) => {}
            Err(e) => text = format!("{text}; weather cases: {e}"),
        }
    }
    r.rule_editor.status = Some((text, false));
}

/// The preset's weather cases missing from the model (by name): added to
/// the model's weather case file (or the preset's file used when the
/// model has none) — one change of the project's weather case file, one
/// undo step. What was done.
pub(crate) fn preset_cases(
    r: &mut Ready,
    plugin: &str,
    name: &str,
    text: &str,
    wc: &ohl_weather::WeatherCases,
) -> Result<String, String> {
    let have: Vec<String> = r.model.cases.iter().map(|c| c.case.name.clone()).collect();
    if wc.cases.iter().all(|c| have.contains(&c.name)) {
        return Ok(String::new());
    }
    let mut out = crate::clearance_zones::Out::default();
    let stem = ohl_io::codes::rule_preset_stem(plugin, name);
    let m = match r.model.sources.weather.clone() {
        Some(to) => {
            let tmp = std::env::temp_dir().join(format!("{stem}-{}.ohlwc", std::process::id()));
            std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
            let added = crate::clearance_zones::add_missing_cases(&to, &tmp, &have);
            let _ = std::fs::remove_file(&tmp);
            let added = added?;
            out.weather_saved = Some(to);
            format!("weather cases added to the model: {}", added.join(", "))
        }
        None => {
            let dir = match (&r.model.library_dir, &r.model.editing) {
                (Some(w), Ok(_)) => w.join("weather"),
                _ => ohl_core::files::dir_of(&r.model.sources.file),
            };
            std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            let to = dir.join(format!("{stem}.{}", ohl_weather::WEATHER_FILE_EXTENSION));
            std::fs::write(&to, text).map_err(|e| format!("{}: {e}", to.display()))?;
            out.weather_use = Some(to);
            "its weather cases are used for the model".to_owned()
        }
    };
    // A preset's weather cases (no violations to draw).
    crate::app::clearance_zone_out(r, ohl_clearance::Measure::Vertical, out);
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"format = "ohl-clearance-rules"
version = 1
name = "Test"

# The ground (a comment kept).
[[rule]]
name = "Ground"
target = "ground"
vertical = "7 m" # trailing comment kept
cases = ["hot"]

[[rule]]
name = "Wires"
target = "same_line"
radial = "3 m"
"#;

    /// The three forms written into the file in place: comments, other
    /// fields and an unchanged rule's text kept; read back exactly.
    #[test]
    fn distances_are_written_in_place() {
        let d = ohl_clearance::rules::RuleDefaults::default();
        let mut e = RuleEditor::default();
        e.take(FILE, &d).unwrap();
        assert_eq!(e.rules.len(), 2);
        assert_eq!(e.text().unwrap(), FILE, "nothing changed: the same text");
        let table = Requirement::ByVoltage(vec![(0.0, 5.0), (150.0, 6.0), (420.0, 7.5)]);
        let formula = Requirement::Formula {
            base: 3.0,
            per_kv: 0.01,
            above_kv: 0.0,
        };
        e.rules[0].vertical = Some(table.clone());
        e.rules[1].radial = Some(formula.clone());
        let text = e.text().unwrap();
        assert!(text.contains("# The ground (a comment kept)."), "{text}");
        assert!(text.contains("# trailing comment kept"), "{text}");
        assert!(text.contains("cases = [\"hot\"]"), "{text}");
        let set = RuleSet::parse(&text).unwrap();
        assert_eq!(set.rules[0].vertical, Some(table));
        assert_eq!(set.rules[1].radial, Some(formula));
        // An incomplete rule is not written.
        e.rules[1].radial = Some(Requirement::ByVoltage(vec![(220.0, 4.0), (110.0, 3.0)]));
        assert!(e.problems()[0].contains("increase"), "{:?}", e.problems());
        // Added and removed.
        e.add_rule("other_lines");
        assert_eq!(e.rules[2].radial, Some(Requirement::Fixed(0.0)));
        e.remove_rule(1);
        e.rules[1].radial = Some(Requirement::Fixed(4.0));
        let set = RuleSet::parse(&e.text().unwrap()).unwrap();
        assert_eq!(set.rules.len(), 2);
        assert_eq!(set.rules[1].name, "Rule 3");
    }

    fn preset(rules: &str) -> RulePresetChecked {
        let text = format!("format = \"ohl-clearance-rules\"\nversion = 1\nname = \"P\"\n{rules}");
        let list = ohl_io::codes::check_rule_presets(
            "code",
            vec![ohl_plugins::RulePreset {
                name: "p".into(),
                description: String::new(),
                rules: text,
                weather_cases: String::new(),
            }],
        )
        .unwrap();
        list[0].clone()
    }

    /// Merge replaces a rule of the same name and adds the others;
    /// Replace takes the preset's rules alone.
    #[test]
    fn presets_merge_or_replace() {
        let d = ohl_clearance::rules::RuleDefaults::default();
        let p = preset(
            "# from the code\n[[rule]]\nname = \"Ground\"\ntarget = \"ground\"\n\
             vertical = { base = \"7 m\", per_kv = \"0.01 m\", above_kv = 0 }\n\
             [[rule]]\nname = \"Roads\"\ntarget = \"ground\"\nvertical = \"8 m\"\n",
        );
        let mut e = RuleEditor::default();
        e.take(FILE, &d).unwrap();
        let m = e.load_preset(&p, Load::Merge, &d).unwrap();
        assert_eq!(m, "1 rule(s) replaced, 1 added");
        let names: Vec<&str> = e.rules.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Ground", "Wires", "Roads"]);
        assert_eq!(
            e.rules[0].vertical.as_ref().unwrap().at(380.0),
            Ok(7.0 + 3.8)
        );
        let mut e = RuleEditor::default();
        e.take(FILE, &d).unwrap();
        e.load_preset(&p, Load::Replace, &d).unwrap();
        let names: Vec<&str> = e.rules.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Ground", "Roads"]);
        assert!(e.text().unwrap().contains("name = \"Test\""));
    }
}
