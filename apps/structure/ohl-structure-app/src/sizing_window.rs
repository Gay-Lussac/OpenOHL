// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Choose the members** (2026-11-26, roadmap O49 stage 1; built, in
//! active development, not approved yet): Design › Choose the members…
//! opens one window (the tool-window and check-window patterns) that runs
//! the member sizer ([`ohl_structure_link::sizing`]) on a thread of its
//! own — per member group the lightest section of the project's library in
//! its family passing the code's check, the code's limits and the
//! joint-fit limits, the forces solved again until the choice settles, a
//! full run with the chosen sections. The proposal is a table: per group
//! its section, mass, usage, governing case and joint-fit problems before
//! and after, why, a tick to take it. **Apply** writes the ticked groups
//! as one undo step; nothing is changed before. Escape cancels (a run
//! going on is stopped).

use std::collections::BTreeSet;
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};

use eframe::egui;
use ohl_core::budget::{Budget, CancelToken};
use ohl_structure_link::sizing::{self, Settings, Sizing, SizingInput};

use crate::app::StructureApp;

/// The manual's page of the window (page, anchor).
pub const SIZING_PAGE: (&str, &str) = ("user-guide/member-sizing", SIZING_ANCHOR);
const SIZING_ANCHOR: &str = "choose-the-members";

/// A proposal, worked out for the file's revision and the settings.
pub struct SizingResult {
    /// The file's revision.
    pub revision: u64,
    /// The settings it was worked out with.
    pub settings: Settings,
    /// The sizer's answer.
    pub sizing: Sizing,
}

/// A run going on.
struct Job {
    rx: Receiver<Result<Sizing, String>>,
    cancel: CancelToken,
    step: Arc<Mutex<String>>,
    revision: u64,
    settings: Settings,
}

/// The Choose the members window.
pub struct SizingWindow {
    /// The settings.
    pub settings: Settings,
    /// The proposal.
    pub result: Option<SizingResult>,
    /// The groups ticked to take.
    pub accepted: BTreeSet<String>,
    job: Option<Job>,
    /// Why there is no proposal.
    pub error: Option<String>,
}

impl SizingWindow {
    /// A run is going on.
    pub fn running(&self) -> bool {
        self.job.is_some()
    }

    /// Closed: a run going on is stopped.
    pub fn cancel(&self) {
        if let Some(job) = &self.job {
            job.cancel.cancel();
        }
    }
}

const INTRO: &str = "Proposes, for each member group, the lightest section of the project's \
     section library that passes the structure's design check and its limits and can still be \
     bolted; nothing changes before Apply.";

const MORE: &str = "For each member group (the members of one group with one section; members \
     without a group with their mirror images) the sections of the project's section library in \
     its family — the same shape and steel: an angle stays an angle — are tried from the \
     lightest up. A section is taken when every member of the group passes: its usage by the \
     structure's design code (the code plugin loaded: ASCE 10-97 by default) at most the target \
     in every load case, the code's limits on the members' sizes (slenderness, flat width to \
     thickness, least thickness: from the plugin), and the joint-fit limits (its leg takes its \
     bolt at the usual gauge with the code's edge distance; it is long enough for the bolts its \
     end force needs). Forces change with the sections' stiffness and weight, so the structure \
     is solved again with the new sections and the choice made again, until it stops changing \
     (a choice coming back: the heavier, said). The proposal is then a full run of the structure \
     with the chosen sections — never estimated forces. Only a member's section changes (and its \
     design data's thickness and w/t, which described the old section); never a work point, a \
     roll or a group. Apply takes the ticked groups as one undo step.";

impl StructureApp {
    /// Opens the Choose the members window.
    pub(crate) fn open_sizing_window(&mut self) {
        self.sizing_window = Some(SizingWindow {
            settings: Settings::default(),
            result: None,
            accepted: BTreeSet::new(),
            job: None,
            error: None,
        });
    }

    /// Why the sizer cannot run now (none: it can).
    fn sizing_blocked(&mut self) -> Option<String> {
        let Some(f) = &self.file else {
            return Some("Open a structure first".into());
        };
        if !f.structure().detail().is_design() {
            return Some(
                "A level 3 structure is needed: every member with a section and a supported joint"
                    .into(),
            );
        }
        self.loads.refresh(f);
        if self.loads.table().cases().is_empty() {
            return Some("No load case: the load table has none".into());
        }
        None
    }

