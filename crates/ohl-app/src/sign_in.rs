// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! The **Sign in to a network** window (2026-11-20; ADR-0013 phase 13's
//! sign-in options on the desktop): File › Join a network starts here.
//!
//! - **The invitation** a network admin gave (`ohl://HOST:PORT/CODE/…`):
//!   the window says at once until when it is valid, or that it expired
//!   (a personal invitation says its last day itself); a reissued one is
//!   refused by the server, and the window says what that means.
//! - **Sign in with your organisation** when the server offers it (asked
//!   as soon as the invitation gives its address): the authorization code
//!   flow with PKCE in the user's own browser, the answer taken on this
//!   computer's loopback ([`ohl_server::sign_in`]). OHL-Design never sees
//!   a password; the ID token is kept in memory only, for joining and
//!   joining again, and gone when OHL-Design closes.
//!
//! The window follows the tool windows' rules
//! (docs/development/tool-pages.md): a sentence and its (?), User guide ↗,
//! the fields with their (?), one button row (Join first, Cancel last),
//! Escape cancels (a sign-in in the browser too), errors in boxes that
//! scroll.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use eframe::egui;
use ohl_server::Invitation;
use ohl_server::sign_in::{
    Flow, OidcOffer, SIGN_IN_TIMEOUT, SignInOptions, SignedIn, day_text, invitation_end, today,
};

use crate::tool_ui;

/// The manual's section of the window.
pub const GUIDE: (&str, &str) = ("user-guide/sign-in", "sign-in-to-a-network");

/// What the window asks for.
pub enum SignInAction {
    /// Join with the invitation (and the sign-in, when signed in).
    Join {
        /// The invitation.
        invitation: Invitation,
        /// The user name.
        user: String,
        /// Signed in with the organisation.
        signed: Option<Box<SignedIn>>,
    },
    /// Closed: nothing changes.
    Cancel,
}

/// A sign-in in the browser under way.
struct Browser {
    cancel: Arc<AtomicBool>,
    done: Receiver<Result<SignedIn, String>>,
}

/// The window's state.
#[derive(Default)]
pub struct SignIn {
    /// The invitation, as given.
    pub invitation: String,
    /// User name.
    pub user: String,
    /// Why the last attempt failed (explained).
    pub error: Option<String>,
    /// The server asked (its address) and its answer.
    options: Option<(String, Result<SignInOptions, String>)>,
    asking: Option<(String, Receiver<Result<SignInOptions, String>>)>,
    browser: Option<Browser>,
    /// Signed in with the organisation (in memory only).
    signed: Option<SignedIn>,
}

/// What an invitation says of itself, in words (and whether it is good).
pub fn invitation_note(text: &str, today: u32) -> Option<(String, bool)> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let inv = match text.parse::<Invitation>() {
        Ok(i) => i,
        Err(e) => return Some((e, false)),
    };
    Some(match invitation_end(&inv.code) {
        Some(day) if day < today => (
            format!(
                "This invitation expired on {}: ask a network admin for a new one (or sign \
                 in with your organisation, when the network offers it).",
                day_text(day)
            ),
            false,
        ),
        Some(day) => (
            format!("Personal invitation, valid until {}.", day_text(day)),
            true,
        ),
        None => (
            "Invitation without an end (valid until a network admin reissues it).".into(),
            true,
        ),
    })
}

/// A refusal of the server explained for the user (the server's words
/// kept after it).
pub fn explain(error: &str) -> String {
    let why = if error.contains("reissued since") {
        Some(
            "This invitation was reissued: a network admin gave you a newer one, and the older \
             ones no longer work. Use your newest invitation, or ask a network admin for it.",
        )
    } else if error.contains("expired on") {
        Some(
            "This invitation has expired (personal invitations end after a number of days, 90 \
             by default). Ask a network admin for a new one.",
        )
    } else if error.contains("not mapped to a user") {
        Some(
            "You signed in, but your organisation's account is not yet a user of this network. \
             Ask a network admin to add it (ohl network rights map-identity).",
        )
    } else if error.contains("another user's") {
        Some("This sign-in belongs to another user: type the user name your account is for.")
    } else if error.contains("wrong access code") || error.contains("access code") {
        Some("The invitation's code is not valid for this user name: check both.")
    } else if error.contains("sign in again") || error.contains("expired: ") {
        Some("Your sign-in has expired: sign in with your organisation again.")
    } else {
        None
    };
    match why {
        Some(w) => format!("{w}\n\nThe server said: {error}"),
        None => error.to_owned(),
    }
}

impl SignIn {
    /// For `user`.
    pub fn new(user: String) -> Self {
        Self {
            user,
            ..Self::default()
        }
    }

