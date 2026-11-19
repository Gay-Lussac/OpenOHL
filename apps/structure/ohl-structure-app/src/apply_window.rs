// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **Apply this joint's detail to similar joints** (the automatic joint
//! layout's J4, 2026-11-19): from the joint's bar in Joint details, one
//! tool window (the tool-window pattern) that copies the joint's detail —
//! its member ends' connections and its plates, laid out or detailed by
//! hand — mirrored or turned onto the joints of its kind
//! ([`ohl_structure_link::apply_detail`]). A preview worked out on a copy
//! (on a thread of its own) lists each joint like it with how (mirrored,
//! turned, moved), what it changes there, and those left with why; the
//! joints ticked are the ones changed. **Apply** writes it as one undo step.

use std::collections::BTreeSet;
use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use ohl_structure_link::ApplyRun;

use crate::app::StructureApp;

/// The manual's section of the window (page, anchor).
pub const APPLY_PAGE: (&str, &str) = ("user-guide/joint-layout", APPLY_ANCHOR);
const APPLY_ANCHOR: &str = "apply-this-joints-detail-to-similar-joints";

/// The preview: the detail applied on a copy.
pub struct ApplyPreview {
    /// The inputs it was worked out for.
    pub key: u64,
    /// The file's revision it was worked out on.
    pub revision: u64,
    /// Applied to every joint like it (the list to tick from).
    pub all: ApplyRun,
    /// Applied to the joints ticked (what Apply writes).
    pub run: ApplyRun,
}

type Job = Option<(u64, Receiver<Result<ApplyPreview, String>>)>;

/// The window.
pub struct ApplyWindow {
    /// The joint whose detail is applied.
    pub joint: String,
    /// The joints like it left out (unticked).
    pub unticked: BTreeSet<String>,
    /// The preview.
    pub preview: Option<ApplyPreview>,
    job: Job,
    /// Why there is no preview.
    pub error: Option<String>,
}

impl ApplyWindow {
    /// For joint `joint`, every joint like it ticked.
    pub fn new(joint: &str) -> Self {
        Self {
            joint: joint.to_owned(),
            unticked: BTreeSet::new(),
            preview: None,
            job: None,
            error: None,
        }
    }

    fn key(&self, revision: u64, libs: u64) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (revision, libs, &self.joint, &self.unticked).hash(&mut h);
        h.finish()
    }
}

