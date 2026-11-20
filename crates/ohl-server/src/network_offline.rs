// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Offline working sets on a network's server (ADR-0013, phase 12).
//!
//! **Export** ([`NetHost::offline_export`]): a user's working set (picked
//! as a join picks it) written as a project file of today's format
//! ([`ohl_session::offline`]): the working set editable; its context and
//! the line's own neighbours within the checks' reach read-only; other
//! lines' neighbours within the reach (what solving their sections there
//! reads, structure types resolved, with their wire, weather and load
//! case files) in the meta. Everything is filtered by the rights' one
//! function: the working set must be seen whole (as a join), a neighbour
//! the user may not see is left out and counted — never written, named or
//! placed. The base is the model's main revision, its changes recorded
//! first by the rule of a branch over a working set (phase 11). The export
//! is recorded (schema 6: its revision and tree, no object again) and
//! audited. No reservation is made (plan of 2026-11-16).
//!
//! **Bringing back** ([`NetHost::offline_import`]): a bundle of only the
//! changes, verified against the export the network recorded (an unknown
//! base, another user's export, a bundle changed by hand: refused), its
//! changes applied on a **branch over the working set made from the
//! export's base** — never on main — as one command checked by the user's
//! current rights and the rules of shared towers (phase 8), then
//! committed; library files and the read-only neighbours are not changed
//! offline. What changed on main meanwhile is listed (conflicts its merge
//! will ask to settle), never settled. All of it or nothing: a refusal
//! takes the branch away again.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use ohl_core::grid::Bounds;
use ohl_session::offline::{
    LeftOut, NEIGHBOURS_KEY, NeighbourLine, OfflineMeta, offline_changes, tree_object_hashes,
    value_hash,
};
use ohl_session::part::PartSummary;
use ohl_session::rights::{Permission, UserRights};
use ohl_session::versions::{Bundle, MAIN, short};
use ohl_session::working_set::{Extent, Pick, Replica, WORKING_SET_KEY, WorkingSetSpec};
use ohl_session::{Command, Kind, Session};
use ohl_store_pg::offline::OfflineExport;

use super::NetHost;
use crate::network_rights as nr;
use crate::packager::{Caches, LibFiles};
use crate::{BroughtBack, OfflineExported, OfflineFile, Op, TileKey};

/// The largest reach an export takes, m (a plugin's reach's bound).
pub(crate) const MAX_REACH: f64 = 10_000.0;

/// The library kinds a neighbour line's checks read.
const NEIGHBOUR_LIBRARIES: [&str; 3] = ["cables", "weather", "loadcases"];

/// The routes of working set `spec` (its picks'; an area's: those its
/// extent is on) — a branch over it works on them.
pub(crate) fn routes_of(spec: &WorkingSetSpec, extent: &Extent) -> Vec<String> {
    let mut out = BTreeSet::new();
    for p in &spec.picks {
        match p {
            Pick::Route { route }
            | Pick::Stations { route, .. }
            | Pick::Structures { route, .. } => {
                out.insert(route.clone());
            }
            Pick::Area { .. } => {
                out.extend(extent.summary.routes.iter().map(|r| r.route.clone()));
            }
        }
    }
    out.into_iter().collect()
}

/// A rectangle as a polygon.
fn polygon(b: &Bounds) -> Vec<[f64; 2]> {
    vec![
        [b.min[0], b.min[1]],
        [b.max[0], b.min[1]],
        [b.max[0], b.max[1]],
        [b.min[0], b.max[1]],
    ]
}

/// The objects of a part of a line (kind, id).
fn part_objects(l: &ohl_line::Line) -> Vec<(Kind, String)> {
    let mut v = Vec::new();
    v.extend(l.routes.iter().map(|x| (Kind::Route, x.id.clone())));
    v.extend(l.alignments.iter().map(|x| (Kind::Alignment, x.id.clone())));
    v.extend(
        l.alignment_points
            .iter()
            .map(|x| (Kind::AlignmentPoint, x.id.clone())),
    );
    v.extend(l.structures.iter().map(|x| (Kind::Structure, x.id.clone())));
    v.extend(l.sections.iter().map(|x| (Kind::Section, x.id.clone())));
    v.extend(l.jumpers.iter().map(|x| (Kind::Jumper, x.id.clone())));
    v.extend(l.spacers.iter().map(|x| (Kind::Spacer, x.id.clone())));
    v
}

