// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Replace structure (structure building decision 8, §1.6): a placed
//! structure is replaced by another structure type of the library, at the
//! same place and orientation, keeping its id
//! ([`ohl_session::Command::ReplaceStructure`]).
//!
//! The Replace structure tool (right-click a structure), the structure's
//! Properties (Replace…) and the Structures table (right-click a row's ⤴)
//! open the structure picker ([`crate::structure_picker::Picker`]). Once
//! a type is chosen:
//!
//! - every old set something ends at is on the new type, and nothing else
//!   is dropped: the structure is replaced at once and the status line
//!   says how many wires were rewired;
//! - else the mapping window: the old wire points (set, label, side,
//!   height, what ends there) on the left, a choice of the new type's
//!   sets on the right (proposed: the same set, else the same side in the
//!   same height order; "none": removed), and under them everything that
//!   would be removed, in a scroll box, drawn red in the 3D view while the
//!   window is open. Replace applies it as one undoable change; Cancel,
//!   Escape or closing the window leave things as they were.
//!
//! The extension is kept when the new type has one of the same name, else
//! the structure is placed without one (and the window or the status line
//! says so).

use std::collections::BTreeMap;

use eframe::egui;
use ohl_line::{Line, Structure};
use ohl_session::replace::{NewWire, SetMove};
use ohl_session::{Command, Impact};

use crate::model::Model;
use crate::structure_picker::{Outcome, Picker};

/// A wire point with a set, as the window shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct WireRow {
    /// Set.
    pub set: u32,
    /// Label.
    pub label: String,
    /// Left, right or middle, looking ahead along the line.
    pub side: Side,
    /// Height above the structure's position, m.
    pub height: f64,
    /// What ends there (sections, jumpers), in words.
    pub ends: String,
}

/// Which side of the structure a wire point is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Side {
    /// Left of the line, looking ahead.
    Left,
    /// On the axis (within 5 cm).
    Middle,
    /// Right of the line.
    Right,
}

impl Side {
    fn name(self) -> &'static str {
        match self {
            Side::Left => "left",
            Side::Middle => "middle",
            Side::Right => "right",
        }
    }
}

/// The wire points with a set of `s` (resolved), with their sides and
/// heights; `ends`: what ends at each set.
fn wire_rows(s: &Structure, ends: &dyn Fn(u32) -> String) -> Vec<WireRow> {
    let (c, n) = {
        let a = s.orientation_deg.to_radians();
        (a.cos(), a.sin())
    };
    let mut rows: Vec<WireRow> = s
        .wires
        .iter()
        .filter_map(|w| {
            let set = w.set?;
            let x = (w.position[0] - s.position[0]) * c + (w.position[1] - s.position[1]) * n;
            let side = if x > 0.05 {
                Side::Right
            } else if x < -0.05 {
                Side::Left
            } else {
                Side::Middle
            };
            Some(WireRow {
                set,
                label: w.label.clone(),
                side,
                height: w.position[2] - s.position[2],
                ends: ends(set),
            })
        })
        .collect();
    rows.sort_by_key(|r| r.set);
    rows
}

/// The proposal for old sets `used`: the same set when the new type has
/// it, else the new set on the same side in the same height order (from
/// the top) among those still free; else none.
pub fn propose(old: &[WireRow], new: &[WireRow], used: &[u32]) -> BTreeMap<u32, Option<u32>> {
    let mut out: BTreeMap<u32, Option<u32>> = BTreeMap::new();
    let has = |set: u32| new.iter().any(|w| w.set == set);
    for &set in used {
        out.insert(set, has(set).then_some(set));
    }
    for side in [Side::Left, Side::Middle, Side::Right] {
        let by_height = |rows: Vec<&WireRow>| {
            let mut rows = rows;
            rows.sort_by(|a, b| b.height.total_cmp(&a.height));
            rows.into_iter().map(|r| r.set).collect::<Vec<u32>>()
        };
        let open = by_height(
            old.iter()
                .filter(|w| w.side == side && out.get(&w.set) == Some(&None))
                .collect(),
        );
        let free = by_height(
            new.iter()
                .filter(|w| w.side == side && !out.values().any(|v| *v == Some(w.set)))
                .collect(),
        );
        for (o, n) in open.into_iter().zip(free) {
            out.insert(o, Some(n));
        }
    }
    out
}