impl StructureApp {
    /// The Apply this joint's detail to similar joints window; its section
    /// of the manual: [`APPLY_PAGE`].
    pub(crate) fn apply_window(&mut self, ctx: &egui::Context) {
        let Some(mut w) = self.apply_window.take() else {
            return;
        };
        let bolts = self.bolt_library();
        let (revision, text) = match self.file.as_ref() {
            Some(f) => (f.revision(), f.text()),
            None => return,
        };
        let key = w.key(revision, self.libs_stamp);
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
            let joint = w.joint.clone();
            let unticked = w.unticked.clone();
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                let r = (|| {
                    let all =
                        ohl_structure_link::apply_detail(&text, &lib, &bolts.0, &joint, None)?;
                    let run = if unticked.is_empty() {
                        all.clone()
                    } else {
                        let only: BTreeSet<String> = all
                            .applied
                            .iter()
                            .map(|a| a.joint.clone())
                            .filter(|j| !unticked.contains(j))
                            .collect();
                        ohl_structure_link::apply_detail(
                            &text,
                            &lib,
                            &bolts.0,
                            &joint,
                            Some(&only),
                        )?
                    };
                    Ok(ApplyPreview {
                        key,
                        revision,
                        all,
                        run,
                    })
                })();
                let _ = tx.send(r);
            });
            w.job = Some((key, rx));
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        let title = format!("Apply joint {}'s detail to similar joints", w.joint);
        egui::Window::new(title)
            .id(egui::Id::new("apply-detail-window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(520.0)
            .show(ctx, |ui| {
                ohl_ui::tool_window::intro(
                    ui,
                    "Copies this joint's detail — each member end's connection and its plates — \
                     onto the joints like it, mirrored or turned: the tower's symmetry and its \
                     repeated panels.",
                    "A joint is like this one when a move with a mirror or a quarter, half or \
                     three-quarter turn about the vertical carries every member end of this \
                     joint onto one of it: the same direction and length (within a tenth of a \
                     degree and 2 mm), the same cross-section turned alike, the same class. Its \
                     own connections and plates are replaced by the copy: what an end bolts onto \
                     said with its own members, the plates placed there, the cuts, shifts and \
                     bolts as here. A laid-out detail stays automatic, one given by hand stays \
                     yours; a hand detail is copied too. Each copy is checked for clashes in its \
                     own place: a joint where it would run into more than here is left as it \
                     was. A joint with a splice is left (its covers by hand). Nothing is written \
                     before Apply (one undo step).",
                    APPLY_ANCHOR,
                    || crate::help::open_section(APPLY_PAGE.0, APPLY_PAGE.1),
                );
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
                    Some(p) => apply_preview(ui, p, &mut w.unticked),
                }
                let ready = w
                    .preview
                    .as_ref()
                    .is_some_and(|p| p.key == key && p.revision == revision);
                let n = w
                    .preview
                    .as_ref()
                    .filter(|p| p.key == key)
                    .map_or(0, |p| p.run.applied.len());
                use ohl_ui::tool_window::{Keys, Pressed, buttons};
                match buttons(
                    ui,
                    &format!("Apply ({n} joints)"),
                    ready && n > 0,
                    Some(if ready {
                        "No joint like it to change"
                    } else {
                        "The preview is being worked out"
                    }),
                    Keys::Both,
                ) {
                    Some(Pressed::Action) => apply = true,
                    Some(Pressed::Cancel) => cancel = true,
                    None => {}
                }
            });
        if apply && let Some(p) = w.preview.take() {
            let n = p.run.applied.len();
            let text = p.run.text.clone();
            let joints: Vec<String> = p.run.applied.iter().map(|a| a.joint.clone()).collect();
            if let Some(f) = self.file.as_mut() {
                f.edit(
                    &format!("Joint {}'s detail to {n} similar joint(s)", w.joint),
                    |doc, _| {
                        if let Ok(d) = text.parse::<toml_edit::DocumentMut>() {
                            *doc = d;
                        }
                    },
                );
            }
            self.message = Some((
                format!(
                    "joint {}'s detail on {n} joint(s) like it ({}; one undo step){}",
                    w.joint,
                    joints.join(", "),
                    if p.run.left.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "; {} left as they were (the window said why)",
                            p.run.left.len()
                        )
                    }
                ),
                false,
            ));
            return;
        }
        if open && !cancel {
            self.apply_window = Some(w);
        }
    }
}

/// The preview: each joint like it (ticked: changed), how, what changes;
/// those left with why.
fn apply_preview(ui: &mut egui::Ui, p: &ApplyPreview, unticked: &mut BTreeSet<String>) {
    let all = &p.all;
    ui.label(format!(
        "{} joint(s) like it{}. Worked out in {:.2} s (measured).",
        all.applied.len() + all.left.len(),
        if all.left.is_empty() {
            String::new()
        } else {
            format!(", {} of them left as they are", all.left.len())
        },
        p.all.seconds + if p.run == p.all { 0.0 } else { p.run.seconds }
    ));
    ohl_ui::tool_window::heading(
        ui,
        "The joints it changes",
        "Ticked: the joint's own connections and plates replaced by this joint's, mapped there. \
         Untick a joint to leave it as it is.",
    );
    egui::ScrollArea::vertical()
        .id_salt("apply-detail-list")
        .max_height(200.0)
        .show(ui, |ui| {
            for a in &all.applied {
                let mut on = !unticked.contains(&a.joint);
                let changed = ui
                    .checkbox(
                        &mut on,
                        format!(
                            "{} — {}: {} member end(s) given its connections, plates {} before, {} after",
                            a.joint, a.how, a.ends, a.plates.0, a.plates.1
                        ),
                    )
                    .changed();
                if changed {
                    if on {
                        unticked.remove(&a.joint);
                    } else {
                        unticked.insert(a.joint.clone());
                    }
                }
            }
        });
    if !all.left.is_empty() {
        egui::CollapsingHeader::new(format!("Left as they are ({})", all.left.len()))
            .id_salt("apply-detail-left")
            .default_open(true)
            .show(ui, |ui| {
                for (j, why) in &all.left {
                    ui.label(
                        egui::RichText::new(format!("{j}: {why}"))
                            .color(ui.visuals().warn_fg_color),
                    );
                }
            });
    }
}