    /// Starts the sizer on a thread of its own.
    fn start_sizing(&mut self, w: &mut SizingWindow, ctx: &egui::Context) {
        let Some(f) = &self.file else {
            return;
        };
        let structure = f.structure().clone();
        let text = f.text();
        let revision = f.revision();
        let dir = f
            .path()
            .and_then(std::path::Path::parent)
            .map(std::path::Path::to_path_buf);
        let table = self.loads.table().clone();
        let recipes = self.loads.run_recipes().to_vec();
        let sections = self.loads.sections.clone();
        let settings = w.settings.clone();
        let cancel = CancelToken::new();
        let step = Arc::new(Mutex::new(String::from("starting")));
        let (tx, rx) = channel();
        let (c2, s2, set2) = (cancel.clone(), step.clone(), settings.clone());
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let cases = ohl_structure_link::cases_to_run(&table, &recipes, None);
            let r = sizing::size_with(
                &SizingInput {
                    structure: &structure,
                    text: &text,
                    dir: dir.as_deref(),
                    sections: &sections,
                    table: &table,
                    recipes: &recipes,
                    cases: &cases,
                    settings: &set2,
                },
                &Budget::new().cancel_token(c2),
                &|x| {
                    if let Ok(mut s) = s2.lock() {
                        *s = x.to_owned();
                    }
                    ctx2.request_repaint();
                },
            );
            let _ = tx.send(r);
            ctx2.request_repaint();
        });
        w.error = None;
        w.job = Some(Job {
            rx,
            cancel,
            step,
            revision,
            settings,
        });
    }

    /// The Choose the members window ([`Self::open_sizing_window`]); its
    /// page of the manual: [`SIZING_PAGE`].
    pub(crate) fn sizing_window(&mut self, ctx: &egui::Context) {
        let Some(mut w) = self.sizing_window.take() else {
            return;
        };
        let Some(revision) = self.file.as_ref().map(|f| f.revision()) else {
            return;
        };
        if let Some(job) = &w.job {
            match job.rx.try_recv() {
                Ok(r) => {
                    let job = w.job.take().expect("a job");
                    match r {
                        Ok(sizing) => {
                            w.accepted = sizing
                                .groups
                                .iter()
                                .filter(|g| g.changed())
                                .map(|g| g.group.name.clone())
                                .collect();
                            w.result = Some(SizingResult {
                                revision: job.revision,
                                settings: job.settings,
                                sizing,
                            });
                        }
                        Err(e) => w.error = Some(e),
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(200));
                }
                Err(_) => w.job = None,
            }
        }
        let groups: Vec<String> = self
            .file
            .as_ref()
            .map(|f| {
                ohl_structure_edit::groups(f.structure())
                    .into_iter()
                    .map(|g| g.name)
                    .collect()
            })
            .unwrap_or_default();
        let blocked = self.sizing_blocked();
        let fresh = w
            .result
            .as_ref()
            .is_some_and(|r| r.revision == revision && r.settings == w.settings);
        let mut open = true;
        let mut run = false;
        let mut apply = false;
        let mut cancel = false;
        egui::Window::new("Choose the members")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(760.0)
            .show(ctx, |ui| {
                ohl_ui::tool_window::intro(ui, INTRO, MORE, SIZING_ANCHOR, || {
                    crate::help::open_section(SIZING_PAGE.0, SIZING_PAGE.1)
                });
                setup(ui, &mut w.settings, &groups);
                ui.separator();
                if let Some(b) = &blocked {
                    crate::app::error_label(ui, b.as_str());
                }
                if let Some(e) = &w.error {
                    crate::app::error_label(ui, e.as_str());
                }
                if let Some(job) = &w.job {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!(
                            "Choosing… {}",
                            job.step.lock().map(|s| s.clone()).unwrap_or_default()
                        ));
                    });
                } else if let Some(r) = &w.result {
                    if !fresh {
                        ui.label(
                            egui::RichText::new(
                                "The structure or the settings changed since: choose again \
                                 before applying.",
                            )
                            .color(ui.visuals().warn_fg_color),
                        );
                    }
                    proposal(ui, &r.sizing, &mut w.accepted);
                }
                use ohl_ui::tool_window::{Keys, Pressed, buttons};
                let n = w
                    .result
                    .as_ref()
                    .map_or(0, |r| {
                        r.sizing
                            .groups
                            .iter()
                            .filter(|g| g.changed() && w.accepted.contains(&g.group.name))
                            .count()
                    });
                let (label, enabled, why) = if fresh {
                    (
                        format!("Apply ({n} group(s))"),
                        n > 0,
                        "No group ticked, or no change proposed",
                    )
                } else {
                    (
                        "Choose the members".to_owned(),
                        blocked.is_none() && w.job.is_none(),
                        if w.job.is_some() {
                            "Choosing: the window shows each step"
                        } else {
                            "The line above says why"
                        },
                    )
                };
                match buttons(ui, &label, enabled, Some(why), Keys::Escape) {
                    Some(Pressed::Action) if fresh => apply = true,
                    Some(Pressed::Action) => run = true,
                    Some(Pressed::Cancel) => cancel = true,
                    None => {}
                }
                if fresh
                    && ui
                        .small_button("Choose again")
                        .on_hover_text("Runs the sizer again with the settings above")
                        .clicked()
                {
                    run = true;
                }
            });
        if run && w.job.is_none() && blocked.is_none() {
            self.start_sizing(&mut w, ctx);
        }
        if apply && let Some(r) = &w.result {
            let accepted: Vec<String> = w.accepted.iter().cloned().collect();
            let n = r
                .sizing
                .groups
                .iter()
                .filter(|g| g.changed() && w.accepted.contains(&g.group.name))
                .count();
            let text = self.file.as_ref().map(|f| f.text()).unwrap_or_default();
            match sizing::apply(&text, &r.sizing, &accepted) {
                Ok(new) => {
                    if let Some(f) = self.file.as_mut() {
                        f.edit(&format!("Choose the members ({n} group(s))"), |doc, _| {
                            if let Ok(d) = new.parse::<toml_edit::DocumentMut>() {
                                *doc = d;
                            }
                        });
                    }
                    self.message = Some((
                        format!(
                            "{n} member group(s) given the sections chosen (one undo step); run \
                             Results to see the structure's usage"
                        ),
                        false,
                    ));
                    return;
                }
                Err(e) => w.error = Some(e),
            }
        }
        if !open || cancel {
            if let Some(job) = &w.job {
                job.cancel.cancel();
            }
            return;
        }
        self.sizing_window = Some(w);
    }
}

