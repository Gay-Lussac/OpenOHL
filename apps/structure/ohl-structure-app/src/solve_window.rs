// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Solve the clashes** (O44 L11, the owner, 2026-11-21: not "this huge
//! list that's a bit unreadable and where buttons don't seem to do much",
//! but "all the errors in a menu that can be opened and the automatic
//! layout should find the best solution to offer to solve all of it and
//! that can be put in place with a simple click"). Joint details' Problems
//! open it: the joint's best whole solution worked out on copies of the
//! structure on a thread of its own ([`ohl_structure_link::solve_joint`]),
//! what it changes (the steel and what the analysis takes marked), the
//! clashes and problems before and after, the joint's close-up before and
//! after, on request the members' usage before and after, a leg turn the
//! joints call for only as a proposal ticked by hand, and **Apply**: one
//! undo step. Escape cancels; nothing is written before Apply. The
//! tool-window pattern (`docs/development/tool-pages.md`), its section of
//! the manual [`SOLVE_PAGE`].

use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use ohl_structure_link::{Candidate, Effect, EffectKind, JointDetails, Solution, SolveAsk};

use crate::app::StructureApp;
use crate::layout_window::UsageCompare;

/// The manual's section of the window (page, anchor).
pub const SOLVE_PAGE: (&str, &str) = ("user-guide/joint-layout", SOLVE_ANCHOR);
const SOLVE_ANCHOR: &str = "solve-the-clashes";

/// A solution and the joint in detail before, after it, and after each leg
/// turn proposed.
pub struct Solved {
    /// What it is for: the file's revision, the libraries' stamp.
    pub key: u64,
    /// The file's revision it was worked out on.
    pub revision: u64,
    /// The solution.
    pub solution: Solution,
    /// The joint as it is.
    pub before: JointDetails,
    /// After the solution chosen (none: none).
    pub after: Option<JointDetails>,
    /// After each leg turn's solution (in `solution.leg_turns`' order).
    pub legs: Vec<Option<JointDetails>>,
}

/// The Solve the clashes window.
pub struct SolveWindow {
    /// The joint.
    pub joint: String,
    /// The solution.
    pub solved: Option<Solved>,
    job: Option<(u64, Receiver<Result<Solved, String>>)>,
    /// Why there is none.
    pub error: Option<String>,
    /// The leg turn proposed that is ticked (an index into the solution's
    /// leg turns): applied instead of the solution chosen.
    pub leg: Option<usize>,
    /// The close-ups' views (before, after).
    pub views: Box<(
        crate::joint_panel::JointPanel,
        crate::joint_panel::JointPanel,
    )>,
    /// The usage before and after.
    pub usage: Option<UsageCompare>,
    usage_job: crate::layout_window::Job<UsageCompare>,
}

impl SolveWindow {
    /// What is applied: the leg turn ticked, else the solution chosen.
    pub fn applied(&self) -> Option<&Candidate> {
        let s = self.solved.as_ref()?;
        match self.leg {
            Some(k) => s.solution.leg_turns.get(k)?.with.as_ref(),
            None => s.solution.best(),
        }
    }

    /// The joint in detail after what is applied.
    fn after(&self) -> Option<&JointDetails> {
        let s = self.solved.as_ref()?;
        match self.leg {
            Some(k) => s.legs.get(k)?.as_ref(),
            None => s.after.as_ref(),
        }
    }

    /// The usage's key: the solution's and what is applied.
    fn usage_key(&self) -> u64 {
        let k = self.solved.as_ref().map_or(0, |s| s.key);
        k.wrapping_mul(31)
            .wrapping_add(self.leg.map_or(0, |x| x as u64 + 1))
    }
}

impl StructureApp {
    /// Opens Solve the clashes for joint `joint`.
    pub(crate) fn open_solve_window(&mut self, joint: &str) {
        self.solve_window = Some(SolveWindow {
            joint: joint.to_owned(),
            solved: None,
            job: None,
            error: None,
            leg: None,
            views: {
                let mut v: Box<(
                    crate::joint_panel::JointPanel,
                    crate::joint_panel::JointPanel,
                )> = Box::default();
                v.0.zoom = -0.4;
                v.1.zoom = -0.4;
                v
            },
            usage: None,
            usage_job: None,
        });
    }

