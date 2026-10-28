// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The Clearances window's **rule editor**: the rules of the rule file chosen
//! (ground, LiDAR classes, the line's own wires, other lines) in the
//! shared rule table ([`crate::check_ui::RuleTable`], 2026-10-28): each
//! distance set as one distance, a table by voltage or a base plus an
//! amount per kV ([`crate::requirement_ui`]); the weather cases, and for
//! the rules between wires the case pairs, the pair's voltage, the
//! circuits and the exclusions ([`RuleFields`]); rules added, copied and
//! removed; **Preset ▾**: the presets of clearance rules of the enabled
//! design-code plugins (their optional `rule-presets` export) and of the
//! project, previewed, then merged into the file (a rule of the same name
//! replaced, the others added) or replacing its rules. Its weather cases
//! are matched to the project's by physics first, then by name
//! ([`crate::preset_cases`]).
//!
//! Each rule's **voltage filter** ("only for these voltages", empty:
//! every wire) is shown with its distances: optional with one distance,
//! behind "Only for some voltages…" with a table or base + per kV, which
//! already depend on the voltage (owner, 2026-10-21;
//! [`crate::requirement_ui::voltage_filter_ui`]). **Merge rules by
//! voltage** ([`merge_groups`]) turns rules alike but for one voltage and
//! a plain distance into one table rule without a filter, previewed
//! first, undoable.
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
    /// Only the wires of these nominal voltages, kV (0: earthed); empty:
    /// every wire.
    pub voltages: Vec<f64>,
    /// Its weather cases, case pairs and the wire rules' fields
    /// (owner, 2026-10-28: edited in the window, not only in the file).
    pub fields: RuleFields,
    /// [`EditRule::fields`] as the file has them (a field is written only
    /// when it changed: the file's own text kept otherwise).
    written: RuleFields,
    /// The rest of the rule as read (every field but the name, the
    /// distances, the voltages and [`EditRule::fields`]), to find rules
    /// alike ([`merge_groups`]).
    rest: String,
    /// Which table of the file it is (0: a new rule; a copy keeps its
    /// original's until it is written).
    id: u64,
}

/// The fields of a rule besides its name, distances and voltages that the
/// rule table edits (the others stay as the file has them).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RuleFields {
    /// The weather cases it applies to (`*` `?` wildcards; empty: every
    /// case).
    pub cases: Vec<String>,
    /// Wire rules: the first wire in the first case, the second in the
    /// second (empty: both in the same case).
    pub case_pairs: Vec<[String; 2]>,
    /// Wire rules: the voltage of a pair.
    pub pair_voltage: ohl_clearance::rules::PairVoltage,
    /// Wires of this line: the pairs checked by their circuits.
    pub circuits: ohl_clearance::rules::CircuitFilter,
    /// Wires of this line: wires of the same circuit and phase too.
    pub include_same_phase: bool,
    /// The wire within this horizontal distance of its ends left out, m.
    pub exclude_near_ends: f64,
    /// Spans ending at these structures left out (`*` `?` wildcards).
    pub exclude_spans_at: Vec<String>,
}

impl RuleFields {
    /// The fields of rule `r`.
    fn of(r: &ohl_clearance::Rule) -> Self {
        Self {
            cases: r.cases.clone(),
            case_pairs: r.case_pairs.clone(),
            pair_voltage: r.pair_voltage,
            circuits: r.circuits,
            include_same_phase: matches!(
                r.target,
                ohl_clearance::Target::SameLine {
                    include_same_phase: true
                }
            ),
            exclude_near_ends: r.exclude_near_ends,
            exclude_spans_at: r.exclude_spans_at.clone(),
        }
    }
}

impl EditRule {
    /// A new rule of `target` (`ground`, `same_line`, `other_lines`), its
    /// distance to set: vertical to the ground, radial between wires.
    pub fn new(name: &str, target: &str) -> Self {
        let wire = matches!(target, "same_line" | "other_lines");
        Self {
            name: name.to_owned(),
            target: target.to_owned(),
            vertical: (!wire).then_some(Requirement::Fixed(0.0)),
            radial: wire.then_some(Requirement::Fixed(0.0)),
            voltages: Vec::new(),
            fields: RuleFields::default(),
            written: RuleFields::default(),
            rest: String::new(),
            id: 0,
        }
    }

    /// Whether a distance of the rule depends on the voltage (a table or
    /// base + per kV).
    pub fn by_voltage(&self) -> bool {
        [&self.vertical, &self.radial]
            .into_iter()
            .flatten()
            .any(Requirement::by_voltage)
    }

    /// Whether it is a rule between wires (this line's or other lines').
    pub fn between_wires(&self) -> bool {
        matches!(self.target.as_str(), "same_line" | "other_lines")
    }
}

