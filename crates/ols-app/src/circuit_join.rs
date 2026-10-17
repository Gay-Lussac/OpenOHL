// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Joining two circuits with a jumper: a jumper between conductors of two
//! different circuits (another name, phase or electrical system) makes
//! them one. The user is told first (Continue / Cancel), then shown both
//! circuits side by side and chooses the one that stays: the other's
//! sections take its name, phase and system, in the same command as the
//! jumper (one undo takes all back).

use eframe::egui;
use ols_line::{Jumper, JumperEnd, Line, Section};
use ols_session::{Command, Kind};

use crate::model::Model;

/// One of the two circuits a jumper would join.
#[derive(Debug, Clone, PartialEq)]
pub struct Side {
    /// Its name (its sections' `circuit_name`; empty: none given).
    pub name: String,
    /// Its phase label.
    pub phase: String,
    /// Its electrical system's id (none: no system).
    pub system: Option<String>,
    /// The system in words: name, kind and voltage.
    pub system_text: String,
    /// The system's phases, in words.
    pub system_phases: String,
    /// Its sections, from its start.
    pub sections: Vec<String>,
    /// Its ends: the structures it runs between.
    pub ends: String,
    /// Its length, km (plan, along its sections' wire points).
    pub km: f64,
}

impl Side {
    /// Its name for the window (`(no name)` when none).
    pub fn title(&self) -> String {
        if self.name.trim().is_empty() {
            "(no name)".into()
        } else {
            self.name.clone()
        }
    }

    /// Its name with its phase (`Example 1 · L2`): telling two circuits
    /// of one name apart.
    pub fn label(&self) -> String {
        if self.phase.trim().is_empty() {
            self.title()
        } else {
            format!("{} · {}", self.title(), self.phase)
        }
    }

    fn same_as(&self, other: &Side) -> bool {
        self.name == other.name && self.phase == other.phase && self.system == other.system
    }
}

/// Where the window is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// "You are trying to connect 2 different circuits": Continue / Cancel.
    Warn,
    /// Both circuits side by side; the one that stays chosen.
    Choose,
}

/// A jumper waiting for the user: it joins two different circuits.
#[derive(Debug, Clone, PartialEq)]
pub struct CircuitJoin {
    /// The jumper to add.
    pub jumper: Jumper,
    /// The change in words (the jumper's).
    pub what: String,
    /// Open the jumper's window once added (helping strings, a tap).
    pub open_editor: bool,
    /// The circuit at its `from` end, at its `to` end.
    pub sides: [Side; 2],
    /// The one that stays (0 or 1).
    pub keep: usize,
    /// Where the window is.
    pub step: Step,
}

/// The sections of the circuit through `end` (a wire point's, or the
/// section a tap hangs on), with its circuit's index.
fn circuit_at(
    line: &Line,
    circuits: &[ols_line::electrical::Circuit],
    end: &JumperEnd,
) -> Option<usize> {
    let section = match end {
        JumperEnd::Wire(p) => line.sections.iter().find(|s| s.points.contains(p))?,
        JumperEnd::Tap { tap } => line.tap_section(tap, 0)?,
    };
    circuits
        .iter()
        .position(|c| c.sections.contains(&section.id))
}

