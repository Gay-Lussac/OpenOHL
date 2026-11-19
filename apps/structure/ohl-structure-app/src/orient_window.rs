// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Orient angles** (2026-11-18): Design › Orient angles… opens one tool
//! window (the tool-window pattern) that turns a tower's legs and chords
//! heel out with their flanges in the tower's faces and lays its plan and
//! hip members flat ([`ohl_structure_link::orient_angles`]) — for a tower
//! without orientation data (an import, a tower drawn by hand). Which
//! members (those picked, a member group's, the whole tower), whether
//! rolls given by hand are replaced; a preview worked out on a copy (on a
//! thread of its own): what turns and by how much, the legs and chords
//! left with why, the rolls given by hand kept. **Apply** writes it as one
//! undo step (a joint layout near the members follows, in the same step).

use std::collections::BTreeSet;
use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use ohl_structure_edit::Handle;
use ohl_structure_link::{OrientAsk, OrientKind, OrientRun};

use crate::app::StructureApp;

/// The manual's section of the window (page, anchor).
pub const ORIENT_PAGE: (&str, &str) = ("user-guide/joint-layout", ORIENT_ANCHOR);
const ORIENT_ANCHOR: &str = "orient-angles";

/// Which members.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrientScope {
    /// The members picked.
    Picked,
    /// A member group's.
    Group(String),
    /// Every member.
    Tower,
}

/// The preview: a run on a copy.
pub struct OrientPreview {
    /// The inputs it was worked out for.
    pub key: u64,
    /// The file's revision it was worked out on.
    pub revision: u64,
    /// The run.
    pub run: OrientRun,
}

type Job = Option<(u64, Receiver<Result<OrientPreview, String>>)>;

/// The Orient angles window.
pub struct OrientWindow {
    /// Which members.
    pub scope: OrientScope,
    /// Rolls given by hand replaced too.
    pub replace: bool,
    /// The preview.
    pub preview: Option<OrientPreview>,
    job: Job,
    /// Why there is no preview.
    pub error: Option<String>,
}

impl OrientWindow {
    fn key(&self, revision: u64, libs: u64, members: &Option<BTreeSet<String>>) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (revision, libs, members, self.replace).hash(&mut h);
        h.finish()
    }
}

impl StructureApp {
    /// Opens the Orient angles window: the members picked when some are,
    /// else the whole tower.
    pub(crate) fn open_orient_window(&mut self) {
        let picked = self.selected.iter().any(|h| matches!(h, Handle::Member(_)));
        self.orient_window = Some(OrientWindow {
            scope: if picked {
                OrientScope::Picked
            } else {
                OrientScope::Tower
            },
            replace: false,
            preview: None,
            job: None,
            error: None,
        });
    }

    /// The members the window's scope names (none: every member).
    fn orient_members(&self, w: &OrientWindow) -> Option<BTreeSet<String>> {
        let f = self.file.as_ref()?;
        match &w.scope {
            OrientScope::Tower => None,
            OrientScope::Picked => Some(
                self.selected
                    .iter()
                    .filter_map(|h| match h {
                        Handle::Member(m) => Some(m.clone()),
                        _ => None,
                    })
                    .collect(),
            ),
            OrientScope::Group(g) => Some(
                f.structure()
                    .members
                    .iter()
                    .filter(|m| m.group.as_deref() == Some(g.as_str()))
                    .map(|m| m.id.clone())
                    .collect(),
            ),
        }
    }