/// Rules that "Merge rules by voltage" turns into one: alike in every
/// field but the name, each for one voltage (a different one) with plain
/// distances, the same measures (vertical, radial) set. The indices of
/// each group of two or more, in the file's order.
pub fn merge_groups(rules: &[EditRule]) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (k, r) in rules.iter().enumerate() {
        let plain = [&r.vertical, &r.radial]
            .into_iter()
            .flatten()
            .all(|q| matches!(q, Requirement::Fixed(_)));
        if r.voltages.len() != 1 || !plain || (r.vertical.is_none() && r.radial.is_none()) {
            continue;
        }
        let alike = |g: &Vec<usize>| {
            let o = &rules[g[0]];
            o.rest == r.rest
                && o.fields == r.fields
                && o.target == r.target
                && o.vertical.is_some() == r.vertical.is_some()
                && o.radial.is_some() == r.radial.is_some()
                && !g
                    .iter()
                    .any(|&j| (rules[j].voltages[0] - r.voltages[0]).abs() < 1e-6)
        };
        match groups.iter_mut().find(|g| alike(g)) {
            Some(g) => g.push(k),
            None => groups.push(vec![k]),
        }
    }
    groups.retain(|g| g.len() > 1);
    groups
}

/// The table rule of a group ([`merge_groups`]): rows (voltage up to,
/// distance) by voltage, no voltage filter; the first rule's name (the
/// common one, when every name is the same).
pub fn merged(rules: &[EditRule], group: &[usize]) -> EditRule {
    let mut g: Vec<&EditRule> = group.iter().map(|&k| &rules[k]).collect();
    g.sort_by(|a, b| a.voltages[0].total_cmp(&b.voltages[0]));
    let table = |pick: fn(&EditRule) -> &Option<Requirement>| {
        pick(g[0]).as_ref().map(|_| {
            Requirement::ByVoltage(
                g.iter()
                    .map(|r| {
                        let d = match pick(r) {
                            Some(Requirement::Fixed(d)) => *d,
                            _ => 0.0,
                        };
                        (r.voltages[0], d)
                    })
                    .collect(),
            )
        })
    };
    let first = &rules[group[0]];
    EditRule {
        name: first.name.clone(),
        target: first.target.clone(),
        vertical: table(|r| &r.vertical),
        radial: table(|r| &r.radial),
        voltages: Vec::new(),
        fields: first.fields.clone(),
        written: first.written.clone(),
        rest: first.rest.clone(),
        id: first.id,
    }
}

/// What a merge changes for the wires of the line's voltages (and
/// earthed ones): per voltage, the distance before (its rule's, or not
/// checked) and after (the table's, or none above its last row).
pub fn merge_preview(rules: &[EditRule], group: &[usize], voltages: &[f64]) -> Vec<String> {
    let after = merged(rules, group);
    let mut kvs = vec![0.0];
    kvs.extend(voltages.iter().copied().filter(|v| *v > 0.0));
    kvs.dedup();
    let mut out = Vec::new();
    for (m, now, then) in [
        (
            "vertical",
            after.vertical.as_ref(),
            (|r| r.vertical.clone()) as fn(&EditRule) -> Option<Requirement>,
        ),
        ("radial", after.radial.as_ref(), |r: &EditRule| {
            r.radial.clone()
        }),
    ] {
        let Some(now) = now else { continue };
        for &kv in &kvs {
            let before = group
                .iter()
                .map(|&k| &rules[k])
                .find(|r| (r.voltages[0] - kv).abs() < 1e-6)
                .and_then(then)
                .map_or("not checked by these rules".to_owned(), |q| match q {
                    Requirement::Fixed(d) => format!("{d:.2} m"),
                    _ => "?".into(),
                });
            let after = match now.at(kv) {
                Ok(d) => format!("{d:.2} m"),
                Err(_) => "no distance (above the table: the check stops)".into(),
            };
            let changed = before != after;
            out.push(format!(
                "{m}, {}: {before}, merged {after}{}",
                crate::requirement_ui::voltage_name(kv),
                if changed { "  (changes)" } else { "" }
            ));
        }
    }
    out
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

/// The presets of the design-code plugins every check window offers
/// (*Preset ▾* of [`crate::check_ui::RuleTable`]), read once.
#[derive(Default)]
pub struct PresetsWindow {
    /// The plugins were asked (read again with [`PresetsWindow::read`]).
    pub asked: bool,
    /// The plugins asked (name, directory): read again when the enabled
    /// design-code plugins change.
    asked_for: Vec<(String, PathBuf)>,
    /// The presets read, per plugin, with the project's edits of their
    /// values ([`crate::preset_edits`]).
    pub list: Vec<PluginPresets>,
    /// The presets as the plugins give them.
    pub original: Vec<PluginPresets>,
    /// Presets the project's edits make invalid (kept as the plugin's).
    pub edit_errors: Vec<String>,
    /// The project and library revision the edits were applied for.
    applied: Option<(PathBuf, u64)>,
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
        self.original.clear();
        self.applied = None;
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
                self.original = list.clone();
                self.list = list;
                self.applied = None;
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
}

/// The editor of the rule file chosen in the Clearances window.
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
    /// The ids of the document's rule tables, in its order
    /// ([`EditRule`]'s id).
    ids: Vec<u64>,
    /// The last id given.
    next_id: u64,
    /// "Remove all rules" asked, waiting for its confirmation.
    confirm_all: bool,
    /// "Merge rules by voltage" asked: the group previewed.
    merge: Option<usize>,
    /// When the file was read or written (undo writes it again).
    stamp: Option<std::time::SystemTime>,
    /// The design-code plugins' presets.
    pub presets: PresetsWindow,
}