/// A circuit's side, from its sections.
fn side(line: &Line, c: &ols_line::electrical::Circuit) -> Side {
    let sections: Vec<&Section> = c
        .sections
        .iter()
        .filter_map(|id| line.sections.iter().find(|s| &s.id == id))
        .collect();
    let first = sections.first();
    let system = first.and_then(|s| s.system.clone());
    let sys = system
        .as_ref()
        .and_then(|id| line.systems.iter().find(|s| &s.id == id));
    let system_text = match sys {
        Some(s) => format!(
            "{} ({}, {} kV)",
            if s.name.is_empty() { &s.id } else { &s.name },
            match s.kind {
                ols_line::electrical::SystemKind::Ac => "AC",
                ols_line::electrical::SystemKind::Dc => "DC",
            },
            s.voltage_kv
        ),
        None => "none".into(),
    };
    let system_phases = sys.map_or(String::new(), |s| {
        s.phases
            .iter()
            .map(|p| p.label.clone())
            .collect::<Vec<_>>()
            .join(", ")
    });
    let at = |p: &ols_line::SectionPoint| line.wire_point(p).map(|w| w.position);
    let km: f64 = sections
        .iter()
        .flat_map(|s| s.points.windows(2))
        .filter_map(|w| Some((at(&w[0])?, at(&w[1])?)))
        .map(|(a, b)| (b[0] - a[0]).hypot(b[1] - a[1]))
        .sum::<f64>()
        / 1000.0;
    let mut ends: Vec<String> = c
        .starts
        .iter()
        .chain(&c.ends)
        .map(|p| p.structure.clone())
        .collect();
    ends.dedup();
    Side {
        name: first.map_or(String::new(), |s| s.circuit_name.clone()),
        phase: first.map_or(String::new(), |s| s.phase_label.clone()),
        system,
        system_text,
        system_phases,
        sections: c.sections.clone(),
        ends: if ends.is_empty() {
            "a closed loop".into()
        } else {
            ends.join(" – ")
        },
        km,
    }
}

impl CircuitJoin {
    /// The join `jumper` would make in `model`'s line: `None` when its ends
    /// are on one circuit, on circuits of the same name, phase and system,
    /// or not on circuits at all (a wire point no section passes).
    pub fn check(model: &Model, jumper: &Jumper, what: &str, open_editor: bool) -> Option<Self> {
        let line = model.stored_line().unwrap_or(&model.line);
        let circuits = ols_line::electrical::circuits(line);
        let a = circuit_at(line, &circuits, &jumper.from)?;
        let b = circuit_at(line, &circuits, &jumper.to)?;
        if a == b {
            return None;
        }
        let sides = [side(line, &circuits[a]), side(line, &circuits[b])];
        if sides[0].same_as(&sides[1]) {
            return None;
        }
        Some(Self {
            jumper: jumper.clone(),
            what: what.to_owned(),
            open_editor,
            sides,
            keep: 0,
            step: Step::Warn,
        })
    }

    /// The command: the jumper, and the other circuit's sections given the
    /// name, phase and system of the one that stays.
    pub fn command(&self, model: &Model) -> Result<Command, String> {
        let line = model.stored_line().unwrap_or(&model.line);
        let keep = &self.sides[self.keep];
        let other = &self.sides[1 - self.keep];
        let mut commands = vec![Command::Put {
            kind: Kind::Jumper,
            id: self.jumper.id.clone(),
            value: serde_json::to_value(&self.jumper).map_err(|e| e.to_string())?,
            ord: None,
        }];
        for id in &other.sections {
            let mut s = line
                .sections
                .iter()
                .find(|s| &s.id == id)
                .ok_or_else(|| format!("section {id} is not in the line"))?
                .clone();
            s.circuit_name = keep.name.clone();
            s.phase_label = keep.phase.clone();
            s.system = keep.system.clone();
            commands.push(Command::Put {
                kind: Kind::Section,
                id: id.clone(),
                value: serde_json::to_value(&s).map_err(|e| e.to_string())?,
                ord: None,
            });
        }
        Ok(Command::Batch { commands })
    }

    /// The change in words.
    pub fn what(&self) -> String {
        let keep = &self.sides[self.keep];
        let other = &self.sides[1 - self.keep];
        format!(
            "{}, joining circuit {} into {}",
            self.what,
            other.label(),
            keep.label()
        )
    }
}

/// What the user chose in the window this frame.
pub enum Outcome {
    /// Still open.
    Open,
    /// Closed without a change.
    Cancel,
    /// Join: the jumper and the merge, as chosen.
    Join,
}

