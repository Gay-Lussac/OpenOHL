// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks on an offline working set (ADR-0013, phase 12): the scoped
//! checks of phase 10 run on the file alone, with the neighbours the
//! export wrote into it — **the same code** as the network's server
//! ([`crate::scoped_report`]), so the same check on the same neighbours
//! gives the same report.
//!
//! **Safe side**: the file holds the neighbours within the export's area
//! (the working set's bounds grown by the reach and three times the
//! network's largest span margin: every area a check on it asks for). A
//! check asking for anything outside — the working
//! set moved or grown offline, a reach larger than the export's — says
//! **NOT ALL CHECKED**, as does an export that left out neighbours its
//! user may not see (they are computed on the server only), and a
//! neighbour line whose library file was left out.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use ohl_core::grid::Bounds;
use ohl_line::{Line, Point};
use ohl_scope::{Cases, Neighbour, Neighbourhood, SpanChord, WorkingSet};
use ohl_session::offline::OfflineMeta;
use ohl_session::{Kind, Session};

use crate::packager::{Caches, LibFiles};
use crate::scoped_report::Viewer;
use crate::{ScopedCheck, ScopedReport};

/// An offline working set as a scoped check reads it.
struct FileNet<'a> {
    own: String,
    session: &'a mut Session,
    others: BTreeMap<String, Other>,
    libs: BTreeMap<String, (Arc<ohl_cables::Library>, Arc<Cases>)>,
    meta: OfflineMeta,
    /// Something outside what the export holds was asked for.
    beyond: bool,
    caches: Caches,
    work: std::path::PathBuf,
}

/// Another line's neighbours, as exported.
struct Other {
    line: Line,
    libraries: BTreeMap<String, BTreeMap<String, String>>,
}

/// The cases of a line from its weather and load case files (none: the
/// shipped defaults), as the network's server reads them.
fn cases_of(
    model: &str,
    weather: &[(String, String)],
    loads: &[(String, String)],
) -> Result<Cases, String> {
    let mut cases = ohl_scope::default_cases()?;
    if !weather.is_empty() {
        cases.weather.clear();
        for (name, text) in weather {
            let w = ohl_weather::WeatherCases::parse(text)
                .map_err(|e| format!("weather file {name}: {e}"))?;
            cases.weather.extend(w.cases);
        }
        cases.source = format!("line {model}'s own library");
    }
    if !loads.is_empty() {
        cases.loads.clear();
        for (name, text) in loads {
            let l = ohl_loads::LoadCases::parse(text)
                .map_err(|e| format!("load case file {name}: {e}"))?;
            cases.loads.extend(l.cases);
        }
        cases.source = format!("line {model}'s own library");
    }
    Ok(cases)
}

impl FileNet<'_> {
    /// Whether `area` lies within what the export holds.
    fn covered(&self, area: &Bounds) -> bool {
        let a = self.meta.area;
        area.min[0] >= a[0] && area.min[1] >= a[1] && area.max[0] <= a[2] && area.max[1] <= a[3]
    }

    fn line(&self, model: &str) -> Option<&Line> {
        if model == self.own {
            Some(self.session.line())
        } else {
            self.others.get(model).map(|o| &o.line)
        }
    }

    fn lines(&self) -> Vec<(&str, &Line)> {
        let mut v = vec![(self.own.as_str(), self.session.line())];
        v.extend(self.others.iter().map(|(m, o)| (m.as_str(), &o.line)));
        v
    }

    /// Whether structure `id` of `model` has a twin in another line.
    fn has_twin(&mut self, model: &str, id: &str) -> bool {
        let Some((p, kind)) = self.structure(model, id) else {
            return false;
        };
        let tol = ohl_scope::reach::TWIN_TOLERANCE;
        let area = Bounds::new([p[0], p[1]], [p[0], p[1]]).grown(tol);
        let near = self.structures_in(&area);
        near.into_iter().any(|(m, t)| {
            m != model
                && self
                    .structure(&m, &t)
                    .is_some_and(|(q, k)| (q[0] - p[0]).hypot(q[1] - p[1]) <= tol && k == kind)
        })
    }

    fn libs(&mut self, model: &str) -> Result<(Arc<ohl_cables::Library>, Arc<Cases>), String> {
        if let Some(l) = self.libs.get(model) {
            return Ok(l.clone());
        }
        let (files, weather, loads): (LibFiles, Vec<_>, Vec<_>) = if model == self.own {
            let mut files = BTreeMap::new();
            for kind in crate::web::SCENE_LIBRARIES {
                files.insert(
                    kind.to_owned(),
                    self.session
                        .library_files(kind)
                        .map_err(|e| e.to_string())?,
                );
            }
            let weather = self
                .session
                .library_files("weather")
                .map_err(|e| e.to_string())?;
            let loads = self
                .session
                .library_files("loadcases")
                .map_err(|e| e.to_string())?;
            (LibFiles { files }, weather, loads)
        } else {
            let o = self
                .others
                .get(model)
                .ok_or_else(|| format!("line {model} is not in the offline working set"))?;
            let list = |kind: &str| -> Vec<(String, String)> {
                o.libraries
                    .get(kind)
                    .map(|m| m.iter().map(|(n, t)| (n.clone(), t.clone())).collect())
                    .unwrap_or_default()
            };
            let mut files = BTreeMap::new();
            files.insert("cables".to_owned(), list("cables"));
            (LibFiles { files }, list("weather"), list("loadcases"))
        };
        let libs =
            crate::packager::model_libs(model, false, 0, Arc::new(files), &self.caches, &self.work);
        let out = (
            Arc::new(libs.cables.clone()),
            Arc::new(cases_of(model, &weather, &loads)?),
        );
        self.libs.insert(model.to_owned(), out.clone());
        Ok(out)
    }
}