/// The mapping window's state.
pub struct Mapping {
    /// The structure replaced.
    pub id: String,
    /// The new structure as stored (its type and extension).
    pub structure: Structure,
    /// Its wire points as placed.
    pub wires: Vec<NewWire>,
    /// The old structure's wire points something ends at.
    pub old: Vec<WireRow>,
    /// The new structure's wire points with a set.
    pub new: Vec<WireRow>,
    /// Old set → new set (none: removed).
    pub choice: BTreeMap<u32, Option<u32>>,
    /// About the extension, when it was not kept.
    pub note: Option<String>,
    /// What is removed with the choice (or why it is refused), worked out
    /// again when the choice changes.
    pub impact: Result<Impact, String>,
    /// The choice `impact` is for.
    impact_of: BTreeMap<u32, Option<u32>>,
}

impl Mapping {
    /// The command for the choice (confirmed: the window lists what goes).
    pub fn command(&self) -> Command {
        Command::ReplaceStructure {
            id: self.id.clone(),
            structure: Box::new(self.structure.clone()),
            wires: self.wires.clone(),
            sets: self.moves(),
            confirm: true,
        }
    }

    fn moves(&self) -> Vec<SetMove> {
        self.choice
            .iter()
            .map(|(&from, &to)| SetMove { from, to })
            .collect()
    }

    /// Works out what is removed again (when the choice changed).
    fn refresh(&mut self, model: &Model) {
        if self.impact_of == self.choice {
            return;
        }
        self.impact_of = self.choice.clone();
        self.impact = match model.session() {
            Some(s) => s
                .replacement(&self.id, &self.structure, &self.wires, &self.moves())
                .map(|p| p.impact)
                .map_err(|e| e.to_string()),
            None => Err("not editable".into()),
        };
    }
}

/// Where the replacement of a structure is.
pub enum Stage {
    /// Choosing the new type.
    Pick(Picker),
    /// Saying which old wire point goes where.
    Map(Box<Mapping>),
}

/// A replacement under way.
pub struct Replace {
    /// The structure replaced.
    pub id: String,
    /// Its stage.
    pub stage: Stage,
}

/// What a frame of the replacement asks the application to do.
pub enum Step {
    /// Nothing yet.
    Wait,
    /// Closed without a change.
    Cancel,
    /// Apply this command (said in words).
    Apply(String, Command),
    /// Could not go on (said in the status line).
    Fail(String),
}

impl Replace {
    /// Starts replacing structure `id`: the picker with its type and
    /// extension.
    pub fn start(model: &Model, id: &str) -> Result<Replace, String> {
        model.can_edit()?;
        let s = model
            .stored_line()
            .and_then(|l| l.structure(id))
            .ok_or_else(|| format!("no structure {id}"))?;
        if model
            .types
            .as_ref()
            .is_none_or(|(t, _)| t.iter().next().is_none())
        {
            return Err(
                "Replace structure: no structure type in the library (make a pylon type first)"
                    .into(),
            );
        }
        Ok(Replace {
            id: id.to_owned(),
            stage: Stage::Pick(Picker::new(
                format!("Replace structure {id} — the new structure type"),
                s.structure_type.as_deref(),
                s.extension.as_deref(),
                true,
                false,
            )),
        })
    }

    /// The type chosen: the replacement at once when every set matches and
    /// nothing goes, else the mapping window.
    pub fn chosen(
        &mut self,
        model: &Model,
        structure_type: String,
        extension: Option<String>,
    ) -> Step {
        match self.plan(model, structure_type, extension) {
            Ok(m) => {
                let m = *m;
                let every_set = m.choice.iter().all(|(from, to)| *to == Some(*from));
                match &m.impact {
                    Ok(i) if every_set && !i.has_dependents() => {
                        let what = rewired_words(&m, model);
                        Step::Apply(what, m.command())
                    }
                    _ => {
                        self.stage = Stage::Map(Box::new(m));
                        Step::Wait
                    }
                }
            }
            Err(e) => Step::Fail(e),
        }
    }

