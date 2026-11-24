// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! **The neighbours of an offline working set in the check windows**
//! (2026-11-23; ADR-0013 phase 12, ROADMAP O17's "left"): an offline
//! working set's file (Versions › Offline copy › Export working set for
//! offline work…) holds the other lines near its working set as the
//! network's server exported them (`network.neighbours`). The ordinary
//! check windows read them when such a file is open — never silently:
//!
//! - **Clearances** (wire clearances by rule): the other lines' wires are
//!   the targets of the rules to other lines (their crossings among
//!   them), each solved in every position checked with its own line's
//!   settings and wire library, as the network's checks solve them;
//! - **Structure loads**: the towers shared with another line of the
//!   export (twins: the same type within 0.5 m) named and, since
//!   2026-11-24, computed with every line's sections on them under each
//!   line's cases, the worst kept, by the network's own check on the file
//!   (the Results' Shared towers view, [`crate::loads_shared`]);
//! - **Magnetic field** (2026-11-24): the export's lines whose wires come
//!   within the network's field distance of the spans computed, each
//!   solved in the run's case with its own settings and wires, their
//!   field added by size at every profile point, given point and cut —
//!   the network's own selection and sources
//!   ([`MagneticNeighbours`], `ohl_server::offline_checks::field_neighbours`);
//! - **Electric field**: the export's lines within the field distance
//!   named — their field is not added (NOT ALL CHECKED; the network
//!   computes no electric field with other lines either);
//! - a **plugin's check** whose reach needs neighbours: they are not
//!   handed to the plugin (NOT ALL CHECKED, named).
//!
//! Each window says that the export's neighbours of its date are
//! included, which lines and sections it read, and that **they may be
//! stale**: the file knows nothing of the network after its export. What
//! changed while the export was written (the server's
//! `changed_meanwhile`) is known when OHL-Design wrote the file: it keeps
//! the server's answer in the file's application setting
//! [`RECORD_KEY`] (not part of the line, not in its revisions, not in a
//! bundle). Where the file cannot give a complete answer — neighbours its
//! user may not see left out of the export, a check reaching beyond what
//! was exported, a neighbour that does not solve — the result says
//! **NOT ALL CHECKED**, never passing on that account.

use std::path::Path;
use std::sync::Arc;

use ohl_core::grid::Bounds;
use ohl_line::Line;
use ohl_server::OfflineExported;
use ohl_server::offline_checks::NeighbourWires;
use ohl_session::Session;
use ohl_session::offline::OfflineMeta;
use serde::{Deserialize, Serialize};

/// The application setting holding what the server said of the export
/// when OHL-Design wrote the file (JSON, [`ExportRecord`]).
pub const RECORD_KEY: &str = "app.offline_export";

/// The manual's section on the neighbours in the check windows.
pub const GUIDE: (&str, &str) = (
    "user-guide/offline-working-set",
    "checks-with-the-exports-neighbours",
);

/// How far a wire may lie from its structure's position in plan (the
/// arms), m: added to every search for neighbours (more candidates, the
/// safe side).
const ARM_MARGIN: f64 = 30.0;

/// What the server said of an export, kept in the exported file
/// ([`RECORD_KEY`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportRecord {
    /// The export's number on the server.
    pub number: i64,
    /// A change landed on the network while the file was written.
    pub stale: bool,
    /// Objects of the working set or within its reach changed meanwhile
    /// (not in the file).
    pub changed_meanwhile: usize,
}

/// Keeps what the server said of the export written to `path` in the file
/// itself ([`RECORD_KEY`]): its check windows say it later.
pub fn record_export(path: &Path, report: &OfflineExported) -> Result<(), String> {
    let s = Session::open(path).map_err(|e| e.to_string())?;
    let record = ExportRecord {
        number: report.number,
        stale: report.stale,
        changed_meanwhile: report.changed_meanwhile,
    };
    s.set_setting(
        RECORD_KEY,
        Some(&serde_json::to_string(&record).expect("serialises")),
    )
    .map_err(|e| e.to_string())
}

/// The neighbours an open offline working set holds.
#[derive(Debug, Clone)]
pub struct Neighbours {
    /// The export's record.
    pub meta: OfflineMeta,
    /// The other lines near, each with its wire library.
    pub lines: Vec<NeighbourWires>,
    /// What the server said when OHL-Design wrote the file (none: written
    /// otherwise, e.g. by `ohl network export-working-set`).
    pub record: Option<ExportRecord>,
}