impl Neighbourhood for FileNet<'_> {
    fn sections_in(&mut self, area: &Bounds) -> Vec<(String, String)> {
        if !self.covered(area) {
            self.beyond = true;
        }
        let mut out = Vec::new();
        for (code, line) in self.lines() {
            for sec in &line.sections {
                let b = Bounds::of_points(
                    sec.points
                        .iter()
                        .filter_map(|p| line.structure(&p.structure).map(|s| &s.position)),
                );
                if b.is_some_and(|b| b.intersects(area)) {
                    out.push((code.to_owned(), sec.id.clone()));
                }
            }
        }
        out
    }

    fn structures_in(&mut self, area: &Bounds) -> Vec<(String, String)> {
        if !self.covered(area) {
            self.beyond = true;
        }
        let mut out = Vec::new();
        for (code, line) in self.lines() {
            for st in &line.structures {
                let p = [st.position[0], st.position[1]];
                if area.intersects(&Bounds::new(p, p)) {
                    out.push((code.to_owned(), st.id.clone()));
                }
            }
        }
        out
    }

    fn structure(&mut self, model: &str, id: &str) -> Option<(Point, Option<String>)> {
        self.line(model)?
            .structure(id)
            .map(|s| (s.position, s.structure_type.clone()))
    }

    fn sections_at(&mut self, model: &str, id: &str) -> Vec<String> {
        self.line(model)
            .map(|l| {
                l.sections
                    .iter()
                    .filter(|s| s.points.iter().any(|p| p.structure == id))
                    .map(|s| s.id.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn chords(&mut self, model: &str, sections: &[String]) -> Vec<(String, Vec<SpanChord>)> {
        let Some(line) = self.line(model) else {
            return Vec::new();
        };
        let wanted: BTreeSet<&String> = sections.iter().collect();
        line.sections
            .iter()
            .filter(|s| wanted.contains(&s.id))
            .map(|s| {
                (
                    s.id.clone(),
                    ohl_scope::bound::chords_of(s, |id| line.structure(id)),
                )
            })
            .collect()
    }

    fn part(
        &mut self,
        model: &str,
        structures: &[String],
        sections: &[String],
    ) -> Result<Line, String> {
        if model == self.own {
            let part = self.session.drawing_part(
                structures,
                sections,
                &[],
                ohl_session::interest::SOLVE_NEAR,
            );
            if !self.session.any_typed() {
                return Ok(part);
            }
            let mut files = BTreeMap::new();
            for kind in crate::web::SCENE_LIBRARIES {
                files.insert(
                    kind.to_owned(),
                    self.session
                        .library_files(kind)
                        .map_err(|e| e.to_string())?,
                );
            }
            let libs = crate::packager::model_libs(
                model,
                true,
                0,
                Arc::new(LibFiles { files }),
                &self.caches,
                &self.work,
            );
            return Ok(crate::packager::resolve_part(part, &libs).0);
        }
        let o = self
            .others
            .get(model)
            .ok_or_else(|| format!("line {model} is not in the offline working set"))?;
        // Taken again from the part the server wrote (types resolved
        // there): the same sub-line for the sections it holds whole.
        Ok(ohl_session::interest::line_part(
            &o.line,
            structures,
            sections,
            &[],
            ohl_session::interest::SOLVE_NEAR,
        ))
    }

    fn potentials(&mut self, model: &str) -> Vec<Option<ohl_line::electrical::Potential>> {
        self.line(model)
            .map(ohl_scope::system_potentials)
            .unwrap_or_default()
    }

    fn library(&mut self, model: &str) -> Result<Arc<ohl_cables::Library>, String> {
        Ok(self.libs(model)?.0)
    }

    fn cases(&mut self, model: &str) -> Result<Arc<Cases>, String> {
        Ok(self.libs(model)?.1)
    }

    fn max_margin(&mut self) -> f64 {
        let own = ohl_scope::bound::max_margin(self.session.line());
        self.meta.max_margin.max(own)
    }
}

/// Everything in the file may be named: what its user could not see was
/// never written into it.
struct FileViewer<'a> {
    session: &'a mut Session,
    own: String,
}

impl Viewer for FileViewer<'_> {
    fn see(&mut self, _: &Neighbour) -> bool {
        true
    }

    fn routes(&mut self, n: &Neighbour) -> Option<BTreeSet<String>> {
        if n.model != self.own {
            return None;
        }
        let kind = match n.kind {
            ohl_scope::NeighbourKind::Section => Kind::Section,
            ohl_scope::NeighbourKind::Jumper => Kind::Jumper,
            ohl_scope::NeighbourKind::Structure => Kind::Structure,
        };
        self.session.object_routes(&[(kind, n.id.clone())]).pop()
    }

    fn names_hidden(&self, _: &str) -> bool {
        false
    }
}

/// Runs scoped check `check` on the offline working set in project file
/// `path` with the neighbours its export holds: the report as the
/// network's server gives it to a user who sees everything the file holds,
/// NOT ALL CHECKED where the file cannot give the server's result.
pub fn check_offline(path: &Path, check: &ScopedCheck) -> Result<ScopedReport, String> {
    let started = Instant::now();
    let mut session = Session::open(path).map_err(|e| e.to_string())?;
    let meta = session
        .offline_meta()
        .map_err(|e| e.to_string())?
        .ok_or("not an offline working set (ohl network export-working-set)")?;
    let neighbours = session.offline_neighbours().map_err(|e| e.to_string())?;
    let wanted: BTreeSet<(Kind, String)> = session.part().cloned().unwrap_or_default();
    let of = |k: Kind| -> Vec<String> {
        wanted
            .iter()
            .filter(|(x, _)| *x == k)
            .map(|(_, id)| id.clone())
            .collect()
    };
    let mut ws = WorkingSet {
        model: meta.model.clone(),
        sections: of(Kind::Section),
        jumpers: of(Kind::Jumper),
        structures: of(Kind::Structure),
    };
    let work = std::env::temp_dir().join(format!(
        "ohl-offline-check-{}-{}",
        std::process::id(),
        crate::random_code(8)
    ));
    let name = check.name().to_owned();
    let model = meta.model.clone();
    let left_out = meta.left_out.clone();
    let reach = meta.reach_m;
    let (raw, beyond) = {
        let mut net = FileNet {
            own: model.clone(),
            session: &mut session,
            others: neighbours
                .into_iter()
                .map(|n| {
                    (
                        n.model,
                        Other {
                            line: n.line,
                            libraries: n.libraries,
                        },
                    )
                })
                .collect(),
            libs: BTreeMap::new(),
            meta,
            beyond: false,
            caches: Caches::new(64, 16),
            work: work.clone(),
        };
        let raw = crate::scoped_report::run(&mut net, &mut ws, check, |n, m, s| n.has_twin(m, s));
        let _ = std::fs::remove_dir_all(&work);
        (raw?, net.beyond)
    };
    let mut report = crate::scoped_report::report(
        &mut FileViewer {
            session: &mut session,
            own: model,
        },
        &ws,
        &name,
        &raw,
        started.elapsed().as_secs_f64(),
    );
    if beyond {
        report.not_all_checked.push(format!(
            "the check reaches beyond what the offline working set holds (its neighbours within \
             {reach:.0} m of the working set): ask the network's server, or export it again \
             with a larger reach"
        ));
    }
    if !left_out.is_empty() {
        report.not_all_checked.push(format!(
            "neighbours you may not see were left out of the export ({} of other lines, {} of \
             this line): they are checked on the network's server only",
            left_out.other_lines, left_out.this_line
        ));
    }
    if !report.not_all_checked.is_empty() {
        report.status = "NOT ALL CHECKED".into();
    }
    Ok(report)
}