/// The numbered setup.
fn setup(ui: &mut egui::Ui, s: &mut Settings, groups: &[String]) {
    ohl_ui::tool_window::heading(
        ui,
        "1. Target",
        "The largest usage a chosen section may have in any load case, by the structure's design \
         code (1 = 100 % of its capacity). Lower keeps a reserve: heavier sections. The time \
         hardly changes (each section tried is one check of every group at once).",
    );
    ui.horizontal(|ui| {
        ohl_ui::tool_window::label(
            ui,
            "Target usage (%)",
            "Default 100 %. A section is taken only when every member of its group stays at or \
             below it in every case; the proposal's run says every member above it.",
        );
        let mut pct = s.target * 100.0;
        if ui
            .add(egui::DragValue::new(&mut pct).range(10.0..=200.0).speed(1.0).suffix(" %"))
            .changed()
        {
            s.target = pct / 100.0;
        }
    });
    ohl_ui::tool_window::heading(
        ui,
        "2. Limits",
        "The code's limits (slenderness, flat width to thickness, least thickness) come from the \
         design code's plugin and are always kept. The joint-fit limits keep a member \
         connectable: its leg takes its bolt at mid-leg (the hole clear of the root fillet, the \
         code's edge distance to the toe) and it is long enough for the bolts its end force needs \
         (the code's end distance and spacing, one line of bolts).",
    );
    ui.checkbox(&mut s.joint_fit, "Keep to the joint-fit limits")
        .on_hover_text(
            "On (the default): a section whose leg cannot take its bolt, or a member too short for \
             its bolts, is not taken. Off: the check alone; the proposal then says how many groups \
             have joint-fit problems. Effect: after a run the window says how many groups the \
             limits kept heavier and by how much mass (measured, worked out in the same run: no \
             extra time).",
        );
    egui::CollapsingHeader::new("3. Groups")
        .id_salt("sizing-groups")
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(
                    "Split: each member of the group (with its mirror images) gets its own \
                     section. Keep: the group is left as it is.",
                )
                .small(),
            );
            egui::ScrollArea::vertical()
                .id_salt("sizing-groups-list")
                .max_height(140.0)
                .show(ui, |ui| {
                    egui::Grid::new("sizing-groups-grid")
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong("Group");
                            ui.strong("Split");
                            ui.strong("Keep");
                            ui.end_row();
                            for g in groups {
                                ui.label(g);
                                tick(ui, &mut s.split, g);
                                tick(ui, &mut s.keep, g);
                                ui.end_row();
                            }
                        });
                });
        });
    egui::CollapsingHeader::new("4. Model")
        .id_salt("sizing-model")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ohl_ui::tool_window::label(
                    ui,
                    "Rounds at most",
                    "Default 10. Each round solves every load case with the sections chosen and \
                     chooses again; a structure usually settles in two to four. Not settled \
                     within them: the heavier of the last two choices, said. Time: one solve of \
                     every case and one check per section tried, per round.",
                );
                ui.add(egui::DragValue::new(&mut s.rounds).range(1..=50));
            });
        });
}