fn s(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// A neighbour line as an export takes it on the authority: its part as
/// the session holds it, the library files its checks read (those the
/// user sees) and, when a structure of it is typed, its structure types'
/// library (merged in when the export is written).
pub(super) struct NeighbourPlan {
    model: String,
    part: ohl_line::Line,
    libraries: BTreeMap<String, BTreeMap<String, String>>,
    types: Option<(u64, Arc<LibFiles>)>,
}

/// What an export's record and report need, decided on the authority.
pub(crate) struct ExportHead {
    pub(super) user: String,
    pub(super) code: String,
    pub(super) path: PathBuf,
    number: i64,
    base: String,
    spec: WorkingSetSpec,
    routes: Vec<String>,
    reach: f64,
    wanted: Vec<(Kind, String)>,
    left_out: LeftOut,
    recorded_first: bool,
    neighbour_lines: usize,
    neighbour_sections: usize,
    started: Instant,
    /// The file read back (base64) and removed by its writer: a client's
    /// export, sent through the protocol.
    pub(super) for_client: bool,
}

/// An export decided on the authority ([`NetHost::export_plan`]): what is
/// left is writing it ([`write_export`]), on the exports' thread or on the
/// authority — the same function either way.
pub(super) struct ExportPlan {
    pub(super) head: ExportHead,
    replica: Replica,
    meta: Vec<(String, String)>,
    neighbours: Vec<NeighbourPlan>,
    offline: OfflineMeta,
    message: String,
    /// The tiles of its area (the reach).
    tiles: BTreeSet<TileKey>,
    caches: Arc<Caches>,
    work: PathBuf,
}

/// A bundle decided on the authority ([`NetHost::bundle_decide`]): what
/// its final apply needs ([`NetHost::bundle_apply`]).
pub(crate) struct BundleDecided {
    started: Instant,
    what: String,
    head: String,
    row: OfflineExport,
    pub(super) code: String,
    changes: ohl_session::offline::OfflineChanges,
    wanted: BTreeSet<(Kind, String)>,
    exported: BTreeMap<(Kind, String), String>,
    name: String,
}

/// What an export holds, for the changes landing while it is written: its
/// objects (the line's and its neighbours'), its lines, its area's tiles.
#[derive(Default)]
pub(super) struct Holds {
    pub(super) objects: BTreeSet<(String, Kind, String)>,
    pub(super) models: BTreeSet<String>,
    pub(super) tiles: BTreeSet<TileKey>,
}

/// An export written ([`write_export`]).
pub(crate) struct ExportWritten {
    pub(super) head: ExportHead,
    written: Result<(PartSummary, OfflineMeta, String), String>,
    bytes: u64,
    /// A client's export: the file, base64 (or why it cannot be sent).
    pub(super) file: Option<Result<String, String>>,
    pub(super) holds: Holds,
}

/// What changed on the network while an export was written, after its
/// content was taken (phase 14): objects of it or within its reach (each
/// once), and whether a line of it changed whole (its libraries).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct Meanwhile {
    pub(super) objects: usize,
    pub(super) at_large: bool,
}

impl Meanwhile {
    fn stale(&self) -> bool {
        self.objects > 0 || self.at_large
    }
}

/// A project file and its SQLite companions removed.
pub(super) fn remove_project(path: &Path) {
    for ext in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
    }
}

