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
use std::path::Path;
use std::time::Instant;

use ohl_core::grid::Bounds;
use ohl_session::offline::{
    LeftOut, NEIGHBOURS_KEY, NeighbourLine, OfflineMeta, offline_changes, tree_object_hashes,
    value_hash,
};
use ohl_session::rights::{Permission, UserRights};
use ohl_session::versions::{Bundle, MAIN, short};
use ohl_session::working_set::{Extent, Pick, WORKING_SET_KEY, WorkingSetSpec};
use ohl_session::{Command, Kind, Session};
use ohl_store_pg::offline::OfflineExport;

use super::NetHost;
use crate::network_rights as nr;
use crate::{BroughtBack, OfflineExported, Op};

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

impl NetHost {
    /// Whether every object of `part` (of model `code`) is seen by
    /// `rights`.
    fn sees_part(&mut self, rights: &UserRights, code: &str, part: &ohl_line::Line) -> bool {
        rights.sees_everything()
            || part_objects(part)
                .iter()
                .all(|(k, id)| self.visible(rights, code, *k, id))
    }

    /// What solving `sections` of model `code` reads, structure types
    /// resolved.
    fn neighbour_part(&mut self, code: &str, sections: &[String]) -> ohl_line::Line {
        let work = self.copies.join("work").join("offline");
        let caches = self.packager.caches.clone();
        let m = self.models.get_mut(code).expect("a model");
        let line = m.session.line();
        let mut structures: BTreeSet<String> = BTreeSet::new();
        for id in sections {
            if let Some(sec) = line.sections.iter().find(|x| &x.id == id) {
                structures.extend(sec.points.iter().map(|p| p.structure.clone()));
            }
        }
        let structures: Vec<String> = structures.into_iter().collect();
        let part = m.session.drawing_part(
            &structures,
            sections,
            &[],
            ohl_session::interest::SOLVE_NEAR,
        );
        if !m.session.any_typed() {
            return part;
        }
        let libs = crate::packager::model_libs(code, true, m.libs_rev, m.libs(), &caches, &work);
        crate::packager::resolve_part(part, &libs).0
    }

    /// Exports working set `spec` of `user` (acting with `rights`) as an
    /// offline working set into the new project file `path`, its
    /// neighbours taken within `reach` m (default the field distance).
    pub(super) fn offline_export(
        &mut self,
        user: &str,
        rights: &UserRights,
        spec: WorkingSetSpec,
        reach: Option<f64>,
        path: &Path,
    ) -> Result<OfflineExported, String> {
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
        for t in self.tiles_of(&area) {
            if let Some(st) = self.tiles.get(&t) {
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
            neighbour_sections += part.sections.iter().filter(|x| all.contains(&x.id)).count();
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
            neighbours.push(NeighbourLine {
                model,
                line: part,
                libraries,
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
            (
                NEIGHBOURS_KEY.to_owned(),
                serde_json::to_string(&neighbours).map_err(s)?,
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
        let written = Session::write_offline(replica, path, user, &meta, offline, &message);
        let (summary, offline, tree) = match written {
            Ok(w) => w,
            Err(e) => {
                let _ = std::fs::remove_file(path);
                return Err(format!("the offline working set: {e}"));
            }
        };
        let record = OfflineExport {
            id: number,
            model: code.clone(),
            user: user.to_owned(),
            time: String::new(),
            base: base.clone(),
            export: offline.export.clone(),
            tree,
            wanted: extent.wanted.iter().cloned().collect(),
            routes: routes.clone(),
            spec,
            reach,
            brought_back: Vec::new(),
        };
        if let Err(e) = self.net.record_offline_export(&record) {
            let _ = std::fs::remove_file(path);
            return Err(format!("the export was not recorded: {e}"));
        }
        let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let detail = format!(
            "offline working set {number}: routes {}, base {}, export {}; {} objects, {} \
             neighbour line(s) with {} section(s); left out (not seen): {} of other lines, {} \
             of this line",
            routes.join(", "),
            short(&base),
            short(&offline.export),
            summary.wanted + summary.context,
            neighbours.len(),
            neighbour_sections,
            left_out.other_lines,
            left_out.this_line
        );
        if let Err(e) = self.net.audit(user, "export working set", &code, &detail) {
            super::log(&format!("the audit: {e}"));
        }
        Ok(OfflineExported {
            model: code,
            number,
            bytes,
            wanted: summary.wanted,
            context: summary.context,
            neighbour_lines: neighbours.len(),
            neighbour_sections,
            left_out,
            base,
            export: offline.export,
            recorded_first: !clean,
            seconds: started.elapsed().as_secs_f64(),
        })
    }

    /// Brings back `bundle`, an offline working set's work of `user`
    /// (acting with `rights`), onto a branch over its working set made
    /// from the export's base (named `branch`, default
    /// `offline-USER-N-K`).
    pub(super) fn offline_import(
        &mut self,
        user: &str,
        rights: &UserRights,
        bundle: Bundle,
        branch: Option<String>,
    ) -> Result<BroughtBack, String> {
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
