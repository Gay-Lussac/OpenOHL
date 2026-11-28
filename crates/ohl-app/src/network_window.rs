// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! OHL-Design on a country server's network (ADR-0013, phase 6): File ›
//! Join a network picks a **working set** — whole routes (ticked in a
//! list or clicked on the map), a part of a route by station range or from
//! one structure to another, or an area drawn on the map — shows it as the
//! server snaps it out to whole tension sections, saves it by name, and
//! opens it: the **partial replica** comes as a project file (the
//! desktop's format, unchanged) and is fed by the server's rows; the rest
//! of the network around the camera is the read-only context
//! ([`crate::context_tiles`]). File › Working set adds a route to it (the
//! route comes, caught up) or saves it.
//!
//! Edits go to the server (the whole model validates them); the replica
//! takes the rows that come back, in place, like any edit's.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use eframe::egui;
use ohl_server::client::{Incoming, Joined, NetJoin, Progress, join_network};
use ohl_server::{ClientMsg, Invitation, NetworkInfo, SavedSet, ServerMsg};
use ohl_session::working_set::{ExtentSummary, Pick, RouteDetail, WorkingSetSpec};

use crate::remote::{JoinEvent, Remote};

/// What a desktop joined to a network keeps beside its replica.
#[derive(Default)]
pub struct NetState {
    /// The model (line) of the working set.
    pub model: String,
    /// The working set followed.
    pub spec: WorkingSetSpec,
    /// What the network holds (after the welcome).
    pub info: Option<NetworkInfo>,
    /// The context around the camera.
    pub context: crate::context_tiles::ContextTiles,
    /// The context's revision drawn last.
    pub drawn: u64,
    /// The user's saved working sets (last listed).
    pub saved: Vec<SavedSet>,
    /// The Working set window is open.
    pub window: bool,
    /// The name typed to save it.
    pub save_name: String,
    /// The last answer of the server to the window (a refusal).
    pub note: Option<(String, bool)>,
    /// Joining again after the connection was lost (with what the
    /// replica holds: only what differs comes).
    #[allow(clippy::type_complexity)]
    pub rejoin: Option<Receiver<Result<Joined, String>>>,
    /// Signed in with the organisation (in memory only): joining again
    /// carries its ID token, renewed when old.
    pub signed: Option<ohl_server::sign_in::SignedIn>,
    /// The server's answers to the Offline working set window (exports
    /// and bundles brought back, 2026-11-21), taken by it by their id.
    pub offline_answers: Vec<ohl_server::ServerMsg>,
}

impl std::fmt::Debug for NetState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetState")
            .field("model", &self.model)
            .field("spec", &self.spec)
            .finish_non_exhaustive()
    }
}

impl NetState {
    /// For working set `spec`.
    pub fn new(spec: WorkingSetSpec) -> Self {
        Self {
            model: spec.model.clone(),
            save_name: spec.name.clone(),
            spec,
            window: true,
            ..Self::default()
        }
    }
}

/// How a working set is picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Whole routes (a list, or clicked on the map).
    #[default]
    Routes,
    /// A part of a route by station range.
    Stations,
    /// A part of a route from one structure to another.
    Structures,
    /// An area drawn on the map.
    Area,
}

/// The map of the network in the window: routes by their structures (or
/// their bounds until those came), the area drawn, the extent shown.
#[derive(Debug, Default)]
struct Map {
    centre: Option<[f64; 2]>,
    scale: f64,
}

/// File › Join a network: first the **Sign in to a network** window
/// ([`crate::sign_in`]), then the working set picked.
#[derive(Default)]
pub struct JoinNetworkWindow {
    /// The sign-in (invitation, user name, organisation's sign-in).
    pub sign_in: crate::sign_in::SignIn,
    /// User name (as signed in).
    pub user: String,
    /// Why the last attempt failed.
    pub error: Option<String>,
    /// Signed in with the organisation (in memory only).
    signed: Option<ohl_server::sign_in::SignedIn>,
    connecting: Option<Receiver<Result<(Joined, Invitation), String>>>,
    browse: Option<(Joined, Invitation)>,
    info: Option<NetworkInfo>,
    model: Option<String>,
    /// How it is picked.
    pub mode: Mode,
    ticked: BTreeSet<String>,
    route: Option<String>,
    from: f64,
    to: f64,
    from_structure: String,
    to_structure: String,
    polygon: Vec<[f64; 2]>,
    details: BTreeMap<String, RouteDetail>,
    asked_details: BTreeSet<String>,
    /// The picks shown and the server's answer.
    shown: Option<(Vec<Pick>, Result<ExtentSummary, String>)>,
    asking: Option<(u64, Vec<Pick>)>,
    saved: Vec<SavedSet>,
    save_name: String,
    map: Map,
    opening: Option<Receiver<JoinEvent>>,
    progress: Option<Progress>,
}