/// Writes an export decided on the authority (phase 14: on the exports'
/// thread, or on the authority without one): the neighbours' structure
/// types merged in, the meta, the project file with its export revision
/// ([`Session::write_offline`]); for a client the file read back and
/// removed. Reads nothing of the authority.
pub(super) fn write_export(plan: ExportPlan) -> ExportWritten {
    let ExportPlan {
        head,
        replica,
        mut meta,
        neighbours,
        offline,
        message,
        tiles,
        caches,
        work,
    } = plan;
    let mut holds = Holds {
        objects: replica
            .rows
            .iter()
            .map(|r| (head.code.clone(), r.kind, r.id.clone()))
            .collect(),
        models: BTreeSet::from([head.code.clone()]),
        tiles,
    };
    for n in &neighbours {
        holds.models.insert(n.model.clone());
        holds.objects.extend(
            part_objects(&n.part)
                .into_iter()
                .map(|(k, id)| (n.model.clone(), k, id)),
        );
    }
    let lines: Vec<NeighbourLine> = neighbours
        .into_iter()
        .map(|n| {
            let line = match n.types {
                Some((rev, files)) => {
                    let libs =
                        crate::packager::model_libs(&n.model, true, rev, files, &caches, &work);
                    crate::packager::resolve_part(n.part, &libs).0
                }
                None => n.part,
            };
            NeighbourLine {
                model: n.model,
                line,
                libraries: n.libraries,
            }
        })
        .collect();
    let written = serde_json::to_string(&lines).map_err(s).and_then(|text| {
        meta.push((NEIGHBOURS_KEY.to_owned(), text));
        Session::write_offline(replica, &head.path, &head.user, &meta, offline, &message)
            .map_err(|e| format!("the offline working set: {e}"))
    });
    if written.is_err() {
        remove_project(&head.path);
    }
    let bytes = std::fs::metadata(&head.path).map(|m| m.len()).unwrap_or(0);
    let file = (head.for_client && written.is_ok()).then(|| {
        let file = std::fs::read(&head.path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| {
                if bytes.len() * 4 / 3 + 4096 > crate::MAX_MESSAGE {
                    Err(format!(
                        "the offline working set is {:.0} MB: more than a message carries \
                         (pick less, or ask for it with the command line on the server)",
                        bytes.len() as f64 / 1e6
                    ))
                } else {
                    Ok(crate::base64::encode(&bytes))
                }
            });
        remove_project(&head.path);
        file
    });
    ExportWritten {
        head,
        written,
        bytes,
        file,
        holds,
    }
}

impl NetHost {
    /// Whether every object of `part` (of model `code`) is seen by
    /// `rights`.
    fn sees_part(&mut self, rights: &UserRights, code: &str, part: &ohl_line::Line) -> bool {
        rights.sees_everything()
            || part_objects(part)
                .iter()
                .all(|(k, id)| self.visible(rights, code, *k, id))
    }

    /// What solving `sections` of model `code` reads, as the session's
    /// line holds it: its structure types are merged in when the export
    /// is written ([`write_export`]: on the exports' thread when the
    /// server has one) — the same objects either way, a type merged into
    /// its structure.
    fn neighbour_part(&mut self, code: &str, sections: &[String]) -> ohl_line::Line {
        let m = self.models.get_mut(code).expect("a model");
        let wanted: BTreeSet<&String> = sections.iter().collect();
        let structures: BTreeSet<String> = m
            .session
            .line()
            .sections
            .iter()
            .filter(|x| wanted.contains(&x.id))
            .flat_map(|x| x.points.iter().map(|p| p.structure.clone()))
            .collect();
        let structures: Vec<String> = structures.into_iter().collect();
        m.session.drawing_part(
            &structures,
            sections,
            &[],
            ohl_session::interest::SOLVE_NEAR,
        )
    }

    /// Exports working set `spec` of `user` (acting with `rights`) as an
    /// offline working set into the new project file `path`, its
    /// neighbours taken within `reach` m (default the field distance) —
    /// decided, written and recorded here, on the authority (the way
    /// without the exports' thread, and the command line's).
    pub(super) fn offline_export(
        &mut self,
        user: &str,
        rights: &UserRights,
        spec: WorkingSetSpec,
        reach: Option<f64>,
        path: &Path,
    ) -> Result<OfflineExported, String> {
        let plan = self.export_plan(user, rights, spec, reach, path, false)?;
        let written = write_export(plan);
        self.export_done(written, Meanwhile::default())
    }

    /// A client's export written here, on the authority (without the
    /// exports' thread): its report and the file (base64), the file
    /// removed.
    pub(super) fn client_export_here(
        &mut self,
        user: &str,
        rights: &UserRights,
        spec: WorkingSetSpec,
        reach: Option<f64>,
        path: &Path,
    ) -> Result<Box<OfflineFile>, String> {
        let result = self
            .export_plan(user, rights, spec, reach, path, true)
            .and_then(|plan| {
                let w = write_export(plan);
                let file = w.file.clone();
                let report = self.export_done(w, Meanwhile::default())?;
                let file = file.ok_or("the file was not read back")??;
                Ok(Box::new(OfflineFile { report, file }))
            });
        remove_project(path);
        result
    }

