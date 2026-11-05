// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The members' steel for the 3D view (owner 2026-11-05: "In the 3d view
//! of the OHL structure, I need the profiles to be shown with there
//! actual right orientation and placement as defined in the joints and
//! members detail view"): the link crate's solids
//! ([`ohl_structure_link::member_solids`], the Detail view's own builder),
//! worked out once per file revision and libraries.
//!
//! A small structure's joint details are worked out at once (and shared
//! with the Joint details panel); a large one's (a real lattice tower of
//! some 3000 members takes about 0.3 s) on a thread of its own, one at a
//! time: meanwhile the view draws the solids it has where their work
//! lines have not moved, the others as sticks.

use std::sync::Arc;
use std::sync::mpsc;

use eframe::egui;
use ohl_structure_link::MemberSolid;

use crate::app::StructureApp;

/// Up to this many members the details are worked out at once.
pub const AT_ONCE: usize = 400;

/// What the solids were made for: (file revision, libraries stamp).
type Key = (u64, u64);

/// The solids kept between frames.
#[derive(Default)]
pub struct Solids {
    /// The solids and what they were made for.
    have: Option<(Key, Arc<Vec<MemberSolid>>)>,
    /// A thread working them out, for its key.
    pending: Option<(Key, mpsc::Receiver<Vec<MemberSolid>>)>,
    /// How often new solids were taken (the view draws again then).
    pub generation: u64,
}

impl StructureApp {
    /// The members' solids for the 3D view (none when Display options ›
    /// Profiles as placed is off, or no file is open): the current ones,
    /// or while a large structure's are worked out, the last ones.
    pub(crate) fn member_solids(&mut self, ctx: &egui::Context) -> Arc<Vec<MemberSolid>> {
        let Some(f) = self.file.as_ref() else {
            return Arc::default();
        };
        if !self.profiles {
            return Arc::default();
        }
        let key = (f.revision(), self.libs_stamp);
        let members = f.structure().members.len();
        // A thread done: its solids taken (even when made for an older
        // revision: their work lines say where they still fit).
        if let Some((k, rx)) = &self.solids.pending {
            match rx.try_recv() {
                Ok(s) => {
                    let k = *k;
                    self.solids.pending = None;
                    self.take(k, Arc::new(s));
                }
                Err(mpsc::TryRecvError::Disconnected) => self.solids.pending = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some((k, s)) = &self.solids.have
            && *k == key
        {
            return s.clone();
        }
        // The Joint details panel's, when it has them; a small structure's
        // at once.
        let ready = matches!(self.joints.details_key(), Some(k) if k == key);
        if ready || members <= AT_ONCE {
            let s = match self.joint_details().as_deref() {
                Some(Ok(d)) => ohl_structure_link::member_solids(d),
                _ => Vec::new(),
            };
            let s = Arc::new(s);
            self.take(key, s.clone());
            return s;
        }
        // A large one: on a thread, one at a time.
        if self.solids.pending.is_none()
            && let Some(text) = self.file.as_ref().map(|f| f.text_ref().to_owned())
        {
            let sections = self.loads.sections.clone();
            let bolts = self.bolt_library();
            let (tx, rx) = mpsc::channel();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let s = ohl_structure_link::joint_details(&text, &sections, &bolts.0)
                    .map(|d| ohl_structure_link::member_solids(&d))
                    .unwrap_or_default();
                tx.send(s).ok();
                ctx.request_repaint();
            });
            self.solids.pending = Some((key, rx));
        }
        self.solids
            .have
            .as_ref()
            .map(|h| h.1.clone())
            .unwrap_or_default()
    }

    fn take(&mut self, key: Key, s: Arc<Vec<MemberSolid>>) {
        self.solids.have = Some((key, s));
        self.solids.generation += 1;
    }

    /// Waits until the solids of the file as it is are worked out (tests).
    #[cfg(test)]
    pub(crate) fn wait_for_solids(&mut self, ctx: &egui::Context) -> Arc<Vec<MemberSolid>> {
        loop {
            let s = self.member_solids(ctx);
            let key = self.file.as_ref().map(|f| (f.revision(), self.libs_stamp));
            if self.solids.have.as_ref().map(|h| h.0) == key || self.solids.pending.is_none() {
                return s;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}