/// A tower of the working set shared with another line of the export.
#[derive(Debug, Clone, PartialEq)]
pub struct Twin {
    /// This line's structure.
    pub structure: String,
    /// The other line.
    pub model: String,
    /// Its structure there.
    pub other: String,
}

impl Neighbours {
    /// The neighbours of `session` when it is an offline working set
    /// (none: it is not one).
    pub fn read(session: &Session) -> Result<Option<Self>, String> {
        let Some(meta) = session.offline_meta().map_err(|e| e.to_string())? else {
            return Ok(None);
        };
        let lines = ohl_server::offline_checks::neighbour_wires(session)?;
        let record = session
            .setting(RECORD_KEY)
            .map_err(|e| e.to_string())?
            .and_then(|t| serde_json::from_str(&t).ok());
        Ok(Some(Self {
            meta,
            lines,
            record,
        }))
    }

    /// When it was exported, in words: "2026-11-20 14:03:12 UTC".
    pub fn when(&self) -> String {
        let t = self.meta.time.trim();
        let t = t.strip_suffix('Z').unwrap_or(t).replacen('T', " ", 1);
        format!("{t} UTC")
    }

    /// "the export of 2026-11-20 14:03:12 UTC (export 3)".
    pub fn export_words(&self) -> String {
        match &self.record {
            Some(r) => format!("the export of {} (export {})", self.when(), r.number),
            None => format!("the export of {}", self.when()),
        }
    }