    /// The Solve the clashes window ([`Self::open_solve_window`]).
    pub(crate) fn solve_window(&mut self, ctx: &egui::Context) {
        let Some(mut w) = self.solve_window.take() else {
            return;
        };
        let Some((revision, text)) = self.file.as_ref().map(|f| (f.revision(), f.text())) else {
            return;
        };
        let key = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (revision, self.libs_stamp, &w.joint).hash(&mut h);
            h.finish()
        };
        if let Some((k, rx)) = &w.job {
            match rx.try_recv() {
                Ok(r) => {
                    if *k == key {
                        match r {
                            Ok(s) => {
                                w.solved = Some(s);
                                w.error = None;
                                w.leg = None;
                            }
                            Err(e) => w.error = Some(e),
                        }
                    }
                    w.job = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
                Err(_) => w.job = None,
            }
        }
        let fresh = w.solved.as_ref().is_some_and(|s| s.key == key);
        if !fresh
            && w.job.as_ref().is_none_or(|(k, _)| *k != key)
            && let Some((settings, fields, leg_bolt)) = self.layout_settings_now()
        {
            let lib = self.loads.sections.clone();
            let bolts = self.bolt_library();
            let joint = w.joint.clone();
            let (tx, rx) = channel();
            let c = ctx.clone();
            std::thread::spawn(move || {
                let r = (|| {
                    let ask = SolveAsk {
                        library: &lib,
                        bolts: &bolts.0,
                        settings: &settings,
                        preset_fields: fields,
                        leg_bolt,
                        joint: joint.clone(),
                    };
                    let solution = ohl_structure_link::solve_joint(&text, &ask)?;
                    let only = std::collections::BTreeSet::from([joint.clone()]);
                    let at = |t: &str| {
                        ohl_structure_link::joint_details_at(t, &lib, &bolts.0, Some(&only))
                    };
                    let before = at(&text)?;
                    let after = match solution.best() {
                        Some(b) => Some(at(&b.text)?),
                        None => None,
                    };
                    let mut legs = Vec::new();
                    for l in &solution.leg_turns {
                        legs.push(match &l.with {
                            Some(c) => Some(at(&c.text)?),
                            None => None,
                        });
                    }
                    Ok(Solved {
                        key,
                        revision,
                        solution,
                        before,
                        after,
                        legs,
                    })
                })();
                let _ = tx.send(r);
                c.request_repaint();
            });
            w.job = Some((key, rx));
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some(e) = crate::layout_window::poll_usage_job(&mut w.usage_job, &mut w.usage, ctx) {
            self.message = Some((format!("usage before and after: {e}"), true));
        }
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        let mut usage_asked = false;
        egui::Window::new("Solve the clashes")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(560.0)
            .show(ctx, |ui| {
                let dark = ui.visuals().dark_mode;
                ohl_ui::tool_window::intro(
                    ui,
                    "Works out the best whole solution for this joint's clashes and problems, \
                     on copies of the structure: one click applies it.",
                    "Tried in order, each on a copy: the automatic layout of the joint (values \
                     given by hand kept, then replaced); where the layout proposes pieces in \
                     line one part or a leg splice place here, the same with those confirmed \
                     (marked below, never silent); then, on the best so far, the fixes of the \
                     problems left, one at a time, each kept only where the joint has fewer \
                     problems after it. The best is the one leaving the fewest clashes, then \
                     problems, then members left for a hand detail. A way is refused where it \
                     pulls back a leg or a member running on through the joint, turns a leg, \
                     changes a member's section, or leaves more clashes and problems at the \
                     other joints of the members meeting here. A leg piece the joints say is \
                     turned is only proposed, with its effect measured: tick it to apply it. \
                     Nothing is written before Apply (one undo step).",
                    SOLVE_ANCHOR,
                    || crate::help::open_section(SOLVE_PAGE.0, SOLVE_PAGE.1),
                );
                egui::ScrollArea::vertical()
                    .id_salt("solve-window-body")
                    .max_height(420.0)
                    .show(ui, |ui| {
                        ui.strong(format!("Joint {}", w.joint));
                        if let Some(e) = &w.error {
                            crate::app::error_label(ui, e.clone());
                        }
                        let Some(s) = w.solved.as_ref().filter(|s| s.key == key) else {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Working out the solution on copies of the structure…");
                            });
                            return;
                        };
                        let sol = &s.solution;
                        let b = sol.before;
                        match sol.best() {
                            Some(c) => {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Clashes {} to {}, other problems {} to {}{}",
                                        b.clashes,
                                        c.counts.clashes,
                                        b.problems,
                                        c.counts.problems,
                                        if c.counts.left > 0 {
                                            format!(
                                                "; {} member(s) left for a hand detail",
                                                c.counts.left
                                            )
                                        } else {
                                            String::new()
                                        }
                                    ))
                                    .strong(),
                                );
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Chosen: {}. Worked out in {:.1} s (measured).",
                                        c.label, sol.seconds
                                    ))
                                    .small()
                                    .weak(),
                                );
                            }
                            None => {
                                ui.colored_label(
                                    ui.visuals().warn_fg_color,
                                    format!(
                                        "No better solution found ({} clash(es), {} other \
                                         problem(s) now): {}",
                                        b.clashes,
                                        b.problems,
                                        sol.why_none.as_deref().unwrap_or("")
                                    ),
                                );
                            }
                        }
                        // The leg turns proposed (never applied unless
                        // ticked).
                        if !sol.leg_turns.is_empty() {
                            ohl_ui::tool_window::heading(
                                ui,
                                "Proposed, not applied",
                                "A leg piece, or a chord running on through the joint (an \
                                 arm's chord, a body's member running on), whose joints say it \
                                 is turned about its axis: the bracing ends meeting it lie in \
                                 planes turned against its flanges by about the same angle (the \
                                 tower's faces through the neighbouring legs or chords are a \
                                 hint only). Never applied silently: tick it to apply the \
                                 solution worked out with that turn instead (the pieces in line \
                                 with it turned alike, so that the whole leg or chord keeps \
                                 matching).",
                            );
                            for (k, l) in sol.leg_turns.iter().enumerate() {
                                let mut on = w.leg == Some(k);
                                let words = match &l.with {
                                    Some(c) => format!(
                                        "Turn {} by {:+.1}°: with it clashes {} to {}, other \
                                         problems {} to {}",
                                        l.members.join(", "),
                                        l.roll.turn.unwrap_or(0.0),
                                        b.clashes,
                                        c.counts.clashes,
                                        b.problems,
                                        c.counts.problems
                                    ),
                                    None => format!(
                                        "Turn {} by {:+.1}°: no better solution with it",
                                        l.members.join(", "),
                                        l.roll.turn.unwrap_or(0.0)
                                    ),
                                };
                                let r = ui
                                    .add_enabled(
                                        l.with.is_some(),
                                        egui::Checkbox::new(&mut on, words),
                                    )
                                    .on_hover_text(l.roll.words());
                                if r.changed() {
                                    w.leg = on.then_some(k);
                                }
                            }
                        }
                        if let Some(c) = w.applied() {
                            ohl_ui::tool_window::heading(
                                ui,
                                "What it changes",
                                "Each change at the joint, as the joint details read the file \
                                 after it. Marked: what changes the steel's place or what the \
                                 analysis takes (a member turned, an end moved sideways — an \
                                 eccentricity the analysis takes, a proposal of the layout \
                                 confirmed). A member left for a hand detail is said with why.",
                            );
                            effects(ui, &c.effects);
                        }
                        // The close-ups before and after.
                        let pair = (
                            s.before.joint(&w.joint).cloned(),
                            w.after().and_then(|d| d.joint(&w.joint)).cloned(),
                        );
                        if let (Some(a), Some(after)) = pair {
                            ui.label("Before and after:");
                            let k = w.usage_key();
                            crate::layout_window::before_after(
                                ui,
                                &mut w.views,
                                &a,
                                &after,
                                k,
                                dark,
                            );
                        }
                        if w.applied().is_some() {
                            usage_asked = crate::layout_window::layout_usage(
                                ui,
                                w.usage.as_ref(),
                                w.usage_key(),
                                w.usage_job.is_some(),
                            );
                        }
                        egui::CollapsingHeader::new(format!(
                            "Everything tried ({})",
                            sol.tried.len()
                        ))
                        .id_salt("solve-tried")
                        .show(ui, |ui| {
                            for (k, c) in sol.tried.iter().enumerate() {
                                let mark = if Some(k) == sol.chosen {
                                    "chosen: "
                                } else {
                                    ""
                                };
                                let text = match &c.refused {
                                    Some(r) => format!("{mark}{} — refused: {r}", c.label),
                                    None => format!(
                                        "{mark}{} — {} clash(es), {} other problem(s), {} left \
                                         for a hand detail",
                                        c.label, c.counts.clashes, c.counts.problems, c.counts.left
                                    ),
                                };
                                ui.label(egui::RichText::new(text).small());
                            }
                        });
                    });
                let ready = w.applied().is_some()
                    && w.solved
                        .as_ref()
                        .is_some_and(|s| s.key == key && s.revision == revision);
                use ohl_ui::tool_window::{Keys, Pressed, buttons};
                match buttons(
                    ui,
                    "Apply",
                    ready,
                    Some(if w.solved.as_ref().is_some_and(|s| s.key == key) {
                        "No better solution to apply"
                    } else {
                        "The solution is being worked out"
                    }),
                    Keys::Escape,
                ) {
                    Some(Pressed::Action) => apply = true,
                    Some(Pressed::Cancel) => cancel = true,
                    None => {}
                }
            });
        if usage_asked {
            let before = text_of(self);
            if let (Some(before), Some(c)) = (before, w.applied()) {
                let k = w.usage_key();
                if let Some(rx) = self.usage_thread(before, c.text.clone(), k) {
                    w.usage_job = Some((k, rx));
                }
            }
        }
        if apply && let Some(c) = w.applied().cloned() {
            let b = w
                .solved
                .as_ref()
                .map(|s| s.solution.before)
                .unwrap_or_default();
            if let Some(f) = self.file.as_mut() {
                f.edit_alone(&format!("Solve the clashes at {}", w.joint), |doc, _| {
                    if let Ok(d) = c.text.parse::<toml_edit::DocumentMut>() {
                        *doc = d;
                    }
                });
            }
            self.message = Some((
                format!(
                    "{}: clashes {} to {}, other problems {} to {} (one undo step)",
                    w.joint, b.clashes, c.counts.clashes, b.problems, c.counts.problems
                ),
                false,
            ));
            return;
        }
        if open && !cancel {
            self.solve_window = Some(w);
        }
    }
}