/// A requirement as a TOML value (`"7 m"`, an inline table).
fn requirement_item(r: &Requirement) -> Item {
    let doc: DocumentMut = format!("x = {}", r.to_toml())
        .parse()
        .expect("a required clearance's text reads");
    doc["x"].clone()
}

/// Writes into rule table `t` the [`RuleFields`] that differ from what
/// the file has (`written`); a field back at its default is removed.
fn write_fields(t: &mut toml_edit::Table, now: &RuleFields, written: &RuleFields, target: &str) {
    use ohl_clearance::rules::{CircuitFilter, PairVoltage};
    let strings = |v: &[String]| {
        let mut a = toml_edit::Array::new();
        for s in v {
            a.push(s.as_str());
        }
        a
    };
    if now.cases != written.cases {
        if now.cases.is_empty() {
            t.remove("cases");
        } else {
            t["cases"] = toml_edit::value(strings(&now.cases));
        }
    }
    if now.case_pairs != written.case_pairs {
        if now.case_pairs.is_empty() {
            t.remove("case_pairs");
        } else {
            let mut a = toml_edit::Array::new();
            for [x, y] in &now.case_pairs {
                a.push(strings(&[x.clone(), y.clone()]));
            }
            t["case_pairs"] = toml_edit::value(a);
        }
    }
    if now.pair_voltage != written.pair_voltage {
        match now.pair_voltage {
            PairVoltage::Higher => {
                t.remove("pair_voltage");
            }
            PairVoltage::Sum => t["pair_voltage"] = toml_edit::value("sum"),
            PairVoltage::Difference => t["pair_voltage"] = toml_edit::value("difference"),
        }
    }
    if now.circuits != written.circuits {
        match now.circuits {
            CircuitFilter::Any => {
                t.remove("circuits");
            }
            CircuitFilter::Same => t["circuits"] = toml_edit::value("same"),
            CircuitFilter::Different => t["circuits"] = toml_edit::value("different"),
        }
    }
    if now.include_same_phase != written.include_same_phase && target == "same_line" {
        if now.include_same_phase {
            t["include_same_phase"] = toml_edit::value(true);
        } else {
            t.remove("include_same_phase");
        }
    }
    if now.exclude_near_ends != written.exclude_near_ends {
        if now.exclude_near_ends > 0.0 {
            t["exclude_near_ends"] = toml_edit::value(format!("{} m", now.exclude_near_ends));
        } else {
            t.remove("exclude_near_ends");
        }
    }
    if now.exclude_spans_at != written.exclude_spans_at {
        if now.exclude_spans_at.is_empty() {
            t.remove("exclude_spans_at");
        } else {
            t["exclude_spans_at"] = toml_edit::value(strings(&now.exclude_spans_at));
        }
    }
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
        let first = self.next_id + 1;
        self.rules = set
            .rules
            .into_iter()
            .zip(targets)
            .enumerate()
            .map(|(k, (r, target))| {
                let fields = RuleFields::of(&r);
                let rest = format!(
                    "{:?}",
                    ohl_clearance::Rule {
                        name: String::new(),
                        vertical: None,
                        radial: None,
                        voltages: Vec::new(),
                        cases: Vec::new(),
                        case_pairs: Vec::new(),
                        pair_voltage: Default::default(),
                        circuits: Default::default(),
                        exclude_near_ends: 0.0,
                        exclude_spans_at: Vec::new(),
                        target: match r.target {
                            ohl_clearance::Target::SameLine { .. } => {
                                ohl_clearance::Target::SameLine {
                                    include_same_phase: false,
                                }
                            }
                            ref t => t.clone(),
                        },
                        ..r.clone()
                    }
                );
                EditRule {
                    name: r.name,
                    target,
                    vertical: r.vertical,
                    radial: r.radial,
                    voltages: r.voltages,
                    written: fields.clone(),
                    fields,
                    rest,
                    id: first + k as u64,
                }
            })
            .collect();
        self.next_id = first + self.rules.len() as u64;
        self.ids = self.rules.iter().map(|r| r.id).collect();
        self.merge = None;
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
            if r.fields
                .case_pairs
                .iter()
                .any(|[a, b]| a.trim().is_empty() || b.trim().is_empty())
            {
                out.push(format!(
                    "\"{}\": a case pair needs both weather cases",
                    r.name
                ));
            }
        }
        out
    }

    /// The rule tables of the document in the order of the rules: a rule
    /// added, copied or removed in the table (2026-10-28) changes the list
    /// of tables; each table keeps its text and place in the file while
    /// the order allows, a new or copied one goes after the file's tables.
    fn arrange(&mut self) {
        let ids: Vec<u64> = self.rules.iter().map(|r| r.id).collect();
        let Some(doc) = self.doc.as_mut() else {
            return;
        };
        if ids == self.ids && ids.iter().all(|i| *i != 0) {
            return;
        }
        let old: Vec<toml_edit::Table> = doc
            .get("rule")
            .and_then(Item::as_array_of_tables)
            .map(|a| a.iter().cloned().collect())
            .unwrap_or_default();
        let by_id: std::collections::HashMap<u64, &toml_edit::Table> =
            self.ids.iter().copied().zip(old.iter()).collect();
        let mut seen = std::collections::HashSet::new();
        let mut list = toml_edit::ArrayOfTables::new();
        let mut last: Option<isize> = None;
        let mut in_place = true;
        for r in &mut self.rules {
            let mut t = match by_id.get(&r.id) {
                Some(t) if seen.insert(r.id) => (*t).clone(),
                found => {
                    // New, or a copy: written whole from what it holds.
                    let mut t = found.map_or_else(toml_edit::Table::new, |t| (*t).clone());
                    if found.is_none() {
                        t["name"] = toml_edit::value(r.name.clone());
                        t["target"] = toml_edit::value(r.target.clone());
                        r.written = RuleFields::default();
                    }
                    t.set_position(None);
                    self.next_id += 1;
                    r.id = self.next_id;
                    seen.insert(r.id);
                    t
                }
            };
            // The file's order kept while it is the rules' order.
            match t.position() {
                Some(p) if in_place && last.is_none_or(|l| p > l) => last = Some(p),
                _ => {
                    in_place = false;
                    t.set_position(None);
                }
            }
            list.push(t);
        }
        if list.is_empty() {
            doc.remove("rule");
        } else {
            doc["rule"] = Item::ArrayOfTables(list);
        }
        self.ids = self.rules.iter().map(|r| r.id).collect();
    }

    /// The rules written into the document (names, distances, voltages and
    /// [`RuleFields`]; the rest of each table kept, a value's trailing
    /// comment too).
    fn sync(&mut self) {
        self.arrange();
        let Some(doc) = self.doc.as_mut() else {
            return;
        };
        let Some(list) = doc.get_mut("rule").and_then(Item::as_array_of_tables_mut) else {
            return;
        };
        for (t, r) in list.iter_mut().zip(self.rules.iter_mut()) {
            if t.get("name").and_then(Item::as_str) != Some(r.name.as_str()) {
                t["name"] = toml_edit::value(r.name.clone());
            }
            // The voltage filter: left as written while it is the same.
            let written: Option<Vec<f64>> = t.get("voltages").and_then(Item::as_array).map(|a| {
                a.iter()
                    .filter_map(|v| v.as_float().or(v.as_integer().map(|i| i as f64)))
                    .collect()
            });
            if written.as_deref().unwrap_or(&[]) != r.voltages.as_slice() {
                if r.voltages.is_empty() {
                    t.remove("voltages");
                } else {
                    let mut a = toml_edit::Array::new();
                    for v in &r.voltages {
                        if v.fract() == 0.0 && v.abs() < 1e9 {
                            a.push(*v as i64);
                        } else {
                            a.push(*v);
                        }
                    }
                    t["voltages"] = toml_edit::value(a);
                }
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
            write_fields(t, &r.fields, &r.written, &r.target);
            r.written = r.fields.clone();
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

    /// Adds a rule of `target` (its distance to set; the table's *Add
    /// rule* adds [`EditRule::new`]).
    #[cfg(test)]
    pub fn add_rule(&mut self, target: &str) {
        if self.doc.is_none() {
            return;
        }
        let name = format!("Rule {}", self.rules.len() + 1);
        self.rules.push(EditRule::new(&name, target));
    }

    /// Removes every rule (the file's other tables, `[wind]`, `[check]`,
    /// `[vegetation]`, stay).
    pub fn remove_all_rules(&mut self) {
        self.rules.clear();
    }

    /// Merges the rules of `group` ([`merge_groups`]) into one table rule
    /// without a voltage filter, in the first one's place (its comments
    /// and other fields kept); the others removed.
    pub fn merge_rules(&mut self, group: &[usize]) {
        if group.len() < 2 || group.iter().any(|&k| k >= self.rules.len()) {
            return;
        }
        let one = merged(&self.rules, group);
        let first = group[0];
        self.rules[first] = one;
        let mut rest: Vec<usize> = group[1..].to_vec();
        rest.sort_unstable();
        for &k in rest.iter().rev() {
            self.remove_rule(k);
        }
        self.merge = None;
    }

    /// Removes rule `k`.
    pub fn remove_rule(&mut self, k: usize) {
        if k < self.rules.len() {
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

/// The Clearances window's rules of the rule file chosen, in the shared
/// rule table (owner, 2026-10-28): one row per rule, the selected one's
/// fields below it — its distances (with their preview at the line's
/// voltages), its voltages, weather cases and, for rules between wires,
/// the case pairs, the pair's voltage, the circuits and the exclusions —;
/// one *Preset ▾*, Import / Export; *Merge rules by voltage…* and *Remove
/// all rules* of this window beside.
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
    ui.label(
        egui::RichText::new(
            "Each distance: one distance, a table \"up to kV: distance\" or a base plus an \
             amount per kV, so one rule covers every voltage (the preview gives the distance at \
             the project's voltages; wire rules take the pair's voltage). Written when the mouse \
             is released and every rule is complete.",
        )
        .small()
        .weak(),
    );
    {
        let e = &mut r.rule_editor;
        if let Some(err) = &e.error {
            crate::menu::error_label(ui, err.as_str());
            return;
        }
        if e.doc.is_none() {
            ui.label("Choose a rule file above.");
            return;
        }
        // This window's own: merge rules by voltage, remove them all.
        let groups = merge_groups(&e.rules);
        ui.horizontal_wrapped(|ui| {
            if !groups.is_empty()
                && ui
                    .add_enabled(editable, egui::Button::new("Merge rules by voltage…"))
                    .on_hover_text(
                        "Rules alike but for one voltage and a plain distance each, made one rule \
                         with a table by voltage and no voltage filter (previewed first; Edit › \
                         Undo takes it back)",
                    )
                    .clicked()
            {
                e.merge = Some(0);
            }
            if ui
                .add_enabled(
                    editable && !e.rules.is_empty(),
                    egui::Button::new("Remove all rules"),
                )
                .on_hover_text(
                    "Every rule of this file removed (after a confirmation; Edit › Undo brings \
                     them back); its wind and check settings stay",
                )
                .clicked()
            {
                e.confirm_all = true;
            }
        });
        if e.confirm_all {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!("Remove all {} rules of this file?", e.rules.len()),
                );
                if ui.button("Remove them").clicked() {
                    e.remove_all_rules();
                    e.confirm_all = false;
                    e.dirty = true;
                }
                if ui.button("Keep them").clicked() {
                    e.confirm_all = false;
                }
            });
        }
        merge_ui(ui, e, &voltages);
    }
    let cases: Vec<String> = r.model.cases.iter().map(|c| c.case.name.clone()).collect();
    let case_groups = crate::case_groups::groups(&r.model);
    let preview_voltages = voltages.clone();
    let e = &mut r.rule_editor;
    let t = crate::check_ui::RuleTable::new("clearance-rules", &mut e.rules, &voltages)
        .add(
            "To the ground",
            "a rule to the ground: set its distance",
            |n| EditRule::new(&format!("Rule {}", n + 1), "ground"),
        )
        .add(
            "Between the wires of this line",
            "a rule between the line's own wires: set its distance",
            |n| EditRule::new(&format!("Rule {}", n + 1), "same_line"),
        )
        .add(
            "To the wires of other lines",
            "a rule to other lines' wires (run by ohl line --other-line): set its distance",
            |n| EditRule::new(&format!("Rule {}", n + 1), "other_lines"),
        )
        .editable(editable)
        .presets(
            &r.model,
            &e.presets.list,
            crate::preset_cases::Part::Rules,
            move |set| {
                let mut lines = Vec::new();
                for (group, list) in crate::preset_cases::grouped(&set.rules) {
                    lines.push(format!("— {group} —"));
                    for x in list {
                        let mut parts = Vec::new();
                        for (m, q) in [("vertical", &x.vertical), ("radial", &x.radial)] {
                            if let Some(q) = q {
                                parts.push(format!(
                                    "{m} {}",
                                    crate::requirement_ui::preview_text(q, &preview_voltages)
                                ));
                            }
                        }
                        lines.push(format!(
                            "{}: {} ({})",
                            x.name,
                            parts.join("; "),
                            crate::preset_cases::cases_label(x)
                        ));
                    }
                }
                lines
            },
        )
        .ui(ui, |ui, k, rule| {
            edit_rule(ui, k, rule, &voltages, &cases, &case_groups)
        });
    e.dirty |= t.changed;
    if t.export
        && let Some(p) = crate::rule_table::export_file(
            &file
                .as_ref()
                .and_then(|f| f.file_name())
                .map_or("clearance-rules.ohlr".into(), |n| {
                    n.to_string_lossy().into_owned()
                }),
        )
    {
        e.status = Some(match e.text().map(|text| std::fs::write(&p, text)) {
            Some(Ok(())) => (format!("exported to {}", p.display()), false),
            Some(Err(err)) => (format!("{}: {err}", p.display()), true),
            None => ("no rule file".into(), true),
        });
    }
    if let Some((text, error)) = &e.status {
        if *error {
            crate::menu::error_label(ui, text.as_str());
        } else {
            crate::menu::message(ui, egui::RichText::new(text.as_str()).small(), None);
        }
    }
    if let Some(pick) = t.preset {
        apply_preset(r, pick);
    }
    if t.import
        && let Some(p) = crate::rule_table::import_file()
    {
        import(r, &p);
    }
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

/// The fields of rule `rule` (row `k`) below the rule table: its
/// distances, voltages, weather cases and, for a rule between wires, the
/// case pairs, the pair's voltage, the circuits and the exclusions. True
/// when one changed.
fn edit_rule(
    ui: &mut egui::Ui,
    k: usize,
    rule: &mut EditRule,
    voltages: &[f64],
    cases: &[String],
    groups: &[ohl_weather::CaseGroup],
) -> bool {
    use ohl_clearance::rules::{CircuitFilter, PairVoltage};
    let mut changed = false;
    egui::Grid::new(("clearance-rule", k))
        .num_columns(2)
        .spacing(crate::check_help::GRID_SPACING)
        .show(ui, |ui| {
            for (m, q) in [
                ("Vertical", &mut rule.vertical),
                ("Radial", &mut rule.radial),
            ] {
                let mut on = q.is_some();
                if ui
                    .checkbox(&mut on, m)
                    .on_hover_text(format!("A {} clearance", m.to_lowercase()))
                    .changed()
                {
                    *q = on.then_some(Requirement::Fixed(0.0));
                    changed = true;
                }
                match q.as_mut() {
                    Some(q) => {
                        changed |= crate::requirement_ui::requirement_ui(
                            ui,
                            ("rule-editor", m, k),
                            q,
                            voltages,
                        );
                    }
                    None => {
                        ui.label(egui::RichText::new("not checked").weak());
                    }
                }
                ui.end_row();
            }
            ui.label("Voltages");
            ui.horizontal(|ui| {
                let depends = rule.by_voltage();
                if !depends {
                    ui.label("only for");
                }
                changed |= crate::requirement_ui::voltage_filter_ui(
                    ui,
                    ("rule-editor", k),
                    &mut rule.voltages,
                    depends,
                    voltages,
                );
            });
            ui.end_row();
            ui.label("Weather cases");
            changed |= cases_menu(ui, &mut rule.fields.cases, cases, groups);
            ui.end_row();
            if !rule.between_wires() {
                return;
            }
            ui.label("Case pairs").on_hover_text(
                "The first wire in the first weather case, the second wire in the second (an iced \
                 upper wire over a lower one without ice); none: both in the same case",
            );
            ui.vertical(|ui| {
                let mut remove = None;
                for (j, pair) in rule.fields.case_pairs.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        for (side, name) in pair.iter_mut().enumerate() {
                            let shown = if name.is_empty() {
                                "choose…".to_owned()
                            } else {
                                name.clone()
                            };
                            egui::ComboBox::from_id_salt(("case-pair", k, j, side))
                                .selected_text(shown)
                                .show_ui(ui, |ui| {
                                    for c in cases {
                                        changed |=
                                            ui.selectable_value(name, c.clone(), c).changed();
                                    }
                                });
                            if side == 0 {
                                ui.label("over");
                            }
                        }
                        if ui
                            .small_button("×")
                            .on_hover_text("Remove this pair")
                            .clicked()
                        {
                            remove = Some(j);
                        }
                    });
                }
                if let Some(j) = remove {
                    rule.fields.case_pairs.remove(j);
                    changed = true;
                }
                if ui.small_button("Add a case pair").clicked() {
                    rule.fields.case_pairs.push([String::new(), String::new()]);
                    changed = true;
                }
            });
            ui.end_row();
            ui.label("Pair voltage");
            let name = |p: PairVoltage| match p {
                PairVoltage::Higher => "the higher of the two",
                PairVoltage::Sum => "the sum of both",
                PairVoltage::Difference => "the voltage between the two",
            };
            egui::ComboBox::from_id_salt(("pair-voltage", k))
                .selected_text(name(rule.fields.pair_voltage))
                .show_ui(ui, |ui| {
                    for p in [
                        PairVoltage::Higher,
                        PairVoltage::Sum,
                        PairVoltage::Difference,
                    ] {
                        changed |= ui
                            .selectable_value(&mut rule.fields.pair_voltage, p, name(p))
                            .changed();
                    }
                })
                .response
                .on_hover_text("The voltage a wire pair's distance is taken at");
            ui.end_row();
            if rule.target == "same_line" {
                ui.label("Circuits");
                ui.horizontal_wrapped(|ui| {
                    let name = |c: CircuitFilter| match c {
                        CircuitFilter::Any => "every pair",
                        CircuitFilter::Same => "same circuit (phase to phase)",
                        CircuitFilter::Different => "different circuits",
                    };
                    egui::ComboBox::from_id_salt(("pair-circuits", k))
                        .selected_text(name(rule.fields.circuits))
                        .show_ui(ui, |ui| {
                            for c in [
                                CircuitFilter::Any,
                                CircuitFilter::Same,
                                CircuitFilter::Different,
                            ] {
                                changed |= ui
                                    .selectable_value(&mut rule.fields.circuits, c, name(c))
                                    .changed();
                            }
                        });
                    changed |= ui
                        .checkbox(&mut rule.fields.include_same_phase, "the same phase too")
                        .on_hover_text(
                            "Wires of the same circuit and phase (its back and ahead spans, the \
                             jumpers) checked too",
                        )
                        .changed();
                });
                ui.end_row();
            }
            ui.label("Left out");
            ui.horizontal_wrapped(|ui| {
                ui.label("near the ends [m]");
                let r = crate::style::field(
                    ui,
                    egui::DragValue::new(&mut rule.fields.exclude_near_ends)
                        .range(0.0..=200.0)
                        .speed(0.1),
                )
                .on_hover_text(
                    "The wire within this horizontal distance of its ends (the clamps) is not \
                     checked (0: none)",
                );
                changed |= r.drag_stopped() || r.lost_focus();
                ui.label("spans at");
                let mut text = rule.fields.exclude_spans_at.join(", ");
                let r = ui
                    .add(
                        egui::TextEdit::singleline(&mut text)
                            .hint_text("structures, e.g. term_*")
                            .desired_width(160.0),
                    )
                    .on_hover_text(
                        "Spans ending at these structures are not checked (names, * ? \
                         wildcards, separated by commas)",
                    );
                if r.changed() {
                    rule.fields.exclude_spans_at = text
                        .split(',')
                        .map(|s| s.trim().to_owned())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                changed |= r.lost_focus();
            });
            ui.end_row();
        });
    changed
}