    /// An export decided (phase 14: what the authority does of it before
    /// the writing, on the exports' thread or here): the reach checked,
    /// the working set seen whole and exportable by `rights`, main's
    /// changes recorded first, the base taken; the neighbours within the
    /// reach chosen by the rights; the replica, the meta and the export's
    /// number taken — all in one moment, between two commands, so the
    /// file is the base revision's working set and the neighbours as they
    /// were then. `for_client`: the file read back and removed by the
    /// writer (sent through the protocol).
    pub(super) fn export_plan(
        &mut self,
        user: &str,
        rights: &UserRights,
        spec: WorkingSetSpec,
        reach: Option<f64>,
        path: &Path,
        for_client: bool,
    ) -> Result<ExportPlan, String> {
        let started = Instant::now();
        let reach = reach.unwrap_or(ohl_scope::reach::FIELD_DISTANCE);
        if !(reach.is_finite() && reach > 0.0 && reach <= MAX_REACH) {
            return Err(format!(
                "the checks' reach of an export: more than 0 m and at most {MAX_REACH} m"
            ));
        }
        if path.exists() {
            return Err(format!("{}: exists already", path.display()));
        }
        let code = spec.model.clone();
        let what = format!("{code}: offline working set");
        // The working set as a join takes it: everything of it seen.
        let extent = {
            let m = self
                .models
                .get_mut(&code)
                .ok_or_else(|| format!("the network has no line {code}"))?;
            m.session.pick_extent(&spec.picks).map_err(s)
        };
        let extent = extent.map_err(|e| self.sanitise(rights, &e))?;
        if let Err(why) = self.check_extent(rights, &code, &extent) {
            self.audit_refused(user, &what, &why);
            return Err(why);
        }
        let routes = routes_of(&spec, &extent);
        // Its routes: seen, something of each editable (a branch over the
        // working set is made of them when it comes back); its objects:
        // each one exported (its context and neighbours are only read, as
        // a join's replica holds them: seen).
        let allowed = {
            let m = &self.models[&code];
            nr::check_working_set(rights, &code, &routes, &m.places).and_then(|()| {
                match extent.wanted.iter().find(|(k, id)| {
                    !rights.allows(
                        Permission::Export,
                        &nr::target(&code, *k, id, m.places.get(*k, id)),
                    )
                }) {
                    Some((k, id)) => Err(format!(
                        "{user} may not export {} {id}: an offline working set needs export on \
                         each object of the working set",
                        k.as_str()
                    )),
                    None => Ok(()),
                }
            })
        };
        if let Err(why) = allowed {
            self.audit_refused(user, &what, &why);
            return Err(self.sanitise(rights, &why));
        }
        // The base: the main line's revision, its changes recorded first
        // (the rule of a branch over a working set, phase 11).
        let clean = {
            let m = self.models.get_mut(&code).expect("checked");
            match m.session.branch_revision(MAIN).map_err(s)? {
                Some(_) => !m.session.has_uncommitted(MAIN).map_err(s)?,
                None => false,
            }
        };
        if !clean {
            self.apply(
                None,
                user,
                &code,
                MAIN,
                Op::Commit {
                    message: format!("Recorded for an offline working set of {user}"),
                },
            )
            .map_err(|e| {
                let e = e.strip_prefix("refused: ").unwrap_or(&e).to_owned();
                self.sanitise(
                    rights,
                    &format!(
                        "refused: the main line's changes since its last revision are recorded \
                         before an export (as for a branch over a working set), and {e} — ask \
                         whoever may edit them all to commit the main line"
                    ),
                )
            })?;
        }
        let base = self.models[&code]
            .session
            .branch_revision(MAIN)
            .map_err(s)?
            .ok_or("the main line has no revision")?;
        // The reach: the working set's bounds grown by the reach and three
        // times the network's largest span margin — every area a scoped
        // check on it asks for (its wires' bounds grown by the reach and
        // twice the margin) lies inside.
        let max_margin = self.network_max_margin();
        let b = extent
            .summary
            .bounds
            .ok_or("the working set has no structure: nothing to export")?;
        let area = Bounds::new([b[0], b[1]], [b[2], b[3]]).grown(reach + 3.0 * max_margin + 1.0);
        let mut left_out = LeftOut::default();
        // The line's own neighbours: the sections with a span within the
        // reach, and what solving each alone reads (read-only); left out
        // whole when one of them is hidden.
        let near = {
            let picks = vec![Pick::Area {
                polygon: polygon(&area),
            }];
            self.models
                .get_mut(&code)
                .expect("checked")
                .session
                .extent_of(&picks)
        };
        let extra: Vec<(Kind, String)> = near.keep.difference(&extent.keep).cloned().collect();
        let hidden_here = !rights.sees_everything()
            && extra
                .iter()
                .any(|(k, id)| !self.visible(rights, &code, *k, id));
        let mut keep = extent.keep.clone();
        if hidden_here {
            left_out.this_line = extra.len();
        } else {
            keep.extend(extra);
        }
        let combined = Extent {
            wanted: extent.wanted.clone(),
            keep,
            summary: extent.summary.clone(),
        };
        // Other lines' neighbours: their sections in the tiles of the
        // reach, what solving them reads.
        let mut by_model: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let area_tiles = self.tiles_of(&area);
        for t in &area_tiles {
            if let Some(st) = self.tiles.get(t) {
                for (m, k, id) in &st.objects {
                    if *k == Kind::Section && &**m != code.as_str() {
                        by_model
                            .entry(m.to_string())
                            .or_default()
                            .insert(id.clone());
                    }
                }
            }
        }
        let mut neighbours = Vec::new();
        let mut neighbour_sections = 0usize;
        for (model, sections) in by_model {
            let all: Vec<String> = sections.into_iter().collect();
            let mut part = self.neighbour_part(&model, &all);
            if !self.sees_part(rights, &model, &part) {
                // Section by section: those whose solve reads only what the
                // user sees.
                let mut seen = Vec::new();
                for id in &all {
                    let p = self.neighbour_part(&model, std::slice::from_ref(id));
                    if self.sees_part(rights, &model, &p) {
                        seen.push(id.clone());
                    } else {
                        left_out.other_lines += 1;
                    }
                }
                if seen.is_empty() {
                    continue;
                }
                part = self.neighbour_part(&model, &seen);
            }
            let asked: BTreeSet<&String> = all.iter().collect();
            neighbour_sections += part
                .sections
                .iter()
                .filter(|x| asked.contains(&x.id))
                .count();
            let mut libraries: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
            for kind in NEIGHBOUR_LIBRARIES {
                let files = self.models[&model].session.library_files(kind).map_err(s)?;
                for (name, text) in files {
                    let id = format!("{kind}/{name}");
                    if self.visible(rights, &model, Kind::Library, &id) {
                        libraries
                            .entry(kind.to_owned())
                            .or_default()
                            .insert(name, text);
                    } else {
                        // A file it may not see: the line's checks offline
                        // cannot be the server's.
                        left_out.other_lines += 1;
                    }
                }
            }
            let m = self.models.get_mut(&model).expect("a model");
            let types = m.session.any_typed().then(|| (m.libs_rev, m.libs()));
            neighbours.push(NeighbourPlan {
                model,
                part,
                libraries,
                types,
            });
        }
        let number = self.net.next_offline_export().map_err(s)?;
        let pins = self.net.line_pins(&code).map_err(s)?;
        let mut replica = self
            .models
            .get_mut(&code)
            .expect("checked")
            .session
            .replica_of(&combined);
        replica.users = self.filter_users(rights, &code, std::mem::take(&mut replica.users));
        let time = self.models[&code].session.now().map_err(s)?;
        let offline = OfflineMeta {
            network: self.name.clone(),
            model: code.clone(),
            user: user.to_owned(),
            time: time.clone(),
            base: base.clone(),
            export: String::new(),
            spec: spec.clone(),
            routes: routes.clone(),
            reach_m: reach,
            area: [area.min[0], area.min[1], area.max[0], area.max[1]],
            max_margin,
            library_pins: pins.clone(),
            left_out: left_out.clone(),
            neighbour_lines: neighbours.len(),
            neighbour_sections,
        };
        let meta = vec![
            ("network.name".to_owned(), self.name.clone()),
            ("network.line".to_owned(), code.clone()),
            (
                "network.position".to_owned(),
                serde_json::to_string(&self.position).map_err(s)?,
            ),
            (
                WORKING_SET_KEY.to_owned(),
                serde_json::to_string(&spec).map_err(s)?,
            ),
            (
                "network.library_pins".to_owned(),
                serde_json::to_string(&pins).map_err(s)?,
            ),
        ];
        let message = format!(
            "Offline working set {number} of {user}: {} (from revision {}, {time})",
            spec.picks
                .iter()
                .map(Pick::describe)
                .collect::<Vec<_>>()
                .join(", "),
            short(&base)
        );
        Ok(ExportPlan {
            head: ExportHead {
                user: user.to_owned(),
                code,
                path: path.to_owned(),
                number,
                base,
                spec,
                routes,
                reach,
                wanted: extent.wanted.iter().cloned().collect(),
                left_out,
                recorded_first: !clean,
                neighbour_lines: neighbours.len(),
                neighbour_sections,
                started,
                for_client,
            },
            tiles: area_tiles.into_iter().collect(),
            replica,
            meta,
            neighbours,
            offline,
            message,
            caches: self.packager.caches.clone(),
            work: self.copies.join("work").join("offline"),
        })
    }