    /// The new structure placed, its wire points, the proposal.
    fn plan(
        &self,
        model: &Model,
        structure_type: String,
        extension: Option<String>,
    ) -> Result<Box<Mapping>, String> {
        let id = &self.id;
        let stored = model
            .stored_line()
            .ok_or_else(|| "the model cannot be edited".to_owned())?;
        let old_stored = stored
            .structure(id)
            .ok_or_else(|| format!("no structure {id}"))?;
        let mut new = old_stored.clone();
        new.structure_type = Some(structure_type.clone());
        new.extension = extension.clone();
        new.points.clear();
        new.strings.clear();
        new.wires.clear();
        new.omit.clear();
        new.omit_sets.clear();
        new.members.clear();
        let note = match (&old_stored.extension, &extension) {
            (Some(e), None) => Some(format!(
                "Type {structure_type} has no extension \"{e}\": placed without one."
            )),
            _ => None,
        };
        let placed = placed(model, stored, &new)?;
        let wires: Vec<NewWire> = placed
            .wires
            .iter()
            .map(|w| NewWire {
                label: w.label.clone(),
                set: w.set,
            })
            .collect();
        let session = model
            .session()
            .ok_or_else(|| "the model cannot be edited".to_owned())?;
        // What the new structure gives without a choice: the sets used.
        let plain = session
            .replacement(id, &new, &wires, &[])
            .map_err(|e| e.to_string())?;
        let used: Vec<u32> = plain.moves.iter().map(|(s, _)| *s).collect();
        let line = &model.line;
        let ends = |set: u32| {
            let p = ohl_line::SectionPoint::new(id.clone(), set);
            let mut out: Vec<String> = line
                .sections
                .iter()
                .filter(|s| s.points.contains(&p))
                .map(|s| s.id.clone())
                .collect();
            out.extend(
                line.jumpers
                    .iter()
                    .filter(|j| j.is_at(&p))
                    .map(|j| format!("jumper {}", j.id)),
            );
            if line.circuit_starts.contains(&p) {
                out.push("circuit start".into());
            }
            out.join(", ")
        };
        let old_resolved = line
            .structure(id)
            .ok_or_else(|| format!("no structure {id}"))?;
        let old: Vec<WireRow> = wire_rows(old_resolved, &ends)
            .into_iter()
            .filter(|w| used.contains(&w.set))
            .collect();
        let new_rows = wire_rows(&placed, &|_| String::new());
        let choice = propose(&old, &new_rows, &used);
        let mut m = Mapping {
            id: id.clone(),
            structure: new,
            wires,
            old,
            new: new_rows,
            choice,
            note,
            impact: Err(String::new()),
            impact_of: BTreeMap::new(),
        };
        // Worked out for the choice (refresh skips an equal choice).
        m.impact_of.insert(u32::MAX, None);
        m.refresh(model);
        Ok(Box::new(m))
    }
}

/// "replaced T3 by type-02: rewired N wires".
fn rewired_words(m: &Mapping, model: &Model) -> String {
    let line = &model.line;
    let at = |p: &ohl_line::SectionPoint| {
        p.structure == m.id && matches!(m.choice.get(&p.set), Some(Some(_)))
    };
    // Those removed are not rewired.
    let (sections, jumpers) = match &m.impact {
        Ok(i) => (&i.sections[..], &i.jumpers[..]),
        Err(_) => (&[][..], &[][..]),
    };
    let n = line
        .sections
        .iter()
        .filter(|s| !sections.contains(&s.id))
        .map(|s| s.points.iter().filter(|p| at(p)).count())
        .sum::<usize>()
        + line
            .jumpers
            .iter()
            .filter(|j| !jumpers.contains(&j.id))
            .map(|j| j.points().filter(|p| at(p)).count())
            .sum::<usize>();
    let mut what = format!(
        "replaced {} by {}: rewired {n} wire{}",
        m.id,
        m.structure.structure_type.as_deref().unwrap_or("no type"),
        if n == 1 { "" } else { "s" }
    );
    if let Some(note) = &m.note {
        what.push_str(&format!(" ({note})"));
    }
    what
}

