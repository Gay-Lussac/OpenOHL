// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Checks on an offline working set (ADR-0013, phase 12): the scoped
//! checks of phase 10 run on the file alone, with the neighbours the
//! export wrote into it — **the same code** as the network's server
//! (the crate's `scoped_report` module), so the same check on the same neighbours
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
    /// Whether the shared towers' usage is checked here (the check on the
    /// file), else left to the caller (OHL-Design's window checks with its
    /// own libraries).
    usage: bool,
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

    fn structure_checks(
        &mut self,
        model: &str,
    ) -> Option<Result<Arc<ohl_scope::loads::StructureChecks>, String>> {
        if !self.usage {
            return None;
        }
        if model != self.own {
            return Some(Err(format!(
                "line {model}'s structure libraries are not in the offline working set"
            )));
        }
        let mut kinds = BTreeMap::new();
        for (kind, _) in crate::packager::CHECK_LIBRARIES {
            match self.session.library_files(kind) {
                Ok(f) => {
                    kinds.insert(kind.to_owned(), f);
                }
                Err(e) => return Some(Err(e.to_string())),
            }
        }
        // On the user's own computer: its plugins as OHL-Design runs them.
        let work = std::env::temp_dir().join(format!("ohl-offline-types-{}", std::process::id()));
        Some(crate::packager::structure_checks(&kinds, &work, true).map(Arc::new))
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

/// Another line's neighbours of an offline working set with the wire
/// library its checks read — the same library the network's checks take
/// for it ([`check_offline`]: its own wire files as the export wrote them,
/// else the shipped library), for OHL-Design's check windows.
#[derive(Debug, Clone)]
pub struct NeighbourWires {
    /// The model (line).
    pub model: String,
    /// Its part, as the export wrote it (structure types resolved).
    pub line: Line,
    /// Its wire library.
    pub library: Arc<ohl_cables::Library>,
    /// What could not be read of its library, in words.
    pub notes: Vec<String>,
}

/// The other lines' neighbours offline working set `session` holds, each
/// with its wire library (none: not an offline working set, or no other
/// line near).
pub fn neighbour_wires(session: &Session) -> Result<Vec<NeighbourWires>, String> {
    let neighbours = session.offline_neighbours().map_err(|e| e.to_string())?;
    let caches = Caches::new(64, 16);
    // Wire libraries only (no structure types): nothing is written there.
    let work = std::env::temp_dir().join(format!(
        "ohl-offline-wires-{}-{}",
        std::process::id(),
        crate::random_code(8)
    ));
    let out = neighbours
        .into_iter()
        .map(|n| {
            let files: Vec<(String, String)> = n
                .libraries
                .get("cables")
                .map(|m| m.iter().map(|(a, b)| (a.clone(), b.clone())).collect())
                .unwrap_or_default();
            let mut kinds = BTreeMap::new();
            kinds.insert("cables".to_owned(), files);
            let libs = crate::packager::model_libs(
                &n.model,
                false,
                0,
                Arc::new(LibFiles { files: kinds }),
                &caches,
                &work,
            );
            NeighbourWires {
                model: n.model,
                line: n.line,
                library: Arc::new(libs.cables.clone()),
                notes: libs.notes.clone(),
            }
        })
        .collect();
    let _ = std::fs::remove_dir_all(&work);
    Ok(out)
}

/// What an offline working set cannot give a check, found while it ran.
struct FileGaps {
    /// Something outside what the export holds was asked for.
    beyond: bool,
    /// The export's reach, m.
    reach: f64,
    /// What its user may not see, left out of the export.
    left_out: ohl_session::offline::LeftOut,
}

impl FileGaps {
    /// Each a NOT ALL CHECKED reason.
    fn reasons(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.beyond {
            out.push(format!(
                "the check reaches beyond what the offline working set holds (its neighbours \
                 within {:.0} m of the working set): ask the network's server, or export it \
                 again with a larger reach",
                self.reach
            ));
        }
        if !self.left_out.is_empty() {
            out.push(format!(
                "neighbours you may not see were left out of the export ({} of other lines, {} \
                 of this line): they are checked on the network's server only",
                self.left_out.other_lines, self.left_out.this_line
            ));
        }
        out
    }
}

/// Runs `f` on the offline working set in project file `path` as the
/// network's server would run a scoped check on it: the file's own line
/// and the neighbours its export holds ([`FileNet`]), its working set as
/// exported (the part). Gives `f`'s answer, the session, the working set
/// (as `f` left it) and what the file could not give.
fn on_file<T>(
    path: &Path,
    usage: bool,
    f: impl FnOnce(&mut FileNet<'_>, &mut WorkingSet) -> Result<T, String>,
) -> Result<(T, Session, WorkingSet, FileGaps), String> {
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
    let model = meta.model.clone();
    let left_out = meta.left_out.clone();
    let reach = meta.reach_m;
    let (answer, beyond) = {
        let mut net = FileNet {
            own: model,
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
            usage,
        };
        let answer = f(&mut net, &mut ws);
        let _ = std::fs::remove_dir_all(&work);
        (answer?, net.beyond)
    };
    Ok((
        answer,
        session,
        ws,
        FileGaps {
            beyond,
            reach,
            left_out,
        },
    ))
}

/// Runs scoped check `check` on the offline working set in project file
/// `path` with the neighbours its export holds: the report as the
/// network's server gives it to a user who sees everything the file holds,
/// NOT ALL CHECKED where the file cannot give the server's result.
pub fn check_offline(path: &Path, check: &ScopedCheck) -> Result<ScopedReport, String> {
    let started = Instant::now();
    let name = check.name().to_owned();
    let (raw, mut session, ws, gaps) = on_file(path, true, |net, ws| {
        crate::scoped_report::run(net, ws, check, |n, m, s| n.has_twin(m, s))
    })?;
    let mut report = crate::scoped_report::report(
        &mut FileViewer {
            session: &mut session,
            own: ws.model.clone(),
        },
        &ws,
        &name,
        &raw,
        started.elapsed().as_secs_f64(),
    );
    report.not_all_checked.extend(gaps.reasons());
    if !report.not_all_checked.is_empty() {
        report.status = "NOT ALL CHECKED".into();
    }
    Ok(report)
}

/// The loads on an offline working set's shared towers, as the network's
/// check computes them ([`shared_tower_loads`]).
#[derive(Debug, Clone)]
pub struct SharedTowerLoads {
    /// Per tower: every line's cases on it with every line's sections,
    /// the worst kept — the network's own result.
    pub loads: ohl_scope::loads::LoadsResult,
    /// What the file could not give (each makes it NOT ALL CHECKED): the
    /// result's own reasons and the export's.
    pub not_all_checked: Vec<String>,
    /// Seconds.
    pub seconds: f64,
}

/// The loads on structures `structures` of the offline working set in
/// project file `path` (each of its working set; none: every one shared
/// with another line of the export), each with every line's sections on
/// it under each line's cases, the worst kept — **the network's check
/// itself** ([`check_offline`]'s *structure loads*, before its report),
/// for OHL-Design's Structure loads window (2026-11-24).
pub fn shared_tower_loads(path: &Path, structures: &[String]) -> Result<SharedTowerLoads, String> {
    let started = Instant::now();
    let check = ScopedCheck::StructureLoads {
        structures: structures.to_vec(),
    };
    let (raw, _, _, gaps) = on_file(path, false, |net, ws| {
        crate::scoped_report::run(net, ws, &check, |n, m, s| n.has_twin(m, s))
    })?;
    let crate::scoped_report::Raw::Loads(loads) = raw else {
        return Err("not a structure loads result".into());
    };
    let mut not_all_checked = loads.not_all_checked.clone();
    not_all_checked.extend(gaps.reasons());
    not_all_checked.dedup();
    Ok(SharedTowerLoads {
        loads: *loads,
        not_all_checked,
        seconds: started.elapsed().as_secs_f64(),
    })
}

/// The other lines near an offline working set for its magnetic field,
/// as the network's field check selects and computes them
/// ([`field_neighbours`]).
#[derive(Debug, Clone)]
pub struct FieldNeighbours {
    /// The lines, each its wires as the field's sources.
    pub others: ohl_scope::fields::OtherLines,
    /// What the file could not give (each makes it NOT ALL CHECKED): the
    /// other lines' own reasons and the export's.
    pub not_all_checked: Vec<String>,
    /// Seconds.
    pub seconds: f64,
}

/// The other lines whose wires can come within `spec.distance` of the
/// wires of `sections` (none: the working set's) of the offline working
/// set in project file `path`, in weather case `case`, each computed on
/// its own as the field's sources — **the network's field check's own
/// selection and sources** ([`ohl_scope::fields::other_lines`] on the
/// file), for OHL-Design's Magnetic field window (2026-11-24).
pub fn field_neighbours(
    path: &Path,
    sections: &[String],
    case: &ohl_weather::WeatherCase,
    spec: &ohl_scope::fields::FieldSpec,
) -> Result<FieldNeighbours, String> {
    let started = Instant::now();
    let (others, _, _, gaps) = on_file(path, false, |net, ws| {
        if !sections.is_empty() {
            ws.sections = sections.to_vec();
        }
        ohl_scope::fields::other_lines(net, ws, case, spec)
    })?;
    let mut not_all_checked = others.wires_failed.clone();
    not_all_checked.extend(others.not_all_checked.iter().cloned());
    not_all_checked.extend(gaps.reasons());
    not_all_checked.dedup();
    Ok(FieldNeighbours {
        others,
        not_all_checked,
        seconds: started.elapsed().as_secs_f64(),
    })
}

/// An offline working set's lines near solved together for its electric
/// field at given profiles ([`electric_neighbours`]).
#[derive(Debug, Clone)]
pub struct ElectricNeighbours {
    /// The other lines in the system, each with its sections.
    pub lines: Vec<(String, Vec<String>)>,
    /// What the system took in, in words.
    pub taken_in: Vec<String>,
    /// How the unknown phase angles were taken (exact or a bound).
    pub method: String,
    /// Per profile asked, the lines together (none computed when no other
    /// line is near).
    pub profiles: Vec<Result<ohl_scope::electric::JointProfile, String>>,
    /// What the file could not give (each makes it NOT ALL CHECKED): the
    /// system's own reasons and the export's.
    pub not_all_checked: Vec<String>,
    /// What was assumed (voltages, left out).
    pub notes: Vec<String>,
    /// Seconds.
    pub seconds: f64,
    /// The lines' wires as one system, for systems at other places (a
    /// volume's stretches, 2026-11-25; none: no other line near).
    pub system: Option<Arc<ohl_scope::electric::JointLines>>,
}

/// The lines whose wires can come within `distance` of the wires of
/// `sections` (none: the working set's) of the offline working set in
/// project file `path`, solved together with this line in weather case
/// `case` at `profiles` (each its system: the wires within `distance` of
/// its middle), the worst over their phase angles at every point — **the
/// network's electric field check's own selection and system**
/// ([`ohl_scope::electric::joint_lines`] and
/// [`ohl_scope::electric::joint_at`] on the file), for OHL-Design's
/// Electric field window (2026-11-24); `given`: the window's voltages of
/// this line's circuits.
pub fn electric_neighbours(
    path: &Path,
    sections: &[String],
    case: &ohl_weather::WeatherCase,
    distance: f64,
    given: &[ohl_scope::electric::CircuitVoltage],
    profiles: &[ohl_scope::electric::ProfileAt],
) -> Result<ElectricNeighbours, String> {
    let started = Instant::now();
    let ((lines, model), _, _, gaps) = on_file(path, false, |net, ws| {
        if !sections.is_empty() {
            ws.sections = sections.to_vec();
        }
        Ok((
            ohl_scope::electric::joint_lines(net, ws, case, distance, given)?,
            ws.model.clone(),
        ))
    })?;
    let computed = if lines.lines.is_empty() {
        Vec::new()
    } else {
        ohl_scope::electric::joint_at(&lines, profiles, distance)
    };
    let mut not_all_checked = lines.not_all_checked.clone();
    not_all_checked.extend(gaps.reasons());
    not_all_checked.dedup();
    Ok(ElectricNeighbours {
        taken_in: lines.taken_in(&model),
        method: lines.method(),
        lines: lines.lines.clone(),
        profiles: computed,
        not_all_checked,
        notes: lines.notes.clone(),
        seconds: started.elapsed().as_secs_f64(),
        system: (!lines.lines.is_empty()).then(|| Arc::new(lines)),
    })
}