/// The weather cases of a rule ticked among the model's (none ticked:
/// every case), the case groups' buttons first; a menu that stays open.
fn cases_menu(
    ui: &mut egui::Ui,
    chosen: &mut Vec<String>,
    cases: &[String],
    groups: &[ohl_weather::CaseGroup],
) -> bool {
    let mut changed = false;
    crate::menu::options_menu(ui, &crate::rule_table::cases_words(chosen), |ui| {
        ui.label(
            egui::RichText::new("None ticked: every weather case")
                .small()
                .weak(),
        );
        changed |= crate::case_groups::pick_names(ui, groups, chosen);
        for c in cases {
            let mut on = chosen.iter().any(|p| ohl_core::pattern::matches(p, c));
            if ui.checkbox(&mut on, c).changed() {
                if on {
                    chosen.push(c.clone());
                } else {
                    chosen.retain(|p| !ohl_core::pattern::matches(p, c));
                }
                changed = true;
            }
        }
    });
    changed
}

/// A rule of the Clearances window as a row of the shared rule table.
impl crate::check_ui::RuleRow for EditRule {
    fn name(&self) -> &str {
        &self.name
    }

    fn name_mut(&mut self) -> &mut String {
        &mut self.name
    }

    fn summary(&self, _: &[f64]) -> crate::check_ui::Summary {
        let mut distance = Vec::new();
        for (m, q) in [("vertical", &self.vertical), ("radial", &self.radial)] {
            if let Some(q) = q {
                distance.push(format!("{m} {}", crate::rule_table::distance_words(q)));
            }
        }
        let mut cases = crate::rule_table::cases_words(&self.fields.cases);
        if !self.fields.case_pairs.is_empty() {
            let pairs: Vec<String> = self
                .fields
                .case_pairs
                .iter()
                .map(|[a, b]| format!("{a} over {b}"))
                .collect();
            cases = format!("pairs {}", pairs.join(", "));
        }
        crate::check_ui::Summary {
            target: match self.target.as_str() {
                "ground" => "ground",
                "points" => "LiDAR points",
                "same_line" => "wires of this line",
                "other_lines" => "wires of other lines",
                other => other,
            }
            .to_owned(),
            voltages: crate::rule_table::voltages_words(&self.voltages),
            cases,
            distance: if distance.is_empty() {
                "none".into()
            } else {
                distance.join("; ")
            },
            wires: if self.fields.exclude_near_ends > 0.0 {
                format!("but {} m at the ends", self.fields.exclude_near_ends)
            } else {
                "every wire".into()
            },
        }
    }
}