/// Structure `new` placed from the library (a line of the stored one
/// without its wires: nothing ends at its sets yet).
fn placed(model: &Model, stored: &Line, new: &Structure) -> Result<Structure, String> {
    let (types, insulators) = model
        .types
        .as_ref()
        .ok_or_else(|| "no structure library".to_owned())?;
    let mut l = stored.clone();
    l.sections.clear();
    l.jumpers.clear();
    l.spacers.clear();
    l.circuit_starts.clear();
    l.structures.retain(|s| s.id == new.id);
    l.structures[0] = new.clone();
    l.alignment_points.iter_mut().for_each(|p| {
        if p.structure.as_deref() != Some(new.id.as_str()) {
            p.structure = None;
        }
    });
    let r = l
        .resolve(types, insulators)
        .map_err(|e| format!("{}: {e}", new.id))?;
    r.structure(&new.id)
        .cloned()
        .ok_or_else(|| format!("no structure {}", new.id))
}

/// Polylines (in the case shown) of the sections and jumpers `impact`
/// removes, to draw red in the 3D view.
pub fn removed_lines(model: &Model, case: usize, impact: &Impact) -> Vec<Vec<[f64; 3]>> {
    let mut out = Vec::new();
    let Some(c) = model.cases.get(case) else {
        return out;
    };
    for solved in c.drawn() {
        for sh in solved
            .shapes
            .iter()
            .filter(|x| impact.sections.contains(&x.section))
        {
            out.extend(sh.spans.iter().map(|sp| sp.points.clone()));
        }
        for j in solved
            .jumpers
            .iter()
            .filter(|x| impact.jumpers.contains(&x.jumper))
        {
            let (_, lines) = crate::scene::jumper_lines(&model.line, j, &solved.bundles);
            out.extend(lines);
        }
    }
    out
}