    /// The lines read, in words: "line L002 (12 sections), line L007 (3
    /// sections)", or "no other line near".
    pub fn read_words(&self) -> String {
        if self.lines.is_empty() {
            return format!(
                "no other line within {:.0} m of the working set",
                self.meta.reach_m
            );
        }
        self.lines
            .iter()
            .map(|n| {
                format!(
                    "line {} ({} section{})",
                    n.model,
                    n.line.sections.len(),
                    if n.line.sections.len() == 1 { "" } else { "s" }
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// That they may be stale, with what is known of it.
    pub fn stale_words(&self) -> String {
        let mut out = format!(
            "They may be stale: the file knows nothing of the network after {} — changes made \
             there since are not in it (bringing the work back lists them).",
            self.when()
        );
        match &self.record {
            Some(r) if r.stale => out.push_str(&format!(
                " STALE already when exported: {} object(s) of the working set or within its \
                 reach changed on the network while the file was written — not in the file.",
                r.changed_meanwhile
            )),
            Some(_) => out.push_str(" Nothing changed on the network while the file was written."),
            None => out.push_str(
                " Whether anything changed while it was written is not known (not exported by \
                 OHL-Design).",
            ),
        }
        out
    }

    /// The export's own gaps, each a NOT ALL CHECKED reason.
    pub fn gaps(&self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.meta.left_out.is_empty() {
            out.push(format!(
                "NOT ALL CHECKED: neighbours you may not see were left out of the export ({} of \
                 other lines, {} of this line): they are checked on the network's server only",
                self.meta.left_out.other_lines, self.meta.left_out.this_line
            ));
        }
        for n in &self.lines {
            for note in &n.notes {
                out.push(format!(
                    "NOT ALL CHECKED: line {}'s {note} (its wires taken from what could be read)",
                    n.model
                ));
            }
        }
        out
    }

    /// The window's heading line: "Neighbours from the export of … are
    /// included: …".
    pub fn included_words(&self, how: &str) -> String {
        format!(
            "Neighbours from {} are included ({how}): {}, taken within {:.0} m of the working \
             set.",
            self.export_words(),
            self.read_words(),
            self.meta.reach_m
        )
    }

    /// Whether plan area `b` lies within what the export holds.
    pub fn covers(&self, b: &Bounds) -> bool {
        let a = self.meta.area;
        b.min[0] >= a[0] && b.min[1] >= a[1] && b.max[0] <= a[2] && b.max[1] <= a[3]
    }

    /// The neighbour lines and their sections whose structures come
    /// within `distance` of `area` in plan (the arms and the export's
    /// largest span margin added: more, never fewer).
    pub fn near(&self, area: &Bounds, distance: f64) -> Vec<(&NeighbourWires, Vec<String>)> {
        let wide = area.grown(distance + ARM_MARGIN + self.meta.max_margin.max(0.0));
        self.lines
            .iter()
            .filter_map(|n| {
                let secs: Vec<String> = n
                    .line
                    .sections
                    .iter()
                    .filter(|s| section_bounds(&n.line, s).is_some_and(|b| b.intersects(&wide)))
                    .map(|s| s.id.clone())
                    .collect();
                (!secs.is_empty()).then_some((n, secs))
            })
            .collect()
    }

    /// The structures of `structures` (ids of `line`) shared with another
    /// line of the export: one of the same type within
    /// [`ohl_scope::reach::TWIN_TOLERANCE`] in plan (as the network finds
    /// twins).
    pub fn twins(&self, line: &Line, structures: &[String]) -> Vec<Twin> {
        let tol = ohl_scope::reach::TWIN_TOLERANCE;
        let mut out = Vec::new();
        for id in structures {
            let Some(s) = line.structure(id) else {
                continue;
            };
            for n in &self.lines {
                for t in &n.line.structures {
                    let d = (t.position[0] - s.position[0]).hypot(t.position[1] - s.position[1]);
                    if d <= tol && t.structure_type == s.structure_type {
                        out.push(Twin {
                            structure: id.clone(),
                            model: n.model.clone(),
                            other: t.id.clone(),
                        });
                    }
                }
            }
        }
        out
    }
}

/// A section's structures' extent in plan.
pub fn section_bounds(line: &Line, s: &ohl_line::Section) -> Option<Bounds> {
    Bounds::of_points(
        s.points
            .iter()
            .filter_map(|p| line.structure(&p.structure).map(|st| &st.position)),
    )
}

/// The extent in plan of sections `sections` of `line` (empty: every
/// one).
pub fn sections_bounds(line: &Line, sections: &[String]) -> Option<Bounds> {
    line.sections
        .iter()
        .filter(|s| sections.is_empty() || sections.contains(&s.id))
        .filter_map(|s| section_bounds(line, s))
        .reduce(|a, b| a.union(&b))
}

/// What a window says of the export's neighbours for a check that does
/// not add them (Structure loads, the fields, a plugin): the lines near
/// named, NOT ALL CHECKED for them; nothing near: said, complete for
/// neighbours. `what` names the check, `how` where they are computed.
pub fn not_added_notes(
    n: &Neighbours,
    what: &str,
    near: &[(&NeighbourWires, Vec<String>)],
    within: &str,
    how: &str,
) -> Vec<String> {
    let mut out = Vec::new();
    if near.is_empty() {
        out.push(format!(
            "Neighbours from {} read: {} — none {within}: nothing of them is missing from {what}. \
             {}",
            n.export_words(),
            n.read_words(),
            n.stale_words()
        ));
    } else {
        let lines: Vec<String> = near
            .iter()
            .map(|(l, s)| format!("line {} ({} section(s))", l.model, s.len()))
            .collect();
        out.push(format!(
            "NOT ALL CHECKED: {} of {} {within} — not added in {what}: {how}",
            lines.join(", "),
            n.export_words()
        ));
        out.push(n.stale_words());
    }
    out.extend(n.gaps());
    out
}

/// What a field window says of the export's neighbours (2026-11-23) on an
/// offline working set's file: the lines within the network's field
/// distance named, their field not added here (NOT ALL CHECKED); on a
/// network's working set joined (phase 10): its neighbours not pulled
/// in. Nothing otherwise. `magnetic`: the magnetic field (the Offline
/// working set window adds the others' by size), else the electric one.
pub fn field_notes(model: &crate::model::Model, magnetic: bool) -> Vec<String> {
    let what = if magnetic {
        "the magnetic field"
    } else {
        "the electric field"
    };
    let distance = ohl_scope::reach::FIELD_DISTANCE;
    let Some(n) = model.offline_neighbours() else {
        return model
            .network_replica()
            .then(|| {
                ohl_scope::reach::replica_note(
                    what,
                    &ohl_scope::reach_of(&ohl_scope::CheckKind::Fields { distance }),
                )
            })
            .flatten()
            .into_iter()
            .collect();
    };
    let n = match n.as_ref() {
        Ok(n) => n,
        Err(e) => {
            return vec![format!(
                "NOT ALL CHECKED: the offline working set's neighbours could not be read ({e}): \
                 the field of other lines near not looked for"
            )];
        }
    };
    let Some(area) = sections_bounds(&model.line, &[]) else {
        return Vec::new();
    };
    let near = n.near(&area, distance);
    let how = if magnetic {
        "Versions › Offline copy › Check the offline working set… › Magnetic field adds their \
         field by size (the safe side)"
    } else {
        "no check adds them yet — the field where they come near is NOT ON THE SAFE SIDE"
    };
    let mut out = not_added_notes(
        n,
        &format!("{what} of this window"),
        &near,
        &format!("within {distance:.0} m (the network's field distance)"),
        how,
    );
    if !n.covers(&area.grown(distance)) {
        out.push(format!(
            "NOT ALL CHECKED: {distance:.0} m around the line in this file (its working set and \
             the read-only parts around it) reaches beyond what the export holds (other lines \
             within {:.0} m of the working set): lines there may be near and are not named",
            n.meta.reach_m
        ));
    }
    out
}

/// What a plugin's check says of the export's neighbours (2026-11-23) on
/// an offline working set's file, for its reach: they are not handed to
/// the plugin — NOT ALL CHECKED when its reach needs them (the lines it
/// would reach named); nothing when it needs none or the file is not one.
pub fn plugin_notes(
    model: &crate::model::Model,
    check: &str,
    reach: &ohl_scope::Reach,
) -> Option<Vec<String>> {
    let n = model.offline_neighbours()?;
    if !ohl_scope::reach::needs_neighbours(reach) {
        return Some(Vec::new());
    }
    let n = match n.as_ref() {
        Ok(n) => n,
        Err(e) => {
            return Some(vec![format!(
                "NOT ALL CHECKED: the offline working set's neighbours could not be read ({e})"
            )]);
        }
    };
    let metres = match reach {
        ohl_scope::Reach::Own => 0.0,
        ohl_scope::Reach::Wires { metres }
        | ohl_scope::Reach::Lines { metres }
        | ohl_scope::Reach::Unknown { metres } => *metres,
        ohl_scope::Reach::Towers { tolerance } => *tolerance,
    };
    let area = sections_bounds(&model.line, &[])?;
    let near = n.near(&area, metres);
    Some(not_added_notes(
        n,
        &format!("the plugin's check {check}"),
        &near,
        &format!("within its reach ({})", reach.describe()),
        "a plugin's check gets this line only — check on the network for its neighbours",
    ))
}

/// What the Magnetic field window takes at Run to add an offline working
/// set's other lines to its field (2026-11-24): the export's neighbours,
/// the file, the current of their circuits without one of their own.
pub struct MagneticNeighbours {
    n: Arc<Result<Neighbours, String>>,
    path: std::path::PathBuf,
    amps: Option<f64>,
}

impl MagneticNeighbours {
    /// On an offline working set's file (none otherwise); `amps`: the
    /// current of the other lines' circuits without an operating current
    /// of their own, A (none: 0 A, said).
    pub fn of(model: &crate::model::Model, amps: Option<f64>) -> Option<Self> {
        Some(Self {
            n: model.offline_neighbours()?,
            path: model.sources.file.clone(),
            amps,
        })
    }

    /// Adds to `field` (of `line` in weather case `case`) the field of the
    /// export's other lines whose wires come within the network's field
    /// distance of the spans it computed — each line solved in `case` with
    /// its own settings and wires and its field added by size at every
    /// profile point, given point and cut, by the network's own code
    /// ([`ohl_server::offline_checks::field_neighbours`]); gives what the
    /// results say first (the lines, that they may be stale, NOT ALL
    /// CHECKED where incomplete).
    pub fn add(
        &self,
        line: &Line,
        case: &ohl_weather::WeatherCase,
        field: &mut ohl_electrical::magnetic::MagneticField,
    ) -> Vec<String> {
        let n = match self.n.as_ref() {
            Ok(n) => n,
            Err(e) => {
                return vec![format!(
                    "NOT ALL CHECKED: the offline working set's neighbours could not be read \
                     ({e}): the field of other lines near not added"
                )];
            }
        };
        let distance = ohl_scope::reach::FIELD_DISTANCE;
        // The spans computed: the other lines are taken within the field
        // distance of their sections' wires (the working set's when only
        // points are given).
        let mut spans: Vec<(&str, &str)> = Vec::new();
        spans.extend(
            field
                .profiles
                .iter()
                .map(|p| (p.from.as_str(), p.to.as_str())),
        );
        spans.extend(field.cuts.iter().map(|c| (c.from.as_str(), c.to.as_str())));
        spans.extend(
            field
                .volumes
                .iter()
                .map(|v| (v.from.as_str(), v.to.as_str())),
        );
        let sections: Vec<String> = line
            .sections
            .iter()
            .filter(|s| {
                s.points.windows(2).any(|w| {
                    spans.iter().any(|(a, b)| {
                        (w[0].structure == *a && w[1].structure == *b)
                            || (w[0].structure == *b && w[1].structure == *a)
                    })
                })
            })
            .map(|s| s.id.clone())
            .collect();
        let spec = ohl_scope::fields::FieldSpec {
            distance,
            amps: self.amps,
            ..Default::default()
        };
        let mut out = Vec::new();
        let f = match ohl_server::offline_checks::field_neighbours(
            &self.path, &sections, case, &spec,
        ) {
            Ok(f) => f,
            Err(e) => {
                out.push(format!(
                    "NOT ALL CHECKED: the field of the export's other lines was not computed \
                     ({e}): {} — not added",
                    n.read_words()
                ));
                out.push(n.stale_words());
                out.extend(n.gaps());
                return out;
            }
        };
        let refs: Vec<&ohl_electrical::magnetic::Wires3d> =
            f.others.lines.iter().map(|l| &l.wires).collect();
        add_others(field, &refs);
        if f.others.lines.is_empty() {
            out.push(format!(
                "Neighbours from {} read: {} — none within {distance:.0} m of the spans computed \
                 (the network's field distance): nothing of other lines is missing from the \
                 field.",
                n.export_words(),
                n.read_words()
            ));
        } else {
            let lines: Vec<String> = f
                .others
                .lines
                .iter()
                .map(|l| format!("line {} ({} section(s))", l.model, l.sections.len()))
                .collect();
            out.push(format!(
                "Neighbours from {} included: {} within {distance:.0} m of the spans computed, \
                 each solved in this case with its own settings and wires, their field added by \
                 size at every profile point, given point and cut (the network's code, {:.1} s).",
                n.export_words(),
                lines.join(", "),
                f.seconds
            ));
        }
        out.push(n.stale_words());
        out.extend(n.gaps());
        out.extend(
            f.not_all_checked
                .iter()
                .map(|t| format!("NOT ALL CHECKED: {t}")),
        );
        if !f.others.lines.is_empty() {
            out.extend(
                f.others
                    .notes
                    .iter()
                    .map(|t| format!("NOT ON THE SAFE SIDE: {t}")),
            );
            out.push(ohl_scope::fields::ADDED_BY_SIZE.to_owned());
            if !field.volumes.is_empty() {
                out.push(
                    "NOT ALL CHECKED: the volumes' surfaces are this line's field alone — the \
                     other lines' field is added to the profiles, the given points and the cuts"
                        .to_owned(),
                );
            }
            if !field.points.is_empty() {
                out.push(format!(
                    "NOT ON THE SAFE SIDE: the given points take the other lines within \
                     {distance:.0} m of the spans computed — a point far from them may lie \
                     nearer another line, left out"
                ));
            }
        }
        out.push(format!(
            "NOT ON THE SAFE SIDE: the other lines' wires farther than {distance:.0} m from the \
             spans computed are left out (the network's field distance): their field is not in \
             the result"
        ));
        out
    }
}

/// Adds the field of `others` (wires with their currents) by size to
/// every profile point, given point and cut of `field` — as the network's
/// field check adds the other lines' at its profiles
/// ([`ohl_scope::fields::add_by_size`], the same sums) — the profiles'
/// highest value and limit extent, the points' margins again.
pub fn add_others(
    field: &mut ohl_electrical::magnetic::MagneticField,
    others: &[&ohl_electrical::magnetic::Wires3d],
) {
    if others.is_empty() {
        return;
    }
    let block = |at: &[[f64; 3]]| -> Vec<(f64, f64)> {
        let mut sum = vec![(0.0, 0.0); at.len()];
        for w in others {
            for (s, (b, m)) in sum.iter_mut().zip(w.field_block(at)) {
                s.0 += b * 1e6;
                s.1 += m * 1e6;
            }
        }
        sum
    };
    let limit = field.limit_ut;
    let add = |points: &mut [ohl_electrical::magnetic::FieldPoint]| {
        let at: Vec<[f64; 3]> = points.iter().map(|q| [q.x, q.y, q.z]).collect();
        for (q, (b, m)) in points.iter_mut().zip(block(&at)) {
            q.b_ut += b;
            q.major_ut += m;
            q.margin_ut = limit.map(|l| l - q.b_ut);
        }
    };
    for p in &mut field.profiles {
        add(&mut p.points);
        let (offset, max) =
            p.points
                .iter()
                .map(|q| (q.offset, q.b_ut))
                .fold(
                    (0.0, f64::NEG_INFINITY),
                    |a, b| if b.1 > a.1 { b } else { a },
                );
        p.max_ut = max;
        p.max_offset = offset;
        if let Some(l) = limit {
            let above: Vec<f64> = p
                .points
                .iter()
                .filter(|q| q.b_ut >= l)
                .map(|q| q.offset)
                .collect();
            p.limit = match (above.first(), above.last()) {
                (Some(&left), Some(&right)) => Some(ohl_electrical::magnetic::LimitExtent {
                    left,
                    right,
                    beyond: p.points.first().is_some_and(|q| q.b_ut >= l)
                        || p.points.last().is_some_and(|q| q.b_ut >= l),
                }),
                _ => None,
            };
        }
    }
    add(&mut field.points);
    for c in &mut field.cuts {
        let v = &c.values;
        let plan: Vec<[f64; 2]> = v.offsets.iter().map(|o| v.cut.at(*o)).collect();
        let at: Vec<[f64; 3]> = v
            .elevations
            .iter()
            .flat_map(|z| plan.iter().map(move |p| [p[0], p[1], *z]))
            .collect();
        for (x, (b, _)) in c.values.values.iter_mut().zip(block(&at)) {
            *x += b;
        }
    }
}

#[cfg(test)]
thread_local! {
    /// Tests: an offline working set's file read as before 2026-11-23 (its
    /// neighbours not read) — the before pictures.
    pub(crate) static IGNORED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

impl crate::model::Model {
    /// The neighbours of the open project when it is an offline working
    /// set (none: it is not one), read once per file.
    pub fn offline_neighbours(&self) -> Option<Arc<Result<Neighbours, String>>> {
        #[cfg(test)]
        if IGNORED.with(std::cell::Cell::get) {
            return None;
        }
        if self.remote.is_some() {
            return None;
        }
        let session = self.session()?;
        // The record only (small) decides whether it is one at all.
        if !matches!(session.offline_meta(), Ok(Some(_))) {
            return None;
        }
        let key = self.sources.file.clone();
        let v = self.kept.offline_neighbours.get(key, || {
            Arc::new(match Neighbours::read(session) {
                Ok(Some(n)) => Ok(n),
                Ok(None) => Err("not an offline working set".into()),
                Err(e) => Err(e),
            })
        });
        Some(v)
    }
}

/// The window's block for an offline working set: what is included or
/// not, which lines, that they may be stale (in the warning colour), the
/// export's gaps; with a (?) and its User guide ↗. True when the guide
/// was asked.
pub fn banner(ui: &mut eframe::egui::Ui, n: &Result<Neighbours, String>, lines: &[String]) -> bool {
    use eframe::egui;
    let mut guide = false;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Offline working set: neighbours from its export").strong());
        crate::check_help::question_both(
            ui,
            "The other lines near your working set, as the network exported them with the file.",
            "An offline working set's file holds the other lines near its working set as the \
             network's server exported them (within the export's reach). The rules to other lines \
             take their wires — each solved in every position checked with its own line's \
             settings and wires. Structure loads computes the towers shared with another line \
             with that line's sections (its Shared towers view), the Magnetic field adds the \
             other lines' field. The electric field and plugins do not add them: they say \
             which lines lie near, and NOT ALL CHECKED for them. The neighbours are \
             those of the export's date: what changed on the network since is not in the file.",
        );
        guide = ui
            .small_button("user guide ↗")
            .on_hover_text("Help › User guide: checks with the export's neighbours")
            .clicked();
    });
    let warn = ui.visuals().warn_fg_color;
    match n {
        Err(e) => {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("NOT ALL CHECKED: the export's neighbours could not be read: {e}"),
            );
        }
        Ok(_) => {
            for l in lines {
                if l.starts_with("NOT ALL CHECKED") || l.contains("STALE") {
                    ui.colored_label(ui.visuals().error_fg_color, egui::RichText::new(l).small());
                } else if l.starts_with("They may be stale") {
                    ui.colored_label(warn, egui::RichText::new(l).small());
                } else {
                    ui.label(egui::RichText::new(l).small());
                }
            }
        }
    }
    guide
}

/// Opens the manual's section on the neighbours in the check windows,
/// said in the status line.
pub fn open_guide() -> (String, bool) {
    match crate::help::open_section(GUIDE.0, GUIDE.1) {
        Ok(url) => (format!("opened {url}"), false),
        Err(e) => (e, true),
    }
}