/// *Import…* in the Clearances window: the rules of rule file `p` replace
/// those of the file chosen (its `[wind]` and `[check]` too, if it has
/// them).
fn import(r: &mut Ready, p: &Path) {
    let defaults: ohl_clearance::rules::RuleDefaults = (&r.model.line.defaults).into();
    let read = std::fs::read_to_string(p)
        .map_err(|e| format!("{}: {e}", p.display()))
        .and_then(|text| {
            let rules = RuleSet::parse_with(&text, &defaults)?;
            Ok(RulePresetChecked {
                name: p
                    .file_name()
                    .map_or(String::new(), |n| n.to_string_lossy().into_owned()),
                description: String::new(),
                rules_text: text,
                weather_text: String::new(),
                rules,
                weather: None,
            })
        });
    let e = &mut r.rule_editor;
    match read.and_then(|x| e.load_preset(&x, Load::Replace, &defaults)) {
        Ok(m) => {
            e.status = Some((format!("imported from {}: {m}", p.display()), false));
            e.dirty = true;
        }
        Err(err) => e.status = Some((err, true)),
    }
}
/// "Merge rules by voltage": the group chosen previewed (what changes at
/// each of the line's voltages), merged on Merge, or left.
fn merge_ui(ui: &mut egui::Ui, e: &mut RuleEditor, voltages: &[f64]) {
    let Some(mut at) = e.merge else {
        return;
    };
    let groups = merge_groups(&e.rules);
    if groups.is_empty() {
        e.merge = None;
        return;
    }
    at = at.min(groups.len() - 1);
    let group = groups[at].clone();
    let mut close = false;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.strong("Merge rules by voltage");
            if groups.len() > 1 {
                ui.label(format!("group {} of {}", at + 1, groups.len()));
                if ui.small_button("Next group").clicked() {
                    at = (at + 1) % groups.len();
                }
            }
        });
        let names: Vec<String> = group
            .iter()
            .map(|&k| {
                format!(
                    "\"{}\" ({})",
                    e.rules[k].name,
                    crate::requirement_ui::voltage_name(e.rules[k].voltages[0])
                )
            })
            .collect();
        ui.label(format!(
            "{} become one rule \"{}\" with a table by voltage and no voltage filter:",
            names.join(", "),
            e.rules[group[0]].name
        ));
        let one = merged(&e.rules, &group);
        for q in [&one.vertical, &one.radial].into_iter().flatten() {
            ui.label(egui::RichText::new(q.to_string()).monospace().small());
        }
        ui.label(
            egui::RichText::new(
                "Without the filter every wire is checked: a voltage between two rows takes \
                 the next row up, earthed wires (0 kV) the first; above the last row there is \
                 no distance. At the line's voltages:",
            )
            .small()
            .weak(),
        );
        crate::check_help::list(
            ui,
            "rule-editor-merge-preview",
            170.0,
            egui::ScrollArea::vertical().id_salt("rule-editor-merge-preview"),
            |ui| {
                for line in merge_preview(&e.rules, &group, voltages) {
                    ui.label(egui::RichText::new(line).small());
                }
            },
        );
        ui.horizontal(|ui| {
            if ui
                .button("Merge")
                .on_hover_text("Edit › Undo takes it back")
                .clicked()
            {
                e.merge_rules(&group);
                e.status = Some((
                    format!("{} rules merged into one table rule", group.len()),
                    false,
                ));
                e.dirty = true;
                close = true;
            }
            if ui.button("Leave them").clicked() {
                close = true;
            }
        });
    });
    e.merge = (!close).then_some(at);
}