/// Connects to look around (no working set yet), in the background
/// (with the organisation's sign-in when `token`).
fn connect(
    invitation: Invitation,
    user: String,
    token: Option<String>,
    ctx: egui::Context,
) -> Receiver<Result<(Joined, Invitation), String>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let how = NetJoin {
            id_token: token,
            ..NetJoin::default()
        };
        let r = join_network(&invitation, &user, &how, &mut |_| {})
            .map(|j| (j, invitation))
            .map_err(|e| crate::sign_in::explain(&e));
        let _ = tx.send(r);
        ctx.request_repaint();
    });
    rx
}

/// Where a working set's replica is kept (one file per server address,
/// user and line, replaced at every join).
pub fn replica_file(dir: &Path, invitation: &Invitation, user: &str, model: &str) -> PathBuf {
    let clean = |s: &str| -> String {
        s.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect()
    };
    dir.join(format!(
        "{}-{}-{}-working-set.ohl",
        clean(&invitation.address),
        clean(user),
        clean(model)
    ))
}

/// Opens working set `spec` in the background: its replica written to a
/// file of `dir`, the connection kept.
pub fn start_open(
    invitation: Invitation,
    user: String,
    spec: WorkingSetSpec,
    dir: PathBuf,
    signed: Option<ohl_server::sign_in::SignedIn>,
    ctx: egui::Context,
) -> Receiver<JoinEvent> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = std::fs::create_dir_all(&dir);
        let file = replica_file(&dir, &invitation, &user, &spec.model);
        let _ = std::fs::remove_file(&file);
        let p = tx.clone();
        let c = ctx.clone();
        // The organisation's sign-in renewed when old.
        let signed = match signed.as_ref().map(crate::sign_in::fresh).transpose() {
            Ok(s) => s,
            Err(e) => {
                let _ = tx.send(JoinEvent::Done(Err(e)));
                ctx.request_repaint();
                return;
            }
        };
        let how = NetJoin {
            working_set: Some(spec.clone()),
            file: Some(file.clone()),
            id_token: signed.as_ref().map(|s| s.id_token().to_owned()),
            ..NetJoin::default()
        };
        let result = join_network(&invitation, &user, &how, &mut |g| {
            let _ = p.send(JoinEvent::Progress(g));
            c.request_repaint();
        });
        let done = result
            .map(|j| {
                let mut remote = Remote::new(j, invitation, false);
                let mut state = NetState::new(spec);
                state.signed = signed;
                remote.network = Some(Box::new(state));
                (file, Box::new(remote))
            })
            .map_err(|e| crate::sign_in::explain(&e));
        let _ = tx.send(JoinEvent::Done(done));
        ctx.request_repaint();
    });
    rx
}

impl JoinNetworkWindow {
    /// The window for `user`, not connected.
    pub fn new(user: String) -> Self {
        Self {
            sign_in: crate::sign_in::SignIn::new(user.clone()),
            user,
            ..Self::default()
        }
    }