/// A tick for `name` in `list`.
fn tick(ui: &mut egui::Ui, list: &mut Vec<String>, name: &str) {
    let mut on = list.iter().any(|x| x == name);
    if ui.checkbox(&mut on, "").changed() {
        if on {
            list.push(name.to_owned());
        } else {
            list.retain(|x| x != name);
        }
    }
}

/// A usage in percent.
fn pct(u: Option<f64>) -> String {
    u.map_or("–".to_owned(), |u| format!("{:.0} %", u * 100.0))
}

/// The proposal: the totals, the notes, the table.
fn proposal(ui: &mut egui::Ui, s: &Sizing, accepted: &mut BTreeSet<String>) {
    let changed = s.groups.iter().filter(|g| g.changed()).count();
    let trials: usize = s.rounds.iter().map(|r| r.trials).sum();
    ui.label(format!(
        "{changed} of {} group(s) changed: mass {:.0} → {:.0} kg, largest usage {} → {}, joint-fit \
         problems {} → {}. Worked out in {:.1} s ({} round(s), {trials} trial(s); measured).",
        s.groups.len(),
        s.mass[0],
        s.mass[1],
        pct(s.usage[0]),
        pct(s.usage[1]),
        s.fit[0],
        s.fit[1],
        s.took.as_secs_f64(),
        s.rounds.len()
    ));
    ui.label(egui::RichText::new(&s.fit_effect).small());
    let warn = ui.visuals().warn_fg_color;
    match &s.settled {
        sizing::Settled::Settled => {}
        sizing::Settled::Cycle(x) | sizing::Settled::NotSettled(x) => {
            ui.label(egui::RichText::new(x).color(warn));
        }
    }
    for n in &s.notes {
        let text = egui::RichText::new(n);
        ui.label(if n.starts_with("NOT ON THE SAFE SIDE") || n.contains("off:") {
            text.color(warn)
        } else {
            text.small()
        });
    }
    if !s.over_target.is_empty() {
        ui.label(
            egui::RichText::new(format!(
                "{} member(s) above the target in the run with the chosen sections:",
                s.over_target.len()
            ))
            .color(warn),
        );
        egui::ScrollArea::vertical()
            .id_salt("sizing-over")
            .max_height(80.0)
            .show(ui, |ui| {
                for o in &s.over_target {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} {:.0} % in {} ({})",
                            o.member,
                            o.usage * 100.0,
                            o.case,
                            o.governs
                        ))
                        .color(warn),
                    );
                }
            });
    }
    ui.horizontal(|ui| {
        if ui.small_button("Tick all changes").clicked() {
            accepted.extend(
                s.groups
                    .iter()
                    .filter(|g| g.changed())
                    .map(|g| g.group.name.clone()),
            );
        }
        if ui.small_button("Untick all").clicked() {
            accepted.clear();
        }
    });
    egui::ScrollArea::both()
        .id_salt("sizing-table")
        .max_height(300.0)
        .show(ui, |ui| {
            egui::Grid::new("sizing-grid")
                .striped(true)
                .min_col_width(24.0)
                .show(ui, |ui| {
                    for h in [
                        "Take",
                        "Group",
                        "Members",
                        "Section",
                        "Mass (kg)",
                        "Usage",
                        "Governing case",
                        "Joint fit",
                    ] {
                        ui.strong(h);
                    }
                    ui.end_row();
                    for g in &s.groups {
                        let name = &g.group.name;
                        let mut on = accepted.contains(name);
                        let r = ui.add_enabled(g.changed(), egui::Checkbox::without_text(&mut on));
                        if r.changed() {
                            if on {
                                accepted.insert(name.clone());
                            } else {
                                accepted.remove(name);
                            }
                        }
                        ui.label(name).on_hover_text(g.group.members.join(", "));
                        ui.label(g.group.members.len().to_string());
                        let section = if g.changed() {
                            format!("{} → {}", g.from, g.to)
                        } else {
                            format!("{} (kept)", g.from)
                        };
                        let mut why = g.why.clone();
                        if let Some(x) = &g.without_fit {
                            why.push_str(&format!(
                                "; without the joint-fit limits: {x}"
                            ));
                        }
                        for r in &g.removed {
                            why.push_str(&format!("; removed: {r}"));
                        }
                        ui.label(section).on_hover_text(why);
                        ui.label(format!("{:.1} → {:.1}", g.mass[0], g.mass[1]));
                        let usage = egui::RichText::new(format!(
                            "{} → {}",
                            pct(g.usage[0]),
                            pct(g.usage[1])
                        ));
                        ui.label(if g.over_target { usage.color(warn) } else { usage })
                            .on_hover_text(&g.governs);
                        ui.label(&g.case[1]);
                        ui.label(format!("{} → {}", g.fit[0], g.fit[1]));
                        ui.end_row();
                    }
                });
        });
}