/// The mapping window; what it asks for.
pub fn mapping_window(ctx: &egui::Context, m: &mut Mapping, model: &Model) -> Step {
    m.refresh(model);
    let mut step = Step::Wait;
    let mut open = true;
    let title = format!(
        "Replace structure {} by {}",
        m.id,
        m.structure.structure_type.as_deref().unwrap_or("no type")
    );
    let window = egui::Window::new(title.clone())
        .id(egui::Id::new(("replace-structure", m.id.clone())))
        .collapsible(false)
        .resizable(false)
        .default_width(620.0);
    let panel = crate::dockable::Panel::Replace;
    crate::dockable::show(ctx, panel, title, window, Some(&mut open), |ui| {
        ui.label(
            "Which wire point of the new structure each wire point of the old one goes to \
                 (what ends there moves with it); none: what ends there is removed.",
        );
        if let Some(note) = &m.note {
            ui.colored_label(ui.visuals().warn_fg_color, note);
        }
        ui.separator();
        egui::Grid::new("replace-mapping")
            .striped(true)
            .num_columns(6)
            .show(ui, |ui| {
                for h in ["set", "label", "side", "height", "ends here", "new set"] {
                    ui.label(egui::RichText::new(h).small().strong());
                }
                ui.end_row();
                for w in &m.old {
                    ui.label(w.set.to_string());
                    ui.label(&w.label);
                    ui.label(w.side.name());
                    ui.label(format!("{:.2} m", w.height));
                    ui.label(&w.ends);
                    let chosen = m.choice.entry(w.set).or_insert(None);
                    let text = |set: Option<u32>| match set {
                        None => "none (removed)".to_owned(),
                        Some(s) => match m.new.iter().find(|n| n.set == s) {
                            Some(n) => format!(
                                "set {s} {} — {}, {:.2} m",
                                n.label,
                                n.side.name(),
                                n.height
                            ),
                            None => format!("set {s}"),
                        },
                    };
                    egui::ComboBox::from_id_salt(("replace-set", w.set))
                        .selected_text(text(*chosen))
                        .width(230.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(chosen, None, text(None));
                            for n in &m.new {
                                ui.selectable_value(chosen, Some(n.set), text(Some(n.set)));
                            }
                        });
                    ui.end_row();
                }
            });
        if m.old.is_empty() {
            ui.label(egui::RichText::new("Nothing ends at this structure.").weak());
        }
        ui.separator();
        let mut ok = false;
        match &m.impact {
            Ok(i) if i.has_dependents() => {
                ui.label("Removed or dropped with it (drawn red in the 3D view):");
                egui::ScrollArea::vertical()
                    .id_salt("replace-removed")
                    .max_height(180.0)
                    .min_scrolled_height(180.0)
                    .show(ui, |ui| {
                        for (title, items) in i.groups() {
                            if items.is_empty() {
                                continue;
                            }
                            ui.label(
                                egui::RichText::new(format!("{title} ({})", items.len())).strong(),
                            );
                            for it in items {
                                ui.label(egui::RichText::new(format!("  {it}")).small());
                            }
                        }
                    });
                ok = true;
            }
            Ok(_) => {
                ui.label("Nothing is removed.");
                ok = true;
            }
            Err(e) => {
                egui::ScrollArea::vertical()
                    .id_salt("replace-error")
                    .max_height(80.0)
                    .show(ui, |ui| {
                        ui.colored_label(ui.visuals().error_fg_color, e);
                    });
            }
        }
        ui.label(
            egui::RichText::new("It is one log entry: undo brings everything back.")
                .small()
                .weak(),
        );
        ui.horizontal(|ui| {
            if ui.add_enabled(ok, egui::Button::new("Replace")).clicked() {
                step = Step::Apply(rewired_words(m, model), m.command());
            }
            if ui.button("Cancel").clicked() {
                step = Step::Cancel;
            }
        });
    });
    if !open || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        step = Step::Cancel;
    }
    step
}

/// The picker or the mapping window of `r`'s replacement, and what they
/// end with.
pub fn window(
    ctx: &egui::Context,
    r: &mut crate::app::Ready,
    rs: Option<&eframe::egui_wgpu::RenderState>,
) {
    let Some(mut rep) = r.replace.take() else {
        if !r.view3d.removed.is_empty() {
            r.view3d.removed.clear();
            r.view3d.dirty.insert(crate::scene::Layer::Draft);
        }
        return;
    };
    let step = match &mut rep.stage {
        Stage::Pick(p) => {
            let id = egui::Id::new(("replace-picker", rep.id.clone()));
            match p.show(ctx, id, &r.model, rs, &mut r.picker_preview) {
                Some(Outcome::Choose {
                    structure_type: Some(t),
                    extension,
                }) => rep.chosen(&r.model, t, extension),
                Some(_) => Step::Cancel,
                None => Step::Wait,
            }
        }
        Stage::Map(m) => mapping_window(ctx, m, &r.model),
    };
    // What would be removed, red in the 3D view.
    let lines = match &rep.stage {
        Stage::Map(m) => match &m.impact {
            Ok(i) => removed_lines(&r.model, r.case, i),
            Err(_) => Vec::new(),
        },
        Stage::Pick(_) => Vec::new(),
    };
    if lines != r.view3d.removed {
        r.view3d.removed = lines;
        r.view3d.dirty.insert(crate::scene::Layer::Draft);
    }
    match step {
        Step::Wait => r.replace = Some(rep),
        Step::Cancel => {}
        Step::Apply(what, command) => {
            r.perform(crate::edit::Action::Command { what, command });
        }
        Step::Fail(e) => r.status = Some((e, true)),
    }
    if r.replace.is_none() && !r.view3d.removed.is_empty() {
        r.view3d.removed.clear();
        r.view3d.dirty.insert(crate::scene::Layer::Draft);
    }
}
