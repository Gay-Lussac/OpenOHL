// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! A computation on its own thread for a window of the Checks menu: its
//! answer, how long it took, a spinner while it runs.

use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use eframe::egui;

/// A background computation and its last answer.
pub struct Job<T> {
    running: Option<(Instant, Receiver<Result<T, String>>)>,
    /// The last answer (or why there is none).
    pub result: Option<Result<T, String>>,
    /// How long it took.
    pub took: Option<Duration>,
    /// What the answer was computed from ([`crate::revision`]).
    pub stamp: crate::revision::Stamp,
    /// Counts the answers taken (a new one opens the Results tab,
    /// [`crate::check_ui::tabs`]).
    pub generation: u64,
    /// The run's cancel token: Stop cancels it (a computation that looks
    /// at it ends early; one that does not finishes unseen).
    cancel: Option<ohl_core::budget::CancelToken>,
}

impl<T> Default for Job<T> {
    fn default() -> Self {
        Self {
            running: None,
            result: None,
            took: None,
            stamp: Default::default(),
            generation: 0,
            cancel: None,
        }
    }
}

impl<T: Send + 'static> Job<T> {
    /// Whether it runs.
    pub fn running(&self) -> bool {
        self.running.is_some()
    }

    /// Whether it was started: running, or with an answer (tests).
    #[cfg(test)]
    pub fn started(&self) -> bool {
        self.running() || self.result.is_some()
    }

    /// Runs `work` on its own thread (unless it runs already), on the
    /// model at revision `now`: its answer is out of date once what it
    /// uses changes ([`crate::revision::Stamp`]).
    pub fn start_at(
        &mut self,
        ctx: &egui::Context,
        now: crate::revision::Revision,
        work: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) {
        if self.running.is_some() {
            return;
        }
        self.stamp.started(now);
        self.start(ctx, work);
    }

    /// Since when it runs.
    pub fn elapsed(&self) -> Option<Duration> {
        self.running.as_ref().map(|(t, _)| t.elapsed())
    }

    /// A cancel token for the next run ([`Job::start`]): Stop cancels it;
    /// a computation that checks it (a budget's token) ends early.
    pub fn cancel_token(&mut self) -> ohl_core::budget::CancelToken {
        let token = ohl_core::budget::CancelToken::new();
        self.cancel = Some(token.clone());
        token
    }

    /// Stops the run (owner, 2026-10-27: Stop in every check): its token
    /// cancelled, its answer not waited for; the previous answer stays.
    pub fn stop(&mut self) {
        if let Some(t) = self.cancel.take() {
            t.cancel();
        }
        self.running = None;
    }

    /// Runs `work` on its own thread (unless it runs already).
    pub fn start(
        &mut self,
        ctx: &egui::Context,
        work: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) {
        if self.running.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(work());
            ctx.request_repaint();
        });
        self.running = Some((Instant::now(), rx));
    }

    /// Takes a finished answer.
    pub fn poll(&mut self) {
        let Some((t, rx)) = &self.running else {
            return;
        };
        let r = match rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("the computation stopped".into()),
        };
        self.took = Some(t.elapsed());
        self.stamp.arrived();
        if let Err(e) = &r {
            log::warn!("{e}");
        }
        self.result = Some(r);
        self.running = None;
        self.cancel = None;
        self.generation += 1;
    }
}

/// Text for a value in a table, `-` without one.
pub fn or_dash(v: Option<f64>, digits: usize) -> String {
    v.map_or("-".into(), |v| format!("{v:.digits$}"))
}

/// A weather case chooser: `case` indexes the model's cases.
pub fn case_combo(ui: &mut egui::Ui, id: &str, names: &[String], case: &mut usize) {
    if names.is_empty() {
        return;
    }
    *case = (*case).min(names.len() - 1);
    egui::ComboBox::from_id_salt(id)
        .selected_text(names[*case].as_str())
        .width(180.0)
        .show_ui(ui, |ui| {
            for (k, n) in names.iter().enumerate() {
                ui.selectable_value(case, k, n);
            }
        });
}

/// Saves `text` to a CSV file chosen by the user.
pub fn save_csv(name: &str, text: &str) {
    if let Some(path) = rfd::FileDialog::new()
        .add_filter("CSV", &["csv"])
        .set_file_name(name)
        .save_file()
        && let Err(e) = std::fs::write(&path, text)
    {
        log::warn!("{}: {e}", path.display());
    }
}