    /// The window as it looks after a station range was shown, without a
    /// server (the user guide's picture; a synthetic network).
    #[cfg(test)]
    pub fn sample() -> Self {
        use ohl_server::{ModelInfo, Position, RouteInfo};
        use ohl_session::working_set::{AlignmentLength, RouteExtent, StructureStation};
        let path = |k: usize| {
            [
                1000.0 + 310.0 * k as f64,
                2000.0 + 40.0 * (k as f64).sin() * 3.0,
            ]
        };
        let structures: Vec<StructureStation> = (0..24)
            .map(|k| StructureStation {
                id: format!("L001-T{:03}", k + 1),
                station: Some(310.0 * k as f64),
                position: path(k),
            })
            .collect();
        let mut details = BTreeMap::new();
        details.insert(
            "L001".to_owned(),
            RouteDetail {
                route: "L001".into(),
                name: "Line 1".into(),
                alignments: vec![AlignmentLength {
                    id: "L001".into(),
                    name: String::new(),
                    length: 310.0 * 23.0,
                }],
                structures,
            },
        );
        let models = vec![
            ModelInfo {
                code: "L001".into(),
                name: "Line 1".into(),
                structures: 24,
            },
            ModelInfo {
                code: "L002".into(),
                name: "Line 2".into(),
                structures: 31,
            },
        ];
        let routes = vec![
            RouteInfo {
                id: "L001".into(),
                model: "L001".into(),
                name: "Line 1".into(),
                bounds: Some([1000.0, 1880.0, 8130.0, 2120.0]),
            },
            RouteInfo {
                id: "L002".into(),
                model: "L002".into(),
                name: "Line 2".into(),
                bounds: Some([3000.0, 2300.0, 9000.0, 6000.0]),
            },
        ];
        let summary = ExtentSummary {
            routes: vec![RouteExtent {
                route: "L001".into(),
                whole: false,
                alignment: Some("L001".into()),
                from: Some(1860.0),
                to: Some(4650.0),
                first: Some("L001-T007".into()),
                last: Some("L001-T016".into()),
                structures: 10,
                outline: (6..16).map(path).collect(),
            }],
            structures: 10,
            sections: 30,
            jumpers: 12,
            spacers: 0,
            context: 41,
            bounds: None,
            notes: vec![
                "route L001: snapped out to whole tension sections, stations 1860–4650 m".into(),
            ],
        };
        let picks = vec![Pick::Stations {
            route: "L001".into(),
            alignment: None,
            from: 2500.0,
            to: 4000.0,
        }];
        Self {
            user: "tester".into(),
            info: Some(NetworkInfo {
                name: "synthetic".into(),
                crs: String::new(),
                grid_size: 256.0,
                tile_level: 2,
                context_case: "15 °C, no wind".into(),
                at: Position::default(),
                heartbeat_ms: 2000,
                bounds: Some([1000.0, 1880.0, 9000.0, 6000.0]),
                models,
                routes,
                admin: 0,
            }),
            model: Some("L001".into()),
            mode: Mode::Stations,
            route: Some("L001".into()),
            from: 2500.0,
            to: 4000.0,
            details,
            asked_details: ["L001".to_owned(), "L002".to_owned()].into(),
            shown: Some((picks, Ok(summary))),
            saved: vec![SavedSet {
                spec: WorkingSetSpec {
                    name: "north part".into(),
                    model: "L002".into(),
                    picks: vec![Pick::Route {
                        route: "L002".into(),
                    }],
                },
                routes: vec!["L002".into()],
                time: "2026-11-19T08:00:00.000Z".into(),
            }],
            save_name: "middle third".into(),
            map: Map {
                centre: Some([5000.0, 3500.0]),
                scale: 340.0 / 8000.0,
            },
            ..Self::default()
        }
    }

    /// Whether joined to look around (past the sign-in; tests).
    #[cfg(test)]
    pub fn joined_network(&self) -> bool {
        self.browse.is_some()
    }

    /// The picks of the controls (or why there are none).
    fn picks(&self) -> Result<Vec<Pick>, String> {
        match self.mode {
            Mode::Routes => {
                if self.ticked.is_empty() {
                    return Err("tick a route (in the list or on the map)".into());
                }
                Ok(self
                    .ticked
                    .iter()
                    .map(|r| Pick::Route { route: r.clone() })
                    .collect())
            }
            Mode::Stations => Ok(vec![Pick::Stations {
                route: self.route.clone().ok_or("choose a route")?,
                alignment: None,
                from: self.from,
                to: self.to,
            }]),
            Mode::Structures => {
                if self.from_structure.is_empty() || self.to_structure.is_empty() {
                    return Err("choose both structures".into());
                }
                Ok(vec![Pick::Structures {
                    route: self.route.clone().ok_or("choose a route")?,
                    from: self.from_structure.clone(),
                    to: self.to_structure.clone(),
                }])
            }
            Mode::Area => {
                if self.polygon.len() < 3 {
                    return Err("click three corners or more on the map".into());
                }
                Ok(vec![Pick::Area {
                    polygon: self.polygon.clone(),
                }])
            }
        }
    }