    /// Whether the server's sign-in options are known for the invitation
    /// as typed (tests and pictures).
    #[cfg(test)]
    pub fn options_known(&self) -> bool {
        let address = self
            .invitation
            .parse::<Invitation>()
            .ok()
            .map(|i| i.address);
        self.options
            .as_ref()
            .is_some_and(|(a, _)| Some(a) == address.as_ref())
    }

    /// Whether signed in with the organisation.
    #[cfg(test)]
    pub fn signed(&self) -> Option<&SignedIn> {
        self.signed.as_ref()
    }

    /// Stops a sign-in in the browser under way.
    pub fn cancel(&mut self) {
        if let Some(b) = self.browser.take() {
            b.cancel.store(true, Ordering::SeqCst);
        }
    }

    /// The answers of the background work (the server's options, the
    /// browser's sign-in).
    fn poll(&mut self, ctx: &egui::Context) {
        let address = self
            .invitation
            .parse::<Invitation>()
            .ok()
            .map(|i| (i.address.clone(), i));
        if let Some((asked, rx)) = &self.asking
            && let Ok(r) = rx.try_recv()
        {
            self.options = Some((asked.clone(), r));
            self.asking = None;
        }
        // The server asked once per address given.
        if let Some((a, inv)) = address {
            let known = self.options.as_ref().is_some_and(|(x, _)| *x == a)
                || self.asking.as_ref().is_some_and(|(x, _)| *x == a);
            if !known {
                let (tx, rx) = channel();
                let c = ctx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(ohl_server::sign_in::options(&inv));
                    c.request_repaint();
                });
                self.asking = Some((a, rx));
                // Another server: another sign-in.
                self.signed = None;
            }
        }
        if let Some(b) = &self.browser
            && let Ok(r) = b.done.try_recv()
        {
            self.browser = None;
            match r {
                Ok(s) => {
                    self.signed = Some(s);
                    self.error = None;
                }
                Err(e) => self.error = Some(explain(&e)),
            }
        }
        if self.asking.is_some() || self.browser.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    /// Starts signing in with `offer` in the browser.
    fn start_browser(&mut self, offer: OidcOffer, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let c = ctx.clone();
        std::thread::spawn(move || {
            let r = Flow::start(&offer).and_then(|flow| {
                crate::help::open_url(flow.url())?;
                flow.finish(SIGN_IN_TIMEOUT, &|| stop.load(Ordering::SeqCst))
            });
            let _ = tx.send(r);
            c.request_repaint();
        });
        self.error = None;
        self.browser = Some(Browser { cancel, done: rx });
    }

    /// The window's body; `busy`: joining (its spinner shown by the
    /// caller). What was asked, if anything.
    pub fn ui(&mut self, ui: &mut egui::Ui, busy: bool) -> Option<SignInAction> {
        self.poll(ui.ctx());
        let mut out = None;
        tool_ui::intro(
            ui,
            "Paste the invitation your network's administrator gave you, type your user name and \
             join — or sign in with your organisation's account when the network offers it.",
            "OHL-Design never sees your password: an organisation's sign-in happens in your own \
             browser, and what it gives back is kept in memory only, until OHL-Design closes. \
             A personal invitation is yours alone: a newer one replaces the older ones, and it \
             may end on a day it says (90 days by default).",
            GUIDE,
        );
        tool_ui::label(
            ui,
            "Invitation",
            "ohl://HOST:PORT/CODE/FINGERPRINT, from the network's administrator: the server's \
             address, your access code and the server's certificate. Signing in with your \
             organisation needs the address and the certificate only.",
        );
        ui.add(
            egui::TextEdit::singleline(&mut self.invitation)
                .desired_width(480.0)
                .hint_text("ohl://host:port/code/fingerprint"),
        );
        if let Some((text, good)) = invitation_note(&self.invitation, today()) {
            if good {
                ui.colored_label(
                    ohl_ui::messages::ok_color(ui),
                    egui::RichText::new(text).small(),
                );
            } else {
                crate::menu::error_label(ui, egui::RichText::new(text).small());
            }
        }
        ui.horizontal(|ui| {
            tool_ui::label(
                ui,
                "User name",
                "Your user name on the network (the one your invitation, or your organisation's \
                 account, is for).",
            );
            ui.text_edit_singleline(&mut self.user);
        });
        ui.add_space(4.0);
        tool_ui::heading(
            ui,
            "Sign in with your organisation",
            "When the network's server checks your organisation's sign-in (OIDC), you can sign \
             in in your browser instead of using the invitation's code: the browser opens your \
             organisation's sign-in page, and comes back to OHL-Design on this computer.",
        );
        let address = self
            .invitation
            .parse::<Invitation>()
            .ok()
            .map(|i| i.address);
        let answer = self
            .options
            .as_ref()
            .filter(|(a, _)| Some(a) == address.as_ref())
            .map(|(_, r)| r.clone());
        match (&address, answer) {
            (None, _) => {
                ui.label(
                    egui::RichText::new("Give the invitation first: its server is asked.")
                        .small()
                        .weak(),
                );
            }
            (Some(_), None) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("asking the server how it takes sign-ins…");
                });
            }
            (Some(_), Some(Err(e))) => {
                crate::menu::error_label(
                    ui,
                    egui::RichText::new(format!("The server could not be asked: {e}")).small(),
                );
            }
            (Some(_), Some(Ok(o))) => match o.oidc {
                None => {
                    ui.label(
                        egui::RichText::new(
                            "This network takes invitations only (no organisation sign-in).",
                        )
                        .small()
                        .weak(),
                    );
                }
                Some(offer) => {
                    if let Some(s) = &self.signed {
                        ui.colored_label(
                            ohl_ui::messages::ok_color(ui),
                            format!("Signed in as {} at {}.", s.subject(), offer.host()),
                        );
                    } else if self.browser.is_some() {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(format!(
                                "Finish signing in in your browser ({}); Cancel stops.",
                                offer.host()
                            ));
                        });
                    } else {
                        ui.label(format!(
                            "This network also takes your organisation's sign-in ({}).",
                            offer.host()
                        ));
                        if ui
                            .add_enabled(
                                !busy,
                                egui::Button::new("Sign in with your organisation ↗"),
                            )
                            .on_hover_text(
                                "Opens your organisation's sign-in page in your browser; \
                                 OHL-Design never sees your password",
                            )
                            .clicked()
                        {
                            self.start_browser(offer, ui.ctx());
                        }
                    }
                }
            },
        }
        if let Some(e) = &self.error {
            crate::menu::error_label(ui, e);
        }
        let parsed = self.invitation.parse::<Invitation>();
        let why_not = if parsed.is_err() {
            Some("Give the invitation (ohl://…)")
        } else if self.user.trim().is_empty() {
            Some("Give your user name")
        } else if busy {
            Some("Joining…")
        } else if self.browser.is_some() {
            Some("Finish signing in in your browser first")
        } else {
            None
        };
        let pressed = tool_ui::buttons(
            ui,
            if self.signed.is_some() {
                "Join with your sign-in"
            } else {
                "Join with the invitation"
            },
            why_not.is_none(),
            why_not,
            tool_ui::Keys::Both,
        );
        let enter = why_not.is_none() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if escape || pressed == Some(tool_ui::Pressed::Cancel) {
            self.cancel();
            out = Some(SignInAction::Cancel);
        } else if (enter || pressed == Some(tool_ui::Pressed::Action))
            && let Ok(invitation) = parsed
        {
            self.error = None;
            out = Some(SignInAction::Join {
                invitation,
                user: self.user.trim().to_owned(),
                signed: self.signed.clone().map(Box::new),
            });
        }
        out
    }
}