    /// The Orient angles window ([`Self::open_orient_window`]); its
    /// section of the manual: [`ORIENT_PAGE`].
    pub(crate) fn orient_window(&mut self, ctx: &egui::Context) {
        let Some(mut w) = self.orient_window.take() else {
            return;
        };
        let members = self.orient_members(&w);
        let (revision, text) = match self.file.as_ref() {
            Some(f) => (f.revision(), f.text()),
            None => return,
        };
        let key = w.key(revision, self.libs_stamp, &members);
        if let Some((k, rx)) = &w.job {
            match rx.try_recv() {
                Ok(r) => {
                    if *k == key {
                        match r {
                            Ok(p) => {
                                w.preview = Some(p);
                                w.error = None;
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
        let fresh = w.preview.as_ref().is_some_and(|p| p.key == key);
        if !fresh && w.job.as_ref().is_none_or(|(k, _)| *k != key) {
            let lib = self.loads.sections.clone();
            let ask = OrientAsk {
                members: members.clone(),
                replace_hand: w.replace,
            };
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                let r = ohl_structure_link::orient_angles(&text, &lib, &ask)
                    .map(|run| OrientPreview { key, revision, run });
                let _ = tx.send(r);
            });
            w.job = Some((key, rx));
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
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
        let picked = self
            .selected
            .iter()
            .filter(|h| matches!(h, Handle::Member(_)))
            .count();
        let all = self
            .file
            .as_ref()
            .map_or(0, |f| f.structure().members.len());
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        egui::Window::new("Orient angles")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(520.0)
            .show(ctx, |ui| {
                ohl_ui::tool_window::intro(
                    ui,
                    "Turns the tower's legs and chords heel out with their flanges in its faces, \
                     lays the members of its faces in them and its plan and hip members flat — \
                     for a tower without orientation data (an import, a tower drawn by hand).",
                    "Legs and chords: the members of class leg and the angles running on in line \
                     (within 10°) through a joint. Each is turned so that its heel points out of \
                     the tower and its two flanges lie in the two faces meeting at it — the faces \
                     of its neighbouring legs' lines (the tower's section through its middle). \
                     Where the faces do not meet at 90°, the middle of the angle's legs is put on \
                     the middle of the faces (each flange off its face by half the difference, \
                     said). Plan and hip members (horizontal angles in no steep face): one leg \
                     flat, the other pointing down (assumed: no water lying in it). Face members \
                     (the other angles lying in a steep face, between its legs or chords): one \
                     leg in the face, the other out of the tower, the leg in the face down from \
                     the heel where it can (assumed: no water lying in it). The rest is left to \
                     Lay out joints, which lays it flat where that helps. A roll given \
                     by hand is kept unless replaced. Nothing is written before Apply (one undo \
                     step); what is set is marked automatic.",
                    ORIENT_ANCHOR,
                    || crate::help::open_section(ORIENT_PAGE.0, ORIENT_PAGE.1),
                );
                ohl_ui::tool_window::heading(
                    ui,
                    "Members",
                    "Which members are turned: of those, the legs and chords and the plan and \
                     hip members (the tower's faces are found from all its legs and chords).",
                );
                ui.radio_value(
                    &mut w.scope,
                    OrientScope::Picked,
                    format!("The members picked ({picked})"),
                );
                ui.horizontal(|ui| {
                    let in_group = matches!(w.scope, OrientScope::Group(_));
                    if ui.radio(in_group, "A member group's").clicked()
                        && !in_group
                        && let Some(g) = groups.first()
                    {
                        w.scope = OrientScope::Group(g.clone());
                    }
                    if let OrientScope::Group(g) = &mut w.scope {
                        egui::ComboBox::from_id_salt("orient-group")
                            .selected_text(g.clone())
                            .show_ui(ui, |ui| {
                                for x in &groups {
                                    ui.selectable_value(g, x.clone(), x);
                                }
                            });
                    }
                });
                ui.radio_value(
                    &mut w.scope,
                    OrientScope::Tower,
                    format!("The whole tower ({all} members)"),
                );
                ui.checkbox(&mut w.replace, "Replace rolls given by hand")
                    .on_hover_text(
                        "Off: a member's roll, reference or orientation given by hand is kept \
                         (listed below). On: replaced by the tower's shape — for an import whose \
                         rolls are not as built.",
                    );
                ui.separator();
                if let Some(e) = &w.error {
                    crate::app::error_label(ui, e.clone());
                }
                match w.preview.as_ref().filter(|p| p.key == key) {
                    None => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Working it out on a copy…");
                        });
                    }
                    Some(p) => orient_preview(ui, &p.run),
                }
                let ready = w
                    .preview
                    .as_ref()
                    .is_some_and(|p| p.key == key && p.revision == revision);
                let n = w.preview.as_ref().map_or(0, |p| p.run.turned.len());
                use ohl_ui::tool_window::{Keys, Pressed, buttons};
                match buttons(
                    ui,
                    &format!("Apply ({n} members)"),
                    ready && n > 0,
                    Some(if ready {
                        "Nothing to turn"
                    } else {
                        "The preview is being worked out"
                    }),
                    Keys::Escape,
                ) {
                    Some(Pressed::Action) => apply = true,
                    Some(Pressed::Cancel) => cancel = true,
                    None => {}
                }
            });
        if apply && let Some(p) = w.preview.take() {
            let n = p.run.turned.len();
            let text = p.run.text.clone();
            if let Some(f) = self.file.as_mut() {
                // With the layout's follow: laid-out joints at the members
                // turned are laid out again in the same undo step.
                f.edit(&format!("Orient angles ({n})"), |doc, _| {
                    if let Ok(d) = text.parse::<toml_edit::DocumentMut>() {
                        *doc = d;
                    }
                });
            }
            let count = |k: OrientKind| p.run.turned.iter().filter(|o| o.kind == k).count();
            let chords = count(OrientKind::Chord);
            self.message = Some((
                format!(
                    "{n} member(s) turned ({chords} leg(s) and chord(s), {} face member(s), {} \
                     plan member(s); one undo step){}",
                    count(OrientKind::Face),
                    count(OrientKind::Plan),
                    if p.run.left.is_empty() {
                        String::new()
                    } else {
                        format!("; {} leg(s) left (the window says why)", p.run.left.len())
                    }
                ),
                false,
            ));
            return;
        }
        if open && !cancel {
            self.orient_window = Some(w);
        }
    }
}

/// The preview's numbers and lists.
fn orient_preview(ui: &mut egui::Ui, r: &OrientRun) {
    let count = |k: OrientKind| r.turned.iter().filter(|o| o.kind == k).count();
    ui.label(format!(
        "{} member(s) turned: {} leg(s) and chord(s), {} face member(s) (bracing in a face), {} \
         plan or hip member(s); {} already so; {} roll(s) given by hand kept; {} roll(s) the \
         joint layout chose for both ends kept; {} leg(s) left. Worked out in {:.2} s \
         (measured).",
        r.turned.len(),
        count(OrientKind::Chord),
        count(OrientKind::Face),
        count(OrientKind::Plan),
        r.same.len(),
        r.kept.len(),
        r.by_layout.len(),
        r.left.len(),
        r.seconds
    ));
    let worst = r
        .turned
        .iter()
        .chain(&r.same)
        .filter(|o| o.kind == OrientKind::Chord)
        .map(|o| o.off)
        .fold(0.0, f64::max);
    if worst > 0.05 {
        ui.label(
            egui::RichText::new(format!(
                "Where the faces do not meet at 90°, each flange lies off its face by half the \
                 difference: up to {worst:.1}° here{}",
                if worst > ohl_structure_link::FLAT_OFF {
                    " — more than the 2° a member is laid flat within: bracing there needs a \
                     bent plate or a hand detail"
                } else {
                    ""
                }
            ))
            .small()
            .color(if worst > ohl_structure_link::FLAT_OFF {
                ui.visuals().warn_fg_color
            } else {
                ui.visuals().weak_text_color()
            }),
        );
    }
    let line = |o: &ohl_structure_link::OrientedMember| {
        format!(
            "{}: {} {:+.1}° (roll {}{} to {:.2}°){}",
            o.member,
            match o.kind {
                OrientKind::Chord => "leg/chord",
                OrientKind::Plan => "plan member",
                OrientKind::Face => "face member",
            },
            o.turn,
            if o.before.is_empty() { "none" } else { "" },
            o.before,
            o.roll,
            if o.kind == OrientKind::Chord && o.off > 0.05 {
                format!(", each flange {:.1}° off its face", o.off)
            } else {
                String::new()
            }
        )
    };
    egui::CollapsingHeader::new(format!("What turns ({})", r.turned.len()))
        .id_salt("orient-turned")
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("orient-turned-list")
                .max_height(160.0)
                .show(ui, |ui| {
                    for o in &r.turned {
                        ui.label(line(o));
                    }
                });
        });
    if !r.kept.is_empty() {
        egui::CollapsingHeader::new(format!("Rolls given by hand kept ({})", r.kept.len()))
            .id_salt("orient-kept")
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("orient-kept-list")
                    .max_height(120.0)
                    .show(ui, |ui| {
                        for o in &r.kept {
                            ui.label(format!("{} (would turn {:+.1}°)", o.member, o.turn));
                        }
                    });
            });
    }
    if !r.by_layout.is_empty() {
        egui::CollapsingHeader::new(format!(
            "Rolls the joint layout chose for both ends kept ({})",
            r.by_layout.len()
        ))
        .id_salt("orient-by-layout")
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("orient-by-layout-list")
                .max_height(120.0)
                .show(ui, |ui| {
                    for o in &r.by_layout {
                        ui.label(format!(
                            "{} (from the shape alone: {:+.1}°)",
                            o.member, o.turn
                        ));
                    }
                });
        })
        .header_response
        .on_hover_text(
            "Lay out joints turned these so that both their ends lie flat (one roll for the \
             whole member); Orient angles keeps that, so the two agree",
        );
    }
    egui::CollapsingHeader::new(format!("Legs left ({})", r.left.len()))
        .id_salt("orient-left")
        .default_open(!r.left.is_empty() && r.left.len() <= 5)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("orient-left-list")
                .max_height(120.0)
                .show(ui, |ui| {
                    for (m, why) in &r.left {
                        ui.label(
                            egui::RichText::new(format!("{m}: {why}"))
                                .color(ui.visuals().warn_fg_color),
                        );
                    }
                });
        });
}