    /// The messages of the browsing connection.
    fn poll(&mut self) {
        let Some((j, _)) = &self.browse else {
            return;
        };
        let mut closed = None;
        let mut msgs = Vec::new();
        while let Some(m) = j.link.try_recv() {
            match m {
                Incoming::Msg(m) => msgs.push(m),
                Incoming::Closed(why) => {
                    closed = Some(why);
                    break;
                }
            }
        }
        for m in msgs {
            match m {
                ServerMsg::Network(info) => {
                    if self.model.is_none() {
                        self.model = info.models.first().map(|m| m.code.clone());
                    }
                    if let Some(b) = info.bounds {
                        self.map.centre = Some([(b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0]);
                        self.map.scale = 340.0 / (b[2] - b[0]).max(b[3] - b[1]).max(1.0);
                    }
                    self.info = Some(*info);
                }
                ServerMsg::Extent { id, result, .. } => {
                    if let Some((asked, picks)) = self.asking.take() {
                        if asked == id {
                            self.shown = Some((picks, result));
                        } else {
                            self.asking = Some((asked, picks));
                        }
                    }
                }
                ServerMsg::RouteDetail { result: Ok(d), .. } => {
                    self.details.insert(d.route.clone(), d);
                }
                ServerMsg::RouteDetail { result: Err(e), .. } => self.error = Some(e),
                ServerMsg::WorkingSets { sets } => self.saved = sets,
                ServerMsg::Notice { text } => self.error = Some(text),
                _ => {}
            }
        }
        if let Some(why) = closed {
            self.error = Some(format!("disconnected: {why}"));
            self.browse = None;
        }
    }

    fn send(&mut self, msg: ClientMsg) {
        if let Some((j, _)) = &self.browse
            && let Err(e) = j.link.send(msg)
        {
            self.error = Some(e);
        }
    }

    /// The window; `Some` once a working set is open (its replica file,
    /// the connection).
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        open: &mut bool,
        dir: &Path,
    ) -> Option<(PathBuf, Box<Remote>)> {
        let mut joined = None;
        if let Some(rx) = &self.connecting
            && let Ok(r) = rx.try_recv()
        {
            self.connecting = None;
            match r {
                Ok(b) => {
                    self.browse = Some(b);
                    self.send(ClientMsg::WorkingSets);
                }
                Err(e) => self.error = Some(e),
            }
        }
        if let Some(rx) = &self.opening {
            while let Ok(e) = rx.try_recv() {
                match e {
                    JoinEvent::Progress(p) => self.progress = Some(p),
                    JoinEvent::Done(Ok(j)) => joined = Some(j),
                    JoinEvent::Done(Err(e)) => self.error = Some(e),
                }
            }
            if joined.is_some() || self.error.is_some() {
                self.opening = None;
            }
        }
        self.poll();
        // The routes of the line chosen: their structures asked once.
        if let (Some(info), Some(model)) = (&self.info, self.model.clone()) {
            let want: Vec<String> = info
                .routes
                .iter()
                .filter(|r| r.model == model && !self.asked_details.contains(&r.id))
                .map(|r| r.id.clone())
                .collect();
            for r in want {
                self.asked_details.insert(r.clone());
                self.send(ClientMsg::RouteDetail {
                    model: model.clone(),
                    route: r,
                });
            }
        }
        let mut open_spec: Option<WorkingSetSpec> = None;
        if self.browse.is_none() && self.info.is_none() {
            // First the sign-in, a window of its own.
            let mut asked = None;
            egui::Window::new("Sign in to a network")
                .open(open)
                .collapsible(false)
                .resizable(false)
                .default_width(520.0)
                .show(ctx, |ui| {
                    asked = self.sign_in.ui(ui, self.connecting.is_some());
                    if let Some(e) = &self.error {
                        crate::menu::error_label(ui, e);
                    }
                    if self.connecting.is_some() {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("connecting…");
                        });
                    }
                });
            match asked {
                Some(crate::sign_in::SignInAction::Cancel) => *open = false,
                Some(crate::sign_in::SignInAction::Join {
                    invitation,
                    user,
                    signed,
                }) => {
                    self.error = None;
                    self.user = user.clone();
                    self.signed = signed.as_deref().cloned();
                    let token = signed.map(|s| s.id_token().to_owned());
                    self.connecting = Some(connect(invitation, user, token, ctx.clone()));
                }
                None => {}
            }
            if !*open {
                self.sign_in.cancel();
            }
            if self.connecting.is_some() {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            return joined;
        }
        egui::Window::new("Join a network")
            .open(open)
            .collapsible(false)
            .resizable(true)
            .default_width(780.0)
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(390.0);
                        open_spec = self.pick_ui(ui);
                    });
                    ui.separator();
                    self.map_ui(ui);
                });
                if let Some(e) = &self.error {
                    crate::menu::error_label(ui, e);
                }
                if self.opening.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        match self.progress {
                            Some(p) => ui.label(format!(
                                "receiving the working set: {:.1} of {:.1} MB",
                                p.received as f64 / 1e6,
                                p.total as f64 / 1e6
                            )),
                            None => ui.label("the server writes the working set…"),
                        };
                    });
                }
            });
        if let (Some(spec), Some((_, inv))) = (open_spec, &self.browse) {
            self.error = None;
            self.progress = None;
            self.opening = Some(start_open(
                inv.clone(),
                self.user.trim().to_owned(),
                spec,
                dir.to_owned(),
                self.signed.clone(),
                ctx.clone(),
            ));
        }
        if self.connecting.is_some() || self.opening.is_some() || self.asking.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        joined
    }

    /// The saved working sets, the picking controls, what was shown;
    /// `Some` when one is to be opened.
    fn pick_ui(&mut self, ui: &mut egui::Ui) -> Option<WorkingSetSpec> {
        let mut open = None;
        let busy = self.opening.is_some();
        if !self.saved.is_empty() {
            ui.label(egui::RichText::new("Your saved working sets").strong());
            let mut delete = None;
            for s in &self.saved {
                ui.horizontal(|ui| {
                    ui.label(&s.spec.name).on_hover_text(format!(
                        "line {}: {} (saved {})",
                        s.spec.model,
                        s.spec
                            .picks
                            .iter()
                            .map(Pick::describe)
                            .collect::<Vec<_>>()
                            .join(", "),
                        s.time
                    ));
                    if ui.add_enabled(!busy, egui::Button::new("Open")).clicked() {
                        open = Some(s.spec.clone());
                    }
                    if ui.small_button("Delete").clicked() {
                        delete = Some(s.spec.name.clone());
                    }
                });
            }
            if let Some(name) = delete {
                self.send(ClientMsg::DeleteWorkingSet { name });
            }
            ui.separator();
        }
        let Some(info) = self.info.clone() else {
            ui.label("reading the network…");
            return open;
        };
        ui.label(egui::RichText::new("A new working set").strong());
        let before = self.picks().ok();
        egui::ComboBox::from_label("Line")
            .selected_text(self.model.clone().unwrap_or_default())
            .show_ui(ui, |ui| {
                for m in &info.models {
                    let label = format!("{} — {} ({} structures)", m.code, m.name, m.structures);
                    if ui
                        .selectable_label(self.model.as_deref() == Some(&*m.code), label)
                        .clicked()
                    {
                        self.model = Some(m.code.clone());
                        self.ticked.clear();
                        self.route = None;
                    }
                }
            });
        let model = self.model.clone().unwrap_or_default();
        let routes: Vec<&ohl_server::RouteInfo> =
            info.routes.iter().filter(|r| r.model == model).collect();
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.mode, Mode::Routes, "Whole routes");
            ui.selectable_value(&mut self.mode, Mode::Stations, "Station range");
            ui.selectable_value(&mut self.mode, Mode::Structures, "Structure to structure");
            ui.selectable_value(&mut self.mode, Mode::Area, "Area");
        });
        match self.mode {
            Mode::Routes => {
                for r in &routes {
                    let mut on = self.ticked.contains(&r.id);
                    let name = if r.name.is_empty() { &r.id } else { &r.name };
                    if ui.checkbox(&mut on, format!("{name} ({})", r.id)).changed() {
                        if on {
                            self.ticked.insert(r.id.clone());
                        } else {
                            self.ticked.remove(&r.id);
                        }
                    }
                }
                ui.label(
                    egui::RichText::new("or click a route on the map")
                        .small()
                        .weak(),
                );
            }
            Mode::Stations | Mode::Structures => {
                egui::ComboBox::from_label("Route")
                    .selected_text(self.route.clone().unwrap_or_default())
                    .show_ui(ui, |ui| {
                        for r in &routes {
                            ui.selectable_value(&mut self.route, Some(r.id.clone()), &r.id);
                        }
                    });
                let detail = self.route.as_ref().and_then(|r| self.details.get(r));
                match (self.mode, detail) {
                    (_, None) => {
                        ui.label("reading the route's structures…");
                    }
                    (Mode::Stations, Some(d)) => {
                        let length = d.alignments.first().map_or(0.0, |a| a.length);
                        ui.label(format!(
                            "{} structures along {:.0} m (alignment {})",
                            d.structures.len(),
                            length,
                            d.alignments.first().map_or("", |a| a.id.as_str())
                        ));
                        ui.horizontal(|ui| {
                            ui.label("From station");
                            ui.add(
                                egui::DragValue::new(&mut self.from)
                                    .range(0.0..=length)
                                    .suffix(" m"),
                            );
                            ui.label("to");
                            ui.add(
                                egui::DragValue::new(&mut self.to)
                                    .range(0.0..=length)
                                    .suffix(" m"),
                            );
                        });
                    }
                    (_, Some(d)) => {
                        let ids: Vec<String> = d.structures.iter().map(|s| s.id.clone()).collect();
                        for (label, value) in [
                            ("From structure", &mut self.from_structure),
                            ("To structure", &mut self.to_structure),
                        ] {
                            egui::ComboBox::from_label(label)
                                .selected_text(value.clone())
                                .show_ui(ui, |ui| {
                                    for id in &ids {
                                        ui.selectable_value(value, id.clone(), id);
                                    }
                                });
                        }
                    }
                }
            }
            Mode::Area => {
                ui.horizontal(|ui| {
                    ui.label(format!("{} corners clicked on the map", self.polygon.len()));
                    if ui.small_button("Clear").clicked() {
                        self.polygon.clear();
                    }
                });
            }
        }
        let picks = self.picks();
        if picks.as_ref().ok() != before.as_ref() {
            self.shown = None;
        }
        ui.horizontal(|ui| {
            let can = picks.is_ok() && self.asking.is_none();
            if ui
                .add_enabled(can, egui::Button::new("Show"))
                .on_hover_text(
                    "Ask the server what it holds, snapped out to whole tension sections",
                )
                .on_disabled_hover_text(picks.as_ref().err().cloned().unwrap_or_default())
                .clicked()
                && let (Ok(p), Some((j, _))) = (&picks, &mut self.browse)
            {
                let id = j.link.next_id();
                self.asking = Some((id, p.clone()));
                let _ = j.link.send(ClientMsg::Pick {
                    id,
                    model: model.clone(),
                    picks: p.clone(),
                });
            }
            if self.asking.is_some() {
                ui.spinner();
            }
        });
        let shown_ok = matches!(&self.shown, Some((p, Ok(_))) if Ok(p) == picks.as_ref());
        if let Some((_, result)) = &self.shown {
            match result {
                Ok(e) => extent_ui(ui, e),
                Err(e) => {
                    crate::menu::error_label(ui, e);
                }
            }
        }
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.add(egui::TextEdit::singleline(&mut self.save_name).desired_width(160.0));
            if ui
                .add_enabled(
                    shown_ok && !self.save_name.trim().is_empty(),
                    egui::Button::new("Save"),
                )
                .on_hover_text("Keep these picks on the server under this name")
                .on_disabled_hover_text("Show it first, and give it a name")
                .clicked()
                && let Ok(p) = &picks
            {
                let spec = WorkingSetSpec {
                    name: self.save_name.trim().to_owned(),
                    model: model.clone(),
                    picks: p.clone(),
                };
                self.send(ClientMsg::SaveWorkingSet { spec });
            }
        });
        if ui
            .add_enabled(
                shown_ok && !busy,
                egui::Button::new("Open this working set"),
            )
            .on_disabled_hover_text("Show it first")
            .clicked()
            && let Ok(p) = &picks
        {
            open = Some(WorkingSetSpec {
                name: self.save_name.trim().to_owned(),
                model,
                picks: p.clone(),
            });
        }
        open
    }

    /// The map: drag to pan, scroll to zoom; a click ticks a route (whole
    /// routes) or adds a corner (area).
    fn map_ui(&mut self, ui: &mut egui::Ui) {
        let (response, painter) =
            ui.allocate_painter(egui::vec2(360.0, 360.0), egui::Sense::click_and_drag());
        let rect = response.rect;
        painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(24, 28, 34));
        let Some(info) = &self.info else {
            return;
        };
        let centre = self.map.centre.unwrap_or([0.0, 0.0]);
        let scale = if self.map.scale > 0.0 {
            self.map.scale
        } else {
            1e-3
        };
        if response.dragged() {
            let d = response.drag_delta();
            self.map.centre = Some([
                centre[0] - f64::from(d.x) / scale,
                centre[1] + f64::from(d.y) / scale,
            ]);
        }
        if response.hovered() {
            let s = ui.input(|i| i.smooth_scroll_delta.y);
            if s != 0.0 {
                self.map.scale = scale * (1.0 + f64::from(s) * 0.002).clamp(0.5, 2.0);
            }
        }
        let c = rect.center();
        let to_screen = |p: [f64; 2]| {
            egui::pos2(
                c.x + ((p[0] - centre[0]) * scale) as f32,
                c.y - ((p[1] - centre[1]) * scale) as f32,
            )
        };
        let to_world = |q: egui::Pos2| {
            [
                centre[0] + f64::from(q.x - c.x) / scale,
                centre[1] - f64::from(q.y - c.y) / scale,
            ]
        };
        let painter = painter.with_clip_rect(rect);
        let model = self.model.clone().unwrap_or_default();
        let mut nearest: Option<(f32, String)> = None;
        let pointer = response.interact_pointer_pos();
        for r in &info.routes {
            let picked = r.model == model && self.ticked.contains(&r.id);
            let col = if picked {
                egui::Color32::from_rgb(90, 200, 220)
            } else if r.model == model {
                egui::Color32::from_gray(170)
            } else {
                egui::Color32::from_gray(90)
            };
            match self.details.get(&r.id).filter(|_| r.model == model) {
                Some(d) => {
                    let pts: Vec<egui::Pos2> =
                        d.structures.iter().map(|s| to_screen(s.position)).collect();
                    if let Some(p) = pointer {
                        for q in &pts {
                            let dist = q.distance(p);
                            if nearest.as_ref().is_none_or(|(n, _)| dist < *n) {
                                nearest = Some((dist, r.id.clone()));
                            }
                        }
                    }
                    painter.add(egui::Shape::line(pts, egui::Stroke::new(1.5, col)));
                }
                None => {
                    if let Some(b) = r.bounds {
                        let rr = egui::Rect::from_two_pos(
                            to_screen([b[0], b[1]]),
                            to_screen([b[2], b[3]]),
                        );
                        painter.rect_stroke(
                            rr,
                            0.0,
                            egui::Stroke::new(1.0, col),
                            egui::StrokeKind::Middle,
                        );
                    }
                }
            }
        }
        // The extent shown (snapped), yellow.
        if let Some((_, Ok(e))) = &self.shown {
            for r in &e.routes {
                let pts: Vec<egui::Pos2> = r.outline.iter().map(|p| to_screen(*p)).collect();
                painter.add(egui::Shape::line(
                    pts,
                    egui::Stroke::new(3.0, egui::Color32::from_rgb(240, 200, 60)),
                ));
            }
        }
        // The area drawn.
        if !self.polygon.is_empty() {
            let mut pts: Vec<egui::Pos2> = self.polygon.iter().map(|p| to_screen(*p)).collect();
            if pts.len() > 2 {
                pts.push(pts[0]);
            }
            painter.add(egui::Shape::line(
                pts.clone(),
                egui::Stroke::new(1.5, egui::Color32::from_rgb(120, 220, 140)),
            ));
            for p in pts {
                painter.circle_filled(p, 3.0, egui::Color32::from_rgb(120, 220, 140));
            }
        }
        if response.clicked()
            && let Some(p) = pointer
        {
            match self.mode {
                Mode::Area => self.polygon.push(to_world(p)),
                Mode::Routes => {
                    if let Some((d, id)) = nearest
                        && d < 12.0
                        && !self.ticked.remove(&id)
                    {
                        self.ticked.insert(id);
                    }
                }
                _ => {}
            }
        }
        painter.text(
            rect.left_top() + egui::vec2(6.0, 6.0),
            egui::Align2::LEFT_TOP,
            match self.mode {
                Mode::Area => "click the area's corners",
                Mode::Routes => "click a route to tick it",
                _ => "drag to move, scroll to zoom",
            },
            egui::FontId::proportional(11.0),
            egui::Color32::from_gray(160),
        );
    }
}