/// `signed` fresh enough to join with: renewed with its refresh token
/// when old (see [`SignedIn::stale`]), or said to be signed in again.
pub fn fresh(signed: &SignedIn) -> Result<SignedIn, String> {
    if !signed.stale() {
        return Ok(signed.clone());
    }
    if signed.refreshable() {
        ohl_server::sign_in::refresh(signed)
    } else {
        Err("your sign-in has expired: sign in again (File › Join a network)".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invitations_and_refusals_in_words() {
        let fp = "0".repeat(64);
        let good = format!("ohl://h:1/2.20271231.ab12/{fp}");
        let old = format!("ohl://h:1/2.20200101.ab12/{fp}");
        let plain = format!("ohl://h:1/abcd2345/{fp}");
        let today = 20261120;
        assert_eq!(
            invitation_note(&good, today),
            Some(("Personal invitation, valid until 2027-12-31.".into(), true))
        );
        let (t, ok) = invitation_note(&old, today).unwrap();
        assert!(!ok && t.contains("expired on 2020-01-01"), "{t}");
        assert!(invitation_note(&plain, today).unwrap().1);
        assert!(!invitation_note("nonsense", today).unwrap().1);
        assert_eq!(invitation_note("  ", today), None);
        let e = explain(
            "refused: this invitation was reissued since (the older ones are refused): ask a \
             network admin for your new one",
        );
        assert!(e.starts_with("This invitation was reissued"), "{e}");
        assert!(e.contains("The server said: refused"), "{e}");
        assert!(explain("refused: this invitation expired on 2026-01-02").contains("has expired"));
        assert!(explain("refused: this sign-in is not mapped to a user").contains("map-identity"));
        assert_eq!(explain("something else"), "something else");
    }
}