    /// An export written: recorded (schema 6: its revision and tree, no
    /// object again) and audited here, on the authority; what changed on
    /// the network while it was written (`meanwhile`) said in its report.
    pub(super) fn export_done(
        &mut self,
        w: ExportWritten,
        meanwhile: Meanwhile,
    ) -> Result<OfflineExported, String> {
        let ExportWritten {
            head,
            written,
            bytes,
            ..
        } = w;
        let (summary, offline, tree) = written?;
        let path = &head.path;
        let record = OfflineExport {
            id: head.number,
            model: head.code.clone(),
            user: head.user.clone(),
            time: String::new(),
            base: head.base.clone(),
            export: offline.export.clone(),
            tree,
            wanted: head.wanted.clone(),
            routes: head.routes.clone(),
            spec: head.spec.clone(),
            reach: head.reach,
            brought_back: Vec::new(),
        };
        if let Err(e) = self.net.record_offline_export(&record) {
            remove_project(path);
            return Err(format!("the export was not recorded: {e}"));
        }
        let changed = if meanwhile.stale() {
            format!(
                "; {} object(s) of it changed on the server while it was written{} (not in it)",
                meanwhile.objects,
                if meanwhile.at_large {
                    ", a line of it changed whole"
                } else {
                    ""
                }
            )
        } else {
            String::new()
        };
        let detail = format!(
            "offline working set {}: routes {}, base {}, export {}; {} objects, {} neighbour \
             line(s) with {} section(s); left out (not seen): {} of other lines, {} of this \
             line{changed}",
            head.number,
            head.routes.join(", "),
            short(&head.base),
            short(&offline.export),
            summary.wanted + summary.context,
            head.neighbour_lines,
            head.neighbour_sections,
            head.left_out.other_lines,
            head.left_out.this_line
        );
        if let Err(e) = self
            .net
            .audit(&head.user, "export working set", &head.code, &detail)
        {
            super::log(&format!("the audit: {e}"));
        }
        Ok(OfflineExported {
            model: head.code,
            number: head.number,
            bytes,
            wanted: summary.wanted,
            context: summary.context,
            neighbour_lines: head.neighbour_lines,
            neighbour_sections: head.neighbour_sections,
            left_out: head.left_out,
            base: head.base,
            export: offline.export,
            recorded_first: head.recorded_first,
            seconds: head.started.elapsed().as_secs_f64(),
            changed_meanwhile: meanwhile.objects,
            stale: meanwhile.stale(),
        })
    }