/// A working set in numbers, as the server snapped it.
pub fn extent_ui(ui: &mut egui::Ui, e: &ExtentSummary) {
    ui.label(format!(
        "{} structures, {} sections, {} jumpers, {} spacers; {} objects of context \
         (read-only)",
        e.structures, e.sections, e.jumpers, e.spacers, e.context
    ));
    for r in &e.routes {
        let text = match (r.whole, r.from, r.to) {
            (true, _, _) => format!("route {}: whole ({} structures)", r.route, r.structures),
            (false, Some(a), Some(b)) => format!(
                "route {}: stations {a:.0}–{b:.0} m, {} to {} ({} structures)",
                r.route,
                r.first.as_deref().unwrap_or("?"),
                r.last.as_deref().unwrap_or("?"),
                r.structures
            ),
            _ => format!("route {}: {} structures", r.route, r.structures),
        };
        ui.label(text);
    }
    for n in &e.notes {
        ui.label(egui::RichText::new(n).small().weak());
    }
}

/// File › Working set (on a network): what is held, a route added (it
/// comes, caught up), saved by name. Returns a message to show.
pub fn working_set_window(ctx: &egui::Context, remote: &mut Remote) -> Option<(String, bool)> {
    let Remote {
        link,
        network,
        closed,
        invitation,
        ..
    } = remote;
    let n = network.as_mut()?;
    if closed.is_some() && n.rejoin.is_none() && !n.window {
        n.window = true;
    }
    if !n.window {
        return n.note.take();
    }
    let mut open = true;
    let mut send: Option<ClientMsg> = None;
    egui::Window::new("Working set")
        .open(&mut open)
        .default_width(460.0)
        .show(ctx, |ui| {
            if let Some(why) = closed.as_ref() {
                crate::menu::error_label(ui, format!("disconnected: {why}"));
                if n.rejoin.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("joining again…");
                    });
                } else if ui
                    .button("Join again")
                    .on_hover_text(
                        "Join the network again with this working set: only what changed \
                         meanwhile comes",
                    )
                    .clicked()
                {
                    let (tx, rx) = channel();
                    let (inv, user, spec) = (invitation.clone(), link.user.clone(), n.spec.clone());
                    let signed = n.signed.clone();
                    let ctx = ui.ctx().clone();
                    std::thread::spawn(move || {
                        // The organisation's sign-in renewed when old.
                        let token = match signed.as_ref().map(crate::sign_in::fresh).transpose() {
                            Ok(s) => s.map(|s| s.id_token().to_owned()),
                            Err(e) => {
                                let _ = tx.send(Err(e));
                                ctx.request_repaint();
                                return;
                            }
                        };
                        let how = NetJoin {
                            working_set: Some(spec),
                            held: true,
                            id_token: token,
                            ..NetJoin::default()
                        };
                        let r = join_network(&inv, &user, &how, &mut |_| {})
                            .map_err(|e| crate::sign_in::explain(&e));
                        let _ = tx.send(r);
                        ctx.request_repaint();
                    });
                    n.rejoin = Some(rx);
                }
                ui.separator();
            }
            ui.label(format!("Line {}", n.model));
            for p in &n.spec.picks {
                ui.label(format!("• {}", p.describe()));
            }
            ui.label(
                egui::RichText::new(
                    "Edits go to the server and come back as rows; the grey context around \
                     it is a drawing (each section solved alone in the network's context case), \
                     never a result and never used by a check. Checks run on what the working \
                     set holds (each of its structures with all its wires); neighbours beyond \
                     it — other lines' wires within a clearance's reach, the field distance — \
                     are not pulled in yet (phase 10).",
                )
                .small()
                .weak(),
            );
            let (tiles, bytes) = n.context.size();
            ui.label(
                egui::RichText::new(format!(
                    "context: {tiles} tiles kept ({:.1} MB)",
                    bytes as f64 / 1e6
                ))
                .small()
                .weak(),
            );
            ui.separator();
            ui.label(egui::RichText::new("Add a route").strong());
            let held: BTreeSet<&str> = n
                .spec
                .picks
                .iter()
                .filter_map(|p| match p {
                    Pick::Route { route } => Some(route.as_str()),
                    _ => None,
                })
                .collect();
            let mut candidates: BTreeMap<String, (String, bool)> = BTreeMap::new();
            if let Some(info) = &n.info {
                for r in &info.routes {
                    candidates.insert(r.id.clone(), (r.model.clone(), false));
                }
            }
            for (model, route) in n.context.routes().keys() {
                candidates
                    .entry(route.clone())
                    .and_modify(|e| e.1 = true)
                    .or_insert((model.clone(), true));
            }
            egui::ScrollArea::vertical()
                .max_height(220.0)
                .show(ui, |ui| {
                    for (route, (model, seen)) in &candidates {
                        if held.contains(route.as_str()) {
                            continue;
                        }
                        let same = *model == n.model;
                        if !same && !seen {
                            continue;
                        }
                        ui.horizontal(|ui| {
                            ui.label(format!(
                                "{route} (line {model}){}",
                                if *seen { ", in view" } else { "" }
                            ));
                            if ui
                                .add_enabled(same, egui::Button::new("Add this route"))
                                .on_disabled_hover_text(
                                    "A working set is on one line: open that line's own \
                                     working set (File › Join a network)",
                                )
                                .clicked()
                            {
                                let mut spec = n.spec.clone();
                                spec.picks.push(Pick::Route {
                                    route: route.clone(),
                                });
                                send = Some(ClientMsg::SetWorkingSet { spec });
                            }
                        });
                    }
                });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Save as");
                ui.add(egui::TextEdit::singleline(&mut n.save_name).desired_width(160.0));
                if ui
                    .add_enabled(!n.save_name.trim().is_empty(), egui::Button::new("Save"))
                    .clicked()
                {
                    let mut spec = n.spec.clone();
                    spec.name = n.save_name.trim().to_owned();
                    send = Some(ClientMsg::SaveWorkingSet { spec });
                }
            });
            if let Some((text, error)) = &n.note {
                if *error {
                    crate::menu::error_label(ui, text);
                } else {
                    ui.label(egui::RichText::new(text).small());
                }
            }
        });
    if !open {
        n.window = false;
    }
    if let Some(m) = send
        && let Err(e) = link.send(m)
    {
        n.note = Some((e, true));
    }
    None
}