/// The presets of every enabled design-code plugin, read once (on a
/// worker thread) for every window offering them: the rule editor's Load
/// preset…, the vertical and radial clearance windows, the vegetation
/// panel ([`preset_dropdown`]).
pub fn ensure_presets(ctx: &egui::Context, r: &mut Ready, plugins: &crate::plugins::Plugins) {
    let w = &mut r.rule_editor.presets;
    let dirs: Vec<(String, PathBuf)> = crate::codes::choices(plugins)
        .into_iter()
        .filter(|c| c.problem.is_none())
        .map(|c| (c.name, c.dir))
        .collect();
    // Once, and again when a plugin is added, removed, turned on or off.
    if !w.busy() && ((w.list.is_empty() && !w.asked) || dirs != w.asked_for) {
        w.asked = true;
        w.asked_for = dirs.clone();
        if dirs.is_empty() {
            w.list.clear();
            w.original.clear();
        } else {
            w.read(dirs, r.design_code.params.clone());
        }
    }
    w.poll(std::time::Duration::ZERO);
    if w.busy() {
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }
    // The project's edits of the presets' values, again when they change.
    let stamp = (r.model.sources.file.clone(), r.model.revisions.libraries);
    if !w.busy() && w.applied.as_ref() != Some(&stamp) {
        let (edits, _) = crate::preset_edits::of_model(&r.model);
        let (list, errors) = crate::preset_edits::apply_presets(&w.original, &edits);
        w.list = list;
        w.edit_errors = errors;
        w.applied = Some(stamp);
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
                        "No design-code plugin turned on (File › Plugins › Plugin manager)",
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

/// Takes the preset chosen in the Clearances window's *Preset ▾* into the
/// rule file chosen (one write, one command): merged or replacing its
/// rules; its weather cases missing from the model added to the model's
/// weather cases.
pub fn apply_preset(r: &mut Ready, pick: crate::rule_table::PresetPick) {
    let (p, lines) = match pick.taken(&r.model, crate::preset_cases::Part::Rules) {
        Ok(x) => x,
        Err(e) => {
            r.rule_editor.status = Some((e, true));
            return;
        }
    };
    let defaults: ohl_clearance::rules::RuleDefaults = (&r.model.line.defaults).into();
    let done = match r.rule_editor.load_preset(&p, pick.how, &defaults) {
        Ok(m) => m,
        Err(e) => {
            r.rule_editor.status = Some((e, true));
            return;
        }
    };
    let mut text = pick.said(&done, &[]);
    if r.rule_editor.save(&mut r.model) {
        if let Some(k) = r.clearances.chosen {
            r.clearances.choose(k);
        }
    } else {
        return;
    }
    if let Some(wc) = &p.weather {
        match preset_cases(r, &pick.group, &p.name, &p.weather_text, wc) {
            Ok(m) if !m.is_empty() => text = format!("{text}; {m}"),
            Ok(_) => {}
            Err(e) => text = format!("{text}; weather cases: {e}"),
        }
    }
    if !lines.is_empty() {
        text = format!("{text}\n{}", lines.join("\n"));
    }
    r.rule_editor.status = Some((text, false));
}
/// The preset's weather cases missing from the model (by name: `wc` is the
/// preset as matched to the project, [`crate::preset_cases`], holding only
/// the cases to create): added to the model's weather case file (or the
/// preset's file used when the model has none) — one change of the
/// project's weather case file, one undo step. What was done.
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
    /// Remove all rules empties the file's rules and keeps the rest of
    /// it readable (a rule file without rules).
    #[test]
    fn all_rules_are_removed() {
        let d = ohl_clearance::rules::RuleDefaults::default();
        let mut e = RuleEditor::default();
        e.take(FILE, &d).unwrap();
        assert_eq!(e.rules.len(), 2);
        e.remove_all_rules();
        assert!(e.rules.is_empty());
        let text = e.text().unwrap();
        assert!(!text.contains("[[rule]]"), "{text}");
        assert!(RuleSet::parse(&text).unwrap().rules.is_empty());
    }

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