    /// Brings back `bundle`, an offline working set's work of `user`
    /// (acting with `rights`), onto a branch over its working set made
    /// from the export's base (named `branch`, default
    /// `offline-USER-N-K`) — decided and applied here, on the authority
    /// (the way without the exports' thread).
    pub(super) fn offline_import(
        &mut self,
        user: &str,
        rights: &UserRights,
        bundle: Bundle,
        branch: Option<String>,
    ) -> Result<BroughtBack, String> {
        let decided = self.bundle_decide(user, rights, bundle, branch)?;
        self.bundle_apply(user, rights, decided, None)
    }

    /// A bundle decided (the exports' thread, 2026-11-20: what the
    /// authority does of it before its branch's line is made ready): its
    /// base the one revision its revisions come from, an export of this
    /// network by `user`, not brought back already, verified against the
    /// export's record, no library file and nothing read-only changed —
    /// refusals audited.
    pub(super) fn bundle_decide(
        &mut self,
        user: &str,
        rights: &UserRights,
        bundle: Bundle,
        branch: Option<String>,
    ) -> Result<BundleDecided, String> {
        let started = Instant::now();
        let what = format!("bundle {}", short(&bundle.head));
        let refuse = |h: &mut Self, why: String| -> String {
            h.audit_refused(user, &what, &why);
            h.sanitise(rights, &why)
        };
        // Its base: the one revision its revisions come from.
        let ids: BTreeSet<&str> = bundle.revisions.iter().map(|r| r.id.as_str()).collect();
        let mut roots: BTreeSet<String> = bundle
            .revisions
            .iter()
            .flat_map(|r| r.parents.iter())
            .filter(|p| !ids.contains(p.as_str()))
            .cloned()
            .collect();
        roots.extend(
            bundle
                .revisions
                .iter()
                .filter(|r| r.parents.is_empty())
                .map(|r| r.id.clone()),
        );
        if roots.len() != 1 {
            return Err(refuse(
                self,
                "the bundle is not the work of one offline working set (its revisions come from \
                 several bases, or none)"
                    .into(),
            ));
        }
        let base = roots.into_iter().next().expect("one");
        let row = match self.net.offline_export(&base).map_err(s)? {
            Some(r) => r,
            None => {
                return Err(refuse(
                    self,
                    format!(
                        "this network does not know the bundle's base {}: it was not exported \
                         from this network as an offline working set",
                        short(&base)
                    ),
                ));
            }
        };
        if row.user != user {
            return Err(refuse(
                self,
                format!(
                    "offline working set {} was exported by another user: its work comes back by \
                     them",
                    row.id
                ),
            ));
        }
        let code = row.model.clone();
        if !self.models.contains_key(&code) {
            return Err(refuse(self, format!("the network has no line {code} now")));
        }
        let changes = match offline_changes(&bundle, &row.export, &row.tree) {
            Ok(c) => c,
            Err(e) => return Err(refuse(self, e.to_string())),
        };
        if let Some((_, b)) = row.brought_back.iter().find(|(h, _)| *h == bundle.head) {
            return Err(refuse(
                self,
                format!(
                    "this bundle (head {}) was brought back already, onto branch {b}",
                    short(&bundle.head)
                ),
            ));
        }
        if !changes.libraries.is_empty() {
            return Err(refuse(
                self,
                format!(
                    "the bundle changes library files ({}): they change on the network (a request \
                     to the library's maintainers), not in an offline working set",
                    changes
                        .libraries
                        .iter()
                        .map(|(k, n)| format!("{k}/{n}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ));
        }
        // The read-only neighbours and context: not changed offline.
        let wanted: BTreeSet<(Kind, String)> = row.wanted.iter().cloned().collect();
        let exported = tree_object_hashes(&row.tree).map_err(s)?;
        let read_only: Vec<String> = changes
            .objects()
            .into_iter()
            .filter(|k| exported.contains_key(k) && !wanted.contains(k))
            .map(|(k, id)| format!("{} {id}", k.as_str()))
            .collect();
        if !read_only.is_empty() {
            return Err(refuse(
                self,
                format!(
                    "the bundle changes what was read-only in the offline working set ({}): \
                     neighbours and context are changed with their own routes — nothing applied",
                    read_only.join(", ")
                ),
            ));
        }
        // A branch over the working set made from the export's base.
        let name = branch
            .unwrap_or_else(|| format!("offline-{user}-{}-{}", row.id, row.brought_back.len() + 1));
        Ok(BundleDecided {
            started,
            what,
            head: bundle.head,
            row,
            code,
            changes,
            wanted,
            exported,
            name,
        })
    }

    /// The line a decided bundle's branch is made of, as the exports'
    /// thread makes it ready ([`ohl_session::lines::LineSource`]): the
    /// export's base read along the main line's.
    pub(super) fn bundle_source(
        &self,
        d: &BundleDecided,
    ) -> Result<ohl_session::lines::LineSource, String> {
        self.models
            .get(&d.code)
            .ok_or_else(|| format!("the network has no line {} now", d.code))?
            .session
            .line_source(&d.row.base)
            .map_err(s)
    }

    /// A decided bundle applied (the final apply, always on the
    /// authority): its branch over the working set made from the export's
    /// base — of `ready`, the line the exports' thread made ready, when
    /// given — its changes applied there as one command checked by the
    /// user's rights now and the rules of shared towers, committed; the
    /// conflicts with main as main is now listed. All or nothing.
    pub(super) fn bundle_apply(
        &mut self,
        user: &str,
        rights: &UserRights,
        d: BundleDecided,
        ready: Option<ohl_session::lines::PreparedLine>,
    ) -> Result<BroughtBack, String> {
        let BundleDecided {
            started,
            what,
            head,
            row,
            code,
            changes,
            wanted,
            exported,
            name,
        } = d;
        let refuse = |h: &mut Self, why: String| -> String {
            h.audit_refused(user, &what, &why);
            h.sanitise(rights, &why)
        };
        if !self.models.contains_key(&code) {
            return Err(refuse(self, format!("the network has no line {code} now")));
        }
        if let Some(ready) = ready {
            // Brought back meanwhile (by another of the user's
            // connections): refused as at its decision.
            let again = self.net.offline_export(&row.base).map_err(s)?;
            if let Some((_, b)) = again
                .as_ref()
                .and_then(|r| r.brought_back.iter().find(|(h, _)| *h == head))
            {
                return Err(refuse(
                    self,
                    format!(
                        "this bundle (head {}) was brought back already, onto branch {b}",
                        short(&head)
                    ),
                ));
            }
            self.models
                .get_mut(&code)
                .expect("checked")
                .session
                .offer_line(ready);
        }
        let made = self.apply(
            None,
            user,
            &code,
            MAIN,
            Op::CreateWorkingSetBranch {
                name: name.clone(),
                from: row.base.clone(),
                routes: row.routes.clone(),
                team: Vec::new(),
            },
        );
        // A line made ready and not taken (refused first): not kept.
        if let Some(m) = self.models.get_mut(&code) {
            m.session.withdraw_line_offer();
        }
        if let Err(e) = made {
            return Err(refuse(self, e));
        }
        // What changed on main since the export, on what the bundle
        // changes: conflicts listed, never settled.
        let objects = changes.objects();
        let mut conflicts = Vec::new();
        for (k, id) in &objects {
            let (at_base, now) = {
                let m = &self.models[&code];
                (
                    m.session.line_object_value(&name, *k, id),
                    m.session.line_object_value(MAIN, *k, id),
                )
            };
            if at_base != now {
                conflicts.push(if self.visible(rights, &code, *k, id) {
                    format!("{} {id}", k.as_str())
                } else {
                    "an object you may not see".to_owned()
                });
            }
        }
        let mut commands: Vec<Command> = changes
            .puts
            .iter()
            .map(|(kind, id, value)| Command::Put {
                kind: *kind,
                id: id.clone(),
                value: value.clone(),
                ord: None,
            })
            .collect();
        commands.extend(changes.deletes.iter().map(|(kind, id)| Command::Delete {
            kind: *kind,
            id: id.clone(),
            confirm: false,
        }));
        let message = format!(
            "Offline work of {user} (offline working set {}, {} revision(s), head {}): {}",
            row.id,
            changes.revisions.len(),
            short(&changes.head),
            changes
                .revisions
                .iter()
                .map(|(_, _, m)| m.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        );
        let applied = self
            .apply(
                None,
                user,
                &code,
                &name,
                Op::Do {
                    command: Command::Batch { commands },
                },
            )
            .and_then(|()| self.apply(None, user, &code, &name, Op::Commit { message }));
        if let Err(e) = applied {
            // All or nothing: the branch goes again.
            let _ = self.apply(
                None,
                user,
                &code,
                MAIN,
                Op::DeleteLine { name: name.clone() },
            );
            return Err(refuse(self, e));
        }
        // The read-only neighbours changed on the server since the export.
        let context_changed = {
            let m = &self.models[&code];
            exported
                .iter()
                .filter(|(k, _)| !wanted.contains(*k))
                .filter(|((k, id), h)| {
                    m.session
                        .line_object_value(MAIN, *k, id)
                        .is_none_or(|v| &value_hash(&v) != *h)
                })
                .count()
        };
        if let Err(e) = self.net.note_brought_back(row.id, &changes.head, &name) {
            super::log(&format!("offline export {}: {e}", row.id));
        }
        let detail = format!(
            "bundle {} of offline working set {}: branch {name}, {} put, {} deleted, {} \
             conflict(s) with main, {} read-only neighbour(s) changed since",
            short(&changes.head),
            row.id,
            changes.puts.len(),
            changes.deletes.len(),
            conflicts.len(),
            context_changed
        );
        if let Err(e) = self.net.audit(user, "import bundle", &code, &detail) {
            super::log(&format!("the audit: {e}"));
        }
        Ok(BroughtBack {
            model: code,
            branch: name,
            revisions: changes.revisions.len(),
            puts: changes.puts.len(),
            deletes: changes.deletes.len(),
            conflicts,
            context_changed,
            seconds: started.elapsed().as_secs_f64(),
        })
    }
}