fn text_of(app: &StructureApp) -> Option<String> {
    app.file.as_ref().map(|f| f.text())
}

/// The effects, one per line: a small tag of its kind (marked ones in the
/// warning colour), then its words.
fn effects(ui: &mut egui::Ui, list: &[Effect]) {
    egui::ScrollArea::vertical()
        .id_salt("solve-effects")
        .max_height(180.0)
        .show(ui, |ui| {
            for e in list {
                ui.horizontal_wrapped(|ui| {
                    let tag = match e.kind {
                        EffectKind::Confirmed => "confirmed",
                        EffectKind::Turned => "turned",
                        EffectKind::Cut => "steel stops",
                        EffectKind::Moved => "moved sideways",
                        EffectKind::Onto => "bolted onto",
                        EffectKind::PlateAdded => "plate added",
                        EffectKind::PlateRemoved => "plate taken away",
                        EffectKind::Fixed => "fixed",
                        EffectKind::Left => "left",
                    };
                    let color = if e.marked() || e.kind == EffectKind::Left {
                        ui.visuals().warn_fg_color
                    } else {
                        ui.visuals().weak_text_color()
                    };
                    ui.label(
                        egui::RichText::new(format!("[{tag}]"))
                            .small()
                            .strong()
                            .color(color),
                    );
                    ui.label(egui::RichText::new(&e.words).small());
                });
            }
        });
}