/// The window. Escape cancels.
pub fn window(ctx: &egui::Context, join: &mut CircuitJoin) -> Outcome {
    let mut outcome = Outcome::Open;
    let mut open = true;
    let (a, b) = (join.sides[0].label(), join.sides[1].label());
    let title = match join.step {
        Step::Warn => "Different circuits",
        Step::Choose => "Join two circuits",
    };
    egui::Window::new(title)
        .id(egui::Id::new("circuit-join"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| match join.step {
            Step::Warn => {
                ui.heading("You are trying to connect 2 different circuits");
                ui.add_space(4.0);
                ui.label(format!(
                    "Jumper {} joins circuit {a} (system {}) and circuit {b} (system {}).",
                    join.jumper.id, join.sides[0].system_text, join.sides[1].system_text,
                ));
                ui.label(
                    "Connected, they become one circuit with one name, phase and system. \
                     Continue to compare them and choose which one stays.",
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .button("Continue")
                        .on_hover_text("Compare both circuits and choose the one that stays")
                        .clicked()
                    {
                        join.step = Step::Choose;
                    }
                    if ui
                        .button("Cancel")
                        .on_hover_text("Leave the line as it is: no jumper")
                        .clicked()
                    {
                        outcome = Outcome::Cancel;
                    }
                });
            }
            Step::Choose => {
                ui.label("Choose the circuit that stays. The other one's sections take its name, phase and system.");
                ui.add_space(6.0);
                let sides = join.sides.clone();
                egui::Grid::new("circuit-join-grid")
                    .num_columns(3)
                    .spacing([18.0, 6.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label("");
                        for (k, s) in sides.iter().enumerate() {
                            let text = egui::RichText::new(format!("Keep {}", s.label())).strong();
                            if ui
                                .radio(join.keep == k, text)
                                .on_hover_text("This circuit stays as it is")
                                .clicked()
                            {
                                join.keep = k;
                            }
                        }
                        ui.end_row();
                        let row = |ui: &mut egui::Ui, name: &str, v: [String; 2]| {
                            // Differences stand out.
                            let differ = v[0] != v[1];
                            ui.label(egui::RichText::new(name).weak());
                            for x in v {
                                let t = egui::RichText::new(x);
                                ui.label(if differ {
                                    t.color(ui.visuals().warn_fg_color)
                                } else {
                                    t
                                });
                            }
                            ui.end_row();
                        };
                        row(ui, "circuit name", sides.clone().map(|s| s.title()));
                        row(ui, "phase", sides.clone().map(|s| or_none(&s.phase)));
                        row(ui, "system", sides.clone().map(|s| s.system_text));
                        row(
                            ui,
                            "system's phases",
                            sides.clone().map(|s| or_none(&s.system_phases)),
                        );
                        row(ui, "runs between", sides.clone().map(|s| s.ends));
                        row(
                            ui,
                            "sections",
                            sides.clone().map(|s| {
                                let n = s.sections.len();
                                let list = s.sections.iter().take(4).cloned().collect::<Vec<_>>();
                                format!(
                                    "{n}: {}{}",
                                    list.join(", "),
                                    if n > 4 { ", …" } else { "" }
                                )
                            }),
                        );
                        row(
                            ui,
                            "length",
                            sides.clone().map(|s| format!("{:.2} km", s.km)),
                        );
                    });
                ui.add_space(6.0);
                let keep = &join.sides[join.keep];
                let other = &join.sides[1 - join.keep];
                ui.label(format!(
                    "{} {} ({}) will become circuit {}, phase {}, system {}.",
                    if other.sections.len() == 1 {
                        "Section"
                    } else {
                        "Sections"
                    },
                    other.sections.join(", "),
                    other.label(),
                    keep.title(),
                    or_none(&keep.phase),
                    keep.system_text
                ));
                ui.label(
                    egui::RichText::new("One undo takes the jumper and this change back together.")
                        .small()
                        .weak(),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .button(egui::RichText::new(format!("Join into {}", keep.label())).strong())
                        .clicked()
                    {
                        outcome = Outcome::Join;
                    }
                    if ui.button("Back").clicked() {
                        join.step = Step::Warn;
                    }
                    if ui.button("Cancel").clicked() {
                        outcome = Outcome::Cancel;
                    }
                });
            }
        });
    if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        outcome = Outcome::Cancel;
    }
    outcome
}

fn or_none(s: &str) -> String {
    if s.trim().is_empty() {
        "none".into()
    } else {
        s.to_owned()
    }
}
